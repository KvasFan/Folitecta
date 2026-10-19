//! Drawing pages with markers, to learn which operation of which content
//! stream drew each glyph, picture and path.
//!
//! A copy of the document is made in which every text-showing operation,
//! every `Do` and every inline picture of the probed streams is wrapped in a
//! marked-content sequence whose MCID identifies it. The copy is drawn once
//! per page with a device that records what is drawn under which markers.
//! With [`ProbeOptions::forms`] the forms drawn by the pages are wrapped too
//! (each form once, whatever draws it: the chain of `Do` markers tells which
//! drawing a glyph comes from). With [`ProbeOptions::reveal`] nothing is
//! hidden: optional content is neutralized at the source (every marked-content
//! property list becomes a plain tag, no XObject keeps its `/OC`), so content
//! of layers that are off — directly or through membership dictionaries with
//! any policy or visibility expression — is drawn, and clip-only text
//! (`7 Tr`) is drawn as invisible text.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;

use hayro::hayro_interpret::font::{Glyph, GlyphRun};
use hayro::hayro_interpret::{
    BlendMode, ClipPath, Context, Device, DrawMode, DrawProps, Image, ImageDrawProps, InterpreterCache,
    InterpreterSettings, SoftMask, TransformExt, interpret_page,
};
use hayro::hayro_syntax::Pdf;
use hayro::kurbo::{Affine, BezPath, Point, Shape};
use lopdf::{Dictionary, Object, ObjectId, Stream, dictionary};

use crate::content_edit::joined;
use crate::content_lex::{name_of, operations};
use crate::error::{Error, Result};
use crate::util::deref;

/// Marker of a text-showing operation.
const TX: &[u8] = b"FoliTx";
/// Marker of a `Do`.
const DO: &[u8] = b"FoliDo";
/// Marker of an inline picture.
const IM: &[u8] = b"FoliIm";
/// Marker of a stream that is not content (the covers of a redaction).
const CV: &[u8] = b"FoliCv";
/// The plain tag every marked-content sequence gets when revealing.
const NEUTRAL: &[u8] = b"FoliMc";

/// Text-showing operators.
pub(crate) const SHOW: &[&[u8]] = &[b"Tj", b"TJ", b"'", b"\""];

/// What a probe draws.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct ProbeOptions {
    /// Hidden optional content and clip-only text are drawn too.
    pub reveal: bool,
    /// The forms drawn by the pages are wrapped too.
    pub forms: bool,
}

/// A content stream: of a page (its streams joined) or of a form.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum StreamKey {
    Page(usize),
    Form(ObjectId),
}

/// What each marker stands for; the index is the MCID written in the copy.
#[derive(Debug, Default)]
pub(crate) struct ProbeIds {
    /// A text-showing operation: its stream and its index among the
    /// text-showing operations of the stream.
    pub text: Vec<(StreamKey, usize)>,
    /// A `Do`: its stream and its index among the operations of the stream.
    pub draws: Vec<(StreamKey, usize)>,
    /// An inline picture: its stream and the index of its `BI` operation.
    pub inline: Vec<(StreamKey, usize)>,
}

impl ProbeIds {
    /// The marker of every `Do`, by stream and operation.
    pub fn draw_markers(&self) -> HashMap<(StreamKey, usize), usize> {
        self.draws.iter().enumerate().map(|(m, k)| (*k, m)).collect()
    }
}

/// A glyph drawn while probing.
#[derive(Debug, Clone)]
pub(crate) struct ProbeGlyph {
    /// The marker of the text-showing operation that drew it (an index of
    /// [`ProbeIds::text`]; for [`probe_page_glyphs`], the index of the
    /// operation among those of the page).
    pub op: Option<usize>,
    /// The `Do` markers it was drawn under, outermost first.
    pub chain: Vec<usize>,
    pub text: String,
    /// Quadrilateral as in the text layer (display space).
    pub quad: [f32; 4 * 2],
    /// Bounds of the body of the glyph (from below the baseline to the
    /// height of capitals), narrower than the quad so the lines above and
    /// below are not touched (display space).
    pub body: [f32; 4],
    /// How far the next glyph of the same operation starts, in thousandths
    /// of the font size (the units of `TJ` numbers): the width of this glyph
    /// with the spacing and the `TJ` numbers between them.
    pub to_next: Option<f64>,
    /// The advance of the glyph's outline (same units); None for Type 3 glyphs.
    pub own_advance: Option<f64>,
    /// Drawn by a stream that is not content (a redaction cover).
    pub covered: bool,
}

