//! The Export Tables dialog: the tables found on the pages, each with its
//! page, size and first row, to choose; the format and the pages.

use dioxus::prelude::*;
use folitecta_core::{FoundTable, TableFormat};

use crate::commands::table_export;
use crate::i18n::{t, tf};
use crate::state::AppState;
use crate::ui::dialogs::Frame;

/// The first row of a table, shortened.
fn first_row(table: &FoundTable) -> String {
    let row = table
        .rows
        .first()
        .map(|r| {
            r.iter()
                .filter(|c| !c.is_empty())
                .cloned()
                .collect::<Vec<_>>()
                .join(" · ")
        })
        .unwrap_or_default();
    if row.chars().count() > 70 {
        format!("{}…", row.chars().take(69).collect::<String>())
    } else {
        row
    }
}

#[component]
pub fn ExportTablesDialog() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let mut format =
        use_signal(|| TableFormat::from_id(&st.value("sheets.format")).unwrap_or(TableFormat::Xlsx));
    let mut all = use_signal(|| true);
    let mut chosen = use_signal(Vec::<bool>::new);
    let found = use_resource(move || async move {
        let tables = table_export::find(st, all()).await;
        chosen.set(vec![true; tables.len()]);
        tables
    });
    let tables = found.read().clone();
    let picked: Vec<FoundTable> = tables
        .iter()
        .flatten()
        .zip(chosen.read().iter())
        .filter(|(_, c)| **c)
        .map(|(t, _)| t.clone())
        .collect();
    let none_picked = picked.is_empty();
    let formats: Vec<(&'static str, String)> = TableFormat::ALL
        .iter()
        .map(|f| (f.id(), t(lang, &format!("sheets.format.{}", f.id()))))
        .collect();
    rsx! {
        Frame { title: t(lang, "sheets.title"), ok_label: t(lang, "sheets.export"), wide: true,
            ok_disabled: none_picked,
            on_ok: move |_| {
                st.set_value("sheets.format", format().id());
                table_export::export(st, format(), picked.clone());
            },
            div { class: "field", label { "{t(lang, \"sheets.format\")}" }
                select { class: "sheets-format", onchange: move |e| { if let Some(f) = TableFormat::from_id(&e.value()) { format.set(f); } },
                    for (id, label) in formats { option { key: "{id}", value: "{id}", selected: id == format().id(), "{label}" } }
                }
            }
            div { class: "field", label { "{t(lang, \"sheets.pages\")}" }
                select { class: "sheets-pages", onchange: move |e| all.set(e.value() == "all"),
                    option { value: "all", selected: all(), "{t(lang, \"office.pages.all\")}" }
                    option { value: "current", selected: !all(), "{t(lang, \"office.pages.current\")}" }
                }
            }
            match tables {
                None => rsx! { p { class: "hint sheets-looking", "{t(lang, \"sheets.looking\")}" } },
                Some(list) if list.is_empty() => rsx! { p { class: "hint sheets-none", "{t(lang, \"sheets.none\")}" } },
                Some(list) => rsx! {
                    div { class: "list sheets-tables",
                        for (i, table) in list.iter().enumerate() {
                            label { key: "{i}", class: "check sheets-table", "data-page": "{table.page}",
                                input { r#type: "checkbox",
                                    checked: chosen.read().get(i).copied().unwrap_or(false),
                                    onchange: move |_| {
                                        let mut c = chosen.write();
                                        if let Some(v) = c.get_mut(i) { *v = !*v; }
                                    }
                                }
                                span {
                                    b { {tf(lang, "sheets.item", &[
                                        ("page", &(table.page + 1).to_string()),
                                        ("rows", &table.rows.len().to_string()),
                                        ("columns", &table.columns().to_string()),
                                    ])} }
                                    " {first_row(table)}"
                                }
                            }
                        }
                    }
                },
            }
            p { class: "hint", "{t(lang, \"sheets.hint\")}" }
        }
    }
}
