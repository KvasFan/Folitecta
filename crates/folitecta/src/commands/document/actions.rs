//! Action Sequences (FOL-L047): sequences kept in the settings, edited in
//! their dialog, imported and exported as JSON; run on the open document
//! (one undo step) or on files and folders: the plan shown first, then the
//! files one by one away from the interface, each with its result, until
//! the end or Cancel (the files not run are counted as skipped).

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

use dioxus::prelude::*;
use folitecta_core::actions::{batch_inputs, plan_batch, run_batch_item};
use folitecta_core::{ActionSequence, BatchItem, FileResult, OutputRule, Step};

use crate::commands::handlers;
use crate::i18n::{t, tf};
use crate::state::{AppState, Dialog, ToastKind};

/// A batch: its plan, how it goes.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Batch {
    pub inputs: Vec<PathBuf>,
    pub rule: OutputRule,
    pub items: Vec<BatchItem>,
    pub running: bool,
    pub results: Vec<FileResult>,
    pub cancelled: bool,
}

/// What the dialog shows.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ActionsState {
    pub sequences: Vec<ActionSequence>,
    pub current: usize,
    pub batch: Option<Batch>,
}

pub static ACTIONS: GlobalSignal<ActionsState> = Signal::global(ActionsState::default);

static CANCEL: AtomicBool = AtomicBool::new(false);

/// The sequences of the settings (one to start with when there are none).
fn load(st: AppState) -> Vec<ActionSequence> {
    let saved: Vec<ActionSequence> =
        serde_json::from_str(&st.value("sequence.sequences")).unwrap_or_default();
    if saved.is_empty() {
        vec![ActionSequence {
            name: t(st.lang(), "sequence.first_name"),
            steps: vec![Step::new("recompress").unwrap_or(Step::Grey)],
        }]
    } else {
        saved
    }
}

/// Keep the sequences in the settings.
pub fn save(st: AppState) {
    let list = ACTIONS.peek().sequences.clone();
    st.set_value(
        "sequence.sequences",
        serde_json::to_string(&list).unwrap_or_default(),
    );
}

fn open(st: AppState) {
    let sequences = load(st);
    *ACTIONS.write() = ActionsState {
        sequences,
        current: 0,
        batch: None,
    };
    st.set_dialog(Dialog::Actions);
}

/// Change the sequence shown, and keep it.
pub fn edit(st: AppState, change: impl FnOnce(&mut ActionSequence)) {
    {
        let mut s = ACTIONS.write();
        let i = s.current.min(s.sequences.len().saturating_sub(1));
        if let Some(seq) = s.sequences.get_mut(i) {
            change(seq);
        }
        s.batch = None;
    }
    save(st);
}

pub fn new_sequence(st: AppState) {
    {
        let mut s = ACTIONS.write();
        let n = s.sequences.len() + 1;
        s.sequences.push(ActionSequence {
            name: tf(st.lang(), "sequence.new_name", &[("count", &n.to_string())]),
            steps: Vec::new(),
        });
        s.current = s.sequences.len() - 1;
        s.batch = None;
    }
    save(st);
}

pub fn delete_sequence(st: AppState) {
    {
        let mut s = ACTIONS.write();
        if s.sequences.len() <= 1 {
            return;
        }
        let i = s.current.min(s.sequences.len() - 1);
        s.sequences.remove(i);
        s.current = i.saturating_sub(1);
        s.batch = None;
    }
    save(st);
}

/// The sequence shown.
pub fn current() -> Option<ActionSequence> {
    let s = ACTIONS.peek();
    s.sequences
        .get(s.current.min(s.sequences.len().saturating_sub(1)))
        .cloned()
}

pub fn import(st: AppState) {
    spawn(async move {
        let Some(f) = rfd::AsyncFileDialog::new()
            .add_filter("JSON", &["json"])
            .pick_file()
            .await
        else {
            return;
        };
        let text = String::from_utf8_lossy(&f.read().await).to_string();
        match ActionSequence::from_json(&text) {
            Ok(seq) => {
                {
                    let mut s = ACTIONS.write();
                    s.sequences.push(seq);
                    s.current = s.sequences.len() - 1;
                    s.batch = None;
                }
                save(st);
            }
            Err(e) => st.error(st.core_error_text(&e)),
        }
    });
}

