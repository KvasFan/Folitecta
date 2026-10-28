//! Freehand strokes made smooth: the points of the pointer joined by
//! Catmull-Rom curves (cubic Béziers), sampled densely enough that the
//! polyline through the samples no longer shows corners.

use hayro::kurbo::{CubicBez, ParamCurve, Point, Vec2};

fn point((x, y): (f32, f32)) -> Point {
    Point::new(x as f64, y as f64)
}

/// The cubic Bézier pieces of the curve through the points. The tangent at a
/// point follows its neighbours, scaled by the length of the pieces on each
/// side so short pieces next to long ones do not overshoot.
pub fn curves(points: &[(f32, f32)]) -> Vec<CubicBez> {
    let p: Vec<Point> = points.iter().copied().map(point).collect();
    if p.len() < 2 {
        return Vec::new();
    }
    let last = p.len() - 1;
    // The handle leaving p[i] towards p[i + 1] (or arriving from p[i - 1]
    // when `towards` is that one): a third of the tangent, weighted.
    let handle = |i: usize, towards: usize| -> Vec2 {
        let (prev, next) = (p[i.saturating_sub(1)], p[(i + 1).min(last)]);
        let before = (p[i] - prev).hypot();
        let after = (next - p[i]).hypot();
        let side = (p[towards] - p[i]).hypot();
        if before + after <= f64::EPSILON {
            return Vec2::ZERO;
        }
        (next - prev) * (side / (before + after)) / 3.0
    };
    (0..last)
        .map(|i| {
            CubicBez::new(
                p[i],
                p[i] + handle(i, i + 1),
                p[i + 1] - handle(i + 1, i),
                p[i + 1],
            )
        })
        .collect()
}

/// A smooth freehand stroke: points closer than `min_gap` to the previous
/// one are dropped (the jitter of the pointer), then each piece between two
/// points becomes at least two pieces of the curve through them, one more
/// for every `step` of length (up to 16).
pub fn smooth_stroke(points: &[(f32, f32)], min_gap: f32, step: f32) -> Vec<(f32, f32)> {
    smooth_pressed_stroke(points, &[], min_gap, step).0
}

/// [`smooth_stroke`] for a stroke of a pen that tells how hard it presses:
/// `pressures` (0 to 1, one per point; missing ones count as 0.5) follow
/// the points that are kept, softened with their neighbours, and go
/// smoothly from one point to the next along the curve.
pub fn smooth_pressed_stroke(
    points: &[(f32, f32)],
    pressures: &[f32],
    min_gap: f32,
    step: f32,
) -> (Vec<(f32, f32)>, Vec<f32>) {
    let pressure = |i: usize| pressures.get(i).copied().unwrap_or(0.5).clamp(0.0, 1.0);
    let mut kept: Vec<(f32, f32)> = Vec::with_capacity(points.len());
    let mut kept_p: Vec<f32> = Vec::with_capacity(points.len());
    for (i, &p) in points.iter().enumerate() {
        let far = kept
            .last()
            .is_none_or(|q| (p.0 - q.0).hypot(p.1 - q.1) >= min_gap);
        if far || i == points.len() - 1 {
            if !far && kept.len() > 1 {
                kept.pop();
                kept_p.pop();
            }
            kept.push(p);
            kept_p.push(pressure(i));
        }
    }
    // The pressure a pen reports trembles: each one with its neighbours.
    let last = kept_p.len().saturating_sub(1);
    let soft: Vec<f32> = (0..kept_p.len())
        .map(|i| (kept_p[i.saturating_sub(1)] + 2.0 * kept_p[i] + kept_p[(i + 1).min(last)]) / 4.0)
        .collect();
    if kept.len() < 3 {
        return (kept, soft);
    }
    let step = step.max(0.01) as f64;
    let mut out = vec![kept[0]];
    let mut out_p = vec![soft[0]];
    for (i, c) in curves(&kept).into_iter().enumerate() {
        let n = (((c.p3 - c.p0).hypot() / step).ceil() as usize).clamp(2, 16);
        for k in 1..=n {
            let t = k as f64 / n as f64;
            let q = c.eval(t);
            out.push((q.x as f32, q.y as f32));
            out_p.push(soft[i] + (soft[i + 1] - soft[i]) * t as f32);
        }
    }
    (out, out_p)
}

/// The width of a pen stroke of `width` where the pen presses with
/// `pressure` (0 to 1): the width itself at half the pressure, from 0.3 to
/// 1.7 times it.
pub fn pressed_width(width: f32, pressure: f32) -> f32 {
    width * (0.3 + 1.4 * pressure.clamp(0.0, 1.0))
}

