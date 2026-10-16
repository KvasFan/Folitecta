//! File menu: open, create, save, close, print, properties.

use super::{confirm, doc_dialog, handlers, no_doc, open_url};
use crate::files;
use crate::i18n::t;
use crate::state::{AfterUnsaved, AppState, ConfirmAction, Dialog, ToastKind};
use crate::util;
use dioxus::prelude::ReadableExt;

fn close_where(st: AppState, pick: impl Fn(usize, usize) -> bool) {
    let active = st.active_index().unwrap_or(0);
    let ids: Vec<u64> = st
        .docs
        .read()
        .iter()
        .enumerate()
        .filter(|(i, _)| pick(*i, active))
        .map(|(_, t)| t.id)
        .collect();
    for id in ids {
        files::close_tab(st, id, false, AfterUnsaved::Close);
    }
}

fn copy_path(st: AppState) {
    match st
        .read_active(|t| t.doc.path().map(|p| p.display().to_string()))
        .flatten()
    {
        Some(p) => {
            util::copy_to_clipboard(&p);
            st.toast(ToastKind::Ok, t(st.lang(), "toast.copied"), p);
        }
        None => st.toast(ToastKind::Warn, t(st.lang(), "toast.no_path"), ""),
    }
}

fn duplicate_tab(st: AppState) {
    if no_doc(st) {
        return;
    }
    let bytes = st.read_active(|t| t.doc.bytes()).unwrap();
    match folitecta_core::Document::from_bytes(bytes.as_ref().clone(), None, None) {
        Ok(doc) => {
            let id = st.alloc_id();
            st.docs_mut().push(crate::state::DocTab::new(id, doc));
            let n = st.docs.read().len();
            st.set_active(n - 1);
            files::set_window_title(st);
        }
        Err(e) => st.error(e),
    }
}

handlers! {
    "open" => |st| files::open_dialog(st),
    "openGroup" => |st| files::open_dialog(st),
    "openFrom" => |st| files::open_dialog(st),
    "openFromComp" => |st| files::open_dialog(st),
    "openFromPlaces" => |st| files::open_dialog(st),
    "browseForOpen" => |st| files::open_dialog(st),
    "placesListOpen" => |st| files::open_dialog(st),
    "window.new" => |st| files::open_dialog(st),
    "newDocBlank" => |st| doc_dialog_or_blank(st),
    "newDoc" => |st| files::new_blank(st, 1, 595.0, 842.0),
    "newDocText" => |st| files::new_from_text_file(st),
    "window.newBlank" => |st| files::new_blank(st, 1, 595.0, 842.0),
    "newDocFromFiles" => |st| files::combine_files(st),
    "newDocCombine" => |st| files::combine_files(st),
    "save" => |st| files::save(st),
    "saveAs" => |st| files::save_as(st),
    "saveAsGroup" => |st| files::save_as(st),
    "saveAsCopy" => |st| files::save_copy(st),
    "saveCopyAs" => |st| files::save_copy(st),
    "saveToComp" => |st| files::save_as(st),
    "saveToPlaces" => |st| files::save_as(st),
    "placesListSave" => |st| files::save_as(st),
    "browseForSave" => |st| files::save_as(st),
    "saveAll" => |st| files::save_all(st),
    "revert" => |st| { if !no_doc(st) { confirm(st, "cmd.revert", "dialog.confirm.revert", ConfirmAction::Revert) } },
    "close" => |st| files::close_active(st),
    "closeTab" => |st| files::close_active(st),
    "closeAll" => |st| files::close_all(st),
    "closeOtherTabs" => |st| close_where(st, |i, a| i != a),
    "closeTabsToTheLeft" => |st| close_where(st, |i, a| i < a),
    "closeTabsToTheRight" => |st| close_where(st, |i, a| i > a),
    "duplicateTab" => duplicate_tab,
    "reopenJustClosedDoc" => |st| files::reopen_closed(st),
    "print" => |st| doc_dialog(st, Dialog::Print(false)),
    "printAll" => |st| doc_dialog(st, Dialog::Print(true)),
    "exit" => |st| files::exit_app(st),
    "docProps" => |st| doc_dialog(st, Dialog::DocProps),
    "openSrcFolder" => |st| {
        match st.read_active(|t| t.doc.path().and_then(|p| p.parent().map(|d| d.display().to_string()))).flatten() {
            Some(d) => open_url(st, &d),
            None => st.toast(ToastKind::Warn, t(st.lang(), "toast.no_path"), ""),
        }
    },
    "copySrcFileName" => copy_path,
    "copySrcFile" => copy_path,
    "sendByEmail" => |st| {
        if no_doc(st) { return; }
        let name = st.read_active(|t| t.title()).unwrap_or_default();
        open_url(st, &format!("mailto:?subject={}", name.replace(' ', "%20")));
        if let Some(Some(dir)) = st.read_active(|t| t.doc.path().and_then(|p| p.parent()).map(|d| d.display().to_string())) {
            let _ = util::open_external(&dir);
        }
    },
    "recentFiles" => |st| st.set_palette(Some(String::new())),
    "recentsList" => |st| st.set_palette(Some(String::new())),
    "manageHistory" => |st| { st.ui_mut().recent.clear(); st.save_prefs(); st.toast(ToastKind::Ok, t(st.lang(), "toast.history_cleared"), ""); },
    "historyList.clear" => |st| { st.ui_mut().recent.clear(); st.save_prefs(); st.toast(ToastKind::Ok, t(st.lang(), "toast.history_cleared"), ""); },
}

fn doc_dialog_or_blank(st: AppState) {
    st.set_dialog(Dialog::NewBlank);
}
