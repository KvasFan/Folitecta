//! Tagged PDF: the logical structure (tags) with its role map and class map,
//! the marked content of the pages and where it is drawn, automatic tagging,
//! tags made from an area of a page, and the reading order.
//!
//! Where content is drawn is measured with hayro: a device follows the marked
//! content sequences while the page is interpreted. Content that is not marked
//! yet is measured the same way after it is wrapped, in a copy of the
//! document, in probe sequences with their own identifiers.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

use hayro::hayro_interpret::font::{Glyph, GlyphRun};
use hayro::hayro_interpret::{
    BlendMode, ClipPath, Context, Device, DrawMode, DrawProps, Image, ImageDrawProps, InterpreterCache,
    InterpreterSettings, SoftMask, TransformExt, interpret_page,
};
use hayro::hayro_syntax::Pdf;
use hayro::kurbo::{BezPath, Point, Rect, Shape};
use lopdf::{Dictionary, Object, ObjectId, Stream, dictionary};

use crate::content_lex::{Op, name_of, operations};
use crate::document::Document;
use crate::error::{Error, Result};
use crate::util::{deref, dict_text};

/// Standard structure types.
pub const STANDARD_TYPES: &[&str] = &[
    "Document",
    "Part",
    "Art",
    "Sect",
    "Div",
    "BlockQuote",
    "Caption",
    "TOC",
    "TOCI",
    "Index",
    "NonStruct",
    "Private",
    "H",
    "H1",
    "H2",
    "H3",
    "H4",
    "H5",
    "H6",
    "P",
    "L",
    "LI",
    "Lbl",
    "LBody",
    "Table",
    "TR",
    "TH",
    "TD",
    "THead",
    "TBody",
    "TFoot",
    "Span",
    "Quote",
    "Note",
    "Reference",
    "BibEntry",
    "Code",
    "Link",
    "Annot",
    "Ruby",
    "RB",
    "RT",
    "RP",
    "Warichu",
    "WT",
    "WP",
    "Figure",
    "Formula",
    "Form",
];

/// Elements drawn inside a line of text: they belong to the block around them
/// in the reading order.
const INLINE: &[&str] = &[
    "Span",
    "Quote",
    "Note",
    "Reference",
    "BibEntry",
    "Code",
    "Link",
    "Annot",
    "Ruby",
    "RB",
    "RT",
    "RP",
    "Warichu",
    "WT",
    "WP",
    "Lbl",
];

/// Elements that only group others: never an item of the reading order by themselves.
const GROUPING: &[&str] = &[
    "Document",
    "Part",
    "Art",
    "Sect",
    "Div",
    "BlockQuote",
    "TOC",
    "TOCI",
    "Index",
    "NonStruct",
    "Private",
    "TR",
    "THead",
    "TBody",
    "TFoot",
    "LI",
    "LBody",
];

/// Marked content identifiers of probe sequences (above any real one).
const PROBE_BASE: i64 = 900_000_000;
/// Most elements read from a structure tree.
const MAX_ELEMENTS: usize = 200_000;
/// Highest marked content identifier kept in the parent tree.
const MAX_MCID: i64 = 1_000_000;

pub fn is_standard(tag: &str) -> bool {
    STANDARD_TYPES.contains(&tag)
}

/// Heading level of a standard type (`H1` gives 1, `H` gives 1).
pub fn heading_level(role: &str) -> Option<u8> {
    match role {
        "H" => Some(1),
        _ => role
            .strip_prefix('H')
            .and_then(|n| n.parse::<u8>().ok())
            .filter(|n| (1..=6).contains(n)),
    }
}

/// What an element of the structure contains, in order.
#[derive(Debug, Clone, PartialEq)]
pub enum StructKid {
    Elem(StructElem),
    /// A marked content sequence of a page.
    Content {
        page: usize,
        mcid: i64,
    },
    /// An annotation (link, form field, comment): its page, index and subtype.
    Annot {
        page: usize,
        index: usize,
        subtype: String,
    },
    /// Something else: content of an XObject, an unknown reference.
    Other(String),
}

/// An element of the structure tree. The root has the tag `StructTreeRoot`.
#[derive(Debug, Clone, PartialEq)]
pub struct StructElem {
    pub id: ObjectId,
    /// The tag as written (`/S`).
    pub tag: String,
    /// The standard type the tag maps to through the role map.
    pub role: String,
    pub title: String,
    pub alt: String,
    pub actual_text: String,
    pub lang: String,
    /// Page of the element (its own `/Pg` or the one of an element around it).
    pub page: Option<usize>,
    /// The attribute owners and keys (`/A`), as `Owner/Key` strings.
    pub attributes: Vec<String>,
    pub kids: Vec<StructKid>,
}

impl StructElem {
    pub fn is_root(&self) -> bool {
        self.tag == "StructTreeRoot"
    }

    /// Child elements.
    pub fn children(&self) -> impl Iterator<Item = &StructElem> {
        self.kids.iter().filter_map(|k| match k {
            StructKid::Elem(e) => Some(e),
            _ => None,
        })
    }

    /// Every element of the subtree (this one first), with its depth.
    pub fn walk(&self, f: &mut dyn FnMut(&StructElem, usize)) {
        fn go(e: &StructElem, depth: usize, f: &mut dyn FnMut(&StructElem, usize)) {
            f(e, depth);
            for c in e.children() {
                go(c, depth + 1, f);
            }
        }
        go(self, 0, f);
    }

    pub fn find(&self, id: ObjectId) -> Option<&StructElem> {
        if self.id == id {
            return Some(self);
        }
        self.children().find_map(|c| c.find(id))
    }

    /// The element whose kids include `id`.
    pub fn parent_of(&self, id: ObjectId) -> Option<&StructElem> {
        if self.children().any(|c| c.id == id) {
            return Some(self);
        }
        self.children().find_map(|c| c.parent_of(id))
    }

    /// Marked content of the subtree, in order.
    pub fn content(&self) -> Vec<(usize, i64)> {
        let mut out = Vec::new();
        self.walk(&mut |e, _| {
            for k in &e.kids {
                if let StructKid::Content { page, mcid } = k {
                    out.push((*page, *mcid));
                }
            }
        });
        out
    }

    /// Annotations of the subtree, in order.
    pub fn annotations(&self) -> Vec<(usize, usize)> {
        let mut out = Vec::new();
        self.walk(&mut |e, _| {
            for k in &e.kids {
                if let StructKid::Annot { page, index, .. } = k {
                    out.push((*page, *index));
                }
            }
        });
        out
    }

    /// The subtree has no content and no annotation.
    pub fn is_empty(&self) -> bool {
        self.content().is_empty() && self.annotations().is_empty()
    }

    /// First page with content of the subtree.
    pub fn first_page(&self) -> Option<usize> {
        self.content()
            .first()
            .map(|c| c.0)
            .or_else(|| self.annotations().first().map(|a| a.0))
            .or(self.page)
    }
}

/// Properties of an element that can be edited.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct StructProps {
    pub tag: String,
    pub title: String,
    pub alt: String,
    pub actual_text: String,
    pub lang: String,
}

/// Where a marked content sequence (or content outside them) is drawn.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MarkedItem {
    /// None for artifacts and for content outside marked content.
    pub mcid: Option<i64>,
    pub tag: String,
    pub artifact: bool,
    /// Bounds in display space (points, y down).
    pub rect: [f32; 4],
    pub text: String,
    pub has_text: bool,
    pub has_image: bool,
    pub has_path: bool,
    /// Average size of the text.
    pub font_size: f32,
}

/// Marked content of pages measured for a revision.
pub type MarkedCache = Option<(u64, HashMap<usize, Arc<Vec<MarkedItem>>>)>;

/// One element of the reading order of a page.
#[derive(Debug, Clone, PartialEq)]
pub struct OrderItem {
    pub id: ObjectId,
    pub tag: String,
    pub role: String,
    /// Bounds on the page (display space).
    pub rect: [f32; 4],
    pub text: String,
    /// Subtype of the first annotation of the item on the page (empty when none).
    pub annotation: String,
}

/// An item of the reading order being built: element, tag, role, bounds, texts.
type OrderAcc = (ObjectId, String, String, Option<[f32; 4]>, Vec<String>);

/// What automatic tagging made.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AutoTagReport {
    pub paragraphs: usize,
    pub headings: usize,
    pub figures: usize,
    pub artifacts: usize,
    pub annotations: usize,
}

// ----------------------------------------------------------------------
// Measuring marked content
// ----------------------------------------------------------------------

#[derive(Default)]
struct Acc {
    tag: String,
    rect: Option<Rect>,
    text: String,
    last_end: Option<(Point, f64)>,
    size_sum: f64,
    glyphs: usize,
    image: bool,
    path: bool,
}

impl Acc {
    fn add_rect(&mut self, r: Rect) {
        if !r.width().is_finite() || !r.height().is_finite() {
            return;
        }
        self.rect = Some(match self.rect {
            Some(a) => a.union(r),
            None => r,
        });
    }
}

/// Key of the content outside marked content and of artifacts.
const UNMARKED: i64 = i64::MIN;
const ARTIFACT: i64 = i64::MIN + 1;

#[derive(Default)]
struct McDevice {
    stack: Vec<(Vec<u8>, Option<i32>)>,
    order: Vec<i64>,
    items: HashMap<i64, Acc>,
    /// Positions of the last filled glyph run (fill and stroke text arrives twice).
    last_fill: Vec<(i64, i64)>,
}

impl McDevice {
    fn current(&mut self) -> &mut Acc {
        let (key, tag) = match self
            .stack
            .iter()
            .rev()
            .find_map(|(t, m)| m.map(|m| (m as i64, t.clone())))
        {
            Some((m, t)) => (m, t),
            None if self.stack.iter().any(|(t, _)| t == b"Artifact") => (ARTIFACT, b"Artifact".to_vec()),
            None => (UNMARKED, Vec::new()),
        };
        if !self.items.contains_key(&key) {
            self.order.push(key);
        }
        self.items.entry(key).or_insert_with(|| Acc {
            tag: String::from_utf8_lossy(&tag).to_string(),
            ..Acc::default()
        })
    }

    fn into_items(mut self) -> Vec<(i64, Acc)> {
        self.order
            .iter()
            .filter_map(|k| self.items.remove(k).map(|a| (*k, a)))
            .collect()
    }
}

