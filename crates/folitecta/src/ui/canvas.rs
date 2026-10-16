//! Document tabs and the page canvas with drawing tools.

use dioxus::html::geometry::WheelDelta;
use dioxus::html::{InteractionElementOffset, ModifiersInteraction, PointerInteraction};
use dioxus::prelude::*;
use folitecta_core::marks::display_to_pdf;
use folitecta_core::{Markup, PageSize, Shape};

use crate::commands::{self, current_zoom};
use crate::files;
use crate::i18n::t;
use crate::icons::Icon;
use crate::state::{AfterUnsaved, AppState, Dialog, Layout, ToastKind, Tool};
use crate::ui::welcome::Welcome;

#[component]
pub fn CanvasArea() -> Element {
    let st = use_context::<AppState>();
    if !st.has_doc() {
        return rsx! { div { class: "canvas-wrap", Welcome {} } };
    }
    rsx! {
        div { class: "canvas-wrap",
            DocTabs {}
            Canvas {}
        }
    }
}

#[component]
fn DocTabs() -> Element {
    let st = use_context::<AppState>();
    let active = st.active_index().unwrap_or(0);
    let tabs: Vec<(u64, String, bool)> = st
        .docs
        .read()
        .iter()
        .map(|t| (t.id, t.title(), t.doc.is_dirty()))
        .collect();
    rsx! {
        div { class: "doc-tabs",
            for (i, (id, title, dirty)) in tabs.into_iter().enumerate() {
                button { class: if i == active { "doc-tab active" } else { "doc-tab" }, key: "{id}", title: "{title}",
                    onclick: move |_| { st.set_active(i); files::set_window_title(st); },
                    span { class: "name", "{title}" }
                    if dirty { span { class: "dirty", "•" } }
                    span { class: "close", title: "Close", onclick: move |e| { e.stop_propagation(); files::close_tab(st, id, false, AfterUnsaved::Close); }, "×" }
                }
            }
            button { class: "doc-tab", title: "Open…", onclick: move |_| files::open_dialog(st), Icon { name: "insert" } }
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
struct Drag {
    start: (f64, f64),
    current: (f64, f64),
}

#[component]
fn Canvas() -> Element {
    let st = use_context::<AppState>();
    let zoom = current_zoom(st);
    let dpr = (st.dpr)();
    let tool = (st.tool)();
    let Some((tab_id, page, rev, layout, count, sizes, rotation)) = st.read_active(|t| {
        (
            t.id,
            t.page,
            t.revision,
            t.layout,
            t.doc.page_count(),
            t.page_sizes.clone(),
            t.view_rotation,
        )
    }) else {
        return rsx! {};
    };
    let render_scale = ((zoom * dpr as f32) * 1000.0).round().clamp(50.0, 6000.0) as u32;
    let pages: Vec<usize> = match layout {
        Layout::Single => vec![page.min(count.saturating_sub(1))],
        _ => (0..count).collect(),
    };
    let tool_class = match tool {
        Tool::Hand => "tool-hand",
        Tool::Select | Tool::Zoom => "tool-select",
        _ => "tool-draw",
    };
    let mut scroll_pending = use_signal(|| false);
    let cls = format!("canvas {tool_class}");
    let pages_class = if layout == Layout::TwoPages {
        "pages two"
    } else {
        "pages"
    };

    rsx! {
        div {
            id: "canvas",
            class: "{cls}",
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
                    st.with_active(|t| t.zoom = crate::state::Zoom::Fixed(next.clamp(0.1, 8.0)));
                }
            },
            onscroll: move |_| {
                if layout == Layout::Single || *scroll_pending.peek() {
                    return;
                }
                scroll_pending.set(true);
                spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_millis(90)).await;
                    let js = "const c=document.getElementById('canvas'); if(!c) return 0; const r=c.getBoundingClientRect(); let best=0,bd=1e9; c.querySelectorAll('.page').forEach((el,i)=>{const d=Math.abs(el.getBoundingClientRect().top-r.top-24); if(d<bd){bd=d;best=i;}}); return best;";
                    if let Ok(v) = document::eval(js).await {
                        if let Some(i) = v.as_u64() {
                            let i = i as usize;
                            let cur = st.read_active(|t| t.page).unwrap_or(0);
                            if cur != i {
                                st.with_active(|t| t.page = i);
                            }
                        }
                    }
                    scroll_pending.set(false);
                });
            },
            div { class: "{pages_class}",
                for p in pages {
                    {
                        let size = sizes.get(p).copied();
                        let (w_pt, h_pt) = size.map(|s| if rotation % 180 != 0 { (s.height, s.width) } else { (s.width, s.height) }).unwrap_or((595.0, 842.0));
                        rsx! {
                            PageView {
                                key: "{tab_id}-{p}",
                                tab_id, page: p, rev, scale: render_scale, zoom,
                                width: (w_pt * zoom) as f64, height: (h_pt * zoom) as f64,
                                current: p == page, tool, view_rotation: rotation,
                            }
                        }
                    }
                }
            }
        }
    }
}

