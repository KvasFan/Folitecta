//! Edit menu: undo/redo, selection, clipboard, find, preferences.

use super::{core_result, handlers, no_doc, show_pane};
use crate::files;
use crate::i18n::t;
use crate::state::{AppState, Clip, Dialog, Pane, ToastKind};
use dioxus::prelude::ReadableExt;

fn select_all(st: AppState) {
    if no_doc(st) {
        return;
    }
    st.with_active(|t| {
        let n = t.doc.page_count();
        t.selected_pages = (0..n).collect();
    });
}

/// Copy the selected comment (internal clipboard + its text) or the current page as an image.
fn copy(st: AppState) {
    if no_doc(st) {
        return;
    }
    let sel = st.read_active(|t| (t.id, t.selected_annot, t.page)).unwrap();
    if let (tab, Some((page, index)), _) = sel {
        let text = st
            .read_active(|t| {
                t.doc
                    .annotations(page)
                    .ok()
                    .and_then(|a| a.get(index).map(|x| x.contents.clone()))
            })
            .flatten()
            .unwrap_or_default();
        st.set_clip(Some(Clip::Annotation { tab, page, index }));
        if !text.is_empty() {
            crate::clipboard::set_text(&text);
        }
        st.toast(ToastKind::Ok, t(st.lang(), "toast.copied"), "");
        return;
    }
    let (_, _, page) = sel;
    let png = st.read_active(|t| t.doc.render_page_png(page, 2.0));
    match png {
        Some(Ok(img)) => match crate::clipboard::set_png(&img.png) {
            Ok(()) => st.toast(ToastKind::Ok, t(st.lang(), "toast.page_copied"), ""),
            Err(e) => st.error(e),
        },
        Some(Err(e)) => st.error(e),
        None => {}
    }
}

fn cut(st: AppState) {
    if no_doc(st) {
        return;
    }
    let sel = st.read_active(|t| t.selected_annot).flatten();
    match sel {
        Some(_) => {
            copy(st);
            // Keep a detached copy so the paste still works after deletion.
            let snapshot = st.read_active(|t| {
                let (p, i) = t.selected_annot.unwrap();
                t.doc.annotation_snapshot(p, i).ok()
            });
            if let Some(Some(s)) = snapshot {
                st.set_clip(Some(Clip::Detached(s)));
            }
            let r = st.with_active(|t| {
                let (p, i) = t.selected_annot.unwrap();
                t.doc.delete_annotation(p, i)
            });
            core_result(st, r);
        }
        None => st.toast(ToastKind::Info, t(st.lang(), "toast.select_comment_first"), ""),
    }
}

/// Paste the internal comment clipboard, else a clipboard image, else clipboard text.
fn paste(st: AppState) {
    if no_doc(st) {
        return;
    }
    if let Some(clip) = st.clip.peek().clone() {
        paste_clip(st, clip);
        return;
    }
    if paste_image(st) {
        return;
    }
    paste_text(st);
}

fn paste_clip(st: AppState, clip: Clip) {
    let page = st.current_page();
    let r = match clip {
        Clip::Annotation {
            tab,
            page: src_page,
            index,
        } => {
            let same = st.read_active(|t| t.id == tab).unwrap_or(false);
            if same {
                st.with_active(|t| {
                    t.doc
                        .duplicate_annotation(src_page, index, page, 12.0, -12.0)
                        .map(|_| ())
                })
            } else {
                let snap = st
                    .docs
                    .read()
                    .iter()
                    .find(|d| d.id == tab)
                    .and_then(|d| d.doc.annotation_snapshot(src_page, index).ok());
                match snap {
                    Some(s) => {
                        st.with_active(|t| t.doc.paste_annotation_snapshot(page, &s, 12.0, -12.0).map(|_| ()))
                    }
                    None => {
                        st.toast(ToastKind::Warn, t(st.lang(), "toast.clip_gone"), "");
                        return;
                    }
                }
            }
        }
        Clip::Detached(s) => {
            st.with_active(|t| t.doc.paste_annotation_snapshot(page, &s, 0.0, 0.0).map(|_| ()))
        }
    };
    core_result(st, r);
}

fn paste_image(st: AppState) -> bool {
    let Some(png) = crate::clipboard::get_png() else {
        return false;
    };
    let page = st.current_page();
    let r = st.with_active(|t| t.doc.add_image(page, png, None).map(|_| ()));
    core_result(st, r);
    true
}

fn paste_text(st: AppState) {
    let Some(text) = crate::clipboard::get_text().filter(|s| !s.trim().is_empty()) else {
        st.toast(ToastKind::Info, t(st.lang(), "toast.clipboard_empty"), "");
        return;
    };
    let page = st.current_page();
    let author = st.ui.read().author.clone();
    let r = st.with_active(|t| {
        let s = t.page_sizes[page];
        let lines = text.lines().count().max(1) as f32;
        let longest = text
            .lines()
            .map(|l| folitecta_core::util::helvetica_width(l, 12.0))
            .fold(40.0, f32::max);
        let w = (longest + 16.0).clamp(80.0, s.width * 0.8);
        let h = (lines * 14.5 + 10.0).clamp(24.0, s.height * 0.8);
        let c = s.crop_box;
        let (cx, cy) = ((c[0] + c[2]) / 2.0, (c[1] + c[3]) / 2.0);
        let rect = [cx - w / 2.0, cy - h / 2.0, cx + w / 2.0, cy + h / 2.0];
        t.doc
            .add_free_text(page, rect, &text, &author, 12.0, [0.0, 0.0, 0.0], false)
            .map(|_| ())
    });
    core_result(st, r);
}

