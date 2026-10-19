//! Checking a redaction: in memory before the covers are drawn (a page that
//! fails is drawn as a picture) and on the exact bytes a save writes, read
//! again from scratch (decrypted with the kept key when protected). Nothing is
//! written when that check fails.
//!
//! On every redacted page: no glyph under a mark (hidden layers and
//! clip-only text included, through the probe that reveals them); no path
//! geometry inside a mark and no painted path the renderer finds entirely
//! inside one; the pictures under a solid mark show its colour there; no text
//! the renderer cannot place. In the whole file: one revision, no XFA form, no
//! original replaced by a copy left behind, and the removed words nowhere in
//! the objects only the redacted pages use (unless the page still shows them
//! elsewhere); with quoting chosen, not in bookmarks, document information,
//! destination names, links or tags either.

use std::collections::{BTreeMap, HashMap, HashSet};

use hayro::kurbo::Rect;
use lopdf::{Object, ObjectId};

use super::counts::drawing_counts;
use super::paths::{PathEdit, redact_path};
use super::sanitize::is_struct_elem;
use super::scan::{Gs, page_state, resource, scan};
use super::terms::{Term, count_in_bytes, count_terms};
use super::{Overlay, RedactionReport};
use crate::content_edit::joined;
use crate::document::{Document, ProtectionKey};
use crate::error::{Error, Result};
use crate::marks::display_to_pdf;
use crate::page_text::probe::{
    Marker, PageProbe, Probe, ProbeOptions, StreamKey, probe_pages, stream_data, tagged_ranges,
};
use crate::pictures::{Colors, decode_image, filters, picture_colors, pixel_area};
use crate::textedit::{shows_glyphs, text_ops_from};
use crate::util::{deref, text_of};

/// Where the removed text is still quoted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuotePlace {
    Bookmarks,
    Metadata,
    Names,
    Links,
    Tags,
}

/// A problem the check found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckProblem {
    /// Text under a redacted area (or text whose place cannot be known).
    Text { page: usize },
    /// A picture not covered under a solid redacted area.
    Picture { page: usize },
    /// Path geometry under a redacted area.
    Drawing { page: usize },
    /// Removed content still stored in the file: an original left behind,
    /// the removed words in objects of the page, an XFA form.
    Residual { page: Option<usize> },
    /// The file keeps an earlier version.
    Revisions,
    /// The removed text is still quoted (with quoting chosen).
    Quote { place: QuotePlace },
}

/// What the check of a redacted file found.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RedactionCheck {
    /// Redacted pages checked.
    pub pages: usize,
    pub problems: Vec<CheckProblem>,
    /// Blurred or pixelated areas: their pictures are only hidden from view.
    pub visual_only: usize,
    /// Places where the removed words still appear and that are only
    /// reported: page text, comments, form values (and, without quoting
    /// chosen, bookmarks, metadata, names, links, tags).
    pub quoted_elsewhere: usize,
    /// Areas of pictures too small to check (lossy pictures).
    pub unchecked_pictures: usize,
}

impl RedactionCheck {
    pub fn passed(&self) -> bool {
        self.problems.is_empty()
    }
}

/// A mark as the check needs it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CheckMark {
    /// Display space (as the page is shown): the same before and after a
    /// page becomes a picture.
    pub shown: [f32; 4],
    pub solid: bool,
    pub fill: [u8; 3],
}

impl CheckMark {
    pub(crate) fn new(shown: [f32; 4], overlay: Overlay, fill: [f32; 3]) -> Self {
        let q = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        CheckMark {
            shown,
            solid: overlay == Overlay::Standard,
            fill: [q(fill[0]), q(fill[1]), q(fill[2])],
        }
    }
}

/// What a redaction leaves for the check of the saved file.
#[derive(Clone, Default, PartialEq)]
pub(crate) struct CheckData {
    pub marks: BTreeMap<usize, Vec<CheckMark>>,
    /// Every word of the removed text.
    pub terms: Vec<Term>,
    /// The words (lower case) the person chose to leave where they are quoted.
    pub skipped: Vec<String>,
    pub quoting: bool,
    /// The `/FolitectaMark` of the covers drawn by this redaction.
    pub cover_tag: Vec<u8>,
    /// Originals replaced by copies, with the page that drew them.
    pub replaced: Vec<(ObjectId, usize)>,
}

impl std::fmt::Debug for CheckData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The removed text is never written to logs.
        f.debug_struct("CheckData")
            .field("pages", &self.marks.len())
            .field("terms", &self.terms.len())
            .field("quoting", &self.quoting)
            .finish()
    }
}

