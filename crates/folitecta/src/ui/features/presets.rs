//! The Presets window: the presets of a kind, each with its name (typed in
//! place) and a description of its settings, and the buttons of Edit ›
//! Presets.

use dioxus::prelude::*;
use folitecta_core::registry;

use crate::commands::dispatch;
use crate::commands::presets::{CHOSEN, KINDS, load, rename, summary, use_chosen};
use crate::i18n::{Lang, cmd_label, t};
use crate::icons::Icon;
use crate::state::AppState;
use crate::ui::dialogs::Frame;

fn label_of(lang: Lang, id: &str) -> String {
    registry()
        .get(id)
        .map(|c| cmd_label(lang, c))
        .unwrap_or_else(|| id.to_string())
        .trim_end_matches('…')
        .to_string()
}

#[component]
pub fn PresetsDialog() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let chosen = CHOSEN();
    let kind = chosen.kind.clone();
    // Read again when the settings change (a preset added or renamed).
    let _ = st.ui.read();
    let all = load(st, &kind);
    let any = chosen.index.is_some_and(|i| i < all.len());
    let tools: [(&str, &str, bool); 8] = [
        ("presetList.add", "new", true),
        ("presets.addNew2", "page-add", true),
        ("presetList.edit", "text-edit", any),
        ("presetList.clone", "copy", any),
        ("presetList.delete", "delete", any),
        ("presetList.up", "up", any),
        ("presetList.down", "down", any),
        ("presetList.import", "import", true),
    ];
    rsx! {
        Frame { title: label_of(lang, "presets"), wide: true,
            div { class: "field",
                label { "{t(lang, \"presets.kind\")}" }
                select { class: "preset-kind",
                    onchange: move |e| { let mut c = CHOSEN.write(); c.kind = e.value(); c.index = None; },
                    for (id, cmd, _) in KINDS.iter() {
                        option { key: "{id}", value: "{id}", selected: *id == kind, "{label_of(lang, cmd)}" }
                    }
                }
            }
            div { class: "pane-toolbar icon-toolbar",
                for (id, icon, enabled) in tools {
                    button { class: "ibtn", key: "{id}", "data-cmd": "{id}", title: "{label_of(lang, id)}", disabled: !enabled,
                        onclick: move |_| dispatch(st, id), Icon { name: icon } }
                }
                button { class: "ibtn", "data-cmd": "presetList.export", title: "{label_of(lang, \"presetList.export\")}", disabled: all.is_empty(),
                    onclick: move |_| dispatch(st, "presetList.export"), Icon { name: "export" } }
                div { class: "spacer" }
                button { class: "tbtn primary preset-use", disabled: !any, onclick: move |_| use_chosen(st), "{t(lang, \"presets.use\")}" }
            }
            div { class: "preset-list",
                if all.is_empty() { div { class: "pane-empty", "{t(lang, \"presets.none\")}" } }
                for (i, (name, values)) in all.iter().cloned().enumerate() {
                    {
                        let selected = chosen.index == Some(i);
                        let text = summary(lang, &kind, &values);
                        rsx! {
                            div { class: if selected { "preset-row selected" } else { "preset-row" }, key: "{kind}-{i}-{name}",
                                onclick: move |_| CHOSEN.write().index = Some(i),
                                input { id: "preset-name-{i}", class: "preset-name", value: "{name}",
                                    onfocus: move |_| CHOSEN.write().index = Some(i),
                                    onchange: move |e| rename(st, i, &e.value()) }
                                span { class: "muted preset-summary", title: "{text}", "{text}" }
                            }
                        }
                    }
                }
            }
            p { class: "hint", "{t(lang, \"presets.hint\")}" }
        }
    }
}