/// New document from the clipboard: a picture becomes a page; text becomes a
/// page with the text as page content.
fn new_doc_from_clipboard(st: AppState) {
    let doc = if let Some(png) = crate::clipboard::get_png() {
        folitecta_core::Document::from_images(vec![png])
    } else if let Some(text) = crate::clipboard::get_text().filter(|s| !s.trim().is_empty()) {
        folitecta_core::Document::from_text(&text, folitecta_core::TextLayout::default())
    } else {
        st.toast(ToastKind::Info, t(st.lang(), "toast.clipboard_empty"), "");
        return;
    };
    match doc {
        Ok(mut doc) => {
            doc.clear_history();
            let id = st.alloc_id();
            st.docs_mut().push(crate::state::DocTab::new(id, doc));
            let n = st.docs.read().len();
            st.set_active(n - 1);
            files::set_window_title(st);
        }
        Err(e) => st.error(e),
    }
}

/// Insert the clipboard picture as a new page after the current one.
fn insert_page_from_clipboard(st: AppState) {
    if no_doc(st) {
        return;
    }
    let Some(png) = crate::clipboard::get_png() else {
        st.toast(ToastKind::Info, t(st.lang(), "toast.clipboard_no_image"), "");
        return;
    };
    let at = st.current_page() + 1;
    match st.with_active(|t| t.doc.insert_image_pages(at, vec![png])) {
        Some(Ok(_)) => {
            files::set_window_title(st);
            st.go_to_page(at);
        }
        Some(Err(e)) => st.error(e),
        None => {}
    }
}

handlers! {
    "newDocClipboard" => new_doc_from_clipboard,
    "document.insertPagesFromClipboard" => insert_page_from_clipboard,
    "undo" => |st| {
        if no_doc(st) { return; }
        match st.with_active(|t| t.doc.undo()) { Some(Err(_)) => st.toast(ToastKind::Info, t(st.lang(), "toast.nothing_to_undo"), ""), _ => files::set_window_title(st) }
    },
    "redo" => |st| {
        if no_doc(st) { return; }
        match st.with_active(|t| t.doc.redo()) { Some(Err(_)) => st.toast(ToastKind::Info, t(st.lang(), "toast.nothing_to_redo"), ""), _ => files::set_window_title(st) }
    },
    "selectAll" => select_all,
    "invertSelection" => |st| { if !no_doc(st) { st.with_active(|t| { let n = t.doc.page_count(); t.selected_pages = (0..n).filter(|p| !t.selected_pages.contains(p)).collect(); }); } },
    "edit.deselect" => |st| { st.with_active(|t| { t.selected_pages.clear(); t.selected_annot = None; }); },
    "delete" => |st| {
        if no_doc(st) { return; }
        match st.read_active(|t| t.selected_annot).flatten() {
            Some((page, index)) => core_result(st, st.with_active(|t| t.doc.delete_annotation(page, index))),
            None => st.set_dialog(Dialog::DeletePages),
        }
    },
    "copy" => copy,
    "cut" => cut,
    "paste" => paste,
    "pasteSpecial" => |st| st.set_dialog(Dialog::PasteSpecial),
    "pasteImage" => |st| { if !no_doc(st) && !paste_image(st) { st.toast(ToastKind::Info, t(st.lang(), "toast.clipboard_no_image"), ""); } },
    "pastePlainText" => |st| { if !no_doc(st) { paste_text(st) } },
    "pasteAnnots" => |st| {
        if no_doc(st) { return; }
        match st.clip.peek().clone() { Some(c) => paste_clip(st, c), None => st.toast(ToastKind::Info, t(st.lang(), "toast.clipboard_empty"), "") }
    },
    "find" => |st| show_pane(st, Pane::Search),
    "findGroup" => |st| show_pane(st, Pane::Search),
    "find.edit" => |st| show_pane(st, Pane::Search),
    "search" => |st| show_pane(st, Pane::Search),
    "search.start" => |st| show_pane(st, Pane::Search),
    "prefs" => |st| st.set_dialog(Dialog::Prefs),
    "view.uiOptions" => |st| st.set_dialog(Dialog::Prefs),
    "settings" => |st| st.set_dialog(Dialog::Prefs),
    "settings.manage" => |st| st.set_dialog(Dialog::Prefs),
    "settings.reset" => |st| {
        let recent = st.ui.read().recent.clone();
        *st.ui_mut() = crate::state::UiPrefs { recent, ..Default::default() };
        st.save_prefs();
        st.toast(ToastKind::Ok, t(st.lang(), "toast.settings_reset"), "");
    },
    "settings.export" => |st| files::export_settings(st),
    "settings.import" => |st| files::import_settings(st),
    "saveSettingsAndHistory" => |st| { st.save_prefs(); st.toast(ToastKind::Ok, t(st.lang(), "toast.settings_saved"), ""); },
}
