//! The Pencil's quick panel (FOL-L025): while the Pencil is the tool, a
//! small bar over the pages with the thickness of the stroke, four standard
//! colours (black, green, blue, red) and the colour palette for any other.
//! They change the style of the next strokes, the default style of ink.

use dioxus::prelude::*;

use crate::commands::paths::{colour_hex, parse_colour};
use crate::i18n::t;
use crate::icons::Icon;
use crate::state::AppState;
use crate::state::comment_styles::{default_style, set_default_style};

/// The colours of the panel: key of their name, and the colour.
pub const COLOURS: [(&str, [f32; 3]); 4] = [
    ("black", [0.0, 0.0, 0.0]),
    ("green", [0.0, 0.6, 0.2]),
    ("blue", [0.1, 0.35, 0.9]),
    ("red", [0.86, 0.1, 0.1]),
];

/// The thicknesses offered, in points.
pub const WIDTHS: [f32; 6] = [0.5, 1.0, 2.0, 3.0, 5.0, 8.0];

/// Set the colour or the width of the next strokes.
pub fn set_pencil(st: AppState, colour: Option<[f32; 3]>, width: Option<f32>) {
    let mut style = default_style(st, "Ink");
    if let Some(c) = colour {
        style.color = Some(c);
    }
    if let Some(w) = width {
        style.width = w.clamp(0.25, 50.0);
    }
    set_default_style(st, "Ink", &style);
}

fn same(a: [f32; 3], b: [f32; 3]) -> bool {
    a.iter().zip(b).all(|(x, y)| (x - y).abs() < 0.01)
}

#[component]
pub fn PencilPanel() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let style = default_style(st, "Ink");
    let colour = style.color.unwrap_or([0.0; 3]);
    let width = style.width;
    let mut widths_open = use_signal(|| false);
    let width_text = format!("{}", (width * 10.0).round() / 10.0);
    let thickness = t(lang, "pencil.thickness");
    let palette = t(lang, "pencil.palette");
    let hex = colour_hex(colour);
    let other = !COLOURS.iter().any(|(_, c)| same(*c, colour));
    // The line drawn in the thickness button, never wider than the button.
    let line = width.clamp(1.0, 10.0);
    rsx! {
        div { class: "pencil-panel", role: "toolbar", "aria-label": "{t(lang, \"pencil.panel\")}",
            onmousedown: move |e| e.stop_propagation(),
            div { class: "pencil-width-wrap",
                button { class: if widths_open() { "pencil-btn pencil-width active" } else { "pencil-btn pencil-width" },
                    title: "{thickness}: {width_text} pt", "aria-label": "{thickness}",
                    onclick: move |_| widths_open.set(!widths_open()),
                    span { class: "pencil-line", style: "height:{line}px" }
                    span { class: "pencil-width-text", "{width_text}" }
                }
                if widths_open() {
                    div { class: "pencil-widths",
                        for w in WIDTHS {
                            {
                                let label = format!("{w}");
                                let line = w.clamp(1.0, 10.0);
                                let on = (w - width).abs() < 0.01;
                                rsx! {
                                    button { key: "{label}", class: if on { "pencil-btn active" } else { "pencil-btn" }, title: "{label} pt",
                                        "data-width": "{label}",
                                        onclick: move |_| { set_pencil(st, None, Some(w)); widths_open.set(false); },
                                        span { class: "pencil-line", style: "height:{line}px" }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            for (key, c) in COLOURS {
                {
                    let name = t(lang, &format!("pencil.{key}"));
                    let on = same(c, colour);
                    let swatch = colour_hex(c);
                    rsx! {
                        button { key: "{key}", class: if on { "pencil-btn pencil-colour active" } else { "pencil-btn pencil-colour" },
                            "data-colour": "{key}", title: "{name}", "aria-label": "{name}", "aria-pressed": "{on}",
                            onclick: move |_| set_pencil(st, Some(c), None),
                            span { class: "pencil-swatch", style: "background:{swatch}" }
                        }
                    }
                }
            }
            label { class: if other { "pencil-btn pencil-palette active" } else { "pencil-btn pencil-palette" }, title: "{palette}",
                Icon { name: "palette" }
                input { r#type: "color", value: "{hex}", "aria-label": "{palette}",
                    onchange: move |e| { if let Some(c) = parse_colour(&e.value()) { set_pencil(st, Some(c), None); } } }
            }
        }
    }
}