/// What drew a picture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Marker {
    /// A `Do` (an index of [`ProbeIds::draws`]).
    Do(usize),
    /// An inline picture (an index of [`ProbeIds::inline`]).
    Inline(usize),
}

/// A picture drawn while probing.
#[derive(Debug, Clone)]
pub(crate) struct ProbeImage {
    /// Bounds (display space).
    pub bounds: [f32; 4],
    /// The `Do` markers of the forms it was drawn in, outermost first.
    pub chain: Vec<usize>,
    pub marker: Option<Marker>,
    pub covered: bool,
}

/// A path painted while probing.
#[derive(Debug, Clone)]
pub(crate) struct ProbePath {
    /// Bounds of the path (display space; strokes without their width).
    pub bounds: [f32; 4],
    pub covered: bool,
}

/// What a page drew.
#[derive(Debug, Default)]
pub(crate) struct PageProbe {
    pub glyphs: Vec<ProbeGlyph>,
    pub images: Vec<ProbeImage>,
    pub paths: Vec<ProbePath>,
}

/// The result of a probe: the markers and what each page drew (pages the
/// renderer failed on are missing).
#[derive(Debug, Default)]
pub(crate) struct Probe {
    pub ids: ProbeIds,
    pub pages: HashMap<usize, PageProbe>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Frame {
    Tx(usize),
    Do(usize),
    Im(usize),
    Cover,
    Other,
}

#[derive(Default)]
struct ProbeDevice {
    stack: Vec<Frame>,
    out: PageProbe,
    last_fill: Vec<(i64, i64)>,
}

fn bounds_of(points: &[Point]) -> [f32; 4] {
    [
        points.iter().map(|p| p.x).fold(f64::MAX, f64::min) as f32,
        points.iter().map(|p| p.y).fold(f64::MAX, f64::min) as f32,
        points.iter().map(|p| p.x).fold(f64::MIN, f64::max) as f32,
        points.iter().map(|p| p.y).fold(f64::MIN, f64::max) as f32,
    ]
}

impl ProbeDevice {
    fn covered(&self) -> bool {
        self.stack.contains(&Frame::Cover)
    }

