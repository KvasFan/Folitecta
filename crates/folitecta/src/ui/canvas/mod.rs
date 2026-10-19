//! Document tabs and the page canvas: rendering, tools, selections and overlays.

use dioxus::html::input_data::MouseButton;
use dioxus::html::{InteractionElementOffset, InteractionLocation, ModifiersInteraction, PointerInteraction};
use dioxus::prelude::*;
use folitecta_core::marks::{display_to_pdf, pdf_rect_to_display};
use folitecta_core::{Markup, PageSize, Shape};

use crate::commands::{self, current_zoom, option_on};
use crate::files;
use crate::i18n::t;
use crate::icons::Icon;
use crate::state::{AfterUnsaved, AppState, Dialog, Layout, TextSel, ToastKind, Tool, Zoom};
use crate::ui::chrome::welcome::Welcome;
use crate::ui::features::form_fill::{FieldInput, fill_field};

mod create;
mod geometry;
mod page;
mod popups;
mod preview;
mod rulers;

pub use create::*;
use geometry::*;
use page::*;
pub use popups::*;
use preview::*;
use rulers::*;

/// Points per ruler unit and the minor/major tick spacing (in units).
pub fn unit_info(units: &str) -> (f32, f32, f32, &'static str) {
    match units {
        "pt" => (1.0, 10.0, 50.0, "pt"),
        "in" => (72.0, 0.125, 1.0, "in"),
        "cm" => (28.3465, 0.5, 1.0, "cm"),
        "pc" => (12.0, 1.0, 6.0, "pc"),
        _ => (2.83465, 5.0, 10.0, "mm"),
    }
}

#[component]
pub fn CanvasArea() -> Element {
    let st = use_context::<AppState>();
    if !st.has_doc() {
        return rsx! { div { class: "canvas-wrap", Welcome {} } };
    }
    let info_bar = option_on(st, "view.docInfoBar");
    // Portfolios show their files unless the cover is asked for.
    let files = st
        .read_active(|t| !t.portfolio.cover && t.doc.portfolio().is_some_and(|p| p.view != "H"))
        .unwrap_or(false);
    let split = st
        .read_active(|t| t.split.as_ref().map(|s| s.side_by_side))
        .flatten();
    rsx! {
        div { class: "canvas-wrap",
            DocTabs {}
            if info_bar { DocInfoBar {} }
            if files {
                crate::ui::features::portfolio::PortfolioView {}
            } else {
                if st.read_active(|t| t.doc.is_portfolio()).unwrap_or(false) {
                    crate::ui::features::portfolio::CoverBar {}
                }
                match split {
                    None => rsx! { Canvas { pane: 0 } },
                    Some(side) => rsx! {
                        div { class: if side { "canvas-split side" } else { "canvas-split stacked" },
                            Canvas { pane: 0 }
                            Canvas { pane: 1 }
                        }
                    },
                }
                Loupe {}
                if option_on(st, "view.panzoom") { PanZoomWindow {} }
                if option_on(st, "view.commentStyles") { crate::ui::panes::comment_styles::StylesPalette {} }
            }
        }
    }
}

/// Visible part of the current page as fractions of its size: x0, y0, x1, y1.
pub(crate) const VIEWPORT_JS: &str = "const c = document.getElementById('canvas'); const el = document.getElementById('pg-PAGE'); if (!c || !el) return null; const r = el.getBoundingClientRect(), v = c.getBoundingClientRect(); const f = (a, b) => Math.min(1, Math.max(0, a / b)); return [f(v.left - r.left, r.width), f(v.top - r.top, r.height), f(v.right - r.left, r.width), f(v.bottom - r.top, r.height)];";

