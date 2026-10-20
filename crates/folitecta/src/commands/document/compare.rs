//! Comparing two open documents: running the comparison, moving between the
//! differences, what the pages show, and a summary document.

use dioxus::prelude::*;
use folitecta_core::Document;
use folitecta_core::compare::DiffKind;

use crate::commands::{handlers, no_doc};
use crate::i18n::{t, tf};
use crate::state::{AppState, CompareState, Dialog, Pane, ToastKind};

fn need_comparison(st: AppState) -> Option<CompareState> {
    let c = st.comparison.peek().clone();
    if c.is_none() {
        st.toast(ToastKind::Info, t(st.lang(), "toast.compare_first"), "");
        st.set_dialog(Dialog::Compare);
    }
    c
}

fn tab_index(st: AppState, id: u64) -> Option<usize> {
    st.docs.peek().iter().position(|t| t.id == id)
}

/// Compare the document of tab `old` with that of tab `new`.
pub fn run(st: AppState, old: u64, new: u64, ignore_case: bool) {
    let docs = st.docs.peek();
    let (Some(a), Some(b)) = (
        docs.iter().find(|t| t.id == old),
        docs.iter().find(|t| t.id == new),
    ) else {
        return;
    };
    let (ba, bb) = (a.doc.bytes(), b.doc.bytes());
    let (na, nb) = (a.doc.display_name(), b.doc.display_name());
    let protection = a.doc.protection().or(b.doc.protection()).cloned();
    drop(docs);
    st.set_busy(true);
    spawn(async move {
        let result = tokio::task::spawn_blocking(move || {
            let da = Document::from_bytes(ba.to_vec(), None, None)?;
            let db = Document::from_bytes(bb.to_vec(), None, None)?;
            Ok::<_, folitecta_core::Error>(da.compare_with(&db, ignore_case))
        })
        .await;
        st.set_busy(false);
        match result {
            Ok(Ok(result)) => {
                let n = result.differences.len();
                *st.comparison.write_unchecked() = Some(CompareState {
                    old_tab: old,
                    new_tab: new,
                    old_name: na,
                    new_name: nb,
                    result,
                    current: 0,
                    filter: "all".into(),
                    rows: false,
                    protection,
                });
                crate::commands::show_pane(st, Pane::Comparison);
                if let Some(i) = tab_index(st, new) {
                    st.set_active(i);
                }
                let msg = if n == 0 {
                    t(st.lang(), "toast.compare_same")
                } else {
                    tf(st.lang(), "toast.compare_done", &[("count", &n.to_string())])
                };
                st.toast(ToastKind::Ok, msg, "");
                if n > 0 {
                    goto(st, 0);
                }
            }
            Ok(Err(e)) => st.error(e),
            Err(e) => st.error(e),
        }
    });
}

/// Show difference `i`: the document where it is visible, at its page.
pub fn goto(st: AppState, i: usize) {
    let Some(c) = st.comparison.peek().clone() else {
        return;
    };
    let n = c.result.differences.len();
    if n == 0 {
        return;
    }
    let i = i.min(n - 1);
    let d = c.result.differences[i].clone();
    if let Some(cs) = st.comparison.write_unchecked().as_mut() {
        cs.current = i;
    }
    // Deletions are only visible in the old document.
    let (tab, page, rects) = if d.kind == DiffKind::Deleted {
        (c.old_tab, d.old_page, d.old.first().map(|s| s.rects.clone()))
    } else {
        (c.new_tab, d.new_page, d.new.first().map(|s| s.rects.clone()))
    };
    if let Some(idx) = tab_index(st, tab) {
        st.set_active(idx);
        st.navigate(page, true);
        if let Some(r) = rects.and_then(|r| r.first().copied()) {
            st.with_active(|t| t.flash = Some((page, r)));
        }
    }
}

fn step(st: AppState, delta: isize) {
    let Some(c) = need_comparison(st) else { return };
    let n = c.result.differences.len() as isize;
    if n == 0 {
        st.toast(ToastKind::Info, t(st.lang(), "toast.compare_same"), "");
        return;
    }
    let next = (c.current as isize + delta).rem_euclid(n) as usize;
    goto(st, next);
}

fn edge(st: AppState, last: bool) {
    let Some(c) = need_comparison(st) else { return };
    let n = c.result.differences.len();
    if n > 0 {
        goto(st, if last { n - 1 } else { 0 });
    }
}

fn set_filter(st: AppState, f: &str) {
    if need_comparison(st).is_none() {
        return;
    }
    if let Some(c) = st.comparison.write_unchecked().as_mut() {
        c.filter = f.into();
    }
    bump(st);
}

/// Redraw the pages of the compared documents.
fn bump(st: AppState) {
    for t in st.docs_mut().iter_mut() {
        t.bump_view();
    }
}

fn show_doc(st: AppState, old: bool) {
    let Some(c) = need_comparison(st) else { return };
    match tab_index(st, if old { c.old_tab } else { c.new_tab }) {
        Some(i) => st.set_active(i),
        None => st.toast(ToastKind::Info, t(st.lang(), "toast.compare_closed"), ""),
    }
}