    fn chain(&self) -> Vec<usize> {
        self.stack
            .iter()
            .filter_map(|f| match f {
                Frame::Do(m) => Some(*m),
                _ => None,
            })
            .collect()
    }
}

impl<'a> Device<'a> for ProbeDevice {
    fn draw_path(&mut self, path: &BezPath, props: DrawProps<'a>, _mode: &DrawMode) {
        let b = (props.transform * path.clone()).bounding_box();
        if !(b.x0.is_finite() && b.y0.is_finite() && b.x1.is_finite() && b.y1.is_finite()) {
            return;
        }
        self.out.paths.push(ProbePath {
            bounds: [b.x0 as f32, b.y0 as f32, b.x1 as f32, b.y1 as f32],
            covered: self.covered(),
        });
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
        // Fill and stroke text arrives twice; keep the first copy only.
        if matches!(mode, DrawMode::Stroke(_)) && key == self.last_fill {
            return;
        }
        if !matches!(mode, DrawMode::Stroke(_)) {
            self.last_fill = key;
        }
        let op = self.stack.iter().rev().find_map(|f| match f {
            Frame::Tx(m) => Some(*m),
            _ => None,
        });
        let chain = self.chain();
        let covered = self.covered();
        // Where the next glyph of the run starts, seen from each glyph.
        let to_next: Vec<Option<f64>> = (0..glyphs.len())
            .map(|i| {
                glyphs
                    .get(i + 1)
                    .map(|n| (glyphs[i].transform().inverse() * n.transform() * Point::ZERO).x)
                    .filter(|d| d.is_finite() && d.abs() < 100_000.0)
            })
            .collect();
        for (i, g) in glyphs.iter().enumerate() {
            // The same geometry as the text layer, so the glyphs can be matched.
            let t: Affine = props.transform * g.transform();
            let (own, text) = match &**g {
                Glyph::Outline(o) => (o.advance_width().map(|a| a as f64), g.as_unicode()),
                Glyph::Type3(_) => (None, g.as_unicode()),
            };
            let own = own.filter(|a| *a > 0.0);
            let adv = own.unwrap_or_else(|| to_next[i].filter(|d| *d > 0.0 && *d < 3000.0).unwrap_or(500.0));
            let text = match text {
                Some(hayro::hayro_interpret::hayro_cmap::BfString::Char(c)) => c.to_string(),
                Some(hayro::hayro_interpret::hayro_cmap::BfString::String(s)) => s,
                None => String::new(),
            };
            let corners = [
                t * Point::new(0.0, 800.0),
                t * Point::new(adv, 800.0),
                t * Point::new(0.0, -200.0),
                t * Point::new(adv, -200.0),
            ];
            let size = ((t * Point::new(0.0, 1000.0)) - (t * Point::ZERO)).hypot();
            let body = bounds_of(&[
                t * Point::new(0.0, -120.0),
                t * Point::new(adv, -120.0),
                t * Point::new(0.0, 700.0),
                t * Point::new(adv, 700.0),
            ]);
            let quad = [
                corners[0].x as f32,
                corners[0].y as f32,
                corners[1].x as f32,
                corners[1].y as f32,
                corners[2].x as f32,
                corners[2].y as f32,
                corners[3].x as f32,
                corners[3].y as f32,
            ];
            // The text layer leaves out glyphs without size; so does the probe,
            // but they still count among the glyphs of their operation.
            self.out.glyphs.push(ProbeGlyph {
                op,
                chain: chain.clone(),
                text: if size.is_finite() && size >= 0.01 {
                    text
                } else {
                    String::from('\u{0}')
                },
                quad,
                body,
                to_next: to_next[i],
                own_advance: own,
                covered,
            });
        }
    }
    fn draw_image(&mut self, image: Image<'a, '_>, props: ImageDrawProps<'a>) {
        // The transform places the pixels of the picture.
        let (w, h) = (image.width() as f64, image.height() as f64);
        let corners =
            [(0.0, 0.0), (w, 0.0), (0.0, h), (w, h)].map(|(x, y)| props.transform * Point::new(x, y));
        let marker = self.stack.iter().rev().find_map(|f| match f {
            Frame::Do(m) => Some(Marker::Do(*m)),
            Frame::Im(m) => Some(Marker::Inline(*m)),
            _ => None,
        });
        let mut chain = self.chain();
        if let Some(Marker::Do(m)) = marker {
            if chain.last() == Some(&m) {
                chain.pop();
            }
        }
        self.out.images.push(ProbeImage {
            bounds: bounds_of(&corners),
            chain,
            marker,
            covered: self.covered(),
        });
    }
    fn pop_clip(&mut self) {}
    fn pop_transparency_group(&mut self) {}
    fn begin_marked_content(&mut self, tag: &[u8], mcid: Option<i32>) {
        let m = mcid.map(|m| m.max(0) as usize);
        self.stack.push(match (tag, m) {
            (TX, Some(m)) => Frame::Tx(m),
            (DO, Some(m)) => Frame::Do(m),
            (IM, Some(m)) => Frame::Im(m),
            (CV, _) => Frame::Cover,
            _ => Frame::Other,
        });
    }
    fn end_marked_content(&mut self) {
        self.stack.pop();
    }
}

/// A number operand.
fn number(data: &[u8], r: (usize, usize)) -> Option<f64> {
    std::str::from_utf8(data.get(r.0..r.1)?).ok()?.trim().parse().ok()
}

/// A stream wrapped with markers. `covers` are byte ranges of `data` that
/// are not content. Returns the new data and the `Do` names in order.
fn wrap(
    data: &[u8],
    key: StreamKey,
    ids: &mut ProbeIds,
    reveal: bool,
    covers: &[(usize, usize)],
) -> (Vec<u8>, Vec<Vec<u8>>) {
    let mut out = Vec::with_capacity(data.len() + data.len() / 4 + 64);
    let mut names = Vec::new();
    let mut text = 0usize;
    let mut in_cover: Option<usize> = None;
    for (i, op) in operations(data).iter().enumerate() {
        let cover = covers.iter().position(|(a, b)| op.start >= *a && op.start < *b);
        if cover != in_cover {
            if in_cover.is_some() {
                out.extend_from_slice(b"EMC\n");
            }
            if cover.is_some() {
                out.extend_from_slice(b"/FoliCv BMC\n");
            }
            in_cover = cover;
        }
        let raw = &data[op.start..op.end];
        match op.operator {
            o if SHOW.contains(&o) => {
                out.extend_from_slice(format!("/FoliTx <</MCID {}>> BDC ", ids.text.len()).as_bytes());
                ids.text.push((key, text));
                text += 1;
                out.extend_from_slice(raw);
                out.extend_from_slice(b" EMC\n");
            }
            b"Do" => {
                out.extend_from_slice(format!("/FoliDo <</MCID {}>> BDC ", ids.draws.len()).as_bytes());
                ids.draws.push((key, i));
                out.extend_from_slice(raw);
                out.extend_from_slice(b" EMC\n");
                if let Some(n) = op.operands.first().and_then(|r| name_of(data, *r)) {
                    names.push(n);
                }
            }
            b"BI" => {
                out.extend_from_slice(format!("/FoliIm <</MCID {}>> BDC\n", ids.inline.len()).as_bytes());
                ids.inline.push((key, i));
                out.extend_from_slice(raw);
                out.extend_from_slice(b"\nEMC\n");
            }
            b"BDC" if reveal => out.extend_from_slice(b"/FoliMc BMC\n"),
            b"BMC" if reveal => {
                out.extend_from_slice(b"/");
                out.extend_from_slice(NEUTRAL);
                out.extend_from_slice(b" BMC\n");
            }
            b"Tr" if reveal && op.operands.len() == 1 && number(data, op.operands[0]) == Some(7.0) => {
                out.extend_from_slice(b"3 Tr\n");
            }
            _ => {
                out.extend_from_slice(raw);
                out.push(b'\n');
            }
        }
    }
    if in_cover.is_some() {
        out.extend_from_slice(b"EMC\n");
    }
    (out, names)
}

/// The resources a content stream uses: the form's own, or those it inherits.
pub(crate) fn stream_resources(doc: &lopdf::Document, stream: &Stream, parent: &Dictionary) -> Dictionary {
    stream
        .dict
        .get(b"Resources")
        .ok()
        .and_then(|r| deref(doc, r).as_dict().ok())
        .cloned()
        .unwrap_or_else(|| parent.clone())
}

/// The resources of a page, inherited from its parents when needed.
pub(crate) fn page_resources(doc: &lopdf::Document, page_id: ObjectId) -> Dictionary {
    let mut cur = Some(page_id);
    for _ in 0..64 {
        let Some(id) = cur else { break };
        let Ok(d) = doc.get_dictionary(id) else { break };
        if let Ok(r) = d.get(b"Resources") {
            return deref(doc, r).as_dict().cloned().unwrap_or_default();
        }
        cur = d.get(b"Parent").ok().and_then(|p| p.as_reference().ok());
    }
    Dictionary::new()
}

/// The XObject a name of `res` stands for, when it is an indirect object.
pub(crate) fn xobject_id(doc: &lopdf::Document, res: &Dictionary, name: &[u8]) -> Option<ObjectId> {
    let xo = deref(doc, res.get(b"XObject").ok()?).as_dict().ok()?;
    xo.get(name).ok()?.as_reference().ok()
}

/// The form of a stream object (Subtype Form).
pub(crate) fn is_form(doc: &lopdf::Document, id: ObjectId) -> bool {
    doc.get_object(id)
        .and_then(Object::as_stream)
        .ok()
        .and_then(|s| s.dict.get(b"Subtype").ok()?.as_name().ok())
        == Some(b"Form".as_slice())
}

/// The data of a stream, decoded when it can be.
pub(crate) fn stream_data(s: &Stream) -> Vec<u8> {
    s.decompressed_content().unwrap_or_else(|_| s.content.clone())
}

/// Byte ranges of the joined content of a page that come from streams
/// tagged `/FolitectaMark tag` (not content: covers).
pub(crate) fn tagged_ranges(doc: &lopdf::Document, page_id: ObjectId, tag: &[u8]) -> Vec<(usize, usize)> {
    let j = joined(doc, page_id);
    let mut out = Vec::new();
    for (k, id) in j.ids.iter().enumerate() {
        let tagged = doc
            .get_object(*id)
            .and_then(Object::as_stream)
            .ok()
            .and_then(|s| s.dict.get(b"FolitectaMark").ok()?.as_name().ok())
            == Some(tag);
        if tagged {
            let end = j.starts.get(k + 1).copied().unwrap_or(j.data.len());
            out.push((j.starts[k], end));
        }
    }
    out
}

/// Draw `pages` of `doc` with probes. `cover_tag` names the streams
/// (`/FolitectaMark`) whose drawing is not content.
pub(crate) fn probe_pages(
    doc: &lopdf::Document,
    pages: &[usize],
    opts: ProbeOptions,
    cover_tag: Option<&[u8]>,
) -> Result<Probe> {
    let mut copy = doc.clone();
    let page_ids: Vec<ObjectId> = copy.get_pages().into_values().collect();
    if opts.reveal {
        // No XObject is optional any more.
        for obj in copy.objects.values_mut() {
            if let Object::Stream(s) = obj {
                let sub = s.dict.get(b"Subtype").ok().and_then(|v| v.as_name().ok());
                if matches!(sub, Some(b"Form") | Some(b"Image")) {
                    s.dict.remove(b"OC");
                }
            }
        }
    }
    let mut ids = ProbeIds::default();
    let mut queue: VecDeque<(ObjectId, Dictionary)> = VecDeque::new();
    for &p in pages {
        let Some(&pid) = page_ids.get(p) else {
            continue;
        };
        let j = joined(&copy, pid);
        let covers = match cover_tag {
            Some(tag) => tagged_ranges(&copy, pid, tag),
            None => Vec::new(),
        };
        let (data, names) = wrap(&j.data, StreamKey::Page(p), &mut ids, opts.reveal, &covers);
        let sid = copy.add_object(Stream::new(dictionary! {}, data));
        copy.get_dictionary_mut(pid)?
            .set("Contents", Object::Reference(sid));
        if opts.forms {
            let res = page_resources(&copy, pid);
            for n in names {
                if let Some(id) = xobject_id(&copy, &res, &n) {
                    queue.push_back((id, res.clone()));
                }
            }
        }
    }
    let mut done: HashSet<ObjectId> = HashSet::new();
    while let Some((fid, parent)) = queue.pop_front() {
        if !is_form(&copy, fid) || !done.insert(fid) || done.len() > 20_000 {
            continue;
        }
        let Ok(s) = copy.get_object(fid).and_then(Object::as_stream) else {
            continue;
        };
        let data = stream_data(s);
        let res = stream_resources(&copy, s, &parent);
        let (wrapped, names) = wrap(&data, StreamKey::Form(fid), &mut ids, opts.reveal, &[]);
        for n in names {
            if let Some(id) = xobject_id(&copy, &res, &n) {
                queue.push_back((id, res.clone()));
            }
        }
        if let Ok(Object::Stream(s)) = copy.get_object_mut(fid) {
            s.dict.remove(b"Filter");
            s.dict.remove(b"DecodeParms");
            s.set_plain_content(wrapped);
        }
    }
    let bytes = Arc::new(crate::document::serialize(&mut copy)?);
    let pdf = Pdf::new(bytes).map_err(|e| Error::Render(format!("cannot parse document: {e:?}")))?;
    let all = pdf.pages();
    let mut out = Probe {
        ids,
        pages: HashMap::new(),
    };
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
            hayro::kurbo::Rect::new(0.0, 0.0, w as f64, h as f64),
            &cache,
            page.xref(),
            settings,
        );
        let mut dev = ProbeDevice::default();
        let drawn = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            interpret_page(page, &mut ctx, &mut dev);
        }));
        if drawn.is_ok() {
            out.pages.insert(p, dev.out);
        }
    }
    Ok(out)
}

