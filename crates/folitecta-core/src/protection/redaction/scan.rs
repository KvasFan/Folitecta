//! Following the graphics of a content stream: its path objects, shadings,
//! pictures, forms, inline pictures, text objects and marked content, each
//! with the graphics state it is drawn in (matrix to page space, line width,
//! patterns, soft mask, bounds of the clip).

use hayro::kurbo::{Affine, PathEl, Point, Rect};
use lopdf::{Dictionary, Object, ObjectId};

use crate::content_lex::{name_of, operations};
use crate::textedit::TextState;
use crate::util::deref;

/// Fill rule of a fill or a clip.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FillRule {
    NonZero,
    EvenOdd,
}

/// The graphics state an object is drawn in.
#[derive(Debug, Clone)]
pub(crate) struct Gs {
    /// From the user space of the stream to the default space of the page.
    pub ctm: Affine,
    pub lw: f64,
    /// Line cap (0 butt, 1 round, 2 projecting square).
    pub cap: i64,
    /// Line join (0 miter, 1 round, 2 bevel) and miter limit.
    pub join: i64,
    pub miter: f64,
    /// The pattern of the fill and stroke colours (a name of `/Pattern`).
    pub fill_pattern: Option<Vec<u8>>,
    pub stroke_pattern: Option<Vec<u8>>,
    /// The group of an active soft mask and the matrix it was set with.
    pub smask: Option<(ObjectId, Affine)>,
    /// Bounds of the clip in page space.
    pub clip: Rect,
    /// The text state (forms start from it).
    pub text: TextState,
}

/// How a path object is painted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Paint {
    /// `n`: not painted.
    None,
    Stroke,
    Fill(FillRule),
    FillStroke(FillRule),
}

/// A subpath in the user space of the stream.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Subpath {
    pub els: Vec<PathEl>,
    /// The operands of `re` when the subpath is a rectangle written so.
    pub re: Option<[f64; 4]>,
}

/// A path object: construction operations and the operation painting it.
#[derive(Debug, Clone)]
pub(crate) struct PathObj {
    /// Bytes from the first construction operation to the end of the painting one.
    pub start: usize,
    pub end: usize,
    pub subpaths: Vec<Subpath>,
    pub paint: Paint,
    /// The path is also a clip (`W` or `W*`).
    pub clip: Option<FillRule>,
    pub gs: Gs,
    /// Malformed (a segment without a current point, an operation of another
    /// kind before the painting one): never painted as written.
    pub uncertain: bool,
}

/// The property list of a marked-content sequence.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Props {
    None,
    /// An inline dictionary: its byte range.
    Inline((usize, usize)),
    /// A name of `/Properties`.
    Named(Vec<u8>),
}

/// A marked-content sequence.
#[derive(Debug, Clone)]
pub(crate) struct Marked {
    /// Byte range of its `BDC`/`BMC` operation.
    pub begin: (usize, usize),
    /// Byte range of its `EMC` (None when the stream ends first).
    pub end: Option<(usize, usize)>,
    pub tag: Vec<u8>,
    pub props: Props,
    pub mcid: Option<i64>,
}

impl Marked {
    /// The sequence holds the bytes `a..b`.
    pub fn holds(&self, a: usize, b: usize) -> bool {
        a >= self.begin.1 && self.end.is_none_or(|e| b <= e.0)
    }
}

/// A drawing of an XObject (`Do`).
#[derive(Debug, Clone)]
pub(crate) struct Draw {
    /// Its index among the operations of the stream.
    pub op: usize,
    pub start: usize,
    pub end: usize,
    pub id: ObjectId,
    pub gs: Gs,
}

/// What a content stream draws.
#[derive(Debug, Default)]
pub(crate) struct Scan {
    pub paths: Vec<PathObj>,
    /// `sh` operations: byte range, shading name, state.
    pub shadings: Vec<(usize, usize, Vec<u8>, Gs)>,
    pub images: Vec<Draw>,
    pub forms: Vec<Draw>,
    /// Inline pictures: operation index, byte range of `BI … EI`, state.
    pub inline: Vec<(usize, usize, usize, Gs)>,
    pub marked: Vec<Marked>,
    /// End of each text object (`ET`) with the matrix there, in order.
    pub text_ends: Vec<(usize, Affine)>,
    /// Text-showing operations painted under a soft mask: byte range and the mask.
    pub masked_text: Vec<(usize, usize, (ObjectId, Affine))>,
}

