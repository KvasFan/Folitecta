//! The bar of View › Read Aloud, at the bottom of the window: start or
//! pause, the paragraph before or after, stop, the voice and the speed, and
//! what is being read.

use dioxus::prelude::*;

use crate::commands::read_aloud::{self as ra, READ};
use crate::i18n::{t, tf};
use crate::state::AppState;

#[component]
pub fn ReadAloudBar() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let r = READ.read().clone();
    let voice = ra::voice(st);
    let rate = ra::rate(st);
    let status = if let Some(p) = &r.problem {
        p.clone()
    } else if r.starting {
        t(lang, "read.starting")
    } else if r.running {
        let page = r.blocks.get(r.current).map(|b| b.page + 1).unwrap_or(0);
        let key = if r.paused { "read.paused" } else { "read.reading" };
        tf(
            lang,
            key,
            &[
                ("n", &(r.current + 1).to_string()),
                ("count", &r.blocks.len().to_string()),
                ("page", &page.to_string()),
            ],
        )
    } else {
        t(lang, "read.ready")
    };
    let playing = r.running && !r.paused;
    rsx! {
        div { class: "read-bar",
            button { class: if playing { "tbtn read-play on" } else { "tbtn read-play" },
                title: if playing { t(lang, "read.pause") } else { t(lang, "read.play") },
                onclick: move |_| ra::pause(st),
                if playing { "\u{23f8}" } else { "\u{25b6}" }
            }
            button { class: "tbtn read-prev", title: "{t(lang, \"read.previous\")}", disabled: !r.running,
                onclick: move |_| ra::step(st, -1), "\u{23ee}" }
            button { class: "tbtn read-next", title: "{t(lang, \"read.next\")}", disabled: !r.running,
                onclick: move |_| ra::step(st, 1), "\u{23ed}" }
            button { class: "tbtn read-stop", title: "{t(lang, \"read.stop\")}", disabled: !r.running && !r.starting,
                onclick: move |_| ra::stop(st), "\u{23f9}" }
            span { class: if r.problem.is_some() { "read-status warn" } else { "read-status muted" }, "{status}" }
            span { class: "grow" }
            if r.voices.len() > 1 {
                select { class: "read-voice", title: "{t(lang, \"read.voice\")}",
                    onchange: move |e| st.set_value("read.voice", e.value()),
                    option { value: "", selected: voice.is_empty(), "{t(lang, \"read.default_voice\")}" }
                    for v in r.voices.iter() {
                        option { key: "{v.id}", value: "{v.id}", selected: v.id == voice,
                            if v.lang.is_empty() { "{v.name}" } else { "{v.name} ({v.lang})" }
                        }
                    }
                }
            }
            label { class: "read-rate", title: "{t(lang, \"read.speed\")}",
                span { class: "muted", "{t(lang, \"read.speed\")}" }
                input { r#type: "range", min: "0.5", max: "2", step: "0.1", value: "{rate}",
                    oninput: move |e| st.set_value("read.rate", e.value()) }
                span { class: "muted read-rate-value", "{rate:.1}\u{d7}" }
            }
            button { class: "tbtn read-close", title: "{t(lang, \"read.close\")}",
                onclick: move |_| crate::commands::dispatch(st, "view.readAloud"), "\u{2715}" }
        }
    }
}
