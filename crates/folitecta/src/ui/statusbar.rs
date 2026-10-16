//! Status bar: navigation, zoom, layout and document facts.

use dioxus::prelude::*;

use crate::commands::{self, current_zoom, dispatch};
use crate::i18n::t;
use crate::icons::Icon;
use crate::state::{AppState, Layout, Tool, Zoom};
use crate::util::human_size;

#[component]
pub fn StatusBar() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let has_doc = st.has_doc();
    let page = st.current_page();
    let count = st.page_count();
    let zoom_pct = (current_zoom(st) * 100.0).round() as i32;
    let (layout, size, selected, tool) = st
        .read_active(|t| {
            (
                t.layout,
                t.doc.bytes().len() as u64,
                t.selected_pages.len(),
                (st.tool)(),
            )
        })
        .unwrap_or((Layout::Continuous, 0, 0, Tool::Hand));
    let mut page_input = use_signal(String::new);
    let tool_label = t(
        lang,
        match tool {
            Tool::Hand => "tool.hand",
            Tool::Select => "tool.select",
            Tool::Zoom => "cmd.tool.zoom",
            Tool::Note => "tool.note",
            Tool::Rect => "tool.rect",
            Tool::Ellipse => "tool.ellipse",
            Tool::Line => "tool.line",
            Tool::Arrow => "tool.arrow",
            Tool::Highlight => "tool.highlight",
            Tool::TextBox => "tool.text",
            Tool::Ink => "tool.ink",
            Tool::Link => "tool.link",
        },
    );
    let busy = (st.busy)();

    rsx! {
        div { class: "statusbar",
            span { class: "chip", Icon { name: "hand" } "{tool_label}" }
            if selected > 0 {
                span { class: "chip", "{selected} {t(lang, \"dialog.pages\")}" }
            }
            if busy {
                span { class: "chip", "{t(lang, \"status.rendering\")}" }
            }
            div { class: "spacer" }
            if has_doc {
                div { class: "seg",
                    button { title: "First", onclick: move |_| dispatch(st, "view.goto.firstPage"), Icon { name: "first" } }
                    button { title: "Previous", onclick: move |_| dispatch(st, "view.goto.prevPage"), Icon { name: "prev" } }
                }
                span { "{t(lang, \"status.page\")}" }
                input {
                    class: "page-input",
                    r#type: "text",
                    value: if page_input().is_empty() { format!("{}", page + 1) } else { page_input() },
                    onfocus: move |_| page_input.set(format!("{}", page + 1)),
                    oninput: move |e| page_input.set(e.value()),
                    onkeydown: move |e| {
                        e.stop_propagation();
                        if e.key() == Key::Enter {
                            if let Ok(n) = page_input().trim().parse::<usize>() {
                                if n >= 1 { st.go_to_page(n - 1); }
                            }
                            page_input.set(String::new());
                        }
                    },
                    onblur: move |_| page_input.set(String::new()),
                }
                span { "{t(lang, \"status.of\")} {count}" }
                div { class: "seg",
                    button { title: "Next", onclick: move |_| dispatch(st, "view.goto.nextPage"), Icon { name: "next" } }
                    button { title: "Last", onclick: move |_| dispatch(st, "view.goto.lastPage"), Icon { name: "last" } }
                }
                div { class: "spacer" }
                div { class: "seg",
                    button { title: "Zoom out (Ctrl+-)", onclick: move |_| dispatch(st, "zoomOut"), Icon { name: "zoom-out" } }
                    input {
                        r#type: "range", min: "10", max: "400", step: "5", value: "{zoom_pct.clamp(10, 400)}",
                        oninput: move |e| { if let Ok(v) = e.value().parse::<f32>() { st.with_active(|t| t.zoom = Zoom::Fixed(v / 100.0)); } },
                    }
                    button { title: "Zoom in (Ctrl++)", onclick: move |_| dispatch(st, "zoomIn"), Icon { name: "zoom-in" } }
                    span { class: "zoom-pct", "{zoom_pct}%" }
                }
                div { class: "seg",
                    button { class: if st.read_active(|t| t.zoom == Zoom::FitWidth).unwrap_or(false) { "active" } else { "" }, title: "{t(lang, \"status.fit_width\")}", onclick: move |_| dispatch(st, "zoom.level.fitWidth"), Icon { name: "fit" } }
                    button { class: if st.read_active(|t| t.zoom == Zoom::FitPage).unwrap_or(false) { "active" } else { "" }, title: "{t(lang, \"status.fit_page\")}", onclick: move |_| dispatch(st, "zoom.level.fitPage"), Icon { name: "page" } }
                }
                div { class: "seg",
                    button { class: if layout == Layout::Single { "active" } else { "" }, title: "{t(lang, \"status.single\")}", onclick: move |_| dispatch(st, "view.layout.onePage"), Icon { name: "single" } }
                    button { class: if layout == Layout::Continuous { "active" } else { "" }, title: "{t(lang, \"status.continuous\")}", onclick: move |_| dispatch(st, "view.layout.oneColumn"), Icon { name: "continuous" } }
                    button { class: if layout == Layout::TwoPages { "active" } else { "" }, title: "{t(lang, \"status.two\")}", onclick: move |_| dispatch(st, "view.layout.twoPages"), Icon { name: "two" } }
                }
                span { class: "muted", "{human_size(size)}" }
            } else {
                span { "{t(lang, \"status.ready\")}" }
            }
            span { class: "muted", "{commands::implemented_ids().len() + commands::partial_ids().len()}/{folitecta_core::registry().len()}" }
        }
    }
}
