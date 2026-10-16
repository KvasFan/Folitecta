pub mod canvas;
pub mod dialogs;
pub mod menubar;
pub mod palette;
pub mod panes;
pub mod ribbon;
pub mod ribbon_layout;
pub mod rightpane;
pub mod statusbar;
pub mod toasts;
pub mod welcome;

use dioxus::html::{InteractionElementOffset, InteractionLocation};
use dioxus::prelude::*;
use folitecta_core::Command;

use crate::commands;
use crate::i18n::{Lang, cmd_label};
use crate::state::AppState;

/// Width of menus, submenus and ribbon dropdowns (CSS `--menu-w`).
pub const MENU_W: f64 = 300.0;
/// Approximate height of one menu row.
const ROW_H: f64 = 30.0;

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

/// Recent-file entries rendered inside a submenu.
fn recent_entries(st: AppState) -> Element {
    let recent = st.ui.read().recent.clone();
    rsx! {
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

/// Position of a submenu opened from the row that received `evt`.
fn submenu_position(st: AppState, evt: &Event<MouseData>, rows: usize) -> (f64, f64) {
    let c = evt.client_coordinates();
    let o = evt.element_coordinates();
    let (row_left, row_top) = (c.x - o.x, c.y - o.y);
    let (vw, vh) = *st.viewport.peek();
    let row_w = MENU_W - 12.0;
    let mut left = row_left + row_w + 4.0;
    if left + MENU_W > vw - 4.0 {
        left = row_left - MENU_W - 4.0 + 12.0;
    }
    let height = (rows as f64 * ROW_H + 12.0).min(vh * 0.7);
    st.clamp_menu_pos(left, row_top - 6.0, MENU_W, height)
}

/// A menu entry (leaf or nested submenu). Submenus open on hover next to the
/// row with fixed positioning, so scrolling containers never clip them.
pub fn menu_entry(st: AppState, lang: Lang, cmd: &'static Command, depth: usize) -> Element {
    let reg = folitecta_core::registry();
    let label = cmd_label(lang, cmd);
    let id = cmd.id.clone();
    let is_recent = cmd.id == "recentFiles" || cmd.id == "recentsList";
    let children: Vec<&'static Command> = if is_recent {
        Vec::new()
    } else if commands::is_implemented(&cmd.id) {
        // An implemented command runs directly; its children stay reachable elsewhere.
        Vec::new()
    } else {
        reg.children(&cmd.id)
    };
    let has_sub = (is_recent || !children.is_empty()) && depth < 6;
    if has_sub {
        let rows = if is_recent {
            st.ui.read().recent.len().max(1)
        } else {
            children.len()
        };
        let open_pos = st
            .submenus
            .read()
            .get(depth)
            .filter(|(sid, _, _)| *sid == cmd.id)
            .map(|(_, x, y)| (*x, *y));
        let id_hover = id.clone();
        let id_click = id.clone();
        return rsx! {
            div { class: if open_pos.is_some() { "menu-item open" } else { "menu-item" },
                onmouseenter: move |evt| {
                    let pos = submenu_position(st, &evt, rows);
                    let mut stack = st.submenus_mut();
                    stack.truncate(depth);
                    stack.push((id_hover.clone(), pos.0, pos.1));
                },
                onclick: move |evt| {
                    evt.stop_propagation();
                    // Keyboard/touch users: clicking toggles the submenu.
                    let pos = submenu_position(st, &evt, rows);
                    let mut stack = st.submenus_mut();
                    let open = stack.get(depth).map(|(s, _, _)| *s == id_click).unwrap_or(false);
                    stack.truncate(depth);
                    if !open {
                        stack.push((id_click.clone(), pos.0, pos.1));
                    }
                },
                span { class: "check-mark" }
                span { class: "label", "{label}" }
                span { class: "chev", "›" }
                if let Some((left, top)) = open_pos {
                    div { class: "submenu", style: "left:{left}px;top:{top}px",
                        onclick: move |e| e.stop_propagation(),
                        if is_recent {
                            {recent_entries(st)}
                        }
                        for child in children.iter() {
                            {menu_entry(st, lang, child, depth + 1)}
                        }
                    }
                }
            }
        };
    }
    let shortcut = cmd.shortcut.clone().unwrap_or_default();
    let checked =
        commands::toggle_state(st, &cmd.id).or_else(|| ribbon::is_active(st, &cmd.id).then_some(true));
    rsx! {
        div { class: "menu-item",
            onmouseenter: move |_| {
                if st.submenus.peek().len() > depth {
                    st.submenus_mut().truncate(depth);
                }
            },
            onclick: move |e| { e.stop_propagation(); commands::dispatch(st, &id); },
            span { class: "check-mark", if checked == Some(true) { "✓" } else { "" } }
            span { class: "label", "{label}" }
            if !shortcut.is_empty() {
                span { class: "shortcut", "{shortcut}" }
            }
            {status_badge(&cmd.id)}
        }
    }
}
