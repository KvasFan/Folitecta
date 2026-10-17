//! Document recovery. While a document has changes that are not saved, a
//! copy of it is kept in the recovery folder (checked every minute and after
//! saving or closing), with a note of where it came from. The copy goes away
//! when the document is saved or closed, and every copy of a window when it
//! is left with Exit. Copies left by a window that ended otherwise (a crash,
//! the window closed with changes) are offered at the next start.
//!
//! Every window is a session: its process id and start time (a process id
//! used again later is another session), and a lock file it holds while it
//! runs, so the copies of a window that ended are known at once. Copies are
//! written to a temporary file and renamed, one checkpoint at a time, on a
//! thread of their own. Documents opened from encrypted files are not copied
//! (the copy would be readable without the password); the copy of a signed
//! document is an incremental update, so its signatures stay valid.

use std::collections::HashMap;
use std::fs::{File, TryLockError};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, Mutex, OnceLock, Weak};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

use crate::state::{AppState, Dialog, ToastKind};

/// How often the open documents are checked.
pub const EVERY: Duration = Duration::from_secs(60);

/// The note kept with a copy.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    /// The window the copy belongs to.
    pub session: String,
    pub tab: u64,
    pub title: String,
    /// Where the document was opened from (also for a document recovered
    /// before and not saved since).
    pub path: Option<PathBuf>,
    pub pages: usize,
    /// When the copy was written (seconds since 1970).
    pub saved: u64,
}

impl Entry {
    fn stem(&self) -> String {
        stem(&self.session, self.tab)
    }
}

/// Name and original place of a recovered document, by tab.
type Origins = HashMap<u64, (String, Option<PathBuf>)>;

fn stem(session: &str, tab: u64) -> String {
    format!("{session}-{tab}")
}

/// This window's session.
static SESSION: LazyLock<String> = LazyLock::new(|| {
    let started = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{}-{started}", std::process::id())
});
/// The lock file this session holds while it runs.
static LOCK: OnceLock<Option<File>> = OnceLock::new();
/// One checkpoint at a time.
static WRITING: Mutex<()> = Mutex::new(());
/// The state each tab's copy was written from, to write only what changed.
static WRITTEN: LazyLock<Mutex<HashMap<u64, Weak<Vec<u8>>>>> = LazyLock::new(Default::default);
/// Name and place of the documents recovered in this window, kept in their
/// notes until they are saved.
static ORIGINS: LazyLock<Mutex<Origins>> = LazyLock::new(Default::default);
/// The copies left behind were offered in this window.
static OFFERED: AtomicBool = AtomicBool::new(false);

/// This window's session name.
pub fn session() -> &'static str {
    SESSION.as_str()
}