impl<'a> Device<'a> for McDevice {
    fn draw_path(&mut self, path: &BezPath, props: DrawProps<'a>, _mode: &DrawMode) {
        let r = (props.transform * path.clone()).bounding_box();
        let acc = self.current();
        acc.path = true;
        acc.add_rect(r);
    }
    fn push_clip_path(&mut self, _clip_path: &ClipPath) {}
    fn push_transparency_group(
        &mut self,
        _opacity: f32,
        _mask: Option<SoftMask<'a>>,
        _blend_mode: BlendMode,
    ) {
    }
    fn draw_glyph_run(&mut self, run: &GlyphRun<'_, 'a>, props: DrawProps<'a>, mode: &DrawMode) {
        let glyphs = run.glyphs();
        let key: Vec<(i64, i64)> = glyphs
            .iter()
            .map(|g| {
                let o = props.transform * g.transform() * Point::ZERO;
                ((o.x * 10.0) as i64, (o.y * 10.0) as i64)
            })
            .collect();
        if matches!(mode, DrawMode::Stroke(_)) && key == self.last_fill {
            return;
        }
        if !matches!(mode, DrawMode::Stroke(_)) {
            self.last_fill = key;
        }
        for (i, g) in glyphs.iter().enumerate() {
            let t = props.transform * g.transform();
            let (adv, text) = match &**g {
                Glyph::Outline(o) => (o.advance_width().map(|a| a as f64), g.as_unicode()),
                Glyph::Type3(_) => (None, g.as_unicode()),
            };
            let adv = adv.filter(|a| *a > 0.0).unwrap_or_else(|| {
                glyphs
                    .get(i + 1)
                    .map(|n| (g.transform().inverse() * n.transform() * Point::ZERO).x)
                    .filter(|d| d.is_finite() && *d > 0.0 && *d < 3000.0)
                    .unwrap_or(500.0)
            });
            let text = match text {
                Some(hayro::hayro_interpret::hayro_cmap::BfString::Char(c)) => c.to_string(),
                Some(hayro::hayro_interpret::hayro_cmap::BfString::String(s)) => s,
                None => String::new(),
            };
            let origin = t * Point::ZERO;
            let size = ((t * Point::new(0.0, 1000.0)) - origin).hypot();
            if !size.is_finite() || size < 0.01 {
                continue;
            }
            let end = t * Point::new(adv, 0.0);
            let r = Rect::from_points(t * Point::new(0.0, 800.0), t * Point::new(adv, 800.0))
                .union_pt(t * Point::new(0.0, -200.0))
                .union_pt(t * Point::new(adv, -200.0));
            let acc = self.current();
            if let Some((last, last_size)) = acc.last_end {
                let s = last_size.max(size);
                let dy = (origin.y - last.y).abs();
                let gap = origin.x - last.x;
                if (dy > 0.5 * s || gap > 0.25 * s || gap < -1.5 * s)
                    && !acc.text.ends_with(' ')
                    && !text.starts_with(char::is_whitespace)
                {
                    acc.text.push(' ');
                }
            }
            for c in text.chars() {
                acc.text
                    .push(if c.is_control() || c == '\u{a0}' { ' ' } else { c });
            }
            acc.last_end = Some((end, size));
            acc.size_sum += size;
            acc.glyphs += 1;
            acc.add_rect(r);
        }
    }
    fn draw_image(&mut self, _image: Image<'a, '_>, props: ImageDrawProps<'a>) {
        let unit = Rect::new(0.0, 0.0, 1.0, 1.0).to_path(0.1);
        let r = (props.transform * unit).bounding_box();
        let acc = self.current();
        acc.image = true;
        acc.add_rect(r);
    }
    fn pop_clip(&mut self) {}
    fn pop_transparency_group(&mut self) {}
    fn begin_marked_content(&mut self, tag: &[u8], mcid: Option<i32>) {
        self.stack.push((tag.to_vec(), mcid));
    }
    fn end_marked_content(&mut self) {
        self.stack.pop();
    }
}

fn to_item(key: i64, a: Acc) -> MarkedItem {
    let rect = a
        .rect
        .map(|r| [r.x0 as f32, r.y0 as f32, r.x1 as f32, r.y1 as f32])
        .unwrap_or_default();
    MarkedItem {
        mcid: (key != UNMARKED && key != ARTIFACT).then_some(key),
        tag: a.tag,
        artifact: key == ARTIFACT,
        rect,
        text: a.text.split_whitespace().collect::<Vec<_>>().join(" "),
        has_text: a.glyphs > 0,
        has_image: a.image,
        has_path: a.path,
        font_size: if a.glyphs > 0 {
            (a.size_sum / a.glyphs as f64) as f32
        } else {
            0.0
        },
    }
}

/// Measure the marked content of some pages of a serialized document.
fn measure(bytes: Arc<Vec<u8>>, pages: &[usize]) -> Result<HashMap<usize, Vec<MarkedItem>>> {
    let pdf = Pdf::new(bytes).map_err(|e| Error::Render(format!("cannot parse document: {e:?}")))?;
    let all = pdf.pages();
    let mut out = HashMap::new();
    for &p in pages {
        let Some(page) = all.get(p) else { continue };
        let (w, h) = page.render_dimensions();
        let cache = InterpreterCache::new();
        let settings = InterpreterSettings {
            render_annotations: false,
            ..InterpreterSettings::default()
        };
        let mut ctx = Context::new(
            page.initial_transform(true).to_kurbo(),
            Rect::new(0.0, 0.0, w as f64, h as f64),
            &cache,
            page.xref(),
            settings,
        );
        let mut dev = McDevice::default();
        let ok = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            interpret_page(page, &mut ctx, &mut dev);
        }))
        .is_ok();
        let items = if ok {
            dev.into_items().into_iter().map(|(k, a)| to_item(k, a)).collect()
        } else {
            Vec::new()
        };
        out.insert(p, items);
    }
    Ok(out)
}

// ----------------------------------------------------------------------
// Content streams
// ----------------------------------------------------------------------

/// The MCID of a `BDC` with an inline property list.
fn mcid_of(data: &[u8], op: &Op<'_>) -> Option<i64> {
    if op.operator != b"BDC" || op.operands.len() != 2 {
        return None;
    }
    let (a, b) = op.operands[1];
    let props = data.get(a..b)?;
    let pos = props.windows(5).position(|w| w == b"/MCID")?;
    let rest = &props[pos + 5..];
    let digits: Vec<u8> = rest
        .iter()
        .skip_while(|c| c.is_ascii_whitespace())
        .take_while(|c| c.is_ascii_digit())
        .copied()
        .collect();
    std::str::from_utf8(&digits).ok()?.parse().ok()
}

fn is_artifact(data: &[u8], op: &Op<'_>) -> bool {
    matches!(op.operator, b"BDC" | b"BMC")
        && op.operands.first().and_then(|r| name_of(data, *r)).as_deref() == Some(b"Artifact")
}

/// A name written for a content stream or a dictionary.
fn pdf_name(tag: &str) -> String {
    let mut out = String::from("/");
    for b in tag.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.') {
            out.push(b as char);
        } else {
            out.push_str(&format!("#{b:02X}"));
        }
    }
    out
}

