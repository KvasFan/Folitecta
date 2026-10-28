//! The quick panel of the drawing tools (FOL-L025): while the Pencil, the
//! free highlighter or a shape tool is the tool, a small bar over the pages
//! with the thickness of the line, four colours of the tool (black, green,
//! blue and red; yellow, green, pink and blue for the highlighter) and the
//! colour palette for any other, and for closed shapes their fill. They
//! change the default style of the tool's comments, so the next ones follow.

use dioxus::prelude::*;

use crate::commands::paths::{colour_hex, parse_colour};
use crate::i18n::t;
use crate::icons::Icon;
use crate::state::comment_styles::{default_style, set_default_style, tool_subtype};
use crate::state::{AppState, Tool};

/// The colours of the pencil and the shapes: key of their name, and the colour.
pub const COLOURS: [(&str, [f32; 3]); 4] = [
    ("black", [0.0, 0.0, 0.0]),
    ("green", [0.0, 0.6, 0.2]),
    ("blue", [0.1, 0.35, 0.9]),
    ("red", [0.86, 0.1, 0.1]),
];

/// The colours of the free highlighter.
pub const MARKER_COLOURS: [(&str, [f32; 3]); 4] = [
    ("yellow", [1.0, 0.85, 0.0]),
    ("green", [0.35, 0.85, 0.35]),
    ("pink", [1.0, 0.45, 0.75]),
    ("blue", [0.35, 0.7, 1.0]),
];

/// The thicknesses offered, in points.
pub const WIDTHS: [f32; 6] = [0.5, 1.0, 2.0, 3.0, 5.0, 8.0];

/// The thicknesses of the free highlighter, in points.
pub const MARKER_WIDTHS: [f32; 6] = [6.0, 9.0, 12.0, 16.0, 20.0, 28.0];

/// The tools that have the panel.
pub fn has_panel(tool: Tool) -> bool {
    matches!(
        tool,
        Tool::Ink
            | Tool::FreeHighlight
            | Tool::Rect
            | Tool::Ellipse
            | Tool::Line
            | Tool::Arrow
            | Tool::Polygon
            | Tool::Polyline
            | Tool::Cloud
    )
}

/// Closed shapes, which have a fill.
fn closed(tool: Tool) -> bool {
    matches!(tool, Tool::Rect | Tool::Ellipse | Tool::Polygon | Tool::Cloud)
}

/// What the panel changes in the style of the next comments of a tool.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Change {
    Colour([f32; 3]),
    Width(f32),
    /// The fill of closed shapes; `None` for no fill.
    Fill(Option<[f32; 3]>),
}

/// Change the default style of the comments `tool` makes.
pub fn set_tool_style(st: AppState, tool: Tool, change: Change) {
    let Some(kind) = tool_subtype(tool) else { return };
    let mut style = default_style(st, kind);
    match change {
        Change::Colour(c) => style.color = Some(c),
        Change::Width(w) => style.width = w.clamp(0.25, 50.0),
        Change::Fill(f) => style.fill = f,
    }
    set_default_style(st, kind, &style);
}

fn same(a: [f32; 3], b: [f32; 3]) -> bool {
    a.iter().zip(b).all(|(x, y)| (x - y).abs() < 0.01)
}

/// Which popover of the panel is open.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Open {
    None,
    Widths,
    Fill,
}

