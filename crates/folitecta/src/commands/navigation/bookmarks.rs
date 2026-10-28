//! Bookmarks and named destinations: creation from the current view, editing,
//! moving, sections and their pages, import and export.

use dioxus::prelude::*;
use folitecta_core::outline::item_at;
use folitecta_core::{DestView, OutlineItem};

use crate::commands::{core_result, current_zoom, handlers, no_doc, option_on, show_pane};
use crate::files;
use crate::i18n::{t, tf};
use crate::state::{AppState, Dialog, Pane, ToastKind};

/// Where a new bookmark goes relative to the selected one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NewAt {
    After,
    Before,
    FirstChild,
    LastChild,
}

/// The selected bookmark, when it still exists.
pub(crate) fn selected_path(st: AppState) -> Option<Vec<usize>> {
    st.read_active(|t| {
        let p = t.selected_bookmark.clone()?;
        item_at(&t.doc.outline(), &p).map(|_| p)
    })
    .flatten()
}

fn selected_item(st: AppState) -> Option<(Vec<usize>, OutlineItem)> {
    st.read_active(|t| {
        let p = t.selected_bookmark.clone()?;
        let item = item_at(&t.doc.outline(), &p)?.clone();
        Some((p, item))
    })
    .flatten()
}

fn with_selected(st: AppState, f: impl FnOnce(AppState, Vec<usize>)) {
    if no_doc(st) {
        return;
    }
    match selected_path(st) {
        Some(p) => f(st, p),
        None => st.toast(ToastKind::Info, t(st.lang(), "toast.select_bookmark_first"), ""),
    }
}

fn select(st: AppState, path: Option<Vec<usize>>) {
    st.with_active(|t| {
        if let Some(p) = &path {
            // Make sure the parents are expanded so the bookmark shows.
            for depth in 1..p.len() {
                let key = p[..depth]
                    .iter()
                    .map(|x| x.to_string())
                    .collect::<Vec<_>>()
                    .join(".");
                t.expanded_bookmarks.insert(key);
            }
        }
        t.selected_bookmark = path;
    });
}

/// Page and view of the window: the top of the visible part of the current
/// page, with the current zoom unless the zoom is inherited.
pub(crate) async fn current_view(st: AppState) -> (usize, DestView) {
    let page = st.current_page();
    let zoom = current_zoom(st);
    let js = format!(
        "const el = document.getElementById('pg-{page}'); const c = document.getElementById('canvas'); if (el && c) {{ return c.getBoundingClientRect().top - el.getBoundingClientRect().top; }} return 0;"
    );
    let offset = document::eval(&js)
        .await
        .ok()
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0)
        .max(0.0) as f32
        / zoom.max(0.01);
    let size = st.read_active(|t| t.page_sizes.get(page).copied()).flatten();
    let top = size.map(|s| folitecta_core::marks::display_to_pdf(&s, 0.0, s.height - offset.min(s.height)).1);
    let inherited = option_on(st, "bookmarksView.useInheritedZoom");
    (
        page,
        DestView::Xyz {
            left: None,
            top,
            zoom: if inherited { None } else { Some(zoom) },
        },
    )
}

/// Destination from the text selection: its area, or the top of its first line.
fn selection_view(st: AppState) -> Option<(usize, DestView)> {
    st.read_active(|t| {
        let s = t.text_sel.filter(|s| !s.is_empty())?;
        let (a, b) = s.range();
        let rects = t.doc.text_layers().get(s.page)?.range_rects(a, b);
        let size = t.page_sizes.get(s.page).copied()?;
        let r = rects
            .iter()
            .fold([f32::MAX, f32::MAX, f32::MIN, f32::MIN], |acc, r| {
                [
                    acc[0].min(r[0]),
                    acc[1].min(r[1]),
                    acc[2].max(r[2]),
                    acc[3].max(r[3]),
                ]
            });
        let pdf = folitecta_core::textops::display_rect_to_pdf(&size, r);
        Some((s.page, DestView::Rect(pdf)))
    })
    .flatten()
}

