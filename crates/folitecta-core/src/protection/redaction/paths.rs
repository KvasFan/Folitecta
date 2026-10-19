//! Paths under redaction marks lose their geometry there. Marks are
//! rectangles of the default space of the page (axis-aligned there); paths are
//! examined in that space and written back in their own.
//!
//! - **Fills.** A subpath whose region meets a mark (a vertex inside it, an
//!   edge through it, or the mark inside the region) is flattened and becomes
//!   *subpath minus mark*: its contour clipped (Sutherland–Hodgman) to the four
//!   bands around the mark. The bands are disjoint and convex, so inside a
//!   band each piece winds as the original did and outside it not at all: the
//!   pieces filled with the same rule paint exactly the region minus the mark,
//!   for both rules, and in one path no seams show.
//! - **Clips.** A clip keeps its geometry unless a vertex or an edge is inside
//!   a mark (a clip around the mark tells nothing about it); otherwise it is
//!   cut the same way. A clip that becomes empty is written as a small square
//!   far outside the page, so what it hid stays hidden for every reader.
//! - **Strokes.** The centre line loses the parts inside the marks widened by
//!   half the line width (Liang–Barsky), so no ink stays inside.

use hayro::kurbo::{self, Affine, BezPath, PathEl, Point, Rect};
use lopdf::{Dictionary, Object};

use super::scan::{FillRule, Paint, PathObj, Subpath, half_width, path_bounds};
use crate::util::deref;

/// Flattening tolerance in page space (points).
const TOLERANCE: f64 = 0.05;
/// A subpath flattened to more points is dropped whole where it meets a mark.
const MAX_POINTS: usize = 100_000;
/// Margin that makes "inside" strict.
const EPS: f64 = 1e-6;

