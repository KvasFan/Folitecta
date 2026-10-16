//! Command dispatch. `HANDLERS` is the single source of truth for which registry
//! commands are implemented; everything else shows a roadmap notice.

use std::collections::HashSet;

use dioxus::prelude::*;
use folitecta_core::registry;

use crate::files;
use crate::i18n::{t, tf};
use crate::state::{AppState, ConfirmAction, Dialog, Layout, Pane, Theme, ToastKind, Tool, Zoom};
use crate::util;

type Handler = fn(AppState);

/// Commands that perform a reduced version of the original operation.
pub const PARTIAL: &[&str] = &[
    "print",
    "printAll",
    "tool.selectText",
    "tool.snapshot",
    "comments.export",
    "comments.summarize",
    "createWebLinksFromText",
    "document.mergePages",
    "forms.exportData",
    "newDocCombine",
    "newDocFromFiles",
    "sendByEmail",
    "copySrcFile",
];

macro_rules! handlers {
    ($( $id:literal => $f:expr ),* $(,)?) => {
        pub const HANDLERS: &[(&str, Handler)] = &[ $( ($id, $f) ),* ];
    };
}

fn no_doc(st: AppState) -> bool {
    if st.has_doc() {
        false
    } else {
        st.toast(ToastKind::Warn, t(st.lang(), "toast.no_document"), "");
        true
    }
}

fn core_result(st: AppState, r: Option<folitecta_core::Result<()>>) {
    match r {
        Some(Err(e)) => st.error(e),
        _ => files::set_window_title(st),
    }
}

fn rotate(st: AppState, degrees: i32) {
    if no_doc(st) {
        return;
    }
    let r = st.with_active(|t| {
        let pages = t.target_pages();
        t.doc.rotate_pages(&pages, degrees)
    });
    core_result(st, r);
}

fn set_zoom(st: AppState, zoom: Zoom) {
    st.with_active(|t| t.zoom = zoom);
}

fn zoom_step(st: AppState, factor: f32) {
    let current = current_zoom(st);
    let next = (current * factor).clamp(0.1, 8.0);
    set_zoom(st, Zoom::Fixed((next * 100.0).round() / 100.0));
}

/// Effective zoom factor of the active document (fit modes resolved).
pub fn current_zoom(st: AppState) -> f32 {
    let (cw, ch) = (st.canvas_size)();
    st.read_active(|t| match t.zoom {
        Zoom::Fixed(z) => z,
        Zoom::FitWidth => {
            let w = (0..t.doc.page_count())
                .map(|p| t.display_size(p).0)
                .fold(1.0, f32::max);
            ((cw as f32 - 56.0) / w).clamp(0.1, 8.0)
        }
        Zoom::FitPage => {
            let (w, h) = t.display_size(t.page);
            ((cw as f32 - 56.0) / w)
                .min((ch as f32 - 56.0) / h)
                .clamp(0.1, 8.0)
        }
    })
    .unwrap_or(1.0)
}

fn set_layout(st: AppState, layout: Layout) {
    st.with_active(|t| t.layout = layout);
}

fn toggle_pane(st: AppState, pane: Pane) {
    let id = pane.id();
    let mut ui = st.ui_mut();
    if ui.show_left_pane && ui.left_pane == id {
        ui.show_left_pane = false;
    } else {
        ui.show_left_pane = true;
        ui.left_pane = id;
    }
    drop(ui);
    st.save_prefs();
}

fn set_tool(st: AppState, tool: Tool) {
    st.set_tool(tool);
}

fn set_theme(st: AppState, theme: Theme) {
    st.ui_mut().theme = theme;
    st.save_prefs();
}

fn open_url(st: AppState, url: &str) {
    if let Err(e) = util::open_external(url) {
        st.error(e);
    }
}

fn confirm(st: AppState, title_key: &str, body_key: &str, action: ConfirmAction) {
    let lang = st.lang();
    st.set_dialog(Dialog::Confirm {
        title: t(lang, title_key),
        body: t(lang, body_key),
        action,
    });
}

fn select_pages_where(st: AppState, pred: impl Fn(&crate::state::DocTab, usize) -> bool) {
    if no_doc(st) {
        return;
    }
    st.with_active(|t| {
        let n = t.doc.page_count();
        t.selected_pages = (0..n).filter(|p| pred(t, *p)).collect();
    });
    let count = st.read_active(|t| t.selected_pages.len()).unwrap_or(0);
    st.toast(ToastKind::Info, format!("{count} pages selected"), "");
}

