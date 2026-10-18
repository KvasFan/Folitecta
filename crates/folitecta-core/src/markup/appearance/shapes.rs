//! Paths drawn by comment appearances: polygons, smooth curves, ellipses,
//! line endings and cloudy borders.

use lopdf::Dictionary;

use super::name_of;
use crate::util::{deref, fmt_num, num};

/// Path of an ellipse inscribed in a rectangle (four Bézier curves).
pub(super) fn ellipse(x0: f32, y0: f32, x1: f32, y1: f32) -> String {
    let k = 0.5523;
    let (cx, cy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
    let (rx, ry) = ((x1 - x0) / 2.0, (y1 - y0) / 2.0);
    let (ox, oy) = (rx * k, ry * k);
    let f = fmt_num;
    format!(
        "{} {} m {} {} {} {} {} {} c {} {} {} {} {} {} c {} {} {} {} {} {} c {} {} {} {} {} {} c h ",
        f(cx + rx),
        f(cy),
        f(cx + rx),
        f(cy + oy),
        f(cx + ox),
        f(cy + ry),
        f(cx),
        f(cy + ry),
        f(cx - ox),
        f(cy + ry),
        f(cx - rx),
        f(cy + oy),
        f(cx - rx),
        f(cy),
        f(cx - rx),
        f(cy - oy),
        f(cx - ox),
        f(cy - ry),
        f(cx),
        f(cy - ry),
        f(cx + ox),
        f(cy - ry),
        f(cx + rx),
        f(cy - oy),
        f(cx + rx),
        f(cy)
    )
}

pub(super) fn poly(points: &[(f32, f32)], close: bool) -> String {
    let mut s = String::new();
    for (i, (x, y)) in points.iter().enumerate() {
        s.push_str(&format!(
            "{} {} {} ",
            fmt_num(*x),
            fmt_num(*y),
            if i == 0 { "m" } else { "l" }
        ));
    }
    if close {
        s.push_str("h ");
    }
    s
}

/// A smooth path through the points: the curves of [`crate::smooth::curves`].
pub(super) fn curve_path(points: &[(f32, f32)]) -> String {
    let mut s = format!("{} {} m ", fmt_num(points[0].0), fmt_num(points[0].1));
    for c in crate::smooth::curves(points) {
        for p in [c.p1, c.p2, c.p3] {
            s.push_str(&format!("{} {} ", fmt_num(p.x as f32), fmt_num(p.y as f32)));
        }
        s.push_str("c ");
    }
    s
}

/// Size of a line ending for a stroke width.
pub(super) fn ending_size(width: f32) -> f32 {
    (width * 4.0).max(7.0)
}

/// Drawing operators for one line ending at `tip`; `dir` is the unit vector
/// pointing from the line towards the tip. Closed shapes are filled when
/// `filled` (with the current fill colour) and always stroked.
pub(super) fn line_ending(kind: &str, tip: (f32, f32), dir: (f32, f32), width: f32, filled: bool) -> String {
    let s = ending_size(width);
    let (dx, dy) = dir;
    let (px, py) = (-dy, dx);
    let at = |along: f32, across: f32| (tip.0 + dx * along + px * across, tip.1 + dy * along + py * across);
    let paint = if filled { "b " } else { "s " };
    match kind {
        "OpenArrow" => poly(&[at(-s, s * 0.5), tip, at(-s, -s * 0.5)], false) + "S ",
        "ClosedArrow" => poly(&[at(-s, s * 0.5), tip, at(-s, -s * 0.5)], true) + paint,
        "ROpenArrow" => poly(&[at(s, s * 0.5), tip, at(s, -s * 0.5)], false) + "S ",
        "RClosedArrow" => poly(&[at(s, s * 0.5), tip, at(s, -s * 0.5)], true) + paint,
        "Butt" => poly(&[at(0.0, s * 0.5), at(0.0, -s * 0.5)], false) + "S ",
        "Slash" => poly(&[at(-s * 0.25, -s * 0.45), at(s * 0.25, s * 0.45)], false) + "S ",
        "Square" => {
            poly(
                &[
                    at(-s * 0.3, -s * 0.3),
                    at(s * 0.3, -s * 0.3),
                    at(s * 0.3, s * 0.3),
                    at(-s * 0.3, s * 0.3),
                ],
                true,
            ) + paint
        }
        "Diamond" => {
            poly(
                &[
                    at(-s * 0.4, 0.0),
                    at(0.0, -s * 0.4),
                    at(s * 0.4, 0.0),
                    at(0.0, s * 0.4),
                ],
                true,
            ) + paint
        }
        "Circle" => {
            let r = s * 0.3;
            ellipse(tip.0 - r, tip.1 - r, tip.0 + r, tip.1 + r) + paint
        }
        _ => String::new(),
    }
}

pub(super) fn unit(from: (f32, f32), to: (f32, f32)) -> (f32, f32) {
    let (dx, dy) = (to.0 - from.0, to.1 - from.1);
    let len = (dx * dx + dy * dy).sqrt().max(0.0001);
    (dx / len, dy / len)
}

pub(super) fn bounds(points: &[(f32, f32)], pad: f32) -> [f32; 4] {
    let mut r = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
    for (x, y) in points {
        r = [r[0].min(*x), r[1].min(*y), r[2].max(*x), r[3].max(*y)];
    }
    [r[0] - pad, r[1] - pad, r[2] + pad, r[3] + pad]
}

pub(super) fn pairs(v: &[f32]) -> Vec<(f32, f32)> {
    v.as_chunks::<2>().0.iter().map(|c| (c[0], c[1])).collect()
}

/// Cloudy border intensity (`/BE /S /C /I n`), if the annotation has one.
pub(super) fn cloud_intensity(doc: &lopdf::Document, d: &Dictionary) -> Option<f32> {
    let be = d
        .get(b"BE")
        .ok()
        .map(|b| deref(doc, b))
        .and_then(|b| b.as_dict().ok())?;
    if name_of(be, b"S").as_deref() != Some("C") {
        return None;
    }
    Some(
        be.get(b"I")
            .ok()
            .and_then(|i| num(deref(doc, i)))
            .unwrap_or(1.0)
            .clamp(0.0, 2.0),
    )
}

pub(super) fn signed_area(pts: &[(f32, f32)]) -> f32 {
    let n = pts.len();
    (0..n)
        .map(|i| {
            let (a, b) = (pts[i], pts[(i + 1) % n]);
            a.0 * b.1 - b.0 * a.1
        })
        .sum::<f32>()
        / 2.0
}

/// Scalloped outline around a closed polygon; returns the path and the bump radius.
pub(super) fn cloud_path(pts: &[(f32, f32)], intensity: f32) -> (String, f32) {
    let n = pts.len();
    if n < 2 {
        return (String::new(), 0.0);
    }
    let ccw = signed_area(pts) >= 0.0;
    let target = 10.0 + 6.0 * intensity;
    let f = fmt_num;
    let mut s = String::new();
    let mut max_r: f32 = 0.0;
    let mut first = true;
    for i in 0..n {
        let (a, b) = (pts[i], pts[(i + 1) % n]);
        let len = ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt();
        if len < 0.01 {
            continue;
        }
        let k = ((len / target).round() as usize).max(1);
        let (dx, dy) = ((b.0 - a.0) / len, (b.1 - a.1) / len);
        let (nx, ny) = if ccw { (dy, -dx) } else { (-dy, dx) };
        for j in 0..k {
            let t0 = j as f32 / k as f32;
            let t1 = (j + 1) as f32 / k as f32;
            let p = (a.0 + (b.0 - a.0) * t0, a.1 + (b.1 - a.1) * t0);
            let q = (a.0 + (b.0 - a.0) * t1, a.1 + (b.1 - a.1) * t1);
            let r = len / k as f32 / 2.0;
            max_r = max_r.max(r);
            let h = r * 4.0 / 3.0;
            if first {
                s.push_str(&format!("{} {} m ", f(p.0), f(p.1)));
                first = false;
            }
            s.push_str(&format!(
                "{} {} {} {} {} {} c ",
                f(p.0 + nx * h),
                f(p.1 + ny * h),
                f(q.0 + nx * h),
                f(q.1 + ny * h),
                f(q.0),
                f(q.1)
            ));
        }
    }
    s.push_str("h ");
    (s, max_r)
}

/// Vertices of an ellipse inscribed in a rectangle.
pub(super) fn ellipse_points(x0: f32, y0: f32, x1: f32, y1: f32, n: usize) -> Vec<(f32, f32)> {
    let (cx, cy, rx, ry) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0, (x1 - x0) / 2.0, (y1 - y0) / 2.0);
    (0..n)
        .map(|i| {
            let a = i as f32 / n as f32 * std::f32::consts::TAU;
            (cx + rx * a.cos(), cy + ry * a.sin())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::appearance::LINE_ENDINGS;

    #[test]
    fn every_line_ending_draws_something() {
        for kind in LINE_ENDINGS.iter().filter(|k| **k != "None") {
            assert!(
                !line_ending(kind, (10.0, 10.0), (1.0, 0.0), 1.0, true).is_empty(),
                "{kind}"
            );
        }
        assert!(line_ending("None", (0.0, 0.0), (1.0, 0.0), 1.0, false).is_empty());
    }
}
