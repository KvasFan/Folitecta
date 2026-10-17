//! Document tabs and the page canvas: rendering, tools, selections and overlays.

use dioxus::html::geometry::WheelDelta;
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
use crate::ui::form_fill::{FieldInput, fill_field};
use crate::ui::welcome::Welcome;

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
    rsx! {
        div { class: "canvas-wrap",
            DocTabs {}
            if info_bar { DocInfoBar {} }
            Canvas {}
            Loupe {}
            if option_on(st, "view.panzoom") { PanZoomWindow {} }
        }
    }
}

/// Visible part of the current page as fractions of its size: x0, y0, x1, y1.
const VIEWPORT_JS: &str = "const c = document.getElementById('canvas'); const el = document.getElementById('pg-PAGE'); if (!c || !el) return null; const r = el.getBoundingClientRect(), v = c.getBoundingClientRect(); const f = (a, b) => Math.min(1, Math.max(0, a / b)); return [f(v.left - r.left, r.width), f(v.top - r.top, r.height), f(v.right - r.left, r.width), f(v.bottom - r.top, r.height)];";

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
                onmousemove: move |e| {
                    if dragging() {
                        let p = e.element_coordinates();
                        move_to(p.x / box_w as f64, p.y / box_h as f64);
                    }
                },
                onmouseup: move |_| dragging.set(false),
                onmouseleave: move |_| dragging.set(false),
                img { src: "/page/{tab}/{page}/{rev}/{scale}.png", alt: "", draggable: "false", width: "{box_w}", height: "{box_h}" }
                if let Some(style) = rect {
                    div { class: "panzoom-view", style: "{style}" }
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
    let mag = (zoom * 3.0).min(8.0);
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

#[component]
fn Canvas() -> Element {
    let st = use_context::<AppState>();
    use_hook(|| {
        spawn(async move {
            let _ = document::eval(PAN_SCRIPT).await;
        });
    });
    let zoom = current_zoom(st);
    let dpr = (st.dpr)();
    let tool = (st.tool)();
    let Some((tab_id, page, rev, layout, count, rotation)) = st.read_active(|t| {
        (
            t.id,
            t.page,
            t.view_rev(),
            t.layout,
            t.doc.page_count(),
            t.view_rotation,
        )
    }) else {
        return rsx! {};
    };
    let render_scale = ((zoom * dpr as f32) * 1000.0).round().clamp(50.0, 6000.0) as u32;
    let labels = st.read_active(|t| t.doc.page_labels()).unwrap_or_default();
    let gaps = option_on(st, "view.layout.showGaps");
    let r2l = option_on(st, "view.layout.r2l");
    let cover = option_on(st, "view.layout.showCover");
    let rulers = option_on(st, "view.showRulers");
    let horizontal = option_on(st, "view.layout.ribbon") && layout != Layout::Single;
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
    let tool_class = match tool {
        Tool::Hand => "tool-hand",
        Tool::Select | Tool::TextHighlight | Tool::TextUnderline | Tool::TextStrike => "tool-text",
        Tool::Zoom => "tool-zoom",
        Tool::Loupe => "tool-loupe",
        Tool::EditAnnots => "tool-edit",
        _ => "tool-draw",
    };
    let mut scroll_pending = use_signal(|| false);
    let mut pages_class = String::from("pages");
    if !gaps {
        pages_class.push_str(" nogap");
    }
    if rulers {
        pages_class.push_str(" with-rulers");
    }
    if horizontal {
        pages_class.push_str(" horizontal");
    }

    rsx! {
        div {
            id: "canvas",
            class: "canvas {tool_class}",
            onresize: move |evt| {
                if let Ok(size) = evt.data().get_content_box_size() {
                    st.set_canvas_size((size.width, size.height));
                }
            },
            onwheel: move |evt| {
                if evt.modifiers().ctrl() {
                    evt.prevent_default();
                    let dy = match evt.delta() {
                        WheelDelta::Pixels(v) => v.y,
                        WheelDelta::Lines(v) => v.y * 40.0,
                        WheelDelta::Pages(v) => v.y * 400.0,
                    };
                    let z = current_zoom(st);
                    let next = if dy < 0.0 { z * 1.1 } else { z / 1.1 };
                    st.with_active(|t| t.zoom = Zoom::Fixed(next.clamp(0.1, 8.0)));
                }
            },
            onmouseleave: move |_| {
                if st.loupe.peek().is_some() { st.set_loupe(None); }
                if st.pointer.peek().is_some() { st.set_pointer(None); }
            },
            onscroll: move |_| {
                if layout == Layout::Single || *scroll_pending.peek() {
                    return;
                }
                scroll_pending.set(true);
                spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_millis(90)).await;
                    let js = if horizontal {
                        "const c=document.getElementById('canvas'); if(!c) return -1; const r=c.getBoundingClientRect(); let best=-1,bd=1e9; c.querySelectorAll('.page').forEach(el=>{const d=Math.abs(el.getBoundingClientRect().left-r.left-24); if(d<bd){bd=d;best=parseInt(el.dataset.page);}}); return best;"
                    } else {
                        "const c=document.getElementById('canvas'); if(!c) return -1; const r=c.getBoundingClientRect(); let best=-1,bd=1e9; c.querySelectorAll('.page').forEach(el=>{const d=Math.abs(el.getBoundingClientRect().top-r.top-24); if(d<bd){bd=d;best=parseInt(el.dataset.page);}}); return best;"
                    };
                    if let Ok(v) = document::eval(js).await {
                        if let Some(i) = v.as_i64().filter(|i| *i >= 0) {
                            let i = i as usize;
                            if st.read_active(|t| t.page).unwrap_or(0) != i {
                                st.with_active(|t| t.page = i);
                            }
                        }
                    }
                    scroll_pending.set(false);
                });
            },
            div { class: "{pages_class}",
                for (ri, row) in rows.into_iter().enumerate() {
                    div { class: "page-row", key: "row-{ri}",
                        for p in row {
                            PageView { key: "{tab_id}-{p}", tab_id, page: p, rev, scale: render_scale, zoom, current: p == page, tool, view_rotation: rotation, label: labels.get(p).cloned().unwrap_or_default() }
                        }
                    }
                }
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
struct Drag {
    start: (f64, f64),
    current: (f64, f64),
}

#[derive(Clone, Copy, PartialEq)]
enum AnnotDragKind {
    Move,
    /// Corner: 0 top-left, 1 top-right, 2 bottom-left, 3 bottom-right (display space).
    Resize(u8),
}

#[derive(Clone, Copy, PartialEq)]
struct AnnotDrag {
    index: usize,
    kind: AnnotDragKind,
    start_client: (f64, f64),
    cur_client: (f64, f64),
    rect: [f32; 4],
}

/// Client-space delta to local (unrotated page) CSS pixels.
fn client_to_local(dx: f64, dy: f64, view_rotation: i32) -> (f64, f64) {
    match view_rotation {
        90 => (dy, -dx),
        180 => (-dx, -dy),
        270 => (-dy, dx),
        _ => (dx, dy),
    }
}

/// Display point (y down) to PDF user space.
fn to_pdf(size: &PageSize, x: f32, y: f32) -> (f32, f32) {
    display_to_pdf(size, x, size.height - y)
}

fn rect_to_pdf(size: &PageSize, r: [f32; 4]) -> [f32; 4] {
    folitecta_core::textops::display_rect_to_pdf(size, r)
}

fn dragged_rect(d: &AnnotDrag, zoom: f32, view_rotation: i32) -> [f32; 4] {
    let (dx, dy) = client_to_local(
        d.cur_client.0 - d.start_client.0,
        d.cur_client.1 - d.start_client.1,
        view_rotation,
    );
    let (dx, dy) = (dx as f32 / zoom, dy as f32 / zoom);
    let mut r = d.rect;
    match d.kind {
        AnnotDragKind::Move => {
            r[0] += dx;
            r[2] += dx;
            r[1] += dy;
            r[3] += dy;
        }
        AnnotDragKind::Resize(c) => {
            if c == 0 || c == 2 {
                r[0] += dx;
            } else {
                r[2] += dx;
            }
            if c == 0 || c == 1 {
                r[1] += dy;
            } else {
                r[3] += dy;
            }
        }
    }
    [r[0].min(r[2]), r[1].min(r[3]), r[0].max(r[2]), r[1].max(r[3])]
}

/// Snap a display point to the grid and/or guides when snapping is on.
fn snap(st: AppState, page: usize, x: f32, y: f32, zoom: f32) -> (f32, f32) {
    if !option_on(st, "view.snap") {
        return (x, y);
    }
    let (mut sx, mut sy) = (x, y);
    if option_on(st, "view.snap.toGrid") {
        let step = grid_step(st);
        sx = (x / step).round() * step;
        sy = (y / step).round() * step;
    }
    if option_on(st, "view.snap.toGuides") {
        let tol = 6.0 / zoom;
        let guides = st.read_active(|t| t.guides.clone()).unwrap_or_default();
        for g in guides.iter().filter(|g| g.page.is_none() || g.page == Some(page)) {
            if g.horizontal && (y - g.pos).abs() < tol {
                sy = g.pos;
            }
            if !g.horizontal && (x - g.pos).abs() < tol {
                sx = g.pos;
            }
        }
    }
    (sx, sy)
}

fn grid_step(st: AppState) -> f32 {
    st.ui
        .read()
        .value("grid.step")
        .and_then(|v| v.parse().ok())
        .filter(|v: &f32| *v > 1.0)
        .unwrap_or(28.3465)
}

/// Create a form field from a drag (display rectangle `d`), select it and show its properties.
fn create_field(st: AppState, page: usize, size: &PageSize, tool: Tool, d: [f32; 4], tiny: bool) {
    use folitecta_core::{FieldProps, NewField};
    let Some(kind) = tool.new_field() else { return };
    let (dw, dh) = match kind {
        NewField::Text | NewField::ComboBox => (160.0, 22.0),
        NewField::CheckBox | NewField::RadioButton => (14.0, 14.0),
        NewField::ListBox => (160.0, 64.0),
        NewField::PushButton => (90.0, 24.0),
        NewField::Signature => (180.0, 48.0),
    };
    let rect = if tiny {
        rect_to_pdf(size, [d[0], d[1], d[0] + dw, d[1] + dh])
    } else {
        rect_to_pdf(size, d)
    };
    let mut props = FieldProps::default();
    match kind {
        NewField::RadioButton => {
            // Join the group of the selected radio button, else the group of this tool session.
            let selected_group = st
                .read_active(|t| {
                    let (p, i) = t.selected_annot?;
                    let wid = t
                        .doc
                        .annotations(p)
                        .ok()?
                        .into_iter()
                        .find(|a| a.index == i)?
                        .id?;
                    t.doc
                        .form_fields()
                        .into_iter()
                        .find(|f| {
                            f.kind == folitecta_core::FieldKind::RadioButton
                                && t.doc.field_widgets(f.id).contains(&wid)
                        })
                        .map(|f| f.name)
                })
                .flatten();
            let group = selected_group.or_else(|| st.radio_group.peek().clone());
            let existing = group.as_ref().and_then(|g| {
                st.read_active(|t| {
                    t.doc
                        .form_fields()
                        .into_iter()
                        .find(|f| &f.name == g)
                        .map(|f| t.doc.field_widgets(f.id).len())
                })
                .flatten()
            });
            match (group, existing) {
                (Some(g), Some(n)) => {
                    props.name = g;
                    props.export_value = format!("Choice{}", n + 1);
                }
                _ => props.export_value = "Choice1".into(),
            }
        }
        NewField::ComboBox | NewField::ListBox => {
            props.options = (1..=3)
                .map(|i| format!("{} {i}", t(st.lang(), "field.option")))
                .collect()
        }
        NewField::PushButton => props.caption = t(st.lang(), "field.button_caption"),
        _ => {}
    }
    match st.with_active(|t| t.doc.add_form_field(page, kind, rect, &props)) {
        Some(Ok(widget)) => {
            let info = st
                .read_active(|t| {
                    let idx = t
                        .doc
                        .annotations(page)
                        .ok()?
                        .into_iter()
                        .find(|a| a.id == Some(widget))
                        .map(|a| a.index)?;
                    let name = t
                        .doc
                        .form_fields()
                        .into_iter()
                        .find(|f| t.doc.field_widgets(f.id).contains(&widget))
                        .map(|f| f.name)?;
                    Some((idx, name))
                })
                .flatten();
            if let Some((idx, name)) = info {
                if kind == NewField::RadioButton {
                    st.set_radio_group(Some(name));
                }
                st.with_active(|t| t.selected_annot = Some((page, idx)));
            }
            files::set_window_title(st);
            if !option_on(st, "tool.keepSelected") {
                st.set_tool(Tool::EditFields);
            }
        }
        Some(Err(e)) => st.error(e),
        None => {}
    }
}

/// Finish the shape drawn with a multi-click tool (polygon, polyline, cloud,
/// perimeter, area). Called on double click and on Enter.
pub fn finish_drawing(st: AppState) {
    let Some((page, pts)) = st.drawing.peek().clone() else {
        return;
    };
    st.set_drawing(None);
    let tool = *st.tool.peek();
    let Some(size) = st.read_active(|t| t.page_sizes.get(page).copied()).flatten() else {
        return;
    };
    // A double click adds the same point twice.
    let mut pts = pts;
    pts.dedup_by(|a, b| (a.0 - b.0).abs() < 0.5 && (a.1 - b.1).abs() < 0.5);
    let pdf: Vec<(f32, f32)> = pts.iter().map(|(x, y)| to_pdf(&size, *x, *y)).collect();
    let author = st.ui.peek().author.clone();
    let style = |kind: &str| crate::styles::default_style(st, kind);
    let scale = crate::ui::dialogs::measure_scale(st);
    let r = st.with_active(|t| match tool {
        Tool::Polygon => t
            .doc
            .add_polyline(page, &pdf, true, &style("Polygon"), &author, "")
            .map(|_| ()),
        Tool::Polyline => t
            .doc
            .add_polyline(page, &pdf, false, &style("PolyLine"), &author, "")
            .map(|_| ()),
        Tool::Cloud => t.doc.add_cloud(page, &pdf, &style("Cloud"), &author).map(|_| ()),
        Tool::Perimeter => t
            .doc
            .add_measurement(
                page,
                folitecta_core::MeasureKind::Perimeter,
                &pdf,
                &style("Perimeter"),
                &scale,
                &author,
            )
            .map(|_| ()),
        Tool::Area => t
            .doc
            .add_measurement(
                page,
                folitecta_core::MeasureKind::Area,
                &pdf,
                &style("Area"),
                &scale,
                &author,
            )
            .map(|_| ()),
        _ => Ok(()),
    });
    match r {
        Some(Err(e)) => st.error(e),
        Some(Ok(())) => after_create(st),
        None => {}
    }
}

fn after_create(st: AppState) {
    files::set_window_title(st);
    if !option_on(st, "tool.keepSelected") {
        st.set_tool(Tool::Hand);
    }
}

/// Class of a search hit: the current one, and one color per word of the query.
fn hit_class(current: bool, word: usize, colored: bool) -> String {
    let mut c = String::from("hit");
    if current {
        c.push_str(" current");
    }
    if colored {
        c.push_str(&format!(" w{}", word % 5));
    }
    c
}

#[component]
fn PageView(
    tab_id: u64,
    page: usize,
    rev: u64,
    scale: u32,
    zoom: f32,
    current: bool,
    tool: Tool,
    view_rotation: i32,
    label: String,
) -> Element {
    let st = use_context::<AppState>();
    let mut drag = use_signal(|| None::<Drag>);
    let mut ink: Signal<Vec<(f64, f64)>> = use_signal(Vec::new);
    let mut annot_drag = use_signal(|| None::<AnnotDrag>);
    let mut sel_drag = use_signal(|| false);
    let mut hover = use_signal(|| None::<(f32, f32)>);

    let show_hits = matches!(tool, Tool::Hand | Tool::EditAnnots);
    let edit_fields = tool == Tool::EditFields;
    let fill_forms = matches!(tool, Tool::Hand | Tool::Select);
    let page_label = if !label.is_empty() && label != (page + 1).to_string() {
        format!("{label} ({})", page + 1)
    } else {
        (page + 1).to_string()
    };
    let highlight_links = option_on(st, "view.highlightLinks");
    let show_guides = option_on(st, "view.showGuides");
    let show_grid = option_on(st, "view.showGrid");
    let rulers = option_on(st, "view.showRulers");
    let transparent = option_on(st, "view.showTranspGrid");
    let dark = option_on(st, "view.darkPageMode");
    let contrast = option_on(st, "view.overrideDocumentColors");
    let track_pointer = option_on(st, "view.showPageMeasureInfo");
    let show_size = option_on(st, "view.showPageSizePopup");
    let colored = option_on(st, "document.find.coloredResults");

    struct View {
        size: PageSize,
        annots: Vec<(usize, String, [f32; 4], String)>,
        links: Vec<[f32; 4]>,
        fields: Vec<[f32; 4]>,
        hits: Vec<(bool, [f32; 4], usize)>,
        sel: Vec<[f32; 4]>,
        guides: Vec<(bool, f32)>,
        selected: Option<usize>,
        widgets: Vec<(folitecta_core::PageWidget, [f32; 4])>,
        popups: Vec<PopupView>,
        flash: Option<[f32; 4]>,
    }
    let Some(view) = st
        .read_active(|t| {
            let size = t.page_sizes.get(page).copied()?;
            let infos = t.doc.page_comment_infos(page);
            let filtered: std::collections::HashSet<usize> = infos
                .iter()
                .filter(|c| c.reply_to.is_none() && !c.is_state && !t.comment_filter.shows(c))
                .map(|c| c.index)
                .collect();
            let popups = popup_views(&infos, &filtered, &size);
            let annots = if show_hits || edit_fields || t.selected_annot.map(|(p, _)| p) == Some(page) {
                t.doc
                    .annotations(page)
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|a| {
                        if edit_fields {
                            a.subtype == "Widget"
                        } else {
                            !["Link", "Widget", "Popup"].contains(&a.subtype.as_str())
                                && !a.hidden
                                && !a.is_reply
                                && !filtered.contains(&a.index)
                        }
                    })
                    .map(|a| {
                        (
                            a.index,
                            a.subtype.clone(),
                            pdf_rect_to_display(&size, a.rect),
                            a.contents.clone(),
                        )
                    })
                    .collect()
            } else {
                Vec::new()
            };
            let links = if highlight_links {
                t.doc
                    .links(page)
                    .unwrap_or_default()
                    .into_iter()
                    .map(|l| pdf_rect_to_display(&size, l.rect))
                    .collect()
            } else {
                Vec::new()
            };
            let fields = if t.highlight_fields {
                t.doc
                    .form_fields()
                    .into_iter()
                    .filter(|f| f.page == Some(page))
                    .filter_map(|f| f.rect)
                    .map(|r| pdf_rect_to_display(&size, r))
                    .collect()
            } else {
                Vec::new()
            };
            let hits = t
                .search
                .hits
                .iter()
                .enumerate()
                .filter(|(_, h)| h.page == page)
                .flat_map(|(i, h)| h.rects.iter().map(move |r| (i == t.search.current, *r, h.word)))
                .collect();
            let sel = match t.text_sel {
                Some(s) if s.page == page && !s.is_empty() => {
                    let (a, b) = s.range();
                    t.doc
                        .text_layers()
                        .get(page)
                        .map(|l| l.range_rects(a, b))
                        .unwrap_or_default()
                }
                _ => Vec::new(),
            };
            let guides = t
                .guides
                .iter()
                .filter(|g| g.page.is_none() || g.page == Some(page))
                .map(|g| (g.horizontal, g.pos))
                .collect();
            let selected = t.selected_annot.filter(|(p, _)| *p == page).map(|(_, i)| i);
            let widgets = if fill_forms && t.doc.has_form() {
                t.doc
                    .page_widgets(page)
                    .into_iter()
                    .map(|w| {
                        let r = pdf_rect_to_display(&size, w.rect);
                        (w, r)
                    })
                    .collect()
            } else {
                Vec::new()
            };
            Some(View {
                size,
                annots,
                links,
                fields,
                hits,
                sel,
                guides,
                selected,
                widgets,
                popups,
                flash: t.flash.filter(|(p, _)| *p == page).map(|(_, r)| r),
            })
        })
        .flatten()
    else {
        return rsx! {};
    };
    let size = view.size;
    let size_text = format!(
        "{:.0} × {:.0} mm ({:.2} × {:.2} in)",
        size.width * 25.4 / 72.0,
        size.height * 25.4 / 72.0,
        size.width / 72.0,
        size.height / 72.0
    );
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
    let src = if transparent {
        format!("/page/{tab_id}/{page}/{rev}/{scale}.png?bg=0")
    } else {
        format!("/page/{tab_id}/{page}/{rev}/{scale}.png")
    };
    let mut img_filter = String::new();
    if dark {
        img_filter.push_str("invert(1) hue-rotate(180deg) ");
    }
    if contrast {
        img_filter.push_str("grayscale(1) contrast(1.6) ");
    }
    let z = zoom;
    let css = |r: [f32; 4]| -> String {
        format!(
            "left:{}px;top:{}px;width:{}px;height:{}px",
            r[0] * z,
            r[1] * z,
            ((r[2] - r[0]) * z).max(2.0),
            ((r[3] - r[1]) * z).max(2.0)
        )
    };

    // ---- finishing a creation drag -------------------------------------------------
    let finish_create = move |d: Drag, points: Vec<(f64, f64)>| {
        let (x0, y0) = (
            d.start.0.min(d.current.0) as f32 / z,
            d.start.1.min(d.current.1) as f32 / z,
        );
        let (x1, y1) = (
            d.start.0.max(d.current.0) as f32 / z,
            d.start.1.max(d.current.1) as f32 / z,
        );
        let tiny = (x1 - x0) < 3.0 && (y1 - y0) < 3.0;
        let author = st.ui.read().author.clone();
        let rect = rect_to_pdf(&size, [x0, y0, x1, y1]);
        let start_pdf = to_pdf(&size, d.start.0 as f32 / z, d.start.1 as f32 / z);
        let point = to_pdf(&size, d.current.0 as f32 / z, d.current.1 as f32 / z);
        let style = |kind: &str| crate::styles::default_style(st, kind);
        let path_pdf = || -> Vec<(f32, f32)> {
            points
                .iter()
                .map(|(x, y)| to_pdf(&size, *x as f32 / z, *y as f32 / z))
                .collect()
        };
        let result: Option<folitecta_core::Result<()>> = match tool {
            Tool::Note => {
                st.set_dialog(Dialog::Note {
                    page,
                    x: point.0,
                    y: point.1,
                });
                None
            }
            Tool::TextBox | Tool::Typewriter => {
                let r = if tiny {
                    rect_to_pdf(&size, [x0, y0, x0 + 220.0, y0 + 40.0])
                } else {
                    rect
                };
                st.set_dialog(Dialog::TextBox {
                    page,
                    rect: r,
                    border: tool == Tool::TextBox,
                });
                None
            }
            Tool::Callout => {
                // The press is the pointed spot, the release places the text box.
                let (bx, by) = (d.current.0 as f32 / z, d.current.1 as f32 / z);
                let (bx, by) = if tiny { (bx + 40.0, by - 60.0) } else { (bx, by) };
                let r = rect_to_pdf(&size, [bx, by - 20.0, bx + 170.0, by + 30.0]);
                st.set_dialog(Dialog::Callout {
                    page,
                    tip: start_pdf,
                    rect: r,
                });
                None
            }
            Tool::Link if !tiny => {
                st.set_dialog(Dialog::Link { page, rect });
                None
            }
            Tool::SignArea if !tiny => {
                crate::commands::signatures::finish_sign_area(st, page, rect);
                None
            }
            Tool::PlaceSignature => {
                crate::commands::signs::place_at(st, page, rect, tiny);
                None
            }
            Tool::Rect if !tiny => st.with_active(|t| {
                t.doc
                    .add_shape_styled(page, Shape::Square, rect, &style("Square"), &author, "")
                    .map(|_| ())
            }),
            Tool::Ellipse if !tiny => st.with_active(|t| {
                t.doc
                    .add_shape_styled(page, Shape::Circle, rect, &style("Circle"), &author, "")
                    .map(|_| ())
            }),
            Tool::Highlight if !tiny => {
                let q = [
                    rect[0], rect[3], rect[2], rect[3], rect[0], rect[1], rect[2], rect[1],
                ];
                let c = style("HighlightArea").color.unwrap_or([1.0, 0.9, 0.2]);
                st.with_active(|t| {
                    t.doc
                        .add_text_markup(page, Markup::Highlight, &[q], c, &author, "")
                        .map(|_| ())
                })
            }
            Tool::Line | Tool::Arrow if !tiny => {
                let mut s = style("Line");
                if tool == Tool::Arrow && s.line_end == "None" {
                    s.line_end = "OpenArrow".into();
                }
                st.with_active(|t| {
                    t.doc
                        .add_polyline(page, &[start_pdf, point], false, &s, &author, "")
                        .map(|_| ())
                })
            }
            Tool::Distance if !tiny => {
                let scale = crate::ui::dialogs::measure_scale(st);
                st.with_active(|t| {
                    t.doc
                        .add_measurement(
                            page,
                            folitecta_core::MeasureKind::Distance,
                            &[start_pdf, point],
                            &style("Distance"),
                            &scale,
                            &author,
                        )
                        .map(|_| ())
                })
            }
            Tool::Ink | Tool::FreeHighlight if points.len() > 1 => {
                let s = style(if tool == Tool::Ink { "Ink" } else { "FreeHighlight" });
                let path = path_pdf();
                st.with_active(|t| t.doc.add_ink_styled(page, &[path], &s, &author).map(|_| ()))
            }
            Tool::Eraser => {
                let path = path_pdf();
                // Radius of 6 screen pixels, in page points.
                let radius = (6.0 / z).max(1.5);
                match st.with_active(|t| t.doc.erase_ink(page, &path, radius)) {
                    Some(Ok(_)) => files::set_window_title(st),
                    Some(Err(e)) => st.error(e),
                    None => {}
                }
                None
            }
            Tool::FileAttachment => {
                files::pick_page_attachment(st, page, point);
                None
            }
            Tool::Redact if !tiny => st.with_active(|t| t.doc.add_redaction(page, rect, &author).map(|_| ())),
            Tool::Stamp => {
                let image = if st.ui.read().value("stamp.kind") == Some("image") {
                    st.pending_image.peek().clone()
                } else {
                    None
                };
                match image {
                    Some(data) => {
                        let r = if tiny {
                            let (iw, ih) = folitecta_core::Document::image_size(&data)
                                .map(|(w, h)| (w as f32, h as f32))
                                .unwrap_or((160.0, 80.0));
                            let s = (160.0 / iw.max(ih)).min(1.0);
                            let (w, h) = ((iw * s).max(8.0), (ih * s).max(8.0));
                            rect_to_pdf(&size, [x0 - w / 2.0, y0 - h / 2.0, x0 + w / 2.0, y0 + h / 2.0])
                        } else {
                            rect
                        };
                        st.with_active(|t| t.doc.add_image_stamp(page, r, data, &author, "").map(|_| ()))
                    }
                    None => {
                        let text = st.ui.read().value("stamp.text").unwrap_or("APPROVED").to_string();
                        let color =
                            crate::util::parse_color(st.ui.read().value("stamp.color").unwrap_or("#c81e1e"));
                        let r = if tiny {
                            let w = (folitecta_core::util::helvetica_width(&text, 20.0) + 30.0).max(90.0);
                            rect_to_pdf(&size, [x0 - w / 2.0, y0 - 22.0, x0 + w / 2.0, y0 + 22.0])
                        } else {
                            rect
                        };
                        st.with_active(|t| t.doc.add_stamp(page, r, &text, color, &author).map(|_| ()))
                    }
                }
            }
            Tool::Snapshot if !tiny => {
                let png = st.read_active(|t| t.doc.render_region_png(page, [x0, y0, x1, y1], 2.0));
                match png {
                    Some(Ok(png)) => match crate::clipboard::set_png(&png) {
                        Ok(()) => st.toast(ToastKind::Ok, t(st.lang(), "toast.snapshot_copied"), ""),
                        Err(e) => st.error(e),
                    },
                    Some(Err(e)) => st.error(e),
                    None => {}
                }
                None
            }
            _ if tool.new_field().is_some() => {
                create_field(st, page, &size, tool, [x0, y0, x1, y1], tiny);
                None
            }
            Tool::PlaceImage => {
                let data = st.pending_image.peek().clone();
                match data {
                    Some(data) => {
                        // add_image takes display space with y up.
                        let display = if tiny {
                            None
                        } else {
                            Some([x0, size.height - y1, x1, size.height - y0])
                        };
                        let r = st.with_active(|t| t.doc.add_image(page, data, display).map(|_| ()));
                        st.set_pending_image(None);
                        r
                    }
                    None => None,
                }
            }
            _ => None,
        };
        match result {
            Some(Err(e)) => st.error(e),
            Some(Ok(())) => after_create(st),
            None => {}
        }
    };

    // ---- finishing a move/resize of a comment ------------------------------------
    let finish_annot = move |d: AnnotDrag| {
        let moved = dragged_rect(&d, z, view_rotation);
        if (moved[0] - d.rect[0]).abs() < 0.5
            && (moved[1] - d.rect[1]).abs() < 0.5
            && (moved[2] - d.rect[2]).abs() < 0.5
            && (moved[3] - d.rect[3]).abs() < 0.5
        {
            return;
        }
        let pdf_rect = rect_to_pdf(&size, moved);
        let r = st.with_active(|t| {
            let res = t.doc.set_annotation_rect(page, d.index, pdf_rect);
            t.selected_annot = Some((page, d.index));
            res
        });
        match r {
            Some(Err(e)) => st.error(e),
            _ => {
                st.with_active(|t| t.selected_annot = Some((page, d.index)));
                files::set_window_title(st);
            }
        }
    };

    let mut finish_text = move || {
        sel_drag.set(false);
        let kind = match tool {
            Tool::TextHighlight => Some(Markup::Highlight),
            Tool::TextUnderline => Some(Markup::Underline),
            Tool::TextStrike => Some(Markup::StrikeOut),
            _ => None,
        };
        if let Some(k) = kind {
            let has = st
                .read_active(|t| t.text_sel.map(|s| !s.is_empty()).unwrap_or(false))
                .unwrap_or(false);
            if has {
                commands::markup_selection(st, Some(k));
            }
        }
        if tool == Tool::ReplaceText {
            if let Some(sel) = st.read_active(|t| t.text_sel).flatten().filter(|s| !s.is_empty()) {
                let (start, end) = sel.range();
                st.set_dialog(Dialog::ReplaceText {
                    page: sel.page,
                    start,
                    end,
                });
            }
        }
    };

    let page_class = if current { "page current-page" } else { "page" };
    let ink_points: String = ink()
        .iter()
        .map(|(x, y)| format!("{x},{y}"))
        .collect::<Vec<_>>()
        .join(" ");
    let cur_drag = annot_drag();
    let units = st.ui.read().value("units").unwrap_or("mm").to_string();
    let (unit_pt, minor, major, unit_name) = unit_info(&units);
    let grid_px = grid_step(st) * z;
    let sel_first = view.sel.first().copied();
    // In-progress multi-click shape on this page: points string and vertex dots.
    let drawing_here = (st.drawing)().filter(|(p, _)| *p == page).map(|(_, v)| v);
    let multi_vertices: Vec<(f32, f32)> = drawing_here
        .clone()
        .unwrap_or_default()
        .iter()
        .map(|(x, y)| (x * z, y * z))
        .collect();
    let multi_preview = drawing_here.filter(|v| !v.is_empty()).map(|mut v| {
        if let Some(h) = hover() {
            v.push(h);
        }
        let pts = v
            .iter()
            .map(|(x, y)| format!("{},{}", x * z, y * z))
            .collect::<Vec<_>>()
            .join(" ");
        (pts, matches!(tool, Tool::Polygon | Tool::Cloud | Tool::Area))
    });

    rsx! {
        div {
            id: "pg-{page}",
            class: "{page_class}",
            "data-page": "{page}",
            style: "width:{outer_w}px;height:{outer_h}px",
            if rulers {
                Rulers { width: outer_w, height: outer_h, zoom: z, unit_pt, minor, major, unit_name }
            }
            div {
                class: if transparent { "page-inner transparent" } else { "page-inner" },
                style: "width:{inner_w}px;height:{inner_h}px;transform:{transform}",
                onmousedown: move |evt| {
                    if evt.trigger_button() != Some(MouseButton::Primary) {
                        return;
                    }
                    let p = evt.element_coordinates();
                    let (x, y) = ((p.x as f32) / z, (p.y as f32) / z);
                    if !current {
                        st.with_active(|t| t.page = page);
                    }
                    if tool.is_text() {
                        let idx = st.read_active(|t| t.doc.text_layers().get(page).and_then(|l| l.char_at(x, y))).flatten();
                        match idx {
                            Some(i) => {
                                let extend = evt.modifiers().shift();
                                st.with_active(|t| {
                                    t.selected_annot = None;
                                    match (extend, t.text_sel) {
                                        (true, Some(s)) if s.page == page => t.text_sel = Some(TextSel { head: i, ..s }),
                                        _ => t.text_sel = Some(TextSel { page, anchor: i, head: i }),
                                    }
                                });
                                sel_drag.set(true);
                            }
                            None => {
                                st.with_active(|t| t.text_sel = None);
                            }
                        }
                        return;
                    }
                    if tool.is_multi_click() {
                        let (sx, sy) = snap(st, page, x, y, z);
                        let mut pts = st.drawing.peek().clone().filter(|(p, _)| *p == page).map(|(_, v)| v).unwrap_or_default();
                        pts.push((sx, sy));
                        st.set_drawing(Some((page, pts)));
                        return;
                    }
                    if tool == Tool::InsertText {
                        let hit = st
                            .read_active(|t| {
                                let layers = t.doc.text_layers();
                                let l = layers.get(page)?;
                                let i = l.char_at(x, y)?;
                                let after = l.char_glyph.get(i).copied().flatten().and_then(|g| l.glyphs.get(g)).map(|g| x > (g.bbox[0] + g.bbox[2]) / 2.0).unwrap_or(false);
                                Some((i, after))
                            })
                            .flatten();
                        match hit {
                            Some((index, after)) => st.set_dialog(Dialog::InsertText { page, index, after }),
                            None => st.toast(ToastKind::Info, t(st.lang(), "toast.click_on_text"), ""),
                        }
                        return;
                    }
                    if tool == Tool::EditAnnots || tool == Tool::Hand {
                        st.with_active(|t| t.selected_annot = None);
                        return;
                    }
                    if tool.is_drawing() {
                        let (sx, sy) = snap(st, page, x, y, z);
                        let start = ((sx * z) as f64, (sy * z) as f64);
                        drag.set(Some(Drag { start, current: start }));
                        ink.set(vec![start]);
                    }
                },
                onmousemove: move |evt| {
                    let p = evt.element_coordinates();
                    let (x, y) = ((p.x as f32) / z, (p.y as f32) / z);
                    if track_pointer {
                        st.set_pointer(Some((page, x, y)));
                    }
                    if tool == Tool::Loupe {
                        let c = evt.client_coordinates();
                        st.set_loupe(Some((page, x, y, c.x, c.y)));
                    }
                    if *sel_drag.peek() {
                        if let Some(i) = st.read_active(|t| t.doc.text_layers().get(page).and_then(|l| l.char_at(x, y))).flatten() {
                            let changed = st.read_active(|t| t.text_sel.map(|s| s.head != i).unwrap_or(true)).unwrap_or(false);
                            if changed {
                                st.with_active(|t| {
                                    if let Some(s) = t.text_sel.as_mut() {
                                        if s.page == page {
                                            s.head = i;
                                        }
                                    }
                                });
                            }
                        }
                    }
                    if tool.is_multi_click() && st.drawing.peek().as_ref().map(|(p, _)| *p == page).unwrap_or(false) {
                        let (sx, sy) = snap(st, page, x, y, z);
                        hover.set(Some((sx, sy)));
                    }
                    let freehand = matches!(tool, Tool::Ink | Tool::FreeHighlight | Tool::Eraser);
                    let current_drag: Option<Drag> = *drag.peek();
                    if let Some(mut d) = current_drag {
                        let (sx, sy) = if freehand { (x, y) } else { snap(st, page, x, y, z) };
                        d.current = ((sx * z) as f64, (sy * z) as f64);
                        drag.set(Some(d));
                        if freehand {
                            ink.write().push(d.current);
                        }
                    }
                    let current_annot: Option<AnnotDrag> = *annot_drag.peek();
                    if let Some(mut d) = current_annot {
                        let c = evt.client_coordinates();
                        d.cur_client = (c.x, c.y);
                        annot_drag.set(Some(d));
                    }
                },
                onmouseup: move |_| {
                    if *sel_drag.peek() {
                        finish_text();
                    }
                    let current_drag: Option<Drag> = *drag.peek();
                    if let Some(d) = current_drag {
                        let points = ink.peek().clone();
                        drag.set(None);
                        ink.set(Vec::new());
                        finish_create(d, points);
                    }
                    let current_annot: Option<AnnotDrag> = *annot_drag.peek();
                    if let Some(d) = current_annot {
                        annot_drag.set(None);
                        finish_annot(d);
                    }
                },
                onmouseleave: move |_| {
                    if *sel_drag.peek() {
                        finish_text();
                    }
                    if drag.peek().is_some() {
                        drag.set(None);
                        ink.set(Vec::new());
                    }
                    let current_annot: Option<AnnotDrag> = *annot_drag.peek();
                    if let Some(d) = current_annot {
                        annot_drag.set(None);
                        finish_annot(d);
                    }
                },
                ondoubleclick: move |evt| {
                    if tool.is_multi_click() {
                        finish_drawing(st);
                        return;
                    }
                    if tool != Tool::Select {
                        return;
                    }
                    let p = evt.element_coordinates();
                    let (x, y) = ((p.x as f32) / z, (p.y as f32) / z);
                    let word = st.read_active(|t| t.doc.text_layers().get(page).and_then(|l| l.char_at(x, y).map(|i| l.word_at(i.saturating_sub(0))))).flatten();
                    if let Some((a, b)) = word {
                        st.with_active(|t| t.text_sel = Some(TextSel { page, anchor: a, head: b }));
                    }
                },
                onclick: move |evt| {
                    if tool == Tool::Zoom {
                        let z = current_zoom(st);
                        let f = if evt.modifiers().shift() { 0.8 } else { 1.25 };
                        st.with_active(|t| t.zoom = Zoom::Fixed((z * f).clamp(0.1, 8.0)));
                    }
                },
                img { class: "page-img", src: "{src}", alt: "", loading: "lazy", draggable: "false", style: if img_filter.is_empty() { String::new() } else { format!("filter:{img_filter}") } }
                if show_grid {
                    div { class: "grid-overlay", style: "background-size:{grid_px}px {grid_px}px" }
                }
                if show_guides {
                    for (i, (horizontal, pos)) in view.guides.iter().enumerate() {
                        div { key: "g{i}", class: if *horizontal { "guide h" } else { "guide v" }, style: if *horizontal { format!("top:{}px", pos * z) } else { format!("left:{}px", pos * z) } }
                    }
                }
                for (i, r) in view.links.iter().enumerate() {
                    div { key: "l{i}", class: "link-box", style: "{css(*r)}" }
                }
                for (i, r) in view.fields.iter().enumerate() {
                    div { key: "f{i}", class: "field-box", style: "{css(*r)}" }
                }
                for (i, (cur, r, word)) in view.hits.iter().enumerate() {
                    div { key: "h{i}", class: "{hit_class(*cur, *word, colored)}", style: "{css(*r)}" }
                }
                for (i, r) in view.sel.iter().enumerate() {
                    div { key: "s{i}", class: "sel", style: "{css(*r)}" }
                }
                if let Some(r) = sel_first {
                    if !*sel_drag.read() {
                        SelToolbar { left: r[0] * z, top: (r[1] * z - 38.0).max(2.0) }
                    }
                }
                for (w, r) in view.widgets.iter().cloned() {
                    {
                        let editing = (st.editing_field)() == Some((page, w.index));
                        let kind = w.kind.clone();
                        let class = match (&kind, w.read_only) {
                            (_, true) => "field-hit readonly",
                            (folitecta_core::FieldKind::Text, _) => "field-hit text",
                            _ => "field-hit",
                        };
                        let w2 = w.clone();
                        rsx! {
                            if editing {
                                FieldInput { key: "fi{w.index}", page, widget: w2, left: r[0] * z, top: r[1] * z, width: (r[2] - r[0]) * z, height: (r[3] - r[1]) * z, zoom: z }
                            } else {
                                div { key: "w{w.index}", class: "{class}", title: "{w.name}", style: "{css(r)}",
                                    onmousedown: move |e| e.stop_propagation(),
                                    onclick: move |e| {
                                        e.stop_propagation();
                                        fill_field(st, page, &w);
                                    },
                                }
                            }
                        }
                    }
                }
                for (index, subtype, rect, contents) in view.annots.iter().cloned() {
                    {
                        let is_sel = view.selected == Some(index);
                        let shown = match cur_drag { Some(d) if d.index == index => dragged_rect(&d, z, view_rotation), _ => rect };
                        let title = format!("{}: {contents}", crate::i18n::annot_name(st.lang(), &subtype));
                        let class = if is_sel { "annot-hit selected" } else { "annot-hit" };
                        let can_edit = tool == Tool::EditAnnots || tool == Tool::EditFields;
                        let is_widget = subtype == "Widget";
                        rsx! {
                            div { class: "{class}", key: "a{index}", title: "{title}", style: "{css(shown)}",
                                onmousedown: move |e| {
                                    e.stop_propagation();
                                    st.with_active(|t| { t.selected_annot = Some((page, index)); t.text_sel = None; });
                                    if can_edit && e.trigger_button() == Some(MouseButton::Primary) {
                                        let c = e.client_coordinates();
                                        annot_drag.set(Some(AnnotDrag { index, kind: AnnotDragKind::Move, start_client: (c.x, c.y), cur_client: (c.x, c.y), rect }));
                                    }
                                },
                                ondoubleclick: move |e| {
                                    e.stop_propagation();
                                    if is_widget {
                                        st.ui_mut().show_right_pane = true;
                                        return;
                                    }
                                    let contents = contents.clone();
                                    st.set_dialog(Dialog::AnnotationText { page, index, contents });
                                },
                                if is_sel && can_edit {
                                    for corner in 0u8..4 {
                                        div { key: "c{corner}", class: "handle h{corner}",
                                            onmousedown: move |e| {
                                                e.stop_propagation();
                                                let c = e.client_coordinates();
                                                annot_drag.set(Some(AnnotDrag { index, kind: AnnotDragKind::Resize(corner), start_client: (c.x, c.y), cur_client: (c.x, c.y), rect }));
                                            },
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                if let Some(r) = view.flash {
                    div { class: "dest-flash", style: "{css(r)}" }
                }
                for pv in view.popups.iter().cloned() {
                    PopupNote { key: "pn{pv.index}", page, view: pv, zoom: z, size, view_rotation, area_w: inner_w, area_h: inner_h }
                }
                if let Some((pts, closed)) = multi_preview.clone() {
                    svg { class: "ink-preview", width: "{inner_w}", height: "{inner_h}",
                        if closed {
                            polygon { points: "{pts}", fill: "rgba(79,70,229,0.08)", stroke: "#4f46e5", stroke_width: "1.5", stroke_dasharray: "5 3" }
                        } else {
                            polyline { points: "{pts}", fill: "none", stroke: "#4f46e5", stroke_width: "1.5" }
                        }
                        for (i, (vx, vy)) in multi_vertices.iter().enumerate() {
                            circle { key: "v{i}", cx: "{vx}", cy: "{vy}", r: "3.5", fill: "#ffffff", stroke: "#4f46e5", stroke_width: "1.5" }
                        }
                    }
                }
                if let Some(d) = drag() {
                    if matches!(tool, Tool::Ink | Tool::FreeHighlight | Tool::Eraser) {
                        svg { class: "ink-preview", width: "{inner_w}", height: "{inner_h}",
                            match tool {
                                Tool::Eraser => rsx! { polyline { points: "{ink_points}", fill: "none", stroke: "rgba(220,38,38,0.45)", stroke_width: "12", stroke_linecap: "round", stroke_linejoin: "round" } },
                                Tool::FreeHighlight => rsx! { polyline { points: "{ink_points}", fill: "none", stroke: "rgba(250,204,21,0.5)", stroke_width: "{12.0 * z}", stroke_linecap: "round", stroke_linejoin: "round" } },
                                _ => rsx! { polyline { points: "{ink_points}", fill: "none", stroke: "#3b3bc0", stroke_width: "2" } },
                            }
                        }
                    } else if matches!(tool, Tool::Line | Tool::Arrow | Tool::Distance | Tool::Callout) {
                        svg { class: "ink-preview", width: "{inner_w}", height: "{inner_h}",
                            line { x1: "{d.start.0}", y1: "{d.start.1}", x2: "{d.current.0}", y2: "{d.current.1}", stroke: "#3b3bc0", stroke_width: "2" }
                        }
                    } else {
                        div { class: if tool == Tool::Redact { "marquee redact" } else { "marquee" },
                            style: "left:{d.start.0.min(d.current.0)}px;top:{d.start.1.min(d.current.1)}px;width:{(d.start.0 - d.current.0).abs()}px;height:{(d.start.1 - d.current.1).abs()}px" }
                    }
                }
            }
            div { class: "page-label", "{page_label}" }
            if show_size {
                div { class: "page-size", "{size_text}" }
            }
        }
    }
}

/// An open pop-up note of a comment, in display points.
#[derive(Debug, Clone, PartialEq)]
pub struct PopupView {
    index: usize,
    rect: [f32; 4],
    subtype: String,
    author: String,
    date: String,
    contents: String,
    color: [f32; 3],
    locked: bool,
    replies: Vec<(String, String, String)>,
}

/// Open pop-up notes of the comments of a page.
fn popup_views(
    infos: &[folitecta_core::CommentInfo],
    filtered: &std::collections::HashSet<usize>,
    size: &PageSize,
) -> Vec<PopupView> {
    infos
        .iter()
        .filter(|c| c.reply_to.is_none() && !c.is_state && !c.hidden && !filtered.contains(&c.index))
        .filter_map(|c| {
            let (_, open, r) = c.popup?;
            open.then(|| PopupView {
                index: c.index,
                rect: pdf_rect_to_display(size, r),
                subtype: c.subtype.clone(),
                author: c.author.clone(),
                date: c.modified.clone(),
                contents: c.contents.clone(),
                color: c.color.unwrap_or([1.0, 0.85, 0.2]),
                locked: c.locked,
                replies: infos
                    .iter()
                    .filter(|x| x.reply_to == Some(c.id) && !x.is_state)
                    .map(|x| (x.author.clone(), x.modified.clone(), x.contents.clone()))
                    .collect(),
            })
        })
        .collect()
}

/// Mouse movement in client pixels expressed in page display pixels.
fn unrotate_delta(dx: f64, dy: f64, view_rotation: i32) -> (f64, f64) {
    match view_rotation {
        90 => (dy, -dx),
        180 => (-dx, -dy),
        270 => (-dy, dx),
        _ => (dx, dy),
    }
}

const POPUP_W: f32 = 240.0;
const POPUP_H: f32 = 170.0;

/// A pop-up note: the comment text (editable), its replies and a reply box.
/// The header drags the note.
#[component]
fn PopupNote(
    page: usize,
    view: PopupView,
    zoom: f32,
    size: PageSize,
    view_rotation: i32,
    area_w: f32,
    area_h: f32,
) -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let mut drag = use_signal(|| None::<((f64, f64), (f64, f64))>);
    let mut reply = use_signal(String::new);
    let index = view.index;
    let (dx, dy) = drag()
        .map(|(s, c)| unrotate_delta(c.0 - s.0, c.1 - s.1, view_rotation))
        .unwrap_or((0.0, 0.0));
    let left = (view.rect[0] * zoom + dx as f32).clamp(0.0, (area_w - POPUP_W).max(0.0));
    let top = (view.rect[1] * zoom + dy as f32).clamp(0.0, (area_h - POPUP_H).max(0.0));
    let color = crate::util::color_hex(view.color);
    let original = view.contents.clone();
    let rect = view.rect;
    let send_reply = move || {
        let text = reply.peek().trim().to_string();
        if text.is_empty() {
            return;
        }
        let author = st.ui.peek().author.clone();
        match st.with_active(|t| t.doc.add_reply(page, index, &text, &author)) {
            Some(Err(e)) => st.error(e),
            _ => {
                reply.set(String::new());
                crate::files::set_window_title(st);
            }
        }
    };
    let mut send_key = send_reply;
    let mut send_click = send_reply;
    rsx! {
        div { class: "popup-note", style: "left:{left}px;top:{top}px;--note:{color}",
            onmousedown: move |e| e.stop_propagation(),
            onclick: move |e| e.stop_propagation(),
            ondoubleclick: move |e| e.stop_propagation(),
            onkeydown: move |e| e.stop_propagation(),
            div { class: "popup-head",
                onmousedown: move |e| {
                    e.stop_propagation();
                    let c = e.client_coordinates();
                    drag.set(Some(((c.x, c.y), (c.x, c.y))));
                },
                span { class: "who ellipsis", "{view.author}" }
                span { class: "when", "{view.date}" }
                button { class: "ibtn", title: "{t(lang, \"dialog.close\")}",
                    onmousedown: move |e| e.stop_propagation(),
                    onclick: move |e| {
                        e.stop_propagation();
                        if let Some(Err(err)) = st.with_active(|t| t.doc.set_popup_open(page, index, false)) { st.error(err); }
                    },
                    "×"
                }
            }
            textarea { class: "popup-text", value: "{view.contents}", readonly: view.locked,
                placeholder: "{crate::i18n::annot_name(lang, &view.subtype)}",
                onchange: move |e| {
                    let v = e.value();
                    if v != original {
                        match st.with_active(|t| t.doc.set_annotation_contents(page, index, &v)) {
                            Some(Err(err)) => st.error(err),
                            _ => crate::files::set_window_title(st),
                        }
                    }
                },
            }
            if !view.replies.is_empty() {
                div { class: "popup-replies",
                    for (i, (author, date, text)) in view.replies.iter().enumerate() {
                        div { key: "{i}", class: "popup-reply",
                            div { class: "meta", "{author} · {date}" }
                            div { class: "text", "{text}" }
                        }
                    }
                }
            }
            div { class: "popup-reply-box",
                input { value: "{reply}", placeholder: "{t(lang, \"popup.reply_placeholder\")}",
                    oninput: move |e| reply.set(e.value()),
                    onkeydown: move |e| {
                        e.stop_propagation();
                        if e.key() == Key::Enter { send_key(); }
                    },
                }
                button { class: "tbtn", onclick: move |_| send_click(), "{t(lang, \"popup.reply\")}" }
            }
            if drag().is_some() {
                div { class: "drag-capture",
                    onmousemove: move |e| {
                        let start = drag.peek().as_ref().map(|(s, _)| *s);
                        if let Some(s) = start {
                            let c = e.client_coordinates();
                            drag.set(Some((s, (c.x, c.y))));
                        }
                    },
                    onmouseup: move |_| {
                        let Some((s, c)) = *drag.peek() else { return };
                        drag.set(None);
                        let (dx, dy) = unrotate_delta(c.0 - s.0, c.1 - s.1, view_rotation);
                        if dx.abs() < 2.0 && dy.abs() < 2.0 {
                            return;
                        }
                        let (mx, my) = (dx as f32 / zoom, dy as f32 / zoom);
                        let moved = [rect[0] + mx, rect[1] + my, rect[2] + mx, rect[3] + my];
                        let pdf = rect_to_pdf(&size, moved);
                        if let Some(Err(err)) = st.with_active(|t| t.doc.set_popup_rect(page, index, pdf)) { st.error(err); }
                    },
                }
            }
        }
    }
}

/// Floating actions above a text selection.
#[component]
fn SelToolbar(left: f32, top: f32) -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let act = move |id: &'static str| {
        move |e: Event<MouseData>| {
            e.stop_propagation();
            commands::dispatch(st, id);
        }
    };
    rsx! {
        div { class: "sel-toolbar", style: "left:{left}px;top:{top}px", onmousedown: move |e| e.stop_propagation(),
            button { title: "{t(lang, \"sel.copy\")} (Ctrl+C)", onclick: act("tool.selectText.copyAsRich"), Icon { name: "copy" } }
            button { title: "{t(lang, \"sel.highlight\")}", onclick: act("textsel.addHighlight"), Icon { name: "highlight" } }
            button { title: "{t(lang, \"sel.underline\")}", onclick: act("textsel.addUnderline"), span { class: "u", "U" } }
            button { title: "{t(lang, \"sel.strike\")}", onclick: act("textsel.addStrikeout"), span { class: "s", "S" } }
            button { title: "{t(lang, \"sel.redact\")}", onclick: act("addAnnotBySelect.redaction"), Icon { name: "redact" } }
        }
    }
}

/// Rulers along the top and left edge of a page.
#[component]
fn Rulers(
    width: f32,
    height: f32,
    zoom: f32,
    unit_pt: f32,
    minor: f32,
    major: f32,
    unit_name: &'static str,
) -> Element {
    let minor_px = minor * unit_pt * zoom;
    let major_px = major * unit_pt * zoom;
    let label_every = if major_px < 40.0 {
        ((40.0 / major_px).ceil()) as usize
    } else {
        1
    };
    let xs: Vec<(f32, String)> = (0..)
        .map(|i| i as f32 * major_px)
        .take_while(|x| *x <= width)
        .enumerate()
        .filter(|(i, _)| i % label_every == 0)
        .map(|(i, x)| (x, format!("{}", (i as f32 * major * 100.0).round() / 100.0)))
        .collect();
    let ys: Vec<(f32, String)> = (0..)
        .map(|i| i as f32 * major_px)
        .take_while(|y| *y <= height)
        .enumerate()
        .filter(|(i, _)| i % label_every == 0)
        .map(|(i, y)| (y, format!("{}", (i as f32 * major * 100.0).round() / 100.0)))
        .collect();
    rsx! {
        div { class: "ruler rx", title: "{unit_name}", style: "width:{width}px;--minor:{minor_px.max(2.0)}px;--major:{major_px.max(4.0)}px",
            for (x, label) in xs { span { key: "x{x}", style: "left:{x}px", "{label}" } }
        }
        div { class: "ruler ry", title: "{unit_name}", style: "height:{height}px;--minor:{minor_px.max(2.0)}px;--major:{major_px.max(4.0)}px",
            for (y, label) in ys { span { key: "y{y}", style: "top:{y}px", "{label}" } }
        }
    }
}