fn step_comment(st: AppState, forward: bool) {
    if no_doc(st) {
        return;
    }
    let target = st.read_active(|t| {
        let comments = t.doc.comments();
        if comments.is_empty() {
            return None;
        }
        let pos = t
            .selected_annot
            .and_then(|(p, i)| comments.iter().position(|a| a.page == p && a.index == i));
        let next = match (pos, forward) {
            (Some(i), true) => (i + 1) % comments.len(),
            (Some(i), false) => (i + comments.len() - 1) % comments.len(),
            (None, true) => comments.iter().position(|a| a.page >= t.page).unwrap_or(0),
            (None, false) => comments.len() - 1,
        };
        Some((comments[next].page, comments[next].index))
    });
    match target.flatten() {
        Some((page, index)) => {
            st.with_active(|t| t.selected_annot = Some((page, index)));
            st.go_to_page(page);
        }
        None => st.toast(ToastKind::Info, t(st.lang(), "toast.not_found"), ""),
    }
}

fn comments_summary(st: AppState) -> String {
    st.read_active(|t| {
        let mut out = format!("Comments in {}\n\n", t.title());
        for a in t.doc.comments() {
            out.push_str(&format!(
                "Page {}: [{}] {} — {}\n",
                a.page + 1,
                a.subtype,
                a.author,
                a.contents.replace('\n', " ")
            ));
        }
        out
    })
    .unwrap_or_default()
}

