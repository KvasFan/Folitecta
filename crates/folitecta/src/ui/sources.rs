//! The list of source files of a combined document.

use dioxus::html::ModifiersInteraction;
use dioxus::prelude::*;
use folitecta_core::combine::SourceKind;
use folitecta_core::registry;

use crate::commands::dispatch;
use crate::commands::sources::{SOURCES, combine};
use crate::i18n::{Lang, cmd_label, t, tf};
use crate::icons::Icon;
use crate::state::AppState;
use crate::ui::dialogs::Frame;

fn label_of(lang: Lang, id: &str) -> String {
    registry()
        .get(id)
        .map(|c| cmd_label(lang, c))
        .unwrap_or_else(|| id.to_string())
}

fn kind_key(k: SourceKind) -> &'static str {
    match k {
        SourceKind::Pdf => "sources.kind.pdf",
        SourceKind::Image => "sources.kind.image",
        SourceKind::Text => "sources.kind.text",
    }
}

#[component]
pub fn CombineDialog() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let s = SOURCES();
    let any = !s.selected.is_empty();
    let tools: [(&str, &str, bool); 9] = [
        ("newDocumentList.addFile", "new", true),
        ("newDocumentList.addFolder", "folder", true),
        ("newDocumentList.import", "import", true),
        ("newDocumentList.remove", "delete", any),
        ("newDocumentList.top", "first", any),
        ("newDocumentList.up", "up", any),
        ("newDocumentList.down", "down", any),
        ("newDocumentList.bottom", "last", any),
        ("newDocumentList.viewMode", "list", true),
    ];
    let total: usize = s.items.iter().map(|i| i.pages.unwrap_or(0)).sum();
    let summary = tf(
        lang,
        "sources.summary",
        &[
            ("files", &s.items.len().to_string()),
            ("pages", &total.to_string()),
        ],
    );
    rsx! {
        Frame { title: label_of(lang, "newDocumentList"), wide: true, ok_label: Some(label_of(lang, "newDocCombine")),
            on_ok: move |_| combine(st),
            div { class: "pane-toolbar icon-toolbar",
                for (id, icon, enabled) in tools {
                    button { class: if id == "newDocumentList.viewMode" && s.details { "ibtn on" } else { "ibtn" }, key: "{id}", title: "{label_of(lang, id)}", disabled: !enabled,
                        onclick: move |_| dispatch(st, id), Icon { name: icon } }
                }
                div { class: "spacer" }
                button { class: "tbtn", disabled: s.items.is_empty(), onclick: move |_| dispatch(st, "newDocumentList.clear"), "{label_of(lang, \"newDocumentList.clear\")}" }
            }
            div { class: if s.details { "source-list details" } else { "source-list" },
                if s.items.is_empty() { div { class: "pane-empty", "{t(lang, \"sources.empty\")}" } }
                if s.details && !s.items.is_empty() {
                    div { class: "source-row head",
                        span {} span { "{t(lang, \"sources.file\")}" } span { "{t(lang, \"sources.kind\")}" } span { "{t(lang, \"sources.pages\")}" } span { "{t(lang, \"sources.range\")}" }
                    }
                }
                for (i, it) in s.items.iter().cloned().enumerate() {
                    {
                        let selected = s.selected.contains(&i);
                        let name = it.path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                        let folder = it.path.parent().map(|p| p.display().to_string()).unwrap_or_default();
                        let pages = it.pages.map(|n| n.to_string()).unwrap_or_else(|| "?".into());
                        let pdf = it.kind == SourceKind::Pdf;
                        let icon = if pdf { "page" } else if it.kind == SourceKind::Image { "image" } else { "text" };
                        rsx! {
                            div { class: if selected { "source-row selected" } else { "source-row" }, key: "{i}-{it.path.display()}", title: "{it.path.display()}",
                                onclick: move |e| {
                                    let add = e.modifiers().ctrl() || e.modifiers().meta();
                                    let mut w = SOURCES.write();
                                    if add { if !w.selected.remove(&i) { w.selected.insert(i); } } else { w.selected = std::collections::BTreeSet::from([i]); }
                                },
                                Icon { name: icon }
                                div { class: "source-name",
                                    span { class: "tree-label", "{name}" }
                                    if s.details { span { class: "muted small", "{folder}" } }
                                }
                                if s.details { span { class: "muted", "{t(lang, kind_key(it.kind))}" } }
                                span { class: "muted", title: "{t(lang, \"sources.pages\")}", "{pages}" }
                                input { id: "source-range-{i}", class: "source-range", value: "{it.range}", disabled: !pdf,
                                    placeholder: "{t(lang, \"sources.all_pages\")}", title: "{label_of(lang, \"newDocumentList.range\")}",
                                    onclick: move |e| e.stop_propagation(),
                                    oninput: move |e| { if let Some(x) = SOURCES.write().items.get_mut(i) { x.range = e.value(); } } }
                            }
                        }
                    }
                }
            }
            p { class: "hint", "{t(lang, \"sources.hint\")} {summary}" }
        }
    }
}
