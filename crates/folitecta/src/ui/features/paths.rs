//! The bar over the path chosen with Edit Paths: its stroke and fill
//! colours, its line width, Delete; with a node chosen, its conversions.

use dioxus::prelude::*;

use crate::commands::paths::{self, colour_hex, parse_colour};
use crate::i18n::t;
use crate::icons::Icon;
use crate::state::AppState;

/// The commands of the node chosen, with the tooltips of their buttons.
const NODE_COMMANDS: [(&str, &str); 4] = [
    ("pathedit.convert.corner", "paths.corner"),
    ("pathedit.convert.smooth", "paths.smooth"),
    ("pathedit.convert.straight", "paths.straight"),
    ("pathedit.anchors.add", "paths.add_node"),
];

#[component]
pub fn PathToolbar(
    left: f32,
    top: f32,
    stroke: Option<[f32; 3]>,
    fill: Option<[f32; 3]>,
    stroked: bool,
    filled: bool,
    width: f32,
    /// A node of the path is chosen: the bar converts it.
    #[props(default = false)]
    node_chosen: bool,
) -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let stroke_hex = colour_hex(stroke.unwrap_or([0.0; 3]));
    let fill_hex = colour_hex(fill.unwrap_or([0.0; 3]));
    let width_text = format!("{}", (width * 10.0).round() / 10.0);
    rsx! {
        div { class: "sel-toolbar path-toolbar", style: "left:{left}px;top:{top}px",
            onmousedown: move |e| e.stop_propagation(),
            if stroked {
                label { class: "path-colour", title: "{t(lang, \"paths.stroke\")}",
                    Icon { name: "pencil" }
                    input { class: "path-stroke", r#type: "color", value: "{stroke_hex}",
                        onchange: move |e| { if let Some(c) = parse_colour(&e.value()) { paths::restyle(st, Some(c), None, None); } } }
                }
                input { class: "path-width", r#type: "number", min: "0", step: "0.5", value: "{width_text}", title: "{t(lang, \"paths.width\")}",
                    onchange: move |e| { if let Ok(w) = e.value().trim().replace(',', ".").parse::<f32>() { paths::restyle(st, None, None, Some(w)); } } }
            }
            if filled {
                label { class: "path-colour", title: "{t(lang, \"paths.fill\")}",
                    Icon { name: "palette" }
                    input { class: "path-fill", r#type: "color", value: "{fill_hex}",
                        onchange: move |e| { if let Some(c) = parse_colour(&e.value()) { paths::restyle(st, None, Some(c), None); } } }
                }
            }
            if node_chosen {
                for (id, key) in NODE_COMMANDS {
                    button { key: "{id}", class: "path-node-cmd", "data-cmd": "{id}", title: "{t(lang, key)}",
                        onclick: move |e| { e.stop_propagation(); crate::commands::dispatch(st, id); },
                        Icon { name: crate::icons::for_command(id) }
                    }
                }
            }
            button { class: "path-delete", title: "{t(lang, \"paths.delete\")}", onclick: move |e| { e.stop_propagation(); paths::delete(st); }, Icon { name: "delete" } }
        }
    }
}
