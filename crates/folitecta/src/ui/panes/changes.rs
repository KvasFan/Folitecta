//! The History of Changes pane (FOL-L004): the undo steps of the document,
//! each named after the command or tool that made it, the oldest first, and
//! after them the steps Redo makes again. A click goes back, or forward, to
//! the state after that step.

use dioxus::prelude::*;

use crate::files;
use crate::i18n::t;
use crate::icons::Icon;
use crate::state::AppState;

/// Undo (`by` below 0) or redo as many steps, in one go.
fn go(st: AppState, by: isize) {
    if by == 0 {
        return;
    }
    st.with_active(|t| {
        for _ in 0..by.unsigned_abs() {
            let moved = if by < 0 { t.doc.undo() } else { t.doc.redo() };
            if moved.is_err() {
                break;
            }
        }
    });
    files::set_window_title(st);
}

#[component]
pub fn ChangesPane() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let Some((done, ahead)) = st.read_active(|t| (t.doc.undo_steps(), t.doc.redo_steps())) else {
        return rsx! {};
    };
    let name = |n: &Option<String>| n.clone().unwrap_or_else(|| t(lang, "pane.changes.change"));
    let count = done.len() as isize;
    rsx! {
        div { class: "changes-list",
            if done.is_empty() && ahead.is_empty() {
                div { class: "pane-empty", "{t(lang, \"pane.changes.none\")}" }
            }
            button { class: if done.is_empty() { "changes-row current" } else { "changes-row" },
                title: "{t(lang, \"pane.changes.go\")}",
                onclick: move |_| go(st, -count),
                Icon { name: "history" }
                span { class: "muted", "{t(lang, \"pane.changes.start\")}" }
            }
            for (i, step) in done.iter().enumerate() {
                {
                    let label = name(step);
                    let current = i + 1 == done.len();
                    let back = -(count - 1 - i as isize);
                    rsx! {
                        button { key: "d{i}", class: if current { "changes-row current" } else { "changes-row" },
                            title: "{t(lang, \"pane.changes.go\")}",
                            onclick: move |_| go(st, back),
                            Icon { name: "undo" }
                            span { class: "changes-name", "{label}" }
                        }
                    }
                }
            }
            for (j, step) in ahead.iter().enumerate() {
                {
                    let label = name(step);
                    let forward = j as isize + 1;
                    rsx! {
                        button { key: "r{j}", class: "changes-row ahead",
                            title: "{t(lang, \"pane.changes.go\")}",
                            onclick: move |_| go(st, forward),
                            Icon { name: "redo" }
                            span { class: "changes-name", "{label}" }
                        }
                    }
                }
            }
        }
    }
}
