//! The comment styles pane, the palette of styles for the current tool and
//! the dialog that names a style.

use dioxus::prelude::*;
use folitecta_core::{AnnotStyle, registry};

use crate::commands::comment_styles::{EDITING, STYLES, current_kind, default_name, use_for_tool};
use crate::commands::dispatch;
use crate::i18n::{Lang, annot_name, cmd_label, t};
use crate::icons::Icon;
use crate::state::{AppState, Dialog};
use crate::styles::{KINDS, default_style, named_styles, save_named_styles, tool_subtype};
use crate::ui::dialogs::Frame;
use crate::util::color_hex;

fn label_of(lang: Lang, id: &str) -> String {
    registry()
        .get(id)
        .map(|c| cmd_label(lang, c))
        .unwrap_or_else(|| id.to_string())
}

/// A small sample of a style: its border, fill and line.
#[component]
fn Swatch(style: AnnotStyle) -> Element {
    let border = style
        .color
        .map(|c| format!("{}px solid {}", style.width.clamp(1.0, 4.0), color_hex(c)))
        .unwrap_or_else(|| "1px dashed var(--border)".into());
    let dash = if style.dashed { "border-style:dashed;" } else { "" };
    let fill = style.fill.map(color_hex).unwrap_or_else(|| "transparent".into());
    let opacity = style.opacity.clamp(0.1, 1.0);
    rsx! {
        span { class: "style-swatch", style: "border:{border};{dash}background:{fill};opacity:{opacity}" }
    }
}

const TOOLS: &[(&str, &str)] = &[
    ("commentStyles.applyStyle", "check"),
    ("commentStyles.clone", "copy"),
    ("commentStyles.rename", "pencil"),
    ("commentStyles.delete", "delete"),
    ("commentStyles.showProps", "settings"),
    ("commentStyles.resetStyle", "undo"),
];

#[component]
pub fn CommentStylesPane() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    // The pane starts on the kind of the selected comment or of the tool.
    use_hook(move || {
        let kind = current_kind(st);
        spawn(async move {
            STYLES.write().kind = kind;
        });
    });
    let v = STYLES();
    let kind = v.kind.clone();
    let named: Vec<(usize, crate::styles::NamedStyle)> = named_styles(st)
        .into_iter()
        .enumerate()
        .filter(|(_, n)| n.kind == kind)
        .collect();
    let default = default_style(st, &kind);
    let kind_for_default = kind.clone();
    rsx! {
        div { class: "pane-toolbar icon-toolbar",
            for (id, icon) in TOOLS.iter().copied() {
                button { class: "ibtn", key: "{id}", title: "{label_of(lang, id)}", onclick: move |_| dispatch(st, id), Icon { name: icon } }
            }
            div { class: "spacer" }
            button { class: "tbtn", title: "{label_of(lang, \"commentStyles.reset\")}", onclick: move |_| dispatch(st, "commentStyles.reset"), "{label_of(lang, \"commentStyles.reset\")}" }
        }
        div { class: "styles-kind",
            select { onchange: move |e| { let mut w = STYLES.write(); w.kind = e.value(); w.chosen = None; },
                for k in KINDS.iter().copied() {
                    option { key: "{k}", value: "{k}", selected: k == kind, "{annot_name(lang, k)}" }
                }
            }
        }
        div { class: "tree styles-list",
            div { class: if v.chosen == Some(None) { "tree-item style-row selected" } else { "tree-item style-row" },
                onclick: move |_| STYLES.write().chosen = Some(None),
                ondoubleclick: move |_| dispatch(st, "commentStyles.showProps"),
                Swatch { style: default }
                span { class: "tree-label", "{default_name(lang, &kind_for_default)}" }
            }
            for (i, n) in named.into_iter() {
                div { key: "{i}", class: if v.chosen == Some(Some(i)) { "tree-item style-row selected" } else { "tree-item style-row" },
                    onclick: move |_| STYLES.write().chosen = Some(Some(i)),
                    ondoubleclick: move |_| dispatch(st, "commentStyles.showProps"),
                    Swatch { style: n.style() }
                    span { class: "tree-label", "{n.name}" }
                }
            }
        }
        p { class: "hint pad", "{t(lang, \"styles.hint\")}" }
        if EDITING().is_some() {
            p { class: "hint pad", "{t(lang, \"styles.editing\")}" }
        }
    }
}

/// The styles of the current tool, to pick its default style with a click.
#[component]
pub fn StylesPalette() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let tool = (st.tool)();
    let kind = tool_subtype(tool);
    let current = kind.map(|k| default_style(st, k));
    let named: Vec<(usize, crate::styles::NamedStyle)> = match kind {
        Some(k) => named_styles(st)
            .into_iter()
            .enumerate()
            .filter(|(_, n)| n.kind == k)
            .collect(),
        None => Vec::new(),
    };
    rsx! {
        div { class: "styles-palette",
            div { class: "styles-palette-head",
                span { class: "ellipsis", "{label_of(lang, \"view.commentStyles\")}" }
                button { class: "ibtn", title: "{t(lang, \"dialog.close\")}", onclick: move |_| st.set_option("view.commentStyles", false), "×" }
            }
            match kind {
                None => rsx! { p { class: "hint pad", "{t(lang, \"styles.palette_hint\")}" } },
                Some(k) => rsx! {
                    button { class: "palette-item", title: "{t(lang, \"styles.builtin\")}", onclick: move |_| use_for_tool(st, k, None),
                        Swatch { style: crate::styles::builtin(k) }
                        span { class: "ellipsis", "{t(lang, \"styles.builtin\")}" }
                    }
                    for (i, n) in named.into_iter() {
                        {
                            let on = current.as_ref() == Some(&n.style());
                            rsx! {
                                button { key: "{i}", class: if on { "palette-item on" } else { "palette-item" }, title: "{n.name}", onclick: move |_| use_for_tool(st, k, Some(i)),
                                    Swatch { style: n.style() }
                                    span { class: "ellipsis", "{n.name}" }
                                }
                            }
                        }
                    }
                },
            }
        }
    }
}

/// The name of a named comment style.
#[component]
pub fn StyleNameDialog(index: usize) -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let mut name = use_signal(|| {
        named_styles(st)
            .get(index)
            .map(|n| n.name.clone())
            .unwrap_or_default()
    });
    rsx! {
        Frame { title: label_of(lang, "commentStyles.rename"),
            on_ok: move |_| {
                let mut list = named_styles(st);
                if let Some(n) = list.get_mut(index) {
                    let v = name().trim().to_string();
                    if !v.is_empty() {
                        n.name = v;
                        save_named_styles(st, &list);
                    }
                }
                st.set_dialog(Dialog::None);
            },
            div { class: "field", label { "{t(lang, \"styles.name\")}" }
                input { value: "{name}", autofocus: true, oninput: move |e| name.set(e.value()) } }
        }
    }
}
