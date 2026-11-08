//! Convert Colours to Grey (FOL-L044): what would change looked at on a
//! copy away from the interface, then the colours converted in one step.

use dioxus::prelude::*;
use folitecta_core::GreyReport;

use crate::commands::{handlers, no_doc};
use crate::i18n::t;
use crate::state::{AppState, Dialog, ToastKind};

/// The estimate shown by the dialog (None while it is made).
pub static GREY: GlobalSignal<Option<Result<GreyReport, String>>> = Signal::global(|| None);

/// The pages converted: all of them, or only the current one.
pub static GREY_ALL: GlobalSignal<bool> = Signal::global(|| true);

/// Convert the pages chosen of `doc`.
fn convert_scope(
    doc: &mut folitecta_core::Document,
    all: bool,
    page: usize,
) -> folitecta_core::Result<GreyReport> {
    if all {
        doc.convert_to_grey()
    } else {
        doc.convert_pages_to_grey(&[page])
    }
}

fn open(st: AppState) {
    if no_doc(st) {
        return;
    }
    *GREY_ALL.write() = true;
    st.set_dialog(Dialog::ConvertGrey);
    estimate(st);
}

/// Look again at what would become grey, for the pages chosen.
pub fn estimate(st: AppState) {
    *GREY.write() = None;
    let all = *GREY_ALL.peek();
    let Some((copy, page)) = st.read_active(|t| (t.doc.duplicate(), t.page)) else {
        return;
    };
    spawn(async move {
        let r = tokio::task::spawn_blocking(move || {
            copy.map_err(|e| e.to_string())
                .and_then(|mut c| convert_scope(&mut c, all, page).map_err(|e| e.to_string()))
        })
        .await;
        if let Ok(r) = r {
            *GREY.write() = Some(r);
        }
    });
}

/// Choose the pages converted.
pub fn set_all(st: AppState, all: bool) {
    *GREY_ALL.write() = all;
    estimate(st);
}

/// Convert the colours of the active document (the pages chosen).
pub fn convert(st: AppState) {
    let lang = st.lang();
    let all = *GREY_ALL.peek();
    match st.with_active(|t| {
        let page = t.page;
        convert_scope(&mut t.doc, all, page)
    }) {
        Some(Ok(r)) => {
            st.set_dialog(Dialog::None);
            crate::files::set_window_title(st);
            if r.is_empty() {
                st.toast(ToastKind::Info, t(lang, "toast.grey_nothing"), "");
            } else {
                st.toast(ToastKind::Ok, t(lang, "toast.grey_done"), "");
            }
        }
        Some(Err(e)) => st.error(st.core_error_text(&e)),
        None => {}
    }
}

handlers! {
    "document.convertToGrey" => open,
}
