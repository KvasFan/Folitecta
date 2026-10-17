//! Pane commands: thumbnails, links, search options, and expanding or
//! collapsing the tree of the visible pane.

use super::{handlers, no_doc};
use crate::files;
use crate::i18n::{t, tf};
use crate::state::{AppState, DocTab, ToastKind};
use dioxus::prelude::ReadableExt;
use folitecta_core::{Link, LinkTarget, OutlineItem};

/// View › Expand All / Collapse All: the same command of the visible pane
/// (bookmarks, form fields or comments).
fn tree(st: AppState, expand: bool) {
    let pane = {
        let ui = st.ui.read();
        ui.show_left_pane.then(|| ui.left_pane.clone())
    };
    let area = match pane.as_deref() {
        Some("bookmarks") => "bookmarksView",
        Some("fields") => "fieldsView",
        Some("comments") => "commentsView",
        _ => {
            st.toast(ToastKind::Info, t(st.lang(), "toast.no_tree_pane"), "");
            return;
        }
    };
    let what = if expand { "expandAll" } else { "collapseAll" };
    super::dispatch(st, &format!("{area}.{what}"));
}

/// Keys of a bookmark (its path) and of the bookmarks inside it that have bookmarks inside.
fn expand_keys(items: &[OutlineItem], path: &mut Vec<usize>, out: &mut Vec<String>) {
    for (i, item) in items.iter().enumerate() {
        path.push(i);
        if !item.children.is_empty() {
            out.push(path.iter().map(|x| x.to_string()).collect::<Vec<_>>().join("."));
            expand_keys(&item.children, path, out);
        }
        path.pop();
    }
}

/// View › Expand All Descendants: the selected bookmark and everything in it.
fn expand_descendants(st: AppState) {
    let bookmarks = {
        let ui = st.ui.read();
        ui.show_left_pane && ui.left_pane == "bookmarks"
    };
    let selected = st.read_active(|t| t.selected_bookmark.clone()).flatten();
    let Some(path) = selected.filter(|_| bookmarks) else {
        tree(st, true);
        return;
    };
    st.with_active(|t| {
        let outline = t.doc.outline();
        let mut items = outline.as_slice();
        for &i in &path {
            match items.get(i) {
                Some(item) => items = &item.children,
                None => return,
            }
        }
        let mut keys = vec![path.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(".")];
        expand_keys(items, &mut path.clone(), &mut keys);
        t.expanded_bookmarks.extend(keys);
    });
}

/// The link selected in the links pane.
fn selected_link(st: AppState) -> Option<Link> {
    st.read_active(|t| {
        let (page, index) = t.selected_link?;
        t.doc.links(page).ok()?.into_iter().find(|l| l.index == index)
    })
    .flatten()
}

/// The web address of the selected link, or a message saying what is missing.
fn selected_address(st: AppState) -> Option<String> {
    match selected_link(st).map(|l| l.target) {
        Some(LinkTarget::Uri(u)) => Some(u),
        Some(_) => {
            st.toast(ToastKind::Info, t(st.lang(), "toast.select_web_link"), "");
            None
        }
        None => {
            st.toast(ToastKind::Info, t(st.lang(), "toast.select_link_first"), "");
            None
        }
    }
}

fn select_pages_where(st: AppState, pred: impl Fn(&DocTab, usize) -> bool) {
    if no_doc(st) {
        return;
    }
    st.with_active(|t| {
        let n = t.doc.page_count();
        t.selected_pages = (0..n).filter(|p| pred(t, *p)).collect();
    });
    let count = st.read_active(|t| t.selected_pages.len()).unwrap_or(0);
    st.toast(
        ToastKind::Info,
        tf(
            st.lang(),
            "toast.pages_selected",
            &[("count", &count.to_string())],
        ),
        "",
    );
}

fn has_comments(t: &DocTab, p: usize) -> bool {
    t.doc
        .annotations(p)
        .map(|a| {
            a.iter()
                .any(|x| !["Link", "Widget", "Popup"].contains(&x.subtype.as_str()))
        })
        .unwrap_or(false)
}

