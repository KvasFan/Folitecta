pub mod canvas;
pub mod chrome;
pub mod dialogs;
pub mod features;
pub mod panes;

use std::sync::atomic::{AtomicU64, Ordering};

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
/// How long an open submenu waits before another row of its menu replaces or
/// closes it: the pointer crosses those rows on its way into the submenu.
const SUBMENU_DELAY_MS: u64 = 300;

/// The latest change of the open submenus asked for by the pointer; a change
/// waiting for [`SUBMENU_DELAY_MS`] happens only if it is still the latest.
static SUBMENU_TICKET: AtomicU64 = AtomicU64::new(0);

/// Forget the changes of the submenus waiting to happen (the pointer reached
/// the open submenu, or the menus closed).
pub fn keep_submenus() {
    SUBMENU_TICKET.fetch_add(1, Ordering::SeqCst);
}

/// Close the submenus from `depth` on, then open `open` there: at once when
/// no submenu is open at that depth, after a short delay when one is, unless
/// the pointer reaches it first.
fn switch_submenu(st: AppState, depth: usize, open: Option<(String, f64, f64)>) {
    let ticket = SUBMENU_TICKET.fetch_add(1, Ordering::SeqCst) + 1;
    let apply = move |open: Option<(String, f64, f64)>| {
        let mut stack = st.submenus_mut();
        stack.truncate(depth);
        stack.extend(open);
    };
    if st.submenus.peek().len() <= depth {
        apply(open);
        return;
    }
    dioxus::core::spawn_forever(async move {
        tokio::time::sleep(std::time::Duration::from_millis(SUBMENU_DELAY_MS)).await;
        if SUBMENU_TICKET.load(Ordering::SeqCst) == ticket {
            apply(open);
        }
    });
}

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

/// Comment filter submenus whose entries come from the document.
fn filter_kind(id: &str) -> Option<&'static str> {
    match id {
        "comments.show.byAuthor" => Some("author"),
        "comments.show.byType" => Some("type"),
        "comments.show.checked" => Some("checked"),
        _ => None,
    }
}

/// Entries of a comment filter submenu: value, label, shown.
fn filter_items(st: AppState, lang: Lang, kind: &str) -> Vec<(String, String, bool)> {
    let Some((infos, filter)) = st.read_active(|t| (t.doc.comment_infos(), t.comment_filter.clone())) else {
        return Vec::new();
    };
    let top: Vec<_> = infos
        .into_iter()
        .filter(|c| c.reply_to.is_none() && !c.is_state)
        .collect();
    match kind {
        "author" => {
            let mut v: Vec<String> = top.iter().map(|c| c.author.clone()).collect();
            v.sort();
            v.dedup();
            v.into_iter()
                .map(|a| {
                    let on = !filter.hidden_authors.contains(&a);
                    let label = if a.is_empty() {
                        "—".to_string()
                    } else {
                        a.clone()
                    };
                    (a, label, on)
                })
                .collect()
        }
        "type" => {
            let mut v: Vec<String> = top.iter().map(|c| c.subtype.clone()).collect();
            v.sort();
            v.dedup();
            v.into_iter()
                .map(|ty| {
                    let on = !filter.hidden_types.contains(&ty);
                    let label = crate::i18n::annot_name(lang, &ty);
                    (ty, label, on)
                })
                .collect()
        }
        "checked" => vec![
            (
                "yes".into(),
                crate::i18n::t(lang, "review.marked"),
                filter.checked == Some(true),
            ),
            (
                "no".into(),
                crate::i18n::t(lang, "review.unmarked"),
                filter.checked == Some(false),
            ),
        ],
        _ => Vec::new(),
    }
}

fn filter_entries(st: AppState, lang: Lang, kind: &'static str) -> Element {
    let items = filter_items(st, lang, kind);
    rsx! {
        for (value, label, on) in items {
            {
                let v = value.clone();
                rsx! {
                    div { class: "menu-item", key: "{kind}-{value}",
                        onclick: move |e| {
                            e.stop_propagation();
                            st.close_menus();
                            let v = v.clone();
                            commands::review::update_filter(st, move |f| match kind {
                                "author" => {
                                    if !f.hidden_authors.remove(&v) {
                                        f.hidden_authors.insert(v);
                                    }
                                }
                                "type" => {
                                    if !f.hidden_types.remove(&v) {
                                        f.hidden_types.insert(v);
                                    }
                                }
                                _ => {
                                    let want = v == "yes";
                                    f.checked = if f.checked == Some(want) { None } else { Some(want) };
                                }
                            });
                        },
                        span { class: "check-mark", if on { "✓" } else { "" } }
                        span { class: "label", "{label}" }
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
/// row with fixed positioning, so scrolling containers never clip them; an
/// open submenu stays a moment when the pointer crosses other rows towards it.
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
    let filter_kind = filter_kind(&cmd.id);
    let has_sub = (is_recent || !children.is_empty()) && depth < 6;
    if has_sub {
        let rows = if is_recent {
            st.ui.read().recent.len().max(1)
        } else {
            children.len() + filter_kind.map(|k| filter_items(st, lang, k).len()).unwrap_or(0)
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
                    if st.submenus.peek().get(depth).is_some_and(|(s, _, _)| *s == id_hover) {
                        // Back on the row of the open submenu: it stays.
                        keep_submenus();
                        return;
                    }
                    let pos = submenu_position(st, &evt, rows);
                    switch_submenu(st, depth, Some((id_hover.clone(), pos.0, pos.1)));
                },
                onclick: move |evt| {
                    evt.stop_propagation();
                    // Keyboard and touch users: a click opens the submenu (the row's
                    // children ignore the pointer, so the position is the row's).
                    keep_submenus();
                    if st.submenus.peek().get(depth).is_some_and(|(s, _, _)| *s == id_click) {
                        return;
                    }
                    let pos = submenu_position(st, &evt, rows);
                    let mut stack = st.submenus_mut();
                    stack.truncate(depth);
                    stack.push((id_click.clone(), pos.0, pos.1));
                },
                span { class: "check-mark" }
                span { class: "label", "{label}" }
                span { class: "chev", "›" }
                if let Some((left, top)) = open_pos {
                    div { class: "submenu", style: "left:{left}px;top:{top}px",
                        onmouseenter: move |_| keep_submenus(),
                        onclick: move |e| e.stop_propagation(),
                        if is_recent {
                            {recent_entries(st)}
                        }
                        for child in children.iter() {
                            {menu_entry(st, lang, child, depth + 1)}
                        }
                        if let Some(kind) = filter_kind {
                            {filter_entries(st, lang, kind)}
                        }
                    }
                }
            }
        };
    }
    let shortcut = cmd.shortcut.clone().unwrap_or_default();
    let checked = commands::toggle_state(st, &cmd.id)
        .or_else(|| chrome::ribbon::is_active(st, &cmd.id).then_some(true));
    rsx! {
        div { class: "menu-item",
            onmouseenter: move |_| {
                if st.submenus.peek().len() > depth {
                    switch_submenu(st, depth, None);
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
