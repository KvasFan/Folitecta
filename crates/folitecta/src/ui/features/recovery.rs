//! The Document Recovery dialog: the documents with unsaved changes left by
//! a window that ended, opened again or deleted.

use dioxus::prelude::*;
use folitecta_core::registry;

use crate::i18n::{cmd_label, t, tf};
use crate::recovery::{Entry, discard, recover, recoverable};
use crate::state::{AppState, Dialog};
use crate::ui::dialogs::Frame;
use crate::ui::panes::history::when;

#[component]
pub fn RecoveryDialog() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    // Listed again after every action.
    let mut generation = use_signal(|| 0u32);
    let _ = generation();
    let entries: Vec<Entry> = recoverable();
    let title = registry()
        .get("view.recovery")
        .map(|c| cmd_label(lang, c))
        .unwrap_or_default();
    let all = entries.clone();
    rsx! {
        Frame { title, ok_label: Some(t(lang, "recovery.open_all")),
            on_ok: move |_| {
                for e in all.iter() {
                    recover(st, e);
                }
                st.set_dialog(Dialog::None);
            },
            if entries.is_empty() {
                p { class: "hint", "{t(lang, \"recovery.none\")}" }
            } else {
                p { class: "hint", "{t(lang, \"recovery.hint\")}" }
                div { class: "recovery-list",
                    for e in entries.into_iter() {
                        {
                            let place = e.path.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|| t(lang, "recovery.unsaved"));
                            let meta = tf(lang, "recovery.meta", &[("pages", &e.pages.to_string()), ("when", &when(e.saved))]);
                            let (open, drop) = (e.clone(), e.clone());
                            rsx! {
                                div { class: "recovery-row", key: "{e.session}-{e.tab}",
                                    div { class: "recovery-text",
                                        span { class: "recovery-name", "{e.title}" }
                                        span { class: "muted ellipsis", "{place}" }
                                        span { class: "muted", "{meta}" }
                                    }
                                    button { class: "tbtn recovery-open", onclick: move |_| { recover(st, &open); generation += 1; },
                                        "{t(lang, \"recovery.open\")}" }
                                    button { class: "tbtn danger recovery-delete", onclick: move |_| { discard(&drop); generation += 1; },
                                        "{t(lang, \"recovery.delete\")}" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