/// Overview of the current page with the visible area; click or drag to move it.
#[component]
fn PanZoomWindow() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let mut view: Signal<Option<[f64; 4]>> = use_signal(|| None);
    let mut dragging = use_signal(|| false);
    let Some((tab, page, rev, size)) =
        st.read_active(|t| (t.id, t.page, t.view_rev(), t.page_sizes.get(t.page).copied()))
    else {
        return rsx! {};
    };
    // Follow scrolling and zooming.
    use_future(move || async move {
        loop {
            let p = st.current_page();
            let js = VIEWPORT_JS.replace("PAGE", &p.to_string());
            if let Ok(v) = document::eval(&js).await {
                let arr: Option<[f64; 4]> = v.as_array().and_then(|a| {
                    let n: Vec<f64> = a.iter().filter_map(|x| x.as_f64()).collect();
                    (n.len() == 4).then(|| [n[0], n[1], n[2], n[3]])
                });
                if *view.peek() != arr {
                    view.set(arr);
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        }
    });
    let (w, h) = size.map(|s| (s.width, s.height)).unwrap_or((595.0, 842.0));
    let box_w = 180.0f32;
    let box_h = (box_w * h / w).min(240.0);
    let box_w = box_h * w / h;
    let scale = ((box_w / w) * 2.0 * 1000.0).round().clamp(50.0, 2000.0) as u32;
    let rect = view().map(|r| {
        format!(
            "left:{}px;top:{}px;width:{}px;height:{}px",
            r[0] * box_w as f64,
            r[1] * box_h as f64,
            ((r[2] - r[0]) * box_w as f64).max(4.0),
            ((r[3] - r[1]) * box_h as f64).max(4.0)
        )
    });
    let move_to = move |fx: f64, fy: f64| {
        let page = st.current_page();
        let js = format!(
            "const c = document.getElementById('canvas'); const el = document.getElementById('pg-{page}'); if (c && el) {{ const r = el.getBoundingClientRect(), v = c.getBoundingClientRect(); c.scrollLeft += (r.left + {fx} * r.width) - (v.left + c.clientWidth / 2); c.scrollTop += (r.top + {fy} * r.height) - (v.top + c.clientHeight / 2); }}"
        );
        spawn(async move {
            let _ = document::eval(&js).await;
        });
    };
    let zoom_pct = (current_zoom(st) * 100.0).round();
    rsx! {
        div { class: "panzoom", onmousedown: move |e| e.stop_propagation(), onclick: move |e| e.stop_propagation(),
            div { class: "panzoom-head",
                span { class: "ellipsis", "{t(lang, \"cmd.view.panzoom\")}" }
                button { class: "ibtn", title: "{t(lang, \"dialog.close\")}", onclick: move |_| st.set_option("view.panzoom", false), "×" }
            }
            div { class: "panzoom-page", style: "width:{box_w}px;height:{box_h}px",
                onmousedown: move |e| {
                    dragging.set(true);
                    let p = e.element_coordinates();
                    move_to(p.x / box_w as f64, p.y / box_h as f64);
                },
                onmouseup: move |_| dragging.set(false),
                onmouseleave: move |_| dragging.set(false),
                img { src: "/page/{tab}/{page}/{rev}/{scale}.png", alt: "", draggable: "false", width: "{box_w}", height: "{box_h}" }
                if let Some(style) = rect {
                    div { class: "panzoom-view", style: "{style}" }
                }
                // Moves are followed only while the view is dragged, by a layer
                // over the page: a listener of mouse moves makes every move in
                // the window wait for the program.
                if dragging() {
                    div { class: "panzoom-capture",
                        onmousemove: move |e| {
                            let p = e.element_coordinates();
                            move_to(p.x / box_w as f64, p.y / box_h as f64);
                        },
                    }
                }
            }
            div { class: "panzoom-foot",
                button { class: "ibtn", title: "{t(lang, \"cmd.zoomOut\")}", onclick: move |_| commands::dispatch(st, "zoomOut"), Icon { name: "zoom-out" } }
                span { class: "muted", "{zoom_pct}%" }
                button { class: "ibtn", title: "{t(lang, \"cmd.zoomIn\")}", onclick: move |_| commands::dispatch(st, "zoomIn"), Icon { name: "zoom-in" } }
                button { class: "ibtn", title: "{t(lang, \"cmd.zoom.level.fitPage\")}", onclick: move |_| commands::dispatch(st, "zoom.level.fitPage"), Icon { name: "fit" } }
            }
        }
    }
}

#[component]
fn DocInfoBar() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let Some(info) = st.read_active(|t| t.doc.info()) else {
        return rsx! {};
    };
    let title = if info.title.is_empty() {
        t(lang, "label.untitled")
    } else {
        info.title.clone()
    };
    rsx! {
        div { class: "doc-info-bar",
            Icon { name: "info" }
            span { class: "strong", "{title}" }
            if !info.author.is_empty() { span { "· {info.author}" } }
            span { "· {info.page_count} {t(lang, \"dialog.props.pages\").to_lowercase()}" }
            span { "· PDF {info.version}" }
            span { "· {crate::util::human_size(info.file_size)}" }
            if info.encrypted { span { class: "chip", "{t(lang, \"dialog.props.encrypted\")}" } }
            if info.has_form { span { class: "chip", "{t(lang, \"dialog.props.form\")}" } }
        }
    }
}