fn selected_text(st: AppState) -> Option<String> {
    st.read_active(|t| {
        let s = t.text_sel.filter(|s| !s.is_empty())?;
        let (a, b) = s.range();
        let text = t.doc.text_range(s.page, a, b);
        let line = text.split_whitespace().collect::<Vec<_>>().join(" ");
        (!line.is_empty()).then(|| line.chars().take(120).collect())
    })
    .flatten()
}

/// Create a bookmark for the current view and open its name for editing.
pub(crate) fn new_bookmark(st: AppState, at: NewAt) {
    if no_doc(st) {
        return;
    }
    spawn(async move {
        let (mut page, mut view) = current_view(st).await;
        if option_on(st, "bookmarksView.useRegionForDest") {
            if let Some((p, v)) = selection_view(st) {
                page = p;
                view = v;
            }
        }
        let title = if option_on(st, "bookmarksView.useTextForTitle") {
            selected_text(st)
        } else {
            None
        }
        .unwrap_or_else(|| {
            tf(
                st.lang(),
                "bookmark.default_title",
                &[("page", &(page + 1).to_string())],
            )
        });
        let path = st
            .read_active(|t| {
                let items = t.doc.outline();
                let sel = t
                    .selected_bookmark
                    .clone()
                    .filter(|p| item_at(&items, p).is_some());
                match (at, sel) {
                    (NewAt::After, Some(mut p)) => {
                        *p.last_mut().expect("path") += 1;
                        p
                    }
                    (NewAt::Before, Some(p)) => p,
                    (NewAt::FirstChild, Some(mut p)) => {
                        p.push(0);
                        p
                    }
                    (NewAt::LastChild, Some(mut p)) => {
                        let n = item_at(&items, &p).map(|i| i.children.len()).unwrap_or(0);
                        p.push(n);
                        p
                    }
                    (_, None) => vec![items.len()],
                }
            })
            .unwrap_or_else(|| vec![0]);
        match st.with_active(|t| t.doc.insert_bookmark(&path, &title, page, Some(view))) {
            Some(Ok(p)) => {
                files::set_window_title(st);
                show_pane(st, Pane::Bookmarks);
                select(st, Some(p.clone()));
                // Without the current page, the user chooses the target.
                if option_on(st, "bookmarksView.useCurrentPage") {
                    st.set_dialog(Dialog::BookmarkRename(p));
                } else {
                    st.set_dialog(Dialog::BookmarkProps(p));
                }
            }
            Some(Err(e)) => st.error(e),
            None => {}
        }
    });
}

/// Run a structural change that returns the bookmark's new path.
fn reorder(
    st: AppState,
    f: impl FnOnce(&mut folitecta_core::Document, &[usize]) -> folitecta_core::Result<Vec<usize>>,
) {
    with_selected(st, |st, p| match st.with_active(|t| f(&mut t.doc, &p)) {
        Some(Ok(new)) => {
            files::set_window_title(st);
            select(st, Some(new));
        }
        Some(Err(e)) => st.toast(ToastKind::Info, e.to_string(), ""),
        None => {}
    });
}

fn delete_selected(st: AppState, mode: u8) {
    with_selected(st, |st, p| {
        let r = st.with_active(|t| match mode {
            1 => t.doc.delete_bookmark_children(&p),
            2 => t.doc.delete_bookmark_keep_children(&p),
            _ => t.doc.delete_bookmark(&p),
        });
        if mode == 1 {
            select(st, Some(p));
        } else {
            select(st, None);
        }
        core_result(st, r);
    });
}

/// Show the destination of the selected bookmark.
pub(crate) fn go_to_selected(st: AppState) {
    with_selected(st, |st, p| {
        if let Some((_, item)) = selected_item(st) {
            go_to_item(st, &item);
        } else {
            let _ = p;
        }
    });
}

/// Follow a bookmark: a page destination, or its web address.
pub(crate) fn go_to_item(st: AppState, item: &OutlineItem) {
    match item.page {
        Some(page) => st.go_to_dest(page, item.view.clone()),
        None if item.action.starts_with("http") || item.action.starts_with("mailto:") => {
            crate::commands::open_url(st, &item.action.clone());
        }
        None => st.toast(ToastKind::Warn, t(st.lang(), "toast.bookmark_invalid"), ""),
    }
}

