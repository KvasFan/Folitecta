//! Convert Colours to Grey (FOL-L044): what would change looked at on a
//! copy away from the interface, then the colours converted in one step.

use dioxus::prelude::*;
use folitecta_core::GreyReport;

use crate::commands::{handlers, no_doc};
use crate::i18n::t;
use crate::state::{AppState, Dialog, ToastKind};

/// The estimate shown by the dialog (None while it is made).
pub static GREY: GlobalSignal<Option<Result<GreyReport, String>>> = Signal::global(|| None);

fn open(st: AppState) {
    if no_doc(st) {
        return;
    }
    *GREY.write() = None;
    st.set_dialog(Dialog::ConvertGrey);
    let Some(copy) = st.read_active(|t| t.doc.duplicate()) else {
        return;
    };
    spawn(async move {
        let r = tokio::task::spawn_blocking(move || {
            copy.map_err(|e| e.to_string())
                .and_then(|mut c| c.convert_to_grey().map_err(|e| e.to_string()))
        })
        .await;
        if let Ok(r) = r {
            *GREY.write() = Some(r);
        }
    });
}

/// Convert the colours of the active document.
pub fn convert(st: AppState) {
    let lang = st.lang();
    match st.with_active(|t| t.doc.convert_to_grey()) {
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
