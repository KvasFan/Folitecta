//! Interaction step: going to far pages. In the continuous, single page and
//! two-page layouts, with Go To Page, a bookmark and a search result, every
//! page in the window shows its picture and none of their pictures is
//! refused (the render threads learn the pages a view holds as it draws
//! them, not from its later report).

use serde_json::Value;

use super::*;
use crate::state::{Layout, Zoom};

fn add_tab(st: AppState, bytes: Vec<u8>) -> Option<u64> {
    let doc = folitecta_core::Document::from_bytes(bytes, None, None).ok()?;
    let id = st.alloc_id();
    st.docs_mut().push(DocTab::new(id, doc));
    let n = st.docs.read().len();
    st.set_active(n - 1);
    Some(id)
}

fn close_tab(st: AppState, id: u64, back_to: usize) {
    st.docs_mut().retain(|t| t.id != id);
    st.set_active(back_to);
}

/// The pages whose pictures came without one (refused or failed) since the
/// last call.
const ERRORS_JS: &str = "if (!window.__jumpErrors) { window.__jumpErrors = []; document.addEventListener('error', e => { const img = e.target; if (img && img.classList && (img.classList.contains('page-img') || img.classList.contains('page-tile'))) { const pg = img.closest('.page'); window.__jumpErrors.push(pg ? +pg.dataset.page : -1); } }, true); } const out = window.__jumpErrors; window.__jumpErrors = []; return out;";

/// Wait until page `target` is in the window and every page in it shows its
/// picture: `{ ms, pages: [[page, placeholder, naturalWidth], …] }`, `ms`
/// -1 when that did not happen within 6 s.
fn wait_js(target: usize) -> String {
    format!(
        "const t0 = performance.now(); const vis = () => {{ const c = document.getElementById('canvas'); if (!c) return null; const v = c.getBoundingClientRect(); const out = []; for (const pg of c.getElementsByClassName('page')) {{ const r = pg.getBoundingClientRect(); if (r.bottom <= v.top || r.top >= v.bottom || r.right <= v.left || r.left >= v.right) continue; const img = pg.querySelector('.page-img'); out.push([+pg.dataset.page, pg.classList.contains('placeholder') ? 1 : 0, img ? img.naturalWidth : -1]); }} return out; }}; while (performance.now() - t0 < 6000) {{ const s = vis(); if (s && s.some(p => p[0] === {target}) && s.every(p => p[1] === 0 && p[2] > 0)) return {{ ms: performance.now() - t0, pages: s }}; await new Promise(r => setTimeout(r, 20)); }} return {{ ms: -1, pages: vis() }};"
    )
}

/// After a jump to `target` (already asked for): wait for the pictures and
/// record whether every page in the window shows its picture with none
/// refused.
async fn check_jump(cx: &mut Ctx<'_>, name: &str, target: usize) {
    let st = cx.st;
    let before = crate::perf::PAGE_OTHER.get();
    let got = eval(&wait_js(target)).await.unwrap_or_default();
    pause(150).await;
    let errors = eval(ERRORS_JS).await.unwrap_or_default();
    let ms = got.get("ms").and_then(Value::as_f64).unwrap_or(-1.0);
    let shown: Vec<i64> = got
        .get("pages")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|p| p.get(0).and_then(Value::as_i64))
                .collect()
        })
        .unwrap_or_default();
    // Errors of the pages in the window (pages leaving it may be refused).
    let refused: Vec<i64> = errors
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(Value::as_i64)
                .filter(|p| shown.contains(p))
                .collect()
        })
        .unwrap_or_default();
    let page = st.current_page();
    // In the two-page layout the current page is the first one of the row
    // at the top.
    let rows = st.read_active(|t| t.layout == Layout::TwoPages).unwrap_or(false);
    cx.record(
        name,
        ms >= 0.0 && refused.is_empty() && (page == target || (rows && page + 1 == target)),
        format!(
            "current page {page}; sharp after {ms:.0} ms; window [page, placeholder, naturalWidth] = {}; refused {refused:?}; answers without a picture {}",
            got.get("pages").cloned().unwrap_or_default(),
            crate::perf::PAGE_OTHER.get() - before
        ),
    );
}

/// Ask a far page with Go To Page (as the page number box, the thumbnails
/// and the first and last page commands do) and check it.
async fn go_to(cx: &mut Ctx<'_>, name: &str, target: usize) {
    let _ = eval(ERRORS_JS).await;
    cx.st.navigate(target, true);
    check_jump(cx, name, target).await;
}

