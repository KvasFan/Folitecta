//! Status bar: tool, selection, pointer position, navigation, zoom and layout.

use dioxus::prelude::*;

use crate::commands::{current_zoom, dispatch, option_on};
use crate::i18n::{t, tf};
use crate::icons::Icon;
use crate::state::{AppState, Layout, Tool, Zoom};
use crate::ui::canvas::unit_info;
use crate::util::human_size;

fn fmt_len(v: f32, unit_pt: f32) -> String {
    let x = v / unit_pt;
    if x.abs() >= 100.0 {
        format!("{x:.0}")
    } else {
        format!("{x:.1}")
    }
}

#[component]
pub fn StatusBar() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let has_doc = st.has_doc();
    let page = st.current_page();
    let count = st.page_count();
    let zoom_pct = (current_zoom(st) * 100.0).round() as i32;
    let tool = (st.tool)();
    let (layout, size, selected, zoom_mode, page_size, sel_chars, label, drawing) = st
        .read_active(|t| {
            let sel_chars = t.text_sel.map(|s| s.range()).map(|(a, b)| b - a).unwrap_or(0);
            let label = t.doc.page_labels().get(t.page).cloned().unwrap_or_default();
            // The layer new drawings and text go to.
            let drawing = t
                .doc
                .drawing_layer()
                .and_then(|id| t.doc.layers().into_iter().find(|l| l.id == id))
                .map(|l| l.name);
            (
                t.layout,
                t.doc.bytes().len() as u64,
                t.selected_pages.len(),
                t.zoom,
                t.display_size(t.page),
                sel_chars,
                label,
                drawing,
            )
        })
        .unwrap_or((
            Layout::Continuous,
            0,
            0,
            Zoom::FitWidth,
            (0.0, 0.0),
            0,
            String::new(),
            None,
        ));
    let mut page_input = use_signal(String::new);
    let tool_label = t(lang, tool.label_key());
    let busy = (st.busy)();
    let units = st.ui.read().value("units").unwrap_or("mm").to_string();
    let (unit_pt, _, _, unit_name) = unit_info(&units);
    let show_measure = option_on(st, "view.showPageMeasureInfo");
    let show_wh = option_on(st, "view.pageMeasureInfo.WH");
    let show_xy = option_on(st, "view.pageMeasureInfo.XY");
    let show_units = option_on(st, "view.pageMeasureInfo.units");
    let math_y = option_on(st, "view.mathY");
    let pointer = (st.pointer)();
    let pointer_text = pointer
        .filter(|(p, _, _)| *p == page || layout != Layout::Single)
        .map(|(p, x, y)| {
            let h = st.read_active(|t| t.display_size(p).1).unwrap_or(0.0);
            let y = if math_y { h - y } else { y };
            format!("X {} · Y {}", fmt_len(x, unit_pt), fmt_len(y, unit_pt))
        });
    let unit_suffix = if show_units {
        format!(" {unit_name}")
    } else {
        String::new()
    };
    let tool_icon = match tool {
        Tool::Hand => "hand",
        Tool::Select | Tool::TextHighlight | Tool::TextUnderline | Tool::TextStrike => "select",
        Tool::Zoom | Tool::Loupe => "zoom-in",
        _ => "pencil",
    };
    let page_value = if page_input().is_empty() {
        if !label.is_empty() && label != (page + 1).to_string() {
            label.clone()
        } else {
            format!("{}", page + 1)
        }
    } else {
        page_input()
    };

    rsx! {
        div { class: "statusbar",
            span { class: "chip tool-chip", title: "{tool_label}", Icon { name: tool_icon } span { class: "ellipsis", "{tool_label}" } }
            if let Some(name) = drawing {
                span { class: "chip layer-chip", title: "{t(lang, \"layers.drawing_here\")}",
                    Icon { name: "layers" }
                    span { class: "ellipsis", "{tf(lang, \"status.drawing_layer\", &[(\"name\", &name)])}" }
                    button { class: "chip-x", title: "{t(lang, \"status.drawing_layer_clear\")}",
                        onclick: move |_| dispatch(st, "defaultLayer"),
                        Icon { name: "close" }
                    }
                }
            }
            if selected > 0 {
                span { class: "chip sb-opt", "{selected} {t(lang, \"status.pages_selected\")}" }
            }
            if sel_chars > 0 {
                span { class: "chip sb-opt", "{sel_chars} {t(lang, \"status.chars_selected\")}" }
            }
            if busy {
                span { class: "chip", "{t(lang, \"status.rendering\")}" }
            }
            PrintChip {}
            if tool.is_multi_click() {
                span { class: "chip sb-opt ellipsis", title: "{t(lang, \"status.multi_hint\")}", "{t(lang, \"status.multi_hint\")}" }
            }
            if has_doc && show_measure {
                if show_wh {
                    span { class: "muted sb-opt2", title: "{t(lang, \"status.page_size\")}", "{fmt_len(page_size.0, unit_pt)} × {fmt_len(page_size.1, unit_pt)}{unit_suffix}" }
                }
                if show_xy {
                    if let Some(p) = pointer_text {
                        span { class: "muted sb-opt2 mono", "{p}{unit_suffix}" }
                    }
                }
            }
            div { class: "spacer" }
            if has_doc {
                div { class: "seg",
                    button { title: "{t(lang, \"status.first\")}", onclick: move |_| dispatch(st, "view.goto.firstPage"), Icon { name: "first" } }
                    button { title: "{t(lang, \"status.prev\")}", onclick: move |_| dispatch(st, "view.goto.prevPage"), Icon { name: "prev" } }
                }
                input {
                    class: "page-input",
                    r#type: "text",
                    title: "{t(lang, \"status.page\")}",
                    value: "{page_value}",
                    onfocus: move |_| page_input.set(format!("{}", page + 1)),
                    oninput: move |e| page_input.set(e.value()),
                    onkeydown: move |e| {
                        e.stop_propagation();
                        if e.key() == Key::Enter {
                            let v = page_input().trim().to_string();
                            match v.parse::<usize>() {
                                Ok(n) if n >= 1 => st.go_to_page(n - 1),
                                _ => {
                                    // Logical page label typed by the user.
                                    if let Some(i) = st.read_active(|t| t.doc.page_labels().iter().position(|l| *l == v)).flatten() {
                                        st.go_to_page(i);
                                    }
                                }
                            }
                            page_input.set(String::new());
                        } else if e.key() == Key::Escape {
                            page_input.set(String::new());
                        }
                    },
                    onblur: move |_| page_input.set(String::new()),
                }
                span { class: "nowrap", "/ {count}" }
                div { class: "seg",
                    button { title: "{t(lang, \"status.next\")}", onclick: move |_| dispatch(st, "view.goto.nextPage"), Icon { name: "next" } }
                    button { title: "{t(lang, \"status.last\")}", onclick: move |_| dispatch(st, "view.goto.lastPage"), Icon { name: "last" } }
                }
                div { class: "seg sb-opt",
                    button { class: if zoom_mode == Zoom::FitWidth { "active" } else { "" }, title: "{t(lang, \"status.fit_width\")}", onclick: move |_| dispatch(st, "zoom.level.fitWidth"), Icon { name: "fit" } }
                    button { class: if zoom_mode == Zoom::FitPage { "active" } else { "" }, title: "{t(lang, \"status.fit_page\")}", onclick: move |_| dispatch(st, "zoom.level.fitPage"), Icon { name: "page" } }
                }
                div { class: "seg sb-opt",
                    button { class: if layout == Layout::Single { "active" } else { "" }, title: "{t(lang, \"status.single\")}", onclick: move |_| dispatch(st, "view.layout.onePage"), Icon { name: "single" } }
                    button { class: if layout == Layout::Continuous { "active" } else { "" }, title: "{t(lang, \"status.continuous\")}", onclick: move |_| dispatch(st, "view.layout.oneColumn"), Icon { name: "continuous" } }
                    button { class: if layout == Layout::TwoPages { "active" } else { "" }, title: "{t(lang, \"status.two\")}", onclick: move |_| dispatch(st, "view.layout.twoPages"), Icon { name: "two" } }
                }
                div { class: "seg zoom-seg",
                    button { title: "{t(lang, \"status.zoom_out\")} (Ctrl+-)", onclick: move |_| dispatch(st, "zoomOut"), Icon { name: "zoom-out" } }
                    input {
                        class: "sb-opt",
                        r#type: "range", min: "10", max: "400", step: "5", value: "{zoom_pct.clamp(10, 400)}",
                        title: "{t(lang, \"status.zoom\")}",
                        oninput: move |e| { if let Ok(v) = e.value().parse::<f32>() { st.with_active(|t| t.zoom = Zoom::Fixed(v / 100.0)); } },
                    }
                    button { title: "{t(lang, \"status.zoom_in\")} (Ctrl++)", onclick: move |_| dispatch(st, "zoomIn"), Icon { name: "zoom-in" } }
                    button { class: "zoom-pct", title: "{t(lang, \"dialog.goto.title\")}", onclick: move |_| dispatch(st, "zoom.to.level"), "{zoom_pct}%" }
                }
                span { class: "muted sb-opt2 nowrap", "{human_size(size)}" }
            } else {
                span { "{t(lang, \"status.ready\")}" }
            }
        }
    }
}