/// Tab text: long names are shortened in the middle when `mid` is set (the
/// end is shortened by the style sheet otherwise).
fn tab_label(name: &str, mid: bool) -> String {
    let chars: Vec<char> = name.chars().collect();
    if !mid || chars.len() <= 28 {
        return name.to_string();
    }
    let head: String = chars[..13].iter().collect();
    let tail: String = chars[chars.len() - 13..].iter().collect();
    format!("{head}…{tail}")
}

#[component]
fn DocTabs() -> Element {
    let st = use_context::<AppState>();
    let active = st.active_index().unwrap_or(0);
    let (mode, mid) = {
        let ui = st.ui.read();
        (
            ui.value("tabs.title").unwrap_or("auto").to_string(),
            ui.value("tabs.ellipsis") == Some("mid"),
        )
    };
    let tabs: Vec<(u64, String, String, bool)> = st
        .docs
        .read()
        .iter()
        .map(|t| {
            let file = t.title();
            let doc_title = t.doc.title();
            let shown = match mode.as_str() {
                "filename" => file.clone(),
                "title" if !doc_title.is_empty() => doc_title.clone(),
                "title" => file.clone(),
                _ if !doc_title.is_empty() && t.doc.path().is_none() => doc_title.clone(),
                _ => file.clone(),
            };
            let tip = if doc_title.is_empty() || doc_title == file {
                file.clone()
            } else {
                format!("{doc_title}\n{file}")
            };
            (t.id, tab_label(&shown, mid), tip, t.doc.is_dirty())
        })
        .collect();
    rsx! {
        div { class: "doc-tabs",
            for (i, (id, title, tip, dirty)) in tabs.into_iter().enumerate() {
                button { class: if i == active { "doc-tab active" } else { "doc-tab" }, key: "{id}", title: "{tip}",
                    onclick: move |_| { st.set_active(i); files::set_window_title(st); },
                    span { class: "name", "{title}" }
                    if dirty { span { class: "dirty", "•" } }
                    span { class: "close", title: "Close", onclick: move |e| { e.stop_propagation(); files::close_tab(st, id, false, AfterUnsaved::Close); }, "×" }
                }
            }
            button { class: "doc-tab add", title: "Open…", onclick: move |_| files::open_dialog(st), Icon { name: "insert" } }
        }
    }
}

/// How many times the loupe enlarges the page as shown (Zoom In and Zoom
/// Out of the loupe menu change it).
pub(crate) fn loupe_factor(st: AppState) -> f32 {
    st.ui
        .read()
        .value("loupe.factor")
        .and_then(|v| v.parse().ok())
        .unwrap_or(3.0f32)
        .clamp(1.5, 8.0)
}

