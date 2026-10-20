//! Interaction step: going to far pages. In the continuous, single page and
//! two-page layouts, with Go To Page, a bookmark and a search result, every
//! page in the window shows its picture and none of their pictures is
//! refused (the render threads learn the pages a view holds as it draws
//! them, not from its later report). A bookmark row pressed stays the same
//! element (a row replaced before the release gets no click), and on Linux
//! a bookmark clicked with the mouse, through the window toolkit, goes to
//! its page.

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

/// Wait until every page in the window of view `view` (`canvas`, or
/// `canvas-other` for the other view of a split window) shows its picture:
/// `{ ms, pages }` as [`wait_js`].
fn wait_view_js(view: &str) -> String {
    format!(
        "const t0 = performance.now(); const vis = () => {{ const c = document.getElementById('{view}'); if (!c) return null; const v = c.getBoundingClientRect(); const out = []; for (const pg of c.getElementsByClassName('page')) {{ const r = pg.getBoundingClientRect(); if (r.bottom <= v.top || r.top >= v.bottom || r.right <= v.left || r.left >= v.right) continue; const img = pg.querySelector('.page-img'); out.push([+pg.dataset.page, pg.classList.contains('placeholder') ? 1 : 0, img ? img.naturalWidth : -1]); }} return out; }}; while (performance.now() - t0 < 6000) {{ const s = vis(); if (s && s.length && s.every(p => p[1] === 0 && p[2] > 0)) return {{ ms: performance.now() - t0, pages: s }}; await new Promise(r => setTimeout(r, 20)); }} return {{ ms: -1, pages: vis() }};"
    )
}

/// Record whether every page in the window of view `view` shows its
/// picture, none of them refused.
async fn check_view(cx: &mut Ctx<'_>, name: &str, view: &str) {
    let got = eval(&wait_view_js(view)).await.unwrap_or_default();
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
    let refused: Vec<i64> = errors
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(Value::as_i64)
                .filter(|p| shown.contains(p))
                .collect()
        })
        .unwrap_or_default();
    cx.record(
        name,
        ms >= 0.0 && refused.is_empty(),
        format!(
            "sharp after {ms:.0} ms; window [page, placeholder, naturalWidth] = {}; refused {refused:?}",
            got.get("pages").cloned().unwrap_or_default()
        ),
    );
}