fn set_dest_to_view(st: AppState) {
    with_selected(st, |st, p| {
        spawn(async move {
            let (page, view) = current_view(st).await;
            let r = st.with_active(|t| t.doc.set_bookmark_target(&p, page, Some(view)));
            select(st, Some(p));
            core_result(st, r);
        });
    });
}

fn set_dest_from_selection(st: AppState) {
    with_selected(st, |st, p| match selection_view(st) {
        Some((page, view)) => {
            let r = st.with_active(|t| t.doc.set_bookmark_target(&p, page, Some(view)));
            select(st, Some(p));
            core_result(st, r);
        }
        None => st.toast(ToastKind::Info, t(st.lang(), "toast.select_text_first"), ""),
    });
}

fn toggle_style(st: AppState, bold: bool) {
    with_selected(st, |st, p| {
        if let Some((_, item)) = selected_item(st) {
            let (b, i) = if bold {
                (!item.bold, item.italic)
            } else {
                (item.bold, !item.italic)
            };
            let r = st.with_active(|t| t.doc.set_bookmark_style(&p, item.color, b, i));
            select(st, Some(p));
            core_result(st, r);
        }
    });
}

/// Check state of bookmark commands (menus and buttons).
pub fn bookmark_state(st: AppState, id: &str) -> Option<bool> {
    match id {
        "bookmarks.bold" => Some(selected_item(st).map(|(_, i)| i.bold).unwrap_or(false)),
        "bookmarks.italic" => Some(selected_item(st).map(|(_, i)| i.italic).unwrap_or(false)),
        "bookmarksView.textSize.small" | "bookmarksView.textSize.medium" | "bookmarksView.textSize.large" => {
            let size = st
                .ui
                .read()
                .value("bookmarks.text_size")
                .unwrap_or("medium")
                .to_string();
            Some(id.ends_with(&size))
        }
        _ => None,
    }
}

/// Unique name of a new named destination.
fn unique_dest_name(st: AppState, base: &str) -> String {
    let names: Vec<String> = st
        .read_active(|t| t.doc.named_destinations().into_iter().map(|d| d.name).collect())
        .unwrap_or_default();
    let base = if base.trim().is_empty() {
        "Destination".to_string()
    } else {
        base.trim().to_string()
    };
    if !names.contains(&base) {
        return base;
    }
    (2..)
        .map(|n| format!("{base} {n}"))
        .find(|n| !names.contains(n))
        .expect("a free name")
}

fn add_dest_from_bookmark(st: AppState) {
    with_selected(st, |st, _| {
        let Some((_, item)) = selected_item(st) else {
            return;
        };
        let Some(page) = item.page else {
            st.toast(ToastKind::Warn, t(st.lang(), "toast.bookmark_invalid"), "");
            return;
        };
        let name = unique_dest_name(st, &item.title);
        let r = st.with_active(|t| t.doc.add_named_destination(&name, page, item.view.clone()));
        match r {
            Some(Ok(())) => {
                files::set_window_title(st);
                st.toast(
                    ToastKind::Ok,
                    tf(st.lang(), "toast.dest_created", &[("name", &name)]),
                    "",
                );
            }
            Some(Err(e)) => st.error(e),
            None => {}
        }
    });
}

fn invalid(st: AppState, next: bool) {
    if no_doc(st) {
        return;
    }
    let invalid = st.read_active(|t| t.doc.invalid_bookmarks()).unwrap_or_default();
    if invalid.is_empty() {
        st.toast(ToastKind::Ok, t(st.lang(), "toast.no_invalid_bookmarks"), "");
        return;
    }
    st.set_option("bookmarksView.markInvalid", true);
    show_pane(st, Pane::Bookmarks);
    let target = if next {
        let cur = selected_path(st);
        let paths = st.read_active(|t| t.doc.bookmark_paths()).unwrap_or_default();
        let pos = cur.and_then(|c| paths.iter().position(|p| *p == c));
        invalid
            .iter()
            .find(|p| {
                pos.map(|i| paths.iter().position(|x| x == *p).unwrap_or(0) > i)
                    .unwrap_or(true)
            })
            .or(invalid.first())
            .cloned()
    } else {
        invalid.first().cloned()
    };
    select(st, target);
    st.toast(
        ToastKind::Info,
        tf(
            st.lang(),
            "toast.invalid_bookmarks",
            &[("count", &invalid.len().to_string())],
        ),
        "",
    );
}

