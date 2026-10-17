//! The Find and Replace dialog: what to find and what to put instead, the
//! options of the search, and the occurrences replaced one by one or all.

use dioxus::prelude::*;
use folitecta_core::registry;

use crate::commands::find_replace::{find_next, replace_all, replace_one};
use crate::i18n::{cmd_label, t, tf};
use crate::state::AppState;
use crate::ui::dialogs::Frame;

#[component]
pub fn FindReplaceDialog(find: String) -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let mut what = use_signal(|| find.clone());
    let mut with = use_signal(String::new);
    let mut page_only = use_signal(|| false);
    let (case, words) = st
        .read_active(|t| (t.search.case_sensitive, t.search.whole_words))
        .unwrap_or((false, false));
    let found = st
        .read_active(|t| (t.search.query == what()).then_some((t.search.current, t.search.hits.len())))
        .flatten();
    let count = match found {
        Some((_, 0)) => t(lang, "toast.not_found"),
        Some((cur, n)) => tf(
            lang,
            "findrepl.count",
            &[("current", &(cur + 1).to_string()), ("count", &n.to_string())],
        ),
        None => String::new(),
    };
    let empty = what().is_empty();
    let title = registry()
        .get("findAndRepl")
        .map(|c| cmd_label(lang, c).trim_end_matches('…').to_string())
        .unwrap_or_default();
    // A changed option finds again.
    let forget = move || st.with_active(|t| t.search.hits.clear());
    rsx! {
        Frame { title, ok_label: Some(t(lang, "findrepl.next")),
            on_ok: move |_| { find_next(st, &what()); },
            div { class: "field", label { "{t(lang, \"findrepl.find\")}" }
                input { class: "findrepl-find", value: "{what}", autofocus: true, oninput: move |e| what.set(e.value()) }
            }
            div { class: "field", label { "{t(lang, \"findrepl.with\")}" }
                input { class: "findrepl-with", value: "{with}", oninput: move |e| with.set(e.value()) }
            }
            div { class: "check-row",
                label { class: "check",
                    input { r#type: "checkbox", checked: case,
                        onchange: move |_| { st.with_active(|t| t.search.case_sensitive = !t.search.case_sensitive); forget(); } }
                    span { "{t(lang, \"findrepl.case\")}" }
                }
                label { class: "check",
                    input { r#type: "checkbox", checked: words,
                        onchange: move |_| { st.with_active(|t| t.search.whole_words = !t.search.whole_words); forget(); } }
                    span { "{t(lang, \"findrepl.words\")}" }
                }
            }
            div { class: "radio-row",
                label { input { r#type: "radio", name: "findrepl-scope", checked: !page_only(), onchange: move |_| page_only.set(false) }
                    span { "{t(lang, \"findrepl.all_pages\")}" } }
                label { input { r#type: "radio", name: "findrepl-scope", checked: page_only(), onchange: move |_| page_only.set(true) }
                    span { "{t(lang, \"findrepl.this_page\")}" } }
            }
            div { class: "row",
                button { class: "tbtn findrepl-replace", disabled: empty, onclick: move |_| replace_one(st, &what(), &with()),
                    "{t(lang, \"findrepl.replace\")}" }
                button { class: "tbtn findrepl-all", disabled: empty, onclick: move |_| replace_all(st, &what(), &with(), page_only()),
                    "{t(lang, \"findrepl.replace_all\")}" }
                span { class: "muted grow", "{count}" }
            }
            p { class: "hint", "{t(lang, \"findrepl.hint\")}" }
        }
    }
}
