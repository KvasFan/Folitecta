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
use folitecta_core::render_service::TILE;

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
    let grouped = st.groups.read().is_some();
    rsx! {
        div { class: "canvas-wrap",
            if !grouped { DocTabs {} }
            if info_bar { DocInfoBar {} }
            if files {
                crate::ui::features::portfolio::PortfolioView {}
            } else {
                if st.read_active(|t| t.doc.is_portfolio()).unwrap_or(false) {
                    crate::ui::features::portfolio::CoverBar {}
                }
                if option_on(st, "view.reflow") {
                    crate::ui::features::reflow::ReflowView {}
                } else {
                    match split {
                        // Two groups of tabs: each with its tab strip over its view.
                        Some(side) if grouped => rsx! {
                            div { class: if side { "canvas-split side groups" } else { "canvas-split stacked groups" },
                                for pane in [0u8, 1] {
                                    div { key: "{pane}", class: "tab-group", "data-group": "{pane}",
                                        DocTabs { group: Some(pane as usize) }
                                        Canvas { pane }
                                    }
                                }
                            }
                        },
                        None => rsx! { Canvas { pane: 0 } },
                        Some(side) => rsx! {
                            div { class: if side { "canvas-split side" } else { "canvas-split stacked" },
                                Canvas { pane: 0 }
                                Canvas { pane: 1 }
                            }
                        },
                    }
                }
                if option_on(st, "view.readAloud") {
                    crate::ui::features::read_aloud::ReadAloudBar {}
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
    let Some((tab, page, rev, size)) = st.read_active(|t| {
        (
            t.id,
            t.page,
            t.page_rev(t.page),
            t.page_sizes.get(t.page).copied(),
        )
    }) else {
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

/// The tab strip: of every tab, or of one group of tabs.
#[component]
fn DocTabs(#[props(default)] group: Option<usize>) -> Element {
    let st = use_context::<AppState>();
    let active = st.active_index().unwrap_or(0);
    let groups = st.groups.read().clone();
    let grouped = groups.is_some();
    let in_group = |id: u64| match (&groups, group) {
        (Some(g), Some(i)) => g.tabs[i.min(1)].contains(&id),
        _ => true,
    };
    // The tab a group shows is marked in its strip; the group worked in is
    // marked as a whole.
    let shown = match (&groups, group) {
        (Some(g), Some(i)) => Some(g.shown[i.min(1)]),
        _ => None,
    };
    let focused = match (&groups, group) {
        (Some(g), Some(i)) => g.focus == i,
        _ => true,
    };
    let (mode, mid) = {
        let ui = st.ui.read();
        (
            ui.value("tabs.title").unwrap_or("auto").to_string(),
            ui.value("tabs.ellipsis") == Some("mid"),
        )
    };
    let tabs: Vec<(usize, u64, String, String, bool)> = st
        .docs
        .read()
        .iter()
        .enumerate()
        .filter(|(_, t)| in_group(t.id))
        .map(|(i, t)| {
            let file = t.title();
            let doc_title = t.doc.title();
            let label = match mode.as_str() {
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
            (i, t.id, tab_label(&label, mid), tip, t.doc.is_dirty())
        })
        .collect();
    rsx! {
        div { class: if focused { "doc-tabs" } else { "doc-tabs unfocused" },
            for (i, id, title, tip, dirty) in tabs.into_iter() {
                button { class: if shown.map_or(i == active, |s| s == id) { "doc-tab active" } else { "doc-tab" }, key: "{id}", title: "{tip}",
                    onclick: move |_| {
                        st.set_active(i);
                        files::set_window_title(st);
                        if grouped { commands::split_view::show_pages(st); }
                    },
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
    let Some((tab, rev)) = st.read_active(|t| (t.id, t.page_rev(page))) else {
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
    let (page_w, page_h) = st
        .read_active(|t| t.page_sizes.get(page).map(|s| (s.width, s.height)))
        .flatten()
        .unwrap_or((595.0, 842.0));
    // The loupe shows the tiles of a fixed grid over the page picture at its
    // scale (the whole page at up to 8x was drawn before): only the tiles
    // under it are drawn, and moving it reuses the tiles drawn already.
    let dpr = (st.dpr)() as f32;
    let s = scale as f32 / 1000.0;
    let (pw, ph) = ((page_w * s) as u32, (page_h * s) as u32);
    let (x0, y0) = (x * s - w as f32 * dpr / 2.0, y * s - h as f32 * dpr / 2.0);
    let (x1, y1) = (x0 + w as f32 * dpr, y0 + h as f32 * dpr);
    let t = TILE as f32;
    let cols = (x0.max(0.0) / t) as u32..=((x1.max(0.0) / t) as u32).min(pw.saturating_sub(1) / TILE);
    let rows = (y0.max(0.0) / t) as u32..=((y1.max(0.0) / t) as u32).min(ph.saturating_sub(1) / TILE);
    let tiles: Vec<(u32, u32, String)> = rows
        .flat_map(|r| cols.clone().map(move |c| (c, r)))
        .map(|(c, r)| {
            let (tw, th) = ((TILE + 1).min(pw - c * TILE), (TILE + 1).min(ph - r * TILE));
            let style = format!(
                "left:{}px;top:{}px;width:{}px;height:{}px",
                (c as f32 * t - x0) / dpr,
                (r as f32 * t - y0) / dpr,
                tw as f32 / dpr,
                th as f32 / dpr
            );
            (c, r, style)
        })
        .collect();
    rsx! {
        div { class: "loupe", style: "left:{left}px;top:{top}px;width:{w}px;height:{h}px",
            for (c, r, style) in tiles {
                img { key: "{c}-{r}", src: "/page/{tab}/{page}/{rev}/{scale}.png?tile={c},{r}", alt: "", draggable: "false", style: "{style}" }
            }
        }
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
/// steps of a touchpad a part of one). While a view scrolls, at most every
/// 100 ms, the page at its top (the one whose top edge is nearest to the top
/// of the view) and the first and last pages in it are sent, found by binary
/// searches over the pages.
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
  const num = el => { const p = parseInt(el.dataset.page); return isNaN(p) ? -1 : p; };
  // The page at the top and the first and last pages in the view.
  const shown = c => {
    const pages = c.getElementsByClassName('page'); const n = pages.length;
    if (!n) return null;
    const horizontal = !!c.querySelector('.pages.horizontal');
    const v = c.getBoundingClientRect();
    const start = horizontal ? v.left : v.top, end = horizontal ? v.right : v.bottom;
    const rect = el => el.getBoundingClientRect();
    const lead = el => horizontal ? rect(el).left : rect(el).top;
    const tail = el => horizontal ? rect(el).right : rect(el).bottom;
    // The first page with its edge at or after the top (plus 24 px).
    const target = start + 24;
    let lo = 0, hi = n;
    while (lo < hi) { const m = (lo + hi) >> 1; if (lead(pages[m]) < target) lo = m + 1; else hi = m; }
    let best = -1, bd = Infinity;
    for (let i = Math.max(0, lo - 2); i < Math.min(n, lo + 2); i++) { const d = Math.abs(lead(pages[i]) - target); if (d < bd) { bd = d; best = i; } }
    // The pages of a row share their edge: the first one of the row.
    while (best > 0 && Math.abs(lead(pages[best - 1]) - lead(pages[best])) < 0.5) best--;
    // The first page ending after the top, the last starting before the end.
    lo = 0; hi = n;
    while (lo < hi) { const m = (lo + hi) >> 1; if (tail(pages[m]) <= start) lo = m + 1; else hi = m; }
    const first = Math.min(lo, n - 1);
    lo = first; hi = n;
    while (lo < hi) { const m = (lo + hi) >> 1; if (lead(pages[m]) < end) lo = m + 1; else hi = m; }
    const last = Math.max(first, lo - 1);
    // In a row read from the right the numbers go down: take the extremes.
    const a = num(pages[first]), b = num(pages[last]);
    return [best >= 0 && n > 1 ? num(pages[best]) : -1, Math.min(a, b), Math.max(a, b)];
  };
  const report = c => { const s = shown(c); if (s) send(['view', c.id].concat(s)); };
  // A comparison side by side: the other view follows the focused one, the
  // page window.folitectaSync gives beside the page at the top, as far into
  // it (only when the other view is marked to follow).
  const follow = c => {
    const map = window.folitectaSync;
    if (!map || c.id !== 'canvas') return;
    const o = document.getElementById('canvas-other');
    if (!o || o.dataset.follow !== '1' || c.querySelector('.pages.horizontal')) return;
    const pages = c.getElementsByClassName('page'); const n = pages.length;
    if (!n) return;
    const v = c.getBoundingClientRect();
    let lo = 0, hi = n;
    while (lo < hi) { const m = (lo + hi) >> 1; if (pages[m].getBoundingClientRect().bottom <= v.top) lo = m + 1; else hi = m; }
    const pg = pages[Math.min(lo, n - 1)];
    const p = num(pg);
    const q = p >= 0 && p < map.length ? map[p] : p;
    const t = document.getElementById('spg-' + q);
    if (!t) return;
    const r = pg.getBoundingClientRect();
    const f = Math.min(1, Math.max(0, (v.top - r.top) / Math.max(1, r.height)));
    const tr = t.getBoundingClientRect();
    o.scrollTop += (tr.top - o.getBoundingClientRect().top) + f * tr.height;
  };
  window.folitectaFollow = () => { const c = document.getElementById('canvas'); if (c) follow(c); };
  // At the first scroll event, then at most every 100 ms and once after the last.
  const waiting = new Map();
  document.addEventListener('scroll', e => {
    const c = e.target;
    if (!c || !c.classList || !c.classList.contains('canvas')) return;
    follow(c);
    if (waiting.has(c)) { waiting.set(c, true); return; }
    report(c);
    waiting.set(c, false);
    const later = () => { if (waiting.get(c)) { waiting.set(c, false); report(c); setTimeout(later, 100); } else waiting.delete(c); };
    setTimeout(later, 100);
  }, { capture: true, passive: true });
  // A page whose sharp picture has come stops drawing its small picture
  // under it (drawing it scaled up costs while scrolling).
  document.addEventListener('load', e => {
    const img = e.target;
    if (img && img.classList && img.classList.contains('page-img') && img.parentNode) img.parentNode.setAttribute('data-sharp', '');
  }, true);
  // A picture of a view that comes without one (refused or failed) is
  // asked again a little later, three times at most.
  document.addEventListener('error', e => {
    const img = e.target;
    if (!img || img.tagName !== 'IMG' || !img.closest || !img.closest('.canvas')) return;
    const src = img.getAttribute('src');
    if (!src) return;
    const base = src.replace(/[?&]retry=\d+$/, '');
    if (img.dataset.retryOf !== base) { img.dataset.retryOf = base; img.dataset.retries = '0'; }
    const n = +img.dataset.retries || 0;
    if (n >= 3) return;
    img.dataset.retries = String(n + 1);
    setTimeout(() => {
      if (!img.isConnected || img.getAttribute('src') !== src) return;
      img.setAttribute('src', base + (base.indexOf('?') >= 0 ? '&' : '?') + 'retry=' + (n + 1));
    }, 150 * (n + 1));
  }, true);
  // A view that is resized, or whose pages change size (zoom), reports too;
  // so does a view whose pages change (another page in the single page
  // layout, the pages of a far page mounted), once they are there.
  const sized = new ResizeObserver(entries => {
    const views = new Set(entries.map(en => en.target.closest('.canvas')).filter(Boolean));
    for (const c of views) report(c);
  });
  const queued = new Set();
  const later = c => { if (queued.has(c)) return; queued.add(c); soon(() => { queued.delete(c); report(c); }); };
  const watch = () => {
    for (const c of document.querySelectorAll('.canvas')) {
      if (!c.__foliWatched) { c.__foliWatched = true; sized.observe(c); report(c); }
      const pages = c.querySelector(':scope > .pages');
      if (pages && c.__foliPages !== pages) {
        c.__foliPages = pages;
        sized.observe(pages);
        new MutationObserver(recs => {
          for (const r of recs) {
            const t = r.target;
            if (t === pages || (t.classList && t.classList.contains('page-row'))) { later(c); return; }
          }
        }).observe(pages, { childList: true, subtree: true });
      }
    }
  };
  new MutationObserver(watch).observe(document.body, { childList: true, subtree: true });
  watch();
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
            "view" => {
                let num = |k: usize| m.get(k).and_then(|v| v.as_i64());
                let (Some(top), Some(first), Some(last)) = (num(2), num(3), num(4)) else {
                    continue;
                };
                // The other view of a comparison side by side shows the other document.
                let source = st
                    .read_active(|t| t.split.as_ref().filter(|_| !focused).and_then(|s| s.source))
                    .flatten();
                let source_pages = source.and_then(|id| {
                    st.docs
                        .peek()
                        .iter()
                        .find(|t| t.id == id)
                        .map(|t| t.doc.page_count())
                });
                if source.is_some() && source_pages.is_none() {
                    continue;
                }
                let Some((tab, pane, layout, page, n)) = st.read_active(|t| {
                    let n = t.doc.page_count();
                    let focus = t.split.as_ref().map(|s| s.focus.min(1)).unwrap_or(0) as usize;
                    match t.split.as_ref().filter(|_| !focused) {
                        Some(s) => (
                            s.source.unwrap_or(t.id),
                            1 - focus,
                            s.other.layout,
                            s.other.page,
                            source_pages.unwrap_or(n),
                        ),
                        None => (t.id, focus, t.layout, t.page, n),
                    }
                }) else {
                    continue;
                };
                if first < 0 || last < first || last as usize >= n {
                    continue;
                }
                let (first, last) = (first as usize, last as usize);
                // The pages the view holds with their content: those shown and
                // twice as many on each side (at least 4), moved only when the
                // pages shown come near an end of them.
                let span = last - first + 1;
                let margin = (2 * span).max(4);
                let held = (*st.windows.peek())[pane].filter(|w| w.0 == tab);
                let inside = held.is_some_and(|(_, a, b)| {
                    first >= a
                        && last <= b
                        && (a == 0 || first >= a + margin / 2)
                        && (b + 1 == n || last + margin / 2 <= b)
                });
                if !inside {
                    let a = first.saturating_sub(margin);
                    let b = (last + margin).min(n - 1);
                    st.windows.write_unchecked()[pane] = Some((tab, a, b));
                }
                // What the render threads draw first. (What they no longer need
                // is told by the views themselves, as they draw their pages.)
                let forward = top < 0 || top as usize >= page;
                if focused {
                    st.read_active(|t| t.render.set_focus(Some((first, last)), forward));
                }
                if top >= 0 && layout != Layout::Single && top as usize != page && (top as usize) < n {
                    let i = top as usize;
                    st.with_active(|t| match t.split.as_mut().filter(|_| !focused) {
                        Some(s) => s.other.page = i,
                        None => t.page = i,
                    });
                }
            }
            _ => {}
        }
    }
}

/// What each page of the view shows besides its picture, as one number a
/// page: the state of the document and of the view that its comments,
/// links, fields, selections, search results, guides and marks depend on.
/// A page is drawn again only when its number changes (or its other
/// properties), not at every change of the documents.
fn page_stamps(t: &crate::state::DocTab, count: usize) -> Vec<u64> {
    use std::hash::{Hash, Hasher};
    let mut common = std::collections::hash_map::DefaultHasher::new();
    (t.revision, t.view_gen, t.highlight_fields, t.highlight_required).hash(&mut common);
    t.highlight_kinds.hash(&mut common);
    for g in &t.guides {
        (g.page, g.horizontal, g.pos.to_bits()).hash(&mut common);
    }
    if let Some(run) = &t.spell {
        (run.list.len(), run.current, run.anchor, run.passed.len()).hash(&mut common);
        run.made_for.map(|m| (m.0, m.1, m.2)).hash(&mut common);
    }
    (t.acc.order, t.search.current).hash(&mut common);
    let common = common.finish();
    let mut pages: Vec<std::collections::hash_map::DefaultHasher> = (0..count)
        .map(|_| {
            let mut h = std::collections::hash_map::DefaultHasher::new();
            common.hash(&mut h);
            h
        })
        .collect();
    let mut on = |page: usize, f: &dyn Fn(&mut std::collections::hash_map::DefaultHasher)| {
        if let Some(h) = pages.get_mut(page) {
            f(h);
        }
    };
    if let Some(s) = t.text_sel {
        on(s.page, &|h| (1u8, s.anchor, s.head).hash(h));
    }
    if let Some((page, list)) = t.chosen_annots() {
        on(page, &|h| (2u8, &list).hash(h));
    }
    if let Some((page, i)) = t.image_sel {
        on(page, &|h| (3u8, i).hash(h));
    }
    if let Some((page, r)) = t.flash {
        on(page, &|h| (4u8, r.map(f32::to_bits)).hash(h));
    }
    if let Some((page, rects)) = &t.reading {
        on(*page, &|h| {
            7u8.hash(h);
            for r in rects {
                r.map(f32::to_bits).hash(h);
            }
        });
    }
    for (i, hit) in t.search.hits.iter().enumerate() {
        on(hit.page, &|h| {
            (5u8, i, hit.start, hit.end, hit.word).hash(h);
            for r in &hit.rects {
                r.map(f32::to_bits).hash(h);
            }
        });
    }
    pages.into_iter().map(|h| h.finish()).collect()
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
    use_drop(move || {
        st.peek_active(|t| t.render.set_held_view(usize::from(pane.min(1)), None));
    });
    let dpr = (st.dpr)();
    let tool = (st.tool)();
    let rulers = option_on(st, "view.showRulers");
    let Some(split) = st.read_active(|t| t.split.clone()) else {
        return rsx! {};
    };
    let split_on = split.is_some();
    let focused = split.as_ref().is_none_or(|s| s.focus == pane);
    // The other document of a comparison side by side, for the view not focused.
    let source = split.as_ref().filter(|_| !focused).and_then(|s| s.source);
    let Some((tab_id, revs, count, stamps)) = read_tab(st, source, |t| {
        let count = t.doc.page_count();
        (t.id, t.page_shown.clone(), count, page_stamps(t, count))
    }) else {
        return rsx! {};
    };
    let (view, zoom) = match split.as_ref().filter(|_| !focused) {
        Some(s) => (
            s.other,
            read_tab(st, source, |t| {
                commands::zoom_for(t, &s.other, s.other_size, rulers)
            })
            .unwrap_or(1.0),
        ),
        None => match st.read_active(|t| t.view_state()) {
            Some(v) => (v, current_zoom(st)),
            None => return rsx! {},
        },
    };
    let (page, layout, rotation) = (view.page, view.layout, view.rotation);
    // The scale the pictures are drawn at. After a change of zoom the pages
    // take their new size at once and keep their pictures, scaled, until the
    // zoom has stayed the same for 90 ms: then the pictures of the new
    // scale are asked for, once (not at every step of a wheel or a drag).
    // Pictures with up to a quarter more pixels than needed are kept.
    let wanted = render_scale_for(zoom, dpr);
    let measured = use_signal(|| false);
    let shown_scale = use_hook(|| std::rc::Rc::new(std::cell::Cell::new(0u32)));
    let settle_gen = use_hook(|| std::rc::Rc::new(std::cell::Cell::new(0u64)));
    let mut settled = use_signal(|| 0u64);
    let _ = settled();
    let current = shown_scale.get();
    let render_scale = if !measured() {
        // The size of the view is not known yet: no picture is asked for.
        0
    } else if current == 0 || (current >= wanted && current * 4 <= wanted * 5) {
        if current == 0 {
            shown_scale.set(wanted);
            wanted
        } else {
            current
        }
    } else {
        let g = settle_gen.get() + 1;
        settle_gen.set(g);
        let (shown_scale, settle_gen) = (shown_scale.clone(), settle_gen.clone());
        spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(90)).await;
            if settle_gen.get() == g {
                shown_scale.set(wanted);
                settled += 1;
            }
        });
        current
    };
    // The pages held with their content: those the view shows and some on
    // each side (as the view tells); the others are empty boxes of their size.
    let windows = (st.windows)();
    let (wa, wb) = match windows[usize::from(pane.min(1))].filter(|w| w.0 == tab_id) {
        _ if layout == Layout::Single => (0, count.saturating_sub(1)),
        Some((_, a, b)) if page >= a && page <= b => (a, b),
        _ => (page.saturating_sub(4), (page + 4).min(count.saturating_sub(1))),
    };
    let sizes: Vec<(f32, f32)> = read_tab(st, source, |t| {
        (0..count)
            .map(|p| {
                let s = t.page_sizes.get(p);
                let (w, h) = s.map(|s| (s.width, s.height)).unwrap_or((595.0, 842.0));
                if rotation % 180 != 0 {
                    (h * zoom, w * zoom)
                } else {
                    (w * zoom, h * zoom)
                }
            })
            .collect()
    })
    .unwrap_or_default();
    let preview = crate::ui::panes::thumb_render_scale(st);
    let labels = read_tab(st, source, |t| t.doc.page_labels()).unwrap_or_default();
    let gaps = option_on(st, "view.layout.showGaps");
    let r2l = option_on(st, "view.layout.r2l");
    let cover = option_on(st, "view.layout.showCover");
    let horizontal = option_on(st, "view.layout.ribbon") && layout != Layout::Single;
    let rows = page_rows(layout, page, count, cover, horizontal, r2l);
    // The render threads learn the pages this view holds with their pictures
    // now, as it draws them and before their pictures are asked for (a jump
    // to a far page, another page in the single page layout), not from the
    // view's later report: a picture of a page held is never refused.
    let held = rows
        .iter()
        .flatten()
        .copied()
        .filter(|p| *p >= wa && *p <= wb)
        .fold(None, |acc: Option<(usize, usize)>, p| match acc {
            Some((a, b)) => Some((a.min(p), b.max(p))),
            None => Some((p, p)),
        });
    match source {
        Some(id) => {
            if let Some(t) = st.docs.peek().iter().find(|t| t.id == id) {
                t.render.set_held_view(usize::from(pane.min(1)), held);
            }
        }
        None => {
            st.peek_active(|t| t.render.set_held_view(usize::from(pane.min(1)), held));
        }
    }
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
            // Followed by the focused view (see CANVAS_EVENTS_SCRIPT).
            "data-follow": if source.is_some() { "1" } else { "" },
            onresize: move |evt| {
                if let Ok(size) = evt.data().get_content_box_size() {
                    let size = (size.width, size.height);
                    if !*measured.peek() {
                        let mut measured = measured;
                        measured.set(true);
                    }
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
                            if p < wa || p > wb {
                                PagePlaceholder { key: "e{tab_id}-{p}", id: if focused { format!("pg-{p}") } else { format!("spg-{p}") }, page: p, width: sizes.get(p).map(|s| s.0).unwrap_or(0.0), height: sizes.get(p).map(|s| s.1).unwrap_or(0.0), label: labels.get(p).cloned().unwrap_or_default() }
                            } else if focused {
                                PageView { key: "{tab_id}-{p}", tab_id, page: p, rev: revs.get(p).copied().unwrap_or_default(), scale: render_scale, preview, zoom, current: p == page, tool, view_rotation: rotation, label: labels.get(p).cloned().unwrap_or_default(), stamp: stamps.get(p).copied().unwrap_or_default() }
                            } else {
                                StillPage { key: "s{tab_id}-{p}", tab_id, page: p, rev: revs.get(p).copied().unwrap_or_default(), scale: render_scale, zoom, view_rotation: rotation, label: labels.get(p).cloned().unwrap_or_default() }
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
    // Read without following the documents: the page changes with its properties.
    let Some(size) = st
        .docs
        .peek()
        .iter()
        .find(|t| t.id == tab_id)
        .and_then(|t| t.page_sizes.get(page).copied())
    else {
        return rsx! {};
    };
    let marks = page::compare_marks(st, tab_id, page, size.width);
    let mut filter = String::new();
    if option_on(st, "view.darkPageMode") {
        filter.push_str("invert(1) hue-rotate(180deg) ");
    }
    if option_on(st, "view.greyPreview") && !option_on(st, "view.overrideDocumentColors") {
        filter.push_str("grayscale(1) ");
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
                img { class: "page-img", src: (scale > 0).then(|| format!("/page/{tab_id}/{page}/{rev}/{scale}.png?lane=view")), alt: "", decoding: "async", draggable: "false", style: "{img_style}" }
                for (i, (class, r)) in marks.iter().enumerate() {
                    div { key: "c{i}", class: "{class}", style: "left:{r[0] * zoom}px;top:{r[1] * zoom}px;width:{((r[2] - r[0]) * zoom).max(2.0)}px;height:{((r[3] - r[1]) * zoom).max(2.0)}px" }
                }
            }
            div { class: "page-label", "{number}" }
        }
    }
}

/// Read the tab `id` (None: the active one).
fn read_tab<T>(st: AppState, id: Option<u64>, f: impl FnOnce(&crate::state::DocTab) -> T) -> Option<T> {
    match id {
        None => st.read_active(f),
        Some(id) => st.docs.read().iter().find(|t| t.id == id).map(f),
    }
}

/// The scale (milli) of the pictures of a view at `zoom` on a pixel ratio of
/// `dpr`.
pub(crate) fn render_scale_for(zoom: f32, dpr: f64) -> u32 {
    ((zoom * dpr as f32) * 1000.0).round().clamp(50.0, 6000.0) as u32
}

/// A page of a view that is not near what the view shows: an empty box of
/// the page's size, so the pages keep their places, with its number.
#[component]
fn PagePlaceholder(id: String, page: usize, width: f32, height: f32, label: String) -> Element {
    let number = if !label.is_empty() && label != (page + 1).to_string() {
        format!("{label} ({})", page + 1)
    } else {
        (page + 1).to_string()
    };
    rsx! {
        div { id: "{id}", class: "page placeholder", "data-page": "{page}", style: "width:{width}px;height:{height}px",
            div { class: "page-label", "{number}" }
        }
    }
}