fn thumbs_zoom(st: AppState, factor: f32) {
    let current: f32 = st
        .ui
        .read()
        .value("thumb.size")
        .and_then(|v| v.parse().ok())
        .unwrap_or(160.0);
    st.set_value(
        "thumb.size",
        format!("{}", (current * factor).clamp(80.0, 320.0).round()),
    );
}

handlers! {
    "thumbnailsView.select.empty" => |st| { let empty = st.read_active(|t| t.doc.empty_pages()).unwrap_or_default(); select_pages_where(st, move |_, p| empty.contains(&p)) },
    "thumbnailsView.select.landscape" => |st| select_pages_where(st, |t, p| { let s = t.display_size(p); s.0 > s.1 }),
    "thumbnailsView.select.portrait" => |st| select_pages_where(st, |t, p| { let s = t.display_size(p); s.0 <= s.1 }),
    "thumbnailsView.select.comments" => |st| select_pages_where(st, has_comments),
    "thumbnailsView.select.links" => |st| select_pages_where(st, |t, p| t.doc.links(p).map(|l| !l.is_empty()).unwrap_or(false)),
    "thumbnailsView.select.images" => |st| select_pages_where(st, |t, p| t.doc.image_count(p) > 0),
    "thumbnailsView.select.text" => |st| select_pages_where(st, |t, p| !t.doc.page_text(p).trim().is_empty()),
    "thumbnailsView.select.fields" => |st| { let pages: Vec<usize> = st.read_active(|t| t.doc.form_fields().into_iter().filter_map(|f| f.page).collect()).unwrap_or_default(); select_pages_where(st, move |_, p| pages.contains(&p)) },
    "thumbnailsView.select.bookmarks" => |st| { let pages: Vec<usize> = st.read_active(|t| t.doc.outline_flat().into_iter().filter_map(|(_, _, p)| p).collect()).unwrap_or_default(); select_pages_where(st, move |_, p| pages.contains(&p)) },
    "thumbnailsView.select.linkTargets" => |st| { let pages: Vec<usize> = st.read_active(|t| t.doc.all_links().into_iter().filter_map(|l| match l.target { folitecta_core::LinkTarget::Page(p) => Some(p), _ => None }).collect()).unwrap_or_default(); select_pages_where(st, move |_, p| pages.contains(&p)) },
    "thumbnailsView.select.searchRes" => |st| { let pages: Vec<usize> = st.read_active(|t| t.search.hits.iter().map(|h| h.page).collect()).unwrap_or_default(); select_pages_where(st, move |_, p| pages.contains(&p)) },
    "thumbnailsView.zoomIn" => |st| thumbs_zoom(st, 1.25),
    "thumbnailsView.zoomOut" => |st| thumbs_zoom(st, 0.8),
    "selectPages" => |st| select_pages_where(st, |_, _| true),
    "searchView.results.clear" => |st| { st.with_active(|t| { t.search.hits.clear(); t.search.query.clear(); }); },
    "searchView.results.save" => |st| {
        if no_doc(st) { return; }
        let text = st.read_active(|t| t.search.hits.iter().map(|h| format!("Page {}: {}", h.page + 1, h.snippet)).collect::<Vec<_>>().join("\n")).unwrap_or_default();
        files::save_text_file(st, "search-results.txt", text);
    },
    "document.find.gotoNext" => |st| crate::ui::panes::step_search(st, true),
    "document.find.gotoPrev" => |st| crate::ui::panes::step_search(st, false),
    "expandAll" => |st| tree(st, true),
    "collapseAll" => |st| tree(st, false),
    "expandAllDescendants" => expand_descendants,
    "linksView.link.copyURL" => |st| {
        if let Some(u) = selected_address(st) {
            crate::clipboard::set_text(&u);
            st.toast(ToastKind::Ok, t(st.lang(), "toast.link_copied"), "");
        }
    },
    "linksView.link.openInBrowser" => |st| {
        if let Some(u) = selected_address(st) {
            if let Err(e) = crate::util::open_external(&u) { st.error(e); }
        }
    },
}
