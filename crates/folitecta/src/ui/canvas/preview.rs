//! What a page shows while a tool works on it: the shape being drawn as it will look, the
//! smooth curve of freehand strokes, constrained drags, and the classes of search hits.

use super::*;

/// Class of a search hit: the current one, and one color per word of the query.
pub(super) fn hit_class(current: bool, word: usize, colored: bool) -> String {
    let mut c = String::from("hit");
    if current {
        c.push_str(" current");
    }
    if colored {
        c.push_str(&format!(" w{}", word % 5));
    }
    c
}

pub(super) fn css_color(c: [f32; 3]) -> String {
    let q = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!("rgb({},{},{})", q(c[0]), q(c[1]), q(c[2]))
}

/// Where a drag with Shift held ends: squares and circles for the area
/// tools, lines in steps of 45 degrees.
pub(super) fn constrained(tool: Tool, start: (f64, f64), current: (f64, f64)) -> (f64, f64) {
    let (dx, dy) = (current.0 - start.0, current.1 - start.1);
    match tool {
        Tool::Rect | Tool::Ellipse | Tool::Highlight | Tool::TextBox => {
            let side = dx.abs().max(dy.abs());
            (start.0 + side * dx.signum(), start.1 + side * dy.signum())
        }
        Tool::Line | Tool::Arrow | Tool::Distance | Tool::Radius => {
            let len = dx.hypot(dy);
            let step = std::f64::consts::FRAC_PI_4;
            let angle = (dy.atan2(dx) / step).round() * step;
            (start.0 + len * angle.cos(), start.1 + len * angle.sin())
        }
        _ => current,
    }
}

/// SVG path data of the smooth curve through screen points.
pub(super) fn smooth_svg_path(points: &[(f64, f64)]) -> String {
    let pts: Vec<(f32, f32)> = points.iter().map(|(x, y)| (*x as f32, *y as f32)).collect();
    let Some(first) = pts.first() else {
        return String::new();
    };
    let mut d = format!("M{:.1} {:.1}", first.0, first.1);
    for c in folitecta_core::smooth::curves(&pts) {
        d.push_str(&format!(
            " C{:.1} {:.1} {:.1} {:.1} {:.1} {:.1}",
            c.p1.x, c.p1.y, c.p2.x, c.p2.y, c.p3.x, c.p3.y
        ));
    }
    d
}

