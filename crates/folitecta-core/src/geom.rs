//! Rectangles as `[x0, y0, x1, y1]` arrays, the form PDF writes them in,
//! with the operations the modules share, and conversions to kurbo (the
//! geometry library of the renderer) for transforms and paths.

use hayro::kurbo;

/// A rectangle: two opposite corners.
pub type Rect4 = [f32; 4];

/// The rectangle with its corners in order (`x0 <= x1`, `y0 <= y1`).
pub fn normalize(r: Rect4) -> Rect4 {
    [r[0].min(r[2]), r[1].min(r[3]), r[0].max(r[2]), r[1].max(r[3])]
}

pub fn width(r: Rect4) -> f32 {
    (r[2] - r[0]).abs()
}

pub fn height(r: Rect4) -> f32 {
    (r[3] - r[1]).abs()
}

pub fn area(r: Rect4) -> f32 {
    width(r) * height(r)
}

/// Nothing inside (zero or negative width or height once ordered).
pub fn is_empty(r: Rect4) -> bool {
    let n = normalize(r);
    n[2] <= n[0] || n[3] <= n[1]
}

/// The smallest rectangle holding both.
pub fn union(a: Rect4, b: Rect4) -> Rect4 {
    let (a, b) = (normalize(a), normalize(b));
    [a[0].min(b[0]), a[1].min(b[1]), a[2].max(b[2]), a[3].max(b[3])]
}

/// Grow `acc` with `r`. A line (no width or no height) still counts;
/// a point or a rectangle with its corners swapped both ways adds nothing.
pub fn union_opt(acc: Option<Rect4>, r: Rect4) -> Option<Rect4> {
    if r[2] <= r[0] && r[3] <= r[1] {
        return acc;
    }
    Some(match acc {
        Some(a) => union(a, r),
        None => normalize(r),
    })
}

/// The rectangles overlap (touching edges do not count).
pub fn intersects(a: Rect4, b: Rect4) -> bool {
    let (a, b) = (normalize(a), normalize(b));
    a[0] < b[2] && b[0] < a[2] && a[1] < b[3] && b[1] < a[3]
}

/// The common part, when there is one.
pub fn intersection(a: Rect4, b: Rect4) -> Option<Rect4> {
    let (a, b) = (normalize(a), normalize(b));
    let r = [a[0].max(b[0]), a[1].max(b[1]), a[2].min(b[2]), a[3].min(b[3])];
    (r[2] > r[0] && r[3] > r[1]).then_some(r)
}

pub fn contains_point(r: Rect4, x: f32, y: f32) -> bool {
    let r = normalize(r);
    x >= r[0] && x <= r[2] && y >= r[1] && y <= r[3]
}

pub fn center(r: Rect4) -> (f32, f32) {
    ((r[0] + r[2]) / 2.0, (r[1] + r[3]) / 2.0)
}

/// Grown by `d` on every side (shrunk when negative).
pub fn inflate(r: Rect4, d: f32) -> Rect4 {
    let r = normalize(r);
    [r[0] - d, r[1] - d, r[2] + d, r[3] + d]
}

pub fn translate(r: Rect4, dx: f32, dy: f32) -> Rect4 {
    [r[0] + dx, r[1] + dy, r[2] + dx, r[3] + dy]
}

/// Bounds of points, when there are any.
pub fn bounds(points: impl IntoIterator<Item = (f32, f32)>) -> Option<Rect4> {
    points.into_iter().fold(None, |acc, (x, y)| {
        Some(match acc {
            Some(r) => [x.min(r[0]), y.min(r[1]), x.max(r[2]), y.max(r[3])],
            None => [x, y, x, y],
        })
    })
}

pub fn to_kurbo(r: Rect4) -> kurbo::Rect {
    kurbo::Rect::new(r[0] as f64, r[1] as f64, r[2] as f64, r[3] as f64).abs()
}

pub fn from_kurbo(r: kurbo::Rect) -> Rect4 {
    normalize([r.x0 as f32, r.y0 as f32, r.x1 as f32, r.y1 as f32])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rectangle_operations() {
        let a = [10.0, 20.0, 0.0, 0.0];
        assert_eq!(normalize(a), [0.0, 0.0, 10.0, 20.0]);
        assert_eq!(area(a), 200.0);
        assert_eq!(union(a, [5.0, 5.0, 30.0, 8.0]), [0.0, 0.0, 30.0, 20.0]);
        assert!(intersects(a, [9.0, 19.0, 12.0, 25.0]));
        assert!(
            !intersects(a, [10.0, 0.0, 12.0, 5.0]),
            "touching edges do not overlap"
        );
        assert_eq!(
            intersection(a, [5.0, 5.0, 30.0, 8.0]),
            Some([5.0, 5.0, 10.0, 8.0])
        );
        assert_eq!(intersection(a, [11.0, 0.0, 12.0, 1.0]), None);
        assert_eq!(union_opt(None, [1.0, 1.0, 1.0, 5.0]), Some([1.0, 1.0, 1.0, 5.0]));
        assert_eq!(union_opt(None, [3.0, 3.0, 3.0, 3.0]), None);
        assert_eq!(
            union_opt(Some(a), [40.0, 40.0, 41.0, 41.0]),
            Some([0.0, 0.0, 41.0, 41.0])
        );
        assert!(contains_point(a, 10.0, 20.0));
        assert_eq!(inflate(a, 1.0), [-1.0, -1.0, 11.0, 21.0]);
        assert_eq!(bounds([(3.0, 4.0), (-1.0, 9.0)]), Some([-1.0, 4.0, 3.0, 9.0]));
        assert_eq!(from_kurbo(to_kurbo(a)), normalize(a));
        assert_eq!(center(a), (5.0, 10.0));
    }
}
