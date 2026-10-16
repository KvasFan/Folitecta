pub mod canvas;
pub mod dialogs;
pub mod menubar;
pub mod palette;
pub mod panes;
pub mod ribbon;
pub mod rightpane;
pub mod statusbar;
pub mod toasts;
pub mod welcome;

use dioxus::prelude::*;
use folitecta_core::Command;

use crate::commands;
use crate::i18n::{Lang, cmd_label};
use crate::state::AppState;

/// Status badge for a command (empty when fully implemented).
pub fn status_badge(id: &str) -> Element {
    if commands::is_partial(id) {
        rsx! { span { class: "badge partial", "partial" } }
    } else if commands::is_implemented(id) {
        rsx! {}
    } else {
        rsx! { span { class: "badge planned", "planned" } }
    }
}

/// A menu entry (leaf or nested submenu) rendered recursively.
pub fn menu_entry(st: AppState, lang: Lang, cmd: &'static Command, depth: usize) -> Element {
    let reg = folitecta_core::registry();
    let label = cmd_label(lang, cmd);
    let id = cmd.id.clone();
    if cmd.id == "recentFiles" || cmd.id == "recentsList" {
        let recent = st.ui.read().recent.clone();
        return rsx! {
            div { class: "menu-item", onclick: move |e| e.stop_propagation(),
                span { class: "label", "{label}" }
                span { class: "chev", "›" }
                div { class: "submenu",
                    if recent.is_empty() {
                        div { class: "menu-item disabled", span { class: "label", "—" } }
                    }
                    for path in recent {
                        {
                            let p = path.clone();
                            let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                            let full = path.display().to_string();
                            rsx! {
                                div { class: "menu-item", key: "{full}", title: "{full}",
                                    onclick: move |e| { e.stop_propagation(); st.close_menus(); crate::files::open_path(st, p.clone(), None); },
                                    span { class: "label", "{name}" }
                                }
                            }
                        }
                    }
                }
            }
        };
    }
    let children = reg.children(&cmd.id);
    if !children.is_empty() && depth < 6 {
        return rsx! {
            div { class: "menu-item", onclick: move |e| e.stop_propagation(),
                span { class: "label", "{label}" }
                span { class: "chev", "›" }
                div { class: "submenu",
                    for child in children {
                        {menu_entry(st, lang, child, depth + 1)}
                    }
                }
            }
        };
    }
    let shortcut = cmd.shortcut.clone().unwrap_or_default();
    rsx! {
        div { class: "menu-item", onclick: move |e| { e.stop_propagation(); commands::dispatch(st, &id); },
            span { class: "label", "{label}" }
            if !shortcut.is_empty() {
                span { class: "shortcut", "{shortcut}" }
            }
            {status_badge(&cmd.id)}
        }
    }
}
