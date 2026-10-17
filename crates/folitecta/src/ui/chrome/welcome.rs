//! Start screen shown when no document is open.

use dioxus::prelude::*;
use folitecta_core::registry;

use crate::app::icon_data_url;
use crate::commands::{implemented_ids, partial_ids};
use crate::files;
use crate::i18n::{t, tf};
use crate::icons::Icon;
use crate::state::AppState;

#[component]
pub fn Welcome() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let recent = st.ui.read().recent.clone();
    let reg = registry();
    let (done, partial, planned) = reg.status_summary(&implemented_ids(), &partial_ids());
    let stats = tf(
        lang,
        "welcome.stats",
        &[
            ("commands", &reg.len().to_string()),
            ("implemented", &(done + partial).to_string()),
            ("planned", &planned.to_string()),
        ],
    );
    let icon = icon_data_url();
    rsx! {
        div { class: "welcome",
            div { class: "hero",
                img { src: "{icon}", alt: "" }
                div {
                    h1 { "Folitecta" }
                    p { "{t(lang, \"app.tagline\")}" }
                }
            }
            div { class: "actions",
                div { class: "card", onclick: move |_| files::open_dialog(st),
                    Icon { name: "open" }
                    b { "{t(lang, \"welcome.open\")}" }
                    span { "{t(lang, \"welcome.open.hint\")}" }
                }
                div { class: "card", onclick: move |_| files::new_blank(st, 1, 595.0, 842.0),
                    Icon { name: "new" }
                    b { "{t(lang, \"welcome.new\")}" }
                    span { "{t(lang, \"welcome.new.hint\")}" }
                }
                div { class: "card", onclick: move |_| crate::commands::sources::open(st),
                    Icon { name: "merge" }
                    b { "{reg.get(\"newDocCombine\").map(|c| c.label.clone()).unwrap_or_default()}" }
                    span { "PDF + PDF → one document" }
                }
            }
            div { class: "recent",
                h3 { "{t(lang, \"welcome.recent\")}" }
                if recent.is_empty() {
                    div { class: "pane-empty", "{t(lang, \"welcome.recent.none\")}" }
                }
                for path in recent {
                    {
                        let p = path.clone();
                        let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                        let full = path.display().to_string();
                        rsx! {
                            div { class: "list-item", key: "{full}", onclick: move |_| files::open_path(st, p.clone(), None),
                                div { class: "title", Icon { name: "page" } "{name}" }
                                div { class: "meta", "{full}" }
                            }
                        }
                    }
                }
            }
            div { class: "stats", span { "{stats}" } span { "·" } span { "v{folitecta_core::VERSION}" } }
        }
    }
}