/// The print job under way, with a button that stops it.
#[component]
fn PrintChip() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let job = crate::commands::print::JOB.read().clone();
    if !job.running {
        return rsx! {};
    }
    use crate::printing::JobProgress;
    let doc = if job.docs > 1 {
        format!(" ({}/{})", job.doc + 1, job.docs)
    } else {
        String::new()
    };
    let text = match job.progress {
        Some(JobProgress::Sending { sent, total }) => tf(
            lang,
            "status.printing_send",
            &[(
                "percent",
                &(sent * 100).checked_div(total).unwrap_or(0).to_string(),
            )],
        ),
        Some(JobProgress::Building { done, total }) | Some(JobProgress::Rendering { sheet: done, total }) => {
            tf(
                lang,
                "status.printing",
                &[("done", &done.to_string()), ("total", &total.to_string())],
            )
        }
        None => t(lang, "status.printing_start"),
    };
    rsx! {
        span { class: "chip print-chip", title: "{text}{doc}",
            Icon { name: "print" }
            span { class: "ellipsis", "{text}{doc}" }
            button { class: "ibtn print-cancel", title: "{t(lang, \"status.print_cancel\")}",
                onclick: move |_| crate::commands::print::cancel(),
                Icon { name: "close" }
            }
        }
    }
}