const PAINT: &[&[u8]] = &[b"S", b"s", b"f", b"F", b"f*", b"B", b"B*", b"b", b"b*"];
const PATH_BUILD: &[&[u8]] = &[b"m", b"l", b"c", b"v", b"y", b"h", b"re"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UnitKind {
    Text,
    XObject,
    Image,
    Path,
    Shading,
}

/// Operations that draw something outside tagged content and artifacts: one
/// text showing operation, an XObject, an inline image, a painted path.
#[derive(Debug, Clone, Copy)]
struct Unit {
    first: usize,
    last: usize,
    kind: UnitKind,
    /// Text units of one segment can share a marked content sequence.
    segment: usize,
    /// Position among the units.
    seq: usize,
    /// Where the first operation starts in the joined content.
    offset: usize,
}

fn content_units(data: &[u8], ops: &[Op<'_>]) -> Vec<Unit> {
    let mut out = Vec::new();
    let mut stack: Vec<bool> = Vec::new();
    let mut segment = 0usize;
    let mut in_text = false;
    let mut path_start: Option<usize> = None;
    for (i, op) in ops.iter().enumerate() {
        let covered = stack.iter().any(|c| *c);
        let mut unit = |kind, first: usize| {
            let seq = out.len();
            out.push(Unit {
                first,
                last: i,
                kind,
                segment,
                seq,
                offset: ops[first].start,
            })
        };
        match op.operator {
            b"BDC" | b"BMC" => {
                stack.push(mcid_of(data, op).is_some() || is_artifact(data, op));
                segment += 1;
            }
            b"EMC" => {
                stack.pop();
                segment += 1;
            }
            b"BT" => {
                in_text = true;
                segment += 1;
            }
            b"ET" => {
                in_text = false;
                segment += 1;
            }
            b"Tj" | b"TJ" | b"'" | b"\"" if in_text => {
                if !covered {
                    unit(UnitKind::Text, i);
                }
            }
            _ if in_text => {}
            o if PATH_BUILD.contains(&o) => {
                if path_start.is_none() {
                    path_start = Some(i);
                }
            }
            o if PAINT.contains(&o) => {
                if let Some(s) = path_start.take() {
                    if !covered {
                        unit(UnitKind::Path, s);
                    }
                }
                segment += 1;
            }
            b"n" => {
                path_start = None;
            }
            b"Do" | b"BI" | b"sh" => {
                if !covered {
                    let kind = match op.operator {
                        b"Do" => UnitKind::XObject,
                        b"BI" => UnitKind::Image,
                        _ => UnitKind::Shading,
                    };
                    unit(kind, i);
                }
                segment += 1;
            }
            _ => {}
        }
    }
    out
}

/// What wraps a run of operations.
#[derive(Debug, Clone)]
enum Wrap {
    Mcid(String, i64),
    Artifact,
}

/// Output of a rewrite: bytes that go where an operation of the source was
/// (the start of the operation in the joined content), and whether they
/// differ from the source.
type Piece = (usize, Vec<u8>, bool);

#[cfg(test)]
fn join(pieces: &[Piece]) -> Vec<u8> {
    pieces.iter().flat_map(|p| p.1.iter().copied()).collect()
}

/// Copy the operations with marked content sequences around some runs
/// (`(first op, last op, wrap)`, in order and without overlaps).
fn write_wrapped(data: &[u8], ops: &[Op<'_>], wraps: &[(usize, usize, Wrap)]) -> Vec<Piece> {
    let opens: HashMap<usize, &Wrap> = wraps.iter().map(|(f, _, w)| (*f, w)).collect();
    let closes: HashMap<usize, usize> = wraps.iter().fold(HashMap::new(), |mut m, (_, l, _)| {
        *m.entry(*l).or_insert(0) += 1;
        m
    });
    let mut out = Vec::with_capacity(ops.len() + wraps.len() * 2);
    for (i, op) in ops.iter().enumerate() {
        if let Some(w) = opens.get(&i) {
            let open = match w {
                Wrap::Mcid(tag, n) => format!("{} <</MCID {n}>> BDC\n", pdf_name(tag)).into_bytes(),
                Wrap::Artifact => b"/Artifact BMC\n".to_vec(),
            };
            out.push((op.start, open, true));
        }
        let mut raw = data[op.start..op.end].to_vec();
        raw.push(b'\n');
        out.push((op.start, raw, false));
        for _ in 0..closes.get(&i).copied().unwrap_or(0) {
            out.push((op.start, b"EMC\n".to_vec(), true));
        }
    }
    out
}

/// What happens to a marked content sequence with an MCID.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum McChange {
    Keep,
    /// Becomes an artifact.
    Artifact,
    /// The marker goes away, the content stays.
    Strip,
}

/// Rewrite the markers of marked content with an MCID; None when nothing changed.
fn rewrite_mcids(data: &[u8], f: &dyn Fn(i64) -> McChange) -> Option<Vec<Piece>> {
    let ops = operations(data);
    let mut out = Vec::with_capacity(ops.len());
    let mut stack: Vec<bool> = Vec::new();
    let mut changed = false;
    for op in &ops {
        let mut raw = data[op.start..op.end].to_vec();
        raw.push(b'\n');
        match op.operator {
            b"BDC" | b"BMC" => match mcid_of(data, op).map(f) {
                Some(McChange::Artifact) => {
                    out.push((op.start, b"/Artifact BMC\n".to_vec(), true));
                    stack.push(false);
                    changed = true;
                }
                Some(McChange::Strip) => {
                    out.push((op.start, Vec::new(), true));
                    stack.push(true);
                    changed = true;
                }
                _ => {
                    out.push((op.start, raw, false));
                    stack.push(false);
                }
            },
            b"EMC" => {
                if stack.pop() == Some(true) {
                    out.push((op.start, Vec::new(), true));
                } else {
                    out.push((op.start, raw, false));
                }
            }
            _ => out.push((op.start, raw, false)),
        }
    }
    changed.then_some(out)
}

fn max_mcid(data: &[u8]) -> Option<i64> {
    operations(data).iter().filter_map(|op| mcid_of(data, op)).max()
}

/// The content streams of a page, decoded and joined, with where each starts.
#[derive(Debug, Clone, Default)]
struct PageStreams {
    ids: Vec<ObjectId>,
    starts: Vec<usize>,
    /// Streams of running headers, footers, page numbers and watermarks.
    pagination: Vec<bool>,
    data: Vec<u8>,
}

impl PageStreams {
    fn read(doc: &lopdf::Document, page_id: ObjectId) -> Self {
        let mut ps = PageStreams::default();
        for id in doc.get_page_contents(page_id) {
            ps.ids.push(id);
            ps.starts.push(ps.data.len());
            match doc.get_object(id).and_then(Object::as_stream) {
                Ok(s) => {
                    ps.data
                        .extend(s.decompressed_content().unwrap_or_else(|_| s.content.clone()));
                    let mark = s.dict.get(b"FolitectaMark").ok().and_then(|m| m.as_name().ok());
                    ps.pagination.push(matches!(
                        mark,
                        Some(b"Watermark" | b"HeaderFooter" | b"PageNumber")
                    ));
                }
                Err(_) => ps.pagination.push(false),
            }
            ps.data.push(b'\n');
        }
        ps
    }

    fn stream_of(&self, offset: usize) -> usize {
        self.starts.partition_point(|s| *s <= offset).saturating_sub(1)
    }

    fn is_pagination(&self, offset: usize) -> bool {
        self.pagination
            .get(self.stream_of(offset))
            .copied()
            .unwrap_or(false)
    }
}

/// Write the pieces of a rewrite into the streams they come from; streams
/// without a change stay as they are (marks added by Folitecta keep their
/// own streams).
fn write_pieces(
    doc: &mut lopdf::Document,
    page_id: ObjectId,
    ps: &PageStreams,
    pieces: Vec<Piece>,
) -> Result<()> {
    let n = ps.ids.len();
    if n == 0 {
        let data: Vec<u8> = pieces.into_iter().flat_map(|p| p.1).collect();
        return set_page_content(doc, page_id, data);
    }
    let mut outs = vec![Vec::new(); n];
    let mut changed = vec![false; n];
    for (off, bytes, ch) in pieces {
        let k = ps.stream_of(off).min(n - 1);
        outs[k].extend_from_slice(&bytes);
        changed[k] |= ch;
    }
    for k in 0..n {
        if changed[k] {
            let id = ps.ids[k];
            let mut dict = doc.get_object(id)?.as_stream()?.dict.clone();
            for key in [b"Filter".as_slice(), b"DecodeParms", b"Length"] {
                dict.remove(key);
            }
            let mut s = Stream::new(dict, std::mem::take(&mut outs[k]));
            let _ = s.compress();
            doc.objects.insert(id, Object::Stream(s));
        }
    }
    Ok(())
}

/// Replace the content of a page with one new stream.
fn set_page_content(doc: &mut lopdf::Document, page_id: ObjectId, data: Vec<u8>) -> Result<()> {
    let mut stream = Stream::new(Dictionary::new(), data);
    let _ = stream.compress();
    let sid = doc.add_object(stream);
    doc.get_dictionary_mut(page_id)?
        .set("Contents", Object::Reference(sid));
    Ok(())
}

fn intersects(a: [f32; 4], b: [f32; 4]) -> bool {
    a[0] < b[2] && b[0] < a[2] && a[1] < b[3] && b[1] < a[3]
}

fn union(a: Option<[f32; 4]>, b: [f32; 4]) -> Option<[f32; 4]> {
    if b[2] <= b[0] && b[3] <= b[1] {
        return a;
    }
    Some(match a {
        Some(a) => [a[0].min(b[0]), a[1].min(b[1]), a[2].max(b[2]), a[3].max(b[3])],
        None => b,
    })
}

/// A unit measured through a probe.
#[derive(Debug, Clone)]
struct Probed {
    unit: Unit,
    item: Option<MarkedItem>,
}

// ----------------------------------------------------------------------
// Reading the structure tree
// ----------------------------------------------------------------------

struct Reader<'a> {
    doc: &'a lopdf::Document,
    pages: HashMap<ObjectId, usize>,
    annots: HashMap<ObjectId, (usize, usize, String)>,
    roles: HashMap<String, String>,
    seen: HashSet<ObjectId>,
}

fn name_value(d: &Dictionary, key: &[u8]) -> Option<String> {
    d.get(key)
        .ok()
        .and_then(|o| o.as_name().ok())
        .map(|n| String::from_utf8_lossy(n).to_string())
}

fn role_of(roles: &HashMap<String, String>, tag: &str) -> String {
    let mut t = tag.to_string();
    for _ in 0..16 {
        if is_standard(&t) {
            return t;
        }
        match roles.get(&t) {
            Some(n) if *n != t => t = n.clone(),
            _ => break,
        }
    }
    if is_standard(&t) { t } else { tag.to_string() }
}

impl Reader<'_> {
    fn page_of(&self, d: &Dictionary) -> Option<usize> {
        d.get(b"Pg")
            .ok()
            .and_then(|p| p.as_reference().ok())
            .and_then(|p| self.pages.get(&p).copied())
    }

    fn elem(&mut self, id: ObjectId, page: Option<usize>, depth: usize) -> Option<StructElem> {
        if depth > 256 || self.seen.len() > MAX_ELEMENTS || !self.seen.insert(id) {
            return None;
        }
        let doc = self.doc;
        let d = doc.get_dictionary(id).ok()?;
        let tag = name_value(d, b"S").unwrap_or_default();
        let pg = self.page_of(d).or(page);
        let text = |key: &[u8]| dict_text(doc, d, key).unwrap_or_default();
        let mut attributes = Vec::new();
        if let Ok(a) = d.get(b"A") {
            let list: Vec<&Object> = match deref(doc, a) {
                Object::Array(arr) => arr.iter().collect(),
                o => vec![o],
            };
            for o in list {
                if let Ok(ad) = deref(doc, o).as_dict() {
                    let owner = name_value(ad, b"O").unwrap_or_default();
                    for (k, _) in ad.iter() {
                        if k != b"O" {
                            attributes.push(format!("{owner}/{}", String::from_utf8_lossy(k)));
                        }
                    }
                }
            }
        }
        let mut e = StructElem {
            id,
            role: role_of(&self.roles, &tag),
            tag,
            title: text(b"T"),
            alt: text(b"Alt"),
            actual_text: text(b"ActualText"),
            lang: text(b"Lang"),
            page: pg,
            attributes,
            kids: Vec::new(),
        };
        if let Ok(k) = d.get(b"K") {
            for o in kid_objects(doc, k) {
                let kid = self.kid(&o, pg, depth);
                e.kids.push(kid);
            }
        }
        Some(e)
    }

    /// One kid: always one entry for each object of the `/K` list.
    fn kid(&mut self, o: &Object, pg: Option<usize>, depth: usize) -> StructKid {
        let doc = self.doc;
        let mcid = |n: i64| match pg {
            Some(p) => StructKid::Content { page: p, mcid: n },
            None => StructKid::Other(format!("MCID {n}")),
        };
        match o {
            Object::Integer(n) => mcid(*n),
            Object::Reference(r) => match doc.get_object(*r) {
                Ok(Object::Integer(n)) => mcid(*n),
                Ok(Object::Dictionary(d)) => {
                    if d.has(b"S") && !d.has_type(b"MCR") && !d.has_type(b"OBJR") {
                        match self.elem(*r, pg, depth + 1) {
                            Some(e) => StructKid::Elem(e),
                            None => StructKid::Other(name_value(d, b"S").unwrap_or_default()),
                        }
                    } else {
                        self.dict_kid(d, pg)
                    }
                }
                _ => StructKid::Other(String::new()),
            },
            Object::Dictionary(d) => self.dict_kid(d, pg),
            _ => StructKid::Other(String::new()),
        }
    }

    fn dict_kid(&self, d: &Dictionary, pg: Option<usize>) -> StructKid {
        if d.has_type(b"MCR") {
            let page = self.page_of(d).or(pg);
            let mcid = d.get(b"MCID").ok().and_then(|m| m.as_i64().ok());
            return match (page, mcid, d.has(b"Stm")) {
                (Some(page), Some(mcid), false) => StructKid::Content { page, mcid },
                (_, Some(m), _) => StructKid::Other(format!("MCID {m}")),
                _ => StructKid::Other(String::new()),
            };
        }
        if d.has_type(b"OBJR") {
            if let Some((page, index, subtype)) = d
                .get(b"Obj")
                .ok()
                .and_then(|o| o.as_reference().ok())
                .and_then(|o| self.annots.get(&o))
            {
                return StructKid::Annot {
                    page: *page,
                    index: *index,
                    subtype: subtype.clone(),
                };
            }
            return StructKid::Other("OBJR".into());
        }
        StructKid::Other(name_value(d, b"S").unwrap_or_default())
    }
}

/// The objects of a `/K` entry: one object, an array, or a reference to an array.
fn kid_objects(doc: &lopdf::Document, k: &Object) -> Vec<Object> {
    match k {
        Object::Array(a) => a.clone(),
        Object::Reference(r) => match doc.get_object(*r) {
            Ok(Object::Array(a)) => a.clone(),
            Ok(_) => vec![k.clone()],
            Err(_) => Vec::new(),
        },
        Object::Null => Vec::new(),
        o => vec![o.clone()],
    }
}