fn meets(a: &[f32; 4], b: &[f32; 4]) -> bool {
    a[0] < b[2] && a[2] > b[0] && a[1] < b[3] && a[3] > b[1]
}

fn inside(a: &[f32; 4], b: &[f32; 4]) -> bool {
    a[0] > b[0] && a[2] < b[2] && a[1] > b[1] && a[3] < b[3]
}

fn shrink(r: [f32; 4], d: f32) -> [f32; 4] {
    [r[0] + d, r[1] + d, r[2] - d, r[3] - d]
}

fn overlaps(a: Rect, b: Rect) -> bool {
    a.x0 < b.x1 && a.x1 > b.x0 && a.y0 < b.y1 && a.y1 > b.y0
}

/// What a picture under a solid mark shows there.
enum Pixels {
    Covered,
    Visible,
    Unchecked,
}

struct PageCheck<'a> {
    probe: &'a Probe,
    page: &'a PageProbe,
    markers: HashMap<(StreamKey, usize), usize>,
    marks: Vec<(CheckMark, Rect)>,
    /// The marks shrunk a little, page space: geometry on their edges is fine.
    inner: Vec<Rect>,
    covers: Vec<(usize, usize)>,
    used_images: HashSet<usize>,
    problems: Vec<CheckProblem>,
    unchecked: usize,
    p: usize,
}

impl PageCheck<'_> {
    fn under(&self, shown: &[f32; 4]) -> bool {
        self.marks.iter().any(|(m, _)| meets(shown, &m.shown))
    }

    fn problem(&mut self, pr: CheckProblem) {
        if !self.problems.contains(&pr) {
            self.problems.push(pr);
        }
    }
}

impl Document {
    /// Check redacted pages: `marks` by page, `cover` the tag of the covers
    /// (None before they are drawn). Returns the problems by page and how
    /// many picture areas could not be checked.
    pub(crate) fn check_redacted_pages(
        &self,
        marks: &BTreeMap<usize, Vec<CheckMark>>,
        cover: Option<&[u8]>,
    ) -> (Vec<(usize, CheckProblem)>, usize) {
        let pages: Vec<usize> = marks.keys().copied().filter(|p| *p < self.page_count()).collect();
        let mut out = Vec::new();
        let probe = match probe_pages(
            &self.inner,
            &pages,
            ProbeOptions {
                reveal: true,
                forms: true,
            },
            cover,
        ) {
            Ok(p) => p,
            Err(_) => {
                return (
                    pages
                        .iter()
                        .map(|p| (*p, CheckProblem::Text { page: *p }))
                        .collect(),
                    0,
                );
            }
        };
        let markers = probe.ids.draw_markers();
        let mut unchecked = 0;
        for p in pages {
            let Some(pp) = probe.pages.get(&p) else {
                out.push((p, CheckProblem::Text { page: p }));
                continue;
            };
            let Ok(page_id) = self.page_id(p) else { continue };
            let size = self.page_size_of(page_id);
            let to_pdf = |d: [f32; 4]| -> Rect {
                let a = display_to_pdf(&size, d[0], size.height - d[3]);
                let b = display_to_pdf(&size, d[2], size.height - d[1]);
                Rect::new(
                    a.0.min(b.0) as f64,
                    a.1.min(b.1) as f64,
                    a.0.max(b.0) as f64,
                    a.1.max(b.1) as f64,
                )
            };
            let ms: Vec<(CheckMark, Rect)> = marks[&p].iter().map(|m| (m.clone(), to_pdf(m.shown))).collect();
            let mut pc = PageCheck {
                probe: &probe,
                page: pp,
                markers: markers.clone(),
                inner: ms.iter().map(|(_, r)| r.inflate(-0.01, -0.01)).collect(),
                marks: ms,
                covers: cover
                    .map(|t| tagged_ranges(&self.inner, page_id, t))
                    .unwrap_or_default(),
                used_images: HashSet::new(),
                problems: Vec::new(),
                unchecked: 0,
                p,
            };
            // Glyphs under a mark, as the renderer draws them.
            if pp.glyphs.iter().any(|g| !g.covered && pc.under(&g.body)) {
                pc.problem(CheckProblem::Text { page: p });
            }
            // Paths the renderer paints entirely inside a mark.
            let inside_marks: Vec<[f32; 4]> = pc.marks.iter().map(|(m, _)| shrink(m.shown, 0.01)).collect();
            if pp
                .paths
                .iter()
                .any(|pa| !pa.covered && inside_marks.iter().any(|m| inside(&pa.bounds, m)))
            {
                pc.problem(CheckProblem::Drawing { page: p });
            }
            let j = joined(&self.inner, page_id);
            let res = crate::page_text::probe::page_resources(&self.inner, page_id);
            let c = size.crop_box;
            let gs0 = page_state(Rect::new(c[0] as f64, c[1] as f64, c[2] as f64, c[3] as f64));
            self.check_holder(
                &mut pc,
                StreamKey::Page(p),
                &j.data,
                &res,
                &gs0,
                &[],
                0,
                &mut Vec::new(),
            );
            // Pictures under solid marks that no holder explains.
            for (i, im) in pp.images.iter().enumerate() {
                let solid_under = pc
                    .marks
                    .iter()
                    .any(|(m, _)| m.solid && meets(&im.bounds, &m.shown));
                if !im.covered && solid_under && !pc.used_images.contains(&i) {
                    pc.problem(CheckProblem::Picture { page: p });
                }
            }
            unchecked += pc.unchecked;
            out.extend(pc.problems.into_iter().map(|pr| (p, pr)));
        }
        (out, unchecked)
    }