/// Pages of the selected bookmark: its target page, or its whole section.
fn bookmark_pages(st: AppState, section: bool) -> Option<Vec<usize>> {
    let (p, item) = selected_item(st)?;
    if section {
        let pages = st
            .read_active(|t| t.doc.bookmark_section_pages(&p))
            .unwrap_or_default();
        (!pages.is_empty()).then_some(pages)
    } else {
        item.page.map(|pg| vec![pg])
    }
}

fn pages_action(st: AppState, section: bool, action: &'static str) {
    if no_doc(st) {
        return;
    }
    let Some(pages) = bookmark_pages(st, section) else {
        st.toast(ToastKind::Info, t(st.lang(), "toast.select_bookmark_first"), "");
        return;
    };
    match action {
        "select" => {
            let n = pages.len();
            st.with_active(|t| t.selected_pages = pages.clone());
            st.go_to_page(pages[0]);
            st.toast(
                ToastKind::Info,
                tf(st.lang(), "toast.pages_selected", &[("count", &n.to_string())]),
                "",
            );
        }
        "extract" => match st.read_active(|t| t.doc.extract_pages(&pages)) {
            Some(Ok(doc)) => files::open_document_tab(st, doc),
            Some(Err(e)) => st.error(e),
            None => {}
        },
        "print" => st.set_dialog(Dialog::Print(crate::state::PrintTarget::Pages(pages.clone()))),
        _ => {
            let lang = st.lang();
            let body = tf(
                lang,
                "dialog.confirm.remove_pages",
                &[("count", &pages.len().to_string())],
            );
            crate::ui::dialogs::delete_pages_asking(
                st,
                pages,
                Some((t(lang, "cmd.bookmarksView.pages.remove"), body)),
            );
        }
    }
}

fn export_bookmarks(st: AppState, selected: bool) {
    if no_doc(st) {
        return;
    }
    let only = if selected {
        match selected_path(st) {
            Some(p) => Some(vec![p]),
            None => {
                st.toast(ToastKind::Info, t(st.lang(), "toast.select_bookmark_first"), "");
                return;
            }
        }
    } else {
        None
    };
    let text = st
        .read_active(|t| t.doc.export_bookmarks_text(only.as_deref()))
        .unwrap_or_default();
    if text.is_empty() {
        st.toast(ToastKind::Info, t(st.lang(), "toast.no_bookmarks"), "");
        return;
    }
    files::save_text_file(st, "bookmarks.txt", text);
}

/// Add bookmarks from indented text.
pub(crate) fn import_bookmarks_text(st: AppState, text: &str) {
    match st.with_active(|t| t.doc.import_bookmarks_text(text)) {
        Some(Ok(n)) => {
            files::set_window_title(st);
            show_pane(st, Pane::Bookmarks);
            st.toast(
                ToastKind::Ok,
                tf(
                    st.lang(),
                    "toast.bookmarks_imported",
                    &[("count", &n.to_string())],
                ),
                "",
            );
        }
        Some(Err(e)) => st.error(e),
        None => {}
    }
}

fn import_bookmarks(st: AppState) {
    if no_doc(st) {
        return;
    }
    spawn(async move {
        let Some(file) = rfd::AsyncFileDialog::new()
            .set_title("Import bookmarks")
            .add_filter("Text", &["txt"])
            .add_filter("All files", &["*"])
            .pick_file()
            .await
        else {
            return;
        };
        let data = file.read().await;
        import_bookmarks_text(st, String::from_utf8_lossy(&data).trim_start_matches('\u{feff}'));
    });
}

