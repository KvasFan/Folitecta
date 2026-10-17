//! Rulers along the pages.

use super::*;

/// Rulers along the top and left edge of a page.
#[component]
pub(super) fn Rulers(
    width: f32,
    height: f32,
    zoom: f32,
    unit_pt: f32,
    minor: f32,
    major: f32,
    unit_name: &'static str,
) -> Element {
    let minor_px = minor * unit_pt * zoom;
    let major_px = major * unit_pt * zoom;
    let label_every = if major_px < 40.0 {
        ((40.0 / major_px).ceil()) as usize
    } else {
        1
    };
    let xs: Vec<(f32, String)> = (0..)
        .map(|i| i as f32 * major_px)
        .take_while(|x| *x <= width)
        .enumerate()
        .filter(|(i, _)| i % label_every == 0)
        .map(|(i, x)| (x, format!("{}", (i as f32 * major * 100.0).round() / 100.0)))
        .collect();
    let ys: Vec<(f32, String)> = (0..)
        .map(|i| i as f32 * major_px)
        .take_while(|y| *y <= height)
        .enumerate()
        .filter(|(i, _)| i % label_every == 0)
        .map(|(i, y)| (y, format!("{}", (i as f32 * major * 100.0).round() / 100.0)))
        .collect();
    rsx! {
        div { class: "ruler rx", title: "{unit_name}", style: "width:{width}px;--minor:{minor_px.max(2.0)}px;--major:{major_px.max(4.0)}px",
            for (x, label) in xs { span { key: "x{x}", style: "left:{x}px", "{label}" } }
        }
        div { class: "ruler ry", title: "{unit_name}", style: "height:{height}px;--minor:{minor_px.max(2.0)}px;--major:{major_px.max(4.0)}px",
            for (y, label) in ys { span { key: "y{y}", style: "top:{y}px", "{label}" } }
        }
    }
}
