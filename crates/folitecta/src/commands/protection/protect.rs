//! Protect: redaction, password protection, document scripts.

use folitecta_core::redaction::RedactionMethod;

use crate::commands::{confirm, doc_dialog, handlers, no_doc};
use crate::files;
use crate::i18n::{t, tf};
use crate::state::{AppState, ConfirmAction, Dialog, ToastKind, Tool};

/// Apply the redaction marks of `pages` (all when `None`) the way the
/// redaction settings say, and tell what was removed.
pub fn apply_redactions_now(st: AppState, pages: Option<Vec<usize>>) {
    let method = if st.value("redact.method") == "picture" {
        RedactionMethod::Picture
    } else {
        RedactionMethod::Content
    };
    match st.with_active(|t| t.doc.apply_redactions_with(pages.as_deref(), method)) {
        Some(Ok(r)) => {
            files::set_window_title(st);
            st.set_dialog(Dialog::None);
            let lang = st.lang();
            let mut detail = tf(
                lang,
                "toast.redaction_detail",
                &[
                    ("glyphs", &r.glyphs.to_string()),
                    ("pictures", &r.pictures.to_string()),
                    ("comments", &r.comments.to_string()),
                    ("fields", &r.fields.to_string()),
                ],
            );
            if !r.pages_as_pictures.is_empty() {
                detail.push(' ');
                detail.push_str(&tf(
                    lang,
                    "toast.redaction_as_pictures",
                    &[("pages", &crate::util::format_pages(&r.pages_as_pictures))],
                ));
            }
            st.toast(
                ToastKind::Ok,
                tf(
                    lang,
                    "toast.redactions_applied",
                    &[("count", &r.areas.to_string())],
                ),
                detail,
            );
        }
        Some(Err(e)) => st.error(e),
        None => st.set_dialog(Dialog::None),
    }
}

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
    "saveUnrestricted" => |st| files::save_unprotected_copy(st),
}
