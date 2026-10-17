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
    let mut kept: Vec<(f32, f32)> = Vec::with_capacity(points.len());
    for (i, &p) in points.iter().enumerate() {
        let far = kept
            .last()
            .is_none_or(|q| (p.0 - q.0).hypot(p.1 - q.1) >= min_gap);
        if far || i == points.len() - 1 {
            if !far && kept.len() > 1 {
                kept.pop();
            }
            kept.push(p);
        }
    }
    if kept.len() < 3 {
        return kept;
    }
    let step = step.max(0.01) as f64;
    let mut out = vec![kept[0]];
    for c in curves(&kept) {
        let n = (((c.p3 - c.p0).hypot() / step).ceil() as usize).clamp(2, 16);
        out.extend((1..=n).map(|k| {
            let q = c.eval(k as f64 / n as f64);
            (q.x as f32, q.y as f32)
        }));
    }
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
}
