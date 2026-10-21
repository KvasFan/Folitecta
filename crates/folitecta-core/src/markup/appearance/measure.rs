//! Measurement labels of dimension comments: lengths, perimeters and areas
//! written with the number formats of their measure dictionary; and the
//! measurements of Folitecta's own kinds (`/IT` names of its own, which
//! other readers draw from the appearance): angles, radii and volumes.

use lopdf::Dictionary;

use super::name_of;
use super::shapes::{ellipse, poly, signed_area};
use crate::util::{deref, num};

/// The `/IT` of an angle measurement: a polyline of three points, the angle at the middle one.
pub(crate) const ANGLE: &str = "FolitectaAngle";
/// The `/IT` of a radius measurement: a line from the centre of a circle to its edge.
pub(crate) const RADIUS: &str = "FolitectaRadius";
/// The `/IT` of a volume measurement: a polygon, its area times `/FolitectaDepth`.
pub(crate) const VOLUME: &str = "FolitectaVolume";

/// The angle at `v` between the lines to `a` and `b`, in degrees (0 to 180).
pub(crate) fn angle_at(a: (f32, f32), v: (f32, f32), b: (f32, f32)) -> f32 {
    let (ax, ay, bx, by) = (a.0 - v.0, a.1 - v.1, b.0 - v.0, b.1 - v.1);
    let (la, lb) = (ax.hypot(ay), bx.hypot(by));
    if la < 1e-6 || lb < 1e-6 {
        return 0.0;
    }
    ((ax * bx + ay * by) / (la * lb))
        .clamp(-1.0, 1.0)
        .acos()
        .to_degrees()
}

fn dist(a: (f32, f32), b: (f32, f32)) -> f32 {
    (b.0 - a.0).hypot(b.1 - a.1)
}

/// What a measurement draws besides its lines, to stroke the same way: the
/// arc of an angle, the circle of a radius; with the bounds it needs.
pub(super) fn measure_extra(d: &Dictionary, pts: &[(f32, f32)]) -> Option<(String, [f32; 4])> {
    match name_of(d, b"IT")?.as_str() {
        ANGLE if pts.len() >= 3 => {
            let (a, v, b) = (pts[0], pts[1], pts[2]);
            let r = (dist(a, v).min(dist(b, v)) * 0.35).clamp(6.0, 40.0);
            let (t0, t1) = ((a.1 - v.1).atan2(a.0 - v.0), (b.1 - v.1).atan2(b.0 - v.0));
            // The shorter way round, from one line to the other.
            let mut sweep = t1 - t0;
            if sweep > std::f32::consts::PI {
                sweep -= std::f32::consts::TAU;
            } else if sweep < -std::f32::consts::PI {
                sweep += std::f32::consts::TAU;
            }
            let arc: Vec<(f32, f32)> = (0..=16)
                .map(|i| {
                    let t = t0 + sweep * i as f32 / 16.0;
                    (v.0 + r * t.cos(), v.1 + r * t.sin())
                })
                .collect();
            Some((poly(&arc, false), [v.0 - r, v.1 - r, v.0 + r, v.1 + r]))
        }
        RADIUS if pts.len() >= 2 => {
            let (c, r) = (pts[0], dist(pts[0], pts[1]));
            let (x0, y0, x1, y1) = (c.0 - r, c.1 - r, c.0 + r, c.1 + r);
            Some((ellipse(x0, y0, x1, y1), [x0, y0, x1, y1]))
        }
        _ => None,
    }
}

/// A number format of a measure dictionary (`/X`, `/A`, ...).
struct NumFmt {
    unit: String,
    factor: f32,
    decimals: usize,
    /// Separators of the decimals (`/RD`) and the thousands (`/RT`, none when absent).
    decimal_sep: String,
    group_sep: String,
}

fn num_fmt(doc: &lopdf::Document, measure: &Dictionary, key: &[u8]) -> Option<NumFmt> {
    let arr = measure
        .get(key)
        .ok()
        .map(|a| deref(doc, a))
        .and_then(|a| a.as_array().ok())?;
    let first = arr
        .first()
        .map(|f| deref(doc, f))
        .and_then(|f| f.as_dict().ok())?;
    let unit = first
        .get(b"U")
        .ok()
        .and_then(|u| crate::util::text_of(doc, u))
        .unwrap_or_default();
    let factor = first
        .get(b"C")
        .ok()
        .and_then(|c| num(deref(doc, c)))
        .unwrap_or(1.0);
    let denominator = first
        .get(b"D")
        .ok()
        .and_then(|v| num(deref(doc, v)))
        .unwrap_or(100.0)
        .max(1.0);
    let text = |k: &[u8]| first.get(k).ok().and_then(|v| crate::util::text_of(doc, v));
    Some(NumFmt {
        unit,
        factor,
        decimals: denominator.log10().round().clamp(0.0, 6.0) as usize,
        decimal_sep: text(b"RD")
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| ".".into()),
        group_sep: text(b"RT").unwrap_or_default(),
    })
}