/// The recovery folder: on Windows in the local application data (not the
/// roaming profile, which may travel to other computers).
pub fn folder() -> Option<PathBuf> {
    if std::env::var_os("FOLITECTA_CONFIG_DIR").is_none() {
        #[cfg(target_os = "windows")]
        if let Some(local) = std::env::var_os("LOCALAPPDATA") {
            return Some(PathBuf::from(local).join("Folitecta").join("recovery"));
        }
    }
    crate::state::app_data_dir().map(|d| d.join("recovery"))
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Take this session's lock (once).
fn hold_lock(dir: &Path) {
    LOCK.get_or_init(|| {
        std::fs::create_dir_all(dir).ok()?;
        let file = File::create(dir.join(format!("{}.lock", session()))).ok()?;
        file.try_lock().ok()?;
        Some(file)
    });
}

/// The window of `session` still runs: it holds its lock. When that cannot
/// be told, the window is taken as running (its copies are left alone).
fn running(dir: &Path, session: &str) -> bool {
    if session == self::session() {
        return true;
    }
    let Ok(file) = File::open(dir.join(format!("{session}.lock"))) else {
        return false;
    };
    match file.try_lock() {
        Ok(()) => {
            let _ = file.unlock();
            false
        }
        Err(TryLockError::WouldBlock) => true,
        Err(TryLockError::Error(_)) => true,
    }
}

fn remove(dir: &Path, stem: &str) {
    for ext in ["pdf", "json", "pdf.tmp", "json.tmp"] {
        let _ = std::fs::remove_file(dir.join(format!("{stem}.{ext}")));
    }
}

/// Write `data` to `path` through a temporary file, so a write that fails
/// or stops halfway leaves the previous file whole.
fn write_whole(path: &Path, data: &[u8]) -> std::io::Result<()> {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    std::fs::write(&tmp, data)?;
    std::fs::rename(&tmp, path)
}

fn write_note(dir: &Path, entry: &Entry) -> std::io::Result<()> {
    let json = serde_json::to_vec_pretty(entry).map_err(std::io::Error::other)?;
    write_whole(&dir.join(format!("{}.json", entry.stem())), &json)
}

/// Every note in the recovery folder, newest first.
fn entries(dir: &Path) -> Vec<Entry> {
    let Ok(read) = std::fs::read_dir(dir) else {
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

/// Copies left by windows that ended, newest first.
pub fn recoverable() -> Vec<Entry> {
    let Some(dir) = folder() else { return Vec::new() };
    entries(&dir)
        .into_iter()
        .filter(|e| !running(&dir, &e.session))
        .filter(|e| dir.join(format!("{}.pdf", e.stem())).exists())
        .collect()
}

/// A document as the recovery needs it.
struct Open {
    tab: u64,
    /// It has changes and may be copied.
    keep: bool,
    title: String,
    path: Option<PathBuf>,
    pages: usize,
    /// The current state (its address tells whether it changed).
    state: Arc<Vec<u8>>,
    /// The bytes to write, when the state changed since the last copy.
    copy: Option<Arc<Vec<u8>>>,
}

fn changed(written: &HashMap<u64, Weak<Vec<u8>>>, tab: u64, state: &Arc<Vec<u8>>) -> bool {
    written
        .get(&tab)
        .is_none_or(|w| !std::ptr::eq(w.as_ptr(), Arc::as_ptr(state)))
}

/// Bring the copies of this window in step with the open documents: write
/// the documents with changes (when they changed since), forget the others.
pub fn checkpoint(st: AppState) {
    let Some(dir) = folder() else { return };
    let open: Vec<Open> = {
        let written = lock(&WRITTEN);
        let origins = lock(&ORIGINS);
        st.docs
            .read()
            .iter()
            .map(|t| {
                let keep = t.doc.is_dirty() && !t.doc.from_encrypted_file();
                let state = t.doc.bytes();
                let copy = (keep && changed(&written, t.id, &state)).then(|| t.doc.copy_bytes());
                let origin = origins.get(&t.id);
                Open {
                    tab: t.id,
                    keep,
                    title: t.title(),
                    path: t
                        .doc
                        .path()
                        .map(Path::to_path_buf)
                        .or_else(|| origin.and_then(|o| o.1.clone())),
                    pages: t.doc.page_count(),
                    state,
                    copy,
                }
            })
            .collect()
    };
    // Away from the interface, and not tied to the component that asked.
    std::thread::spawn(move || write_copies(&dir, open));
}

fn write_copies(dir: &Path, open: Vec<Open>) {
    let _one = lock(&WRITING);
    let me = session();
    if open.iter().any(|d| d.keep) {
        hold_lock(dir);
    }
    let notes = entries(dir);
    for d in &open {
        let s = stem(me, d.tab);
        if !d.keep {
            remove(dir, &s);
            lock(&WRITTEN).remove(&d.tab);
            continue;
        }
        let pdf = dir.join(format!("{s}.pdf"));
        let note = notes.iter().find(|e| e.session == me && e.tab == d.tab);
        let fresh = !changed(&lock(&WRITTEN), d.tab, &d.state) && note.is_some() && pdf.exists();
        if fresh {
            continue;
        }
        let bytes = d.copy.clone().unwrap_or_else(|| d.state.clone());
        if write_whole(&pdf, &bytes).is_err() {
            continue;
        }
        let entry = Entry {
            session: me.to_string(),
            tab: d.tab,
            title: d.title.clone(),
            path: d.path.clone(),
            pages: d.pages,
            saved: now(),
        };
        if write_note(dir, &entry).is_ok() {
            lock(&WRITTEN).insert(d.tab, Arc::downgrade(&d.state));
        }
    }
    // Documents of this window closed since.
    for e in notes.iter().filter(|e| e.session == me) {
        if !open.iter().any(|d| d.tab == e.tab) {
            remove(dir, &e.stem());
            lock(&WRITTEN).remove(&e.tab);
        }
    }
    sweep(dir, &notes);
}

/// Remove what windows that ended left without a note: half-written files
/// and lock files.
fn sweep(dir: &Path, notes: &[Entry]) {
    let Ok(read) = std::fs::read_dir(dir) else { return };
    for f in read.flatten() {
        let name = f.file_name().to_string_lossy().to_string();
        let Some((owner, kind)) = owner_of(&name) else {
            continue;
        };
        if owner == session() || running(dir, &owner) {
            continue;
        }
        let noted = notes.iter().any(|e| e.session == owner);
        let stray = match kind {
            "lock" => !noted,
            "tmp" => true,
            // A copy whose note was never written.
            _ => !notes.iter().any(|e| name == format!("{}.pdf", e.stem())),
        };
        if stray {
            let _ = std::fs::remove_file(f.path());
        }
    }
}

/// The session a file of the recovery folder belongs to, and its kind
/// ("lock", "tmp" or "pdf"); notes are not swept.
fn owner_of(name: &str) -> Option<(String, &'static str)> {
    if let Some(s) = name.strip_suffix(".lock") {
        return Some((s.to_string(), "lock"));
    }
    let (base, kind) = if let Some(b) = name
        .strip_suffix(".pdf.tmp")
        .or_else(|| name.strip_suffix(".json.tmp"))
    {
        (b, "tmp")
    } else {
        (name.strip_suffix(".pdf")?, "pdf")
    };
    // `{pid}-{start}-{tab}`: the session is all but the last part.
    let (session, _tab) = base.rsplit_once('-')?;
    Some((session.to_string(), kind))
}

/// Forget the copies of this window (leaving with Exit).
pub fn forget_session() {
    let Some(dir) = folder() else { return };
    let _one = lock(&WRITING);
    let me = session();
    for e in entries(&dir).iter().filter(|e| e.session == me) {
        remove(&dir, &e.stem());
    }
    lock(&WRITTEN).clear();
    let _ = std::fs::remove_file(dir.join(format!("{me}.lock")));
}

/// Forget a copy left behind (deleted from the recovery dialog).
pub fn discard(entry: &Entry) {
    if let Some(dir) = folder() {
        let _one = lock(&WRITING);
        remove(&dir, &entry.stem());
    }
}

/// Open a copy left behind as a new document with changes to save. The copy
/// becomes this window's copy of the new document before its old note goes,
/// so it is never without one. Returns whether it opened.
pub fn recover(st: AppState, entry: &Entry) -> bool {
    let Some(dir) = folder() else { return false };
    let old_pdf = dir.join(format!("{}.pdf", entry.stem()));
    let opened = std::fs::read(&old_pdf)
        .map_err(|e| e.to_string())
        .and_then(|bytes| folitecta_core::Document::from_bytes(bytes, None, None).map_err(|e| e.to_string()));
    let mut doc = match opened {
        Ok(doc) => doc,
        Err(e) => {
            st.error(e);
            return false;
        }
    };
    doc.mark_dirty();
    doc.set_name_hint(&entry.title);
    crate::files::open_document_tab(st, doc);
    let Some((tab, state, pages)) = st.read_active(|t| (t.id, t.doc.bytes(), t.doc.page_count())) else {
        return false;
    };
    lock(&ORIGINS).insert(tab, (entry.title.clone(), entry.path.clone()));
    {
        let _one = lock(&WRITING);
        hold_lock(&dir);
        let adopted = Entry {
            session: session().to_string(),
            tab,
            title: entry.title.clone(),
            path: entry.path.clone(),
            pages,
            saved: entry.saved,
        };
        let new_pdf = dir.join(format!("{}.pdf", adopted.stem()));
        if std::fs::rename(&old_pdf, &new_pdf).is_ok() && write_note(&dir, &adopted).is_ok() {
            lock(&WRITTEN).insert(tab, Arc::downgrade(&state));
            let _ = std::fs::remove_file(dir.join(format!("{}.json", entry.stem())));
        }
    }
    st.toast(
        ToastKind::Ok,
        crate::i18n::tf(st.lang(), "toast.recovered", &[("name", &entry.title)]),
        "",
    );
    true
}

/// Offer the copies left behind, once per window.
fn offer(st: AppState) {
    if OFFERED.load(Ordering::Relaxed) || !matches!(*st.dialog.peek(), Dialog::None) {
        return;
    }
    if !recoverable().is_empty() {
        OFFERED.store(true, Ordering::Relaxed);
        st.set_dialog(Dialog::Recovery);
    }
}

/// Start the recovery of this window: its lock, the offer of the copies left
/// behind, and a checkpoint every minute.
pub fn start(st: AppState) {
    if let Some(dir) = folder() {
        hold_lock(&dir);
    }
    dioxus::core::spawn_forever(async move {
        tokio::time::sleep(Duration::from_secs(2)).await;
        offer(st);
        loop {
            tokio::time::sleep(EVERY).await;
            checkpoint(st);
            offer(st);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notes_round_trip_and_name_their_copy() {
        let e = Entry {
            session: "42-1000".into(),
            tab: 7,
            title: "Report.pdf".into(),
            path: Some(PathBuf::from("C:/docs/Report.pdf")),
            pages: 3,
            saved: 10,
        };
        let json = serde_json::to_string(&e).unwrap();
        assert_eq!(serde_json::from_str::<Entry>(&json).unwrap(), e);
        assert_eq!(e.stem(), "42-1000-7");
        assert_eq!(owner_of("42-1000-7.pdf"), Some(("42-1000".into(), "pdf")));
        assert_eq!(owner_of("42-1000-7.pdf.tmp"), Some(("42-1000".into(), "tmp")));
        assert_eq!(owner_of("42-1000.lock"), Some(("42-1000".into(), "lock")));
        assert_eq!(owner_of("42-1000-7.json"), None);
    }

    #[test]
    fn sessions_are_running_while_they_hold_their_lock() {
        let dir = std::env::temp_dir().join(format!("folitecta-recovery-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let other = "1-2";
        let held = File::create(dir.join(format!("{other}.lock"))).unwrap();
        held.lock().unwrap();
        // Another handle (as another window would open it) finds it locked.
        assert!(running(&dir, other));
        held.unlock().unwrap();
        drop(held);
        assert!(!running(&dir, other));
        assert!(!running(&dir, "no-such-session"));
        assert!(running(&dir, session()));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
