//! Preflight and conversion to PDF/A-2b (FOL-L043): the checks run on a
//! copy away from the interface, the converted copy opened in a new tab.

use dioxus::prelude::*;
use folitecta_core::{Conversion, Preflight};

use crate::commands::{handlers, no_doc};
use crate::i18n::t;
use crate::state::{AppState, Dialog, ToastKind};

/// What the dialog shows.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PreflightState {
    pub busy: bool,
    pub tab: u64,
    pub result: Option<Preflight>,
    pub problem: Option<String>,
}

pub static PREFLIGHT: GlobalSignal<PreflightState> = Signal::global(PreflightState::default);

/// Check the active document against PDF/A-2b, away from the interface.
pub fn check(st: AppState) {
    let Some((tab, copy)) = st.read_active(|t| (t.id, t.doc.duplicate())) else {
        return;
    };
    let copy = match copy {
        Ok(c) => c,
        Err(e) => {
            PREFLIGHT.write().problem = Some(st.core_error_text(&e));
            return;
        }
    };
    *PREFLIGHT.write() = PreflightState {
        busy: true,
        tab,
        ..Default::default()
    };
    spawn(async move {
        let found = tokio::task::spawn_blocking(move || copy.preflight_pdfa()).await;
        let mut s = PREFLIGHT.write();
        s.busy = false;
        if let Ok(found) = found {
            s.result = Some(found);
        }
    });
}

/// Convert a copy of the active document and open it in a new tab.
pub fn convert(st: AppState) {
    let Some(copy) = st.read_active(|t| t.doc.duplicate()) else {
        return;
    };
    let copy = match copy {
        Ok(c) => c,
        Err(e) => return st.error(st.core_error_text(&e)),
    };
    PREFLIGHT.write().busy = true;
    spawn(async move {
        let made = tokio::task::spawn_blocking(move || copy.convert_to_pdfa()).await;
        PREFLIGHT.write().busy = false;
        match made {
            Ok(Ok((doc, outcome))) => opened(st, doc, outcome),
            Ok(Err(e)) => PREFLIGHT.write().problem = Some(st.core_error_text(&e)),
            Err(_) => {}
        }
    });
}

fn opened(st: AppState, doc: folitecta_core::Document, outcome: Conversion) {
    let lang = st.lang();
    st.set_dialog(Dialog::None);
    crate::files::open_document_tab(st, doc);
    if outcome.claimed {
        st.toast(ToastKind::Ok, t(lang, "toast.pdfa_done"), "");
    } else {
        let left: Vec<String> = outcome
            .after
            .broken()
            .into_iter()
            .filter(|(r, _)| *r != "identification")
            .map(|(r, n)| format!("{} ({n})", t(lang, &format!("preflight.rule.{r}"))))
            .collect();
        st.toast(ToastKind::Warn, t(lang, "toast.pdfa_not_done"), left.join(", "));
    }
}

fn open(st: AppState) {
    if no_doc(st) {
        return;
    }
    st.set_dialog(Dialog::Preflight);
    check(st);
}

handlers! {
    "document.preflight" => open,
    "document.convertToPDFA" => open,
}