/// The unit vector from `v` towards `p`.
fn unit_of(p: (f32, f32), v: (f32, f32)) -> (f32, f32) {
    let l = dist(p, v).max(1e-6);
    ((p.0 - v.0) / l, (p.1 - v.1) / l)
}

fn path_length(pts: &[(f32, f32)], closed: bool) -> f32 {
    let mut total: f32 = pts
        .windows(2)
        .map(|w| ((w[1].0 - w[0].0).powi(2) + (w[1].1 - w[0].1).powi(2)).sqrt())
        .sum();
    if closed && pts.len() > 2 {
        let (a, b) = (pts[pts.len() - 1], pts[0]);
        total += ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt();
    }
    total
}

/// Measurement text and its anchor for a dimension annotation (`/IT ...Dimension`).
pub(super) fn measure_label(
    doc: &lopdf::Document,
    d: &Dictionary,
    pts: &[(f32, f32)],
) -> Option<(String, (f32, f32))> {
    let it = name_of(d, b"IT")?;
    let measure = d
        .get(b"Measure")
        .ok()
        .map(|m| deref(doc, m))
        .and_then(|m| m.as_dict().ok())?;
    let n = pts.len();
    let fmt = |v: f32, nf: &NumFmt| {
        let n =
            crate::markup_tools::format_measure(v * nf.factor, nf.decimals, &nf.decimal_sep, &nf.group_sep);
        format!("{n} {}", nf.unit).trim().to_string()
    };
    match it.as_str() {
        "LineDimension" if n >= 2 => {
            let x = num_fmt(doc, measure, b"D").or_else(|| num_fmt(doc, measure, b"X"))?;
            let mid = ((pts[0].0 + pts[n - 1].0) / 2.0, (pts[0].1 + pts[n - 1].1) / 2.0);
            Some((fmt(path_length(pts, false), &x), mid))
        }
        "PolyLineDimension" if n >= 2 => {
            let x = num_fmt(doc, measure, b"D").or_else(|| num_fmt(doc, measure, b"X"))?;
            Some((fmt(path_length(pts, false), &x), pts[n - 1]))
        }
        "PolygonDimension" if n >= 3 => {
            let a = num_fmt(doc, measure, b"A")?;
            let c = pts.iter().fold((0.0, 0.0), |acc, p| {
                (acc.0 + p.0 / n as f32, acc.1 + p.1 / n as f32)
            });
            Some((fmt(signed_area(pts).abs(), &a), c))
        }
        ANGLE if n >= 3 => {
            let x = num_fmt(doc, measure, b"D").or_else(|| num_fmt(doc, measure, b"X"))?;
            let (a, v, b) = (pts[0], pts[1], pts[2]);
            let deg = angle_at(a, v, b);
            let text = format!(
                "{}\u{b0}",
                crate::markup_tools::format_measure(deg, 1, &x.decimal_sep, "")
            );
            // Inside the angle, on its bisector, beyond the arc.
            let (ua, ub) = (unit_of(a, v), unit_of(b, v));
            let (mx, my) = (ua.0 + ub.0, ua.1 + ub.1);
            let len = mx.hypot(my);
            let (bx, by) = if len < 1e-3 {
                (-ua.1, ua.0)
            } else {
                (mx / len, my / len)
            };
            // Farther out the narrower the angle, so the label stays clear of its lines.
            let half = (deg / 2.0).to_radians().sin().max(0.1);
            let r = (dist(a, v).min(dist(b, v)) * 0.35).clamp(6.0, 40.0) + (9.0 / half).clamp(14.0, 80.0);
            Some((text, (v.0 + bx * r, v.1 + by * r)))
        }
        RADIUS if n >= 2 => {
            let x = num_fmt(doc, measure, b"D").or_else(|| num_fmt(doc, measure, b"X"))?;
            let r = dist(pts[0], pts[1]);
            let mid = ((pts[0].0 + pts[1].0) / 2.0, (pts[0].1 + pts[1].1) / 2.0);
            Some((
                format!("R {} \u{b7} \u{d8} {}", fmt(r, &x), fmt(2.0 * r, &x)),
                mid,
            ))
        }
        VOLUME if n >= 3 => {
            let a = num_fmt(doc, measure, b"A")?;
            let x = num_fmt(doc, measure, b"D").or_else(|| num_fmt(doc, measure, b"X"))?;
            let depth = d
                .get(b"FolitectaDepth")
                .ok()
                .and_then(|v| num(deref(doc, v)))
                .unwrap_or(0.0);
            let c = pts.iter().fold((0.0, 0.0), |acc, p| {
                (acc.0 + p.0 / n as f32, acc.1 + p.1 / n as f32)
            });
            let v = signed_area(pts).abs() * a.factor * depth;
            let number = crate::markup_tools::format_measure(v, a.decimals, &a.decimal_sep, &a.group_sep);
            Some((format!("{number} {}\u{b3}", x.unit).trim().to_string(), c))
        }
        _ => None,
    }
}