fn paste_bookmarks(st: AppState) {
    if no_doc(st) {
        return;
    }
    match crate::clipboard::get_text() {
        Some(text) if !text.trim().is_empty() => import_bookmarks_text(st, &text),
        _ => st.toast(ToastKind::Info, t(st.lang(), "toast.clipboard_empty"), ""),
    }
}

// ---- Named destinations -----------------------------------------------------------

/// The selected named destination, when it still exists.
pub(crate) fn selected_dest(st: AppState) -> Option<folitecta_core::NamedDestination> {
    st.read_active(|t| {
        let name = t.selected_dest.clone()?;
        t.doc.named_destinations().into_iter().find(|d| d.name == name)
    })
    .flatten()
}

fn with_selected_dest(st: AppState, f: impl FnOnce(AppState, folitecta_core::NamedDestination)) {
    if no_doc(st) {
        return;
    }
    match selected_dest(st) {
        Some(d) => f(st, d),
        None => st.toast(ToastKind::Info, t(st.lang(), "toast.select_dest_first"), ""),
    }
}

fn new_dest(st: AppState) {
    if no_doc(st) {
        return;
    }
    spawn(async move {
        let (mut page, mut view) = current_view(st).await;
        if option_on(st, "pdfNamedDestsView.useRegionForDest") {
            if let Some((p, v)) = selection_view(st) {
                page = p;
                view = v;
            }
        }
        let base = if option_on(st, "pdfNamedDestsView.useTextForTitle") {
            selected_text(st)
        } else {
            None
        }
        .unwrap_or_else(|| {
            tf(
                st.lang(),
                "bookmark.default_title",
                &[("page", &(page + 1).to_string())],
            )
        });
        let name = unique_dest_name(st, &base);
        match st.with_active(|t| t.doc.add_named_destination(&name, page, Some(view))) {
            Some(Ok(())) => {
                files::set_window_title(st);
                show_pane(st, Pane::Destinations);
                st.with_active(|t| t.selected_dest = Some(name.clone()));
                st.set_dialog(Dialog::NamedDestEdit(name));
            }
            Some(Err(e)) => st.error(e),
            None => {}
        }
    });
}

fn set_dest(st: AppState, from_selection: bool) {
    with_selected_dest(st, |st, d| {
        if from_selection {
            match selection_view(st) {
                Some((page, view)) => core_result(
                    st,
                    st.with_active(|t| t.doc.set_named_destination(&d.name, page, Some(view))),
                ),
                None => st.toast(ToastKind::Info, t(st.lang(), "toast.select_text_first"), ""),
            }
        } else {
            spawn(async move {
                let (page, view) = current_view(st).await;
                core_result(
                    st,
                    st.with_active(|t| t.doc.set_named_destination(&d.name, page, Some(view))),
                );
            });
        }
    });
}

fn remove_invalid_dests(st: AppState) {
    if no_doc(st) {
        return;
    }
    match st.with_active(|t| t.doc.remove_invalid_named_destinations()) {
        Some(Ok(n)) => {
            files::set_window_title(st);
            st.toast(
                ToastKind::Ok,
                tf(st.lang(), "toast.dests_removed", &[("count", &n.to_string())]),
                "",
            );
        }
        Some(Err(e)) => st.error(e),
        None => {}
    }
}

