//! Document menu: page organisation, transformation and page marks.

use super::{core_result, doc_dialog, doc_op, handlers, no_doc};
use crate::files;
use crate::i18n::{t, tf};
use crate::state::{AppState, Dialog, ToastKind};
use dioxus::prelude::ReadableExt;
use folitecta_core::Sides;

fn rotate(st: AppState, degrees: i32) {
    doc_op(st, |t| {
        let pages = t.target_pages();
        t.doc.rotate_pages(&pages, degrees)
    });
}

fn remove_marks(st: AppState, tag: &'static str) {
    if no_doc(st) {
        return;
    }
    let r = st.with_active(|t| {
        let n = t.doc.page_count();
        t.doc.remove_marks(&(0..n).collect::<Vec<_>>(), Some(tag))
    });
    match r {
        Some(Ok(n)) => {
            files::set_window_title(st);
            st.toast(
                ToastKind::Ok,
                tf(st.lang(), "toast.marks_removed", &[("count", &n.to_string())]),
                "",
            );
        }
        Some(Err(e)) => st.error(e),
        None => {}
    }
}

fn crop_white(st: AppState, sides: Sides) {
    if no_doc(st) {
        return;
    }
    let r = st.with_active(|t| {
        let pages = t.target_pages();
        t.doc.crop_to_white_margins(&pages, 6.0, sides)
    });
    match r {
        Some(Ok(n)) => {
            files::set_window_title(st);
            st.toast(
                ToastKind::Ok,
                tf(st.lang(), "toast.pages_cropped", &[("count", &n.to_string())]),
                "",
            );
        }
        Some(Err(e)) => st.error(e),
        None => {}
    }
}

fn find_duplicates(st: AppState) {
    if no_doc(st) {
        return;
    }
    match st.read_active(|t| t.doc.duplicate_page_groups()) {
        Some(Ok(groups)) if groups.is_empty() => {
            st.toast(ToastKind::Info, t(st.lang(), "toast.no_duplicates"), "")
        }
        Some(Ok(groups)) => {
            let dupes: Vec<usize> = groups.iter().flat_map(|g| g.iter().skip(1).copied()).collect();
            let count = dupes.len();
            st.with_active(|t| t.selected_pages = dupes);
            let detail = groups
                .iter()
                .map(|g| {
                    g.iter()
                        .map(|p| (p + 1).to_string())
                        .collect::<Vec<_>>()
                        .join(" = ")
                })
                .collect::<Vec<_>>()
                .join(", ");
            st.toast(
                ToastKind::Info,
                tf(
                    st.lang(),
                    "toast.duplicates_found",
                    &[("count", &count.to_string())],
                ),
                detail,
            );
        }
        Some(Err(e)) => st.error(e),
        None => {}
    }
}

fn normalize(st: AppState) {
    if no_doc(st) {
        return;
    }
    match st.with_active(|t| t.doc.normalize_page_sizes()) {
        Some(Ok(0)) => st.toast(ToastKind::Info, t(st.lang(), "toast.sizes_already_equal"), ""),
        Some(Ok(n)) => {
            files::set_window_title(st);
            st.toast(
                ToastKind::Ok,
                tf(st.lang(), "toast.pages_resized", &[("count", &n.to_string())]),
                "",
            );
        }
        Some(Err(e)) => st.error(e),
        None => {}
    }
}

fn new_doc_from_selection(st: AppState) {
    if no_doc(st) {
        return;
    }
    let part = st.read_active(|t| {
        let pages = t.target_pages();
        t.doc.extract_pages(&pages)
    });
    match part {
        Some(Ok(mut doc)) => {
            doc.clear_history();
            let id = st.alloc_id();
            st.docs_mut().push(crate::state::DocTab::new(id, doc));
            let n = st.docs.read().len();
            st.set_active(n - 1);
            files::set_window_title(st);
        }
        Some(Err(e)) => st.error(e),
        None => {}
    }
}

fn delete_empty(st: AppState) {
    if no_doc(st) {
        return;
    }
    let empty = st.read_active(|t| t.doc.empty_pages()).unwrap_or_default();
    if empty.is_empty() {
        st.toast(ToastKind::Info, t(st.lang(), "toast.no_empty_pages"), "");
        return;
    }
    let count = empty.len();
    let r = st.with_active(|t| t.doc.delete_pages(&empty));
    core_result(st, r);
    st.toast(
        ToastKind::Ok,
        tf(st.lang(), "toast.pages_deleted", &[("count", &count.to_string())]),
        "",
    );
}