    #[allow(clippy::too_many_arguments)]
    fn check_holder(
        &self,
        pc: &mut PageCheck<'_>,
        key: StreamKey,
        data: &[u8],
        res: &lopdf::Dictionary,
        gs0: &Gs,
        chain: &[usize],
        depth: usize,
        stack: &mut Vec<ObjectId>,
    ) {
        let p = pc.p;
        let doc = &self.inner;
        let covers = if key == StreamKey::Page(p) {
            pc.covers.clone()
        } else {
            Vec::new()
        };
        let in_cover = |a: usize| covers.iter().any(|(s, e)| a >= *s && a < *e);
        let sc = scan(doc, data, res, gs0);
        // Text the renderer did not place.
        let (tops, _) = text_ops_from(data, &gs0.text);
        let mut drawn: HashSet<usize> = HashSet::new();
        for g in &pc.page.glyphs {
            if g.chain != chain {
                continue;
            }
            if let Some(&(k, i)) = g.op.and_then(|m| pc.probe.ids.text.get(m)) {
                if k == key {
                    drawn.insert(i);
                }
            }
        }
        for (k, op) in tops.iter().enumerate() {
            if !drawn.contains(&k) && !in_cover(op.start) && shows_glyphs(data, op) {
                pc.problem(CheckProblem::Text { page: p });
            }
        }
        // Path geometry inside the marks (the rules of the removal, on marks
        // shrunk a little).
        let inner = pc.inner.clone();
        for path in &sc.paths {
            if in_cover(path.start) {
                continue;
            }
            if redact_path(path, &inner) != PathEdit::Keep {
                pc.problem(CheckProblem::Drawing { page: p });
            }
        }
        for (s, _, name, gs) in &sc.shadings {
            if in_cover(*s) {
                continue;
            }
            let sh = resource(doc, res, b"Shading", name).and_then(|o| match o {
                Object::Dictionary(d) => Some(d.clone()),
                Object::Stream(s) => Some(s.dict.clone()),
                _ => None,
            });
            let kind = sh
                .as_ref()
                .and_then(|d| d.get(b"ShadingType").ok())
                .and_then(|t| deref(doc, t).as_i64().ok())
                .unwrap_or(0);
            if kind != 2 && kind != 3 && inner.iter().any(|r| overlaps(gs.clip, *r)) {
                pc.problem(CheckProblem::Drawing { page: p });
            }
        }
        // Pictures under solid marks show the mark's colour there.
        for d in &sc.images {
            if in_cover(d.start) {
                continue;
            }
            let b = d.gs.ctm.transform_rect_bbox(Rect::new(0.0, 0.0, 1.0, 1.0));
            let solid: Vec<(CheckMark, Rect)> = pc
                .marks
                .iter()
                .filter(|(m, r)| m.solid && overlaps(b, *r))
                .cloned()
                .collect();
            if let Some(mk) = pc.markers.get(&(key, d.op)).copied() {
                for (i, im) in pc.page.images.iter().enumerate() {
                    if im.marker == Some(Marker::Do(mk)) && im.chain == chain {
                        pc.used_images.insert(i);
                    }
                }
            }
            for (m, r) in &solid {
                match self.pixels_under(d.id, d.gs.ctm, m, *r) {
                    Pixels::Covered => {}
                    Pixels::Visible => pc.problem(CheckProblem::Picture { page: p }),
                    Pixels::Unchecked => pc.unchecked += 1,
                }
            }
        }
        // Inline pictures are never left under a solid mark.
        for (_, s, _, gs) in &sc.inline {
            let b = gs.ctm.transform_rect_bbox(Rect::new(0.0, 0.0, 1.0, 1.0));
            if !in_cover(*s)
                && pc
                    .marks
                    .iter()
                    .any(|(m, r)| m.solid && overlaps(b, r.inflate(-0.5, -0.5)))
            {
                pc.problem(CheckProblem::Picture { page: p });
            }
        }
        // Forms drawn under the marks, followed.
        for d in &sc.forms {
            if in_cover(d.start) {
                continue;
            }
            let Ok(fs) = doc.get_object(d.id).and_then(Object::as_stream) else {
                continue;
            };
            let bbox = crate::util::rect_of(doc, deref(doc, fs.dict.get(b"BBox").unwrap_or(&Object::Null)))
                .map(|r| Rect::new(r[0] as f64, r[1] as f64, r[2] as f64, r[3] as f64))
                .unwrap_or(Rect::new(-1e6, -1e6, 1e6, 1e6));
            let matrix = fs
                .dict
                .get(b"Matrix")
                .ok()
                .and_then(|m| deref(doc, m).as_array().ok())
                .map(|a| {
                    a.iter()
                        .filter_map(|x| crate::util::num(deref(doc, x)).map(f64::from))
                        .collect::<Vec<f64>>()
                })
                .filter(|v| v.len() == 6)
                .map(|v| hayro::kurbo::Affine::new([v[0], v[1], v[2], v[3], v[4], v[5]]))
                .unwrap_or(hayro::kurbo::Affine::IDENTITY);
            let m = d.gs.ctm * matrix;
            let area = m.transform_rect_bbox(bbox);
            let marker = pc.markers.get(&(key, d.op)).copied();
            let mut sub_chain = chain.to_vec();
            sub_chain.extend(marker);
            let probe_under = marker.is_some()
                && (pc
                    .page
                    .glyphs
                    .iter()
                    .any(|g| !g.covered && g.chain.starts_with(&sub_chain) && pc.under(&g.body))
                    || pc
                        .page
                        .images
                        .iter()
                        .any(|i| !i.covered && i.chain.starts_with(&sub_chain) && pc.under(&i.bounds)));
            if !pc.marks.iter().any(|(_, r)| overlaps(area, *r)) && !probe_under {
                continue;
            }
            if marker.is_none() || depth >= 12 || stack.contains(&d.id) {
                // A drawing the check cannot follow.
                pc.problem(CheckProblem::Drawing { page: p });
                continue;
            }
            let r = crate::page_text::probe::stream_resources(doc, fs, res);
            let mut gs1 = d.gs.clone();
            gs1.ctm = m;
            gs1.clip = d.gs.clip.intersect(area);
            stack.push(d.id);
            self.check_holder(
                pc,
                StreamKey::Form(d.id),
                &stream_data(fs),
                &r,
                &gs1,
                &sub_chain,
                depth + 1,
                stack,
            );
            stack.pop();
        }
    }

