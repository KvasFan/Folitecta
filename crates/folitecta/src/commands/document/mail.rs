//! E-mail: the dialog that prepares the files of a message (the saved file
//! or the current state of each document, chosen pages, comments kept,
//! flattened or removed, form data and comments as data files, one ZIP
//! archive), shows what each file holds, and saves the message as a draft or
//! hands it to the mail program. Files are prepared away from the interface
//! thread; nothing is ever sent by Folitecta.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use dioxus::prelude::*;

pub use super::mail_draft::*;
use crate::commands::{handlers, no_doc};
use crate::i18n::{Lang, t, tf};
use crate::mail::{self, HandOverError, Outgoing, Sent};
use crate::state::{AppState, Dialog, ToastKind};

/// The message the dialog prepares.
pub static MAIL: GlobalSignal<Option<MailDraft>> = Signal::global(|| None);

/// Rows prepared at the same time.
const AT_ONCE: usize = 2;

/// The preparations under way: message, row, revision and the flag that stops them.
type Running = (u64, usize, u64, Arc<AtomicBool>);
static RUNNING: Mutex<Vec<Running>> = Mutex::new(Vec::new());

/// Bumped by every change, so a pause in typing starts the work once.
static GENERATION: AtomicU64 = AtomicU64::new(0);

fn running() -> std::sync::MutexGuard<'static, Vec<Running>> {
    RUNNING.lock().unwrap_or_else(|e| e.into_inner())
}

/// Stop the preparations of message `id` (of row `row` only, when given).
fn stop(id: u64, row: Option<usize>) {
    for (d, r, _, flag) in running().iter() {
        if *d == id && row.is_none_or(|x| x == *r) {
            flag.store(true, Ordering::Relaxed);
        }
    }
}

/// The id of the message being prepared.
fn current_id() -> Option<u64> {
    MAIL.peek().as_ref().map(|d| d.id)
}

/// Forget the message (its prepared files are released) and stop its work.
pub fn forget() {
    if let Some(id) = current_id() {
        stop(id, None);
        *MAIL.write() = None;
    }
}

/// Change the message `id` with `f` (nothing when another one replaced it).
fn with_draft(id: u64, f: impl FnOnce(&mut MailDraft)) -> bool {
    let mut w = MAIL.write();
    match w.as_mut() {
        Some(d) if d.id == id => {
            f(d);
            true
        }
        _ => false,
    }
}

fn set_phase(id: u64, phase: Phase) {
    with_draft(id, |d| d.phase = phase);
}

/// Open the dialog for the active document (`all`: every open one), or for
/// the form data of the active one.
pub fn start(st: AppState, how: Start) {
    if no_doc(st) {
        return;
    }
    let lang = st.lang();
    let active = st.active_index();
    let all = matches!(how, Start::Documents { all: true, .. });
    let docs: Vec<DocFacts> = {
        let docs = st.docs.read();
        docs.iter()
            .enumerate()
            .filter(|(i, _)| all || Some(*i) == active)
            .map(|(_, tab)| DocFacts::of(tab.id, &tab.doc, tab.page, &tab.selected_pages))
            .collect()
    };
    if how == Start::FormData && !docs.iter().any(|d| d.has_form) {
        st.toast(ToastKind::Info, t(lang, "toast.no_form_fields"), "");
        return;
    }
    forget();
    *MAIL.write() = Some(new_draft(lang, how, &docs));
    st.set_dialog(Dialog::Mail);
    schedule(st);
}

/// Change row `i` (what it prepares): it is prepared again.
pub fn edit_row(st: AppState, i: usize, f: impl FnOnce(&mut MailItem)) {
    let Some(id) = current_id() else { return };
    let changed = with_draft(id, |d| {
        if let Some(it) = d.items.get_mut(i) {
            f(it);
            touch(d, i);
        }
    });
    if changed {
        stop(id, Some(i));
        schedule(st);
    }
}

/// Change what the archive holds or how a file is named: the archive is
/// made again, the files are not.
pub fn edit_names(st: AppState, f: impl FnOnce(&mut MailDraft)) {
    let Some(id) = current_id() else { return };
    if with_draft(id, |d| {
        f(d);
        touch_zip(d);
    }) {
        schedule(st);
    }
}