/// CSS-pixel position of a PDF rectangle on an unrotated page view.
fn rect_to_css(size: &PageSize, rect: [f32; 4], zoom: f32) -> Option<(f64, f64, f64, f64)> {
    if size.rotation != 0 {
        return None;
    }
    let c = size.crop_box;
    let left = (rect[0] - c[0]) * zoom;
    let top = (c[3] - rect[3]) * zoom;
    let w = (rect[2] - rect[0]) * zoom;
    let h = (rect[3] - rect[1]) * zoom;
    Some((left as f64, top as f64, w.max(2.0) as f64, h.max(2.0) as f64))
}

/// Convert a CSS-pixel point inside the page element into PDF user space.
fn css_to_pdf(size: &PageSize, zoom: f32, x: f64, y: f64) -> (f32, f32) {
    let u = x as f32 / zoom;
    let v = size.height - y as f32 / zoom;
    display_to_pdf(size, u, v)
}

#[component]
fn PageView(
    tab_id: u64,
    page: usize,
    rev: u64,
    scale: u32,
    zoom: f32,
    width: f64,
    height: f64,
    current: bool,
    tool: Tool,
    view_rotation: i32,
) -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let mut drag = use_signal(|| None::<Drag>);
    let mut ink: Signal<Vec<(f64, f64)>> = use_signal(Vec::new);
    let src = format!("/page/{tab_id}/{page}/{rev}/{scale}.png");
    let (size, annots, fields, selected, highlight_fields) = st
        .read_active(|t| {
            let size = t.page_sizes.get(page).copied();
            let annots = t.doc.annotations(page).unwrap_or_default();
            let fields: Vec<[f32; 4]> = if t.highlight_fields {
                t.doc
                    .form_fields()
                    .into_iter()
                    .filter(|f| f.page == Some(page))
                    .filter_map(|f| f.rect)
                    .collect()
            } else {
                Vec::new()
            };
            (size, annots, fields, t.selected_annot, t.highlight_fields)
        })
        .unwrap_or((None, Vec::new(), Vec::new(), None, false));
    let Some(size) = size else { return rsx! {} };
    let drawing = tool.is_drawing() && view_rotation == 0;

    let finish = move |d: Drag, points: Vec<(f64, f64)>| {
        let (x0, y0) = (d.start.0.min(d.current.0), d.start.1.min(d.current.1));
        let (x1, y1) = (d.start.0.max(d.current.0), d.start.1.max(d.current.1));
        let author = st.ui.read().author.clone();
        let (ax, ay) = css_to_pdf(&size, zoom, x0, y1);
        let (bx, by) = css_to_pdf(&size, zoom, x1, y0);
        let rect = [ax.min(bx), ay.min(by), ax.max(bx), ay.max(by)];
        let tiny = (x1 - x0) < 4.0 && (y1 - y0) < 4.0;
        let result = match tool {
            Tool::Note => {
                let (px, py) = css_to_pdf(&size, zoom, d.current.0, d.current.1);
                st.set_dialog(Dialog::Note { page, x: px, y: py });
                None
            }
            Tool::TextBox => {
                if tiny {
                    st.set_dialog(Dialog::TextBox {
                        page,
                        rect: [rect[0], rect[1] - 40.0, rect[0] + 200.0, rect[1] + 10.0],
                    });
                } else {
                    st.set_dialog(Dialog::TextBox { page, rect });
                }
                None
            }
            Tool::Link => {
                if !tiny {
                    st.set_dialog(Dialog::Link { page, rect });
                }
                None
            }
            Tool::Rect if !tiny => st.with_active(|t| {
                t.doc
                    .add_shape(
                        page,
                        Shape::Square,
                        rect,
                        [0.86, 0.15, 0.15],
                        None,
                        2.0,
                        &author,
                        "",
                    )
                    .map(|_| ())
            }),
            Tool::Ellipse if !tiny => st.with_active(|t| {
                t.doc
                    .add_shape(
                        page,
                        Shape::Circle,
                        rect,
                        [0.1, 0.45, 0.85],
                        None,
                        2.0,
                        &author,
                        "",
                    )
                    .map(|_| ())
            }),
            Tool::Highlight if !tiny => {
                let q = [
                    rect[0], rect[3], rect[2], rect[3], rect[0], rect[1], rect[2], rect[1],
                ];
                st.with_active(|t| {
                    t.doc
                        .add_text_markup(page, Markup::Highlight, &[q], [1.0, 0.9, 0.2], &author, "")
                        .map(|_| ())
                })
            }
            Tool::Line | Tool::Arrow if !tiny => {
                let (fx, fy) = css_to_pdf(&size, zoom, d.start.0, d.start.1);
                let (tx, ty) = css_to_pdf(&size, zoom, d.current.0, d.current.1);
                st.with_active(|t| {
                    t.doc
                        .add_line(
                            page,
                            (fx, fy),
                            (tx, ty),
                            [0.1, 0.1, 0.1],
                            1.5,
                            tool == Tool::Arrow,
                            &author,
                            "",
                        )
                        .map(|_| ())
                })
            }
            Tool::Ink if points.len() > 1 => {
                let path: Vec<(f32, f32)> = points
                    .iter()
                    .map(|(x, y)| css_to_pdf(&size, zoom, *x, *y))
                    .collect();
                st.with_active(|t| {
                    t.doc
                        .add_ink(page, &[path], [0.2, 0.2, 0.75], 2.0, &author)
                        .map(|_| ())
                })
            }
            _ => None,
        };
        if let Some(Err(e)) = result {
            st.error(e);
        } else if result.is_some() {
            files::set_window_title(st);
        }
    };

    let page_class = if current { "page current-page" } else { "page" };
    let ink_points: String = ink()
        .iter()
        .map(|(x, y)| format!("{x},{y}"))
        .collect::<Vec<_>>()
        .join(" ");
    rsx! {
        div {
            id: "pg-{page}",
            class: "{page_class}",
            style: "width:{width}px;height:{height}px",
            onmousedown: move |evt| {
                if !drawing || evt.trigger_button() != Some(dioxus::html::input_data::MouseButton::Primary) {
                    return;
                }
                let p = evt.element_coordinates();
                drag.set(Some(Drag { start: (p.x, p.y), current: (p.x, p.y) }));
                ink.set(vec![(p.x, p.y)]);
            },
            onmousemove: move |evt| {
                let current_drag: Option<Drag> = *drag.peek();
                if let Some(mut d) = current_drag {
                    let p = evt.element_coordinates();
                    d.current = (p.x, p.y);
                    drag.set(Some(d));
                    if tool == Tool::Ink {
                        ink.write().push((p.x, p.y));
                    }
                }
            },
            onmouseup: move |evt| {
                let current_drag: Option<Drag> = *drag.peek();
                if let Some(mut d) = current_drag {
                    let p = evt.element_coordinates();
                    d.current = (p.x, p.y);
                    let points = ink.peek().clone();
                    drag.set(None);
                    ink.set(Vec::new());
                    finish(d, points);
                }
            },
            onmouseleave: move |_| {
                if drag.peek().is_some() { drag.set(None); ink.set(Vec::new()); }
            },
            onclick: move |_| {
                if !current {
                    st.with_active(|t| t.page = page);
                }
                if tool == Tool::Zoom {
                    let z = current_zoom(st);
                    st.with_active(|t| t.zoom = crate::state::Zoom::Fixed((z * 1.25).clamp(0.1, 8.0)));
                }
                if tool.is_drawing() && view_rotation != 0 {
                    st.toast(ToastKind::Warn, t(lang, "cmd.view.rotate.reset"), "Reset the view rotation before drawing.");
                }
            },
            img { src: "{src}", alt: "Page {page + 1}", loading: "lazy", draggable: "false" }
            if tool == Tool::Select {
                for a in annots.iter().filter(|a| !["Link", "Widget", "Popup"].contains(&a.subtype.as_str())) {
                    if let Some((l, tp, w, h)) = rect_to_css(&size, a.rect, zoom) {
                        {
                            let idx = a.index;
                            let sel = selected == Some((page, idx));
                            let title = format!("{} — {}: {}", a.subtype, a.author, a.contents);
                            let contents = a.contents.clone();
                            let extra = if sel { "border-color: var(--accent); background: rgba(79,70,229,.15)" } else { "" };
                            rsx! {
                                div { class: "annot-hit", key: "a{idx}", title: "{title}",
                                    style: "left:{l}px;top:{tp}px;width:{w}px;height:{h}px;{extra}",
                                    onclick: move |e| { e.stop_propagation(); st.with_active(|t| t.selected_annot = Some((page, idx))); },
                                    ondoubleclick: move |e| { e.stop_propagation(); st.set_dialog(Dialog::AnnotationText { page, index: idx, contents: contents.clone() }); },
                                }
                            }
                        }
                    }
                }
            }
            if highlight_fields {
                for (i, r) in fields.iter().enumerate() {
                    if let Some((l, tp, w, h)) = rect_to_css(&size, *r, zoom) {
                        div { key: "f{i}", class: "annot-hit", style: "left:{l}px;top:{tp}px;width:{w}px;height:{h}px;background:rgba(79,70,229,.18);border-color:var(--accent)" }
                    }
                }
            }
            if let Some(d) = drag() {
                if tool == Tool::Ink {
                    svg { style: "position:absolute;left:0;top:0;pointer-events:none", width: "{width}", height: "{height}",
                        polyline { points: "{ink_points}", fill: "none", stroke: "#3b3bc0", stroke_width: "2" }
                    }
                } else {
                    div { class: "marquee",
                        style: "left:{d.start.0.min(d.current.0)}px;top:{d.start.1.min(d.current.1)}px;width:{(d.start.0 - d.current.0).abs()}px;height:{(d.start.1 - d.current.1).abs()}px" }
                }
            }
            div { class: "page-label", "{page + 1}" }
        }
    }
}

#[allow(dead_code)]
pub fn zoom_label(st: AppState) -> String {
    format!("{}%", (commands::current_zoom(st) * 100.0).round())
}
