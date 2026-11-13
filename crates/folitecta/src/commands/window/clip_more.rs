//! More of the clipboard: ruler guides chosen (Guides › Select All, Select
//! on Page), copied and pasted (Paste Guides) or deleted; form fields chosen
//! in the form tool, copied and pasted on the current page of any document
//! (Paste Fields); files copied in a file manager pasted as pages and
//! attachments (Paste Files); the document's file put on the clipboard
//! (Copy File).

use dioxus::prelude::ReadableExt;

use crate::commands::{handlers, no_doc};
use crate::i18n::{t, tf};
use crate::state::{AppState, Clip, ToastKind, Tool};

/// Choose the guides of the document, or those shown on the current page.
fn select_guides(st: AppState, page_only: bool) {
    if no_doc(st) {
        return;
    }
    let page = st.current_page();
    let n = st
        .with_active(|t| {
            t.guide_sel = t
                .guides
                .iter()
                .enumerate()
                .filter(|(_, g)| !page_only || g.page.is_none() || g.page == Some(page))
                .map(|(i, _)| i)
                .collect();
            t.guide_sel.len()
        })
        .unwrap_or(0);
    if n == 0 {
        st.toast(ToastKind::Info, t(st.lang(), "toast.no_guides"), "");
    } else {
        st.toast(
            ToastKind::Ok,
            tf(st.lang(), "toast.guides_chosen", &[("count", &n.to_string())]),
            "",
        );
    }
}

/// The guides chosen in the active tab.
fn chosen_guides(st: AppState) -> Vec<crate::state::Guide> {
    st.read_active(|t| {
        t.guide_sel
            .iter()
            .filter_map(|i| t.guides.get(*i).copied())
            .collect()
    })
    .unwrap_or_default()
}

/// Edit › Copy with guides chosen: they are what is copied.
pub fn copy_guides(st: AppState) -> bool {
    let guides = chosen_guides(st);
    if guides.is_empty() {
        return false;
    }
    let n = guides.len();
    st.set_clip(Some(Clip::Guides(guides)));
    st.toast(
        ToastKind::Ok,
        tf(st.lang(), "toast.guides_on_clip", &[("count", &n.to_string())]),
        "",
    );
    true
}

/// Edit › Delete with guides chosen: they go.
pub fn delete_guides(st: AppState) -> bool {
    let chosen = st.read_active(|t| t.guide_sel.clone()).unwrap_or_default();
    if chosen.is_empty() {
        return false;
    }
    st.with_active(|t| {
        let mut i = 0;
        t.guides.retain(|_| {
            let keep = !chosen.contains(&i);
            i += 1;
            keep
        });
        t.guide_sel.clear();
    });
    true
}

/// Paste the guides copied: those of one page go to the current page.
pub fn paste_guides(st: AppState) {
    if no_doc(st) {
        return;
    }
    let Some(Clip::Guides(guides)) = st.clip.peek().clone() else {
        st.toast(ToastKind::Info, t(st.lang(), "toast.clip_no_guides"), "");
        return;
    };
    let page = st.current_page();
    let n = st
        .with_active(|t| {
            let first = t.guides.len();
            for g in &guides {
                let g = crate::state::Guide {
                    page: g.page.map(|_| page),
                    ..*g
                };
                if !t.guides.contains(&g) {
                    t.guides.push(g);
                }
            }
            t.guide_sel = (first..t.guides.len()).collect();
            t.guides.len() - first
        })
        .unwrap_or(0);
    st.set_option("view.showGuides", true);
    st.toast(
        ToastKind::Ok,
        tf(st.lang(), "toast.guides_pasted", &[("count", &n.to_string())]),
        "",
    );
}