/// A number written in a content stream: at most 6 decimals, never `-0`.
pub(crate) fn fmt_coord(v: f64) -> String {
    if !v.is_finite() {
        return "0".into();
    }
    let s = format!("{v:.6}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" || s.is_empty() {
        "0".into()
    } else {
        s.to_string()
    }
}

fn strictly_inside(p: Point, r: Rect) -> bool {
    p.x > r.x0 + EPS && p.x < r.x1 - EPS && p.y > r.y0 + EPS && p.y < r.y1 - EPS
}

/// The parameter interval of segment `a`–`b` inside `r` (Liang–Barsky).
fn segment_in_rect(a: Point, b: Point, r: Rect) -> Option<(f64, f64)> {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let mut t0: f64 = 0.0;
    let mut t1: f64 = 1.0;
    for (p, q) in [
        (-dx, a.x - r.x0),
        (dx, r.x1 - a.x),
        (-dy, a.y - r.y0),
        (dy, r.y1 - a.y),
    ] {
        if p.abs() < 1e-15 {
            if q < 0.0 {
                return None;
            }
        } else {
            let t = q / p;
            if p < 0.0 {
                t0 = t0.max(t);
            } else {
                t1 = t1.min(t);
            }
        }
    }
    (t0 <= t1).then_some((t0, t1))
}

/// The open interior of `r` (shrunk a little).
fn interior(r: Rect) -> Rect {
    Rect::new(r.x0 + EPS, r.y0 + EPS, r.x1 - EPS, r.y1 - EPS)
}

/// Some edge of the closed polygon (or polyline) goes through the interior of `r`.
fn edge_through(poly: &[Point], closed: bool, r: Rect) -> bool {
    let inner = interior(r);
    if inner.width() <= 0.0 || inner.height() <= 0.0 {
        return false;
    }
    let n = poly.len();
    let segs = if closed { n } else { n.saturating_sub(1) };
    (0..segs).any(|i| segment_in_rect(poly[i], poly[(i + 1) % n], inner).is_some())
}

/// Winding number of a closed polygon around `p`.
fn winding(poly: &[Point], p: Point) -> i32 {
    let n = poly.len();
    let mut w = 0;
    for i in 0..n {
        let (a, b) = (poly[i], poly[(i + 1) % n]);
        let cross = (b.x - a.x) * (p.y - a.y) - (p.x - a.x) * (b.y - a.y);
        if a.y <= p.y {
            if b.y > p.y && cross > 0.0 {
                w += 1;
            }
        } else if b.y <= p.y && cross < 0.0 {
            w -= 1;
        }
    }
    w
}

fn poly_bounds(poly: &[Point]) -> Option<Rect> {
    super::scan::rect_of_points(poly.iter().copied())
}

fn overlaps(a: Rect, b: Rect) -> bool {
    a.x0 < b.x1 && a.x1 > b.x0 && a.y0 < b.y1 && a.y1 > b.y0
}

/// The region of a closed contour meets `r`: a vertex inside it, an edge
/// through it, or `r` inside the region.
pub(crate) fn fill_meets(poly: &[Point], r: Rect) -> bool {
    let Some(b) = poly_bounds(poly) else {
        return false;
    };
    if !overlaps(b, r) {
        return false;
    }
    poly.iter().any(|p| strictly_inside(*p, r))
        || edge_through(poly, true, r)
        || (poly.len() >= 3 && winding(poly, r.center()) != 0)
}

/// The geometry of a clip contour is inside `r` (a vertex or an edge).
pub(crate) fn clip_meets(poly: &[Point], r: Rect) -> bool {
    poly.iter().any(|p| strictly_inside(*p, r)) || edge_through(poly, true, r)
}

/// The centre line of a polyline goes through `r` (or a lone point is inside).
pub(crate) fn line_meets(poly: &[Point], closed: bool, r: Rect) -> bool {
    match poly.len() {
        0 => false,
        1 => strictly_inside(poly[0], r),
        _ => edge_through(poly, closed, r),
    }
}

/// One side of a clipping rectangle: the axis (x or y), the limit and
/// whether points must be at least (true) or at most (false) at it.
#[derive(Clone, Copy)]
struct Side {
    x: bool,
    limit: f64,
    above: bool,
}

impl Side {
    fn inside(self, p: Point) -> bool {
        let v = if self.x { p.x } else { p.y };
        if self.above {
            v >= self.limit
        } else {
            v <= self.limit
        }
    }

    fn cut(self, a: Point, b: Point) -> Point {
        if self.x {
            let t = (self.limit - a.x) / (b.x - a.x);
            Point::new(self.limit, a.y + t * (b.y - a.y))
        } else {
            let t = (self.limit - a.y) / (b.y - a.y);
            Point::new(a.x + t * (b.x - a.x), self.limit)
        }
    }
}

/// Clip a polygon to a rectangle (Sutherland–Hodgman).
fn clip_to_rect(poly: &[Point], band: Rect) -> Vec<Point> {
    let mut out: Vec<Point> = poly.to_vec();
    let sides = [
        Side {
            x: true,
            limit: band.x0,
            above: true,
        },
        Side {
            x: true,
            limit: band.x1,
            above: false,
        },
        Side {
            x: false,
            limit: band.y0,
            above: true,
        },
        Side {
            x: false,
            limit: band.y1,
            above: false,
        },
    ];
    for side in sides {
        if out.is_empty() {
            break;
        }
        let input = std::mem::take(&mut out);
        let n = input.len();
        for i in 0..n {
            let cur = input[i];
            let prev = input[(i + n - 1) % n];
            match (side.inside(cur), side.inside(prev)) {
                (true, true) => out.push(cur),
                (true, false) => {
                    out.push(side.cut(prev, cur));
                    out.push(cur);
                }
                (false, true) => out.push(side.cut(prev, cur)),
                (false, false) => {}
            }
        }
    }
    out
}

#[cfg(test)]
fn area(poly: &[Point]) -> f64 {
    let n = poly.len();
    (0..n)
        .map(|i| {
            let (a, b) = (poly[i], poly[(i + 1) % n]);
            a.x * b.y - b.x * a.y
        })
        .sum::<f64>()
        / 2.0
}

/// A closed contour minus a rectangle, as pieces filled with the same rule.
fn minus_rect(poly: &[Point], r: Rect) -> Vec<Vec<Point>> {
    let Some(b) = poly_bounds(poly) else {
        return Vec::new();
    };
    let b = b.union(r).inflate(1.0, 1.0);
    let bands = [
        Rect::new(b.x0, r.y1, b.x1, b.y1),
        Rect::new(b.x0, b.y0, b.x1, r.y0),
        Rect::new(b.x0, r.y0, r.x0, r.y1),
        Rect::new(r.x1, r.y0, b.x1, r.y1),
    ];
    bands
        .iter()
        .filter(|band| band.width() > 0.0 && band.height() > 0.0)
        .map(|band| clip_to_rect(poly, *band))
        .filter(|p| p.len() >= 3 && poly_bounds(p).is_some_and(|b| b.width() > 0.0 && b.height() > 0.0))
        .collect()
}

/// A closed contour minus the marks it meets (`None` when it meets none).
/// `meets` decides which marks it meets.
pub(crate) fn subtract_marks(
    poly: &[Point],
    marks: &[Rect],
    meets: fn(&[Point], Rect) -> bool,
) -> Option<Vec<Vec<Point>>> {
    if !marks.iter().any(|m| meets(poly, *m)) {
        return None;
    }
    let mut pieces = vec![poly.to_vec()];
    for m in marks {
        let mut next = Vec::with_capacity(pieces.len());
        for p in pieces {
            if meets(&p, *m) {
                next.extend(minus_rect(&p, *m));
            } else {
                next.push(p);
            }
        }
        pieces = next;
        if pieces.iter().map(Vec::len).sum::<usize>() > MAX_POINTS {
            return Some(Vec::new());
        }
    }
    pieces.retain(|p| p.len() >= 3);
    Some(pieces)
}

/// A polyline minus the parts of its centre line inside the marks (`None`
/// when no part is inside). A closed polyline cut somewhere becomes open
/// pieces; the piece through its first point keeps its join there.
pub(crate) fn cut_polyline(pts: &[Point], closed: bool, marks: &[Rect]) -> Option<Vec<Vec<Point>>> {
    if pts.is_empty() {
        return None;
    }
    if pts.len() == 1 || pts.windows(2).all(|w| (w[0] - w[1]).hypot() < 1e-12) {
        // A dot: kept or gone.
        return marks.iter().any(|m| strictly_inside(pts[0], *m)).then(Vec::new);
    }
    if !marks.iter().any(|m| line_meets(pts, closed, *m)) {
        return None;
    }
    let n = pts.len();
    let segs: Vec<(Point, Point)> = (0..if closed { n } else { n - 1 })
        .map(|i| (pts[i], pts[(i + 1) % n]))
        .collect();
    let mut pieces: Vec<Vec<Point>> = Vec::new();
    let mut current: Vec<Point> = Vec::new();
    let mut first_starts_at_origin = false;
    for (si, (a, b)) in segs.iter().enumerate() {
        // Removed intervals of this segment, merged.
        let mut gone: Vec<(f64, f64)> = marks
            .iter()
            .filter_map(|m| segment_in_rect(*a, *b, *m))
            .filter(|(t0, t1)| t1 - t0 > 1e-12 || (a.x - b.x).abs() + (a.y - b.y).abs() < 1e-12)
            .collect();
        gone.sort_by(|x, y| x.0.total_cmp(&y.0));
        let mut kept: Vec<(f64, f64)> = Vec::new();
        let mut at = 0.0;
        for (t0, t1) in gone {
            if t0 > at {
                kept.push((at, t0));
            }
            at = at.max(t1);
        }
        if at < 1.0 {
            kept.push((at, 1.0));
        }
        let point = |t: f64| Point::new(a.x + t * (b.x - a.x), a.y + t * (b.y - a.y));
        for (k0, k1) in kept {
            if k0 > 0.0 || current.is_empty() {
                if current.len() >= 2 {
                    pieces.push(std::mem::take(&mut current));
                }
                current.clear();
                if si == 0 && k0 == 0.0 && pieces.is_empty() {
                    first_starts_at_origin = true;
                }
                current.push(point(k0));
            }
            current.push(point(k1));
            if k1 < 1.0 {
                pieces.push(std::mem::take(&mut current));
            }
        }
    }
    if current.len() >= 2 {
        // A closed line whose last piece reaches the first point joins the first piece.
        let reaches_origin = closed && current.last().is_some_and(|p| (*p - pts[0]).hypot() < 1e-9);
        if reaches_origin && first_starts_at_origin && !pieces.is_empty() {
            let first = pieces.remove(0);
            current.extend(first.into_iter().skip(1));
        }
        pieces.push(current);
    }
    pieces.retain(|p| p.len() >= 2);
    Some(pieces)
}

/// A subpath flattened in page space (closed ones without the repeated point).
pub(crate) fn flatten(sp: &Subpath, m: Affine) -> (Vec<Point>, bool) {
    let mut path = BezPath::new();
    for el in &sp.els {
        path.push(*el);
    }
    let path = m * path;
    let mut pts: Vec<Point> = Vec::new();
    let mut closed = false;
    kurbo::flatten(path.iter(), TOLERANCE, |el| match el {
        PathEl::MoveTo(p) => {
            if pts.is_empty() {
                pts.push(p)
            }
        }
        PathEl::LineTo(p) => {
            if pts.len() <= MAX_POINTS {
                pts.push(p)
            }
        }
        PathEl::ClosePath => closed = true,
        _ => {}
    });
    if closed && pts.len() > 1 && (pts[0] - pts[pts.len() - 1]).hypot() < 1e-9 {
        pts.pop();
    }
    (pts, closed)
}

/// Points written as a path (`m`, `l` … and `h` when closed), mapped by `inv`.
fn write_points(out: &mut String, pts: &[Point], inv: Affine, close: bool) {
    for (i, p) in pts.iter().enumerate() {
        let q = inv * *p;
        out.push_str(&format!(
            "{} {} {} ",
            fmt_coord(q.x),
            fmt_coord(q.y),
            if i == 0 { "m" } else { "l" }
        ));
    }
    if close {
        out.push_str("h ");
    }
}

/// A subpath written as it was built.
fn write_subpath(out: &mut String, sp: &Subpath) {
    if let Some([x, y, w, h]) = sp.re {
        out.push_str(&format!(
            "{} {} {} {} re ",
            fmt_coord(x),
            fmt_coord(y),
            fmt_coord(w),
            fmt_coord(h)
        ));
        return;
    }
    let mut last = Point::ZERO;
    for el in &sp.els {
        match *el {
            PathEl::MoveTo(p) => {
                out.push_str(&format!("{} {} m ", fmt_coord(p.x), fmt_coord(p.y)));
                last = p;
            }
            PathEl::LineTo(p) => {
                out.push_str(&format!("{} {} l ", fmt_coord(p.x), fmt_coord(p.y)));
                last = p;
            }
            PathEl::QuadTo(a, b) => {
                let c1 = last + (a - last) * (2.0 / 3.0);
                let c2 = b + (a - b) * (2.0 / 3.0);
                out.push_str(&format!(
                    "{} {} {} {} {} {} c ",
                    fmt_coord(c1.x),
                    fmt_coord(c1.y),
                    fmt_coord(c2.x),
                    fmt_coord(c2.y),
                    fmt_coord(b.x),
                    fmt_coord(b.y)
                ));
                last = b;
            }
            PathEl::CurveTo(a, b, c) => {
                out.push_str(&format!(
                    "{} {} {} {} {} {} c ",
                    fmt_coord(a.x),
                    fmt_coord(a.y),
                    fmt_coord(b.x),
                    fmt_coord(b.y),
                    fmt_coord(c.x),
                    fmt_coord(c.y)
                ));
                last = c;
            }
            PathEl::ClosePath => out.push_str("h "),
        }
    }
}

/// A clip that hides everything for every reader: a small square far
/// outside the page (in the user space mapped by `inv`).
pub(crate) fn empty_clip(inv: Affine) -> String {
    let corners = [
        Point::new(-20000.0, -20000.0),
        Point::new(-19999.0, -20000.0),
        Point::new(-19999.0, -19999.0),
        Point::new(-20000.0, -19999.0),
    ];
    let mut s = String::new();
    write_points(&mut s, &corners, inv, true);
    s.push_str("W n");
    s
}

/// How a path object changes under the marks.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum PathEdit {
    /// Nothing of it is under a mark.
    Keep,
    /// Written anew (empty: it goes).
    Replace(String),
}