    /// Whether picture `id`, drawn with `ctm`, shows the colour of mark `m`
    /// (page space `r`) under it.
    fn pixels_under(&self, id: ObjectId, ctm: hayro::kurbo::Affine, m: &CheckMark, r: Rect) -> Pixels {
        let doc = &self.inner;
        let Ok(s) = doc.get_object(id).and_then(Object::as_stream) else {
            return Pixels::Unchecked;
        };
        let dim = |k: &[u8]| {
            s.dict
                .get(k)
                .ok()
                .and_then(|v| v.as_i64().ok())
                .unwrap_or(0)
                .max(0) as u32
        };
        let (w, h) = (dim(b"Width"), dim(b"Height"));
        let Some(area) = pixel_area(ctm, w, h, [r.x0, r.y0, r.x1, r.y1]) else {
            return Pixels::Covered;
        };
        let Ok(px) = decode_image(doc, id) else {
            return Pixels::Unchecked;
        };
        if px.dimensions() != (w, h) {
            return Pixels::Unchecked;
        }
        let last = filters(&s.dict).last().cloned().unwrap_or_default();
        let lossy = last == b"DCTDecode" || last == b"JPXDecode";
        let stencil = s.dict.get(b"ImageMask").ok().and_then(|v| v.as_bool().ok()) == Some(true);
        let colors = picture_colors(doc, id);
        let luma =
            ((m.fill[0] as u32 * 299 + m.fill[1] as u32 * 587 + m.fill[2] as u32 * 114 + 500) / 1000) as u8;
        // The colour the cover becomes in the picture, and how close it must be.
        let (want, tol) = match colors {
            Colors::Color => (m.fill, if lossy { 24 } else { 2 }),
            Colors::Gray => ([luma; 3], if lossy { 24 } else { 2 }),
            Colors::BlackWhite => ([if luma >= 128 { 255 } else { 0 }; 3], 2),
            Colors::Indexed => (m.fill, 24),
        };
        // Colour-managed pictures are read through the renderer.
        let tol = if s
            .dict
            .get(b"ColorSpace")
            .ok()
            .map(|c| deref(doc, c))
            .is_some_and(|c| {
                matches!(c, Object::Array(_)) || c.as_name().ok() == Some(b"DeviceCMYK".as_slice())
            }) {
            tol.max(32)
        } else {
            tol
        };
        // Lossy pictures blur the edges of the cover: their inside is checked.
        let (aw, ah) = (area[2] - area[0], area[3] - area[1]);
        let inset = if lossy { 16.min(aw / 4).min(ah / 4) } else { 0 };
        let a = [area[0] + inset, area[1] + inset, area[2] - inset, area[3] - inset];
        if a[2] <= a[0] || a[3] <= a[1] || (lossy && (aw < 8 || ah < 8)) {
            return Pixels::Unchecked;
        }
        for y in a[1]..a[3] {
            for x in a[0]..a[2] {
                let p = px.get_pixel(x, y);
                if p[3] < 250 {
                    return Pixels::Visible;
                }
                if stencil {
                    continue;
                }
                if (0..3).any(|c| (p[c] as i32 - want[c] as i32).unsigned_abs() > tol) {
                    return Pixels::Visible;
                }
            }
        }
        Pixels::Covered
    }
}