pub(super) async fn run(cx: &mut Ctx<'_>) {
    let st = cx.st;
    let back_to = st.active_index().unwrap_or(0);
    let left = {
        let ui = st.ui.read();
        (ui.show_left_pane, ui.left_pane.clone())
    };
    st.ui_mut().show_left_pane = false;

    // 1. The continuous layout, a long document.
    if let Some(id) = add_tab(st, folitecta_core::testdocs::long_doc(1000)) {
        st.with_active(|t| {
            t.layout = Layout::Continuous;
            t.zoom = Zoom::Fixed(1.0);
        });
        pause(1500).await;
        for target in [900usize, 120, 640, 333, 960, 47, 777, 15] {
            go_to(
                cx,
                &format!("jumps continuous: go to page {}", target + 1),
                target,
            )
            .await;
        }
        close_tab(st, id, back_to);
        pause(500).await;
    } else {
        cx.record("jumps: the long document opens", false, String::new());
    }

    // 2. The single page layout, at fit page and at 100 %.
    for zoom in [Zoom::FitPage, Zoom::Fixed(1.0)] {
        let Some(id) = add_tab(st, folitecta_core::testdocs::long_doc(30)) else {
            continue;
        };
        st.with_active(|t| {
            t.layout = Layout::Single;
            t.zoom = zoom;
        });
        pause(1000).await;
        for target in [3usize, 5, 6, 9, 14, 20, 27, 1] {
            go_to(
                cx,
                &format!("jumps single page {zoom:?}: go to page {}", target + 1),
                target,
            )
            .await;
        }
        close_tab(st, id, back_to);
        pause(500).await;
    }

    // 3. The two-page layout.
    if let Some(id) = add_tab(st, folitecta_core::testdocs::long_doc(60)) {
        st.with_active(|t| {
            t.layout = Layout::TwoPages;
            t.zoom = Zoom::Fixed(0.6);
        });
        pause(1000).await;
        for target in [10usize, 33, 51, 2, 44] {
            go_to(cx, &format!("jumps two pages: go to page {}", target + 1), target).await;
        }
        close_tab(st, id, back_to);
        pause(500).await;
    }

    // 4. A bookmark clicked in its pane.
    let mut doc =
        folitecta_core::Document::from_bytes(folitecta_core::testdocs::long_doc(400), None, None).ok();
    let marked = doc.as_mut().is_some_and(|d| {
        d.add_bookmark("Far chapter", 350, &[]).is_ok() && d.add_bookmark("Middle chapter", 120, &[]).is_ok()
    });
    if let (Some(doc), true) = (doc, marked) {
        let id = st.alloc_id();
        st.docs_mut().push(DocTab::new(id, doc));
        let n = st.docs.read().len();
        st.set_active(n - 1);
        {
            let mut ui = st.ui_mut();
            ui.show_left_pane = true;
            ui.left_pane = "bookmarks".into();
        }
        st.with_active(|t| {
            t.layout = Layout::Continuous;
            t.zoom = Zoom::Fixed(1.0);
        });
        pause(1500).await;
        for (i, target) in [(0usize, 350usize), (1, 120)] {
            let _ = eval(ERRORS_JS).await;
            let _ = eval(&format!("const it = document.querySelectorAll('.bookmark-tree .tree-item')[{i}]; if (it) it.click(); return !!it;")).await;
            check_jump(
                cx,
                &format!("jumps: bookmark {} goes to page {}", i + 1, target + 1),
                target,
            )
            .await;
        }
        st.ui_mut().show_left_pane = false;
        close_tab(st, id, back_to);
        pause(500).await;
    } else {
        cx.record("jumps: the document with bookmarks opens", false, String::new());
    }

    // 5. A search result: found by the search, then clicked in its pane
    // after going elsewhere.
    if let Some(id) = add_tab(st, folitecta_core::testdocs::marked_doc(400, 333, "Zyzzyva")) {
        st.with_active(|t| {
            t.layout = Layout::Continuous;
            t.zoom = Zoom::Fixed(1.0);
        });
        {
            let mut ui = st.ui_mut();
            ui.show_left_pane = true;
            ui.left_pane = "search".into();
        }
        pause(1500).await;
        let _ = eval(ERRORS_JS).await;
        crate::ui::panes::run_search(st, "Zyzzyva");
        check_jump(cx, "jumps: the search finds page 334", 333).await;
        go_to(cx, "jumps: back to page 5", 4).await;
        let _ = eval(ERRORS_JS).await;
        let _ = eval("const it = document.querySelector('.search-results .list-item.result'); if (it) it.click(); return !!it;").await;
        check_jump(cx, "jumps: the search result clicked goes to page 334", 333).await;
        crate::ui::panes::run_search(st, "");
        close_tab(st, id, back_to);
        pause(500).await;
    }

    {
        let mut ui = st.ui_mut();
        ui.show_left_pane = left.0;
        ui.left_pane = left.1;
    }
    st.set_active(back_to);
}