fn num(data: &[u8], r: (usize, usize)) -> Option<f64> {
    std::str::from_utf8(data.get(r.0..r.1)?).ok()?.trim().parse().ok()
}

fn nums(data: &[u8], ops: &[(usize, usize)]) -> Option<Vec<f64>> {
    ops.iter().map(|r| num(data, *r)).collect()
}

fn sub_dict<'a>(doc: &'a lopdf::Document, res: &'a Dictionary, cat: &[u8]) -> Option<&'a Dictionary> {
    deref(doc, res.get(cat).ok()?).as_dict().ok()
}

/// The resource `name` of category `cat`.
pub(crate) fn resource<'a>(
    doc: &'a lopdf::Document,
    res: &'a Dictionary,
    cat: &[u8],
    name: &[u8],
) -> Option<&'a Object> {
    Some(deref(doc, sub_dict(doc, res, cat)?.get(name).ok()?))
}

/// A colour space name or resource that is a pattern space.
fn is_pattern_space(doc: &lopdf::Document, res: &Dictionary, name: &[u8]) -> bool {
    if name == b"Pattern" {
        return true;
    }
    match resource(doc, res, b"ColorSpace", name) {
        Some(Object::Name(n)) => n == b"Pattern",
        Some(Object::Array(a)) => a.first().and_then(|o| deref(doc, o).as_name().ok()) == Some(b"Pattern"),
        _ => false,
    }
}

/// The bounds of points.
pub(crate) fn rect_of_points(points: impl IntoIterator<Item = Point>) -> Option<Rect> {
    let mut r: Option<Rect> = None;
    for p in points {
        if !(p.x.is_finite() && p.y.is_finite()) {
            continue;
        }
        r = Some(match r {
            Some(r) => r.union_pt(p),
            None => Rect::from_points(p, p),
        });
    }
    r
}

/// Bounds of a subpath's control points in page space.
pub(crate) fn subpath_bounds(sp: &Subpath, m: Affine) -> Option<Rect> {
    rect_of_points(sp.els.iter().flat_map(|e| {
        let pts: Vec<Point> = match *e {
            PathEl::MoveTo(p) | PathEl::LineTo(p) => vec![p],
            PathEl::QuadTo(a, b) => vec![a, b],
            PathEl::CurveTo(a, b, c) => vec![a, b, c],
            PathEl::ClosePath => vec![],
        };
        pts.into_iter().map(move |p| m * p)
    }))
}

/// Bounds of a path object in page space (strokes widened by what their ink
/// reaches beyond the centre line: half the width, the corners of square
/// caps, miter joins up to their limit).
pub(crate) fn path_bounds(p: &PathObj) -> Option<Rect> {
    let mut r: Option<Rect> = None;
    for sp in &p.subpaths {
        if let Some(b) = subpath_bounds(sp, p.gs.ctm) {
            r = Some(r.map_or(b, |r| r.union(b)));
        }
    }
    let r = r?;
    let widened = matches!(p.paint, Paint::Stroke | Paint::FillStroke(_));
    if !widened {
        return Some(r);
    }
    let mut reach: f64 = 1.0;
    if p.gs.cap == 2 {
        reach = reach.max(std::f64::consts::SQRT_2);
    }
    let corners = p.subpaths.iter().any(|sp| {
        let ends = sp
            .els
            .iter()
            .filter(|e| !matches!(e, PathEl::MoveTo(_) | PathEl::ClosePath))
            .count();
        ends >= 2 || matches!(sp.els.last(), Some(PathEl::ClosePath))
    });
    if p.gs.join == 0 && corners {
        reach = reach.max(p.gs.miter);
    }
    let hw = (half_width_user(&p.gs) * stretches(p.gs.ctm).0 * reach).max(MIN_PEN);
    Some(r.inflate(hw, hw))
}

/// The largest and smallest stretch of the linear part of a matrix.
fn stretches(m: Affine) -> (f64, f64) {
    let c = m.as_coeffs();
    let (a, b, cc, d) = (c[0], c[1], c[2], c[3]);
    let s1 = a * a + b * b + cc * cc + d * d;
    let s2 = ((a * a + b * b - cc * cc - d * d).powi(2) + 4.0 * (a * cc + b * d).powi(2)).sqrt();
    (((s1 + s2) / 2.0).sqrt(), ((s1 - s2) / 2.0).max(0.0).sqrt())
}

/// The half width, in page space, under which no line is drawn thinner:
/// lines thinner than a device pixel are drawn one pixel wide.
pub(crate) const MIN_PEN: f64 = 0.5;