/// The geometry of one use of a path (fill, clip) with its subpaths cut.
fn cut_region(
    p: &PathObj,
    flat: &[(Vec<Point>, bool)],
    marks: &[Rect],
    meets: fn(&[Point], Rect) -> bool,
) -> (String, bool) {
    let inv = p.gs.ctm.inverse();
    let mut out = String::new();
    let mut changed = false;
    for (sp, (pts, _)) in p.subpaths.iter().zip(flat) {
        if pts.len() > MAX_POINTS {
            if marks.iter().any(|m| fill_meets(&pts[..MAX_POINTS], *m)) {
                changed = true;
                continue;
            }
            write_subpath(&mut out, sp);
            continue;
        }
        match subtract_marks(pts, marks, meets) {
            None => write_subpath(&mut out, sp),
            Some(pieces) => {
                changed = true;
                for piece in pieces {
                    write_points(&mut out, &piece, inv, true);
                }
            }
        }
    }
    (out, changed)
}

/// The stroke of a path with its centre lines cut.
fn cut_stroke(p: &PathObj, flat: &[(Vec<Point>, bool)], marks: &[Rect]) -> (String, bool) {
    let inv = p.gs.ctm.inverse();
    let hw = half_width(&p.gs);
    let wide: Vec<Rect> = marks.iter().map(|m| m.inflate(hw, hw)).collect();
    let mut out = String::new();
    let mut changed = false;
    for (sp, (pts, closed)) in p.subpaths.iter().zip(flat) {
        if pts.len() > MAX_POINTS {
            changed = true;
            continue;
        }
        match cut_polyline(pts, *closed, &wide) {
            None => write_subpath(&mut out, sp),
            Some(pieces) => {
                changed = true;
                for piece in pieces {
                    write_points(&mut out, &piece, inv, false);
                }
            }
        }
    }
    (out, changed)
}