/// Draw a page whose content is `data` (its streams joined) with probes on
/// its text-showing operations; returns the glyphs in drawing order, each
/// with the index of its operation among those of the page (None for glyphs
/// of the drawings the page reuses).
pub(crate) fn probe_page_glyphs(doc: &lopdf::Document, page: usize) -> Result<Vec<ProbeGlyph>> {
    let probe = probe_pages(doc, &[page], ProbeOptions::default(), None)?;
    let ids = probe.ids;
    let mut glyphs = probe
        .pages
        .into_iter()
        .next()
        .map(|(_, p)| p.glyphs)
        .ok_or_else(|| Error::Render(format!("the renderer failed on page {}", page + 1)))?;
    for g in &mut glyphs {
        g.op = g.op.and_then(|m| match ids.text.get(m) {
            Some((StreamKey::Page(_), k)) => Some(*k),
            _ => None,
        });
    }
    Ok(glyphs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn streams_are_wrapped_and_revealed() {
        let mut ids = ProbeIds::default();
        let data = b"/OC /L1 BDC 7 Tr BT (a) Tj ET EMC /Im1 Do BI /W 1 /H 1 /CS /G /BPC 8 ID \x00 EI";
        let (out, names) = wrap(data, StreamKey::Page(0), &mut ids, true, &[]);
        let s = String::from_utf8_lossy(&out);
        assert!(s.contains("/FoliMc BMC"), "{s}");
        assert!(!s.contains("/OC /L1"), "{s}");
        assert!(s.contains("3 Tr") && !s.contains("7 Tr"), "{s}");
        assert!(s.contains("/FoliTx <</MCID 0>> BDC (a) Tj EMC"), "{s}");
        assert!(s.contains("/FoliDo <</MCID 0>> BDC /Im1 Do EMC"), "{s}");
        assert!(s.contains("/FoliIm <</MCID 0>> BDC"), "{s}");
        assert_eq!(names, vec![b"Im1".to_vec()]);
        assert_eq!(ids.text, vec![(StreamKey::Page(0), 0)]);
        assert_eq!(ids.draws, vec![(StreamKey::Page(0), 6)]);
        assert_eq!(ids.inline, vec![(StreamKey::Page(0), 7)]);
        // Without revealing, layers and modes stay.
        let (out, _) = wrap(data, StreamKey::Page(0), &mut ProbeIds::default(), false, &[]);
        let s = String::from_utf8_lossy(&out);
        assert!(s.contains("/OC /L1 BDC") && s.contains("7 Tr"), "{s}");
        // Covers are marked.
        let (out, _) = wrap(
            b"q 0 0 1 1 re f Q",
            StreamKey::Page(0),
            &mut ids,
            false,
            &[(0, 16)],
        );
        assert!(String::from_utf8_lossy(&out).starts_with("/FoliCv BMC"));
    }
}