/// The place of tab `id` among the tabs.
fn index_of(st: AppState, id: u64) -> usize {
    st.docs.peek().iter().position(|t| t.id == id).unwrap_or(0)
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

/// The time of an event given to the web view: milliseconds that go on, as
/// those of the window system do (outside the handler of an event the
/// toolkit has no time to give, and two clicks at the same time are a
/// double click).
#[cfg(target_os = "linux")]
fn event_time() -> u32 {
    static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    let start = START.get_or_init(std::time::Instant::now);
    1_000 + start.elapsed().as_millis() as u32
}

/// An event of the pointer given to the web view as the window toolkit gives
/// those of the mouse (trusted input for the page): `kind` a GDK event type.
#[cfg(target_os = "linux")]
fn pointer_event(kind: i32, x: f64, y: f64, pressed: bool) -> bool {
    use dioxus::desktop::wry::WebViewExtUnix;
    use gtk::glib::translate::{FromGlibPtrFull, ToGlibPtr};
    use gtk::prelude::*;
    let window = dioxus::desktop::window();
    let view = window.webview.webview();
    let Some(gdk_window) = view.window() else {
        return false;
    };
    let Some(pointer) = gdk_window.display().default_seat().and_then(|s| s.pointer()) else {
        return false;
    };
    let state = if pressed {
        gtk::gdk::ffi::GDK_BUTTON1_MASK
    } else {
        0
    };
    // SAFETY: the event is made by GDK for its type and every field written
    // is of that type; the window and device given are alive, and the event
    // owns its references once wrapped (`from_glib_full`).
    unsafe {
        let ev = gtk::gdk::ffi::gdk_event_new(kind);
        let time = event_time();
        if kind == gtk::gdk::ffi::GDK_MOTION_NOTIFY {
            let m = &mut (*ev).motion;
            m.window = gdk_window.to_glib_full();
            (m.time, m.x, m.y, m.x_root, m.y_root, m.state) = (time, x, y, x, y, state);
        } else {
            let b = &mut (*ev).button;
            b.window = gdk_window.to_glib_full();
            (b.time, b.x, b.y, b.x_root, b.y_root, b.state, b.button) = (time, x, y, x, y, state, 1);
        }
        gtk::gdk::ffi::gdk_event_set_device(ev, pointer.to_glib_none().0);
        let event = gtk::gdk::Event::from_glib_full(ev);
        view.event(&event)
    }
}

/// A click at (`x`, `y`) of the web view with the mouse: a move there, a
/// press held 150 ms and a release.
#[cfg(target_os = "linux")]
async fn real_click(x: f64, y: f64) -> String {
    let a = pointer_event(gtk::gdk::ffi::GDK_MOTION_NOTIFY, x, y, false);
    pause(200).await;
    let b = pointer_event(gtk::gdk::ffi::GDK_BUTTON_PRESS, x, y, false);
    pause(150).await;
    let c = pointer_event(gtk::gdk::ffi::GDK_BUTTON_RELEASE, x, y, true);
    pause(300).await;
    format!("input {a}/{b}/{c}")
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
        // A row pressed stays the same element until the release.
        let kept = eval("const it = document.querySelectorAll('.bookmark-tree .tree-item')[1]; if (!it) return null; window.__jumpPressed = it; const r = it.getBoundingClientRect(); it.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, button: 0, clientX: r.left + 30, clientY: r.top + 10 })); return true;").await;
        pause(400).await;
        let still = eval("const it = window.__jumpPressed; if (!it) return null; const kept = it.isConnected; const r = it.getBoundingClientRect(); it.dispatchEvent(new MouseEvent('mouseup', { bubbles: true, button: 0, clientX: r.left + 30, clientY: r.top + 10 })); window.__jumpPressed = null; return kept;").await;
        cx.record(
            "jumps: a bookmark row pressed stays the same element",
            still.as_ref().ok().and_then(Value::as_bool) == Some(true),
            format!("pressed {kept:?}, still there at the release {still:?}"),
        );
        #[cfg(target_os = "linux")]
        {
            // A real click (press, 150 ms, release) through the window toolkit.
            let _ = eval(ERRORS_JS).await;
            let pos = eval("const it = document.querySelectorAll('.bookmark-tree .tree-item')[1]; if (!it) return null; const r = it.getBoundingClientRect(); return [r.left + 40, r.top + r.height / 2];").await.unwrap_or_default();
            match pos
                .as_array()
                .and_then(|a| Some((a.first()?.as_f64()?, a.get(1)?.as_f64()?)))
            {
                Some((x, y)) => {
                    let input = real_click(x, y).await;
                    check_jump(
                        cx,
                        &format!("jumps: bookmark 2 clicked with the mouse goes to page 121 ({input})"),
                        120,
                    )
                    .await;
                    // The same row clicked again later: a click, not the
                    // second one of a double click (which renames it).
                    st.navigate(0, false);
                    pause(800).await;
                    let _ = eval(ERRORS_JS).await;
                    let input = real_click(x, y).await;
                    let dialog = st.dialog.peek().clone();
                    check_jump(
                        cx,
                        &format!("jumps: bookmark 2 clicked again with the mouse goes to page 121 ({input})"),
                        120,
                    )
                    .await;
                    cx.record(
                        "jumps: a second click on a bookmark is not a double click",
                        dialog == crate::state::Dialog::None,
                        format!("{dialog:?}"),
                    );
                    st.set_dialog(crate::state::Dialog::None);
                }
                None => cx.record("jumps: the bookmark row is there", false, pos.to_string()),
            }
            st.navigate(0, false);
            pause(800).await;
        }
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

    // 6. A split window: each view goes to its pages, the other keeps its
    // own; then two documents, switching between them quickly.
    if let Some(a) = add_tab(st, folitecta_core::testdocs::long_doc(400)) {
        st.with_active(|t| {
            t.layout = Layout::Continuous;
            t.zoom = Zoom::Fixed(1.0);
        });
        pause(1500).await;
        crate::commands::dispatch(st, "window.split.vert");
        pause(1500).await;
        let _ = eval(ERRORS_JS).await;
        check_view(cx, "jumps split: the focused view", "canvas").await;
        check_view(cx, "jumps split: the other view", "canvas-other").await;
        go_to(cx, "jumps split: the focused view goes to page 301", 300).await;
        check_view(cx, "jumps split: the other view keeps its pages", "canvas-other").await;
        st.focus_view(1);
        pause(600).await;
        go_to(cx, "jumps split: the other view, focused, goes to page 151", 150).await;
        check_view(cx, "jumps split: the first view keeps page 301", "canvas-other").await;
        let other = st
            .read_active(|t| t.split.as_ref().map(|s| s.other.page))
            .flatten();
        cx.record(
            "jumps split: the first view is still at page 301",
            other == Some(300),
            format!("{other:?}"),
        );
        crate::commands::dispatch(st, "window.removeSplit");
        pause(800).await;
        check_view(cx, "jumps split: the window without its split", "canvas").await;
        if let Some(b) = add_tab(st, folitecta_core::testdocs::long_doc(300)) {
            st.with_active(|t| {
                t.layout = Layout::Continuous;
                t.zoom = Zoom::Fixed(1.0);
            });
            pause(1200).await;
            st.navigate(60, true);
            pause(800).await;
            st.set_active(index_of(st, a));
            pause(800).await;
            st.navigate(200, true);
            pause(800).await;
            for _ in 0..4 {
                st.set_active(index_of(st, b));
                pause(40).await;
                st.set_active(index_of(st, a));
                pause(40).await;
            }
            let _ = eval(ERRORS_JS).await;
            st.set_active(index_of(st, b));
            check_view(
                cx,
                "jumps tabs: the second document after quick switching",
                "canvas",
            )
            .await;
            st.set_active(index_of(st, a));
            check_view(
                cx,
                "jumps tabs: the first document after quick switching",
                "canvas",
            )
            .await;
            // A jump, then the other document at once.
            st.navigate(350, true);
            pause(30).await;
            st.set_active(index_of(st, b));
            check_view(
                cx,
                "jumps tabs: the second document after a jump in the first",
                "canvas",
            )
            .await;
            // (The tabs share one scroll position, as in 0.2.0: the first
            // document shows the pages at that position, not its page.)
            st.set_active(index_of(st, a));
            check_view(cx, "jumps tabs: the first document again", "canvas").await;
            close_tab(st, b, index_of(st, a));
        }
        close_tab(st, a, back_to);
        pause(500).await;
    }

    {
        let mut ui = st.ui_mut();
        ui.show_left_pane = left.0;
        ui.left_pane = left.1;
    }
    st.set_active(back_to);
}