/// What becomes of a path object under `marks` (page space).
pub(crate) fn redact_path(p: &PathObj, marks: &[Rect]) -> PathEdit {
    let Some(bounds) = path_bounds(p) else {
        return PathEdit::Keep;
    };
    let hits: Vec<Rect> = marks.iter().copied().filter(|m| overlaps(bounds, *m)).collect();
    if hits.is_empty() {
        return PathEdit::Keep;
    }
    if p.uncertain {
        // Never painted as written: it goes.
        return PathEdit::Replace(String::new());
    }
    let m = p.gs.ctm;
    if m.determinant().abs() < 1e-12 {
        // Nothing of it shows; a clip under it hides everything.
        return PathEdit::Replace(if p.clip.is_some() {
            "0 0 m 1 0 l 1 1 l h W n".into()
        } else {
            String::new()
        });
    }
    let flat: Vec<(Vec<Point>, bool)> = p.subpaths.iter().map(|sp| flatten(sp, m)).collect();
    let rule_op = |r: FillRule| if r == FillRule::EvenOdd { "*" } else { "" };
    let mut out = String::new();
    let mut changed = false;
    match p.paint {
        Paint::None => {
            if p.clip.is_none() {
                // Geometry that paints nothing: it goes when it is under a mark.
                let under = flat.iter().any(|(pts, closed)| {
                    hits.iter()
                        .any(|r| clip_meets(pts, *r) || line_meets(pts, *closed, *r))
                });
                return if under {
                    PathEdit::Replace(String::new())
                } else {
                    PathEdit::Keep
                };
            }
        }
        Paint::Fill(rule) | Paint::FillStroke(rule) => {
            let (fill, c) = cut_region(p, &flat, &hits, fill_meets);
            changed |= c;
            if !fill.trim().is_empty() {
                out.push_str(&fill);
                out.push_str(&format!("f{} ", rule_op(rule)));
            }
            if p.paint != Paint::Fill(rule) {
                let (stroke, c) = cut_stroke(p, &flat, &hits);
                changed |= c;
                if !stroke.trim().is_empty() {
                    out.push_str(&stroke);
                    out.push_str("S ");
                }
            }
        }
        Paint::Stroke => {
            let (stroke, c) = cut_stroke(p, &flat, &hits);
            changed |= c;
            if !stroke.trim().is_empty() {
                out.push_str(&stroke);
                out.push_str("S ");
            }
        }
    }
    if let Some(rule) = p.clip {
        let (clip, c) = cut_region(p, &flat, &hits, clip_meets);
        changed |= c;
        if clip.trim().is_empty() {
            out.push_str(&empty_clip(m.inverse()));
        } else {
            out.push_str(&clip);
            out.push_str(&format!("W{} n", rule_op(rule)));
        }
    }
    if changed {
        PathEdit::Replace(out.trim_end().to_string())
    } else {
        PathEdit::Keep
    }
}

