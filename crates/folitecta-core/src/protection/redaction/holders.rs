//! Taking the content under the marks out of a page and of the drawings it
//! reuses. Each content stream — the page's, or a form's for one drawing of
//! it — is a *holder*: its glyphs under the marks leave their text
//! operations, its paths lose their geometry there, its pictures and inline
//! pictures get their pixels covered, and the forms it draws under the marks
//! are holders in turn. A form or picture drawn elsewhere too is copied for
//! this drawing (the original stays for the others); one drawn only here is
//! changed in place. What the probe saw under a mark and no holder explains
//! sends the page to the picture method.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use hayro::kurbo::{Affine, Rect};
use lopdf::{Dictionary, Object, ObjectId, Stream, dictionary};

use super::RedactionLook;
use super::inline::inline_picture;
use super::paths::{PathEdit, empty_clip, exclusion_clip, redact_path, shading_shown_elsewhere};
use super::scan::{Gs, Paint, PathObj, Props, page_state, path_bounds, resource, scan};
use crate::content_edit::{Splice, check_splices, joined, name_token, rewrite};
use crate::content_lex::{name_of, operations};
use crate::document::{Document, private_resources};
use crate::page_text::probe::{Marker, PageProbe, Probe, StreamKey, is_form, stream_data, stream_resources};
use crate::pictures::{decode_image, picture_colors, pixel_area};
use crate::textedit::{RemovedGlyph, glyph_removal_splices, shows_glyphs, text_ops_from};
use crate::util::deref;

/// Why a page was drawn as a picture instead of keeping its text and drawings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PictureReason {
    /// The picture method was chosen.
    Chosen,
    /// A font under a mark cannot be edited.
    Font,
    /// A colour gradient that can hold a picture is under a mark.
    Shading,
    /// A pattern that draws text or pictures is under a mark.
    Pattern,
    /// A transparency mask that draws content is under a mark.
    SoftMask,
    /// Reused drawings are nested too deeply or draw themselves.
    Nesting,
    /// The page content could not be read or changed.
    Content,
    /// A picture under a mark could not be edited.
    Picture,
    /// The check found content under a mark after the edit.
    Check,
}

/// A redaction mark on a page.
#[derive(Debug, Clone)]
pub(super) struct Mark {
    /// Default space of the page.
    pub pdf: Rect,
    /// Display space (as the page is shown, y down), like the probe.
    pub shown: [f32; 4],
    pub look: RedactionLook,
}

/// A content stream holding content of marked marked-content sequences.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(super) enum TouchKey {
    /// The content of a page (its object id).
    Page(ObjectId),
    /// A form.
    Stream(ObjectId),
}

/// What the edit of the pages did.
#[derive(Debug, Default)]
pub(super) struct Work {
    pub glyphs: usize,
    pub pictures: usize,
    pub paths: usize,
    pub forms: usize,
    /// Marked-content sequences that held removed content.
    pub touched: BTreeSet<(TouchKey, i64)>,
    /// Pictures and forms replaced by copies on the redacted pages: they go
    /// when nothing draws them any more.
    pub replaced: BTreeSet<ObjectId>,
    /// Forms copied for one page: original, copy, page.
    pub copies: Vec<(ObjectId, ObjectId, ObjectId)>,
}

impl Work {
    fn merge(&mut self, o: Work) {
        self.glyphs += o.glyphs;
        self.pictures += o.pictures;
        self.paths += o.paths;
        self.forms += o.forms;
        self.touched.extend(o.touched);
        self.replaced.extend(o.replaced);
        self.copies.extend(o.copies);
    }
}

fn meets(a: &[f32; 4], b: &[f32; 4]) -> bool {
    a[0] < b[2] && a[2] > b[0] && a[1] < b[3] && a[3] > b[1]
}

fn overlaps(a: Rect, b: Rect) -> bool {
    a.x0 < b.x1 && a.x1 > b.x0 && a.y0 < b.y1 && a.y1 > b.y0
}

struct Ctx<'a> {
    marks: &'a [Mark],
    probe: &'a Probe,
    page: &'a PageProbe,
    page_id: ObjectId,
    markers: &'a HashMap<(StreamKey, usize), usize>,
    counts: &'a Counts,
    used_glyphs: HashSet<usize>,
    used_images: HashSet<usize>,
    next_name: usize,
}

impl Ctx<'_> {
    fn under(&self, shown: &[f32; 4]) -> bool {
        self.marks.iter().any(|m| meets(shown, &m.shown))
    }

    fn rects(&self) -> Vec<Rect> {
        self.marks.iter().map(|m| m.pdf).collect()
    }

    /// A drawing is the only one of its object in the document.
    fn alone(&self, id: ObjectId) -> bool {
        self.counts.get(&id) == Some(&1)
    }

    /// The probe saw something under a mark inside the drawing `chain`.
    fn drawn_under(&self, chain: &[usize]) -> bool {
        self.page
            .glyphs
            .iter()
            .any(|g| !g.covered && g.chain.starts_with(chain) && self.under(&g.body))
            || self
                .page
                .images
                .iter()
                .any(|i| !i.covered && i.chain.starts_with(chain) && self.under(&i.bounds))
    }

    /// A resource name of `res` that is not used yet.
    fn fresh(&mut self, res: &Dictionary, prefix: &str) -> Vec<u8> {
        loop {
            self.next_name += 1;
            let name = format!("{prefix}{}", self.next_name).into_bytes();
            let taken = res
                .get(b"XObject")
                .ok()
                .and_then(|x| x.as_dict().ok())
                .is_some_and(|x| x.has(&name));
            if !taken {
                return name;
            }
        }
    }
}

/// A content stream to redact.
struct Holder {
    key: StreamKey,
    data: Vec<u8>,
    /// Its resources (a private copy).
    res: Dictionary,
    gs0: Gs,
    /// The `Do` markers of the drawing, outermost first.
    chain: Vec<usize>,
}