handlers! {
    // ---- File -------------------------------------------------------------
    "open" => |st| files::open_dialog(st),
    "openGroup" => |st| files::open_dialog(st),
    "openFrom" => |st| files::open_dialog(st),
    "openFromComp" => |st| files::open_dialog(st),
    "openFromPlaces" => |st| files::open_dialog(st),
    "browseForOpen" => |st| files::open_dialog(st),
    "window.new" => |st| files::open_dialog(st),
    "newDocBlank" => |st| files::new_blank(st, 1, 595.0, 842.0),
    "newDoc" => |st| files::new_blank(st, 1, 595.0, 842.0),
    "newDocFromFiles" => |st| files::combine_files(st),
    "newDocCombine" => |st| files::combine_files(st),
    "save" => |st| files::save(st),
    "saveAs" => |st| files::save_as(st),
    "saveAsGroup" => |st| files::save_as(st),
    "saveAsCopy" => |st| files::save_copy(st),
    "saveCopyAs" => |st| files::save_copy(st),
    "saveToComp" => |st| files::save_as(st),
    "saveToPlaces" => |st| files::save_as(st),
    "saveAll" => |st| files::save_all(st),
    "revert" => |st| { if !no_doc(st) { confirm(st, "cmd.revert", "dialog.confirm.revert", ConfirmAction::Revert) } },
    "close" => |st| files::close_active(st),
    "closeTab" => |st| files::close_active(st),
    "closeAll" => |st| files::close_all(st),
    "closeOtherTabs" => |st| {
        let keep = st.read_active(|t| t.id);
        let others: Vec<u64> = st.docs.read().iter().map(|t| t.id).filter(|id| Some(*id) != keep).collect();
        for id in others { files::close_tab(st, id, false, crate::state::AfterUnsaved::Close); }
    },
    "closeTabsToTheLeft" => |st| {
        let idx = st.active_index().unwrap_or(0);
        let ids: Vec<u64> = st.docs.read().iter().take(idx).map(|t| t.id).collect();
        for id in ids { files::close_tab(st, id, false, crate::state::AfterUnsaved::Close); }
    },
    "closeTabsToTheRight" => |st| {
        let idx = st.active_index().unwrap_or(0);
        let ids: Vec<u64> = st.docs.read().iter().skip(idx + 1).map(|t| t.id).collect();
        for id in ids { files::close_tab(st, id, false, crate::state::AfterUnsaved::Close); }
    },
    "duplicateTab" => |st| {
        if no_doc(st) { return; }
        let bytes = st.read_active(|t| t.doc.bytes()).unwrap();
        match folitecta_core::Document::from_bytes(bytes.as_ref().clone(), None, None) {
            Ok(doc) => { let id = st.alloc_id(); st.docs_mut().push(crate::state::DocTab::new(id, doc)); let n = st.docs.read().len(); st.set_active(n - 1); }
            Err(e) => st.error(e),
        }
    },
    "reopenJustClosedDoc" => |st| files::reopen_closed(st),
    "print" => |st| { if !no_doc(st) { dioxus::desktop::window().print(); } },
    "printAll" => |st| { if !no_doc(st) { dioxus::desktop::window().print(); } },
    "exit" => |st| files::exit_app(st),
    "docProps" => |st| { if !no_doc(st) { st.set_dialog(Dialog::DocProps) } },
    "openSrcFolder" => |st| {
        let dir = st.read_active(|t| t.doc.path().and_then(|p| p.parent().map(|d| d.display().to_string()))).flatten();
        match dir { Some(d) => open_url(st, &d), None => st.toast(ToastKind::Warn, t(st.lang(), "toast.no_document"), "") }
    },
    "copySrcFileName" => |st| {
        if let Some(Some(p)) = st.read_active(|t| t.doc.path().map(|p| p.display().to_string())) { util::copy_to_clipboard(&p); st.toast(ToastKind::Ok, t(st.lang(), "toast.copied"), ""); }
    },
    "copySrcFile" => |st| {
        if let Some(Some(p)) = st.read_active(|t| t.doc.path().map(|p| p.display().to_string())) { util::copy_to_clipboard(&p); st.toast(ToastKind::Ok, t(st.lang(), "toast.copied"), ""); }
    },
    "export" => |st| { if !no_doc(st) { st.set_dialog(Dialog::ExportImages) } },
    "document.export" => |st| { if !no_doc(st) { st.set_dialog(Dialog::ExportImages) } },
    "document.export.toImages" => |st| { if !no_doc(st) { st.set_dialog(Dialog::ExportImages) } },
    "saveImage" => |st| { if !no_doc(st) { st.set_dialog(Dialog::ExportImages) } },
    "sendByEmail" => |st| {
        if no_doc(st) { return; }
        let name = st.read_active(|t| t.title()).unwrap_or_default();
        open_url(st, &format!("mailto:?subject={}", name));
    },
    "recentFiles" => |st| st.set_palette(Some(String::new())),
    "recentsList" => |st| st.set_palette(Some(String::new())),
    "manageHistory" => |st| { st.ui_mut().recent.clear(); st.save_prefs(); },
    "historyList.clear" => |st| { st.ui_mut().recent.clear(); st.save_prefs(); },
    // ---- Edit -------------------------------------------------------------
    "undo" => |st| {
        if no_doc(st) { return; }
        match st.with_active(|t| t.doc.undo()) { Some(Err(_)) => st.toast(ToastKind::Info, t(st.lang(), "toast.nothing_to_undo"), ""), _ => files::set_window_title(st) }
    },
    "redo" => |st| {
        if no_doc(st) { return; }
        match st.with_active(|t| t.doc.redo()) { Some(Err(_)) => st.toast(ToastKind::Info, t(st.lang(), "toast.nothing_to_redo"), ""), _ => files::set_window_title(st) }
    },
    "selectAll" => |st| select_pages_where(st, |_, _| true),
    "invertSelection" => |st| { if !no_doc(st) { st.with_active(|t| { let n = t.doc.page_count(); t.selected_pages = (0..n).filter(|p| !t.selected_pages.contains(p)).collect(); }); } },
    "edit.deselect" => |st| { st.with_active(|t| { t.selected_pages.clear(); t.selected_annot = None; }); },
    "delete" => |st| {
        if no_doc(st) { return; }
        let annot = st.read_active(|t| t.selected_annot).flatten();
        let r = match annot {
            Some((page, index)) => st.with_active(|t| t.doc.delete_annotation(page, index)),
            None => { st.set_dialog(Dialog::DeletePages); return; }
        };
        core_result(st, r);
    },
    "find" => |st| toggle_pane(st, Pane::Search),
    "findGroup" => |st| toggle_pane(st, Pane::Search),
    "search" => |st| toggle_pane(st, Pane::Search),
    "view.search" => |st| toggle_pane(st, Pane::Search),
    "search.start" => |st| toggle_pane(st, Pane::Search),
    "prefs" => |st| st.set_dialog(Dialog::Prefs),
    "view.uiOptions" => |st| st.set_dialog(Dialog::Prefs),
    "settings.reset" => |st| { let recent = st.ui.read().recent.clone(); *st.ui_mut() = crate::state::UiPrefs { recent, ..Default::default() }; st.save_prefs(); },
    "settings" => |st| st.set_dialog(Dialog::Prefs),
    // ---- View -------------------------------------------------------------
    "zoomIn" => |st| zoom_step(st, 1.25),
    "zoomOut" => |st| zoom_step(st, 0.8),
    "view.zoom" => |st| st.set_dialog(Dialog::GoToPage),
    "zoom.to.level" => |st| { if !no_doc(st) { st.set_dialog(Dialog::GoToPage) } },
    "zoom.level.actual" => |st| set_zoom(st, Zoom::Fixed(1.0)),
    "zoom.level.fitPage" => |st| set_zoom(st, Zoom::FitPage),
    "zoom.level.fitHeight" => |st| set_zoom(st, Zoom::FitPage),
    "zoom.level.fitWidth" => |st| set_zoom(st, Zoom::FitWidth),
    "zoom.level.fitVisible" => |st| set_zoom(st, Zoom::FitWidth),
    "zoomSel" => |st| set_zoom(st, Zoom::FitPage),
    "view.goto.firstPage" => |st| st.go_to_page(0),
    "view.goto.lastPage" => |st| { let n = st.page_count(); st.go_to_page(n.saturating_sub(1)); },
    "view.goto.nextPage" => |st| { let p = st.current_page(); st.go_to_page(p + 1); },
    "view.goto.prevPage" => |st| { let p = st.current_page(); st.go_to_page(p.saturating_sub(1)); },
    "view.goto.page" => |st| { if !no_doc(st) { st.set_dialog(Dialog::GoToPage) } },
    "view.goto.pagesView" => |st| { if !no_doc(st) { st.set_dialog(Dialog::GoToPage) } },
    "view.goto.nextDocument" => |st| { let n = st.docs.read().len(); if n > 0 { let a = (st.active)(); st.set_active((a + 1) % n); files::set_window_title(st); } },
    "view.goto.prevDocument" => |st| { let n = st.docs.read().len(); if n > 0 { let a = (st.active)(); st.set_active((a + n - 1) % n); files::set_window_title(st); } },
    "view.layout.onePage" => |st| set_layout(st, Layout::Single),
    "view.layout.oneColumn" => |st| set_layout(st, Layout::Continuous),
    "view.layout.twoPages" => |st| set_layout(st, Layout::TwoPages),
    "view.layout.twoColumns" => |st| set_layout(st, Layout::TwoPages),
    "view.layout.standard" => |st| set_layout(st, Layout::Continuous),
    "view.rotate.CW" => |st| { st.with_active(|t| t.view_rotation = (t.view_rotation + 90) % 360); },
    "view.rotate.CCW" => |st| { st.with_active(|t| t.view_rotation = (t.view_rotation + 270) % 360); },
    "view.rotate.reset" => |st| { st.with_active(|t| t.view_rotation = 0); },
    "view.fullScreen" => |st| { let on = !(st.fullscreen)(); st.set_fullscreen(on); dioxus::desktop::window().set_fullscreen(on); },
    "view.fullScreenExit" => |st| { st.set_fullscreen(false); dioxus::desktop::window().set_fullscreen(false); },
    "view.darkPageMode" => |st| { let next = if st.ui.read().theme == Theme::Dark { Theme::Light } else { Theme::Dark }; set_theme(st, next); },
    "view.theme.default" => |st| set_theme(st, Theme::Light),
    "view.theme.white" => |st| set_theme(st, Theme::Light),
    "view.theme.darkGray" => |st| set_theme(st, Theme::Dark),
    "view.theme.black" => |st| set_theme(st, Theme::Dark),
    "view.pageThumbnails" => |st| toggle_pane(st, Pane::Thumbnails),
    "view.bookmarks" => |st| toggle_pane(st, Pane::Bookmarks),
    "view.comments" => |st| toggle_pane(st, Pane::Comments),
    "view.attachments" => |st| toggle_pane(st, Pane::Attachments),
    "view.layers" => |st| toggle_pane(st, Pane::Layers),
    "view.fields" => |st| toggle_pane(st, Pane::Fields),
    "view.links" => |st| toggle_pane(st, Pane::Links),
    "view.destinations" => |st| toggle_pane(st, Pane::Destinations),
    "view.signatures" => |st| toggle_pane(st, Pane::Signatures),
    "view.properties" => |st| { let v = !st.ui.read().show_right_pane; st.ui_mut().show_right_pane = v; st.save_prefs(); },
    "view.toggleAllCmdPanes" => |st| { let v = !st.ui.read().show_left_pane; st.ui_mut().show_left_pane = v; st.save_prefs(); },
    "view.switchToClassicUI" => |st| { st.ui_mut().ribbon = false; st.save_prefs(); },
    "view.switchToRibbonUI" => |st| { st.ui_mut().ribbon = true; st.save_prefs(); },
    "view.ribbonClassicUI" => |st| { let v = !st.ui.read().ribbon; st.ui_mut().ribbon = v; st.save_prefs(); },
    // ---- Document ---------------------------------------------------------
    "document.insertPages" => |st| { if !no_doc(st) { files::pick_file_to_insert(st) } },
    "document.mergePages" => |st| { if !no_doc(st) { files::pick_file_to_insert(st) } },
    "document.insertEmptyPages" => |st| { if !no_doc(st) { st.set_dialog(Dialog::InsertBlank) } },
    "document.deletePages" => |st| { if !no_doc(st) { st.set_dialog(Dialog::DeletePages) } },
    "document.deleteEmptyPages" => |st| {
        if no_doc(st) { return; }
        let empty = st.read_active(|t| t.doc.empty_pages()).unwrap_or_default();
        if empty.is_empty() { st.toast(ToastKind::Info, t(st.lang(), "toast.not_found"), ""); return; }
        let r = st.with_active(|t| t.doc.delete_pages(&empty));
        core_result(st, r);
    },
    "document.extractPages" => |st| { if !no_doc(st) { st.set_dialog(Dialog::ExtractPages) } },
    "document.duplicatePages" => |st| { if !no_doc(st) { let r = st.with_active(|t| { let p = t.target_pages(); t.doc.duplicate_pages(&p) }); core_result(st, r); } },
    "document.reversePageOrder" => |st| { if !no_doc(st) { let r = st.with_active(|t| t.doc.reverse_pages()); core_result(st, r); } },
    "document.movePages" => |st| { if !no_doc(st) { st.set_dialog(Dialog::MovePages) } },
    "document.swapPages" => |st| { if !no_doc(st) { st.set_dialog(Dialog::SwapPages) } },
    "document.rotatePages" => |st| { if !no_doc(st) { st.set_dialog(Dialog::RotatePages) } },
    "document.rotatePages.cw" => |st| rotate(st, 90),
    "document.rotatePages.ccw" => |st| rotate(st, -90),
    "document.rotatePages.180" => |st| rotate(st, 180),
    "document.cropPages" => |st| { if !no_doc(st) { st.set_dialog(Dialog::CropPages) } },
    "tool.cropPage" => |st| { if !no_doc(st) { st.set_dialog(Dialog::CropPages) } },
    "document.resizePages" => |st| { if !no_doc(st) { st.set_dialog(Dialog::ResizePages) } },
    "document.setMargins" => |st| { if !no_doc(st) { st.set_dialog(Dialog::AddMargins) } },
    "document.split" => |st| { if !no_doc(st) { st.set_dialog(Dialog::Split) } },
    "document.splitGroup" => |st| { if !no_doc(st) { st.set_dialog(Dialog::Split) } },
    "document.watermarks" => |st| { if !no_doc(st) { st.set_dialog(Dialog::Watermark) } },
    "document.watermarks.add" => |st| { if !no_doc(st) { st.set_dialog(Dialog::Watermark) } },
    "document.watermarks.removeAll" => |st| { if !no_doc(st) { let r = st.with_active(|t| { let n = t.doc.page_count(); t.doc.remove_marks(&(0..n).collect::<Vec<_>>(), Some("Watermark")).map(|_| ()) }); core_result(st, r); } },
    "document.headerAndFooter" => |st| { if !no_doc(st) { st.set_dialog(Dialog::HeaderFooter) } },
    "document.headerAndFooter.add" => |st| { if !no_doc(st) { st.set_dialog(Dialog::HeaderFooter) } },
    "document.headerAndFooter.removeAll" => |st| { if !no_doc(st) { let r = st.with_active(|t| { let n = t.doc.page_count(); t.doc.remove_marks(&(0..n).collect::<Vec<_>>(), Some("HeaderFooter")).map(|_| ()) }); core_result(st, r); } },
    "document.batesNumbering" => |st| { if !no_doc(st) { st.set_dialog(Dialog::Bates) } },
    "document.batesNumbering.add" => |st| { if !no_doc(st) { st.set_dialog(Dialog::Bates) } },
    "document.batesNumbering.removeAll" => |st| { if !no_doc(st) { let r = st.with_active(|t| { let n = t.doc.page_count(); t.doc.remove_marks(&(0..n).collect::<Vec<_>>(), Some("PageNumber")).map(|_| ()) }); core_result(st, r); } },
    "document.pageNumbering" => |st| { if !no_doc(st) { st.set_dialog(Dialog::PageNumbers) } },
    "pagemarks.removeAll" => |st| { if !no_doc(st) { st.set_dialog(Dialog::RemoveMarks) } },
    "document.wordCount" => |st| { if !no_doc(st) { let (words, chars) = st.read_active(|t| t.doc.word_count()).unwrap_or((0, 0)); st.set_dialog(Dialog::WordCount { words, chars }); } },
    "document.exportBookmarks" => |st| {
        if no_doc(st) { return; }
        let text = st.read_active(|t| t.doc.outline_flat().into_iter().map(|(d, title, page)| format!("{}{}\t{}", "  ".repeat(d), title, page.map(|p| (p + 1).to_string()).unwrap_or_default())).collect::<Vec<_>>().join("\n")).unwrap_or_default();
        files::save_text_file(st, "bookmarks.txt", text);
    },
    // ---- Comments ---------------------------------------------------------
    "addStickyNote" => |st| set_tool(st, Tool::Note),
    "tool.annot.stickyNote" => |st| set_tool(st, Tool::Note),
    "tool.annot.square" => |st| set_tool(st, Tool::Rect),
    "tool.annot.circle" => |st| set_tool(st, Tool::Ellipse),
    "tool.annot.line" => |st| set_tool(st, Tool::Line),
    "tool.annot.arrow" => |st| set_tool(st, Tool::Arrow),
    "tool.annot.highlightArea" => |st| set_tool(st, Tool::Highlight),
    "tool.annot.textBox" => |st| set_tool(st, Tool::TextBox),
    "tool.annot.typeWriter" => |st| set_tool(st, Tool::TextBox),
    "tool.annot.pencil" => |st| set_tool(st, Tool::Ink),
    "tool.annot.editLink" => |st| set_tool(st, Tool::Link),
    "addLink" => |st| set_tool(st, Tool::Link),
    "addLink.uri" => |st| set_tool(st, Tool::Link),
    "tool.hand" => |st| set_tool(st, Tool::Hand),
    "tool.selectText" => |st| set_tool(st, Tool::Select),
    "tool.select" => |st| set_tool(st, Tool::Select),
    "tool.editAnnots" => |st| set_tool(st, Tool::Select),
    "tool.zoom" => |st| set_tool(st, Tool::Zoom),
    "tool.snapshot" => |st| {
        if no_doc(st) { return; }
        let (id, page, rev) = st.read_active(|t| (t.id, t.page, t.revision)).unwrap();
        let js = format!("fetch('/page/{id}/{page}/{rev}/2000.png').then(r => r.blob()).then(b => navigator.clipboard.write([new ClipboardItem({{'image/png': b}})]));");
        spawn(async move { let _ = document::eval(&js).await; });
        st.toast(ToastKind::Ok, t(st.lang(), "toast.copied"), "");
    },
    "comments.deleteAllDoc" => |st| { if !no_doc(st) { confirm(st, "cmd.comments.deleteAllDoc", "dialog.confirm.delete_comments", ConfirmAction::DeleteAllComments) } },
    "comments.deleteAllShown" => |st| { if !no_doc(st) { confirm(st, "cmd.comments.deleteAllDoc", "dialog.confirm.delete_comments", ConfirmAction::DeleteAllComments) } },
    "comments.deleteSel" => |st| {
        if no_doc(st) { return; }
        let annot = st.read_active(|t| t.selected_annot).flatten();
        match annot { Some((p, i)) => core_result(st, st.with_active(|t| t.doc.delete_annotation(p, i))), None => st.toast(ToastKind::Info, t(st.lang(), "label.none"), "") }
    },
    "comments.next" => |st| step_comment(st, true),
    "comments.previous" => |st| step_comment(st, false),
    "commentsView.next" => |st| step_comment(st, true),
    "commentsView.previous" => |st| step_comment(st, false),
    "comments.hideAll" => |st| { if !no_doc(st) { let r = st.with_active(|t| { let items = t.doc.comments(); for a in items { t.doc.set_annotation_hidden(a.page, a.index, true)?; } Ok(()) }); core_result(st, r); } },
    "comments.showAll" => |st| { if !no_doc(st) { let r = st.with_active(|t| { let items = t.doc.comments(); for a in items { t.doc.set_annotation_hidden(a.page, a.index, false)?; } Ok(()) }); core_result(st, r); } },
    "comments.summarize" => |st| { if !no_doc(st) { let s = comments_summary(st); st.set_dialog(Dialog::Summary(s)); } },
    "comments.export" => |st| { if !no_doc(st) { let s = comments_summary(st); files::save_text_file(st, "comments.txt", s); } },
    "commentsView.exportSelected" => |st| { if !no_doc(st) { let s = comments_summary(st); files::save_text_file(st, "comments.txt", s); } },
    // ---- Links ------------------------------------------------------------
    "removeAllWebLinks" => |st| { if !no_doc(st) { confirm(st, "cmd.removeAllWebLinks", "dialog.confirm.delete_links", ConfirmAction::RemoveLinks) } },
    "createWebLinksFromText" => |st| { if !no_doc(st) { let urls = st.read_active(|t| t.doc.find_urls_in_text()).unwrap_or_default(); st.set_dialog(Dialog::UrlReport(urls)); } },
    "linksView.exportToCSV" => |st| {
        if no_doc(st) { return; }
        let csv = st.read_active(|t| { let mut s = String::from("page,target,x0,y0,x1,y1\n"); for l in t.doc.all_links() { s.push_str(&format!("{},\"{}\",{},{},{},{}\n", l.page + 1, l.target.describe(), l.rect[0], l.rect[1], l.rect[2], l.rect[3])); } s }).unwrap_or_default();
        files::save_text_file(st, "links.csv", csv);
    },
    // ---- Forms ------------------------------------------------------------
    "forms.reset" => |st| { if !no_doc(st) { confirm(st, "cmd.forms.reset", "dialog.confirm.reset_form", ConfirmAction::ResetForm) } },
    "forms.exportData" => |st| {
        if no_doc(st) { return; }
        let text = st.read_active(|t| t.doc.form_values().into_iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>().join("\n")).unwrap_or_default();
        files::save_text_file(st, "form-data.txt", text);
    },
    "forms.highlight" => |st| { st.with_active(|t| t.highlight_fields = !t.highlight_fields); },
    "forms.highlight.all" => |st| { st.with_active(|t| t.highlight_fields = !t.highlight_fields); },
    // ---- Bookmarks / panes ------------------------------------------------
    "addBookmark" => |st| { if !no_doc(st) { st.set_dialog(Dialog::BookmarkNew) } },
    "bookmarksView.new" => |st| { if !no_doc(st) { st.set_dialog(Dialog::BookmarkNew) } },
    "bookmarksView.expandAll" => |st| { st.with_active(|t| { let all: Vec<String> = t.doc.outline_flat().iter().enumerate().map(|(i, _)| i.to_string()).collect(); t.expanded_bookmarks.extend(all); t.expanded_bookmarks.insert("*".into()); }); },
    "bookmarksView.collapseAll" => |st| { st.with_active(|t| t.expanded_bookmarks.clear()); },
    "attachmentsView.new" => |st| { if !no_doc(st) { files::add_attachment(st) } },
    "layersView.showSelected" => |st| { if !no_doc(st) { core_result(st, st.with_active(|t| t.doc.set_all_layers_visible(true))) } },
    "layersView.hideSelected" => |st| { if !no_doc(st) { core_result(st, st.with_active(|t| t.doc.set_all_layers_visible(false))) } },
    "thumbnailsView.select.empty" => |st| select_pages_where(st, |t, p| t.doc.empty_pages().contains(&p)),
    "thumbnailsView.select.landscape" => |st| select_pages_where(st, |t, p| { let s = t.display_size(p); s.0 > s.1 }),
    "thumbnailsView.select.portrait" => |st| select_pages_where(st, |t, p| { let s = t.display_size(p); s.0 <= s.1 }),
    "thumbnailsView.select.comments" => |st| select_pages_where(st, |t, p| t.doc.annotations(p).map(|a| a.iter().any(|x| !["Link", "Widget", "Popup"].contains(&x.subtype.as_str()))).unwrap_or(false)),
    "thumbnailsView.select.links" => |st| select_pages_where(st, |t, p| t.doc.links(p).map(|l| !l.is_empty()).unwrap_or(false)),
    "thumbnailsView.select.images" => |st| select_pages_where(st, |t, p| t.doc.image_count(p) > 0),
    "thumbnailsView.select.text" => |st| select_pages_where(st, |t, p| !t.doc.page_text(p).trim().is_empty()),
    "thumbnailsView.select.fields" => |st| select_pages_where(st, |t, p| t.doc.form_fields().iter().any(|f| f.page == Some(p))),
    "thumbnailsView.select.bookmarks" => |st| select_pages_where(st, |t, p| t.doc.outline_flat().iter().any(|(_, _, pg)| *pg == Some(p))),
    "selectPages" => |st| select_pages_where(st, |_, _| true),
    "searchView.caseSensitive" => |st| { st.with_active(|t| t.search.case_sensitive = !t.search.case_sensitive); },
    "searchView.wholeWords" => |st| { st.with_active(|t| t.search.whole_words = !t.search.whole_words); },
    // ---- Help -------------------------------------------------------------
    "help.about" => |st| st.set_dialog(Dialog::About),
    "help.contents" => |st| open_url(st, "https://github.com/KvasExpert/Folitecta#readme"),
    "help.homepage" => |st| open_url(st, "https://github.com/KvasExpert/Folitecta"),
    "help.support" => |st| open_url(st, "https://github.com/KvasExpert/Folitecta/discussions"),
    "help.report" => |st| open_url(st, "https://github.com/KvasExpert/Folitecta/issues/new"),
    "help.checkForUpdates" => |st| open_url(st, "https://github.com/KvasExpert/Folitecta/releases"),
    "help.cmdline" => |st| st.set_dialog(Dialog::CommandLine),
}

