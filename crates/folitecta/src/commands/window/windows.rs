//! The other Folitecta windows: each window keeps a note of itself (its
//! documents) next to its lock in the recovery folder and reads its inbox
//! there every second. Merge All Tabs into This Window asks the others for
//! their documents; Move Tabs sends the active document to another window
//! (or a new one); Distribute Tabs gives each document but the active one a
//! window of its own. Only documents saved without changes travel: their
//! files open there and their tabs close here; the others stay, and the
//! window says so.

use std::path::{Path, PathBuf};

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

use crate::commands::{handlers, no_doc};
use crate::i18n::{t, tf};
use crate::state::{AfterUnsaved, AppState, Dialog, ToastKind};

/// What a window says of itself.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct WindowNote {
    pub session: String,
    /// The documents of its tabs.
    pub titles: Vec<String>,
}

/// A message to a window.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Message {
    /// Open these files.
    Open(Vec<PathBuf>),
    /// Send your saved documents to this window.
    Gather(String),
}

fn dir() -> Option<PathBuf> {
    crate::recovery::folder()
}

/// The other windows running, with what they say of themselves.
pub fn others() -> Vec<WindowNote> {
    let Some(dir) = dir() else { return Vec::new() };
    let me = crate::recovery::session();
    let Ok(read) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut out: Vec<WindowNote> = read
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            name.strip_suffix(".lock").map(str::to_string)
        })
        .filter(|s| s != me && crate::recovery::running(&dir, s))
        .map(|s| {
            let note: WindowNote = std::fs::read(dir.join(format!("{s}.window")))
                .ok()
                .and_then(|b| serde_json::from_slice(&b).ok())
                .unwrap_or_default();
            WindowNote { session: s, ..note }
        })
        .collect();
    out.sort_by(|a, b| a.session.cmp(&b.session));
    out
}

/// Leave a message in the inbox of window `session` (a file of its own, so
/// two senders never write the same file).
pub fn send(session: &str, message: &Message) -> Result<(), String> {
    let dir = dir().ok_or("no folder for the windows")?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let path = dir.join(format!("{session}.inbox.{stamp}.{}", crate::recovery::session()));
    let tmp = path.with_extension("tmp");
    let body = serde_json::to_vec(message).map_err(|e| e.to_string())?;
    std::fs::write(&tmp, body).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &path).map_err(|e| e.to_string())
}

/// The messages left for this window, taken out of its inbox.
fn take_messages(dir: &Path) -> Vec<Message> {
    let prefix = format!("{}.inbox.", crate::recovery::session());
    let Ok(read) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = read
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .map(|n| n.to_string_lossy().to_string())
                .is_some_and(|n| n.starts_with(&prefix) && !n.ends_with(".tmp"))
        })
        .collect();
    files.sort();
    files
        .into_iter()
        .filter_map(|p| {
            let m = std::fs::read(&p)
                .ok()
                .and_then(|b| serde_json::from_slice(&b).ok());
            let _ = std::fs::remove_file(&p);
            m
        })
        .collect()
}

/// The documents of this window that can travel (saved, no changes): tab
/// and file; the number of the others.
fn travelling(st: AppState, only: Option<u64>) -> (Vec<(u64, PathBuf)>, usize) {
    let mut kept = 0;
    let mut out = Vec::new();
    for t in st.docs.peek().iter().filter(|t| only.is_none_or(|id| t.id == id)) {
        match t.doc.path().filter(|_| !t.doc.is_dirty()) {
            Some(p) => out.push((t.id, p.to_path_buf())),
            None => kept += 1,
        }
    }
    (out, kept)
}

fn close_tabs(st: AppState, tabs: &[(u64, PathBuf)]) {
    for (id, _) in tabs {
        crate::files::close_tab(st, *id, true, AfterUnsaved::Close);
    }
}

fn say_kept(st: AppState, kept: usize) {
    if kept > 0 {
        st.toast(
            ToastKind::Info,
            tf(st.lang(), "toast.windows_kept", &[("count", &kept.to_string())]),
            "",
        );
    }
}

