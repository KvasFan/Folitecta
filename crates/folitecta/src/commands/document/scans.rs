//! Clean Up Scans: the pages that are a picture of paper looked at away from
//! the interface thread (with the progress shown and a way to stop), then
//! the blank ones deleted and the askew ones straightened in one step.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use dioxus::prelude::*;
use folitecta_core::{Document, ScanFix, ScanLook};

use crate::commands::{handlers, no_doc};
use crate::files;
use crate::i18n::tf;
use crate::state::{AppState, Dialog, ToastKind};

/// The pages looked at last.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ScanRun {
    pub running: bool,
    /// Pages looked at, and pages to look at.
    pub done: usize,
    pub total: usize,
    /// The document looked at: its tab and revision.
    pub tab: u64,
    pub revision: u64,
    /// The pages that are scans, in page order.
    pub looks: Vec<ScanLook>,
    /// Why the pages could not be looked at.
    pub failure: Option<String>,
}

pub static SCAN: GlobalSignal<ScanRun> = Signal::global(ScanRun::default);

static STOP: AtomicBool = AtomicBool::new(false);

/// Look at every page of the active document.
pub fn look(st: AppState) {
    if SCAN.peek().running {
        return;
    }
    let Some((tab, revision, bytes, total)) =
        st.read_active(|t| (t.id, t.doc.revision(), t.doc.bytes(), t.doc.page_count()))
    else {
        return;
    };
    STOP.store(false, Ordering::Relaxed);
    *SCAN.write() = ScanRun {
        running: true,
        total,
        tab,
        revision,
        ..ScanRun::default()
    };
    let found: Arc<Mutex<Vec<ScanLook>>> = Arc::new(Mutex::new(Vec::new()));
    let done = Arc::new(AtomicUsize::new(0));
    dioxus::core::spawn_forever(async move {
        let (found2, done2) = (found.clone(), done.clone());
        let work = tokio::task::spawn_blocking(move || -> Result<(), String> {
            let doc = Document::from_bytes(bytes.to_vec(), None, None).map_err(|e| e.to_string())?;
            for page in 0..doc.page_count() {
                if STOP.load(Ordering::Relaxed) {
                    break;
                }
                if let Some(l) = doc.look_at_scan(page).map_err(|e| e.to_string())? {
                    found2.lock().unwrap_or_else(|e| e.into_inner()).push(l);
                }
                done2.store(page + 1, Ordering::Relaxed);
            }
            Ok(())
        });
        // The progress, shown while the pages are looked at.
        while !work.is_finished() {
            tokio::time::sleep(std::time::Duration::from_millis(150)).await;
            let mut run = SCAN.write();
            run.done = done.load(Ordering::Relaxed);
            run.looks = found.lock().unwrap_or_else(|e| e.into_inner()).clone();
        }
        let result = work.await;
        let mut run = SCAN.write();
        run.running = false;
        run.done = done.load(Ordering::Relaxed);
        run.looks = found.lock().unwrap_or_else(|e| e.into_inner()).clone();
        run.failure = match result {
            Ok(Ok(())) => None,
            Ok(Err(e)) => Some(e),
            Err(e) => Some(e.to_string()),
        };
    });
}

/// Stop looking after the page being looked at.
pub fn stop() {
    STOP.store(true, Ordering::Relaxed);
}

/// The pages looked at are those of the active document as it is now.
pub fn current(st: AppState, run: &ScanRun) -> bool {
    st.read_active(|t| (t.id, t.doc.revision())) == Some((run.tab, run.revision))
}

/// Make the changes chosen, in one step.
pub fn apply(st: AppState, fixes: Vec<ScanFix>) {
    if !current(st, &SCAN.peek()) {
        return;
    }
    match st.with_active(|t| t.doc.clean_up_scans(&fixes)) {
        Some(Ok((straightened, deleted))) => {
            files::set_window_title(st);
            st.set_dialog(Dialog::None);
            st.toast(
                ToastKind::Ok,
                tf(
                    st.lang(),
                    "toast.scans_done",
                    &[
                        ("straightened", &straightened.to_string()),
                        ("deleted", &deleted.to_string()),
                    ],
                ),
                "",
            );
        }
        Some(Err(e)) => st.core_error(&e),
        None => {}
    }
}

fn open(st: AppState) {
    if no_doc(st) {
        return;
    }
    st.set_dialog(Dialog::CleanUpScans);
    look(st);
}

handlers! {
    "document.cleanUpScans" => open,
}
