//! Protect: redaction, password protection, document scripts.

use crate::commands::{confirm, doc_dialog, handlers, no_doc};
use crate::files;
use crate::i18n::t;
use crate::state::{AppState, ConfirmAction, Dialog, ToastKind, Tool};

fn apply_redactions(st: AppState, current_page: bool) {
    if no_doc(st) {
        return;
    }
    let (page, pending) = st
        .read_active(|t| {
            let all = t.doc.pending_redactions();
            let n = if current_page {
                all.iter().filter(|(p, _)| *p == t.page).count()
            } else {
                all.len()
            };
            (t.page, n)
        })
        .unwrap();
    if pending == 0 {
        st.toast(ToastKind::Info, t(st.lang(), "toast.no_redactions"), "");
        return;
    }
    confirm(
        st,
        "dialog.redact.title",
        "dialog.redact.body",
        ConfirmAction::ApplyRedactions(current_page.then_some(page)),
    );
}

handlers! {
    "tool.annot.redaction" => |st| { if !no_doc(st) { st.set_tool(Tool::Redact) } },
    "document.applyRedactions" => |st| apply_redactions(st, false),
    "annots.applyRedactions" => |st| apply_redactions(st, true),
    "document.security" => |st| doc_dialog(st, Dialog::Security),
    "standardSecPolicy" => |st| doc_dialog(st, Dialog::Security),
    "saveUnrestricted" => |st| files::save_copy(st),
}