/// Half the line width in user space (lines thinner than a device pixel
/// are drawn wider: see [`MIN_PEN`]).
pub(crate) fn half_width_user(gs: &Gs) -> f64 {
    gs.lw.max(0.0) / 2.0
}

/// Follow a content stream drawn with `res` from the state `gs0`.
pub(crate) fn scan(doc: &lopdf::Document, data: &[u8], res: &Dictionary, gs0: &Gs) -> Scan {
    let mut out = Scan::default();
    let mut gs = gs0.clone();
    let mut stack: Vec<Gs> = Vec::new();
    // The path being built.
    let mut cur: Option<PathObj> = None;
    let mut point: Option<Point> = None;
    let mut start_point: Option<Point> = None;
    let mut pending_clip: Option<FillRule> = None;
    let mut marked_stack: Vec<usize> = Vec::new();
    let new_path = |gs: &Gs, start: usize| PathObj {
        start,
        end: start,
        subpaths: Vec::new(),
        paint: Paint::None,
        clip: None,
        gs: gs.clone(),
        uncertain: false,
    };
    for (i, op) in operations(data).iter().enumerate() {
        let o = op.operator;
        let n = || nums(data, &op.operands);
        let is_construction = matches!(o, b"m" | b"l" | b"c" | b"v" | b"y" | b"h" | b"re");
        let is_paint = matches!(
            o,
            b"S" | b"s" | b"f" | b"F" | b"f*" | b"B" | b"B*" | b"b" | b"b*" | b"n"
        );
        let is_clip = matches!(o, b"W" | b"W*");
        // An operation of another kind inside a path object ends it unpainted.
        if cur.is_some() && !is_construction && !is_paint && !is_clip {
            if let Some(mut p) = cur.take() {
                p.uncertain = true;
                out.paths.push(p);
            }
            point = None;
            pending_clip = None;
        }
        if is_construction {
            let p = cur.get_or_insert_with(|| new_path(&gs, op.start));
            p.end = op.end;
            match (o, n()) {
                (b"m", Some(v)) if v.len() == 2 => {
                    let q = Point::new(v[0], v[1]);
                    p.subpaths.push(Subpath {
                        els: vec![PathEl::MoveTo(q)],
                        re: None,
                    });
                    point = Some(q);
                    start_point = Some(q);
                }
                (b"re", Some(v)) if v.len() == 4 => {
                    let (x, y, w, h) = (v[0], v[1], v[2], v[3]);
                    p.subpaths.push(Subpath {
                        els: vec![
                            PathEl::MoveTo(Point::new(x, y)),
                            PathEl::LineTo(Point::new(x + w, y)),
                            PathEl::LineTo(Point::new(x + w, y + h)),
                            PathEl::LineTo(Point::new(x, y + h)),
                            PathEl::ClosePath,
                        ],
                        re: Some([x, y, w, h]),
                    });
                    point = Some(Point::new(x, y));
                    start_point = point;
                }
                (b"h", _) => {
                    match p.subpaths.last_mut() {
                        Some(sp) if point.is_some() => {
                            if !matches!(sp.els.last(), Some(PathEl::ClosePath)) {
                                sp.els.push(PathEl::ClosePath);
                            }
                            sp.re = None;
                        }
                        _ => p.uncertain = true,
                    }
                    point = start_point;
                }
                (op_name, Some(v)) => {
                    let Some(at) = point else {
                        p.uncertain = true;
                        continue;
                    };
                    let el = match (op_name, v.len()) {
                        (b"l", 2) => Some(PathEl::LineTo(Point::new(v[0], v[1]))),
                        (b"c", 6) => Some(PathEl::CurveTo(
                            Point::new(v[0], v[1]),
                            Point::new(v[2], v[3]),
                            Point::new(v[4], v[5]),
                        )),
                        // v: the current point is the first control point.
                        (b"v", 4) => Some(PathEl::CurveTo(
                            at,
                            Point::new(v[0], v[1]),
                            Point::new(v[2], v[3]),
                        )),
                        // y: the end point is the second control point.
                        (b"y", 4) => Some(PathEl::CurveTo(
                            Point::new(v[0], v[1]),
                            Point::new(v[2], v[3]),
                            Point::new(v[2], v[3]),
                        )),
                        _ => None,
                    };
                    let Some(el) = el else {
                        p.uncertain = true;
                        continue;
                    };
                    let end_pt = match el {
                        PathEl::LineTo(q) | PathEl::CurveTo(_, _, q) => q,
                        _ => at,
                    };
                    // After a closed subpath, a new one starts where it began.
                    let needs_new = p
                        .subpaths
                        .last()
                        .is_none_or(|sp| matches!(sp.els.last(), Some(PathEl::ClosePath)));
                    if needs_new {
                        p.subpaths.push(Subpath {
                            els: vec![PathEl::MoveTo(at)],
                            re: None,
                        });
                    }
                    if let Some(sp) = p.subpaths.last_mut() {
                        sp.els.push(el);
                        sp.re = None;
                    }
                    point = Some(end_pt);
                }
                _ => p.uncertain = true,
            }
            continue;
        }
        if is_clip {
            pending_clip = Some(if o == b"W*" {
                FillRule::EvenOdd
            } else {
                FillRule::NonZero
            });
            if let Some(p) = cur.as_mut() {
                p.end = op.end;
            }
            continue;
        }
        if is_paint {
            let mut p = cur.take().unwrap_or_else(|| new_path(&gs, op.start));
            p.end = op.end;
            p.clip = pending_clip.take();
            let rule = if o.ends_with(b"*") {
                FillRule::EvenOdd
            } else {
                FillRule::NonZero
            };
            p.paint = match o {
                b"S" | b"s" => Paint::Stroke,
                b"f" | b"F" | b"f*" => Paint::Fill(rule),
                b"B" | b"B*" | b"b" | b"b*" => Paint::FillStroke(rule),
                _ => Paint::None,
            };
            // s, b and b* close the last subpath only (as h does).
            if matches!(o, b"s" | b"b" | b"b*") {
                if let Some(sp) = p.subpaths.last_mut() {
                    if !matches!(sp.els.last(), Some(PathEl::ClosePath)) {
                        sp.els.push(PathEl::ClosePath);
                        sp.re = None;
                    }
                }
            }
            if let Some(_rule) = p.clip {
                // The clip takes effect after painting: later objects are
                // drawn within its bounds.
                let b = path_bounds(&PathObj {
                    paint: Paint::None,
                    ..p.clone()
                });
                gs.clip = match b {
                    Some(b) => gs.clip.intersect(b),
                    None => Rect::ZERO,
                };
            }
            if !p.subpaths.is_empty() || p.clip.is_some() {
                out.paths.push(p);
            }
            point = None;
            continue;
        }
        match o {
            b"q" => stack.push(gs.clone()),
            b"Q" => {
                if let Some(g) = stack.pop() {
                    gs = g;
                }
            }
            b"cm" => {
                if let Some(v) = n().filter(|v| v.len() == 6) {
                    gs.ctm *= Affine::new([v[0], v[1], v[2], v[3], v[4], v[5]]);
                }
            }
            b"w" => {
                if let Some(v) = n().filter(|v| v.len() == 1) {
                    gs.lw = v[0];
                }
            }
            b"j" => {
                if let Some(v) = n().filter(|v| v.len() == 1) {
                    gs.join = v[0] as i64;
                }
            }
            b"J" => {
                if let Some(v) = n().filter(|v| v.len() == 1) {
                    gs.cap = v[0] as i64;
                }
            }
            b"M" => {
                if let Some(v) = n().filter(|v| v.len() == 1) {
                    gs.miter = v[0];
                }
            }
            b"gs" => {
                let Some(name) = op.operands.first().and_then(|r| name_of(data, *r)) else {
                    continue;
                };
                let Some(Object::Dictionary(d)) = resource(doc, res, b"ExtGState", &name) else {
                    continue;
                };
                let get = |k: &[u8]| {
                    d.get(k).ok().map(|v| deref(doc, v)).and_then(|v| match v {
                        Object::Integer(i) => Some(*i as f64),
                        Object::Real(r) => Some(*r as f64),
                        _ => None,
                    })
                };
                if let Some(v) = get(b"LW") {
                    gs.lw = v;
                }
                if let Some(v) = get(b"LJ") {
                    gs.join = v as i64;
                }
                if let Some(v) = get(b"LC") {
                    gs.cap = v as i64;
                }
                if let Some(v) = get(b"ML") {
                    gs.miter = v;
                }
                match d.get(b"SMask").ok().map(|v| deref(doc, v)) {
                    Some(Object::Name(_)) => gs.smask = None,
                    Some(Object::Dictionary(m)) => {
                        gs.smask = m
                            .get(b"G")
                            .ok()
                            .and_then(|g| g.as_reference().ok())
                            .map(|g| (g, gs.ctm));
                    }
                    _ => {}
                }
            }
            b"cs" | b"CS" => {
                let pattern = op
                    .operands
                    .first()
                    .and_then(|r| name_of(data, *r))
                    .is_some_and(|nm| is_pattern_space(doc, res, &nm));
                let slot = if o == b"cs" {
                    &mut gs.fill_pattern
                } else {
                    &mut gs.stroke_pattern
                };
                // A pattern space without a pattern yet paints nothing.
                *slot = pattern.then(Vec::new);
            }
            b"scn" | b"SCN" => {
                let name = op.operands.last().and_then(|r| name_of(data, *r));
                let slot = if o == b"scn" {
                    &mut gs.fill_pattern
                } else {
                    &mut gs.stroke_pattern
                };
                *slot = name;
            }
            b"sc" | b"rg" | b"g" | b"k" => gs.fill_pattern = None,
            b"SC" | b"RG" | b"G" | b"K" => gs.stroke_pattern = None,
            b"sh" => {
                if let Some(name) = op.operands.first().and_then(|r| name_of(data, *r)) {
                    out.shadings.push((op.start, op.end, name, gs.clone()));
                }
            }
            b"Do" => {
                let Some(name) = op.operands.first().and_then(|r| name_of(data, *r)) else {
                    continue;
                };
                let Some(id) = sub_dict(doc, res, b"XObject")
                    .and_then(|x| x.get(&name).ok())
                    .and_then(|v| v.as_reference().ok())
                else {
                    continue;
                };
                let sub = doc
                    .get_object(id)
                    .and_then(Object::as_stream)
                    .ok()
                    .and_then(|s| s.dict.get(b"Subtype").ok()?.as_name().ok().map(<[u8]>::to_vec));
                let draw = Draw {
                    op: i,
                    start: op.start,
                    end: op.end,
                    id,
                    gs: gs.clone(),
                };
                match sub.as_deref() {
                    Some(b"Image") => out.images.push(draw),
                    Some(b"Form") => out.forms.push(draw),
                    _ => {}
                }
            }
            b"BI" => out.inline.push((i, op.start, op.end, gs.clone())),
            b"BDC" | b"BMC" => {
                let tag = op
                    .operands
                    .first()
                    .and_then(|r| name_of(data, *r))
                    .unwrap_or_default();
                let (props, mcid) = match op.operands.get(1) {
                    Some(r) if data.get(r.0) == Some(&b'/') => {
                        let name = name_of(data, *r).unwrap_or_default();
                        let mcid = resource(doc, res, b"Properties", &name)
                            .and_then(|d| d.as_dict().ok())
                            .and_then(|d| d.get(b"MCID").ok())
                            .and_then(|m| m.as_i64().ok());
                        (Props::Named(name), mcid)
                    }
                    Some(r) if data.get(r.0..r.0 + 2) == Some(b"<<") => {
                        (Props::Inline(*r), inline_mcid(&data[r.0..r.1]))
                    }
                    _ => (Props::None, None),
                };
                marked_stack.push(out.marked.len());
                out.marked.push(Marked {
                    begin: (op.start, op.end),
                    end: None,
                    tag,
                    props,
                    mcid,
                });
            }
            b"EMC" => {
                if let Some(k) = marked_stack.pop() {
                    out.marked[k].end = Some((op.start, op.end));
                }
            }
            b"ET" => out.text_ends.push((op.end, gs.ctm)),
            b"Tj" | b"TJ" | b"'" | b"\"" => {
                gs.text.apply(data, o, &op.operands);
                if let Some(m) = gs.smask {
                    out.masked_text.push((op.start, op.end, m));
                }
            }
            _ => gs.text.apply(data, o, &op.operands),
        }
    }
    if let Some(mut p) = cur.take() {
        p.uncertain = true;
        out.paths.push(p);
    }
    out
}