/// Change the subject, the message or the row shown: nothing is prepared again.
pub fn edit_text(f: impl FnOnce(&mut MailDraft)) {
    if let Some(id) = current_id() {
        with_draft(id, f);
    }
}

/// Start the work a quarter of a second after the last change.
pub fn schedule(st: AppState) {
    let generation = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    dioxus::core::spawn_forever(async move {
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        if GENERATION.load(Ordering::SeqCst) == generation {
            start_jobs(st);
        }
    });
}

/// Store the result of row `i` when the row is still the one it was made for.
fn set_prep(id: u64, i: usize, rev: u64, prep: Prep) -> bool {
    let mut stored = false;
    with_draft(id, |d| {
        if let Some(it) = d.items.get_mut(i) {
            if it.rev == rev {
                it.prep = prep;
                stored = true;
            }
        }
    });
    stored
}

/// What preparing `item` needs from its document, read on this thread.
fn job_for(st: AppState, item: &MailItem, plan: Plan) -> Result<Job, Problem> {
    let docs = st.docs.read();
    let tab = docs.iter().find(|t| t.id == item.doc.tab);
    match plan {
        Plan::File(path) => Ok(Job::File {
            same_as: tab
                .filter(|t| !t.doc.is_dirty() && t.doc.path() == Some(path.as_path()))
                .map(|t| t.doc.copy_source()),
            path,
            protected: item.doc.protected,
        }),
        Plan::Copy(options) => {
            let tab = tab.ok_or(Problem::Closed)?;
            Ok(Job::Copy {
                source: tab.doc.copy_source(),
                options,
            })
        }
        Plan::Data(kind) => {
            let tab = tab.ok_or(Problem::Closed)?;
            let bytes = match kind {
                DataKind::Form(f) => tab.doc.export_form_data(f),
                DataKind::Comments(f) => tab
                    .doc
                    .export_comments(f, None)
                    .map_err(|e| Problem::Other(e.to_string()))?,
            };
            Ok(Job::Data { bytes })
        }
    }
}

/// Start the rows that wait (two at a time) and the archive once they are ready.
fn start_jobs(st: AppState) {
    let Some(d) = MAIL.peek().clone() else { return };
    let in_flight = running().iter().filter(|e| e.0 == d.id).count();
    for i in rows_to_start(&d, AT_ONCE, in_flight) {
        let item = &d.items[i];
        let rev = item.rev;
        let job = match item.plan().and_then(|plan| job_for(st, item, plan)) {
            Ok(job) => job,
            Err(p) => {
                set_prep(d.id, i, rev, Prep::Failed(p));
                continue;
            }
        };
        let flag = Arc::new(AtomicBool::new(false));
        running().push((d.id, i, rev, flag.clone()));
        set_prep(d.id, i, rev, Prep::Running(rev));
        let id = d.id;
        dioxus::core::spawn_forever(async move {
            let stop_flag = flag.clone();
            let result = tokio::task::spawn_blocking(move || run_job(job, &stop_flag)).await;
            running().retain(|e| !Arc::ptr_eq(&e.3, &flag));
            let prep = match result {
                Ok(Ok(p)) => Prep::Ready(Arc::new(p)),
                Ok(Err(p)) => Prep::Failed(p),
                Err(e) => Prep::Failed(Problem::Other(e.to_string())),
            };
            set_prep(id, i, rev, prep);
            // A place is free (even when the result came too late to be kept).
            start_jobs(st);
        });
    }
    let Some(d) = MAIL.peek().clone() else { return };
    if zip_wanted(&d) {
        let Some(rows) = prepared_rows(&d) else { return };
        let (id, key) = (d.id, d.zip_key);
        with_draft(id, |d| d.zip_prep = Prep::Running(key));
        dioxus::core::spawn_forever(async move {
            let zip = tokio::task::spawn_blocking(move || zip_of(&rows)).await;
            let prep = match zip {
                Ok(Ok(bytes)) => Prep::Ready(Arc::new(Prepared {
                    bytes: Arc::new(bytes),
                    facts: None,
                    thumbs: Vec::new(),
                    more_pages: 0,
                    text: None,
                    made_here: false,
                })),
                Ok(Err(e)) => Prep::Failed(Problem::Other(e)),
                Err(e) => Prep::Failed(Problem::Other(e.to_string())),
            };
            with_draft(id, |d| {
                if d.zip_key == key {
                    d.zip_prep = prep;
                }
            });
        });
    }
}