pub fn export(st: AppState) {
    let Some(seq) = current() else { return };
    spawn(async move {
        let name = format!("{}.json", seq.name.replace(['/', '\\', ':'], "-"));
        let Some(f) = rfd::AsyncFileDialog::new()
            .add_filter("JSON", &["json"])
            .set_file_name(&name)
            .save_file()
            .await
        else {
            return;
        };
        match folitecta_core::util::write_atomic(f.path(), seq.to_json()) {
            Ok(()) => st.toast(
                ToastKind::Ok,
                t(st.lang(), "toast.actions_exported"),
                f.path().display().to_string(),
            ),
            Err(e) => st.error(e),
        }
    });
}

/// Run the sequence on the open document.
pub fn run_here(st: AppState) {
    let Some(seq) = current() else { return };
    let lang = st.lang();
    match st.with_active(|tab| tab.doc.run_actions(&seq)) {
        Some(Ok(log)) => {
            st.set_dialog(Dialog::None);
            crate::files::set_window_title(st);
            st.toast(
                ToastKind::Ok,
                tf(lang, "toast.actions_done", &[("name", &seq.name)]),
                log.join("\n"),
            );
        }
        Some(Err((log, e))) => st.toast(
            ToastKind::Error,
            tf(lang, "toast.actions_failed", &[("name", &seq.name)]),
            format!("{}\n{}", log.join("\n"), st.core_error_text(&e)),
        ),
        None => {}
    }
}

/// Choose files (or a folder) and plan the batch.
pub fn choose(st: AppState, folder: bool) {
    spawn(async move {
        let picked: Vec<PathBuf> = if folder {
            rfd::AsyncFileDialog::new()
                .pick_folder()
                .await
                .map(|f| vec![f.path().to_path_buf()])
                .unwrap_or_default()
        } else {
            rfd::AsyncFileDialog::new()
                .add_filter("PDF", &["pdf"])
                .pick_files()
                .await
                .map(|fs| fs.iter().map(|f| f.path().to_path_buf()).collect())
                .unwrap_or_default()
        };
        if picked.is_empty() {
            return;
        }
        let rule = ACTIONS
            .peek()
            .batch
            .as_ref()
            .map(|b| b.rule.clone())
            .unwrap_or_default();
        set_inputs(st, picked, rule);
    });
}

/// Plan the batch of `inputs` with `rule` (again after the rule changes).
pub fn set_inputs(st: AppState, inputs: Vec<PathBuf>, rule: OutputRule) {
    let _ = st;
    let files = batch_inputs(&inputs);
    let items = plan_batch(&files, &rule);
    ACTIONS.write().batch = Some(Batch {
        inputs,
        rule,
        items,
        ..Batch::default()
    });
}

/// Change the output rule of the batch planned.
pub fn set_rule(st: AppState, change: impl FnOnce(&mut OutputRule)) {
    let Some(b) = ACTIONS.peek().batch.clone() else {
        return;
    };
    if b.running {
        return;
    }
    let mut rule = b.rule.clone();
    change(&mut rule);
    set_inputs(st, b.inputs, rule);
}

pub fn choose_output(st: AppState) {
    spawn(async move {
        if let Some(f) = rfd::AsyncFileDialog::new().pick_folder().await {
            let path = f.path().to_path_buf();
            set_rule(st, |r| r.folder = Some(path));
        }
    });
}

/// Run the batch planned, file by file away from the interface.
pub fn run_batch(st: AppState) {
    let Some(seq) = current() else { return };
    let Some(b) = ACTIONS.peek().batch.clone() else {
        return;
    };
    if b.running || b.items.is_empty() {
        return;
    }
    CANCEL.store(false, Ordering::SeqCst);
    if let Some(batch) = ACTIONS.write().batch.as_mut() {
        batch.running = true;
        batch.results.clear();
        batch.cancelled = false;
    }
    spawn(async move {
        for item in b.items.iter().cloned() {
            if CANCEL.load(Ordering::SeqCst) {
                let r = FileResult {
                    input: item.input.clone(),
                    output: item.output.clone(),
                    status: "skipped",
                    message: t(st.lang(), "sequence.cancelled"),
                    log: Vec::new(),
                };
                if let Some(batch) = ACTIONS.write().batch.as_mut() {
                    batch.results.push(r);
                    batch.cancelled = true;
                }
                continue;
            }
            let (s, rule) = (seq.clone(), b.rule.clone());
            let r = tokio::task::spawn_blocking(move || run_batch_item(&s, &item, &rule)).await;
            if let (Ok(r), Some(batch)) = (r, ACTIONS.write().batch.as_mut()) {
                batch.results.push(r);
            }
        }
        if let Some(batch) = ACTIONS.write().batch.as_mut() {
            batch.running = false;
        }
    });
}

pub fn cancel() {
    CANCEL.store(true, Ordering::SeqCst);
}

handlers! {
    "tools.actionSequences" => open,
}