handlers! {
    "document.insertPages" => |st| { if !no_doc(st) { files::pick_file_to_insert(st) } },
    "document.mergePages" => |st| { if !no_doc(st) { files::pick_file_to_insert(st) } },
    "document.insertEmptyPages" => |st| doc_dialog(st, Dialog::InsertBlank),
    "document.insertImages" => |st| { if !no_doc(st) { files::insert_image_pages(st) } },
    "document.deletePages" => |st| doc_dialog(st, Dialog::DeletePages),
    "document.deleteEmptyPages" => delete_empty,
    "document.extractPages" => |st| doc_dialog(st, Dialog::ExtractPages),
    "document.createNewDocFromSel" => new_doc_from_selection,
    "document.duplicatePages" => |st| doc_op(st, |t| { let p = t.target_pages(); t.doc.duplicate_pages(&p) }),
    "document.reversePageOrder" => |st| doc_op(st, |t| t.doc.reverse_pages()),
    "document.movePages" => |st| doc_dialog(st, Dialog::MovePages),
    "document.swapPages" => |st| doc_dialog(st, Dialog::SwapPages),
    "document.replacePages" => |st| { if !no_doc(st) { files::pick_file_for(st, files::PickPurpose::Replace) } },
    "document.overlayPages" => |st| { if !no_doc(st) { files::pick_file_for(st, files::PickPurpose::Overlay) } },
    "document.findDuplicates" => find_duplicates,
    "document.normalizePages" => normalize,
    "document.rotatePages" => |st| doc_dialog(st, Dialog::RotatePages),
    "document.rotatePages.cw" => |st| rotate(st, 90),
    "document.rotatePages.ccw" => |st| rotate(st, -90),
    "document.rotatePages.180" => |st| rotate(st, 180),
    "document.cropPages" => |st| doc_dialog(st, Dialog::CropPages),
    "tool.cropPage" => |st| doc_dialog(st, Dialog::CropPages),
    "document.cropToWhiteMargins" => |st| crop_white(st, Sides::ALL),
    "document.cropToWhiteMargins.left" => |st| crop_white(st, Sides { left: true, top: false, right: false, bottom: false }),
    "document.cropToWhiteMargins.right" => |st| crop_white(st, Sides { left: false, top: false, right: true, bottom: false }),
    "document.cropToWhiteMargins.top" => |st| crop_white(st, Sides { left: false, top: true, right: false, bottom: false }),
    "document.cropToWhiteMargins.bottom" => |st| crop_white(st, Sides { left: false, top: false, right: false, bottom: true }),
    "document.resizePages" => |st| doc_dialog(st, Dialog::ResizePages),
    "document.setMargins" => |st| doc_dialog(st, Dialog::AddMargins),
    "document.rasterizePages" => |st| doc_dialog(st, Dialog::Rasterize),
    "document.split" => |st| doc_dialog(st, Dialog::Split),
    "document.splitGroup" => |st| doc_dialog(st, Dialog::Split),
    "document.splitPages" => |st| doc_dialog(st, Dialog::SplitTiles),
    "document.watermarks" => |st| doc_dialog(st, Dialog::Watermark),
    "document.watermarks.add" => |st| doc_dialog(st, Dialog::Watermark),
    "document.watermarks.removeAll" => |st| remove_marks(st, "Watermark"),
    "document.background" => |st| doc_dialog(st, Dialog::Background),
    "document.background.add" => |st| doc_dialog(st, Dialog::Background),
    "document.background.update" => |st| doc_dialog(st, Dialog::Background),
    "document.background.remove" => |st| remove_marks(st, "Background"),
    "document.headerAndFooter" => |st| doc_dialog(st, Dialog::HeaderFooter),
    "document.headerAndFooter.add" => |st| doc_dialog(st, Dialog::HeaderFooter),
    "document.headerAndFooter.removeAll" => |st| remove_marks(st, "HeaderFooter"),
    "document.batesNumbering" => |st| doc_dialog(st, Dialog::Bates),
    "document.batesNumbering.add" => |st| doc_dialog(st, Dialog::Bates),
    "document.batesNumbering.addMulti" => |st| files::bates_multiple(st),
    "document.batesNumbering.removeAll" => |st| remove_marks(st, "PageNumber"),
    "document.pageNumbering" => |st| doc_dialog(st, Dialog::PageLabels),
    "macro.pageNumberingStyle.options" => |st| doc_dialog(st, Dialog::PageLabels),
    "document.pageTransitions" => |st| doc_dialog(st, Dialog::Transitions),
    "pagemarks.removeAll" => |st| doc_dialog(st, Dialog::RemoveMarks),
    "pagemarks.add" => |st| doc_dialog(st, Dialog::PageNumbers),
    "document.wordCount" => |st| { if !no_doc(st) { let (words, chars) = st.read_active(|t| t.doc.word_count()).unwrap_or((0, 0)); st.set_dialog(Dialog::WordCount { words, chars }); } },
    "document.exportBookmarks" => |st| {
        if no_doc(st) { return; }
        let text = st.read_active(|t| t.doc.outline_flat().into_iter().map(|(d, title, page)| format!("{}{}\t{}", "  ".repeat(d), title, page.map(|p| (p + 1).to_string()).unwrap_or_default())).collect::<Vec<_>>().join("\n")).unwrap_or_default();
        files::save_text_file(st, "bookmarks.txt", text);
    },
}