/// What redacting a holder gives.
struct HolderOut {
    splices: Vec<Splice>,
    changed: bool,
    res: Dictionary,
    /// Marked-content ids of the holder that held removed content.
    touched: Vec<i64>,
}

/// The `/XObject` dictionary of private resources.
fn xobjects_mut(res: &mut Dictionary) -> &mut Dictionary {
    if !matches!(res.get(b"XObject"), Ok(Object::Dictionary(_))) {
        res.set("XObject", Object::Dictionary(Dictionary::new()));
    }
    match res.get_mut(b"XObject") {
        Ok(Object::Dictionary(d)) => d,
        _ => unreachable!("set just above"),
    }
}

/// Keys a copy of a form keeps (ISO 32000-1 Table 95, without private,
/// metadata and reference data).
const FORM_KEYS: [&[u8]; 7] = [
    b"Type",
    b"Subtype",
    b"FormType",
    b"BBox",
    b"Matrix",
    b"Group",
    b"StructParents",
];

/// The dictionary of a redacted form: the standard keys of the original and
/// `res` as its resources.
fn form_dict(doc: &lopdf::Document, orig: &Dictionary, res: Dictionary) -> Dictionary {
    let mut d = Dictionary::new();
    for k in FORM_KEYS {
        if let Ok(v) = orig.get(k) {
            d.set(k.to_vec(), deref(doc, v).clone());
        }
    }
    if let Ok(v) = orig.get(b"OC") {
        d.set("OC", v.clone());
    }
    d.set("Resources", Object::Dictionary(res));
    d
}