/// Switch between the two documents.
fn show_both(st: AppState) {
    let Some(c) = need_comparison(st) else { return };
    let active = st.read_active(|t| t.id);
    let other = if active == Some(c.new_tab) {
        c.old_tab
    } else {
        c.new_tab
    };
    if let Some(i) = tab_index(st, other) {
        st.set_active(i);
    }
}

/// A new document listing every difference.
fn summary(st: AppState) {
    let Some(c) = need_comparison(st) else { return };
    let lang = st.lang();
    let mut text = format!(
        "{}\n\n{}\n{}\n\n{}\n\n",
        t(lang, "compare.report_title"),
        tf(
            lang,
            "compare.report_old",
            &[("name", &c.old_name), ("pages", &c.result.old_pages.to_string())]
        ),
        tf(
            lang,
            "compare.report_new",
            &[("name", &c.new_name), ("pages", &c.result.new_pages.to_string())]
        ),
        tf(
            lang,
            "compare.counts",
            &[
                ("changed", &c.result.count(DiffKind::Changed).to_string()),
                ("inserted", &c.result.count(DiffKind::Inserted).to_string()),
                ("deleted", &c.result.count(DiffKind::Deleted).to_string()),
            ],
        )
    );
    for (i, d) in c.result.differences.iter().enumerate() {
        let line = match d.kind {
            DiffKind::Changed => tf(
                lang,
                "compare.line_changed",
                &[("old", &d.old_text), ("new", &d.new_text)],
            ),
            DiffKind::Inserted => tf(lang, "compare.line_inserted", &[("new", &d.new_text)]),
            DiffKind::Deleted => tf(lang, "compare.line_deleted", &[("old", &d.old_text)]),
        };
        text.push_str(&format!(
            "{}. {} {} / {} {}: {line}\n",
            i + 1,
            t(lang, "status.page"),
            d.old_page + 1,
            t(lang, "status.page"),
            d.new_page + 1
        ));
    }
    match Document::from_text(&text, Default::default()) {
        Ok(mut doc) => {
            doc.set_name_hint(&format!("{}.pdf", t(lang, "compare.report_title")));
            // It quotes the documents: protected like them.
            if let Some(p) = &c.protection {
                doc.inherit_protection(p);
            }
            crate::files::open_document_tab(st, doc);
        }
        Err(e) => st.error(e),
    }
}

fn select_differing(st: AppState) {
    let Some(c) = need_comparison(st) else { return };
    let pages = c.result.new_pages_changed();
    if let Some(i) = tab_index(st, c.new_tab) {
        st.set_active(i);
        st.with_active(|t| t.selected_pages = pages.clone());
        crate::commands::show_pane(st, Pane::Thumbnails);
        st.toast(
            ToastKind::Info,
            tf(
                st.lang(),
                "toast.pages_selected",
                &[("count", &pages.len().to_string())],
            ),
            "",
        );
    }
}

/// Check states of the comparison options.
pub fn compare_state(st: AppState, id: &str) -> Option<bool> {
    let c = st.comparison.peek().clone();
    let filter = c.as_ref().map(|c| c.filter.clone()).unwrap_or_default();
    match id {
        "cmpDocs.filter.all" | "cmpDocs.show.allDiffs" => Some(filter == "all"),
        "cmpDocs.filter.list" => Some(filter == "list"),
        "cmpDocs.filter.none" => Some(filter == "none"),
        "cmpDocs.show.rowHighlights" => Some(c.as_ref().is_some_and(|c| c.rows)),
        _ => None,
    }
}

handlers! {
    "compareDocs" => |st| { if !no_doc(st) { st.set_dialog(Dialog::Compare) } },
    "cmpDocs.filter.all" => |st| set_filter(st, "all"),
    "cmpDocs.filter.list" => |st| set_filter(st, "list"),
    "cmpDocs.filter.none" => |st| set_filter(st, "none"),
    "cmpDocs.goto.nextDiff" => |st| step(st, 1),
    "cmpDocs.goto.prevDiff" => |st| step(st, -1),
    "cmpDocs.show.allDiffs" => |st| { set_filter(st, "all"); crate::commands::show_pane(st, Pane::Comparison); },
    "cmpDocs.show.both" => show_both,
    "cmpDocs.show.new" => |st| show_doc(st, false),
    "cmpDocs.show.old" => |st| show_doc(st, true),
    "cmpDocs.show.results" => |st| crate::commands::show_pane(st, Pane::Comparison),
    "cmpDocs.show.rowHighlights" => |st| {
        if need_comparison(st).is_none() { return; }
        if let Some(c) = st.comparison.write_unchecked().as_mut() { c.rows = !c.rows; }
        bump(st);
    },
    "cmpDocs.show.summary" => summary,
    "comparisonView.first" => |st| edge(st, false),
    "comparisonView.last" => |st| edge(st, true),
    "comparisonView.next" => |st| step(st, 1),
    "comparisonView.prev" => |st| step(st, -1),
    "comparisonView.showDiffers" => select_differing,
}