/// Floating magnifier for the loupe tool.
#[component]
fn Loupe() -> Element {
    let st = use_context::<AppState>();
    if (st.tool)() != Tool::Loupe {
        return rsx! {};
    }
    let Some((page, x, y, cx, cy)) = (st.loupe)() else {
        return rsx! {};
    };
    let Some((tab, rev)) = st.read_active(|t| (t.id, t.view_rev())) else {
        return rsx! {};
    };
    let zoom = current_zoom(st);
    let mag = (zoom * loupe_factor(st)).min(8.0);
    let scale = ((mag * (st.dpr)() as f32) * 1000.0).round().clamp(50.0, 8000.0) as u32;
    let (w, h) = (240.0, 170.0);
    let (vw, vh) = (st.viewport)();
    let left = if cx + 24.0 + w > vw {
        cx - w - 24.0
    } else {
        cx + 24.0
    };
    let top = (cy - h / 2.0).clamp(4.0, (vh - h - 4.0).max(4.0));
    let page_w = st
        .read_active(|t| t.page_sizes.get(page).map(|s| s.width))
        .flatten()
        .unwrap_or(595.0)
        * mag;
    let bx = -(x * mag - w as f32 / 2.0);
    let by = -(y * mag - h as f32 / 2.0);
    rsx! {
        div { class: "loupe", style: "left:{left}px;top:{top}px;width:{w}px;height:{h}px;background-image:url(/page/{tab}/{page}/{rev}/{scale}.png);background-size:{page_w}px auto;background-position:{bx}px {by}px" }
    }
}

const PAN_SCRIPT: &str = r#"
(function(){
  if (window.__foliPan) return; window.__foliPan = true;
  let drag = null;
  document.addEventListener('mousedown', e => {
    const c = e.target.closest && e.target.closest('#canvas');
    if (!c || !c.classList.contains('tool-hand') || e.button !== 0) return;
    if (e.target.closest('.annot-hit, .handle, .sel-toolbar, .field-hit, .field-input, input, textarea, select, button, a')) return;
    drag = { c, x: e.clientX, y: e.clientY, sl: c.scrollLeft, st: c.scrollTop };
    c.classList.add('dragging');
    e.preventDefault();
  }, true);
  document.addEventListener('mousemove', e => {
    if (!drag) return;
    drag.c.scrollLeft = drag.sl - (e.clientX - drag.x);
    drag.c.scrollTop = drag.st - (e.clientY - drag.y);
  }, true);
  document.addEventListener('mouseup', () => {
    if (drag) { drag.c.classList.remove('dragging'); drag = null; }
  }, true);
})();
"#;

