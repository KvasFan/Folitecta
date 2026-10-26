//! Sessions: saved sets of open documents, opened again whole or in part,
//! here or in a new window, and exchanged as files.

use std::path::PathBuf;

use dioxus::prelude::*;

use crate::commands::handlers;
use crate::i18n::{t, tf};
use crate::state::{AppState, Dialog, Pane, Session, SessionFile, ToastKind, now_secs};

/// The open documents that have a file, with their current page.
pub fn open_files(st: AppState) -> Vec<SessionFile> {
    st.docs
        .peek()
        .iter()
        .filter_map(|t| {
            t.doc.path().map(|p| SessionFile {
                path: p.to_path_buf(),
                page: t.page,
            })
        })
        .collect()
}

/// A name for a new session: the date and time.
pub fn default_name(st: AppState) -> String {
    let n = st.ui.peek().sessions.len() + 1;
    let date = folitecta_core::util::today_display();
    tf(
        st.lang(),
        "session.default_name",
        &[("n", &n.to_string()), ("date", &date)],
    )
}

/// Save the open documents as a session named `name`.
pub fn save_session(st: AppState, name: &str) {
    let files = open_files(st);
    let skipped = st.docs.peek().len() - files.len();
    if files.is_empty() {
        st.toast(ToastKind::Info, t(st.lang(), "toast.session_nothing"), "");
        return;
    }
    let name = if name.trim().is_empty() {
        default_name(st)
    } else {
        name.trim().to_string()
    };
    st.ui_mut().sessions.push(Session {
        name: name.clone(),
        created: now_secs(),
        opened: 0,
        files,
    });
    st.save_prefs();
    let mut msg = tf(st.lang(), "toast.session_saved", &[("name", &name)]);
    if skipped > 0 {
        msg.push_str(&format!(
            " {}",
            tf(
                st.lang(),
                "toast.session_skipped",
                &[("count", &skipped.to_string())]
            )
        ));
    }
    st.toast(ToastKind::Ok, msg, "");
}

/// The selected session, or none.
fn selected(st: AppState) -> Option<(usize, Option<usize>, Session)> {
    let (i, f) = (*st.session_sel.peek())?;
    let s = st.ui.peek().sessions.get(i).cloned()?;
    Some((i, f, s))
}

fn need_selected(st: AppState) -> Option<(usize, Option<usize>, Session)> {
    let r = selected(st);
    if r.is_none() {
        st.toast(ToastKind::Info, t(st.lang(), "toast.session_select"), "");
    }
    r
}

/// Open files of a session in this window.
fn open_here(st: AppState, files: Vec<SessionFile>) {
    let mut missing = 0;
    for f in files {
        if !f.path.exists() {
            missing += 1;
            continue;
        }
        crate::files::open_path(st, f.path.clone(), None);
        if f.page > 0
            && st
                .read_active(|t| t.doc.path() == Some(f.path.as_path()))
                .unwrap_or(false)
        {
            st.navigate(f.page, true);
        }
    }
    if missing > 0 {
        st.toast(
            ToastKind::Warn,
            tf(
                st.lang(),
                "toast.session_missing",
                &[("count", &missing.to_string())],
            ),
            "",
        );
    }
}

/// Files of the selection: the selected file, or every file of the session.
fn chosen(f: Option<usize>, s: &Session) -> Vec<SessionFile> {
    match f.and_then(|i| s.files.get(i)) {
        Some(file) => vec![file.clone()],
        None => s.files.clone(),
    }
}

fn mark_opened(st: AppState, i: usize) {
    if let Some(s) = st.ui_mut().sessions.get_mut(i) {
        s.opened = now_secs();
    }
    st.save_prefs();
}

/// Open the session instead of the open documents (changed ones stay).
fn open_whole(st: AppState) {
    let Some((i, _, s)) = need_selected(st) else {
        return;
    };
    let keep: Vec<PathBuf> = s.files.iter().map(|f| f.path.clone()).collect();
    st.docs_mut()
        .retain(|t| t.doc.is_dirty() || t.doc.path().is_some_and(|p| keep.iter().any(|k| k == p)));
    let n = st.docs.peek().len();
    st.set_active(n.saturating_sub(1));
    open_here(st, s.files);
    mark_opened(st, i);
    crate::files::set_window_title(st);
}

/// Open the session (or its selected file) next to the open documents.
fn add_here(st: AppState) {
    let Some((i, f, s)) = need_selected(st) else {
        return;
    };
    open_here(st, chosen(f, &s));
    mark_opened(st, i);
}

/// Open the session (or its selected file) in a new window.
fn new_window(st: AppState) {
    let Some((i, f, s)) = need_selected(st) else {
        return;
    };
    let paths: Vec<PathBuf> = chosen(f, &s)
        .into_iter()
        .map(|f| f.path)
        .filter(|p| p.exists())
        .collect();
    match crate::commands::tab_groups::start_window(&paths) {
        Ok(()) => mark_opened(st, i),
        Err(e) => st.error(e),
    }
}