fn affine_of(doc: &lopdf::Document, o: Option<&Object>) -> Affine {
    let v: Vec<f64> = o
        .and_then(|o| deref(doc, o).as_array().ok())
        .map(|a| {
            a.iter()
                .filter_map(|x| match deref(doc, x) {
                    Object::Integer(i) => Some(*i as f64),
                    Object::Real(r) => Some(*r as f64),
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default();
    if v.len() == 6 {
        Affine::new([v[0], v[1], v[2], v[3], v[4], v[5]])
    } else {
        Affine::IDENTITY
    }
}

fn rect_value(doc: &lopdf::Document, o: Option<&Object>) -> Option<Rect> {
    let r = crate::util::rect_of(doc, deref(doc, o?))?;
    Some(Rect::new(r[0] as f64, r[1] as f64, r[2] as f64, r[3] as f64))
}

/// The marked-content property list of a `BDC` operation without the
/// alternate descriptions and replacement text (None when it has none).
fn strip_alternates(op: &[u8], named: Option<&Dictionary>) -> Option<Vec<u8>> {
    const KEYS: [&[u8]; 3] = [b"ActualText", b"Alt", b"E"];
    let content = lopdf::content::Content::decode(op).ok()?;
    let mut operation = content.operations.into_iter().next()?;
    if operation.operator != "BDC" || operation.operands.len() != 2 {
        return None;
    }
    let mut dict = match (&operation.operands[1], named) {
        (Object::Dictionary(d), _) => d.clone(),
        (Object::Name(_), Some(d)) => d.clone(),
        _ => return None,
    };
    if !KEYS.iter().any(|k| dict.has(k)) {
        return None;
    }
    for k in KEYS {
        dict.remove(k);
    }
    operation.operands[1] = Object::Dictionary(dict);
    lopdf::content::Content {
        operations: vec![operation],
    }
    .encode()
    .ok()
}

/// Names a content stream uses, by resource category; forms, soft masks,
/// patterns and Type 3 fonts without resources of their own add theirs.
pub(super) fn used_names(
    doc: &lopdf::Document,
    data: &[u8],
    res: &Dictionary,
    out: &mut HashMap<&'static [u8], HashSet<Vec<u8>>>,
    seen: &mut HashSet<ObjectId>,
) {
    let mut add = |cat: &'static [u8], n: Vec<u8>| {
        out.entry(cat).or_default().insert(n);
    };
    // A stream that uses the resources of its caller.
    let follow = |id: ObjectId, seen: &mut HashSet<ObjectId>, out_names: &mut Vec<ObjectId>| {
        let inherits = doc
            .get_object(id)
            .and_then(Object::as_stream)
            .is_ok_and(|s| !s.dict.has(b"Resources"));
        if inherits && seen.insert(id) {
            out_names.push(id);
        }
    };
    let mut nested: Vec<ObjectId> = Vec::new();
    for op in operations(data) {
        let first = || op.operands.first().and_then(|r| name_of(data, *r));
        match op.operator {
            b"Tf" => {
                if let Some(n) = first() {
                    // Type 3 glyphs without resources draw with these.
                    if let Some(font) = resource(doc, res, b"Font", &n).and_then(|f| f.as_dict().ok()) {
                        if !font.has(b"Resources") {
                            if let Ok(procs) = font.get(b"CharProcs").map(|c| deref(doc, c)) {
                                if let Ok(procs) = procs.as_dict() {
                                    for (_, p) in procs.iter() {
                                        if let Ok(pid) = p.as_reference() {
                                            if seen.insert(pid) {
                                                nested.push(pid);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    add(b"Font", n);
                }
            }
            b"Do" => {
                if let Some(n) = first() {
                    if let Some(id) = res
                        .get(b"XObject")
                        .ok()
                        .and_then(|x| deref(doc, x).as_dict().ok())
                        .and_then(|x| x.get(&n).ok())
                        .and_then(|v| v.as_reference().ok())
                    {
                        follow(id, seen, &mut nested);
                    }
                    add(b"XObject", n);
                }
            }
            b"gs" => {
                if let Some(n) = first() {
                    if let Some(g) = resource(doc, res, b"ExtGState", &n)
                        .and_then(|d| d.as_dict().ok())
                        .and_then(|d| d.get(b"SMask").ok())
                        .map(|m| deref(doc, m))
                        .and_then(|m| m.as_dict().ok())
                        .and_then(|m| m.get(b"G").ok())
                        .and_then(|g| g.as_reference().ok())
                    {
                        follow(g, seen, &mut nested);
                    }
                    add(b"ExtGState", n);
                }
            }
            b"cs" | b"CS" => {
                if let Some(n) = first() {
                    add(b"ColorSpace", n);
                }
            }
            b"scn" | b"SCN" => {
                if let Some(n) = op.operands.last().and_then(|r| name_of(data, *r)) {
                    if let Some(pid) = res
                        .get(b"Pattern")
                        .ok()
                        .and_then(|p| deref(doc, p).as_dict().ok())
                        .and_then(|p| p.get(&n).ok())
                        .and_then(|v| v.as_reference().ok())
                    {
                        follow(pid, seen, &mut nested);
                    }
                    add(b"Pattern", n);
                }
            }
            b"sh" => {
                if let Some(n) = first() {
                    add(b"Shading", n);
                }
            }
            b"BDC" | b"DP" => {
                if let Some(n) = op.operands.get(1).and_then(|r| name_of(data, *r)) {
                    add(b"Properties", n);
                }
            }
            b"BI" => {
                // A named colour space of an inline picture.
                let raw = &data[op.start..op.end];
                for key in [b"/CS".as_slice(), b"/ColorSpace"] {
                    if let Some(at) = raw.windows(key.len()).position(|w| w == key) {
                        let rest = &raw[at + key.len()..];
                        let rest: Vec<u8> = rest
                            .iter()
                            .skip_while(|c| c.is_ascii_whitespace())
                            .copied()
                            .collect();
                        if rest.first() == Some(&b'/') {
                            let name: Vec<u8> = rest[1..]
                                .iter()
                                .take_while(|c| !c.is_ascii_whitespace() && !b"/[]<>()".contains(c))
                                .copied()
                                .collect();
                            add(b"ColorSpace", name);
                        }
                    }
                }
            }
            _ => {}
        }
    }
    // Fonts that still show glyphs (not just size the moves of removed ones).
    for op in text_ops_from(data, &crate::textedit::TextState::default()).0 {
        if shows_glyphs(data, &op) {
            if op.font.is_empty() {
                out.entry(SHOWN_ANY).or_default().insert(Vec::new());
            } else {
                out.entry(SHOWN).or_default().insert(op.font.clone());
            }
        }
    }
    for id in nested {
        if let Ok(s) = doc.get_object(id).and_then(Object::as_stream) {
            let data = stream_data(s);
            used_names(doc, &data, res, out, seen);
        }
    }
}

/// Pseudo-categories of [`used_names`]: fonts showing glyphs, and glyphs
/// shown with a font set elsewhere.
const SHOWN: &[u8] = b"#shown";
const SHOWN_ANY: &[u8] = b"#shown-any";

/// Drop the names of private resources that `data` does not use.
pub(super) fn drop_unused(doc: &lopdf::Document, data: &[u8], res: &mut Dictionary) {
    let mut used: HashMap<&'static [u8], HashSet<Vec<u8>>> = HashMap::new();
    used_names(doc, data, res, &mut used, &mut HashSet::new());
    // A font that only sizes the moves of removed glyphs: its program and
    // its map of codes (which tell the removed characters) give way to a
    // bare standard font of the same name.
    if !used.contains_key(SHOWN_ANY) {
        let shown = used.get(SHOWN).cloned().unwrap_or_default();
        let set = used.get(b"Font".as_slice()).cloned().unwrap_or_default();
        if let Ok(Object::Dictionary(fonts)) = res.get_mut(b"Font") {
            for name in set.iter().filter(|n| !shown.contains(*n)) {
                if fonts.has(name) {
                    fonts.set(
                        name.clone(),
                        Object::Dictionary(dictionary! {
                            "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica", "Encoding" => "WinAnsiEncoding",
                        }),
                    );
                }
            }
        }
    }
    for cat in [
        b"Font".as_slice(),
        b"XObject",
        b"ExtGState",
        b"ColorSpace",
        b"Pattern",
        b"Shading",
        b"Properties",
    ] {
        let keep = used.remove(cat).unwrap_or_default();
        if let Ok(Object::Dictionary(d)) = res.get_mut(cat) {
            let names: Vec<Vec<u8>> = d.iter().map(|(k, _)| k.clone()).collect();
            for n in names {
                // Default colour spaces are used by device colours.
                let default = cat == b"ColorSpace" && n.starts_with(b"Default");
                if !keep.contains(&n) && !default {
                    d.remove(&n);
                }
            }
        }
    }
}

/// What a pattern of the resources paints.
enum PatternKind {
    /// A tiling pattern; whether its cell draws text or pictures.
    Tiling(bool),
    /// A shading pattern: the shading and the pattern matrix.
    Shading(Dictionary, Affine),
    Unknown,
}

/// A content stream draws text or pictures (forms and patterns followed).
fn draws_text_or_pictures(
    doc: &lopdf::Document,
    data: &[u8],
    res: &Dictionary,
    seen: &mut HashSet<ObjectId>,
) -> bool {
    for op in operations(data) {
        match op.operator {
            b"BT" | b"Tj" | b"TJ" | b"'" | b"\"" | b"BI" => return true,
            b"Do" | b"scn" | b"SCN" => {
                let (cat, n): (&[u8], _) = if op.operator == b"Do" {
                    (b"XObject", op.operands.first().and_then(|r| name_of(data, *r)))
                } else {
                    (b"Pattern", op.operands.last().and_then(|r| name_of(data, *r)))
                };
                let Some(id) = n.and_then(|n| {
                    res.get(cat)
                        .ok()
                        .and_then(|x| deref(doc, x).as_dict().ok())
                        .and_then(|x| x.get(&n).ok())
                        .and_then(|v| v.as_reference().ok())
                }) else {
                    continue;
                };
                let Ok(s) = doc.get_object(id).and_then(Object::as_stream) else {
                    continue;
                };
                if s.dict.get(b"Subtype").ok().and_then(|v| v.as_name().ok()) == Some(b"Image") {
                    return true;
                }
                if seen.insert(id) {
                    let r = stream_resources(doc, s, res);
                    if draws_text_or_pictures(doc, &stream_data(s), &r, seen) {
                        return true;
                    }
                }
            }
            _ => {}
        }
    }
    false
}

fn pattern_kind(doc: &lopdf::Document, res: &Dictionary, name: &[u8]) -> PatternKind {
    let Some(p) = resource(doc, res, b"Pattern", name) else {
        return PatternKind::Unknown;
    };
    let (dict, stream) = match p {
        Object::Dictionary(d) => (d, None),
        Object::Stream(s) => (&s.dict, Some(s)),
        _ => return PatternKind::Unknown,
    };
    match dict.get(b"PatternType").ok().and_then(|t| t.as_i64().ok()) {
        Some(1) => {
            let Some(s) = stream else {
                return PatternKind::Unknown;
            };
            let r = stream_resources(doc, s, res);
            PatternKind::Tiling(draws_text_or_pictures(
                doc,
                &stream_data(s),
                &r,
                &mut HashSet::new(),
            ))
        }
        Some(2) => {
            let sh = dict
                .get(b"Shading")
                .ok()
                .map(|s| deref(doc, s))
                .and_then(|s| match s {
                    Object::Dictionary(d) => Some(d.clone()),
                    Object::Stream(s) => Some(s.dict.clone()),
                    _ => None,
                });
            match sh {
                Some(sh) => PatternKind::Shading(sh, affine_of(doc, dict.get(b"Matrix").ok())),
                None => PatternKind::Unknown,
            }
        }
        _ => PatternKind::Unknown,
    }
}

fn shading_type(doc: &lopdf::Document, d: &Dictionary) -> i64 {
    d.get(b"ShadingType")
        .ok()
        .map(|t| deref(doc, t))
        .and_then(|t| t.as_i64().ok())
        .unwrap_or(0)
}

impl Document {
    /// The soft mask in effect meets a mark (in page space).
    fn mask_meets(&self, gs: &Gs, rects: &[Rect]) -> bool {
        let Some((gid, m)) = gs.smask else {
            return false;
        };
        let Ok(g) = self.inner.get_object(gid).and_then(Object::as_stream) else {
            return true;
        };
        let bbox =
            rect_value(&self.inner, g.dict.get(b"BBox").ok()).unwrap_or(Rect::new(-1e6, -1e6, 1e6, 1e6));
        let matrix = affine_of(&self.inner, g.dict.get(b"Matrix").ok());
        let area = (m * matrix).transform_rect_bbox(bbox);
        rects.iter().any(|r| overlaps(area, *r))
    }

    /// Whether the colours a shading paints over `area` (page space) under
    /// the marks are painted outside them too, so a clip around the marks
    /// hides nothing that cannot be seen elsewhere; `to_shading` maps page
    /// space to the shading's space.
    fn shading_shown(&self, sh: &Dictionary, to_shading: Affine, area: Rect, rects: &[Rect]) -> bool {
        let kind = shading_type(&self.inner, sh);
        if kind != 2 && kind != 3 {
            return false;
        }
        let under: Vec<Rect> = rects
            .iter()
            .map(|r| r.intersect(area))
            .filter(|r| r.width() > 0.0 && r.height() > 0.0)
            .collect();
        let Some((_, pieces)) = exclusion_clip(area, rects, Affine::IDENTITY) else {
            return false;
        };
        shading_shown_elsewhere(&self.inner, sh, to_shading, &under, &pieces)
    }

    /// Take the content under `marks` out of `page`, using the probe of the
    /// page. Fails, sending the page to the picture method, when something
    /// under the marks cannot be taken out (the page may be half edited then:
    /// it is drawn as a picture from that state).
    pub(super) fn redact_page(
        &mut self,
        page: usize,
        marks: &[Mark],
        probe: &Probe,
        markers: &HashMap<(StreamKey, usize), usize>,
        counts: &Counts,
        work: &mut Work,
    ) -> std::result::Result<(), PictureReason> {
        use PictureReason as R;
        let page_id = self.page_id(page).map_err(|_| R::Content)?;
        let pp = probe.pages.get(&page).ok_or(R::Content)?;
        let size = self.page_size_of(page_id);
        self.own_page_resources(page_id).map_err(|_| R::Content)?;
        let res = self
            .inner
            .get_dictionary(page_id)
            .ok()
            .and_then(|d| d.get(b"Resources").ok())
            .and_then(|r| r.as_dict().ok())
            .cloned()
            .unwrap_or_default();
        let j = joined(&self.inner, page_id);
        let c = size.crop_box;
        let gs0 = page_state(Rect::new(c[0] as f64, c[1] as f64, c[2] as f64, c[3] as f64));
        let mut ctx = Ctx {
            marks,
            probe,
            page: pp,
            page_id,
            markers,
            counts,
            used_glyphs: HashSet::new(),
            used_images: HashSet::new(),
            next_name: 0,
        };
        let holder = Holder {
            key: StreamKey::Page(page),
            data: j.data.clone(),
            res,
            gs0,
            chain: Vec::new(),
        };
        let mut local = Work::default();
        let out = self.redact_holder(&holder, &mut ctx, 0, &mut Vec::new(), &mut local)?;
        // What the probe saw under a mark must have been taken out.
        for (i, g) in pp.glyphs.iter().enumerate() {
            if !g.covered && ctx.under(&g.body) && !ctx.used_glyphs.contains(&i) {
                return Err(if g.op.is_none() { R::Pattern } else { R::Content });
            }
        }
        for (i, im) in pp.images.iter().enumerate() {
            if !im.covered && ctx.under(&im.bounds) && !ctx.used_images.contains(&i) {
                return Err(R::Picture);
            }
        }
        check_splices(&j, &out.splices).map_err(|_| R::Content)?;
        rewrite(&mut self.inner, page_id, &j, &out.splices).map_err(|_| R::Content)?;
        let mut res = out.res;
        let data = joined(&self.inner, page_id).data;
        drop_unused(&self.inner, &data, &mut res);
        self.inner
            .get_dictionary_mut(page_id)
            .map_err(|_| R::Content)?
            .set("Resources", Object::Dictionary(res));
        for m in out.touched {
            local.touched.insert((TouchKey::Page(page_id), m));
        }
        work.merge(local);
        Ok(())
    }

    fn redact_holder(
        &mut self,
        h: &Holder,
        ctx: &mut Ctx<'_>,
        depth: usize,
        stack: &mut Vec<ObjectId>,
        work: &mut Work,
    ) -> std::result::Result<HolderOut, PictureReason> {
        use PictureReason as R;
        let sc = scan(&self.inner, &h.data, &h.res, &h.gs0);
        let rects = ctx.rects();
        let mut res = h.res.clone();
        let mut splices: Vec<Splice> = Vec::new();
        // Byte ranges of the operations whose drawing changed.
        let mut changed: Vec<(usize, usize)> = Vec::new();

        // ---- Text ----
        let (tops, ends) = text_ops_from(&h.data, &h.gs0.text);
        let mut drawn: HashMap<usize, usize> = HashMap::new();
        let mut removed: BTreeMap<usize, Vec<RemovedGlyph>> = BTreeMap::new();
        for (gi, g) in ctx.page.glyphs.iter().enumerate() {
            if g.covered || g.chain != h.chain {
                continue;
            }
            let Some(&(key, k)) = g.op.and_then(|m| ctx.probe.ids.text.get(m)) else {
                continue;
            };
            if key != h.key {
                continue;
            }
            let c = drawn.entry(k).or_insert(0);
            let index = *c;
            *c += 1;
            if ctx.under(&g.body) {
                removed.entry(k).or_default().push(RemovedGlyph {
                    index,
                    to_next: g.to_next,
                    own_advance: g.own_advance,
                });
                ctx.used_glyphs.insert(gi);
            }
        }
        // Text the probe did not see (a font it cannot read): where it is
        // drawn is unknown.
        for (k, op) in tops.iter().enumerate() {
            if !drawn.contains_key(&k) && shows_glyphs(&h.data, op) {
                return Err(R::Font);
            }
        }
        if !removed.is_empty() {
            let fonts = res
                .get(b"Font")
                .ok()
                .and_then(|f| deref(&self.inner, f).as_dict().ok())
                .cloned()
                .unwrap_or_default();
            let out = glyph_removal_splices(&self.inner, &h.data, &tops, &fonts, &removed, &drawn)
                .map_err(|_| R::Font)?;
            for k in removed.keys() {
                let op = &tops[*k];
                changed.push((op.start, op.end));
                // Text painted under a soft mask that meets a mark.
                if sc.masked_text.iter().any(|(s, e, (gid, m))| {
                    *s == op.start && *e == op.end && self.mask_meets(&mask_gs(*gid, *m), &rects)
                }) {
                    return Err(R::SoftMask);
                }
            }
            work.glyphs += out.removed;
            splices.extend(out.splices);
            // Clipping text that lost every glyph would stop clipping.
            for obj in out.emptied_clips {
                let end = ends.get(obj).copied().flatten().ok_or(R::Content)?;
                let ctm = sc
                    .text_ends
                    .iter()
                    .find(|(e, _)| *e == end)
                    .map(|(_, m)| *m)
                    .ok_or(R::Content)?;
                if ctm.determinant().abs() < 1e-12 {
                    continue;
                }
                splices.push(Splice {
                    start: end - 2,
                    end,
                    with: format!("ET {}", empty_clip(ctm.inverse())).into_bytes(),
                });
            }
        }

        // ---- Paths ----
        for p in &sc.paths {
            let PathEdit::Replace(text) = redact_path(p, &rects) else {
                continue;
            };
            self.check_paint(p, &res, &h.gs0, &rects)?;
            splices.push(Splice {
                start: p.start,
                end: p.end,
                with: text.into_bytes(),
            });
            changed.push((p.start, p.end));
            work.paths += 1;
        }

        // ---- Shadings ----
        for (s, e, name, gs) in &sc.shadings {
            let Some(sh) = resource(&self.inner, &res, b"Shading", name).and_then(|o| match o {
                Object::Dictionary(d) => Some(d.clone()),
                Object::Stream(s) => Some(s.dict.clone()),
                _ => None,
            }) else {
                continue;
            };
            let mut area = gs.clip;
            if let Some(b) = rect_value(&self.inner, sh.get(b"BBox").ok()) {
                area = area.intersect(gs.ctm.transform_rect_bbox(b));
            }
            if area.width() <= 0.0 || area.height() <= 0.0 || !rects.iter().any(|r| overlaps(area, *r)) {
                continue;
            }
            if self.mask_meets(gs, &rects) {
                return Err(R::SoftMask);
            }
            if gs.ctm.determinant().abs() < 1e-12 {
                continue;
            }
            let with = match exclusion_clip(area, &rects, gs.ctm.inverse()) {
                // Painted under the marks only: it goes.
                None => String::new(),
                Some((clip, _)) => {
                    if !self.shading_shown(&sh, gs.ctm.inverse(), area, &rects) {
                        return Err(R::Shading);
                    }
                    format!("q {clip} {} sh Q", name_token(name))
                }
            };
            splices.push(Splice {
                start: *s,
                end: *e,
                with: with.into_bytes(),
            });
            changed.push((*s, *e));
            work.paths += 1;
        }

        // ---- Pictures ----
        for d in &sc.images {
            let b = d.gs.ctm.transform_rect_bbox(Rect::new(0.0, 0.0, 1.0, 1.0));
            let hit: Vec<&Mark> = ctx.marks.iter().filter(|m| overlaps(b, m.pdf)).collect();
            if hit.is_empty() {
                continue;
            }
            if let Some(mk) = ctx.markers.get(&(h.key, d.op)) {
                for (i, im) in ctx.page.images.iter().enumerate() {
                    if im.marker == Some(Marker::Do(*mk)) && im.chain == h.chain {
                        ctx.used_images.insert(i);
                    }
                }
            }
            if self.mask_meets(&d.gs, &rects) {
                return Err(R::SoftMask);
            }
            let Ok(s) = self.inner.get_object(d.id).and_then(Object::as_stream) else {
                return Err(R::Picture);
            };
            let dim = |k: &[u8]| {
                s.dict
                    .get(k)
                    .ok()
                    .and_then(|v| v.as_i64().ok())
                    .unwrap_or(0)
                    .max(0) as u32
            };
            let (w, hh) = (dim(b"Width"), dim(b"Height"));
            let areas: Vec<([u32; 4], &RedactionLook)> = hit
                .iter()
                .filter_map(|m| {
                    pixel_area(d.gs.ctm, w, hh, [m.pdf.x0, m.pdf.y0, m.pdf.x1, m.pdf.y1])
                        .map(|a| (a, &m.look))
                })
                .collect();
            if areas.is_empty() {
                continue;
            }
            let mut px = decode_image(&self.inner, d.id).map_err(|_| R::Picture)?;
            for (a, look) in &areas {
                super::cover(&mut px, *a, look);
            }
            let colors = picture_colors(&self.inner, d.id);
            let (stream, smask, old_mask) = self.new_picture(d.id, &px, colors).map_err(|_| R::Picture)?;
            if ctx.alone(d.id) {
                self.put_picture(d.id, stream, smask, old_mask);
            } else {
                let mut stream = stream;
                // The structure tree knows the original only.
                stream.dict.remove(b"StructParent");
                if let Some(m) = smask {
                    let mid = self.inner.add_object(m);
                    stream.dict.set("SMask", Object::Reference(mid));
                }
                let nid = self.inner.add_object(stream);
                let name = ctx.fresh(&res, "FoliRdI");
                xobjects_mut(&mut res).set(name.clone(), Object::Reference(nid));
                splices.push(Splice {
                    start: d.start,
                    end: d.end,
                    with: format!("{} Do", name_token(&name)).into_bytes(),
                });
                work.replaced.insert(d.id);
            }
            changed.push((d.start, d.end));
            work.pictures += 1;
        }

        // ---- Inline pictures ----
        for (opi, s, e, gs) in &sc.inline {
            let b = gs.ctm.transform_rect_bbox(Rect::new(0.0, 0.0, 1.0, 1.0));
            let hit: Vec<&Mark> = ctx.marks.iter().filter(|m| overlaps(b, m.pdf)).collect();
            if hit.is_empty() {
                continue;
            }
            for (i, im) in ctx.page.images.iter().enumerate() {
                if let Some(Marker::Inline(mk)) = im.marker {
                    if ctx.probe.ids.inline.get(mk) == Some(&(h.key, *opi)) && im.chain == h.chain {
                        ctx.used_images.insert(i);
                    }
                }
            }
            if self.mask_meets(gs, &rects) {
                return Err(R::SoftMask);
            }
            let stream = inline_picture(&self.inner, &h.data[*s..*e], &res).ok_or(R::Picture)?;
            let dim = |k: &[u8]| {
                stream
                    .dict
                    .get(k)
                    .ok()
                    .and_then(|v| v.as_i64().ok())
                    .unwrap_or(0)
                    .max(0) as u32
            };
            let (w, hh) = (dim(b"Width"), dim(b"Height"));
            let tmp = self.inner.add_object(stream);
            let mut px = decode_image(&self.inner, tmp).map_err(|_| R::Picture)?;
            for m in &hit {
                if let Some(a) = pixel_area(gs.ctm, w, hh, [m.pdf.x0, m.pdf.y0, m.pdf.x1, m.pdf.y1]) {
                    super::cover(&mut px, a, &m.look);
                }
            }
            let colors = picture_colors(&self.inner, tmp);
            let (st, sm, om) = self.new_picture(tmp, &px, colors).map_err(|_| R::Picture)?;
            self.put_picture(tmp, st, sm, om);
            let name = ctx.fresh(&res, "FoliRdN");
            xobjects_mut(&mut res).set(name.clone(), Object::Reference(tmp));
            splices.push(Splice {
                start: *s,
                end: *e,
                with: format!("{} Do", name_token(&name)).into_bytes(),
            });
            changed.push((*s, *e));
            work.pictures += 1;
        }

        // ---- Forms ----
        for d in &sc.forms {
            let Ok(fs) = self.inner.get_object(d.id).and_then(Object::as_stream).cloned() else {
                continue;
            };
            let bbox =
                rect_value(&self.inner, fs.dict.get(b"BBox").ok()).unwrap_or(Rect::new(-1e6, -1e6, 1e6, 1e6));
            let m = d.gs.ctm * affine_of(&self.inner, fs.dict.get(b"Matrix").ok());
            let area = m.transform_rect_bbox(bbox);
            let marker = ctx.markers.get(&(h.key, d.op)).copied();
            let mut chain = h.chain.clone();
            chain.extend(marker);
            let under =
                rects.iter().any(|r| overlaps(area, *r)) || (marker.is_some() && ctx.drawn_under(&chain));
            if !under {
                continue;
            }
            if marker.is_none() {
                // Drawn, but the probe does not know the drawing.
                return Err(R::Content);
            }
            if depth >= 12 || stack.contains(&d.id) {
                return Err(R::Nesting);
            }
            if self.mask_meets(&d.gs, &rects) {
                return Err(R::SoftMask);
            }
            let own = fs
                .dict
                .get(b"Resources")
                .ok()
                .and_then(|r| deref(&self.inner, r).as_dict().ok())
                .cloned();
            let sub_res = private_resources(&self.inner, own.as_ref().unwrap_or(&res));
            let mut gs1 = d.gs.clone();
            gs1.ctm = m;
            gs1.clip = d.gs.clip.intersect(area);
            let sub = Holder {
                key: StreamKey::Form(d.id),
                data: stream_data(&fs),
                res: sub_res,
                gs0: gs1,
                chain,
            };
            stack.push(d.id);
            let out = self.redact_holder(&sub, ctx, depth + 1, stack, work);
            stack.pop();
            let out = out?;
            if !out.changed {
                continue;
            }
            let mut data = sub.data.clone();
            let mut list = out.splices;
            list.sort_by_key(|s| std::cmp::Reverse(s.start));
            for s in list {
                data.splice(s.start..s.end, s.with);
            }
            let mut fres = out.res;
            drop_unused(&self.inner, &data, &mut fres);
            let dict = form_dict(&self.inner, &fs.dict, fres);
            let mut st = Stream::new(dict, data);
            let _ = st.compress();
            let target = if ctx.alone(d.id) {
                self.inner.objects.insert(d.id, Object::Stream(st));
                d.id
            } else {
                let nid = self.inner.add_object(st);
                let name = ctx.fresh(&res, "FoliRdF");
                xobjects_mut(&mut res).set(name.clone(), Object::Reference(nid));
                splices.push(Splice {
                    start: d.start,
                    end: d.end,
                    with: format!("{} Do", name_token(&name)).into_bytes(),
                });
                work.replaced.insert(d.id);
                work.copies.push((d.id, nid, ctx.page_id));
                nid
            };
            for mcid in out.touched {
                work.touched.insert((TouchKey::Stream(target), mcid));
            }
            changed.push((d.start, d.end));
            work.forms += 1;
        }

        // ---- Marked content around what changed ----
        let mut touched = Vec::new();
        for mk in &sc.marked {
            if !changed.iter().any(|(a, b)| mk.holds(*a, *b)) {
                continue;
            }
            touched.extend(mk.mcid);
            if mk.tag == b"OC" {
                continue;
            }
            let op = &h.data[mk.begin.0..mk.begin.1];
            let new = match &mk.props {
                Props::Inline(_) => strip_alternates(op, None),
                Props::Named(n) => {
                    let named = resource(&self.inner, &res, b"Properties", n).and_then(|d| d.as_dict().ok());
                    strip_alternates(op, named)
                }
                Props::None => None,
            };
            if let Some(new) = new {
                splices.push(Splice {
                    start: mk.begin.0,
                    end: mk.begin.1,
                    with: new,
                });
            }
        }

        splices.sort_by_key(|s| (s.start, s.end));
        if splices.windows(2).any(|w| w[0].end > w[1].start) {
            return Err(R::Content);
        }
        Ok(HolderOut {
            changed: !changed.is_empty() || !splices.is_empty(),
            splices,
            res,
            touched,
        })
    }

    /// A path painted with a pattern or under a soft mask that cannot be
    /// redacted by cutting its geometry.
    fn check_paint(
        &self,
        p: &PathObj,
        res: &Dictionary,
        gs0: &Gs,
        rects: &[Rect],
    ) -> std::result::Result<(), PictureReason> {
        use PictureReason as R;
        if self.mask_meets(&p.gs, rects) {
            return Err(R::SoftMask);
        }
        let area = path_bounds(p)
            .map(|b| b.intersect(p.gs.clip))
            .unwrap_or(Rect::ZERO);
        let mut patterns = Vec::new();
        if matches!(p.paint, Paint::Fill(_) | Paint::FillStroke(_)) {
            patterns.extend(p.gs.fill_pattern.clone());
        }
        if matches!(p.paint, Paint::Stroke | Paint::FillStroke(_)) {
            patterns.extend(p.gs.stroke_pattern.clone());
        }
        for name in patterns.into_iter().filter(|n| !n.is_empty()) {
            match pattern_kind(&self.inner, res, &name) {
                PatternKind::Tiling(true) => return Err(R::Pattern),
                PatternKind::Tiling(false) => {}
                PatternKind::Shading(sh, matrix) => {
                    // Pattern space is the default space of the stream.
                    let to_shading = (gs0.ctm * matrix).inverse();
                    if area.width() > 0.0
                        && area.height() > 0.0
                        && !self.shading_shown(&sh, to_shading, area, rects)
                    {
                        return Err(R::Shading);
                    }
                }
                PatternKind::Unknown => return Err(R::Pattern),
            }
        }
        Ok(())
    }
}

/// A state holding only a soft mask (to test it).
fn mask_gs(gid: ObjectId, m: Affine) -> Gs {
    let mut gs = page_state(Rect::ZERO);
    gs.smask = Some((gid, m));
    gs
}

/// How many times each picture and form is drawn in the document.
pub(super) struct Counts {
    pub map: HashMap<ObjectId, usize>,
    /// Every drawing was counted (very large documents stop early: then
    /// nothing is changed in place and nothing is removed for not being drawn).
    pub complete: bool,
}

impl Counts {
    pub fn get(&self, id: &ObjectId) -> Option<&usize> {
        self.map.get(id)
    }
}

/// How many times each picture and form is drawn in the document: by the
/// pages, the forms they draw and the appearances of annotations. Drawings
/// inside patterns and soft masks count twice (they repeat).
pub(super) fn drawing_counts(doc: &lopdf::Document) -> Counts {
    fn walk(
        doc: &lopdf::Document,
        data: &[u8],
        res: &Dictionary,
        mult: usize,
        counts: &mut HashMap<ObjectId, usize>,
        depth: usize,
        budget: &mut usize,
    ) {
        if depth > 12 {
            return;
        }
        for op in operations(data) {
            if *budget == 0 {
                return;
            }
            *budget -= 1;
            let (cat, name, factor): (&[u8], _, usize) = match op.operator {
                b"Do" => (b"XObject", op.operands.first().and_then(|r| name_of(data, *r)), 1),
                b"scn" | b"SCN" => (b"Pattern", op.operands.last().and_then(|r| name_of(data, *r)), 2),
                b"gs" => {
                    let g = op
                        .operands
                        .first()
                        .and_then(|r| name_of(data, *r))
                        .and_then(|n| resource(doc, res, b"ExtGState", &n))
                        .and_then(|d| d.as_dict().ok())
                        .and_then(|d| d.get(b"SMask").ok())
                        .map(|m| deref(doc, m))
                        .and_then(|m| m.as_dict().ok())
                        .and_then(|m| m.get(b"G").ok())
                        .and_then(|g| g.as_reference().ok());
                    if let Some(g) = g {
                        *counts.entry(g).or_default() += 2;
                        if let Ok(s) = doc.get_object(g).and_then(Object::as_stream) {
                            let r = stream_resources(doc, s, res);
                            walk(doc, &stream_data(s), &r, 2, counts, depth + 1, budget);
                        }
                    }
                    continue;
                }
                _ => continue,
            };
            let Some(id) = name.and_then(|n| {
                res.get(cat)
                    .ok()
                    .and_then(|x| deref(doc, x).as_dict().ok())
                    .and_then(|x| x.get(&n).ok())
                    .and_then(|v| v.as_reference().ok())
            }) else {
                continue;
            };
            let m = mult * factor;
            *counts.entry(id).or_default() += m;
            if cat == b"XObject" && !is_form(doc, id) {
                continue;
            }
            if let Ok(s) = doc.get_object(id).and_then(Object::as_stream) {
                let r = stream_resources(doc, s, res);
                walk(doc, &stream_data(s), &r, m, counts, depth + 1, budget);
            }
        }
    }
    let mut counts = HashMap::new();
    let mut budget = 4_000_000usize;
    for (_, pid) in doc.get_pages() {
        let res = crate::page_text::probe::page_resources(doc, pid);
        let data = joined(doc, pid).data;
        walk(doc, &data, &res, 1, &mut counts, 0, &mut budget);
        for (_, ad) in doc
            .get_dictionary(pid)
            .map(|pd| crate::annots::annot_refs(doc, pd))
            .unwrap_or_default()
        {
            let Some(ap) = ad.get(b"AP").ok().and_then(|a| deref(doc, a).as_dict().ok()) else {
                continue;
            };
            for (_, v) in ap.iter() {
                let ids: Vec<ObjectId> = match v {
                    Object::Reference(r) => match doc.get_object(*r) {
                        Ok(Object::Dictionary(states)) => {
                            states.iter().filter_map(|(_, s)| s.as_reference().ok()).collect()
                        }
                        _ => vec![*r],
                    },
                    Object::Dictionary(states) => {
                        states.iter().filter_map(|(_, s)| s.as_reference().ok()).collect()
                    }
                    _ => Vec::new(),
                };
                for id in ids {
                    *counts.entry(id).or_default() += 1;
                    if let Ok(s) = doc.get_object(id).and_then(Object::as_stream) {
                        let r = stream_resources(doc, s, &Dictionary::new());
                        walk(doc, &stream_data(s), &r, 1, &mut counts, 1, &mut budget);
                    }
                }
            }
        }
    }
    let complete = budget > 0;
    if !complete {
        // Not every drawing was counted: nothing is changed in place.
        for v in counts.values_mut() {
            *v = (*v).max(2);
        }
    }
    Counts {
        map: counts,
        complete,
    }
}

/// Remove `ids` from the document and every reference to them (dictionary
/// entries and array elements), so nothing keeps them.
pub(super) fn purge(doc: &mut lopdf::Document, ids: &HashSet<ObjectId>) {
    fn strip(o: &mut Object, ids: &HashSet<ObjectId>) {
        match o {
            Object::Dictionary(d) => strip_dict(d, ids),
            Object::Stream(s) => strip_dict(&mut s.dict, ids),
            Object::Array(a) => {
                a.retain(|x| !matches!(x, Object::Reference(r) if ids.contains(r)));
                for x in a.iter_mut() {
                    strip(x, ids);
                }
            }
            _ => {}
        }
    }
    fn strip_dict(d: &mut Dictionary, ids: &HashSet<ObjectId>) {
        let gone: Vec<Vec<u8>> = d
            .iter()
            .filter(|(_, v)| matches!(v, Object::Reference(r) if ids.contains(r)))
            .map(|(k, _)| k.clone())
            .collect();
        for k in gone {
            d.remove(&k);
        }
        for (_, v) in d.iter_mut() {
            strip(v, ids);
        }
    }
    if ids.is_empty() {
        return;
    }
    for id in ids {
        doc.objects.remove(id);
    }
    for o in doc.objects.values_mut() {
        strip(o, ids);
    }
    strip_dict(&mut doc.trailer, ids);
}