/// Edit › Copy with fields chosen in the form tool: the fields are copied.
pub fn copy_fields(st: AppState) -> bool {
    if *st.tool.peek() != Tool::EditFields {
        return false;
    }
    let chosen = st.read_active(|t| (t.id, t.selected_fields.clone()));
    let Some((tab, ids)) = chosen.filter(|(_, ids)| !ids.is_empty()) else {
        return false;
    };
    let copies: Result<Vec<_>, _> = st
        .read_active(|t| ids.iter().map(|id| t.doc.field_copy(*id)).collect())
        .unwrap_or(Ok(Vec::new()));
    match copies {
        Ok(fields) => {
            let n = fields.len();
            st.set_clip(Some(Clip::Fields { tab, fields }));
            st.toast(
                ToastKind::Ok,
                tf(st.lang(), "toast.fields_copied", &[("count", &n.to_string())]),
                "",
            );
        }
        Err(e) => st.error(st.core_error_text(&e)),
    }
    true
}

/// Paste the fields copied on the current page (a little lower and to the
/// right in the document they came from); they are chosen afterwards.
pub fn paste_fields(st: AppState) {
    if no_doc(st) || !st.allowed(folitecta_core::Restricted::Modify) {
        return;
    }
    let Some(Clip::Fields { tab, fields }) = st.clip.peek().clone() else {
        st.toast(ToastKind::Info, t(st.lang(), "toast.clip_no_fields"), "");
        return;
    };
    let page = st.current_page();
    let same = st.read_active(|t| t.id == tab).unwrap_or(false);
    let (dx, dy) = if same { (12.0, -12.0) } else { (0.0, 0.0) };
    match st.with_active(|t| t.doc.paste_fields(page, &fields, dx, dy)) {
        Some(Ok(made)) => {
            let n = made.len();
            st.with_active(|t| t.selected_fields = made);
            crate::files::set_window_title(st);
            st.toast(
                ToastKind::Ok,
                tf(st.lang(), "toast.fields_pasted", &[("count", &n.to_string())]),
                "",
            );
        }
        Some(Err(e)) => st.error(st.core_error_text(&e)),
        None => {}
    }
}

/// Paste Files: the files copied in a file manager, after the current page
/// (PDF files and pictures as pages, other files attached).
pub fn paste_files_now(st: AppState, paths: Vec<std::path::PathBuf>) {
    if no_doc(st) || !st.allowed(folitecta_core::Restricted::Assemble) {
        return;
    }
    if paths.is_empty() {
        st.toast(ToastKind::Info, t(st.lang(), "toast.clipboard_no_files"), "");
        return;
    }
    let at = st.current_page() + 1;
    match st.with_active(|t| t.doc.paste_files(at, &paths)) {
        Some(Ok((pages, attached))) => {
            crate::files::set_window_title(st);
            if pages > 0 {
                st.go_to_page(at);
            }
            st.toast(
                ToastKind::Ok,
                tf(
                    st.lang(),
                    "toast.files_pasted",
                    &[("pages", &pages.to_string()), ("files", &attached.to_string())],
                ),
                "",
            );
        }
        Some(Err(e)) => st.error(st.core_error_text(&e)),
        None => {}
    }
}

/// Copy File: the file of the document on the clipboard, as a file manager
/// copies it (its path as text too).
fn copy_file(st: AppState) {
    let path = st
        .read_active(|t| t.doc.path().map(|p| p.to_path_buf()))
        .flatten();
    let Some(path) = path else {
        st.toast(ToastKind::Warn, t(st.lang(), "toast.no_path"), "");
        return;
    };
    if st.read_active(|t| t.doc.is_dirty()).unwrap_or(false) {
        st.toast(ToastKind::Info, t(st.lang(), "toast.copy_file_saved"), "");
    }
    match crate::clipboard::set_files(std::slice::from_ref(&path)) {
        Ok(()) => st.toast(
            ToastKind::Ok,
            t(st.lang(), "toast.file_copied"),
            path.display().to_string(),
        ),
        Err(e) => st.error(e),
    }
}

handlers! {
    "guidesSelectAll" => |st| select_guides(st, false),
    "guidesSelectOnPage" => |st| select_guides(st, true),
    "pasteGuides" => paste_guides,
    "pasteFields" => paste_fields,
    "pasteFiles" => |st| { let files = crate::clipboard::get_files(); paste_files_now(st, files) },
    "copySrcFile" => copy_file,
}