fn remove(st: AppState) {
    let Some((i, f, _)) = need_selected(st) else {
        return;
    };
    {
        let mut ui = st.ui_mut();
        match f {
            Some(fi) => {
                if let Some(s) = ui.sessions.get_mut(i) {
                    if fi < s.files.len() {
                        s.files.remove(fi);
                    }
                }
            }
            None => {
                ui.sessions.remove(i);
            }
        }
    }
    *st.session_sel.write_unchecked() = None;
    st.save_prefs();
}

fn clear(st: AppState) {
    st.ui_mut().sessions.clear();
    *st.session_sel.write_unchecked() = None;
    st.save_prefs();
    st.toast(ToastKind::Ok, t(st.lang(), "toast.sessions_cleared"), "");
}

fn export(st: AppState, sessions: Vec<Session>, name: &str) {
    match serde_json::to_string_pretty(&sessions) {
        Ok(json) => crate::files::save_text_file(st, name, json),
        Err(e) => st.error(e),
    }
}

/// Read sessions from a file; `open` opens the first one right away.
fn import(st: AppState, open: bool) {
    spawn(async move {
        let Some(f) = rfd::AsyncFileDialog::new()
            .add_filter("JSON", &["json"])
            .pick_file()
            .await
        else {
            return;
        };
        let data = f.read().await;
        // A file holds one session or a list of them.
        let sessions: Result<Vec<Session>, _> = serde_json::from_slice::<Vec<Session>>(&data)
            .or_else(|_| serde_json::from_slice::<Session>(&data).map(|s| vec![s]));
        match sessions {
            Ok(list) if !list.is_empty() => {
                if open {
                    open_here(st, list[0].files.clone());
                }
                let n = list.len();
                st.ui_mut().sessions.extend(list);
                st.save_prefs();
                st.toast(
                    ToastKind::Ok,
                    tf(st.lang(), "toast.sessions_imported", &[("count", &n.to_string())]),
                    "",
                );
            }
            Ok(_) => st.toast(ToastKind::Info, t(st.lang(), "toast.session_nothing"), ""),
            Err(e) => st.error(e),
        }
    });
}

/// "created", "opened" or "alphabetical".
pub fn sort_mode(st: AppState) -> String {
    st.ui
        .peek()
        .value("sessions.sort")
        .unwrap_or("created")
        .to_string()
}

fn sort(st: AppState, mode: &str) {
    st.set_value("sessions.sort", mode);
    *st.session_sel.write_unchecked() = None;
    let mut ui = st.ui_mut();
    match mode {
        "alphabetical" => ui.sessions.sort_by_key(|s| s.name.to_lowercase()),
        "opened" => ui.sessions.sort_by_key(|s| std::cmp::Reverse(s.opened)),
        _ => ui.sessions.sort_by_key(|s| std::cmp::Reverse(s.created)),
    }
    drop(ui);
    st.save_prefs();
}

fn cycle_sort(st: AppState) {
    let modes = ["created", "opened", "alphabetical"];
    let cur = sort_mode(st);
    let i = modes.iter().position(|m| *m == cur).unwrap_or(0);
    sort(st, modes[(i + 1) % modes.len()]);
}

/// Check states of the session sort orders.
pub fn sessions_state(st: AppState, id: &str) -> Option<bool> {
    match id {
        "sessions.sort.alphabetical" => Some(sort_mode(st) == "alphabetical"),
        "sessions.sort.createTime" => Some(sort_mode(st) == "created"),
        "sessions.sort.openTime" => Some(sort_mode(st) == "opened"),
        _ => None,
    }
}

handlers! {
    "saveCurrentSession" => |st| st.set_dialog(Dialog::SessionName(None)),
    "saveCurrentSessionToFile" => |st| {
        let files = open_files(st);
        if files.is_empty() {
            st.toast(ToastKind::Info, t(st.lang(), "toast.session_nothing"), "");
            return;
        }
        let s = Session { name: default_name(st), created: now_secs(), opened: 0, files };
        export(st, vec![s], "session.json");
    },
    "openSessionFromFile" => |st| import(st, true),
    "manageSessions" => |st| crate::commands::show_pane(st, Pane::Sessions),
    "sessionsList" => |st| crate::commands::show_pane(st, Pane::Sessions),
    "sessions.open.wholeSession" => open_whole,
    "sessions.open.addWholeSession" => add_here,
    "sessions.open.addToActiveGroup" => add_here,
    "sessions.open.addToNewGroup" => add_here,
    "sessions.open.addToNewFrame" => new_window,
    "sessions.remove" => remove,
    "sessions.rename" => |st| { if let Some((i, _, _)) = need_selected(st) { st.set_dialog(Dialog::SessionName(Some(i))) } },
    "sessions.clear" => clear,
    "sessions.export" => |st| { let all = st.ui.peek().sessions.clone(); export(st, all, "sessions.json") },
    "sessions.import" => |st| import(st, false),
    "sessions.sort.alphabetical" => |st| sort(st, "alphabetical"),
    "sessions.sort.createTime" => |st| sort(st, "created"),
    "sessions.sort.openTime" => |st| sort(st, "opened"),
    "sessions.sortBy" => cycle_sort,
}