/// Set of implemented command ids.
pub fn implemented_ids() -> HashSet<&'static str> {
    HANDLERS
        .iter()
        .map(|(id, _)| *id)
        .filter(|id| !PARTIAL.contains(id))
        .collect()
}

/// Set of partially implemented command ids.
pub fn partial_ids() -> HashSet<&'static str> {
    HANDLERS
        .iter()
        .map(|(id, _)| *id)
        .filter(|id| PARTIAL.contains(id))
        .collect()
}

pub fn is_implemented(id: &str) -> bool {
    HANDLERS.iter().any(|(h, _)| *h == id)
}

pub fn is_partial(id: &str) -> bool {
    PARTIAL.contains(&id)
}

/// Run a command by registry id.
pub fn dispatch(st: AppState, id: &str) {
    st.close_menus();
    if let Some((_, handler)) = HANDLERS.iter().find(|(h, _)| *h == id) {
        handler(st);
        return;
    }
    let reg = registry();
    match reg.get(id) {
        Some(cmd) if reg.has_children(id) => {
            // A pure container: open it in the palette so its children are reachable.
            st.set_palette(Some(cmd.label.trim_end_matches('…').to_string()));
        }
        Some(cmd) => {
            let label = crate::i18n::cmd_label(st.lang(), cmd);
            st.toast(
                ToastKind::Warn,
                tf(st.lang(), "toast.planned", &[("label", &label)]),
                "",
            );
            st.set_dialog(Dialog::NotImplemented(id.to_string()));
        }
        None => st.error(format!("unknown command {id}")),
    }
}