// ---- The bytes of a saved file ---------------------------------------------------------

/// A dictionary written at `at` (after optional white space and `<<`):
/// its end.
fn dict_end(bytes: &[u8], at: usize) -> Option<usize> {
    let mut i = at;
    let mut depth = 0i32;
    while i + 1 < bytes.len() {
        match bytes[i] {
            b'(' => {
                // Strings may hold anything.
                let mut level = 0;
                while i < bytes.len() {
                    match bytes[i] {
                        b'\\' => i += 1,
                        b'(' => level += 1,
                        b')' => {
                            level -= 1;
                            if level == 0 {
                                break;
                            }
                        }
                        _ => {}
                    }
                    i += 1;
                }
            }
            b'<' if bytes[i + 1] == b'<' => {
                depth += 1;
                i += 1;
            }
            b'>' if bytes[i + 1] == b'>' => {
                depth -= 1;
                i += 1;
                if depth == 0 {
                    return Some(i + 1);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

fn find(hay: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    hay.get(from..)?
        .windows(needle.len())
        .position(|w| w == needle)
        .map(|p| p + from)
}

/// The trailer dictionary of the last cross-reference section: its byte
/// range, from the raw bytes (the parsed trailer forgets `/Prev`).
fn last_trailer(bytes: &[u8]) -> Option<(usize, usize)> {
    let tail = bytes.len().saturating_sub(4096);
    let sx = bytes[tail..].windows(9).rposition(|w| w == b"startxref")? + tail;
    let num: String = bytes[sx + 9..]
        .iter()
        .skip_while(|c| c.is_ascii_whitespace())
        .take_while(|c| c.is_ascii_digit())
        .map(|c| *c as char)
        .collect();
    let at: usize = num.parse().ok()?;
    let rest = bytes.get(at..)?;
    let start = if rest.starts_with(b"xref") {
        find(bytes, b"trailer", at)? + 7
    } else {
        // A cross-reference stream: its dictionary.
        find(bytes, b"obj", at)? + 3
    };
    let open = find(bytes, b"<<", start)?;
    Some((open, dict_end(bytes, open)?))
}

/// The keys of a dictionary written in `text` (top level).
fn dict_keys(text: &[u8]) -> Option<lopdf::Dictionary> {
    let mut src = text.to_vec();
    src.extend_from_slice(b" FoliCheck");
    let c = lopdf::content::Content::decode(&src).ok()?;
    match c.operations.first()?.operands.first()? {
        Object::Dictionary(d) => Some(d.clone()),
        _ => None,
    }
}

/// The file holds one revision: one cross-reference section, without `/Prev`.
fn one_revision(bytes: &[u8]) -> bool {
    let Some((a, b)) = last_trailer(bytes) else {
        return false;
    };
    match dict_keys(&bytes[a..b]) {
        Some(d) => !d.has(b"Prev") && !d.has(b"XRefStm"),
        None => false,
    }
}

/// The plain objects of a protected file's bytes (whole, or with
/// encrypted incremental updates), decrypted with `key`.
fn decrypted(bytes: &[u8], key: &ProtectionKey) -> Result<Vec<u8>> {
    let mut doc = crate::document::load_with_key(bytes, key)?;
    doc.trailer.remove(b"Encrypt");
    doc.trailer.remove(b"Prev");
    crate::document::serialize(&mut doc)
}

/// The objects reachable from `start`, not following `skip` keys, and not
/// entering pages other than those of `start` (a destination naming a page
/// does not make what the page uses its own).
fn reachable(doc: &lopdf::Document, start: &[Object], skip: &[&[u8]]) -> HashSet<ObjectId> {
    let starts: HashSet<ObjectId> = start.iter().filter_map(|o| o.as_reference().ok()).collect();
    let is_page = |o: &Object| {
        o.as_dict()
            .ok()
            .and_then(|d| d.get(b"Type").ok())
            .and_then(|t| t.as_name().ok())
            .is_some_and(|t| t == b"Page" || t == b"Pages")
    };
    let mut seen = HashSet::new();
    let mut todo: Vec<&Object> = start.iter().collect();
    while let Some(o) = todo.pop() {
        match o {
            Object::Reference(r) => {
                if seen.insert(*r) {
                    if let Ok(x) = doc.get_object(*r) {
                        if !is_page(x) || starts.contains(r) {
                            todo.push(x);
                        }
                    }
                }
            }
            Object::Array(a) => todo.extend(a.iter()),
            Object::Dictionary(d) => todo.extend(
                d.iter()
                    .filter(|(k, _)| !skip.contains(&k.as_slice()))
                    .map(|(_, v)| v),
            ),
            Object::Stream(s) => todo.extend(
                s.dict
                    .iter()
                    .filter(|(k, _)| !skip.contains(&k.as_slice()))
                    .map(|(_, v)| v),
            ),
            _ => {}
        }
    }
    seen
}

/// The text strings of an object (not following references; names are
/// font, colour and resource names, not text).
fn strings_of(doc: &lopdf::Document, o: &Object, out: &mut Vec<String>) {
    match o {
        Object::String(..) => out.extend(text_of(doc, o)),
        Object::Array(a) => a.iter().for_each(|x| strings_of(doc, x, out)),
        Object::Dictionary(d) => d.iter().for_each(|(_, v)| strings_of(doc, v, out)),
        Object::Stream(s) => s.dict.iter().for_each(|(_, v)| strings_of(doc, v, out)),
        _ => {}
    }
}

/// A stream whose data is a font program or pixels (not text to search).
fn binary_stream(s: &lopdf::Stream) -> bool {
    s.dict.has(b"Length1")
        || s.dict.has(b"Length2")
        || matches!(
            s.dict.get(b"Subtype").ok().and_then(|v| v.as_name().ok()),
            Some(b"Image" | b"Type1C" | b"CIDFontType0C" | b"OpenType")
        )
}

/// Check the bytes of a redacted file before they are written: `key` is the
/// key of a protected document ([`Document::protection_key`]) and `report`
/// the report of the redaction.
pub fn check_redacted_bytes(
    bytes: &[u8],
    key: Option<&ProtectionKey>,
    report: &RedactionReport,
) -> Result<RedactionCheck> {
    let data = &report.check;
    let mut out = RedactionCheck {
        pages: data.marks.len(),
        visual_only: report.blurred_areas,
        ..RedactionCheck::default()
    };
    let problem = |p: CheckProblem, out: &mut RedactionCheck| {
        if !out.problems.contains(&p) {
            out.problems.push(p);
        }
    };
    if !one_revision(bytes) {
        problem(CheckProblem::Revisions, &mut out);
    }
    let encrypted = last_trailer(bytes)
        .and_then(|(a, b)| dict_keys(&bytes[a..b]))
        .is_some_and(|t| t.has(b"Encrypt"));
    let plain = match (encrypted, key) {
        (true, Some(k)) => decrypted(bytes, k)?,
        (true, None) => {
            return Err(Error::Invalid(
                "the file is protected and its key is not known".into(),
            ));
        }
        (false, _) => bytes.to_vec(),
    };
    let doc = Document::from_bytes(plain, None, None)?;
    let (pages, unchecked) = doc.check_redacted_pages(&data.marks, Some(&data.cover_tag));
    out.unchecked_pictures = unchecked;
    for (_, pr) in pages {
        problem(pr, &mut out);
    }
    let lo = &doc.inner;
    // Originals replaced by copies: gone, unless something still draws them.
    let counts = drawing_counts(lo);
    for (id, page) in &data.replaced {
        if lo.objects.contains_key(id) && counts.get(id).copied().unwrap_or(0) == 0 {
            problem(CheckProblem::Residual { page: Some(*page) }, &mut out);
        }
    }
    // An XFA form repeats the page text and the field values.
    let xfa = lo
        .catalog()
        .ok()
        .and_then(|c| c.get(b"AcroForm").ok())
        .and_then(|a| deref(lo, a).as_dict().ok())
        .is_some_and(|a| a.has(b"XFA"));
    if xfa {
        problem(CheckProblem::Residual { page: None }, &mut out);
    }
    let terms = &data.terms;
    if !terms.is_empty() {
        // The removed words in objects only a redacted page uses, unless the
        // page still shows them outside the marks.
        let page_ids = doc.page_ids();
        for (&p, marks) in &data.marks {
            let Some(&pid) = page_ids.get(p) else { continue };
            let shown: Vec<[f32; 4]> = marks.iter().map(|m| m.shown).collect();
            let visible = doc.visible_text(p, &shown);
            let page_terms: Vec<Term> = terms
                .iter()
                .filter(|t| count_terms(&visible, std::slice::from_ref(t)) == 0)
                .cloned()
                .collect();
            if page_terms.is_empty() {
                continue;
            }
            let mine = reachable(lo, &[Object::Reference(pid)], &[b"Parent", b"Annots", b"P", b"B"]);
            let mut others: Vec<Object> = page_ids
                .iter()
                .filter(|x| **x != pid)
                .map(|x| Object::Reference(*x))
                .collect();
            if let Ok(cat) = lo.catalog() {
                others.extend(
                    cat.iter()
                        .filter(|(k, _)| k.as_slice() != b"Pages")
                        .map(|(_, v)| v.clone()),
                );
            }
            if let Ok(i) = lo.trailer.get(b"Info") {
                others.push(i.clone());
            }
            let theirs = reachable(lo, &others, &[b"Parent", b"P", b"Pg", b"Obj", b"Stm"]);
            let leak = mine.iter().filter(|id| !theirs.contains(id)).any(|id| {
                let Ok(o) = lo.get_object(*id) else { return false };
                let mut texts = Vec::new();
                strings_of(lo, o, &mut texts);
                if texts.iter().any(|t| count_terms(t, &page_terms) > 0) {
                    return true;
                }
                match o {
                    Object::Stream(s) if !binary_stream(s) => {
                        count_in_bytes(&stream_data(s), &page_terms) > 0
                    }
                    _ => false,
                }
            });
            if leak {
                problem(CheckProblem::Residual { page: Some(p) }, &mut out);
            }
        }
        // Places that quote the removed text (the words left there on
        // purpose are only reported).
        let (left, chosen): (Vec<Term>, Vec<Term>) = terms
            .iter()
            .cloned()
            .partition(|t| data.skipped.contains(&t.text.iter().collect::<String>()));
        for (place, n) in doc.quote_places(&chosen) {
            if n == 0 {
                continue;
            }
            if data.quoting {
                problem(CheckProblem::Quote { place }, &mut out);
            } else {
                out.quoted_elsewhere += n;
            }
        }
        if !left.is_empty() {
            out.quoted_elsewhere += doc.quote_places(&left).iter().map(|(_, n)| n).sum::<usize>();
        }
        out.quoted_elsewhere += doc.quoted_in_pages_and_fields(terms);
    }
    Ok(out)
}

impl Document {
    /// The text a page shows outside the marks (`shown`, display space): the
    /// glyphs the probe draws (hidden ones too) and the text layer.
    fn visible_text(&self, page: usize, shown: &[[f32; 4]]) -> String {
        let mut s = String::new();
        if let Ok(pr) = probe_pages(
            &self.inner,
            &[page],
            ProbeOptions {
                reveal: true,
                forms: true,
            },
            None,
        ) {
            if let Some(pp) = pr.pages.get(&page) {
                for g in &pp.glyphs {
                    if !shown.iter().any(|m| meets(&g.body, m)) {
                        s.push_str(&g.text);
                    }
                }
            }
        }
        s.push('\n');
        // The text layer joins words its own way; its characters under the
        // marks are left out.
        if let Some(layer) = self.text_layer(page) {
            for (i, c) in layer.text.chars().enumerate() {
                let under = layer
                    .char_glyph
                    .get(i)
                    .copied()
                    .flatten()
                    .and_then(|g| layer.glyphs.get(g))
                    .is_some_and(|g| shown.iter().any(|m| meets(&g.bbox, m)));
                s.push(if under { ' ' } else { c });
            }
        }
        s
    }

    /// How many times the terms are quoted in each kind of place.
    fn quote_places(&self, terms: &[Term]) -> Vec<(QuotePlace, usize)> {
        let doc = &self.inner;
        let entry = |d: &lopdf::Dictionary, k: &[u8]| {
            d.get(k)
                .ok()
                .and_then(|v| text_of(doc, v))
                .map(|t| count_terms(&t, terms))
                .unwrap_or(0)
        };
        let uri = |d: &lopdf::Dictionary| {
            d.get(b"A")
                .ok()
                .and_then(|a| deref(doc, a).as_dict().ok())
                .and_then(|a| a.get(b"URI").ok())
                .and_then(|u| u.as_str().ok())
                .map(|u| count_terms(&String::from_utf8_lossy(u), terms))
                .unwrap_or(0)
        };
        let mut bookmarks = 0;
        for id in self.outline_objects() {
            if let Ok(d) = doc.get_dictionary(id) {
                bookmarks += entry(d, b"Title") + uri(d);
            }
        }
        let mut metadata = 0;
        if let Some(info) = doc
            .trailer
            .get(b"Info")
            .ok()
            .and_then(|i| deref(doc, i).as_dict().ok())
        {
            for (k, _) in info.iter() {
                metadata += entry(info, k);
            }
        }
        metadata += self.xmp_packet().map(|p| count_terms(&p, terms)).unwrap_or(0);
        let names = self
            .named_destinations()
            .iter()
            .map(|d| count_terms(&d.name, terms))
            .sum();
        let mut links = 0;
        let mut tags = 0;
        for o in doc.objects.values() {
            let Object::Dictionary(d) = o else { continue };
            if d.get(b"Subtype").ok().and_then(|s| s.as_name().ok()) == Some(b"Link") {
                links += entry(d, b"Contents") + uri(d);
            }
            if is_struct_elem(d) {
                for k in [b"Alt".as_slice(), b"ActualText", b"E", b"T"] {
                    tags += entry(d, k);
                }
            }
        }
        vec![
            (QuotePlace::Bookmarks, bookmarks),
            (QuotePlace::Metadata, metadata),
            (QuotePlace::Names, names),
            (QuotePlace::Links, links),
            (QuotePlace::Tags, tags),
        ]
    }

    /// Quotes in the page text, the comments and the form values (reported only).
    fn quoted_in_pages_and_fields(&self, terms: &[Term]) -> usize {
        let mut n = 0;
        for p in 0..self.page_count() {
            if let Some(layer) = self.text_layer(p) {
                n += count_terms(&layer.text, terms);
            }
            for a in self.annotations(p).unwrap_or_default() {
                if a.subtype != "Link" {
                    n += count_terms(&a.contents, terms);
                }
            }
        }
        for f in self.form_fields() {
            n += count_terms(&f.value, terms);
        }
        n
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn revisions_are_read_from_the_raw_trailer() {
        let one = b"%PDF-1.7\n1 0 obj << /Type /Catalog >> endobj\nxref\n0 2\n0000000000 65535 f \n0000000009 00000 n \ntrailer\n<< /Size 2 /Root 1 0 R >>\nstartxref\n45\n%%EOF\n";
        let at = one.windows(4).position(|w| w == b"xref").unwrap();
        let fixed = String::from_utf8_lossy(one).replace("startxref\n45", &format!("startxref\n{at}"));
        assert!(one_revision(fixed.as_bytes()));
        let two = fixed.replace("/Root 1 0 R >>", "/Root 1 0 R /Prev 9 >>");
        assert!(!one_revision(two.as_bytes()));
        // A file that merely holds the word elsewhere is not two revisions.
        let quoted = fixed.replace(
            "<< /Type /Catalog >>",
            "<< /Type /Catalog /Note (startxref /Prev) >>",
        );
        let at = quoted.find("xref\n0 2").unwrap();
        let quoted = quoted.replace(
            &format!("startxref\n{}", fixed.find("xref\n0 2").unwrap()),
            &format!("startxref\n{at}"),
        );
        assert!(one_revision(quoted.as_bytes()));
    }
}
