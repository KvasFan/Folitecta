//! Document recovery. While a document has changes that are not saved, a
//! copy of it is kept in the recovery folder (checked every minute), with a
//! note of where it came from. The copy goes away when the document is saved
//! or closed, and every copy of a session when the app is left with Exit.
//! Copies left by a session that ended otherwise (a crash, the window closed
//! with changes) are offered again at the next start.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

use crate::state::{AppState, Dialog, ToastKind};

/// How often the open documents are checked.
pub const EVERY: Duration = Duration::from_secs(60);
/// A copy whose session has not touched it for this long (seconds) was left
/// behind: no running window keeps it up to date.
const STALE: u64 = 3 * 60;

/// The note kept with a copy.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    pub pid: u32,
    pub tab: u64,
    pub title: String,
    pub path: Option<PathBuf>,
    pub pages: usize,
    /// When the copy was written, and when its window was last running
    /// (seconds since 1970).
    pub saved: u64,
    pub alive: u64,
    pub revision: u64,
}

impl Entry {
    fn stem(&self) -> String {
        stem(self.pid, self.tab)
    }

    /// The copy of the document.
    pub fn pdf(&self) -> Option<PathBuf> {
        dir().map(|d| d.join(format!("{}.pdf", self.stem())))
    }
}

fn stem(pid: u32, tab: u64) -> String {
    format!("{pid}-{tab}")
}

fn dir() -> Option<PathBuf> {
    crate::state::app_data_dir().map(|d| d.join("recovery"))
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn remove(dir: &Path, stem: &str) {
    let _ = std::fs::remove_file(dir.join(format!("{stem}.pdf")));
    let _ = std::fs::remove_file(dir.join(format!("{stem}.json")));
}

/// Every copy in the recovery folder.
fn entries() -> Vec<Entry> {
    let Some(dir) = dir() else { return Vec::new() };
    let Ok(read) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut out: Vec<Entry> = read
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
        .filter_map(|e| serde_json::from_slice(&std::fs::read(e.path()).ok()?).ok())
        .collect();
    out.sort_by_key(|e: &Entry| std::cmp::Reverse(e.saved));
    out
}

/// Copies left by sessions that are no longer running, newest first.
pub fn recoverable() -> Vec<Entry> {
    let (me, now) = (std::process::id(), now());
    entries()
        .into_iter()
        .filter(|e| e.pid != me && now.saturating_sub(e.alive) > STALE)
        .filter(|e| e.pdf().is_some_and(|p| p.exists()))
        .collect()
}

/// A document as the recovery needs it.
struct Open {
    tab: u64,
    dirty: bool,
    revision: u64,
    title: String,
    path: Option<PathBuf>,
    pages: usize,
    bytes: Arc<Vec<u8>>,
}

/// Bring the copies of this session in step with the open documents: write
/// the documents with changes (when they changed since), forget the others.
pub fn checkpoint(st: AppState) {
    if dir().is_none() {
        return;
    }
    let open: Vec<Open> = st
        .docs
        .read()
        .iter()
        .map(|t| Open {
            tab: t.id,
            dirty: t.doc.is_dirty(),
            revision: t.doc.revision(),
            title: t.title(),
            path: t.doc.path().map(Path::to_path_buf),
            pages: t.doc.page_count(),
            bytes: t.doc.bytes(),
        })
        .collect();
    // The files are written away from the interface.
    spawn(async move {
        let _ = tokio::task::spawn_blocking(move || write_copies(open)).await;
    });
}

fn write_copies(open: Vec<Open>) {
    let Some(dir) = dir() else { return };
    let (pid, now) = (std::process::id(), now());
    if open.iter().any(|d| d.dirty) {
        let _ = std::fs::create_dir_all(&dir);
    }
    let known = entries();
    for d in &open {
        let s = stem(pid, d.tab);
        if !d.dirty {
            remove(&dir, &s);
            continue;
        }
        let old = known.iter().find(|e| e.pid == pid && e.tab == d.tab);
        let changed = old.is_none_or(|e| e.revision != d.revision);
        if changed && std::fs::write(dir.join(format!("{s}.pdf")), d.bytes.as_slice()).is_err() {
            continue;
        }
        let entry = Entry {
            pid,
            tab: d.tab,
            title: d.title.clone(),
            path: d.path.clone(),
            pages: d.pages,
            saved: if changed {
                now
            } else {
                old.map(|e| e.saved).unwrap_or(now)
            },
            alive: now,
            revision: d.revision,
        };
        if let Ok(json) = serde_json::to_vec_pretty(&entry) {
            let _ = std::fs::write(dir.join(format!("{s}.json")), json);
        }
    }
    // Documents of this session closed since.
    for e in known.iter().filter(|e| e.pid == pid) {
        if !open.iter().any(|d| d.tab == e.tab) {
            remove(&dir, &e.stem());
        }
    }
}

/// Forget the copies of this session (leaving with Exit).
pub fn forget_session() {
    let Some(dir) = dir() else { return };
    let me = std::process::id();
    for e in entries().iter().filter(|e| e.pid == me) {
        remove(&dir, &e.stem());
    }
}

/// Forget a copy left behind (deleted from the recovery dialog).
pub fn discard(entry: &Entry) {
    if let Some(dir) = dir() {
        remove(&dir, &entry.stem());
    }
}

/// Open a copy left behind as a new document with changes to save; the copy
/// itself is forgotten (this session keeps its own).
pub fn recover(st: AppState, entry: &Entry) {
    let Some(pdf) = entry.pdf() else { return };
    let opened = std::fs::read(&pdf)
        .map_err(|e| e.to_string())
        .and_then(|bytes| folitecta_core::Document::from_bytes(bytes, None, None).map_err(|e| e.to_string()));
    match opened {
        Ok(mut doc) => {
            doc.mark_dirty();
            crate::files::open_document_tab(st, doc);
            discard(entry);
            st.toast(
                ToastKind::Ok,
                crate::i18n::tf(st.lang(), "toast.recovered", &[("name", &entry.title)]),
                "",
            );
        }
        Err(e) => st.error(e),
    }
}

/// Every minute, keep the copies in step (started once with the window), and
/// offer the copies left behind at the start.
pub fn start(st: AppState) {
    dioxus::core::spawn_forever(async move {
        tokio::time::sleep(Duration::from_secs(2)).await;
        if !recoverable().is_empty() && matches!(*st.dialog.peek(), Dialog::None) {
            st.set_dialog(Dialog::Recovery);
        }
        loop {
            tokio::time::sleep(EVERY).await;
            checkpoint(st);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notes_round_trip_and_name_their_copy() {
        let e = Entry {
            pid: 42,
            tab: 7,
            title: "Report.pdf".into(),
            path: Some(PathBuf::from("C:/docs/Report.pdf")),
            pages: 3,
            saved: 10,
            alive: 20,
            revision: 5,
        };
        let json = serde_json::to_string(&e).unwrap();
        assert_eq!(serde_json::from_str::<Entry>(&json).unwrap(), e);
        assert_eq!(e.stem(), "42-7");
    }
}
