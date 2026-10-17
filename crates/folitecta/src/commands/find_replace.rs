//! Find and Replace: the occurrences of a text in the text of the pages,
//! found one by one, replaced one at a time or all at once (one undo step).
//! The occurrences are the results of the search pane, so they are shown
//! and stepped through the same way.

use super::{handlers, no_doc, textedit};
use crate::files;
use crate::i18n::{t, tf};
use crate::state::{AppState, Dialog, TextSel, ToastKind};
use crate::ui::panes::reveal_hit;

/// Open the dialog, with the selected text (or the last search) to find.
fn open(st: AppState) {
    if no_doc(st) {
        return;
    }
    let find = st
        .read_active(|t| {
            let selected = t.text_sel.filter(|s| !s.is_empty()).map(|s| {
                let (a, b) = s.range();
                t.doc
                    .page_text(s.page)
                    .chars()
                    .skip(a)
                    .take(b - a)
                    .collect::<String>()
            });
            selected.unwrap_or_else(|| t.search.query.clone())
        })
        .unwrap_or_default();
    st.set_dialog(Dialog::FindReplace { find });
}

/// Find `query` as a phrase in the text of the pages: the results of the
/// search pane. Returns how many there are.
fn find_all(st: AppState, query: &str) -> usize {
    let (opts, _, _) = super::search::search_settings(st);
    st.with_active(|t| {
        t.search.query = query.to_string();
        t.search.hits = t.doc.search(query, opts);
        t.search.other.clear();
        t.search.current = 0;
        t.search.collapsed.clear();
        t.search.hits.len()
    })
    .unwrap_or(0)
}

/// Show occurrence `i` and select its text.
fn select_hit(st: AppState, i: usize) {
    reveal_hit(st, i);
    st.with_active(|t| {
        if let Some(h) = t.search.hits.get(i) {
            t.text_sel = Some(TextSel {
                page: h.page,
                anchor: h.start,
                head: h.end,
            });
        }
    });
}

/// The next occurrence of `query`: the first one from the current page when
/// the search is new, else the one after the current. False when there is none.
pub fn find_next(st: AppState, query: &str) -> bool {
    if query.is_empty() {
        return false;
    }
    let shown = st
        .read_active(|t| {
            (t.search.query == query && !t.search.hits.is_empty())
                .then_some((t.search.current, t.search.hits.len()))
        })
        .flatten();
    let next = match shown {
        Some((cur, n)) => Some((cur + 1) % n),
        None => {
            let n = find_all(st, query);
            let page = st.current_page();
            st.read_active(|t| t.search.hits.iter().position(|h| h.page >= page))
                .flatten()
                .or((n > 0).then_some(0))
        }
    };
    match next {
        Some(i) => {
            select_hit(st, i);
            true
        }
        None => {
            st.toast(ToastKind::Info, t(st.lang(), "toast.not_found"), "");
            false
        }
    }
}

/// Replace the occurrence shown (when its text is still the selection), then
/// show the next one; without one, find the next occurrence first.
pub fn replace_one(st: AppState, query: &str, with: &str) {
    if query.is_empty() {
        return;
    }
    let current = st
        .read_active(|t| {
            if t.search.query != query {
                return None;
            }
            let h = t.search.hits.get(t.search.current)?;
            let sel = t.text_sel?;
            (sel.page == h.page && sel.range() == (h.start, h.end)).then_some((h.page, h.start, h.end))
        })
        .flatten();
    let Some((page, start, end)) = current else {
        find_next(st, query);
        return;
    };
    if !textedit::edit_text(st, page, start, end, with) {
        return;
    }
    // The next occurrence after the new text (a replacement that contains
    // what was found is not found again).
    find_all(st, query);
    let after = (page, start + with.chars().count());
    let next = st
        .read_active(|t| {
            t.search
                .hits
                .iter()
                .position(|h| (h.page, h.start) >= after)
                .or((!t.search.hits.is_empty()).then_some(0))
        })
        .flatten();
    match next {
        Some(i) => select_hit(st, i),
        None => {
            st.with_active(|t| t.text_sel = None);
            st.toast(ToastKind::Info, t(st.lang(), "toast.no_more_found"), "");
        }
    }
}

/// Replace every occurrence (of the current page only, with `page_only`).
pub fn replace_all(st: AppState, query: &str, with: &str, page_only: bool) {
    if no_doc(st) || query.is_empty() {
        return;
    }
    let (opts, _, _) = super::search::search_settings(st);
    let pages = page_only.then(|| vec![st.current_page()]);
    let r = st.with_active(|t| t.doc.replace_page_text(query, with, opts, pages.as_deref()));
    match r {
        Some(Ok(rep)) => {
            files::set_window_title(st);
            find_all(st, query);
            st.with_active(|t| t.text_sel = None);
            let lang = st.lang();
            let count = rep.replaced.to_string();
            if rep.refused == 0 {
                st.toast(
                    ToastKind::Ok,
                    tf(lang, "toast.replaced", &[("count", &count)]),
                    "",
                );
            } else {
                let left = rep.refused.to_string();
                st.toast(
                    ToastKind::Warn,
                    tf(lang, "toast.replaced_some", &[("count", &count), ("left", &left)]),
                    rep.reason.unwrap_or_default(),
                );
            }
            if !rep.substituted.is_empty() {
                st.toast(
                    ToastKind::Info,
                    tf(lang, "toast.text_substituted", &[("chars", &rep.substituted)]),
                    "",
                );
            }
        }
        Some(Err(e)) => st.error(e),
        None => {}
    }
}

handlers! {
    "findAndRepl" => open,
}
