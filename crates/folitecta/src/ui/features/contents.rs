//! Tables of contents: the headings found in the text, to make bookmarks of
//! (each can be left out), and the pages listing the bookmarks, inserted at
//! the start or before the current page.

use std::collections::HashSet;

use dioxus::prelude::*;
use folitecta_core::{Heading, registry};

use crate::i18n::{cmd_label, t, tf};
use crate::state::{AppState, Dialog, Pane, ToastKind};
use crate::ui::dialogs::Frame;

fn title_of(lang: crate::i18n::Lang, id: &str) -> String {
    registry()
        .get(id)
        .map(|c| cmd_label(lang, c).trim_end_matches('…').to_string())
        .unwrap_or_default()
}

#[component]
pub fn HeadingsDialog() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    // Found once, when the dialog opens.
    let found: Vec<Heading> = use_hook(|| st.peek_active(|t| t.doc.find_headings()).unwrap_or_default());
    let had = use_hook(|| st.peek_active(|t| !t.doc.outline().is_empty()).unwrap_or(false));
    let mut left_out = use_signal(HashSet::<usize>::new);
    let mut replace = use_signal(|| true);
    let chosen: Vec<Heading> = found
        .iter()
        .enumerate()
        .filter(|(i, _)| !left_out.read().contains(i))
        .map(|(_, h)| h.clone())
        .collect();
    let count = tf(
        lang,
        "contents.found",
        &[
            ("count", &found.len().to_string()),
            ("chosen", &chosen.len().to_string()),
        ],
    );
    let make = chosen.clone();
    rsx! {
        Frame { title: title_of(lang, "document.bookmarksFromHeadings"), side: true,
            ok_label: Some(t(lang, "contents.make")), ok_disabled: chosen.is_empty(),
            on_ok: move |_| {
                match st.with_active(|t| t.doc.bookmarks_from_headings(&make, !had || replace())) {
                    Some(Ok(n)) => {
                        crate::files::set_window_title(st);
                        st.set_dialog(Dialog::None);
                        crate::commands::show_pane(st, Pane::Bookmarks);
                        st.toast(ToastKind::Ok, tf(st.lang(), "toast.bookmarks_made", &[("count", &n.to_string())]), "");
                    }
                    Some(Err(e)) => st.core_error(&e),
                    None => {}
                }
            },
            p { class: "hint", "{t(lang, \"contents.headings_hint\")}" }
            if found.is_empty() {
                p { class: "hint warn", "{t(lang, \"contents.no_headings\")}" }
            } else {
                p { class: "muted contents-count", "{count}" }
                div { class: "contents-list",
                    for (i, h) in found.iter().enumerate() {
                        {
                            let on = !left_out.read().contains(&i);
                            let page = h.page;
                            let indent = f32::from(h.level.saturating_sub(1)) * 18.0;
                            let label = tf(lang, "contents.page", &[("page", &(page + 1).to_string())]);
                            rsx! {
                                div { key: "{i}", class: if on { "contents-row" } else { "contents-row off" },
                                    input { r#type: "checkbox", checked: on,
                                        onchange: move |_| {
                                            let mut l = left_out.write();
                                            if !l.remove(&i) {
                                                l.insert(i);
                                            }
                                        }
                                    }
                                    button { class: "linkish contents-go", style: "padding-left:{indent}px",
                                        onclick: move |_| st.navigate(page, true),
                                        span { class: if h.level == 1 { "contents-title top" } else { "contents-title" }, "{h.title}" }
                                    }
                                    span { class: "muted contents-page", "{label}" }
                                }
                            }
                        }
                    }
                }
                if had {
                    label { class: "check",
                        input { r#type: "checkbox", class: "contents-replace", checked: replace(), onchange: move |_| replace.toggle() }
                        span { "{t(lang, \"contents.replace\")}" }
                    }
                }
            }
        }
    }
}

#[component]
pub fn ContentsDialog() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let has = st.read_active(|t| !t.doc.outline().is_empty()).unwrap_or(false);
    let mut title = use_signal(|| t(lang, "contents.default_title"));
    let mut place = use_signal(|| "first".to_string());
    rsx! {
        Frame { title: title_of(lang, "document.insertContents"),
            ok_label: Some(t(lang, "contents.insert")), ok_disabled: !has,
            on_ok: move |_| {
                let at = if place() == "first" { 0 } else { st.current_page() };
                let heading = title();
                match st.with_active(|t| t.doc.insert_contents(at, heading.trim())) {
                    Some(Ok(n)) => {
                        crate::files::set_window_title(st);
                        st.set_dialog(Dialog::None);
                        st.navigate(at, true);
                        st.toast(ToastKind::Ok, tf(st.lang(), "toast.contents_inserted", &[("count", &n.to_string())]), "");
                    }
                    Some(Err(e)) => st.core_error(&e),
                    None => {}
                }
            },
            p { class: "hint", "{t(lang, \"contents.hint\")}" }
            div { class: "field", label { "{t(lang, \"contents.title\")}" }
                input { class: "contents-heading", value: "{title}", oninput: move |e| title.set(e.value()) }
            }
            div { class: "field", label { "{t(lang, \"contents.where\")}" }
                div { class: "radio-row",
                    label { input { r#type: "radio", name: "contents-at", checked: place() == "first", onchange: move |_| place.set("first".into()) }
                        span { "{t(lang, \"contents.at_start\")}" } }
                    label { input { r#type: "radio", name: "contents-at", checked: place() == "current", onchange: move |_| place.set("current".into()) }
                        span { "{t(lang, \"contents.at_current\")}" } }
                }
            }
            if !has {
                div { class: "row",
                    span { class: "hint warn", "{t(lang, \"contents.no_bookmarks\")}" }
                    button { class: "tbtn contents-from-headings",
                        onclick: move |_| crate::commands::dispatch(st, "document.bookmarksFromHeadings"),
                        "{title_of(lang, \"document.bookmarksFromHeadings\")}" }
                }
            }
        }
    }
}