/// The message closes: the dialog when it is still the mail dialog of
/// message `id`, and the message itself.
fn finish(st: AppState, id: u64) {
    if current_id() != Some(id) {
        return;
    }
    stop(id, None);
    if *st.dialog.peek() == Dialog::Mail {
        // Closing the dialog forgets the message.
        st.set_dialog(Dialog::None);
    }
    *MAIL.write() = None;
}

/// What the files of the message are, with the subject and the text.
fn message() -> Option<(u64, String, String, Option<Vec<Outgoing>>)> {
    let r = MAIL.peek();
    let d = r.as_ref()?;
    Some((d.id, d.subject.clone(), d.body.clone(), attachments(d)))
}

/// Save the message as a draft where the user chooses (Save Draft…).
pub fn save_draft(st: AppState) {
    let lang = st.lang();
    let Some((_, subject, _, Some(_))) = message() else {
        st.toast(ToastKind::Info, t(lang, "dialog.mail.wait"), "");
        return;
    };
    dioxus::core::spawn_forever(async move {
        let Some(file) = rfd::AsyncFileDialog::new()
            .set_title(t(lang, "dialog.mail.draft_title"))
            .add_filter(t(lang, "dialog.mail.eml_filter"), &["eml"])
            .set_file_name(mail::draft_file_name(&subject))
            .save_file()
            .await
        else {
            return;
        };
        let mut path = file.path().to_path_buf();
        if !path.extension().is_some_and(|e| e.eq_ignore_ascii_case("eml")) {
            let mut name = path.file_name().unwrap_or_default().to_os_string();
            name.push(".eml");
            path.set_file_name(name);
        }
        if let Err(e) = save_draft_to(st, &path).await {
            st.error(e);
        }
    });
}

/// Write the draft of the message at `path` and close the dialog: what Save
/// Draft… does once the file is chosen.
pub async fn save_draft_to(st: AppState, path: &Path) -> Result<(), String> {
    let lang = st.lang();
    let Some((id, subject, body, files)) = message() else {
        return Err(t(lang, "dialog.mail.nothing"));
    };
    let files = files.ok_or_else(|| t(lang, "dialog.mail.wait"))?;
    set_phase(id, Phase::Writing);
    let target = path.to_path_buf();
    let written = tokio::task::spawn_blocking(move || mail::write_draft(&target, &subject, &body, &files))
        .await
        .map_err(|e| e.to_string())
        .and_then(|r| r);
    match written {
        Ok(()) => {
            finish(st, id);
            st.toast(
                ToastKind::Ok,
                tf(
                    lang,
                    "toast.mail_draft_saved",
                    &[("name", &path.display().to_string())],
                ),
                t(lang, "toast.mail_not_sent"),
            );
            Ok(())
        }
        Err(e) => {
            set_phase(id, Phase::Editing);
            Err(e)
        }
    }
}

/// What a failure of the mail program says.
pub fn reason_text(lang: Lang, e: &HandOverError) -> String {
    let (key, detail) = match e {
        HandOverError::NoProgram(d) => ("dialog.mail.fail_no_program", d),
        HandOverError::Attachments(d) => ("dialog.mail.fail_attach", d),
        HandOverError::Refused(d) => ("dialog.mail.fail_refused", d),
    };
    if detail.is_empty() {
        t(lang, key)
    } else {
        format!("{} ({detail})", t(lang, key))
    }
}