/// The `/MCID` of an inline property list.
fn inline_mcid(dict: &[u8]) -> Option<i64> {
    let at = dict.windows(5).position(|w| w == b"/MCID")?;
    let rest = &dict[at + 5..];
    let digits: Vec<u8> = rest
        .iter()
        .skip_while(|c| c.is_ascii_whitespace())
        .take_while(|c| c.is_ascii_digit())
        .copied()
        .collect();
    std::str::from_utf8(&digits).ok()?.parse().ok()
}

/// A state at the start of a page: identity matrix, clip to `clip`.
pub(crate) fn page_state(clip: Rect) -> Gs {
    Gs {
        ctm: Affine::IDENTITY,
        lw: 1.0,
        cap: 0,
        join: 0,
        miter: 10.0,
        fill_pattern: None,
        stroke_pattern: None,
        smask: None,
        clip,
        text: TextState::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(data: &[u8]) -> Scan {
        let doc = lopdf::Document::with_version("1.7");
        scan(
            &doc,
            data,
            &Dictionary::new(),
            &page_state(Rect::new(0.0, 0.0, 600.0, 800.0)),
        )
    }

    #[test]
    fn matrices_follow_q_and_q() {
        let s = run(b"q 2 0 0 2 10 10 cm 0 0 m 5 0 l S Q 0 0 m 5 0 l S");
        assert_eq!(s.paths.len(), 2);
        assert_eq!(s.paths[0].gs.ctm, Affine::new([2.0, 0.0, 0.0, 2.0, 10.0, 10.0]));
        assert_eq!(s.paths[1].gs.ctm, Affine::IDENTITY);
        assert_eq!(s.paths[0].paint, Paint::Stroke);
    }

    #[test]
    fn rectangles_and_curves() {
        let s = run(b"10 20 30 40 re f 0 0 m 1 1 2 2 v 3 3 4 4 y h b*");
        assert_eq!(s.paths[0].subpaths[0].re, Some([10.0, 20.0, 30.0, 40.0]));
        assert_eq!(s.paths[0].subpaths[0].els.len(), 5);
        assert_eq!(s.paths[0].paint, Paint::Fill(FillRule::NonZero));
        let els = &s.paths[1].subpaths[0].els;
        assert_eq!(
            els[1],
            PathEl::CurveTo(Point::new(0.0, 0.0), Point::new(1.0, 1.0), Point::new(2.0, 2.0))
        );
        assert_eq!(
            els[2],
            PathEl::CurveTo(Point::new(3.0, 3.0), Point::new(4.0, 4.0), Point::new(4.0, 4.0))
        );
        assert_eq!(s.paths[1].paint, Paint::FillStroke(FillRule::EvenOdd));
    }

    #[test]
    fn clips_and_malformed_paths() {
        // A segment without a current point builds nothing; a path left
        // unpainted at the end is malformed.
        let s = run(b"0 0 100 100 re W n 5 5 l S 0 0 m 1 1 l");
        assert_eq!(s.paths.len(), 2);
        assert_eq!(s.paths[0].clip, Some(FillRule::NonZero));
        assert_eq!(s.paths[0].paint, Paint::None);
        assert!(s.paths[1].uncertain);
        // The clip bounds the objects after it.
        let s = run(b"0 0 100 100 re W n 0 0 m 500 500 l S");
        assert_eq!(s.paths[1].gs.clip, Rect::new(0.0, 0.0, 100.0, 100.0));
    }

    #[test]
    fn s_closes_the_last_subpath_only() {
        let s = run(b"0 0 m 10 0 l 20 0 m 30 0 l 30 10 l s");
        let p = &s.paths[0];
        assert!(!matches!(p.subpaths[0].els.last(), Some(PathEl::ClosePath)));
        assert!(matches!(p.subpaths[1].els.last(), Some(PathEl::ClosePath)));
    }

    #[test]
    fn line_width_from_graphics_states() {
        let mut doc = lopdf::Document::with_version("1.7");
        let gs_id = doc.add_object(lopdf::dictionary! { "LW" => 7 });
        let res = lopdf::dictionary! { "ExtGState" => lopdf::dictionary! { "G1" => gs_id } };
        let s = scan(
            &doc,
            b"/G1 gs 0 0 m 1 1 l S",
            &res,
            &page_state(Rect::new(0.0, 0.0, 10.0, 10.0)),
        );
        assert_eq!(s.paths[0].gs.lw, 7.0);
    }

    #[test]
    fn marked_content_and_text_ends() {
        let s = run(b"/Span <</MCID 4 /ActualText (x)>> BDC BT (a) Tj ET EMC /P /Pr1 BDC EMC");
        assert_eq!(s.marked.len(), 2);
        assert_eq!(s.marked[0].mcid, Some(4));
        assert!(matches!(s.marked[0].props, Props::Inline(_)));
        assert_eq!(s.marked[1].props, Props::Named(b"Pr1".to_vec()));
        assert_eq!(s.text_ends.len(), 1);
    }
}
