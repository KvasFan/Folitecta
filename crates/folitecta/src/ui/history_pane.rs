//! History of opened documents and saved sessions.

use std::path::PathBuf;

use base64::Engine;
use dioxus::html::ModifiersInteraction;
use dioxus::prelude::*;
use folitecta_core::registry;

use crate::commands::history::{age_group, file_name, sorted_entries, thumb_path};
use crate::commands::{dispatch, option_on};
use crate::i18n::{Lang, cmd_label, t, tf};
use crate::icons::Icon;
use crate::state::{AppState, Dialog, HistoryEntry, now_secs};
use crate::ui::dialogs::Frame;

/// Date and time of seconds since 1970 (UTC), `YYYY-MM-DD HH:MM`.
pub fn when(secs: u64) -> String {
    folitecta_core::util::unix_display(secs)
}

fn thumb_url(path: &std::path::Path) -> Option<String> {
    let data = std::fs::read(thumb_path(path)?).ok()?;
    Some(format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(data)
    ))
}

fn group_label(lang: Lang, key: &str) -> String {
    t(lang, &format!("history.group.{key}"))
}

#[component]
pub fn HistoryPane() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let _ = st.ui.read();
    let entries = sorted_entries(st);
    let selected = (st.history_sel)();
    let collapsed = (st.history_collapsed)();
    let grouped = option_on(st, "historyList.group.show");
    let pinned_first = option_on(st, "historyList.options.showPinned");
    let show_path = option_on(st, "historyList.options.showPath");
    let captions = option_on(st, "historyList.options.showCaptions");
    let thumbs = option_on(st, "historyList.options.showThumbnail");
    let popup = option_on(st, "historyList.options.showPreviewPopUp");
    let now = now_secs();
    // Groups in order: pinned, then by age (or everything together).
    let mut groups: Vec<(String, Vec<HistoryEntry>)> = Vec::new();
    let mut push = |key: &str, e: HistoryEntry| match groups.iter_mut().find(|(k, _)| k == key) {
        Some((_, v)) => v.push(e),
        None => groups.push((key.to_string(), vec![e])),
    };
    for e in entries.iter().filter(|e| pinned_first && e.pinned) {
        push("pinned", e.clone());
    }
    for e in entries.iter().filter(|e| !(pinned_first && e.pinned)) {
        let key = if grouped {
            age_group(e.opened.max(e.closed), now)
        } else {
            "all"
        };
        push(key, e.clone());
    }
    let order = ["pinned", "today", "yesterday", "week", "older", "all"];
    groups.sort_by_key(|(k, _)| order.iter().position(|o| o == k).unwrap_or(9));
    let reg = registry();
    let label = |id: &str| {
        reg.get(id)
            .map(|c| cmd_label(lang, c))
            .unwrap_or_else(|| id.to_string())
    };
    let has_sel = !selected.is_empty();
    let tools: [(&str, &str, bool); 6] = [
        ("historyList.open", "open", has_sel),
        ("historyList.pin", "star", has_sel),
        ("historyList.folder", "folder", has_sel),
        ("historyList.copyPath", "copy", has_sel),
        ("historyList.remove", "delete", has_sel),
        ("historyList.sortBy", "list", !entries.is_empty()),
    ];
    let multi = groups.len() > 1;
    rsx! {
        div { class: "pane-toolbar icon-toolbar",
            for (id, icon, enabled) in tools {
                button { class: "ibtn", key: "{id}", title: "{label(id)}", disabled: !enabled, onclick: move |_| dispatch(st, id), Icon { name: icon } }
            }
        }
        if entries.is_empty() {
            div { class: "pane-empty", "{t(lang, \"history.none\")}" }
        }
        div { class: "history-list",
            for (key, items) in groups {
                {
                    let open = !collapsed.contains(&key);
                    let k = key.clone();
                    let n = items.len();
                    rsx! {
                        div { key: "g{key}",
                            if multi || key == "pinned" {
                                div { class: "result-group",
                                    onclick: move |_| { let mut c = st.history_collapsed.write_unchecked(); if !c.remove(&k) { c.insert(k.clone()); } },
                                    span { class: "twisty", if open { "▾" } else { "▸" } }
                                    span { class: "ellipsis", "{group_label(lang, &key)}" }
                                    span { class: "chip", "{n}" }
                                }
                            }
                            if open {
                                for e in items {
                                    {
                                        let path = e.path.clone();
                                        let is_sel = selected.contains(&path);
                                        let exists = path.exists();
                                        let name = if captions && !e.title.is_empty() { e.title.clone() } else { file_name(&path) };
                                        let dir = path.parent().map(|p| p.display().to_string()).unwrap_or_default();
                                        let thumb = if thumbs || popup { thumb_url(&path) } else { None };
                                        let mut class = String::from("history-item");
                                        if is_sel { class.push_str(" selected"); }
                                        if !exists { class.push_str(" missing"); }
                                        let meta = tf(lang, "history.meta", &[("when", &when(e.opened)), ("pages", &e.pages.to_string())]);
                                        let tip = format!("{}\n{}", path.display(), meta);
                                        let p_click = path.clone();
                                        let p_open = path.clone();
                                        rsx! {
                                            div { class: "{class}", key: "{path.display()}", title: "{tip}",
                                                onclick: move |ev| {
                                                    let add = ev.modifiers().ctrl() || ev.modifiers().meta();
                                                    let p = p_click.clone();
                                                    let mut sel = st.history_sel.write_unchecked();
                                                    if add {
                                                        if let Some(i) = sel.iter().position(|x| *x == p) { sel.remove(i); } else { sel.push(p); }
                                                    } else {
                                                        *sel = vec![p];
                                                    }
                                                },
                                                ondoubleclick: move |_| crate::commands::history::open_paths(st, vec![p_open.clone()]),
                                                if thumbs {
                                                    if let Some(u) = thumb.clone() {
                                                        img { class: "history-thumb", src: "{u}", alt: "" }
                                                    } else {
                                                        div { class: "history-thumb none", Icon { name: "page" } }
                                                    }
                                                }
                                                div { class: "history-text",
                                                    div { class: "history-name",
                                                        if e.pinned { Icon { name: "star" } }
                                                        span { class: "ellipsis", "{name}" }
                                                    }
                                                    if show_path { div { class: "history-path ellipsis", "{dir}" } }
                                                    div { class: "history-meta ellipsis", "{meta}" }
                                                }
                                                if popup {
                                                    if let Some(u) = thumb {
                                                        div { class: "history-pop", img { src: "{u}", alt: "" } }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
pub fn SessionsPane() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let sessions = st.ui.read().sessions.clone();
    let sel = (st.session_sel)();
    let mut collapsed: Signal<std::collections::HashSet<usize>> = use_signal(Default::default);
    let reg = registry();
    let label = |id: &str| {
        reg.get(id)
            .map(|c| cmd_label(lang, c))
            .unwrap_or_else(|| id.to_string())
    };
    let has_sel = sel.is_some();
    let can_save = st.docs.read().iter().any(|t| t.doc.path().is_some());
    let tools: [(&str, &str, bool); 7] = [
        ("saveCurrentSession", "save", can_save),
        ("sessions.open.wholeSession", "open", has_sel),
        ("sessions.open.addWholeSession", "insert", has_sel),
        ("sessions.open.addToNewFrame", "window", has_sel),
        ("sessions.rename", "pencil", has_sel),
        ("sessions.remove", "delete", has_sel),
        ("sessions.sortBy", "list", sessions.len() > 1),
    ];
    rsx! {
        div { class: "pane-toolbar icon-toolbar",
            for (id, icon, enabled) in tools {
                button { class: "ibtn", key: "{id}", title: "{label(id)}", disabled: !enabled, onclick: move |_| dispatch(st, id), Icon { name: icon } }
            }
        }
        if sessions.is_empty() {
            div { class: "pane-empty",
                "{t(lang, \"session.none\")}"
                if can_save {
                    div { class: "row", style: "justify-content:center;margin-top:8px",
                        button { class: "tbtn", onclick: move |_| dispatch(st, "saveCurrentSession"), "{label(\"saveCurrentSession\")}" }
                    }
                }
            }
        }
        div { class: "history-list",
            for (i, s) in sessions.into_iter().enumerate() {
                {
                    let open = !collapsed().contains(&i);
                    let is_sel = sel == Some((i, None));
                    let meta = tf(lang, "session.meta", &[("count", &s.files.len().to_string()), ("when", &when(s.created))]);
                    rsx! {
                        div { key: "s{i}",
                            div { class: if is_sel { "result-group selected" } else { "result-group" },
                                onclick: move |_| { *st.session_sel.write_unchecked() = Some((i, None)); },
                                ondoubleclick: move |_| dispatch(st, "sessions.open.addWholeSession"),
                                span { class: "twisty",
                                    onclick: move |ev| { ev.stop_propagation(); collapsed.with_mut(|c| { if !c.remove(&i) { c.insert(i); } }); },
                                    if open { "▾" } else { "▸" }
                                }
                                Icon { name: "window" }
                                span { class: "ellipsis", title: "{meta}", "{s.name}" }
                                span { class: "chip", "{s.files.len()}" }
                            }
                            if open {
                                for (j, f) in s.files.iter().cloned().enumerate() {
                                    {
                                        let fsel = sel == Some((i, Some(j)));
                                        let missing = !f.path.exists();
                                        let mut class = String::from("history-item session-file");
                                        if fsel { class.push_str(" selected"); }
                                        if missing { class.push_str(" missing"); }
                                        let path: PathBuf = f.path.clone();
                                        rsx! {
                                            div { class: "{class}", key: "f{j}", title: "{path.display()}",
                                                onclick: move |_| { *st.session_sel.write_unchecked() = Some((i, Some(j))); },
                                                ondoubleclick: move |_| { *st.session_sel.write_unchecked() = Some((i, Some(j))); dispatch(st, "sessions.open.addToActiveGroup"); },
                                                Icon { name: "page" }
                                                span { class: "ellipsis", "{file_name(&path)}" }
                                                if f.page > 0 { span { class: "page-ref", "{f.page + 1}" } }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Name of a new session, or of a saved one.
#[component]
pub fn SessionNameDialog(index: Option<usize>) -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let initial = match index {
        Some(i) => st
            .ui
            .peek()
            .sessions
            .get(i)
            .map(|s| s.name.clone())
            .unwrap_or_default(),
        None => crate::commands::sessions::default_name(st),
    };
    let mut name = use_signal(|| initial);
    let count = crate::commands::sessions::open_files(st).len();
    let title = if index.is_some() {
        t(lang, "session.rename")
    } else {
        t(lang, "session.save")
    };
    rsx! {
        Frame { title,
            on_ok: move |_| {
                match index {
                    Some(i) => {
                        let n = name();
                        if n.trim().is_empty() { return; }
                        if let Some(s) = st.ui_mut().sessions.get_mut(i) { s.name = n.trim().to_string(); }
                        st.save_prefs();
                    }
                    None => crate::commands::sessions::save_session(st, &name()),
                }
                st.set_dialog(Dialog::None);
            },
            div { class: "field",
                label { "{t(lang, \"layers.name\")}" }
                input { value: "{name}", autofocus: true, oninput: move |e| name.set(e.value()) }
            }
            if index.is_none() {
                div { class: "hint", "{tf(lang, \"session.save_hint\", &[(\"count\", &count.to_string())])}" }
            }
        }
    }
}
