//! The Export to Office Formats dialog: the format, the pages, page breaks.

use dioxus::prelude::*;
use folitecta_core::OfficeFormat;

use crate::commands::office;
use crate::i18n::t;
use crate::state::AppState;
use crate::ui::dialogs::Frame;

#[component]
pub fn ExportOfficeDialog() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let mut format =
        use_signal(|| OfficeFormat::from_id(&st.value("office.format")).unwrap_or(OfficeFormat::Docx));
    let mut all = use_signal(|| true);
    let mut breaks = use_signal(|| st.value("office.breaks") != "0");
    let formats: Vec<(&'static str, String)> = OfficeFormat::ALL
        .iter()
        .map(|f| (f.id(), t(lang, &format!("office.format.{}", f.id()))))
        .collect();
    rsx! {
        Frame { title: t(lang, "office.title"), ok_label: t(lang, "office.export"),
            on_ok: move |_| {
                st.set_value("office.format", format().id());
                st.set_value("office.breaks", if breaks() { "1" } else { "0" });
                office::export(st, format(), all(), breaks());
            },
            div { class: "field", label { "{t(lang, \"office.format\")}" }
                select { class: "office-format", onchange: move |e| { if let Some(f) = OfficeFormat::from_id(&e.value()) { format.set(f); } },
                    for (id, label) in formats { option { key: "{id}", value: "{id}", selected: id == format().id(), "{label}" } }
                }
            }
            div { class: "field", label { "{t(lang, \"office.pages\")}" }
                select { onchange: move |e| all.set(e.value() == "all"),
                    option { value: "all", selected: all(), "{t(lang, \"office.pages.all\")}" }
                    option { value: "current", selected: !all(), "{t(lang, \"office.pages.current\")}" }
                }
            }
            label { class: "check", input { r#type: "checkbox", checked: breaks(), onchange: move |_| breaks.set(!breaks()) } span { "{t(lang, \"office.breaks\")}" } }
            p { class: "hint", "{t(lang, \"office.hint\")}" }
        }
    }
}
