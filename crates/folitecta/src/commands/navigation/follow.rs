//! Following the links and buttons of a document. What stays inside it —
//! a page, a destination, a named action, clearing the form — runs at once;
//! what reaches outside — a web address, a file, another document — is
//! named and asked first, unless the document was trusted for the session.
//! Programs are never started from a document, and form data is never sent.

use std::path::PathBuf;

use folitecta_core::LinkTarget;

use crate::i18n::tf;
use crate::state::{AppState, ConfirmAction, Dialog, ToastKind};

/// Extensions of files that run as programs when opened.
const PROGRAMS: &[&str] = &[
    "exe", "com", "bat", "cmd", "msi", "scr", "ps1", "vbs", "vbe", "js", "jse", "wsf", "wsh", "hta", "cpl",
    "jar", "sh", "bash", "zsh", "csh", "command", "app", "desktop", "appimage", "run", "bin", "py", "pl",
    "rb", "lnk", "reg",
];

/// A file named by a document, relative to the document's folder.
fn file_of(st: AppState, name: &str) -> PathBuf {
    let path = PathBuf::from(name.trim_start_matches("file://"));
    if path.is_absolute() {
        return path;
    }
    st.read_active(|t| t.doc.path().and_then(|p| p.parent()).map(|d| d.join(&path)))
        .flatten()
        .unwrap_or(path)
}

fn is_program(path: &std::path::Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| PROGRAMS.contains(&e.to_ascii_lowercase().as_str()))
}

/// Follow `target`, a link or button of the active document.
pub fn follow(st: AppState, target: LinkTarget) {
    let lang = st.lang();
    match &target {
        LinkTarget::Page(p) => st.go_to_page(*p),
        LinkTarget::Named(name) => {
            let page = st
                .read_active(|t| {
                    t.doc
                        .named_destinations()
                        .into_iter()
                        .find(|d| d.name == *name)
                        .and_then(|d| d.page)
                })
                .flatten();
            match page {
                Some(p) => st.go_to_page(p),
                None => st.toast(
                    ToastKind::Warn,
                    tf(lang, "toast.link_nowhere", &[("name", name)]),
                    "",
                ),
            }
        }
        LinkTarget::NamedAction(action) => {
            let (page, count) = (st.current_page(), st.page_count());
            match action.as_str() {
                "NextPage" => st.go_to_page((page + 1).min(count.saturating_sub(1))),
                "PrevPage" => st.go_to_page(page.saturating_sub(1)),
                "FirstPage" => st.go_to_page(0),
                "LastPage" => st.go_to_page(count.saturating_sub(1)),
                "Print" => crate::commands::dispatch(st, "print"),
                _ => st.toast(
                    ToastKind::Info,
                    tf(lang, "toast.link_action", &[("what", &target.describe())]),
                    "",
                ),
            }
        }
        LinkTarget::Reset => crate::commands::confirm(
            st,
            "cmd.forms.reset",
            "dialog.confirm.reset_form",
            ConfirmAction::ResetForm,
        ),
        LinkTarget::Submit(to) => st.toast(ToastKind::Info, tf(lang, "toast.link_submit", &[("to", to)]), ""),
        LinkTarget::Uri(_) | LinkTarget::Launch(_) | LinkTarget::Remote(_) => {
            if let LinkTarget::Launch(name) = &target {
                if is_program(&file_of(st, name)) {
                    st.toast(
                        ToastKind::Warn,
                        tf(lang, "toast.link_program", &[("file", name)]),
                        "",
                    );
                    return;
                }
            }
            let trusted = st.read_active(|t| t.trust_links).unwrap_or(false);
            match st.read_active(|t| t.id) {
                Some(tab) if !trusted => st.set_dialog(Dialog::OpenTarget { tab, target }),
                _ => open(st, &target),
            }
        }
        LinkTarget::Other(_) => st.toast(
            ToastKind::Info,
            tf(lang, "toast.link_action", &[("what", &target.describe())]),
            "",
        ),
    }
}

/// Open what an external target names (asked already).
pub fn open(st: AppState, target: &LinkTarget) {
    match target {
        LinkTarget::Uri(u) => crate::commands::open_url(st, u),
        LinkTarget::Launch(name) => {
            let path = file_of(st, name);
            if is_program(&path) {
                return;
            }
            if let Err(e) = crate::util::open_external(&path.display().to_string()) {
                st.error(e);
            }
        }
        LinkTarget::Remote(name) => {
            let path = file_of(st, name);
            if path.exists() {
                crate::files::open_path(st, path, None);
            } else {
                st.toast(
                    ToastKind::Warn,
                    tf(
                        st.lang(),
                        "toast.link_missing_file",
                        &[("file", &path.display().to_string())],
                    ),
                    "",
                );
            }
        }
        _ => {}
    }
}
