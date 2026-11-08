//! The Object Inspector: the objects of the file as they are written, from
//! the trailer, the current page or an object by its number; each entry
//! opens what it holds or the object it names; a stream shows its data.

use dioxus::prelude::*;
use folitecta_core::InspectPath;

use crate::i18n::{t, tf};
use crate::state::AppState;
use crate::ui::dialogs::Frame;

#[component]
pub fn InspectorDialog() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let mut history = use_signal(|| vec![InspectPath::trailer()]);
    let mut number = use_signal(String::new);
    let at = history.read().last().cloned().unwrap_or_default();
    let view = st.read_active(|t| t.doc.inspect(&at));
    let objects = st.read_active(|t| t.doc.object_count()).unwrap_or(0);
    let page = st
        .read_active(|t| t.doc.page_ids().get(t.page).copied())
        .flatten();
    let depth = history.read().len();
    let kind = move |k: &str| t(lang, &format!("inspect.kind.{k}"));
    let mut go = move |p: InspectPath| history.write().push(p);
    let mut go_number = move || {
        let parts: Vec<u32> = number()
            .split_whitespace()
            .filter_map(|s| s.parse().ok())
            .collect();
        if let Some(n) = parts.first() {
            let g = parts.get(1).copied().unwrap_or(0) as u16;
            history.write().push(InspectPath::object((*n, g)));
        }
    };
    rsx! {
        Frame { title: t(lang, "inspect.title"), wide: true,
            div { class: "row inspect-bar", style: "gap:4px;flex-wrap:wrap;align-items:center",
                button { class: "tbtn inspect-back", disabled: depth <= 1, onclick: move |_| { history.write().pop(); }, "{t(lang, \"inspect.back\")}" }
                button { class: "tbtn inspect-trailer", onclick: move |_| go(InspectPath::trailer()), "{t(lang, \"inspect.trailer\")}" }
                button { class: "tbtn inspect-page", disabled: page.is_none(),
                    onclick: move |_| { if let Some(id) = page { go(InspectPath::object(id)) } },
                    "{t(lang, \"inspect.page\")}" }
                input { class: "inspect-number", style: "width:7em", placeholder: "{t(lang, \"inspect.number\")}", value: "{number}",
                    oninput: move |e| number.set(e.value()),
                    onkeydown: move |e| { if e.key() == Key::Enter { e.stop_propagation(); go_number(); } } }
                button { class: "tbtn", onclick: move |_| go_number(), "{t(lang, \"inspect.go\")}" }
                span { class: "muted", style: "margin-left:auto", {tf(lang, "inspect.objects", &[("count", &objects.to_string())])} }
            }
            div { class: "inspect-path", b { "{at.title()}" } }
            match view {
                Some(Ok(v)) => rsx! {
                    p { class: "hint inspect-summary", "{v.summary}" }
                    div { class: "list inspect-entries", style: "max-height:22em;overflow:auto",
                        for (i, e) in v.entries.into_iter().enumerate() {
                            {
                                let open = e.open.clone();
                                let class = if open.is_some() { "list-item inspect-entry openable" } else { "list-item inspect-entry" };
                                rsx! {
                                    div { key: "{i}", class: "{class}", "data-key": "{e.key}",
                                        onclick: move |_| { if let Some(p) = open.clone() { go(p) } },
                                        span { class: "inspect-key", style: "display:inline-block;min-width:10em;font-family:monospace", "{e.key}" }
                                        span { class: "muted", style: "display:inline-block;min-width:7em", "{kind(e.kind)}" }
                                        span { class: "inspect-value", style: "font-family:monospace", "{e.summary}" }
                                    }
                                }
                            }
                        }
                    }
                    if let Some(s) = v.stream {
                        p { class: "hint",
                            {tf(lang, "inspect.stream", &[("length", &s.length.to_string()), ("decoded", &s.decoded.map(|d| d.to_string()).unwrap_or_else(|| "?".into()))])}
                            if !s.is_text { " · {t(lang, \"inspect.binary\")}" }
                            if s.truncated { " · {t(lang, \"inspect.truncated\")}" }
                        }
                        pre { class: "inspect-stream", style: "max-height:16em;overflow:auto;white-space:pre-wrap;font-size:12px", "{s.text}" }
                    }
                },
                Some(Err(e)) => rsx! { p { class: "hint inspect-error", {st.core_error_text(&e)} } },
                None => rsx! {},
            }
        }
    }
}
