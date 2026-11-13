//! Bates numbering of several files: the list of files, its columns and the
//! numbers given.

use dioxus::html::ModifiersInteraction;
use dioxus::prelude::*;
use folitecta_core::registry;

use crate::commands::bates_files::{BATES_FILES, COLUMNS, column_on, number};
use crate::commands::dispatch;
use crate::i18n::{Lang, cmd_label, t};
use crate::icons::Icon;
use crate::state::AppState;
use crate::ui::dialogs::Frame;

fn label_of(lang: Lang, id: &str) -> String {
    registry()
        .get(id)
        .map(|c| cmd_label(lang, c))
        .unwrap_or_else(|| id.to_string())
}

/// The text of a column for a file.
fn cell(f: &crate::commands::bates_files::BatesFile, column: &str) -> String {
    match column {
        "batesNumberingFiles.hdr.title" => f.title.clone(),
        "batesNumberingFiles.hdr.location" => f
            .path
            .parent()
            .map(|p| p.display().to_string())
            .unwrap_or_default(),
        "batesNumberingFiles.hdr.type" => f
            .path
            .extension()
            .map(|e| e.to_string_lossy().to_uppercase())
            .unwrap_or_default(),
        "batesNumberingFiles.hdr.size" => crate::util::human_size(f.size),
        "batesNumberingFiles.hdr.modified" => folitecta_core::util::unix_display(f.modified.unwrap_or(0)),
        "batesNumberingFiles.hdr.pagesCount" => f.pages.map(|n| n.to_string()).unwrap_or_else(|| "?".into()),
        "batesNumberingFiles.hdr.pagesSize" => f
            .page_size
            .map(|(w, h)| format!("{:.0} × {:.0} mm", w / 72.0 * 25.4, h / 72.0 * 25.4))
            .unwrap_or_default(),
        _ => String::new(),
    }
}

#[component]
pub fn BatesFilesDialog() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let list = BATES_FILES();
    let any = !list.selected.is_empty();
    let mut format = use_signal(|| crate::commands::macros::bates_format(st));
    let mut start = use_signal(|| crate::commands::macros::bates_next(st).to_string());
    let columns: Vec<&'static str> = COLUMNS.iter().copied().filter(|c| column_on(st, c)).collect();
    let mut columns_open = use_signal(|| false);
    let tools: [(&str, &str, bool); 6] = [
        ("list.add", "new", true),
        ("list.edit", "text-edit", any),
        ("list.delete", "delete", any),
        ("list.up", "up", any),
        ("list.down", "down", any),
        ("list.clear", "eraser", !list.items.is_empty()),
    ];
    rsx! {
        Frame { title: label_of(lang, "document.batesNumbering.addMulti").trim_end_matches('…').to_string(), wide: true,
            ok_label: Some(t(lang, "bates_files.number")),
            ok_disabled: list.items.is_empty(),
            on_ok: move |_| {
                st.set_value("bates.format", format());
                st.set_value("bates.next", start().trim().parse::<i64>().unwrap_or(1).max(0).to_string());
                number(st);
            },
            div { class: "pane-toolbar icon-toolbar",
                for (id, icon, enabled) in tools {
                    button { class: "ibtn", key: "{id}", "data-cmd": "{id}", title: "{label_of(lang, id)}", disabled: !enabled,
                        onclick: move |_| dispatch(st, id), Icon { name: icon } }
                }
                div { class: "spacer" }
                div { class: "dropdown-wrap",
                    button { class: "tbtn bates-columns", onclick: move |_| columns_open.toggle(), "{label_of(lang, \"batesNumberingFiles.hdr\")}" }
                    if columns_open() {
                        div { class: "menu-dropdown bates-columns-menu",
                            for c in COLUMNS.iter().copied() {
                                {
                                    let on = column_on(st, c);
                                    rsx! {
                                        label { class: "check", key: "{c}", input { r#type: "checkbox", "data-col": "{c}", checked: on, onchange: move |_| dispatch(st, c) } span { "{label_of(lang, c)}" } }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            div { class: "bates-files-wrap",
                if list.items.is_empty() { div { class: "pane-empty", "{t(lang, \"bates_files.empty\")}" } }
                else {
                    table { class: "bates-files",
                        thead { tr {
                            th { "{t(lang, \"bates_files.file\")}" }
                            for c in columns.iter() { th { key: "{c}", "{label_of(lang, c)}" } }
                        } }
                        tbody {
                            for (i, f) in list.items.iter().cloned().enumerate() {
                                {
                                    let selected = list.selected.contains(&i);
                                    let name = f.path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                                    let cols = columns.clone();
                                    rsx! {
                                        tr { class: if selected { "selected" } else { "" }, key: "{i}-{f.path.display()}", title: "{f.path.display()}",
                                            onclick: move |e| {
                                                let add = e.modifiers().ctrl() || e.modifiers().meta();
                                                let mut w = BATES_FILES.write();
                                                if add { if !w.selected.remove(&i) { w.selected.insert(i); } } else { w.selected = std::collections::BTreeSet::from([i]); }
                                            },
                                            td { class: "name", "{name}" }
                                            for c in cols.into_iter() {
                                                if c == "batesNumberingFiles.hdr.pagesRange" {
                                                    td { key: "{c}",
                                                        input { id: "bates-range-{i}", class: "source-range", value: "{f.range}",
                                                            placeholder: "{t(lang, \"sources.all_pages\")}",
                                                            onclick: move |e| e.stop_propagation(),
                                                            oninput: move |e| { if let Some(x) = BATES_FILES.write().items.get_mut(i) { x.range = e.value(); } } }
                                                    }
                                                } else {
                                                    td { key: "{c}", class: "muted", "{cell(&f, c)}" }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            div { class: "field-grid",
                div { class: "field", label { "{t(lang, \"dialog.numbers.format\")}" }
                    input { value: "{format}", oninput: move |e| format.set(e.value()) } }
                div { class: "field", label { "{t(lang, \"dialog.numbers.start\")}" }
                    input { r#type: "number", min: "0", value: "{start}", oninput: move |e| start.set(e.value()) } }
            }
            p { class: "hint", "{t(lang, \"bates_files.hint\")}" }
        }
    }
}
