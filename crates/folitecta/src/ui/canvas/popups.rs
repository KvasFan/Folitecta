//! Pop-up notes on the page and the floating toolbar of a text selection.

use super::*;

/// An open pop-up note of a comment, in display points.
#[derive(Debug, Clone, PartialEq)]
pub struct PopupView {
    pub(super) index: usize,
    pub(super) rect: [f32; 4],
    pub(super) subtype: String,
    pub(super) author: String,
    pub(super) date: String,
    pub(super) contents: String,
    pub(super) color: [f32; 3],
    pub(super) locked: bool,
    pub(super) replies: Vec<(String, String, String)>,
}

/// Open pop-up notes of the comments of a page.
pub(super) fn popup_views(
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

pub(super) const POPUP_W: f32 = 240.0;
pub(super) const POPUP_H: f32 = 170.0;

/// A pop-up note: the comment text (editable), its replies and a reply box.
/// The header drags the note.
#[component]
pub(super) fn PopupNote(
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
pub(super) fn SelToolbar(left: f32, top: f32) -> Element {
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