/// Scrolling and Ctrl+wheel zoom of the page views, handled in the page so
/// that scrolling never waits for the program: no listener of the program
/// runs for a wheel or scroll event. Ctrl+wheel steps are added up for a
/// frame and sent once (a notch of a wheel is a step of ×1.1, the small
/// steps of a touchpad a part of one); the page at the top of a view (the
/// one whose top edge is nearest to the top of the view, found by a binary
/// search) is sent at most every 100 ms while it scrolls.
pub(crate) const CANVAS_EVENTS_SCRIPT: &str = r#"
(function(){
  const send = m => dioxus.send(m);
  const soon = f => { let done = false; const go = () => { if (!done) { done = true; f(); } }; requestAnimationFrame(go); setTimeout(go, 60); };
  const zoom = new Map();
  document.addEventListener('wheel', e => {
    if (!e.ctrlKey) return;
    const c = e.target && e.target.closest ? e.target.closest('.canvas') : null;
    if (!c) return;
    e.preventDefault();
    const dy = e.deltaY * (e.deltaMode === 1 ? 40 : e.deltaMode === 2 ? 400 : 1);
    if (!dy) return;
    const steps = Math.sign(dy) * Math.min(Math.abs(dy) / 50, 1);
    if (!zoom.has(c.id)) {
      zoom.set(c.id, 1);
      soon(() => { const f = zoom.get(c.id); zoom.delete(c.id); if (f && Math.abs(f - 1) > 1e-6) send(['zoom', c.id, f]); });
    }
    zoom.set(c.id, zoom.get(c.id) * Math.pow(1.1, -steps));
  }, { capture: true, passive: false });
  const top = c => {
    const pages = c.getElementsByClassName('page'); const n = pages.length;
    if (n < 2) return -1;
    const horizontal = !!c.querySelector('.pages.horizontal');
    const v = c.getBoundingClientRect(); const target = (horizontal ? v.left : v.top) + 24;
    const edge = el => { const r = el.getBoundingClientRect(); return horizontal ? r.left : r.top; };
    let lo = 0, hi = n;
    while (lo < hi) { const m = (lo + hi) >> 1; if (edge(pages[m]) < target) lo = m + 1; else hi = m; }
    let best = -1, bd = Infinity;
    for (let i = Math.max(0, lo - 2); i < Math.min(n, lo + 2); i++) { const d = Math.abs(edge(pages[i]) - target); if (d < bd) { bd = d; best = i; } }
    if (best < 0) return -1;
    // The pages of a row share their edge: the first one of the row.
    while (best > 0 && Math.abs(edge(pages[best - 1]) - edge(pages[best])) < 0.5) best--;
    const p = parseInt(pages[best].dataset.page);
    return isNaN(p) ? -1 : p;
  };
  const waiting = new Set();
  document.addEventListener('scroll', e => {
    const c = e.target;
    if (!c || !c.classList || !c.classList.contains('canvas') || waiting.has(c)) return;
    waiting.add(c);
    setTimeout(() => { waiting.delete(c); const p = top(c); if (p >= 0) send(['page', c.id, p]); }, 100);
  }, { capture: true, passive: true });
  return true;
})();
"#;

/// Follow the messages of [`CANVAS_EVENTS_SCRIPT`] for as long as the
/// window lives: Ctrl+wheel zooms the view it happened in (focusing it in a
/// split window), and the page at the top of a view becomes its current page
/// (written only when it changes).
pub(crate) async fn follow_canvas_events(st: AppState) {
    let mut eval = document::eval(CANVAS_EVENTS_SCRIPT);
    while let Ok(msg) = eval.recv::<serde_json::Value>().await {
        let Some(m) = msg.as_array() else { continue };
        let kind = m.first().and_then(|v| v.as_str()).unwrap_or_default();
        let focused = m.get(1).and_then(|v| v.as_str()) == Some("canvas");
        match kind {
            "zoom" => {
                let Some(f) = m
                    .get(2)
                    .and_then(|v| v.as_f64())
                    .filter(|f| f.is_finite() && *f > 0.0)
                else {
                    continue;
                };
                if !focused {
                    let other = st
                        .read_active(|t| t.split.as_ref().map(|s| 1 - s.focus.min(1)))
                        .flatten();
                    match other {
                        Some(pane) => st.focus_view(pane),
                        None => continue,
                    }
                }
                let z = current_zoom(st);
                let next = (z * f as f32).clamp(0.1, 8.0);
                if (next - z).abs() > 1e-4 {
                    st.with_active(|t| t.zoom = Zoom::Fixed(next));
                }
            }
            "page" => {
                let Some(i) = m.get(2).and_then(|v| v.as_u64()).map(|i| i as usize) else {
                    continue;
                };
                let shown = st.read_active(|t| {
                    let n = t.doc.page_count();
                    match t.split.as_ref().filter(|_| !focused) {
                        Some(s) => (s.other.layout, s.other.page, n),
                        None => (t.layout, t.page, n),
                    }
                });
                let Some((layout, page, n)) = shown else { continue };
                if layout == Layout::Single || page == i || i >= n {
                    continue;
                }
                st.with_active(|t| match t.split.as_mut().filter(|_| !focused) {
                    Some(s) => s.other.page = i,
                    None => t.page = i,
                });
            }
            _ => {}
        }
    }
}