/// Act on the messages of the inbox; keep this window's note up to date.
fn tick(st: AppState) {
    let Some(dir) = dir() else { return };
    for m in take_messages(&dir) {
        match m {
            Message::Open(files) => {
                for f in files.into_iter().filter(|f| f.exists()) {
                    crate::files::open_path(st, f, None);
                }
                dioxus::desktop::window().set_focus();
            }
            Message::Gather(to) => {
                let (tabs, kept) = travelling(st, None);
                if !tabs.is_empty()
                    && send(&to, &Message::Open(tabs.iter().map(|(_, p)| p.clone()).collect())).is_ok()
                {
                    close_tabs(st, &tabs);
                }
                say_kept(st, kept);
            }
        }
    }
    let note = WindowNote {
        session: crate::recovery::session().to_string(),
        titles: st.docs.peek().iter().map(|t| t.title()).collect(),
    };
    let path = dir.join(format!("{}.window", note.session));
    let body = serde_json::to_vec(&note).unwrap_or_default();
    if std::fs::read(&path).ok().as_deref() != Some(body.as_slice()) {
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::fs::write(&path, body);
    }
}

/// Read the inbox every second (started with the window).
pub fn start(st: AppState) {
    dioxus::core::spawn_forever(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            tick(st);
        }
    });
}

/// The inbox read now (the self-test does not wait for the second).
pub fn check_now(st: AppState) {
    tick(st);
}

fn merge_all(st: AppState) {
    let others = others();
    if others.is_empty() {
        st.toast(ToastKind::Info, t(st.lang(), "toast.windows_none"), "");
        return;
    }
    let me = crate::recovery::session().to_string();
    let asked = others
        .iter()
        .filter(|w| send(&w.session, &Message::Gather(me.clone())).is_ok())
        .count();
    st.toast(
        ToastKind::Ok,
        tf(st.lang(), "toast.windows_asked", &[("count", &asked.to_string())]),
        "",
    );
}

/// Each document but the active one to a window of its own.
fn distribute(st: AppState) {
    if no_doc(st) {
        return;
    }
    let active = st.read_active(|t| t.id).unwrap_or(0);
    let (tabs, kept) = travelling(st, None);
    let tabs: Vec<(u64, PathBuf)> = tabs.into_iter().filter(|(id, _)| *id != active).collect();
    let mut moved = Vec::new();
    for (id, path) in tabs {
        match crate::commands::tab_groups::start_window(std::slice::from_ref(&path)) {
            Ok(()) => moved.push((id, path)),
            Err(e) => st.error(e),
        }
    }
    close_tabs(st, &moved);
    let kept = kept
        - usize::from(
            st.read_active(|t| t.doc.is_dirty() || t.doc.path().is_none())
                .unwrap_or(false),
        );
    say_kept(st, kept);
}

/// Move the active document to window `to` (None: a new window).
pub fn move_active(st: AppState, to: Option<String>) {
    if no_doc(st) {
        return;
    }
    let id = st.read_active(|t| t.id).unwrap_or(0);
    let (tabs, kept) = travelling(st, Some(id));
    if kept > 0 || tabs.is_empty() {
        st.toast(ToastKind::Warn, t(st.lang(), "toast.save_before_window"), "");
        return;
    }
    let files: Vec<PathBuf> = tabs.iter().map(|(_, p)| p.clone()).collect();
    let sent = match &to {
        Some(s) => send(s, &Message::Open(files)),
        None => crate::commands::tab_groups::start_window(&files),
    };
    match sent {
        Ok(()) => close_tabs(st, &tabs),
        Err(e) => st.error(e),
    }
}

handlers! {
    "window.allDocsToActiveWindow" => merge_all,
    "window.distributeTabs" => distribute,
    "window.moveTabsTo" => |st| { if !no_doc(st) { st.set_dialog(Dialog::MoveTabs) } },
}