handlers! {
    "addBookmark" => |st| new_bookmark(st, NewAt::After),
    "snapshot.addBookmark" => |st| new_bookmark(st, NewAt::After),
    "bookmarksView.new" => |st| new_bookmark(st, NewAt::After),
    "bookmarksView.new.before" => |st| new_bookmark(st, NewAt::Before),
    "bookmarksView.new.firstChild" => |st| new_bookmark(st, NewAt::FirstChild),
    "bookmarksView.new.lastChild" => |st| new_bookmark(st, NewAt::LastChild),
    "bookmarksView.expandAll" => |st| { st.with_active(|t| { t.expanded_bookmarks.insert("*".into()); }); },
    "bookmarksView.collapseAll" => |st| { st.with_active(|t| t.expanded_bookmarks.clear()); },
    "bookmarksView.deleteOnlyChildren" => |st| delete_selected(st, 1),
    "bookmarksView.deleteWithoutChildren" => |st| delete_selected(st, 2),
    "bookmarksView.moveUp" => |st| reorder(st, |d, p| d.move_bookmark_up(p)),
    "bookmarksView.moveDown" => |st| reorder(st, |d, p| d.move_bookmark_down(p)),
    "bookmarksView.promote" => |st| reorder(st, |d, p| d.promote_bookmark(p)),
    "bookmarksView.demote" => |st| reorder(st, |d, p| d.demote_bookmark(p)),
    "bookmarksView.goto" => go_to_selected,
    "highlight.destination" => go_to_selected,
    "bookmarksView.setDest" => set_dest_to_view,
    "bookmarksView.setDestFromSel" => set_dest_from_selection,
    "bookmarksView.addDest" => add_dest_from_bookmark,
    "bookmarksView.allInvalid" => |st| invalid(st, false),
    "bookmarksView.nextInvalid" => |st| invalid(st, true),
    "bookmarksView.pages.select" => |st| pages_action(st, false, "select"),
    "bookmarksView.pages.extract" => |st| pages_action(st, false, "extract"),
    "bookmarksView.pages.print" => |st| pages_action(st, false, "print"),
    "bookmarksView.pages.remove" => |st| pages_action(st, false, "remove"),
    "bookmarksView.sections.select" => |st| pages_action(st, true, "select"),
    "bookmarksView.sections.extract" => |st| pages_action(st, true, "extract"),
    "bookmarksView.sections.print" => |st| pages_action(st, true, "print"),
    "bookmarksView.sections.remove" => |st| pages_action(st, true, "remove"),
    "bookmarksView.select.entireSection" => |st| pages_action(st, true, "select"),
    "bookmarksView.select.targetPages" => |st| pages_action(st, false, "select"),
    "bookmarksView.select.targetPagesE" => |st| pages_action(st, true, "select"),
    "bookmarks.bold" => |st| toggle_style(st, true),
    "bookmarks.italic" => |st| toggle_style(st, false),
    "bookmarksView.textSize.small" => |st| st.set_value("bookmarks.text_size", "small"),
    "bookmarksView.textSize.medium" => |st| st.set_value("bookmarks.text_size", "medium"),
    "bookmarksView.textSize.large" => |st| st.set_value("bookmarks.text_size", "large"),
    "bookmarksView.createOptions" => |st| st.set_dialog(Dialog::BookmarkOptions),
    "document.bookmarksFromHeadings" => |st| { if !no_doc(st) { st.set_dialog(Dialog::BookmarksFromHeadings) } },
    "document.insertContents" => |st| { if !no_doc(st) { st.set_dialog(Dialog::InsertContents) } },
    "bookmarksView.options" => |st| st.set_dialog(Dialog::BookmarkOptions),
    "bookmarksView.prefs" => |st| st.set_dialog(Dialog::BookmarkOptions),
    "bookmarksView.viewOptions" => |st| st.set_dialog(Dialog::BookmarkOptions),
    "bookmarksView.importExport" => |st| { if !no_doc(st) { st.set_dialog(Dialog::BookmarkImportExport) } },
    "document.exportBookmarks" => |st| export_bookmarks(st, false),
    "document.exportSelBookmarks" => |st| export_bookmarks(st, true),
    "document.importBookmarks" => import_bookmarks,
    "pasteBookmarks" => paste_bookmarks,
    "pdfNamedDestsView.new" => new_dest,
    "pdfNamedDestsView.edit" => |st| with_selected_dest(st, |st, d| st.set_dialog(Dialog::NamedDestEdit(d.name))),
    "pdfNamedDestsView.goto" => |st| with_selected_dest(st, |st, d| { if let Some(p) = d.page { st.go_to_dest(p, d.view) } }),
    "pdfNamedDestsView.set" => |st| set_dest(st, false),
    "pdfNamedDestsView.setFromSel" => |st| set_dest(st, true),
    "pdfNamedDestsView.removeInvalid" => remove_invalid_dests,
    "pdfNamedDestsView.options" => |st| st.set_dialog(Dialog::BookmarkOptions),
    "pdfNamedDestsView.createOptions" => |st| st.set_dialog(Dialog::BookmarkOptions),
}