/// Attributes text: `Key=Value` pairs separated by spaces, values as numbers or names.
fn attrs_to_text(doc: &lopdf::Document, o: &Object) -> String {
    fn value(doc: &lopdf::Document, v: &Object) -> String {
        match deref(doc, v) {
            Object::Name(n) => String::from_utf8_lossy(n).to_string(),
            Object::Integer(i) => i.to_string(),
            Object::Real(r) => r.to_string(),
            Object::Boolean(b) => b.to_string(),
            Object::Array(a) => format!(
                "[{}]",
                a.iter().map(|x| value(doc, x)).collect::<Vec<_>>().join(" ")
            ),
            Object::String(..) => crate::util::text_of(doc, v).unwrap_or_default(),
            _ => String::new(),
        }
    }
    let dicts: Vec<&Object> = match deref(doc, o) {
        Object::Array(a) => a.iter().collect(),
        other => vec![other],
    };
    dicts
        .iter()
        .filter_map(|d| deref(doc, d).as_dict().ok())
        .map(|d| {
            d.iter()
                .map(|(k, v)| format!("{}={}", String::from_utf8_lossy(k), value(doc, v)))
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect::<Vec<_>>()
        .join("; ")
}

fn parse_value(v: &str) -> Object {
    let v = v.trim();
    if let Some(inner) = v.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
        return Object::Array(inner.split_whitespace().map(parse_value).collect());
    }
    if let Ok(i) = v.parse::<i64>() {
        return Object::Integer(i);
    }
    if let Ok(f) = v.parse::<f32>() {
        return Object::Real(f);
    }
    match v {
        "true" => Object::Boolean(true),
        "false" => Object::Boolean(false),
        _ => Object::Name(v.trim_start_matches('/').as_bytes().to_vec()),
    }
}

/// Words separated by spaces, `[...]` kept together.
fn attr_words(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut depth = 0usize;
    for c in text.chars() {
        match c {
            '[' => depth += 1,
            ']' => depth = depth.saturating_sub(1),
            _ => {}
        }
        if c.is_whitespace() && depth == 0 {
            if !cur.is_empty() {
                out.push(std::mem::take(&mut cur));
            }
        } else {
            cur.push(c);
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

fn text_to_attrs(text: &str) -> Object {
    let dicts: Vec<Object> = text
        .split(';')
        .filter(|s| !s.trim().is_empty())
        .map(|part| {
            let mut d = Dictionary::new();
            for pair in attr_words(part) {
                if let Some((k, v)) = pair.split_once('=') {
                    d.set(k.trim_start_matches('/').as_bytes().to_vec(), parse_value(v));
                }
            }
            Object::Dictionary(d)
        })
        .collect();
    if dicts.len() == 1 {
        dicts.into_iter().next().unwrap_or(Object::Null)
    } else {
        Object::Array(dicts)
    }
}

/// Short text of an element for lists (first words).
pub fn excerpt(text: &str, max: usize) -> String {
    let t = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if t.chars().count() <= max {
        t
    } else {
        format!("{}…", t.chars().take(max).collect::<String>().trim_end())
    }
}

impl Document {
    // ------------------------------------------------------------------
    // Reading
    // ------------------------------------------------------------------

    /// The document is marked as tagged (`/MarkInfo /Marked true`).
    pub fn is_tagged(&self) -> bool {
        self.info().tagged
    }

    /// The document has a structure tree.
    pub fn has_structure(&self) -> bool {
        self.inner
            .catalog()
            .map(|c| c.has(b"StructTreeRoot"))
            .unwrap_or(false)
    }

    fn struct_root_object(&self) -> Option<(Option<ObjectId>, &Dictionary)> {
        let c = self.inner.catalog().ok()?;
        match c.get(b"StructTreeRoot").ok()? {
            Object::Reference(r) => self.inner.get_dictionary(*r).ok().map(|d| (Some(*r), d)),
            Object::Dictionary(d) => Some((None, d)),
            _ => None,
        }
    }

    fn roles(&self) -> HashMap<String, String> {
        self.role_map().into_iter().collect()
    }

    /// The role map: tags and the types they stand for.
    pub fn role_map(&self) -> Vec<(String, String)> {
        let Some((_, root)) = self.struct_root_object() else {
            return Vec::new();
        };
        let Some(map) = root
            .get(b"RoleMap")
            .ok()
            .and_then(|m| deref(&self.inner, m).as_dict().ok())
        else {
            return Vec::new();
        };
        map.iter()
            .filter_map(|(k, v)| {
                deref(&self.inner, v).as_name().ok().map(|n| {
                    (
                        String::from_utf8_lossy(k).to_string(),
                        String::from_utf8_lossy(n).to_string(),
                    )
                })
            })
            .collect()
    }

    /// The class map: class names and their attributes as `Key=Value` text.
    pub fn class_map(&self) -> Vec<(String, String)> {
        let Some((_, root)) = self.struct_root_object() else {
            return Vec::new();
        };
        let Some(map) = root
            .get(b"ClassMap")
            .ok()
            .and_then(|m| deref(&self.inner, m).as_dict().ok())
        else {
            return Vec::new();
        };
        map.iter()
            .map(|(k, v)| {
                (
                    String::from_utf8_lossy(k).to_string(),
                    attrs_to_text(&self.inner, v),
                )
            })
            .collect()
    }

    /// The structure tree, its root with the tag `StructTreeRoot`.
    pub fn structure(&self) -> Option<StructElem> {
        let (root_id, root) = self.struct_root_object()?;
        let doc = &self.inner;
        let page_ids = self.page_ids();
        let mut annots = HashMap::new();
        for (p, pid) in page_ids.iter().enumerate() {
            if let Ok(pd) = doc.get_dictionary(*pid) {
                for (i, (aid, d)) in crate::annots::annot_refs(doc, pd).into_iter().enumerate() {
                    if let Some(aid) = aid {
                        annots.insert(aid, (p, i, name_value(&d, b"Subtype").unwrap_or_default()));
                    }
                }
            }
        }
        let mut reader = Reader {
            doc,
            pages: page_ids.iter().enumerate().map(|(i, id)| (*id, i)).collect(),
            annots,
            roles: self.roles(),
            seen: HashSet::new(),
        };
        let id = root_id.unwrap_or((0, 0));
        reader.seen.insert(id);
        let mut e = StructElem {
            id,
            tag: "StructTreeRoot".into(),
            role: String::new(),
            title: String::new(),
            alt: String::new(),
            actual_text: String::new(),
            lang: String::new(),
            page: None,
            attributes: Vec::new(),
            kids: Vec::new(),
        };
        if let Ok(k) = root.get(b"K") {
            for o in kid_objects(doc, k) {
                let kid = reader.kid(&o, None, 0);
                e.kids.push(kid);
            }
        }
        Some(e)
    }

    /// Marked content of a page and where it is drawn (cached per revision).
    pub fn marked_content(&self, page: usize) -> Arc<Vec<MarkedItem>> {
        self.marked_content_pages(&[page])
            .remove(&page)
            .unwrap_or_default()
    }

    /// Marked content of several pages.
    pub fn marked_content_pages(&self, pages: &[usize]) -> HashMap<usize, Arc<Vec<MarkedItem>>> {
        let mut cache = self.marked_cache.lock().unwrap_or_else(|e| e.into_inner());
        if cache.as_ref().map(|(rev, _)| *rev) != Some(self.revision()) {
            *cache = Some((self.revision(), HashMap::new()));
        }
        let Some((_, map)) = cache.as_mut() else {
            return HashMap::new();
        };
        let missing: Vec<usize> = pages.iter().copied().filter(|p| !map.contains_key(p)).collect();
        if !missing.is_empty() {
            if let Ok(found) = measure(self.bytes(), &missing) {
                for (p, items) in found {
                    map.insert(p, Arc::new(items));
                }
            }
        }
        pages
            .iter()
            .filter_map(|p| map.get(p).map(|items| (*p, items.clone())))
            .collect()
    }

    /// Text of an element: its actual text, or the text of its content.
    pub fn struct_text(&self, elem: &StructElem) -> String {
        if !elem.actual_text.trim().is_empty() {
            return elem.actual_text.trim().to_string();
        }
        let content = elem.content();
        let pages: Vec<usize> = content
            .iter()
            .map(|c| c.0)
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect();
        let marked = self.marked_content_pages(&pages);
        let mut parts = Vec::new();
        for (p, m) in content {
            if let Some(item) = marked
                .get(&p)
                .and_then(|items| items.iter().find(|i| i.mcid == Some(m)))
            {
                if !item.text.is_empty() {
                    parts.push(item.text.clone());
                }
            }
        }
        parts.join(" ")
    }

    /// Bounds of an element's content on a page (display space).
    pub fn struct_rect(&self, elem: &StructElem, page: usize) -> Option<[f32; 4]> {
        let marked = self.marked_content(page);
        let mut rect = None;
        for (p, m) in elem.content() {
            if p != page {
                continue;
            }
            if let Some(item) = marked.iter().find(|i| i.mcid == Some(m)) {
                rect = union(rect, item.rect);
            }
        }
        if let Ok(annots) = self.annotations(page) {
            let size = self.page_size(page).ok();
            for (p, i) in elem.annotations() {
                if p == page {
                    if let (Some(a), Some(size)) = (annots.get(i), size.as_ref()) {
                        rect = union(rect, crate::marks::pdf_rect_to_display(size, a.rect));
                    }
                }
            }
        }
        rect
    }

    /// The element that owns a marked content sequence.
    pub fn struct_for_content(&self, page: usize, mcid: i64) -> Option<ObjectId> {
        let tree = self.structure()?;
        let mut found = None;
        tree.walk(&mut |e, _| {
            if found.is_none()
                && e.kids
                    .iter()
                    .any(|k| matches!(k, StructKid::Content { page: p, mcid: m } if *p == page && *m == mcid))
            {
                found = Some(e.id);
            }
        });
        found
    }

    /// The element that owns an annotation.
    pub fn struct_for_annotation(&self, page: usize, index: usize) -> Option<ObjectId> {
        let tree = self.structure()?;
        let mut found = None;
        tree.walk(&mut |e, _| {
            if found.is_none()
                && e.kids.iter().any(
                    |k| matches!(k, StructKid::Annot { page: p, index: i, .. } if *p == page && *i == index),
                )
            {
                found = Some(e.id);
            }
        });
        found
    }

    /// The deepest element with content inside an area of a page.
    pub fn struct_at(&self, page: usize, rect: [f32; 4]) -> Option<ObjectId> {
        let marked = self.marked_content(page);
        let best = marked
            .iter()
            .filter(|i| i.mcid.is_some() && intersects(i.rect, rect))
            .min_by(|a, b| {
                let area = |r: [f32; 4]| (r[2] - r[0]) * (r[3] - r[1]);
                area(a.rect).total_cmp(&area(b.rect))
            })?;
        self.struct_for_content(page, best.mcid?)
    }

    // ------------------------------------------------------------------
    // Reading order
    // ------------------------------------------------------------------

    /// Elements of a page in reading order: blocks with their content, tables,
    /// figures and lists as a whole (cells one by one with `table_cells`).
    pub fn reading_order(&self, page: usize, table_cells: bool) -> Vec<OrderItem> {
        let Some(tree) = self.structure() else {
            return Vec::new();
        };
        let marked = self.marked_content(page);
        let rect_of = |m: i64| marked.iter().find(|i| i.mcid == Some(m));
        let mut out: Vec<OrderAcc> = Vec::new();
        fn has_page(e: &StructElem, page: usize) -> bool {
            e.content().iter().any(|c| c.0 == page) || e.annotations().iter().any(|a| a.0 == page)
        }
        fn visit(
            e: &StructElem,
            page: usize,
            table_cells: bool,
            current: Option<usize>,
            out: &mut Vec<OrderAcc>,
            rect_of: &dyn Fn(i64) -> Option<([f32; 4], String)>,
        ) {
            let mut current = current;
            if current.is_none() && !e.is_root() && has_page(e, page) {
                let role = e.role.as_str();
                let grouping = GROUPING.contains(&role) || (table_cells && role == "Table");
                let unit = matches!(role, "Figure" | "Formula" | "L") || (!table_cells && role == "Table");
                let direct = e.kids.iter().any(|k| {
                    matches!(k, StructKid::Content { page: p, .. } | StructKid::Annot { page: p, .. } if *p == page)
                });
                if unit || direct || (!grouping && !INLINE.contains(&role)) {
                    out.push((e.id, e.tag.clone(), e.role.clone(), None, Vec::new()));
                    current = Some(out.len() - 1);
                }
            }
            if let Some(c) = current {
                for k in &e.kids {
                    if let StructKid::Content { page: p, mcid } = k {
                        if *p == page {
                            if let Some((r, t)) = rect_of(*mcid) {
                                out[c].3 = union(out[c].3, r);
                                if !t.is_empty() {
                                    out[c].4.push(t);
                                }
                            }
                        }
                    }
                }
            }
            for k in &e.kids {
                if let StructKid::Elem(child) = k {
                    visit(child, page, table_cells, current, out, rect_of);
                }
            }
        }
        let lookup = |m: i64| rect_of(m).map(|i| (i.rect, i.text.clone()));
        visit(&tree, page, table_cells, None, &mut out, &lookup);
        // Annotation areas.
        let mut subtypes: HashMap<ObjectId, String> = HashMap::new();
        if let (Ok(annots), Ok(size)) = (self.annotations(page), self.page_size(page)) {
            for item in out.iter_mut() {
                if let Some(e) = tree.find(item.0) {
                    for (p, i) in e.annotations() {
                        if p == page {
                            if let Some(a) = annots.get(i) {
                                item.3 = union(item.3, crate::marks::pdf_rect_to_display(&size, a.rect));
                                subtypes.entry(item.0).or_insert_with(|| a.subtype.clone());
                            }
                        }
                    }
                }
            }
        }
        out.into_iter()
            .map(|(id, tag, role, rect, texts)| OrderItem {
                id,
                tag,
                role,
                rect: rect.unwrap_or_default(),
                text: excerpt(&texts.join(" "), 120),
                annotation: subtypes.remove(&id).unwrap_or_default(),
            })
            .collect()
    }

    // ------------------------------------------------------------------
    // Editing helpers
    // ------------------------------------------------------------------

    fn catalog_id(&self) -> Result<ObjectId> {
        Ok(self.inner.trailer.get(b"Root")?.as_reference()?)
    }

    /// Id of the structure root, made indirect when it was written in the catalog.
    fn struct_root_id(&mut self) -> Option<ObjectId> {
        let cat = self.catalog_id().ok()?;
        let obj = self
            .inner
            .get_dictionary(cat)
            .ok()?
            .get(b"StructTreeRoot")
            .ok()?
            .clone();
        match obj {
            Object::Reference(r) => Some(r),
            Object::Dictionary(d) => {
                let id = self.inner.add_object(Object::Dictionary(d));
                self.inner
                    .get_dictionary_mut(cat)
                    .ok()?
                    .set("StructTreeRoot", Object::Reference(id));
                Some(id)
            }
            _ => None,
        }
    }

    fn set_marked(&mut self, on: bool) -> Result<()> {
        let cat = self.catalog_id()?;
        let c = self.inner.get_dictionary_mut(cat)?;
        let mut mi = match c.get(b"MarkInfo") {
            Ok(Object::Dictionary(d)) => d.clone(),
            _ => Dictionary::new(),
        };
        mi.set("Marked", Object::Boolean(on));
        c.set("MarkInfo", Object::Dictionary(mi));
        Ok(())
    }

    /// The structure root, created when missing (the document becomes tagged).
    fn ensure_struct_root(&mut self) -> Result<ObjectId> {
        if let Some(id) = self.struct_root_id() {
            return Ok(id);
        }
        let pt = self
            .inner
            .add_object(dictionary! { "Nums" => Vec::<Object>::new() });
        let root = self.inner.add_object(dictionary! {
            "Type" => "StructTreeRoot",
            "K" => Vec::<Object>::new(),
            "ParentTree" => pt,
            "ParentTreeNextKey" => 0,
        });
        let cat = self.catalog_id()?;
        self.inner
            .get_dictionary_mut(cat)?
            .set("StructTreeRoot", Object::Reference(root));
        self.set_marked(true)?;
        Ok(root)
    }

    fn kid_list(&self, parent: ObjectId) -> Vec<Object> {
        self.inner
            .get_dictionary(parent)
            .ok()
            .and_then(|d| d.get(b"K").ok())
            .map(|k| kid_objects(&self.inner, k))
            .unwrap_or_default()
    }

    fn set_kid_list(&mut self, parent: ObjectId, kids: Vec<Object>) -> Result<()> {
        self.inner
            .get_dictionary_mut(parent)?
            .set("K", Object::Array(kids));
        Ok(())
    }

    fn insert_kid(&mut self, parent: ObjectId, index: Option<usize>, kid: Object) -> Result<()> {
        let mut kids = self.kid_list(parent);
        let at = index.unwrap_or(kids.len()).min(kids.len());
        kids.insert(at, kid);
        self.set_kid_list(parent, kids)
    }

    fn remove_kid(&mut self, parent: ObjectId, id: ObjectId) -> Result<Option<usize>> {
        let mut kids = self.kid_list(parent);
        let pos = kids.iter().position(|k| k.as_reference().ok() == Some(id));
        if let Some(p) = pos {
            kids.remove(p);
            self.set_kid_list(parent, kids)?;
        }
        Ok(pos)
    }

    fn new_struct_elem(&mut self, parent: ObjectId, tag: &str, page: Option<usize>) -> ObjectId {
        let mut d = dictionary! {
            "Type" => "StructElem",
            "S" => Object::Name(tag.as_bytes().to_vec()),
            "P" => parent,
            "K" => Vec::<Object>::new(),
        };
        if let Some(pid) = page.and_then(|p| self.page_ids().get(p).copied()) {
            d.set("Pg", Object::Reference(pid));
        }
        self.inner.add_object(d)
    }

    fn own_page(&self, id: ObjectId) -> Option<ObjectId> {
        self.inner
            .get_dictionary(id)
            .ok()?
            .get(b"Pg")
            .ok()?
            .as_reference()
            .ok()
    }

    /// A kid object for marked content of a page, inside the element `elem`.
    fn content_kid(&self, elem: ObjectId, page: usize, mcid: i64) -> Object {
        let pid = self.page_ids().get(page).copied();
        if pid.is_some() && self.own_page(elem) == pid {
            Object::Integer(mcid)
        } else {
            let mut d = dictionary! { "Type" => "MCR", "MCID" => mcid };
            if let Some(pid) = pid {
                d.set("Pg", Object::Reference(pid));
            }
            Object::Dictionary(d)
        }
    }

    fn annot_kid(&self, page: usize, index: usize) -> Option<Object> {
        let pid = self.page_ids().get(page).copied()?;
        let pd = self.inner.get_dictionary(pid).ok()?;
        let aid = crate::annots::annot_refs(&self.inner, pd).get(index)?.0?;
        Some(Object::Dictionary(dictionary! {
            "Type" => "OBJR",
            "Obj" => aid,
            "Pg" => pid,
        }))
    }

    /// Keep the kids of an element for which `keep` is true.
    fn retain_kids(
        &mut self,
        tree: &StructElem,
        id: ObjectId,
        keep: &dyn Fn(&StructKid) -> bool,
    ) -> Result<()> {
        let Some(e) = tree.find(id) else { return Ok(()) };
        let raw = self.kid_list(id);
        if raw.len() != e.kids.len() {
            return Ok(());
        }
        let kept: Vec<Object> = raw
            .into_iter()
            .zip(e.kids.iter())
            .filter(|(_, k)| keep(k))
            .map(|(o, _)| o)
            .collect();
        if kept.len() != e.kids.len() {
            self.set_kid_list(id, kept)?;
        }
        Ok(())
    }

    /// Take marked content and annotations away from the elements that own them.
    fn detach_content(
        &mut self,
        mcids: &HashSet<(usize, i64)>,
        annots: &HashSet<(usize, usize)>,
    ) -> Result<()> {
        let Some(tree) = self.structure() else {
            return Ok(());
        };
        let mut owners = Vec::new();
        tree.walk(&mut |e, _| {
            if e.kids.iter().any(|k| match k {
                StructKid::Content { page, mcid } => mcids.contains(&(*page, *mcid)),
                StructKid::Annot { page, index, .. } => annots.contains(&(*page, *index)),
                _ => false,
            }) {
                owners.push(e.id);
            }
        });
        for id in owners {
            self.retain_kids(&tree, id, &|k| match k {
                StructKid::Content { page, mcid } => !mcids.contains(&(*page, *mcid)),
                StructKid::Annot { page, index, .. } => !annots.contains(&(*page, *index)),
                _ => true,
            })?;
        }
        Ok(())
    }

    /// Rebuild the parent tree from the structure tree: the elements that own
    /// each marked content sequence and each tagged annotation.
    pub(crate) fn rebuild_parent_tree(&mut self) -> Result<()> {
        let Some(root_id) = self.struct_root_id() else {
            return Ok(());
        };
        let Some(tree) = self.structure() else {
            return Ok(());
        };
        let mut mcids: BTreeMap<usize, BTreeMap<i64, ObjectId>> = BTreeMap::new();
        let mut annots: HashMap<(usize, usize), ObjectId> = HashMap::new();
        tree.walk(&mut |e, _| {
            if e.is_root() {
                return;
            }
            for k in &e.kids {
                match k {
                    StructKid::Content { page, mcid } if (0..=MAX_MCID).contains(mcid) => {
                        mcids.entry(*page).or_default().insert(*mcid, e.id);
                    }
                    StructKid::Annot { page, index, .. } => {
                        annots.insert((*page, *index), e.id);
                    }
                    _ => {}
                }
            }
        });
        let page_ids = self.page_ids();
        let mut nums: Vec<Object> = Vec::new();
        let mut key: i64 = 0;
        for (p, pid) in page_ids.iter().enumerate() {
            let page = self.inner.get_dictionary_mut(*pid)?;
            match mcids.get(&p) {
                Some(m) if !m.is_empty() => {
                    let max = m.keys().last().copied().unwrap_or(0);
                    let arr: Vec<Object> = (0..=max)
                        .map(|i| m.get(&i).map(|id| Object::Reference(*id)).unwrap_or(Object::Null))
                        .collect();
                    page.set("StructParents", key);
                    nums.push(Object::Integer(key));
                    nums.push(Object::Array(arr));
                    key += 1;
                }
                _ => {
                    page.remove(b"StructParents");
                }
            }
        }
        for (p, pid) in page_ids.iter().enumerate() {
            let refs = match self.inner.get_dictionary(*pid) {
                Ok(pd) => crate::annots::annot_refs(&self.inner, pd),
                Err(_) => continue,
            };
            for (i, (aid, _)) in refs.into_iter().enumerate() {
                let Some(aid) = aid else { continue };
                let Ok(d) = self.inner.get_dictionary_mut(aid) else {
                    continue;
                };
                match annots.get(&(p, i)) {
                    Some(eid) => {
                        d.set("StructParent", key);
                        nums.push(Object::Integer(key));
                        nums.push(Object::Reference(*eid));
                        key += 1;
                    }
                    None => {
                        d.remove(b"StructParent");
                    }
                }
            }
        }
        let tree_dict = Object::Dictionary(dictionary! { "Nums" => nums });
        let old = self
            .inner
            .get_dictionary(root_id)?
            .get(b"ParentTree")
            .ok()
            .and_then(|o| o.as_reference().ok());
        let pt = match old {
            Some(id) => {
                self.inner.objects.insert(id, tree_dict);
                id
            }
            None => self.inner.add_object(tree_dict),
        };
        let root = self.inner.get_dictionary_mut(root_id)?;
        root.set("ParentTree", Object::Reference(pt));
        root.set("ParentTreeNextKey", key);
        Ok(())
    }

    fn check_elem(&self, id: ObjectId) -> Result<StructElem> {
        self.structure()
            .and_then(|t| t.find(id).cloned())
            .ok_or_else(|| Error::Invalid("the tag is no longer in the structure".into()))
    }

    // ------------------------------------------------------------------
    // Editing
    // ------------------------------------------------------------------

    /// Mark the document as tagged or not.
    pub fn set_tagged(&mut self, on: bool) -> Result<()> {
        self.begin_edit();
        self.set_marked(on)?;
        self.commit()
    }

    /// Create an empty structure tree.
    pub fn create_struct_root(&mut self) -> Result<()> {
        if self.has_structure() {
            return Err(Error::Invalid("the document already has a structure root".into()));
        }
        self.begin_edit();
        self.ensure_struct_root()?;
        self.commit()
    }

    /// A new element inside `parent` (the root when None) at `index` among its
    /// kids (the end when None).
    pub fn add_struct_elem(
        &mut self,
        parent: Option<ObjectId>,
        index: Option<usize>,
        tag: &str,
        title: &str,
    ) -> Result<ObjectId> {
        let tag = tag.trim();
        if tag.is_empty() || tag.contains(char::is_whitespace) {
            return Err(Error::Invalid("a tag is one word".into()));
        }
        self.begin_edit();
        let parent = match parent {
            Some(p) => p,
            None => self.ensure_struct_root()?,
        };
        let id = self.new_struct_elem(parent, tag, None);
        if !title.trim().is_empty() {
            self.inner
                .get_dictionary_mut(id)?
                .set("T", lopdf::text_string(title.trim()));
        }
        self.insert_kid(parent, index, Object::Reference(id))?;
        self.commit()?;
        Ok(id)
    }

    /// Remove an element (its content stays on the page, untagged).
    pub fn delete_struct_elem(&mut self, id: ObjectId) -> Result<()> {
        let tree = self
            .structure()
            .ok_or_else(|| Error::Invalid("the document has no tags".into()))?;
        let parent = tree
            .parent_of(id)
            .map(|p| p.id)
            .ok_or_else(|| Error::Invalid("the tag is no longer in the structure".into()))?;
        self.begin_edit();
        let parent = if parent == (0, 0) {
            self.struct_root_id().unwrap_or(parent)
        } else {
            parent
        };
        self.remove_kid(parent, id)?;
        self.rebuild_parent_tree()?;
        self.commit()
    }

    /// Move an element into `new_parent` (the root when None) at `index`
    /// among its kids as they are before the move.
    pub fn move_struct_elem(
        &mut self,
        id: ObjectId,
        new_parent: Option<ObjectId>,
        index: usize,
    ) -> Result<()> {
        let tree = self
            .structure()
            .ok_or_else(|| Error::Invalid("the document has no tags".into()))?;
        let elem = tree
            .find(id)
            .ok_or_else(|| Error::Invalid("the tag is no longer in the structure".into()))?;
        if let Some(np) = new_parent {
            if elem.find(np).is_some() {
                return Err(Error::Invalid("a tag cannot go inside itself".into()));
            }
        }
        let old_parent = tree.parent_of(id).map(|p| p.id);
        let inherited_page = elem.page;
        self.begin_edit();
        // Content written as plain numbers takes its page from the element
        // or from one around it: keep it when the element moves.
        if self.own_page(id).is_none() {
            if let Some(pid) = inherited_page.and_then(|p| self.page_ids().get(p).copied()) {
                self.inner
                    .get_dictionary_mut(id)?
                    .set("Pg", Object::Reference(pid));
            }
        }
        let root = self
            .struct_root_id()
            .ok_or_else(|| Error::Invalid("the document has no tags".into()))?;
        let old_parent = match old_parent {
            Some(p) if p != (0, 0) => p,
            _ => root,
        };
        let new_parent = new_parent.unwrap_or(root);
        let old_pos = self.remove_kid(old_parent, id)?;
        let mut at = index;
        if old_parent == new_parent {
            if let Some(op) = old_pos {
                if op < at {
                    at -= 1;
                }
            }
        }
        self.insert_kid(new_parent, Some(at), Object::Reference(id))?;
        self.inner
            .get_dictionary_mut(id)?
            .set("P", Object::Reference(new_parent));
        self.rebuild_parent_tree()?;
        self.commit()
    }

    /// Change the tag, title, alternate text, actual text and language of an element.
    pub fn set_struct_props(&mut self, id: ObjectId, props: &StructProps) -> Result<()> {
        self.check_elem(id)?;
        let tag = props.tag.trim();
        if tag.is_empty() || tag.contains(char::is_whitespace) {
            return Err(Error::Invalid("a tag is one word".into()));
        }
        self.begin_edit();
        let d = self.inner.get_dictionary_mut(id)?;
        d.set("S", Object::Name(tag.as_bytes().to_vec()));
        for (key, value) in [
            ("T", &props.title),
            ("Alt", &props.alt),
            ("ActualText", &props.actual_text),
            ("Lang", &props.lang),
        ] {
            if value.trim().is_empty() {
                d.remove(key.as_bytes());
            } else {
                d.set(key, lopdf::text_string(value.trim()));
            }
        }
        self.commit()
    }

    /// Change only the tag of an element.
    pub fn set_struct_tag(&mut self, id: ObjectId, tag: &str) -> Result<()> {
        let e = self.check_elem(id)?;
        self.set_struct_props(
            id,
            &StructProps {
                tag: tag.to_string(),
                title: e.title,
                alt: e.alt,
                actual_text: e.actual_text,
                lang: e.lang,
            },
        )
    }

    /// Remove the elements without content; returns how many went away.
    pub fn delete_empty_struct(&mut self) -> Result<usize> {
        let tree = self
            .structure()
            .ok_or_else(|| Error::Invalid("the document has no tags".into()))?;
        let mut empty = Vec::new();
        fn collect(e: &StructElem, out: &mut Vec<(ObjectId, ObjectId)>) {
            for c in e.children() {
                if c.is_empty() {
                    out.push((e.id, c.id));
                } else {
                    collect(c, out);
                }
            }
        }
        collect(&tree, &mut empty);
        if empty.is_empty() {
            return Ok(0);
        }
        self.begin_edit();
        let root = self.struct_root_id().unwrap_or((0, 0));
        for (parent, id) in &empty {
            let parent = if *parent == (0, 0) { root } else { *parent };
            self.remove_kid(parent, *id)?;
        }
        self.rebuild_parent_tree()?;
        self.commit()?;
        Ok(empty.len())
    }

    /// Turn an element and its content into artifacts (decorative content).
    pub fn struct_to_artifact(&mut self, id: ObjectId) -> Result<()> {
        let tree = self
            .structure()
            .ok_or_else(|| Error::Invalid("the document has no tags".into()))?;
        let elem = tree
            .find(id)
            .ok_or_else(|| Error::Invalid("the tag is no longer in the structure".into()))?;
        let parent = tree.parent_of(id).map(|p| p.id);
        let mut by_page: BTreeMap<usize, HashSet<i64>> = BTreeMap::new();
        for (p, m) in elem.content() {
            by_page.entry(p).or_default().insert(m);
        }
        self.begin_edit();
        let page_ids = self.page_ids();
        for (p, set) in by_page {
            let Some(&pid) = page_ids.get(p) else { continue };
            let ps = PageStreams::read(&self.inner, pid);
            if let Some(out) = rewrite_mcids(&ps.data, &|m| {
                if set.contains(&m) {
                    McChange::Artifact
                } else {
                    McChange::Keep
                }
            }) {
                write_pieces(&mut self.inner, pid, &ps, out)?;
            }
        }
        let root = self.struct_root_id().unwrap_or((0, 0));
        let parent = match parent {
            Some(p) if p != (0, 0) => p,
            _ => root,
        };
        self.remove_kid(parent, id)?;
        self.rebuild_parent_tree()?;
        self.commit()
    }

    /// Strip the structure: tags, parent tree and the markers of tagged
    /// content (artifacts stay). Does not commit.
    fn clear_structure_inner(&mut self) -> Result<()> {
        for pid in self.page_ids() {
            let ps = PageStreams::read(&self.inner, pid);
            if let Some(out) = rewrite_mcids(&ps.data, &|_| McChange::Strip) {
                write_pieces(&mut self.inner, pid, &ps, out)?;
            }
            let refs = match self.inner.get_dictionary(pid) {
                Ok(pd) => crate::annots::annot_refs(&self.inner, pd),
                Err(_) => Vec::new(),
            };
            for (aid, _) in refs {
                if let Some(d) = aid.and_then(|a| self.inner.get_dictionary_mut(a).ok()) {
                    d.remove(b"StructParent");
                }
            }
            if let Ok(pd) = self.inner.get_dictionary_mut(pid) {
                pd.remove(b"StructParents");
            }
        }
        let cat = self.catalog_id()?;
        let c = self.inner.get_dictionary_mut(cat)?;
        c.remove(b"StructTreeRoot");
        c.remove(b"MarkInfo");
        self.inner.prune_objects();
        Ok(())
    }

    /// Remove the whole structure of the document.
    pub fn clear_structure(&mut self) -> Result<()> {
        self.begin_edit();
        self.clear_structure_inner()?;
        self.commit()
    }

    /// Replace the role map.
    pub fn set_role_map(&mut self, map: &[(String, String)]) -> Result<()> {
        self.begin_edit();
        let root = self.ensure_struct_root()?;
        let mut d = Dictionary::new();
        for (k, v) in map {
            let (k, v) = (k.trim(), v.trim());
            if !k.is_empty() && !v.is_empty() {
                d.set(k.as_bytes().to_vec(), Object::Name(v.as_bytes().to_vec()));
            }
        }
        let r = self.inner.get_dictionary_mut(root)?;
        if d.is_empty() {
            r.remove(b"RoleMap");
        } else {
            r.set("RoleMap", Object::Dictionary(d));
        }
        self.commit()
    }

    /// Replace the class map (attributes as `Key=Value` text, `;` between dictionaries).
    pub fn set_class_map(&mut self, map: &[(String, String)]) -> Result<()> {
        self.begin_edit();
        let root = self.ensure_struct_root()?;
        let mut d = Dictionary::new();
        for (k, v) in map {
            if !k.trim().is_empty() {
                d.set(k.trim().as_bytes().to_vec(), text_to_attrs(v));
            }
        }
        let r = self.inner.get_dictionary_mut(root)?;
        if d.is_empty() {
            r.remove(b"ClassMap");
        } else {
            r.set("ClassMap", Object::Dictionary(d));
        }
        self.commit()
    }

    /// Write the standard types of the role map on the elements; returns how
    /// many elements changed.
    pub fn apply_role_map(&mut self) -> Result<usize> {
        let tree = self
            .structure()
            .ok_or_else(|| Error::Invalid("the document has no tags".into()))?;
        let mut changes = Vec::new();
        tree.walk(&mut |e, _| {
            if !e.is_root() && e.tag != e.role && is_standard(&e.role) {
                changes.push((e.id, e.role.clone()));
            }
        });
        if changes.is_empty() {
            return Ok(0);
        }
        self.begin_edit();
        for (id, role) in &changes {
            self.inner
                .get_dictionary_mut(*id)?
                .set("S", Object::Name(role.as_bytes().to_vec()));
        }
        self.commit()?;
        Ok(changes.len())
    }

    /// Move the elements of a page's reading order: the item `from` goes to
    /// the place of the item `to`.
    pub fn move_reading_order(
        &mut self,
        page: usize,
        from: usize,
        to: usize,
        table_cells: bool,
    ) -> Result<()> {
        let items = self.reading_order(page, table_cells);
        let (Some(a), Some(b)) = (items.get(from), items.get(to)) else {
            return Err(Error::Invalid("no such item in the reading order".into()));
        };
        if from == to {
            return Ok(());
        }
        let tree = self
            .structure()
            .ok_or_else(|| Error::Invalid("the document has no tags".into()))?;
        let target_parent = tree
            .parent_of(b.id)
            .ok_or_else(|| Error::Invalid("tag not found".into()))?;
        let pos = target_parent
            .kids
            .iter()
            .position(|k| matches!(k, StructKid::Elem(e) if e.id == b.id))
            .unwrap_or(0);
        let index = if from < to { pos + 1 } else { pos };
        let parent = (!target_parent.is_root()).then_some(target_parent.id);
        self.move_struct_elem(a.id, parent, index)
    }

    /// Join neighbouring items of the reading order with the same tag (lines
    /// of one paragraph); returns how many items went away.
    pub fn group_similar(&mut self, page: usize, table_cells: bool) -> Result<usize> {
        let items = self.reading_order(page, table_cells);
        let tree = self
            .structure()
            .ok_or_else(|| Error::Invalid("the document has no tags".into()))?;
        let mut pairs: Vec<(ObjectId, ObjectId, ObjectId)> = Vec::new();
        let mut keep: Option<&OrderItem> = None;
        for it in &items {
            let joinable =
                matches!(it.role.as_str(), "P" | "H" | "Span") || heading_level(&it.role).is_some();
            match keep {
                Some(k)
                    if joinable
                        && k.tag == it.tag
                        && tree.parent_of(k.id).map(|p| p.id) == tree.parent_of(it.id).map(|p| p.id) =>
                {
                    let parent = tree.parent_of(it.id).map(|p| p.id).unwrap_or((0, 0));
                    pairs.push((k.id, it.id, parent));
                }
                _ => keep = Some(it),
            }
            if !joinable {
                keep = None;
            }
        }
        if pairs.is_empty() {
            return Ok(0);
        }
        self.begin_edit();
        let root = self.struct_root_id().unwrap_or((0, 0));
        for (into, from, parent) in &pairs {
            let Some(src) = tree.find(*from) else { continue };
            let raw = self.kid_list(*from);
            let mut moved = Vec::new();
            for (o, k) in raw.into_iter().zip(src.kids.iter()) {
                match k {
                    StructKid::Content { page, mcid } => moved.push(self.content_kid(*into, *page, *mcid)),
                    StructKid::Elem(e) => {
                        self.inner
                            .get_dictionary_mut(e.id)?
                            .set("P", Object::Reference(*into));
                        moved.push(o);
                    }
                    _ => moved.push(o),
                }
            }
            let mut kids = self.kid_list(*into);
            kids.extend(moved);
            self.set_kid_list(*into, kids)?;
            let parent = if *parent == (0, 0) { root } else { *parent };
            self.remove_kid(parent, *from)?;
        }
        self.rebuild_parent_tree()?;
        self.commit()?;
        Ok(pairs.len())
    }

    // ------------------------------------------------------------------
    // Tagging content
    // ------------------------------------------------------------------

    /// Units of drawing outside tagged content, measured through probes.
    fn probe_pages(&self, pages: &[usize]) -> Result<HashMap<usize, (PageStreams, Vec<Probed>)>> {
        let page_ids = self.page_ids();
        let mut copy = self.inner.clone();
        let mut found: HashMap<usize, (PageStreams, Vec<Unit>)> = HashMap::new();
        for &p in pages {
            let Some(&pid) = page_ids.get(p) else { continue };
            let ps = PageStreams::read(&self.inner, pid);
            let data = &ps.data;
            let ops = operations(data);
            let units = content_units(data, &ops);
            let wraps: Vec<(usize, usize, Wrap)> = units
                .iter()
                .enumerate()
                .map(|(k, u)| {
                    (
                        u.first,
                        u.last,
                        Wrap::Mcid("FolitectaProbe".into(), PROBE_BASE + k as i64),
                    )
                })
                .collect();
            let probe: Vec<u8> = write_wrapped(data, &ops, &wraps)
                .into_iter()
                .flat_map(|p| p.1)
                .collect();
            set_page_content(&mut copy, pid, probe)?;
            found.insert(p, (ps, units));
        }
        let bytes = Arc::new(crate::document::serialize(&mut copy)?);
        let measured = measure(bytes, pages)?;
        Ok(found
            .into_iter()
            .map(|(p, (ps, units))| {
                let items = measured.get(&p).cloned().unwrap_or_default();
                let probed = units
                    .into_iter()
                    .enumerate()
                    .map(|(k, unit)| Probed {
                        unit,
                        item: items
                            .iter()
                            .find(|i| i.mcid == Some(PROBE_BASE + k as i64))
                            .cloned(),
                    })
                    .collect();
                (p, (ps, probed))
            })
            .collect())
    }

    /// Runs of operations for units that share marked content: text units of
    /// one segment go together, any other unit alone.
    fn runs(units: &[Unit]) -> Vec<(usize, usize)> {
        // Text units next to each other in one segment: (segment, unit number).
        type Run = (usize, usize, Option<(usize, usize)>);
        let mut out: Vec<Run> = Vec::new();
        for u in units {
            match out.last_mut() {
                Some(last) if u.kind == UnitKind::Text && last.2 == Some((u.segment, u.seq - 1)) => {
                    last.1 = u.last;
                    last.2 = Some((u.segment, u.seq));
                }
                _ => out.push((
                    u.first,
                    u.last,
                    (u.kind == UnitKind::Text).then_some((u.segment, u.seq)),
                )),
            }
        }
        out.into_iter().map(|(a, b, _)| (a, b)).collect()
    }

    /// Tag the content drawn inside an area of a page: content not tagged yet
    /// is marked, tagged content moves to the new element. The element goes
    /// into `parent` (the document element or the root when None).
    pub fn tag_area(
        &mut self,
        page: usize,
        rect: [f32; 4],
        tag: &str,
        parent: Option<ObjectId>,
        index: Option<usize>,
    ) -> Result<ObjectId> {
        let tag = tag.trim();
        if tag.is_empty() || tag.contains(char::is_whitespace) {
            return Err(Error::Invalid("a tag is one word".into()));
        }
        let pid = self.page_id(page)?;
        let probes = self.probe_pages(&[page])?;
        let (ps, probed) = probes.get(&page).cloned().unwrap_or_default();
        let inside = |r: [f32; 4]| {
            if !intersects(r, rect) {
                return false;
            }
            let ix = (r[2].min(rect[2]) - r[0].max(rect[0])).max(0.0);
            let iy = (r[3].min(rect[3]) - r[1].max(rect[1])).max(0.0);
            let area = ((r[2] - r[0]) * (r[3] - r[1])).max(0.01);
            let (cx, cy) = ((r[0] + r[2]) / 2.0, (r[1] + r[3]) / 2.0);
            ix * iy / area > 0.3 || (cx >= rect[0] && cx <= rect[2] && cy >= rect[1] && cy <= rect[3])
        };
        let chosen: Vec<Unit> = probed
            .iter()
            .filter(|p| p.item.as_ref().is_some_and(|i| inside(i.rect)))
            .map(|p| p.unit)
            .collect();
        let existing: Vec<i64> = self
            .marked_content(page)
            .iter()
            .filter(|i| i.mcid.is_some() && inside(i.rect))
            .filter_map(|i| i.mcid)
            .collect();
        if chosen.is_empty() && existing.is_empty() {
            return Err(Error::Invalid("nothing is drawn in the selected area".into()));
        }
        self.begin_edit();
        let mut new_mcids = Vec::new();
        if !chosen.is_empty() {
            let ops = operations(&ps.data);
            let mut next = max_mcid(&ps.data).map(|m| m + 1).unwrap_or(0);
            let wraps: Vec<(usize, usize, Wrap)> = Self::runs(&chosen)
                .into_iter()
                .map(|(a, b)| {
                    let w = Wrap::Mcid(tag.to_string(), next);
                    new_mcids.push(next);
                    next += 1;
                    (a, b, w)
                })
                .collect();
            write_pieces(&mut self.inner, pid, &ps, write_wrapped(&ps.data, &ops, &wraps))?;
        }
        let moved: HashSet<(usize, i64)> = existing.iter().map(|m| (page, *m)).collect();
        self.detach_content(&moved, &HashSet::new())?;
        let root = self.ensure_struct_root()?;
        let parent = match parent {
            Some(p) if p != (0, 0) => p,
            _ => self.document_elem(root)?,
        };
        let id = self.new_struct_elem(parent, tag, Some(page));
        let mut kids: Vec<Object> = Vec::new();
        let mut all: Vec<i64> = existing;
        all.extend(new_mcids);
        all.sort_unstable();
        all.dedup();
        for m in all {
            kids.push(Object::Integer(m));
        }
        self.set_kid_list(id, kids)?;
        self.insert_kid(parent, index, Object::Reference(id))?;
        self.rebuild_parent_tree()?;
        self.commit()?;
        Ok(id)
    }

    /// Tag an annotation (`Link`, `Form` or `Annot` element) inside `parent`.
    pub fn tag_annotation(
        &mut self,
        page: usize,
        index: usize,
        parent: Option<ObjectId>,
    ) -> Result<ObjectId> {
        let annot = self
            .annotations(page)?
            .into_iter()
            .nth(index)
            .ok_or_else(|| Error::Invalid("annotation not found".into()))?;
        let kid = self
            .annot_kid(page, index)
            .ok_or_else(|| Error::Invalid("the annotation cannot be tagged".into()))?;
        let tag = match annot.subtype.as_str() {
            "Link" => "Link",
            "Widget" => "Form",
            _ => "Annot",
        };
        self.begin_edit();
        let mut detach = HashSet::new();
        detach.insert((page, index));
        self.detach_content(&HashSet::new(), &detach)?;
        let root = self.ensure_struct_root()?;
        let parent = match parent {
            Some(p) if p != (0, 0) => p,
            _ => self.document_elem(root)?,
        };
        let id = self.new_struct_elem(parent, tag, Some(page));
        self.set_kid_list(id, vec![kid])?;
        self.insert_kid(parent, None, Object::Reference(id))?;
        self.rebuild_parent_tree()?;
        self.commit()?;
        Ok(id)
    }

    /// The `Document` element under the root, created when the root has none.
    fn document_elem(&mut self, root: ObjectId) -> Result<ObjectId> {
        let roles = self.roles();
        for k in self.kid_list(root) {
            if let Ok(r) = k.as_reference() {
                if let Ok(d) = self.inner.get_dictionary(r) {
                    if name_value(d, b"S").map(|s| role_of(&roles, &s)).as_deref() == Some("Document") {
                        return Ok(r);
                    }
                }
            }
        }
        let id = self.new_struct_elem(root, "Document", None);
        self.insert_kid(root, None, Object::Reference(id))?;
        Ok(id)
    }

    /// Annotations that have no element yet (form fields only with `fields_only`).
    fn untagged_annotations(&self, fields_only: bool) -> Vec<(usize, usize, String)> {
        let tagged: HashSet<(usize, usize)> = self
            .structure()
            .map(|t| {
                let mut s = HashSet::new();
                t.walk(&mut |e, _| {
                    for k in &e.kids {
                        if let StructKid::Annot { page, index, .. } = k {
                            s.insert((*page, *index));
                        }
                    }
                });
                s
            })
            .unwrap_or_default();
        let mut todo = Vec::new();
        for p in 0..self.page_count() {
            for a in self.annotations(p).unwrap_or_default() {
                let taggable = match a.subtype.as_str() {
                    "Popup" | "PrinterMark" | "TrapNet" => false,
                    "Widget" => true,
                    _ => !fields_only && !a.hidden,
                };
                if taggable && a.id.is_some() && !tagged.contains(&(p, a.index)) {
                    todo.push((p, a.index, a.subtype.clone()));
                }
            }
        }
        todo
    }

    /// Tag annotations without an element; returns how many were tagged.
    /// Does not commit.
    fn tag_annotations_inner(&mut self, todo: &[(usize, usize, String)]) -> Result<usize> {
        if todo.is_empty() {
            return Ok(0);
        }
        let root = self.ensure_struct_root()?;
        let doc_elem = self.document_elem(root)?;
        let mut pages_with_annots = HashSet::new();
        for (p, i, subtype) in todo {
            let Some(kid) = self.annot_kid(*p, *i) else {
                continue;
            };
            let tag = match subtype.as_str() {
                "Link" => "Link",
                "Widget" => "Form",
                _ => "Annot",
            };
            let id = self.new_struct_elem(doc_elem, tag, Some(*p));
            self.set_kid_list(id, vec![kid])?;
            self.insert_kid(doc_elem, None, Object::Reference(id))?;
            pages_with_annots.insert(*p);
        }
        let page_ids = self.page_ids();
        for p in pages_with_annots {
            if let Some(pd) = page_ids
                .get(p)
                .and_then(|pid| self.inner.get_dictionary_mut(*pid).ok())
            {
                pd.set("Tabs", Object::Name(b"S".to_vec()));
            }
        }
        self.rebuild_parent_tree()?;
        Ok(todo.len())
    }

    /// Tag the form fields that have no element yet; returns how many.
    pub fn tag_form_fields(&mut self) -> Result<usize> {
        self.tag_untagged(true)
    }

    /// Tag every annotation that has no element yet; returns how many.
    pub fn tag_annotations(&mut self) -> Result<usize> {
        self.tag_untagged(false)
    }

    fn tag_untagged(&mut self, fields_only: bool) -> Result<usize> {
        let todo = self.untagged_annotations(fields_only);
        if todo.is_empty() {
            return Ok(0);
        }
        self.begin_edit();
        let n = self.tag_annotations_inner(&todo)?;
        self.commit()?;
        Ok(n)
    }

    /// Tag the whole document from what its pages draw: paragraphs and
    /// headings (by text size), figures, artifacts for drawings and for the
    /// running headers and footers, annotations. Existing tags are replaced.
    pub fn auto_tag(&mut self) -> Result<AutoTagReport> {
        let pages: Vec<usize> = (0..self.page_count()).collect();
        self.begin_edit();
        if self.has_structure() {
            self.clear_structure_inner()?;
        }
        let probes = self.probe_pages(&pages)?;
        let sizes: Vec<crate::document::PageSize> = self.page_sizes();
        // Text that repeats at the top or bottom of the pages.
        let norm = |t: &str| {
            t.chars()
                .filter(|c| !c.is_ascii_digit())
                .collect::<String>()
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                .to_lowercase()
        };
        let in_margin = |p: usize, r: [f32; 4]| {
            let h = sizes.get(p).map(|s| s.height).unwrap_or(792.0);
            r[3] < h * 0.08 || r[1] > h * 0.92
        };
        let mut repeated: HashMap<String, HashSet<usize>> = HashMap::new();
        for (p, (_, probed)) in &probes {
            for pr in probed {
                if let Some(i) = &pr.item {
                    if i.has_text && in_margin(*p, i.rect) && i.text.chars().count() < 100 {
                        repeated.entry(norm(&i.text)).or_default().insert(*p);
                    }
                }
            }
        }
        let is_running = |p: usize, i: &MarkedItem| {
            if !i.has_text || !in_margin(p, i.rect) || i.text.chars().count() >= 100 {
                return false;
            }
            let n = norm(&i.text);
            n.is_empty() || (pages.len() > 1 && repeated.get(&n).is_some_and(|s| s.len() >= 2))
        };
        // Size of the body text: the size most characters have.
        let mut weights: HashMap<i32, usize> = HashMap::new();
        for (p, (_, probed)) in &probes {
            for pr in probed {
                if let Some(i) = &pr.item {
                    if i.has_text && !is_running(*p, i) {
                        *weights.entry((i.font_size * 2.0).round() as i32).or_default() +=
                            i.text.chars().count();
                    }
                }
            }
        }
        let body = weights
            .iter()
            .max_by_key(|(s, n)| (**n, -**s))
            .map(|(s, _)| *s as f32 / 2.0)
            .unwrap_or(10.0);
        // Paragraph and heading clusters, figures and artifacts per page.
        enum Block {
            Text {
                heading: bool,
                size: f32,
                units: Vec<Unit>,
                rect: [f32; 4],
                chars: usize,
            },
            Figure(Unit, [f32; 4]),
        }
        let mut blocks: BTreeMap<usize, Vec<Block>> = BTreeMap::new();
        let mut artifacts: BTreeMap<usize, Vec<Unit>> = BTreeMap::new();
        let mut heading_sizes: Vec<i32> = Vec::new();
        for &p in &pages {
            let Some((ps, probed)) = probes.get(&p) else {
                continue;
            };
            let list = blocks.entry(p).or_default();
            for pr in probed {
                // Units that draw nothing need no marking.
                let Some(item) = &pr.item else { continue };
                // Watermarks, running headers and page numbers added by Folitecta.
                if ps.is_pagination(pr.unit.offset) {
                    artifacts.entry(p).or_default().push(pr.unit);
                    continue;
                }
                let text_like = item.has_text && !item.text.trim().is_empty();
                if text_like && !is_running(p, item) {
                    let size = item.font_size;
                    let heading = size >= body * 1.2;
                    if let Some(Block::Text {
                        heading: h,
                        size: s,
                        units,
                        rect,
                        chars,
                    }) = list.last_mut()
                    {
                        let same_class = *h == heading && (size - *s).abs() <= 0.12 * s.max(size);
                        let last = *rect;
                        let line = ((item.rect[1] + item.rect[3]) / 2.0 > last[1])
                            && ((item.rect[1] + item.rect[3]) / 2.0 < last[3]);
                        let below =
                            item.rect[1] >= last[3] - 0.5 * size && item.rect[1] - last[3] <= 0.9 * size;
                        let overlap = item.rect[0] < last[2] + size && item.rect[2] > last[0] - size;
                        if same_class && (line || (below && overlap)) {
                            units.push(pr.unit);
                            *rect = union(Some(*rect), item.rect).unwrap_or(*rect);
                            *chars += item.text.chars().count();
                            continue;
                        }
                    }
                    list.push(Block::Text {
                        heading,
                        size,
                        units: vec![pr.unit],
                        rect: item.rect,
                        chars: item.text.chars().count(),
                    });
                } else if item.has_image && !text_like {
                    list.push(Block::Figure(pr.unit, item.rect));
                } else {
                    artifacts.entry(p).or_default().push(pr.unit);
                }
            }
            for b in list.iter_mut() {
                if let Block::Text {
                    heading, size, chars, ..
                } = b
                {
                    if *heading && *chars > 200 {
                        *heading = false;
                    }
                    if *heading {
                        heading_sizes.push((*size * 2.0).round() as i32);
                    }
                }
            }
        }
        heading_sizes.sort_unstable_by(|a, b| b.cmp(a));
        heading_sizes.dedup();
        let level_of = |size: f32| {
            let s = (size * 2.0).round() as i32;
            heading_sizes
                .iter()
                .position(|h| *h == s)
                .map(|i| (i + 1).min(6))
                .unwrap_or(1)
        };
        // Write the marked content and build the elements.
        let mut report = AutoTagReport::default();
        let root = self.ensure_struct_root()?;
        let doc_elem = self.document_elem(root)?;
        let page_ids = self.page_ids();
        for &p in &pages {
            let (Some((ps, _)), Some(&pid)) = (probes.get(&p), page_ids.get(p)) else {
                continue;
            };
            let ops = operations(&ps.data);
            let mut next = 0i64;
            let mut wraps: Vec<(usize, usize, Wrap)> = Vec::new();
            let mut elems: Vec<(String, Vec<i64>, [f32; 4])> = Vec::new();
            for b in blocks.get(&p).map(|v| v.as_slice()).unwrap_or(&[]) {
                match b {
                    Block::Text {
                        heading,
                        size,
                        units,
                        rect,
                        ..
                    } => {
                        let tag = if *heading {
                            format!("H{}", level_of(*size))
                        } else {
                            "P".to_string()
                        };
                        let mut ids = Vec::new();
                        for (a, z) in Self::runs(units) {
                            wraps.push((a, z, Wrap::Mcid(tag.clone(), next)));
                            ids.push(next);
                            next += 1;
                        }
                        if *heading {
                            report.headings += 1;
                        } else {
                            report.paragraphs += 1;
                        }
                        elems.push((tag, ids, *rect));
                    }
                    Block::Figure(u, rect) => {
                        wraps.push((u.first, u.last, Wrap::Mcid("Figure".into(), next)));
                        elems.push(("Figure".into(), vec![next], *rect));
                        next += 1;
                        report.figures += 1;
                    }
                }
            }
            for u in artifacts.get(&p).map(|v| v.as_slice()).unwrap_or(&[]) {
                wraps.push((u.first, u.last, Wrap::Artifact));
                report.artifacts += 1;
            }
            wraps.sort_by_key(|w| w.0);
            if !wraps.is_empty() {
                write_pieces(&mut self.inner, pid, ps, write_wrapped(&ps.data, &ops, &wraps))?;
            }
            let mut created: Vec<(ObjectId, [f32; 4], String)> = Vec::new();
            for (tag, ids, rect) in elems {
                let id = self.new_struct_elem(doc_elem, &tag, Some(p));
                self.set_kid_list(id, ids.into_iter().map(Object::Integer).collect())?;
                self.insert_kid(doc_elem, None, Object::Reference(id))?;
                created.push((id, rect, tag));
            }
            // Links inside the paragraph they cover, other annotations after the page.
            let size = self.page_size(p)?;
            let mut has_annots = false;
            for a in self.annotations(p).unwrap_or_default() {
                if matches!(a.subtype.as_str(), "Popup" | "PrinterMark" | "TrapNet") || a.id.is_none() {
                    continue;
                }
                if a.hidden && a.subtype != "Widget" {
                    continue;
                }
                let Some(kid) = self.annot_kid(p, a.index) else {
                    continue;
                };
                let tag = match a.subtype.as_str() {
                    "Link" => "Link",
                    "Widget" => "Form",
                    _ => "Annot",
                };
                let r = crate::marks::pdf_rect_to_display(&size, a.rect);
                let host = (tag == "Link")
                    .then(|| {
                        created
                            .iter()
                            .filter(|(_, cr, t)| t != "Figure" && intersects(*cr, r))
                            .max_by(|x, y| {
                                let ov = |c: [f32; 4]| {
                                    (c[2].min(r[2]) - c[0].max(r[0])).max(0.0)
                                        * (c[3].min(r[3]) - c[1].max(r[1])).max(0.0)
                                };
                                ov(x.1).total_cmp(&ov(y.1))
                            })
                            .map(|c| c.0)
                    })
                    .flatten();
                let parent = host.unwrap_or(doc_elem);
                let id = self.new_struct_elem(parent, tag, Some(p));
                self.set_kid_list(id, vec![kid])?;
                self.insert_kid(parent, None, Object::Reference(id))?;
                report.annotations += 1;
                has_annots = true;
            }
            if has_annots {
                self.inner
                    .get_dictionary_mut(pid)?
                    .set("Tabs", Object::Name(b"S".to_vec()));
            }
        }
        self.set_marked(true)?;
        self.rebuild_parent_tree()?;
        self.commit()?;
        Ok(report)
    }

    /// Elements of a role (for example every `Figure`), in document order.
    pub fn struct_elems_of(&self, role: &str) -> Vec<StructElem> {
        let mut out = Vec::new();
        if let Some(t) = self.structure() {
            t.walk(&mut |e, _| {
                if e.role == role {
                    out.push(e.clone());
                }
            });
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn units_and_wraps() {
        let data = b"q 0 0 m 5 5 l S Q BT /F1 12 Tf (a) Tj 0 -14 Td (b) Tj ET /P <</MCID 0>> BDC BT (c) Tj ET EMC /Im1 Do";
        let ops = operations(data);
        let units = content_units(data, &ops);
        let kinds: Vec<UnitKind> = units.iter().map(|u| u.kind).collect();
        assert_eq!(
            kinds,
            vec![UnitKind::Path, UnitKind::Text, UnitKind::Text, UnitKind::XObject]
        );
        assert_eq!(units[1].segment, units[2].segment);
        assert_eq!(Document::runs(&units[1..3]).len(), 1);
        assert_eq!(max_mcid(data), Some(0));
        let wraps = vec![(units[1].first, units[2].last, Wrap::Mcid("P".into(), 1))];
        let out = String::from_utf8(join(&write_wrapped(data, &ops, &wraps))).unwrap();
        assert!(out.contains("/P <</MCID 1>> BDC\n(a) Tj\n0 -14 Td\n(b) Tj\nEMC"));
        let stripped = String::from_utf8(join(&rewrite_mcids(data, &|_| McChange::Strip).unwrap())).unwrap();
        assert!(!stripped.contains("MCID") && !stripped.contains("EMC"));
        let art = String::from_utf8(join(&rewrite_mcids(data, &|_| McChange::Artifact).unwrap())).unwrap();
        assert!(art.contains("/Artifact BMC\nBT\n(c) Tj\nET\nEMC"));
    }

    #[test]
    fn attribute_text_round_trip() {
        let o = text_to_attrs("O=Layout Placement=Block SpaceBefore=6 BBox=[0 0 10 10]");
        let doc = lopdf::Document::with_version("1.7");
        let t = attrs_to_text(&doc, &o);
        assert!(t.contains("O=Layout") && t.contains("Placement=Block") && t.contains("SpaceBefore=6"));
        assert!(t.contains("BBox=[0 0 10 10]"));
        assert_eq!(pdf_name("My Tag"), "/My#20Tag");
        assert_eq!(heading_level("H3"), Some(3));
        assert_eq!(heading_level("Hx"), None);
    }
}
