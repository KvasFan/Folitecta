//! Edit menu: undo/redo, selection, clipboard (comments, pictures, text and
//! the pages chosen in the thumbnails), find, preferences.

use crate::commands::{core_result, handlers, no_doc, show_pane};
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
    if crate::commands::clip_more::copy_guides(st) {
        return;
    }
    if st.allowed(folitecta_core::Restricted::Copy) && crate::commands::clip_more::copy_fields(st) {
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
    if !st.allowed(folitecta_core::Restricted::Copy) {
        return;
    }
    // Pages chosen in the thumbnails: the pages themselves too, for Paste
    // Pages (the picture of the page still goes to the system clipboard).
    let chosen = st.read_active(|t| t.selected_pages.clone()).unwrap_or_default();
    let mut pages_copied = None;
    if !chosen.is_empty() {
        let mut pages = chosen;
        pages.sort_unstable();
        match st.read_active(|t| t.doc.extract_pages(&pages).map(|d| d.bytes())) {
            Some(Ok(pdf)) => {
                st.set_clip(Some(Clip::Pages {
                    pdf,
                    count: pages.len(),
                }));
                pages_copied = Some(pages.len());
            }
            Some(Err(e)) => st.error(e),
            None => {}
        }
    }
    // The page as the view shows it.
    let png = st.read_active(|t| t.doc.render_page_png_with_view(page, 2.0, &t.view_layer_states()));
    match png {
        Some(Ok(img)) => match crate::clipboard::set_png(&img.png) {
            Ok(()) => match pages_copied {
                Some(count) => {
                    let text =
                        crate::i18n::tf(st.lang(), "toast.pages_copied", &[("count", &count.to_string())]);
                    st.toast(ToastKind::Ok, text, "");
                }
                None => st.toast(ToastKind::Ok, t(st.lang(), "toast.page_copied"), ""),
            },
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
    // Pages copied go only with Paste Pages: a copy made since in another
    // program comes first here.
    if let Some(clip) = st
        .clip
        .peek()
        .clone()
        .filter(|c| !matches!(c, Clip::Pages { .. }))
    {
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
        Clip::Pages { pdf, .. } => {
            paste_pages(st, &pdf, page + 1, 1);
            return;
        }
        Clip::Fields { .. } => {
            crate::commands::clip_more::paste_fields(st);
            return;
        }
        Clip::Guides(_) => {
            crate::commands::clip_more::paste_guides(st);
            return;
        }
    };
    core_result(st, r);
}

/// Insert the pages copied (`pdf`) `copies` times before page index `at`,
/// one undo step, and go to the first of them.
pub(crate) fn paste_pages(st: AppState, pdf: &std::sync::Arc<Vec<u8>>, at: usize, copies: usize) {
    let src = match folitecta_core::Document::from_bytes(pdf.to_vec(), None, None) {
        Ok(d) => d,
        Err(e) => {
            st.error(e);
            return;
        }
    };
    match st.with_active(|t| t.doc.insert_document_copies(at, &src, copies)) {
        Some(Ok(n)) => {
            files::set_window_title(st);
            st.go_to_page(at);
            let text = crate::i18n::tf(st.lang(), "toast.pages_pasted", &[("count", &n.to_string())]);
            st.toast(ToastKind::Ok, text, "");
        }
        Some(Err(e)) => st.error(e),
        None => {}
    }
}

/// The pages copied, if pages are what was copied last.
fn copied_pages(st: AppState) -> Option<std::sync::Arc<Vec<u8>>> {
    match st.clip.peek().clone() {
        Some(Clip::Pages { pdf, .. }) => Some(pdf),
        _ => None,
    }
}

/// Paste as Stamp: the picture of the clipboard as a stamp in the middle of
/// the current page, at its size (96 to the inch), no larger than most of
/// the page; it moves like any stamp.
fn paste_as_stamp(st: AppState) {
    if no_doc(st) {
        return;
    }
    let Some(png) = crate::clipboard::get_png() else {
        st.toast(ToastKind::Info, t(st.lang(), "toast.clipboard_no_image"), "");
        return;
    };
    let Ok((w, h)) =
        image::load_from_memory(&png).map(|i| (i.width() as f32 * 0.75, i.height() as f32 * 0.75))
    else {
        st.toast(ToastKind::Info, t(st.lang(), "toast.clipboard_no_image"), "");
        return;
    };
    let page = st.current_page();
    let author = st.ui.read().author.clone();
    let r = st.with_active(|t| {
        let c = t.page_sizes[page].crop_box;
        let fit = ((c[2] - c[0]) * 0.6 / w).min((c[3] - c[1]) * 0.6 / h).min(1.0);
        let (w, h) = (w * fit, h * fit);
        let (cx, cy) = ((c[0] + c[2]) / 2.0, (c[1] + c[3]) / 2.0);
        let rect = [cx - w / 2.0, cy - h / 2.0, cx + w / 2.0, cy + h / 2.0];
        t.doc.add_image_stamp(page, rect, png, &author, "").map(|_| ())
    });
    core_result(st, r);
}

/// Paste Pages: the pages copied after the current page; without any, a
/// picture of the clipboard as a new page.
fn paste_pages_command(st: AppState) {
    if no_doc(st) {
        return;
    }
    match copied_pages(st) {
        Some(pdf) => paste_pages(st, &pdf, st.current_page() + 1, 1),
        None if crate::clipboard::get_png().is_some() => insert_page_from_clipboard(st),
        None => st.toast(ToastKind::Info, t(st.lang(), "toast.no_pages_copied"), ""),
    }
}

/// Paste Pages Special: where the pages copied go and how many times.
fn paste_pages_special(st: AppState) {
    if no_doc(st) {
        return;
    }
    if copied_pages(st).is_some() {
        st.set_dialog(Dialog::PastePages);
    } else {
        st.toast(ToastKind::Info, t(st.lang(), "toast.no_pages_copied"), "");
    }
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
            .map(|l| folitecta_core::fonts::text_width(l, 12.0))
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
    "pastePages" => paste_pages_command,
    "pasteAsStamp" => paste_as_stamp,
    "pastePagesSpecial" => paste_pages_special,
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
    "edit.deselect" => |st| { st.with_active(|t| { t.selected_pages.clear(); t.selected_annot = None; t.guide_sel.clear(); }); },
    "delete" => |st| {
        if no_doc(st) { return; }
        if crate::commands::clip_more::delete_guides(st) { return; }
        if crate::commands::content_items::delete_chosen(st) { return; }
        // Every chosen comment (a group, or those added with Shift) in one step.
        match st.read_active(|t| t.chosen_annots()).flatten() {
            Some((page, list)) if list.len() > 1 => core_result(st, st.with_active(|t| t.doc.delete_annotations(page, &list).map(|_| ()))),
            Some((page, list)) => core_result(st, st.with_active(|t| t.doc.delete_annotation(page, list[0]))),
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