#[component]
pub fn QuickPanel(tool: Tool) -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let mut open = use_signal(|| Open::None);
    let Some(kind) = tool_subtype(tool) else {
        return rsx! {};
    };
    let marker = tool == Tool::FreeHighlight;
    let (colours, widths) = if marker {
        (MARKER_COLOURS, MARKER_WIDTHS)
    } else {
        (COLOURS, WIDTHS)
    };
    let style = default_style(st, kind);
    let colour = style.color.unwrap_or([0.0; 3]);
    let width = style.width;
    let fill = style.fill;
    let width_text = format!("{}", (width * 10.0).round() / 10.0);
    let thickness = t(lang, "pencil.thickness");
    let palette = t(lang, "pencil.palette");
    let fill_name = t(lang, "pencil.fill");
    let no_fill = t(lang, "pencil.no_fill");
    let panel_name = t(lang, tool.label_key());
    let hex = colour_hex(colour);
    let other = !colours.iter().any(|(_, c)| same(*c, colour));
    let fill_swatch = fill.map(colour_hex);
    let fill_hex = fill_swatch.clone().unwrap_or_else(|| "#ffffff".into());
    // The line drawn in the thickness buttons, never wider than the button.
    let shown = move |w: f32| {
        if marker {
            (w / 2.5).clamp(2.0, 12.0)
        } else {
            w.clamp(1.0, 10.0)
        }
    };
    let line = shown(width);
    let opacity = if marker { "opacity:.55;" } else { "" };
    rsx! {
        div { class: "pencil-panel quick-panel", role: "toolbar", "aria-label": "{panel_name}", "data-tool": "{kind}",
            onmousedown: move |e| e.stop_propagation(),
            div { class: "pencil-width-wrap",
                button { class: if open() == Open::Widths { "pencil-btn pencil-width active" } else { "pencil-btn pencil-width" },
                    title: "{thickness}: {width_text} pt", "aria-label": "{thickness}",
                    onclick: move |_| open.set(if open() == Open::Widths { Open::None } else { Open::Widths }),
                    span { class: "pencil-line", style: "height:{line}px" }
                    span { class: "pencil-width-text", "{width_text}" }
                }
                if open() == Open::Widths {
                    div { class: "pencil-widths",
                        for w in widths {
                            {
                                let label = format!("{w}");
                                let line = shown(w);
                                let on = (w - width).abs() < 0.01;
                                rsx! {
                                    button { key: "{label}", class: if on { "pencil-btn active" } else { "pencil-btn" }, title: "{label} pt",
                                        "data-width": "{label}",
                                        onclick: move |_| { set_tool_style(st, tool, Change::Width(w)); open.set(Open::None); },
                                        span { class: "pencil-line", style: "height:{line}px" }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            for (key, c) in colours {
                {
                    let name = t(lang, &format!("pencil.{key}"));
                    let on = same(c, colour);
                    let swatch = colour_hex(c);
                    rsx! {
                        button { key: "{key}", class: if on { "pencil-btn pencil-colour active" } else { "pencil-btn pencil-colour" },
                            "data-colour": "{key}", title: "{name}", "aria-label": "{name}", "aria-pressed": "{on}",
                            onclick: move |_| set_tool_style(st, tool, Change::Colour(c)),
                            span { class: "pencil-swatch", style: "{opacity}background:{swatch}" }
                        }
                    }
                }
            }
            label { class: if other { "pencil-btn pencil-palette active" } else { "pencil-btn pencil-palette" }, title: "{palette}",
                Icon { name: "palette" }
                input { r#type: "color", value: "{hex}", "aria-label": "{palette}",
                    onchange: move |e| { if let Some(c) = parse_colour(&e.value()) { set_tool_style(st, tool, Change::Colour(c)); } } }
            }
            if closed(tool) {
                div { class: "pencil-width-wrap",
                    button { class: if open() == Open::Fill { "pencil-btn pencil-fill active" } else { "pencil-btn pencil-fill" },
                        title: "{fill_name}", "aria-label": "{fill_name}",
                        onclick: move |_| open.set(if open() == Open::Fill { Open::None } else { Open::Fill }),
                        match fill_swatch.clone() {
                            Some(s) => rsx! { span { class: "pencil-swatch square", style: "background:{s}" } },
                            None => rsx! { span { class: "pencil-swatch square empty" } },
                        }
                    }
                    if open() == Open::Fill {
                        div { class: "pencil-widths pencil-fills",
                            button { class: if fill.is_none() { "pencil-btn active" } else { "pencil-btn" }, "data-fill": "none", title: "{no_fill}",
                                onclick: move |_| { set_tool_style(st, tool, Change::Fill(None)); open.set(Open::None); },
                                span { class: "pencil-swatch square empty" }
                            }
                            for (key, c) in colours {
                                {
                                    let name = t(lang, &format!("pencil.{key}"));
                                    let on = fill.is_some_and(|f| same(f, c));
                                    let swatch = colour_hex(c);
                                    rsx! {
                                        button { key: "{key}", class: if on { "pencil-btn active" } else { "pencil-btn" }, "data-fill": "{key}", title: "{name}",
                                            onclick: move |_| { set_tool_style(st, tool, Change::Fill(Some(c))); open.set(Open::None); },
                                            span { class: "pencil-swatch square", style: "background:{swatch}" }
                                        }
                                    }
                                }
                            }
                            label { class: "pencil-btn pencil-palette", title: "{palette}",
                                Icon { name: "palette" }
                                input { r#type: "color", value: "{fill_hex}", "aria-label": "{palette}",
                                    onchange: move |e| { if let Some(c) = parse_colour(&e.value()) { set_tool_style(st, tool, Change::Fill(Some(c))); open.set(Open::None); } } }
                            }
                        }
                    }
                }
            }
        }
    }
}