/// Write the files into a private folder and hand them to the mail program
/// (Open in Mail Program). A failure keeps the dialog open with the reason
/// and the ways to go on (save the draft, open it, show the files).
pub fn hand_over(st: AppState) {
    let lang = st.lang();
    let Some((id, subject, body, Some(files))) = message() else {
        st.toast(ToastKind::Info, t(lang, "dialog.mail.wait"), "");
        return;
    };
    set_phase(id, Phase::Writing);
    dioxus::core::spawn_forever(async move {
        let written = tokio::task::spawn_blocking(move || {
            let dir = mail::outbox()?;
            let paths = mail::write_files(&dir, &files)?;
            Ok::<_, String>((dir, paths))
        })
        .await
        .map_err(|e| e.to_string())
        .and_then(|r| r);
        let (dir, paths) = match written {
            Ok(w) => w,
            Err(e) => {
                set_phase(id, Phase::Editing);
                st.error(e);
                return;
            }
        };
        // Closed while the files were written: nothing is handed over.
        if current_id() != Some(id) {
            let _ = std::fs::remove_dir_all(&dir);
            return;
        }
        let count = paths.len();
        with_draft(id, |d| {
            d.outbox = Some(dir);
            d.phase = Phase::Handing;
        });
        let sent = tokio::task::spawn_blocking(move || mail::hand_over(&subject, &body, &paths))
            .await
            .unwrap_or_else(|e| Err(HandOverError::Refused(e.to_string())));
        let here = current_id() == Some(id) && *st.dialog.peek() == Dialog::Mail;
        let check = tf(lang, "toast.mail_check", &[("count", &count.to_string())]);
        match sent {
            Ok(Sent::Attached) => {
                finish(st, id);
                st.toast(ToastKind::Ok, t(lang, "toast.mail_ready"), check);
            }
            Ok(Sent::Opened) if here => set_phase(id, Phase::Opened(count)),
            Ok(Sent::Opened) => st.toast(ToastKind::Info, t(lang, "toast.mail_ready"), check),
            Ok(Sent::Cancelled) => set_phase(id, Phase::Editing),
            Err(e) if here => set_phase(id, Phase::Failed(e)),
            Err(e) => st.error(reason_text(lang, &e)),
        }
    });
}

/// Write the draft next to the files and open it with the program the
/// system has for e-mail files (Open the Draft).
pub fn open_draft(st: AppState) {
    let lang = st.lang();
    let Some((id, subject, body, Some(files))) = message() else {
        return;
    };
    let dir = MAIL.peek().as_ref().and_then(|d| d.outbox.clone());
    dioxus::core::spawn_forever(async move {
        let written = tokio::task::spawn_blocking(move || {
            let dir = match dir {
                Some(d) => d,
                None => mail::outbox()?,
            };
            let path: PathBuf = mail::unique_path(&dir, &mail::draft_file_name(&subject));
            mail::write_draft(&path, &subject, &body, &files)?;
            Ok::<_, String>(path)
        })
        .await
        .map_err(|e| e.to_string())
        .and_then(|r| r);
        match written.and_then(|path| mail::open_with_system(&path).map(|()| path)) {
            Ok(path) => {
                finish(st, id);
                st.toast(
                    ToastKind::Info,
                    t(lang, "toast.mail_draft"),
                    path.display().to_string(),
                );
            }
            Err(e) => st.error(e),
        }
    });
}

/// Show the folder of the files handed to the mail program (Show the Files).
pub fn show_files(st: AppState) {
    let lang = st.lang();
    let Some(dir) = MAIL.peek().as_ref().and_then(|d| d.outbox.clone()) else {
        return;
    };
    match mail::open_with_system(&dir) {
        Ok(()) => st.toast(
            ToastKind::Info,
            t(lang, "toast.mail_folder"),
            t(lang, "toast.mail_folder_hint"),
        ),
        Err(e) => st.error(e),
    }
}

/// Close the dialog once the message is in the mail program (Done).
pub fn done(st: AppState) {
    if let Some(id) = current_id() {
        finish(st, id);
    }
}

handlers! {
    "sendByEmail" => |st| start(st, Start::Documents { all: false, zip: false }),
    "sendByEmailZip" => |st| start(st, Start::Documents { all: false, zip: true }),
    "sendByEmailOpenAll" => |st| start(st, Start::Documents { all: true, zip: false }),
    "sendByEmailOpenAllZip" => |st| start(st, Start::Documents { all: true, zip: true }),
    "forms.emailData" => |st| start(st, Start::FormData),
}