/// A clip that paints `area` (page space) except the marks, in the user
/// space mapped by `inv`; None when nothing of the area is left.
pub(crate) fn exclusion_clip(area: Rect, marks: &[Rect], inv: Affine) -> Option<(String, Vec<Vec<Point>>)> {
    let poly = vec![
        Point::new(area.x0, area.y0),
        Point::new(area.x1, area.y0),
        Point::new(area.x1, area.y1),
        Point::new(area.x0, area.y1),
    ];
    let pieces = subtract_marks(&poly, marks, fill_meets).unwrap_or_else(|| vec![poly.clone()]);
    if pieces.is_empty() {
        return None;
    }
    let mut s = String::new();
    for p in &pieces {
        write_points(&mut s, p, inv, true);
    }
    s.push_str("W n");
    Some((s, pieces))
}

fn number(doc: &lopdf::Document, o: &Object) -> Option<f64> {
    match deref(doc, o) {
        Object::Integer(i) => Some(*i as f64),
        Object::Real(r) => Some(*r as f64),
        _ => None,
    }
}

/// The functions of a shading are smooth (exponential interpolation, or
/// stitched from those): they hold a few colours, no data.
fn smooth_functions(doc: &lopdf::Document, f: &Object, depth: usize) -> bool {
    if depth > 8 {
        return false;
    }
    match deref(doc, f) {
        Object::Array(a) => a.iter().all(|x| smooth_functions(doc, x, depth + 1)),
        Object::Dictionary(d) => smooth_dict(doc, d, depth),
        Object::Stream(_) => false,
        _ => false,
    }
}