/// The shape being drawn, as it will look (the default style of its tool),
/// while the pointer is dragged: SVG content in page pixels.
pub(super) fn shape_preview(st: AppState, tool: Tool, d: Drag, z: f32) -> Option<Element> {
    let kind = match tool {
        Tool::Rect => "Square",
        Tool::Ellipse => "Circle",
        Tool::Highlight => "HighlightArea",
        Tool::TextBox => "FreeText",
        Tool::Typewriter => "Typewriter",
        Tool::Line | Tool::Arrow => "Line",
        Tool::Distance => "Distance",
        Tool::Radius => "Radius",
        Tool::Callout => "Callout",
        _ => return None,
    };
    let s = crate::state::comment_styles::default_style(st, kind);
    // A style with neither border nor fill still shows where the shape goes.
    let invisible = s.color.is_none() && s.fill.is_none();
    let stroke = match s.color {
        Some(c) => css_color(c),
        None if invisible => "#4f46e5".into(),
        None => "none".into(),
    };
    let fill = s.fill.map(css_color).unwrap_or_else(|| "none".into());
    let width = (s.width * z).max(if s.color.is_some() || invisible { 1.0 } else { 0.0 });
    let dash = if s.dashed || invisible {
        format!("{} {}", 3.0 * width.max(1.0), 2.0 * width.max(1.0))
    } else {
        String::new()
    };
    let opacity = s.opacity.clamp(0.05, 1.0);
    let (x0, y0) = (
        d.start.0.min(d.current.0) as f32,
        d.start.1.min(d.current.1) as f32,
    );
    let (w, h) = (
        (d.start.0 - d.current.0).abs() as f32,
        (d.start.1 - d.current.1).abs() as f32,
    );
    // The border is drawn inside the area, as on the page.
    let inset = width / 2.0;
    let (ix, iy, iw, ih) = (x0 + inset, y0 + inset, (w - width).max(0.0), (h - width).max(0.0));
    Some(match tool {
        Tool::Rect | Tool::TextBox => rsx! {
            rect { x: "{ix}", y: "{iy}", width: "{iw}", height: "{ih}", fill: "{fill}", stroke: "{stroke}", stroke_width: "{width}", stroke_dasharray: "{dash}", opacity: "{opacity}" }
        },
        Tool::Ellipse => rsx! {
            ellipse { cx: "{x0 + w / 2.0}", cy: "{y0 + h / 2.0}", rx: "{(iw / 2.0).max(0.5)}", ry: "{(ih / 2.0).max(0.5)}", fill: "{fill}", stroke: "{stroke}", stroke_width: "{width}", stroke_dasharray: "{dash}", opacity: "{opacity}" }
        },
        Tool::Highlight => {
            let c = s.color.map(css_color).unwrap_or_else(|| "rgb(255,230,50)".into());
            rsx! { rect { x: "{x0}", y: "{y0}", width: "{w}", height: "{h}", fill: "{c}", opacity: "0.45", style: "mix-blend-mode:multiply" } }
        }
        Tool::Typewriter => rsx! {
            rect { x: "{x0}", y: "{y0}", width: "{w}", height: "{h}", fill: "none", stroke: "#4f46e5", stroke_width: "1", stroke_dasharray: "4 3" }
        },
        _ => {
            // Lines: an arrow head where the drag ends for arrows.
            let arrow = tool == Tool::Arrow || s.line_end != "None";
            let (ax, ay) = (d.current.0 as f32, d.current.1 as f32);
            let (bx, by) = (d.start.0 as f32, d.start.1 as f32);
            let len = ((ax - bx).powi(2) + (ay - by).powi(2)).sqrt().max(0.001);
            let (ux, uy) = ((ax - bx) / len, (ay - by) / len);
            let size = (width * 4.0).max(8.0);
            let head = format!(
                "{},{} {},{} {},{}",
                ax - ux * size - uy * size * 0.5,
                ay - uy * size + ux * size * 0.5,
                ax,
                ay,
                ax - ux * size + uy * size * 0.5,
                ay - uy * size - ux * size * 0.5
            );
            let color = if stroke == "none" {
                "#3b3bc0".to_string()
            } else {
                stroke.clone()
            };
            rsx! {
                line { x1: "{bx}", y1: "{by}", x2: "{ax}", y2: "{ay}", stroke: "{color}", stroke_width: "{width.max(1.0)}", stroke_dasharray: "{dash}", opacity: "{opacity}", stroke_linecap: "round" }
                // A radius: the circle it measures.
                if tool == Tool::Radius {
                    circle { cx: "{bx}", cy: "{by}", r: "{len}", fill: "none", stroke: "{color}", stroke_width: "{width.max(1.0)}", stroke_dasharray: "{dash}", opacity: "{opacity}" }
                }
                if arrow && len > size {
                    polyline { points: "{head}", fill: "none", stroke: "{color}", stroke_width: "{width.max(1.0)}", opacity: "{opacity}", stroke_linejoin: "round", stroke_linecap: "round" }
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shift_makes_squares_and_straight_lines() {
        let close = |a: (f64, f64), b: (f64, f64)| (a.0 - b.0).abs() < 1e-9 && (a.1 - b.1).abs() < 1e-9;
        assert!(close(
            constrained(Tool::Rect, (10.0, 10.0), (40.0, 20.0)),
            (40.0, 40.0)
        ));
        assert!(close(
            constrained(Tool::Ellipse, (10.0, 10.0), (0.0, 50.0)),
            (-30.0, 50.0)
        ));
        assert!(close(
            constrained(Tool::Line, (0.0, 0.0), (10.0, 1.0)),
            (10.0f64.hypot(1.0), 0.0)
        ));
        let d = constrained(Tool::Arrow, (0.0, 0.0), (10.0, 9.0));
        assert!((d.0 - d.1).abs() < 1e-9, "{d:?}");
        assert_eq!(constrained(Tool::Ink, (0.0, 0.0), (3.0, 4.0)), (3.0, 4.0));
    }
}
