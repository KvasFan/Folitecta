//! Pane commands: bookmarks, attachments, layers, thumbnails and search options.

use super::{handlers, no_doc};
use crate::files;
use crate::i18n::tf;
use crate::state::{AppState, DocTab, ToastKind};
use dioxus::prelude::ReadableExt;

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
    "searchView.caseSensitive" => |st| { st.with_active(|t| t.search.case_sensitive = !t.search.case_sensitive); },
    "searchView.wholeWords" => |st| { st.with_active(|t| t.search.whole_words = !t.search.whole_words); },
    "document.find.caseSensitive" => |st| { st.with_active(|t| t.search.case_sensitive = !t.search.case_sensitive); },
    "document.find.wholeWords" => |st| { st.with_active(|t| t.search.whole_words = !t.search.whole_words); },
    "searchView.results.clear" => |st| { st.with_active(|t| { t.search.hits.clear(); t.search.query.clear(); }); },
    "searchView.results.save" => |st| {
        if no_doc(st) { return; }
        let text = st.read_active(|t| t.search.hits.iter().map(|h| format!("Page {}: {}", h.page + 1, h.snippet)).collect::<Vec<_>>().join("\n")).unwrap_or_default();
        files::save_text_file(st, "search-results.txt", text);
    },
    "document.find.gotoNext" => |st| crate::ui::panes::step_search(st, true),
    "document.find.gotoPrev" => |st| crate::ui::panes::step_search(st, false),
}