fn smooth_dict(doc: &lopdf::Document, d: &Dictionary, depth: usize) -> bool {
    match d.get(b"FunctionType").ok().and_then(|t| number(doc, t)) {
        Some(2.0) => true,
        Some(3.0) => d
            .get(b"Functions")
            .ok()
            .is_some_and(|f| smooth_functions(doc, f, depth + 1)),
        _ => false,
    }
}

/// Whether the colours a shading of type 2 or 3 paints under the marks are
/// painted outside them too, so a clip around the marks hides nothing that
/// cannot be seen elsewhere. `under` is the painted area under the marks and
/// `outside` the pieces painted outside (page space); `to_shading` maps page
/// space to the space of the shading.
pub(crate) fn shading_shown_elsewhere(
    doc: &lopdf::Document,
    shading: &Dictionary,
    to_shading: Affine,
    under: &[Rect],
    outside: &[Vec<Point>],
) -> bool {
    let kind = shading.get(b"ShadingType").ok().and_then(|t| number(doc, t));
    let coords: Vec<f64> = shading
        .get(b"Coords")
        .ok()
        .and_then(|c| deref(doc, c).as_array().ok())
        .map(|a| a.iter().filter_map(|x| number(doc, x)).collect())
        .unwrap_or_default();
    match kind {
        Some(k) if k == 2.0 && coords.len() == 4 => {
            let (p0, p1) = (Point::new(coords[0], coords[1]), Point::new(coords[2], coords[3]));
            let axis = p1 - p0;
            let len2 = axis.hypot2();
            if len2 < 1e-12 {
                return false;
            }
            let t = |p: Point| {
                let q = to_shading * p;
                ((q - p0).dot(axis) / len2).clamp(0.0, 1.0)
            };
            let range = |pts: &[Point]| -> Option<(f64, f64)> {
                let ts: Vec<f64> = pts.iter().map(|p| t(*p)).collect();
                let lo = ts.iter().copied().fold(f64::MAX, f64::min);
                let hi = ts.iter().copied().fold(f64::MIN, f64::max);
                (lo <= hi).then_some((lo, hi))
            };
            let mut shown: Vec<(f64, f64)> = outside.iter().filter_map(|p| range(p)).collect();
            shown.sort_by(|a, b| a.0.total_cmp(&b.0));
            // The union of what is shown outside.
            let mut merged: Vec<(f64, f64)> = Vec::new();
            for (a, b) in shown {
                match merged.last_mut() {
                    Some(last) if a <= last.1 + 1e-9 => last.1 = last.1.max(b),
                    _ => merged.push((a, b)),
                }
            }
            under.iter().all(|r| {
                let corners = [
                    Point::new(r.x0, r.y0),
                    Point::new(r.x1, r.y0),
                    Point::new(r.x1, r.y1),
                    Point::new(r.x0, r.y1),
                ];
                range(&corners)
                    .is_some_and(|(a, b)| merged.iter().any(|(x, y)| *x <= a + 1e-9 && *y >= b - 1e-9))
            })
        }
        Some(3.0) => shading
            .get(b"Function")
            .ok()
            .is_some_and(|f| smooth_functions(doc, f, 0)),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::super::scan::{page_state, scan};
    use super::*;

    fn square(x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<Point> {
        vec![
            Point::new(x0, y0),
            Point::new(x1, y0),
            Point::new(x1, y1),
            Point::new(x0, y1),
        ]
    }

    /// Area painted by pieces with the non-zero rule, measured on a grid.
    fn painted(pieces: &[Vec<Point>], r: Rect, even_odd: bool) -> f64 {
        let mut n = 0;
        let step = 0.5;
        let mut y = r.y0 + step / 2.0;
        while y < r.y1 {
            let mut x = r.x0 + step / 2.0;
            while x < r.x1 {
                let w: i32 = pieces.iter().map(|p| winding(p, Point::new(x, y))).sum();
                if (even_odd && w % 2 != 0) || (!even_odd && w != 0) {
                    n += 1;
                }
                x += step;
            }
            y += step;
        }
        n as f64 * step * step
    }

    #[test]
    fn a_square_crossing_a_mark_keeps_the_rest() {
        let sq = square(0.0, 0.0, 100.0, 100.0);
        let mark = Rect::new(50.0, 25.0, 150.0, 75.0);
        let pieces = subtract_marks(&sq, &[mark], fill_meets).unwrap();
        let total: f64 = pieces.iter().map(|p| area(p).abs()).sum();
        assert!((total - (10000.0 - 2500.0)).abs() < 1e-6, "{total}");
        assert_eq!(painted(&pieces, mark, false), 0.0);
    }

    #[test]
    fn a_contour_around_a_mark_gets_a_hole() {
        let page = square(0.0, 0.0, 400.0, 400.0);
        let mark = Rect::new(150.0, 90.0, 250.0, 210.0);
        assert!(fill_meets(&page, mark));
        assert!(!clip_meets(&page, mark));
        let pieces = subtract_marks(&page, &[mark], fill_meets).unwrap();
        assert_eq!(painted(&pieces, mark, false), 0.0);
        assert!(
            (painted(&pieces, Rect::new(0.0, 0.0, 400.0, 400.0), false) - (160000.0 - 12000.0)).abs() < 1.0
        );
    }

    #[test]
    fn an_even_odd_ring_keeps_its_hole_and_loses_the_mark() {
        let outer = square(0.0, 0.0, 100.0, 100.0);
        let inner = square(30.0, 30.0, 70.0, 70.0);
        let mark = Rect::new(60.0, 40.0, 90.0, 60.0);
        let mut pieces = subtract_marks(&outer, &[mark], fill_meets).unwrap();
        pieces.extend(subtract_marks(&inner, &[mark], fill_meets).unwrap());
        assert_eq!(painted(&pieces, mark, true), 0.0);
        // The ring outside the mark: 10000 - 1600 - (mark area on the ring: 20 x 20).
        let total = painted(&pieces, Rect::new(0.0, 0.0, 100.0, 100.0), true);
        assert!((total - (10000.0 - 1600.0 - 400.0)).abs() < 1.0, "{total}");
    }

    #[test]
    fn overlapping_marks_are_subtracted_in_turn() {
        let sq = square(0.0, 0.0, 100.0, 100.0);
        let marks = [
            Rect::new(10.0, 10.0, 50.0, 50.0),
            Rect::new(30.0, 30.0, 70.0, 70.0),
        ];
        let pieces = subtract_marks(&sq, &marks, fill_meets).unwrap();
        for m in marks {
            assert_eq!(painted(&pieces, m, false), 0.0);
        }
        let total = painted(&pieces, Rect::new(0.0, 0.0, 100.0, 100.0), false);
        assert!(
            (total - (10000.0 - 1600.0 - 1600.0 + 400.0)).abs() < 1.0,
            "{total}"
        );
    }

    #[test]
    fn lines_are_cut_where_they_cross() {
        let line = vec![Point::new(50.0, 200.0), Point::new(350.0, 200.0)];
        let pieces = cut_polyline(&line, false, &[Rect::new(150.0, 90.0, 250.0, 210.0)]).unwrap();
        assert_eq!(pieces.len(), 2);
        assert!((pieces[0][1].x - 150.0).abs() < 1e-9 && (pieces[1][0].x - 250.0).abs() < 1e-9);
        // Untouched lines stay.
        assert!(cut_polyline(&line, false, &[Rect::new(0.0, 0.0, 10.0, 10.0)]).is_none());
        // A dot inside a mark goes.
        assert_eq!(
            cut_polyline(&[Point::new(5.0, 5.0)], false, &[Rect::new(0.0, 0.0, 10.0, 10.0)]),
            Some(vec![])
        );
    }

    #[test]
    fn a_cut_closed_rectangle_is_one_open_line_with_its_corner() {
        let sq = square(0.0, 0.0, 100.0, 100.0);
        let pieces = cut_polyline(&sq, true, &[Rect::new(40.0, 90.0, 60.0, 110.0)]).unwrap();
        assert_eq!(pieces.len(), 1, "{pieces:?}");
        let p = &pieces[0];
        assert!(
            (p[0].x - 40.0).abs() < 1e-9 && (p[0].y - 100.0).abs() < 1e-9,
            "{p:?}"
        );
        assert!((p.last().unwrap().x - 60.0).abs() < 1e-9, "{p:?}");
        // The corner at the origin is inside the piece, not at an end.
        assert!(p[1..p.len() - 1].contains(&Point::new(0.0, 0.0)), "{p:?}");
    }

    #[test]
    fn numbers_are_short_and_never_negative_zero() {
        assert_eq!(fmt_coord(-0.0000001), "0");
        assert_eq!(fmt_coord(1.25), "1.25");
        assert_eq!(fmt_coord(-3.0), "-3");
        assert_eq!(fmt_coord(2.0 / 3.0), "0.666667");
    }

    fn redact(data: &[u8], marks: &[Rect]) -> Vec<PathEdit> {
        let doc = lopdf::Document::with_version("1.7");
        let s = scan(
            &doc,
            data,
            &Dictionary::new(),
            &page_state(Rect::new(0.0, 0.0, 400.0, 400.0)),
        );
        s.paths.iter().map(|p| redact_path(p, marks)).collect()
    }

    #[test]
    fn path_objects_are_rewritten_as_the_table_says() {
        let mark = [Rect::new(150.0, 90.0, 250.0, 210.0)];
        // A clip inside the mark becomes an empty clip far away.
        let e = redact(b"160 160 20 20 re W n", &mark);
        match &e[0] {
            PathEdit::Replace(s) => assert!(s.contains("-20000 -20000 m") && s.ends_with("W n"), "{s}"),
            other => panic!("{other:?}"),
        }
        // A page clip around the mark stays.
        assert_eq!(redact(b"0 0 400 400 re W n", &mark)[0], PathEdit::Keep);
        // A path that paints nothing goes.
        assert_eq!(
            redact(b"160 160 m 170 170 l n", &mark)[0],
            PathEdit::Replace(String::new())
        );
        // Stroke with a clip: the stroke is cut, the clip kept (no geometry inside).
        match &redact(b"0 200 m 400 200 l W S", &mark)[0] {
            PathEdit::Replace(s) => assert!(s.contains(" S ") && s.ends_with("W n"), "{s}"),
            other => panic!("{other:?}"),
        }
        // Fill and stroke become a fill and a stroke.
        match &redact(b"100 100 200 50 re b", &mark)[0] {
            PathEdit::Replace(s) => assert!(s.contains("f ") && s.contains(" S"), "{s}"),
            other => panic!("{other:?}"),
        }
        // Far away: kept.
        assert_eq!(redact(b"0 0 10 10 re f", &mark)[0], PathEdit::Keep);
    }

    #[test]
    fn s_keeps_earlier_subpaths_open() {
        let mark = [Rect::new(25.0, -1.0, 27.0, 1.0)];
        match &redact(b"0 0 m 10 0 l 20 0 m 30 0 l 30 10 l s", &mark)[0] {
            PathEdit::Replace(s) => {
                // The first subpath is written as it was: open; the second,
                // closed, is cut into one line joined at its first point.
                assert!(
                    s.starts_with("0 0 m 10 0 l 27.5 0 m 30 0 l 30 10 l 20 0 l 24.5 0 l S"),
                    "{s}"
                );
            }
            other => panic!("{other:?}"),
        }
    }
}