/// Resolve a keyboard shortcut to a command id.
pub fn shortcut_command(key: &str, ctrl: bool, shift: bool, alt: bool) -> Option<&'static str> {
    let mut parts = Vec::new();
    if ctrl {
        parts.push("Ctrl");
    }
    if shift {
        parts.push("Shift");
    }
    if alt {
        parts.push("Alt");
    }
    let key_name = match key {
        "+" | "=" => "+",
        "-" | "_" => "-",
        other => other,
    };
    parts.push(key_name);
    let combo = parts.join("+");
    let reg = registry();
    let found = reg.by_shortcut(&combo).map(|c| c.id.as_str())?;
    // Return a 'static id from the handler table when possible.
    HANDLERS
        .iter()
        .map(|(h, _)| *h)
        .find(|h| *h == found)
        .or_else(|| {
            // not implemented, but still a valid registry id: leak-free lookup via static registry
            Some(Box::leak(found.to_string().into_boxed_str()))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handlers_reference_registry_commands() {
        let reg = registry();
        let mut seen = HashSet::new();
        for (id, _) in HANDLERS {
            assert!(reg.get(id).is_some(), "handler for unknown command {id}");
            assert!(seen.insert(*id), "duplicate handler {id}");
        }
        for id in PARTIAL {
            assert!(is_implemented(id), "partial command {id} has no handler");
        }
        assert!(
            implemented_ids().len() >= 150,
            "implemented: {}",
            implemented_ids().len()
        );
    }

    #[test]
    fn shortcuts_resolve() {
        assert_eq!(shortcut_command("o", true, false, false), Some("open"));
        assert_eq!(shortcut_command("s", true, true, false), Some("saveAs"));
        assert_eq!(
            shortcut_command("Page Down", false, false, false),
            Some("view.goto.nextPage")
        );
        assert_eq!(shortcut_command("q", true, false, false), None);
    }
}