/// The outline of a stroke through `points` whose width at each point is
/// `widths` (as many): along one side, round around the end, back along
/// the other side and round around the start. Filled, it draws the stroke
/// in one piece, so a stroke that is not opaque has no darker overlaps.
pub fn stroke_outline(points: &[(f32, f32)], widths: &[f32]) -> Vec<(f32, f32)> {
    let n = points.len().min(widths.len());
    if n == 0 {
        return Vec::new();
    }
    let p: Vec<Point> = points[..n].iter().copied().map(point).collect();
    let half: Vec<f64> = widths[..n].iter().map(|w| (w.max(0.0) / 2.0) as f64).collect();
    // The direction of the stroke at each point, from its neighbours; a
    // point on top of its neighbours keeps the direction before it.
    let mut dirs: Vec<Vec2> = Vec::with_capacity(n);
    for i in 0..n {
        let d = p[(i + 1).min(n - 1)] - p[i.saturating_sub(1)];
        let len = d.hypot();
        dirs.push(if len > 1e-9 {
            d / len
        } else {
            dirs.last().copied().unwrap_or(Vec2::new(1.0, 0.0))
        });
    }
    let normal = |d: Vec2| Vec2::new(-d.y, d.x);
    // Half a circle around `c` from the angle `from`, turning back by π.
    let cap = |c: Point, h: f64, from: f64, out: &mut Vec<(f32, f32)>| {
        const STEPS: usize = 8;
        for k in 1..STEPS {
            let a = from - std::f64::consts::PI * k as f64 / STEPS as f64;
            out.push(((c.x + h * a.cos()) as f32, (c.y + h * a.sin()) as f32));
        }
    };
    let at = |q: Point| (q.x as f32, q.y as f32);
    let mut out = Vec::with_capacity(2 * n + 16);
    for i in 0..n {
        out.push(at(p[i] + normal(dirs[i]) * half[i]));
    }
    let end = normal(dirs[n - 1]);
    cap(p[n - 1], half[n - 1], end.y.atan2(end.x), &mut out);
    for i in (0..n).rev() {
        out.push(at(p[i] - normal(dirs[i]) * half[i]));
    }
    let start = -normal(dirs[0]);
    cap(p[0], half[0], start.y.atan2(start.x), &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strokes_pass_through_their_points_with_more_pieces() {
        let pts = [(0.0, 0.0), (10.0, 10.0), (20.0, 0.0), (30.0, 10.0)];
        let s = smooth_stroke(&pts, 0.5, 100.0);
        // Three pieces become at least six.
        assert!(s.len() > 2 * (pts.len() - 1), "{}", s.len());
        for p in pts {
            assert!(
                s.iter()
                    .any(|q| (q.0 - p.0).abs() < 1e-4 && (q.1 - p.1).abs() < 1e-4)
            );
        }
        // The curve bends smoothly: no sample far beyond the points.
        assert!(s.iter().all(|q| q.1 > -2.0 && q.1 < 12.0));
    }

    #[test]
    fn jitter_is_dropped_and_short_strokes_kept() {
        let pts = [(0.0, 0.0), (0.1, 0.1), (0.2, 0.0), (5.0, 0.0)];
        assert_eq!(smooth_stroke(&pts, 1.0, 1.0), vec![(0.0, 0.0), (5.0, 0.0)]);
        assert_eq!(smooth_stroke(&[(1.0, 1.0)], 1.0, 1.0), vec![(1.0, 1.0)]);
        assert!(curves(&[(1.0, 1.0)]).is_empty());
        // Points on a line stay on it.
        let line = [(0.0, 0.0), (1.0, 0.0), (3.0, 0.0), (4.0, 0.0)];
        assert!(smooth_stroke(&line, 0.1, 0.5).iter().all(|q| q.1.abs() < 1e-6));
    }

    #[test]
    fn pressures_follow_the_points_kept_and_the_curve() {
        let pts = [(0.0, 0.0), (0.1, 0.0), (10.0, 0.0), (20.0, 0.0), (30.0, 0.0)];
        let pressures = [0.2, 0.9, 0.2, 0.6, 1.0];
        let (s, p) = smooth_pressed_stroke(&pts, &pressures, 1.0, 2.0);
        assert_eq!(s.len(), p.len());
        // The jitter point went with its pressure; the others are softened.
        assert!((p[0] - 0.2).abs() < 1e-6, "{p:?}");
        assert!((p[p.len() - 1] - 0.9).abs() < 1e-6, "{p:?}");
        // Between two points the pressure goes from one to the other.
        assert!(p.windows(2).all(|w| (w[1] - w[0]).abs() < 0.2), "{p:?}");
        // The same points as without pressure.
        assert_eq!(s, smooth_stroke(&pts, 1.0, 2.0));
        // Missing pressures count as half.
        let (_, half) = smooth_pressed_stroke(&pts, &[], 1.0, 2.0);
        assert!(half.iter().all(|v| (v - 0.5).abs() < 1e-6));
    }

    #[test]
    fn outlines_go_round_the_stroke_at_its_widths() {
        assert_eq!(pressed_width(2.0, 0.5), 2.0);
        assert!((pressed_width(2.0, 0.0) - 0.6).abs() < 1e-6);
        assert!((pressed_width(2.0, 3.0) - 3.4).abs() < 1e-6);
        let pts = [(0.0, 0.0), (10.0, 0.0), (20.0, 0.0)];
        let o = stroke_outline(&pts, &[2.0, 4.0, 6.0]);
        // Both sides of every point and two half circles of 7 points.
        assert_eq!(o.len(), 2 * 3 + 2 * 7);
        // Half the width on each side.
        assert!((o[0].1 - 1.0).abs() < 1e-5 && (o[1].1 - 2.0).abs() < 1e-5 && (o[2].1 - 3.0).abs() < 1e-5);
        let below = &o[3 + 7..3 + 7 + 3];
        assert!(
            (below[0].1 + 3.0).abs() < 1e-5 && (below[2].1 + 1.0).abs() < 1e-5,
            "{below:?}"
        );
        // The round end reaches beyond the last point, the start before the first.
        assert!(o.iter().any(|q| (q.0 - 23.0).abs() < 0.1 && q.1.abs() < 0.5));
        assert!(o.iter().any(|q| (q.0 + 1.0).abs() < 0.1 && q.1.abs() < 0.5));
        assert!(stroke_outline(&[], &[]).is_empty());
    }
}
