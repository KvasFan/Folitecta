//! Pointer drags and coordinates: page points from the screen, snapping to the grid and guides, dragged comment rectangles.

use super::*;

#[derive(Clone, Copy, PartialEq)]
pub(super) struct Drag {
    pub(super) start: (f64, f64),
    pub(super) current: (f64, f64),
}

#[derive(Clone, Copy, PartialEq)]
pub(super) enum AnnotDragKind {
    Move,
    /// Corner: 0 top-left, 1 top-right, 2 bottom-left, 3 bottom-right (display space).
    Resize(u8),
}

#[derive(Clone, Copy, PartialEq)]
pub(super) struct AnnotDrag {
    pub(super) index: usize,
    pub(super) kind: AnnotDragKind,
    pub(super) start_client: (f64, f64),
    pub(super) cur_client: (f64, f64),
    pub(super) rect: [f32; 4],
}

/// Client-space delta to local (unrotated page) CSS pixels.
pub(super) fn client_to_local(dx: f64, dy: f64, view_rotation: i32) -> (f64, f64) {
    match view_rotation {
        90 => (dy, -dx),
        180 => (-dx, -dy),
        270 => (-dy, dx),
        _ => (dx, dy),
    }
}

/// Display point (y down) to PDF user space.
pub(super) fn to_pdf(size: &PageSize, x: f32, y: f32) -> (f32, f32) {
    display_to_pdf(size, x, size.height - y)
}

pub(super) fn rect_to_pdf(size: &PageSize, r: [f32; 4]) -> [f32; 4] {
    folitecta_core::textops::display_rect_to_pdf(size, r)
}

pub(super) fn dragged_rect(d: &AnnotDrag, zoom: f32, view_rotation: i32) -> [f32; 4] {
    let (dx, dy) = client_to_local(
        d.cur_client.0 - d.start_client.0,
        d.cur_client.1 - d.start_client.1,
        view_rotation,
    );
    let (dx, dy) = (dx as f32 / zoom, dy as f32 / zoom);
    let mut r = d.rect;
    match d.kind {
        AnnotDragKind::Move => {
            r[0] += dx;
            r[2] += dx;
            r[1] += dy;
            r[3] += dy;
        }
        AnnotDragKind::Resize(c) => {
            if c == 0 || c == 2 {
                r[0] += dx;
            } else {
                r[2] += dx;
            }
            if c == 0 || c == 1 {
                r[1] += dy;
            } else {
                r[3] += dy;
            }
        }
    }
    folitecta_core::geom::normalize(r)
}

/// The points drawings snap to on a page (display points), kept for the
/// tab, page and revision they were found for.
type SnapCache = Option<((u64, usize, u64), Vec<(f32, f32)>)>;
static SNAP_POINTS: std::sync::Mutex<SnapCache> = std::sync::Mutex::new(None);

fn object_points(st: AppState, page: usize) -> Vec<(f32, f32)> {
    let Some((key, size)) = st
        .read_active(|t| ((t.id, page, t.doc.revision()), t.doc.page_size(page).ok()))
        .and_then(|(k, s)| s.map(|s| (k, s)))
    else {
        return Vec::new();
    };
    let mut cache = SNAP_POINTS.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((k, points)) = cache.as_ref() {
        if *k == key {
            return points.clone();
        }
    }
    let points: Vec<(f32, f32)> = st
        .read_active(|t| t.doc.snap_points(page))
        .unwrap_or_default()
        .into_iter()
        .map(|p| folitecta_core::marks::pdf_to_display(&size, p[0], p[1]))
        .collect();
    *cache = Some((key, points.clone()));
    points
}

/// The point of `points` nearest to `(x, y)` within `tol`.
pub(super) fn nearest_point(points: &[(f32, f32)], x: f32, y: f32, tol: f32) -> Option<(f32, f32)> {
    points
        .iter()
        .copied()
        .map(|p| (p, (p.0 - x).hypot(p.1 - y)))
        .filter(|(_, d)| *d <= tol)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(p, _)| p)
}

/// Snap a display point to the grid and/or guides when snapping is on;
/// to the corners and ends of what the page draws first, when asked.
pub(crate) fn snap(st: AppState, page: usize, x: f32, y: f32, zoom: f32) -> (f32, f32) {
    if !option_on(st, "view.snap") {
        return (x, y);
    }
    if option_on(st, "view.snap.toObjects") {
        if let Some(p) = nearest_point(&object_points(st, page), x, y, 6.0 / zoom) {
            return p;
        }
    }
    let (mut sx, mut sy) = (x, y);
    if option_on(st, "view.snap.toGrid") {
        let step = grid_step(st);
        sx = (x / step).round() * step;
        sy = (y / step).round() * step;
    }
    if option_on(st, "view.snap.toGuides") {
        let tol = 6.0 / zoom;
        let guides = st.read_active(|t| t.guides.clone()).unwrap_or_default();
        for g in guides.iter().filter(|g| g.page.is_none() || g.page == Some(page)) {
            if g.horizontal && (y - g.pos).abs() < tol {
                sy = g.pos;
            }
            if !g.horizontal && (x - g.pos).abs() < tol {
                sx = g.pos;
            }
        }
    }
    (sx, sy)
}

pub(super) fn grid_step(st: AppState) -> f32 {
    st.ui
        .read()
        .value("grid.step")
        .and_then(|v| v.parse().ok())
        .filter(|v: &f32| *v > 1.0)
        .unwrap_or(28.3465)
}

/// Mouse movement in client pixels expressed in page display pixels.
pub(super) fn unrotate_delta(dx: f64, dy: f64, view_rotation: i32) -> (f64, f64) {
    match view_rotation {
        90 => (dy, -dx),
        180 => (-dx, -dy),
        270 => (-dy, dx),
        _ => (dx, dy),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_nearest_point_within_reach() {
        let points = [(10.0, 10.0), (50.0, 10.0), (12.0, 13.0)];
        assert_eq!(nearest_point(&points, 11.0, 11.0, 6.0), Some((10.0, 10.0)));
        assert_eq!(nearest_point(&points, 12.0, 14.0, 6.0), Some((12.0, 13.0)));
        assert_eq!(nearest_point(&points, 30.0, 30.0, 6.0), None);
        assert_eq!(nearest_point(&[], 1.0, 1.0, 6.0), None);
    }
}