/// The rows of pages a view shows: its page alone, every page, or the pages
/// two by two (the first alone when it is a cover); every page in one row
/// when the pages scroll sideways, from the right when they read right to
/// left.
fn page_rows(
    layout: Layout,
    page: usize,
    count: usize,
    cover: bool,
    horizontal: bool,
    r2l: bool,
) -> Vec<Vec<usize>> {
    let mut rows: Vec<Vec<usize>> = match layout {
        Layout::Single => vec![vec![page.min(count.saturating_sub(1))]],
        Layout::Continuous => (0..count).map(|p| vec![p]).collect(),
        Layout::TwoPages => {
            let mut rows = Vec::new();
            let mut i = 0;
            if cover && count > 0 {
                rows.push(vec![0]);
                i = 1;
            }
            while i < count {
                let mut r = vec![i];
                if i + 1 < count {
                    r.push(i + 1);
                }
                rows.push(r);
                i += 2;
            }
            rows
        }
    };
    if horizontal {
        // Horizontal scrolling: every page side by side in one row.
        rows = vec![rows.into_iter().flatten().collect()];
    }
    if r2l {
        rows.iter_mut().for_each(|r| r.reverse());
    }
    rows
}

/// A view of the active document. Without a split window there is one,
/// `pane` 0. In a split window the focused view is drawn with every tool
/// (ids `canvas` and `pg-N`, which the commands look for); the other one
/// shows the pictures of its pages (ids `canvas-other` and `spg-N`) and
/// becomes the focused one when it is clicked. The scrolling element stays
/// the same when the focus moves, so each view keeps its place.
#[component]
fn Canvas(pane: u8) -> Element {
    crate::perf::CANVASES.bump();
    let st = use_context::<AppState>();
    use_hook(|| {
        spawn(async move {
            let _ = document::eval(PAN_SCRIPT).await;
        });
    });
    let dpr = (st.dpr)();
    let tool = (st.tool)();
    let rulers = option_on(st, "view.showRulers");
    let Some((tab_id, rev, count, split)) =
        st.read_active(|t| (t.id, t.view_rev(), t.doc.page_count(), t.split.clone()))
    else {
        return rsx! {};
    };
    let split_on = split.is_some();
    let focused = split.as_ref().is_none_or(|s| s.focus == pane);
    let (view, zoom) = match split.as_ref().filter(|_| !focused) {
        Some(s) => (
            s.other,
            st.read_active(|t| commands::zoom_for(t, &s.other, s.other_size, rulers))
                .unwrap_or(1.0),
        ),
        None => match st.read_active(|t| t.view_state()) {
            Some(v) => (v, current_zoom(st)),
            None => return rsx! {},
        },
    };
    let (page, layout, rotation) = (view.page, view.layout, view.rotation);
    let render_scale = ((zoom * dpr as f32) * 1000.0).round().clamp(50.0, 6000.0) as u32;
    let labels = st.read_active(|t| t.doc.page_labels()).unwrap_or_default();
    let gaps = option_on(st, "view.layout.showGaps");
    let r2l = option_on(st, "view.layout.r2l");
    let cover = option_on(st, "view.layout.showCover");
    let horizontal = option_on(st, "view.layout.ribbon") && layout != Layout::Single;
    let rows = page_rows(layout, page, count, cover, horizontal, r2l);
    let canvas_id = if focused { "canvas" } else { "canvas-other" };
    let tool_class = match tool {
        _ if !focused => "other",
        Tool::Hand => "tool-hand",
        Tool::Select | Tool::TextHighlight | Tool::TextUnderline | Tool::TextStrike => "tool-text",
        Tool::Zoom | Tool::ZoomDyn => "tool-zoom",
        Tool::Loupe => "tool-loupe",
        Tool::EditAnnots => "tool-edit",
        Tool::Ink => "tool-pencil",
        _ => "tool-draw",
    };
    let mut pages_class = String::from("pages");
    if !gaps {
        pages_class.push_str(" nogap");
    }
    if rulers && focused {
        pages_class.push_str(" with-rulers");
    }
    if horizontal {
        pages_class.push_str(" horizontal");
    }
    let canvas_class = if split_on && focused {
        format!("canvas {tool_class} focused")
    } else {
        format!("canvas {tool_class}")
    };

    rsx! {
        div {
            id: canvas_id,
            class: "{canvas_class}",
            onresize: move |evt| {
                if let Ok(size) = evt.data().get_content_box_size() {
                    let size = (size.width, size.height);
                    if focused {
                        st.set_canvas_size(size);
                    } else if st.read_active(|t| t.split.as_ref().map(|s| s.other_size)).flatten() != Some(size) {
                        st.with_active(|t| {
                            if let Some(s) = t.split.as_mut() {
                                s.other_size = size;
                            }
                        });
                    }
                }
            },
            // A press in the other view of a split window focuses it.
            onmousedown: move |_| {
                if !focused {
                    st.focus_view(pane);
                }
            },
            onmouseleave: move |_| {
                if st.loupe.peek().is_some() && option_on(st, "tool.loupe.trackMouse") { st.set_loupe(None); }
                if st.pointer.peek().is_some() { st.set_pointer(None); }
            },
            div { class: "{pages_class}",
                for (ri, row) in rows.into_iter().enumerate() {
                    div { class: "page-row", key: "row-{ri}",
                        for p in row {
                            if focused {
                                PageView { key: "{tab_id}-{p}", tab_id, page: p, rev, scale: render_scale, zoom, current: p == page, tool, view_rotation: rotation, label: labels.get(p).cloned().unwrap_or_default() }
                            } else {
                                StillPage { key: "s{tab_id}-{p}", tab_id, page: p, rev, scale: render_scale, zoom, view_rotation: rotation, label: labels.get(p).cloned().unwrap_or_default() }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// A page of the view that is not focused: its picture (comments included)
/// and its number.
#[component]
fn StillPage(
    tab_id: u64,
    page: usize,
    rev: u64,
    scale: u32,
    zoom: f32,
    view_rotation: i32,
    label: String,
) -> Element {
    let st = use_context::<AppState>();
    let Some(size) = st.read_active(|t| t.page_sizes.get(page).copied()).flatten() else {
        return rsx! {};
    };
    let mut filter = String::new();
    if option_on(st, "view.darkPageMode") {
        filter.push_str("invert(1) hue-rotate(180deg) ");
    }
    if option_on(st, "view.overrideDocumentColors") {
        filter.push_str("grayscale(1) contrast(1.6) ");
    }
    let img_style = if filter.is_empty() {
        String::new()
    } else {
        format!("filter:{filter}")
    };
    let (inner_w, inner_h) = (size.width * zoom, size.height * zoom);
    let (outer_w, outer_h) = if view_rotation % 180 != 0 {
        (inner_h, inner_w)
    } else {
        (inner_w, inner_h)
    };
    let transform = match view_rotation {
        90 => format!("translate({inner_h}px,0) rotate(90deg)"),
        180 => format!("translate({inner_w}px,{inner_h}px) rotate(180deg)"),
        270 => format!("translate(0,{inner_w}px) rotate(270deg)"),
        _ => String::new(),
    };
    let number = if !label.is_empty() && label != (page + 1).to_string() {
        format!("{label} ({})", page + 1)
    } else {
        (page + 1).to_string()
    };
    rsx! {
        div { id: "spg-{page}", class: "page", "data-page": "{page}", style: "width:{outer_w}px;height:{outer_h}px",
            div { class: "page-inner", style: "width:{inner_w}px;height:{inner_h}px;transform:{transform}",
                img { class: "page-img", src: "/page/{tab_id}/{page}/{rev}/{scale}.png", alt: "", loading: "lazy", draggable: "false", style: "{img_style}" }
            }
            div { class: "page-label", "{number}" }
        }
    }
}
