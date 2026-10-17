//! Dialogs of the document tools.

use dioxus::prelude::*;
use folitecta_core::doctools::DOCUMENT_EVENTS;

use crate::i18n::{t, tf, ui_label};
use crate::state::{AppState, Dialog, ToastKind};
use crate::ui::dialogs::Frame;
use crate::util::{format_pages, human_size, parse_page_range};

/// Pages with page marks of a kind, removing them or adding more.
#[component]
pub fn ManageMarksDialog(kind: String) -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let (pages, selected, count) = st
        .read_active(|t| {
            let pages: Vec<usize> = (0..t.doc.page_count())
                .filter(|p| t.doc.marks_on_page(*p).contains(&kind))
                .collect();
            (pages, t.selected_pages.clone(), t.doc.page_count())
        })
        .unwrap_or_default();
    let title = t(lang, &format!("marks.manage.{kind}"));
    let summary = if pages.is_empty() {
        t(lang, "marks.none")
    } else {
        tf(
            lang,
            "marks.on_pages",
            &[
                ("pages", &format_pages(&pages)),
                ("count", &pages.len().to_string()),
                ("total", &count.to_string()),
            ],
        )
    };
    let add = match kind.as_str() {
        "Watermark" => Dialog::Watermark,
        "HeaderFooter" => Dialog::HeaderFooter,
        _ => Dialog::Bates,
    };
    let k_all = kind.clone();
    let k_sel = kind.clone();
    let p_all = pages.clone();
    let sel_with: Vec<usize> = selected.iter().copied().filter(|p| pages.contains(p)).collect();
    let remove = move |k: String, list: Vec<usize>| {
        let r = st.with_active(|t| t.doc.remove_marks_of(&list, &[k.as_str()]));
        match r {
            Some(Ok(n)) => {
                crate::files::set_window_title(st);
                st.toast(
                    ToastKind::Ok,
                    tf(st.lang(), "toast.marks_removed", &[("count", &n.to_string())]),
                    "",
                );
            }
            Some(Err(e)) => st.error(e),
            None => {}
        }
    };
    rsx! {
        Frame { title,
            div { class: "hint", "{summary}" }
            div { class: "row", style: "gap:6px;flex-wrap:wrap",
                button { class: "tbtn", onclick: move |_| st.set_dialog(add.clone()), "{t(lang, \"marks.add\")}" }
                button { class: "tbtn danger", disabled: p_all.is_empty(), onclick: move |_| remove(k_all.clone(), p_all.clone()), "{t(lang, \"marks.remove_all\")}" }
                if !sel_with.is_empty() {
                    button { class: "tbtn danger", onclick: move |_| remove(k_sel.clone(), sel_with.clone()), "{t(lang, \"marks.remove_selected\")}" }
                }
            }
        }
    }
}

/// Space used by each kind of content.
#[component]
pub fn SpaceAuditDialog() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let (items, file) = st
        .read_active(|t| (t.doc.space_audit(), t.doc.bytes().len() as u64))
        .unwrap_or_default();
    let total: u64 = items.iter().map(|i| i.bytes).sum::<u64>().max(1);
    rsx! {
        Frame { title: t(lang, "audit.title"), wide: true,
            div { class: "hint", "{tf(lang, \"audit.hint\", &[(\"size\", &human_size(file))])}" }
            div { class: "audit-table",
                div { class: "audit-row head",
                    span { "{t(lang, \"audit.kind\")}" }
                    span { "{t(lang, \"attach.col.size\")}" }
                    span { "%" }
                    span { "{t(lang, \"audit.objects\")}" }
                }
                for it in items {
                    {
                        let pct = it.bytes as f64 * 100.0 / total as f64;
                        let label = ui_label(lang, "audit", it.category, it.category);
                        rsx! {
                            div { class: "audit-row", key: "{it.category}",
                                span { "{label}" }
                                span { "{human_size(it.bytes)}" }
                                span { class: "audit-bar",
                                    span { class: "fill", style: "width:{pct.min(100.0):.1}%" }
                                    span { class: "pct", "{pct:.1}%" }
                                }
                                span { "{it.objects}" }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Recompress the pictures of the document or of the current page.
#[component]
pub fn RecompressDialog(current: bool) -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let mut quality = use_signal(|| "70".to_string());
    let mut max_px = use_signal(|| "2000".to_string());
    let title = if current {
        t(lang, "recompress.title_page")
    } else {
        t(lang, "recompress.title")
    };
    rsx! {
        Frame { title,
            on_ok: move |_| {
                let q: u8 = quality().trim().parse::<u32>().unwrap_or(70).clamp(10, 100) as u8;
                let m: u32 = max_px().trim().parse().unwrap_or(0);
                let r = st.with_active(|t| {
                    let page = [t.page];
                    t.doc.recompress_images(current.then_some(&page[..]), q, (m > 0).then_some(m))
                });
                match r {
                    Some(Ok(rep)) => {
                        crate::files::set_window_title(st);
                        st.set_dialog(Dialog::None);
                        let msg = if rep.images == 0 {
                            t(lang, "toast.recompress_none")
                        } else {
                            tf(lang, "toast.recompressed", &[("count", &rep.images.to_string()), ("before", &human_size(rep.before)), ("after", &human_size(rep.after))])
                        };
                        st.toast(ToastKind::Ok, msg, "");
                    }
                    Some(Err(e)) => st.error(e),
                    None => {}
                }
            },
            div { class: "field-grid",
                div { class: "field", label { "{t(lang, \"recompress.quality\")}" }
                    input { r#type: "number", min: "10", max: "100", value: "{quality}", oninput: move |e| quality.set(e.value()) } }
                div { class: "field", label { "{t(lang, \"recompress.max\")}" }
                    input { r#type: "number", min: "0", step: "100", value: "{max_px}", oninput: move |e| max_px.set(e.value()) } }
            }
            div { class: "hint", "{t(lang, \"recompress.hint\")}" }
        }
    }
}

/// Pages of typed text inserted after the current page.
#[component]
pub fn InsertTextPagesDialog() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let mut text = use_signal(String::new);
    rsx! {
        Frame { title: t(lang, "inserttext.title"), wide: true,
            on_ok: move |_| {
                st.set_dialog(Dialog::None);
                crate::commands::doctools::insert_text_pages(st, &text());
            },
            div { class: "field",
                label { "{t(lang, \"stamps.text\")}" }
                textarea { class: "tall", value: "{text}", autofocus: true, oninput: move |e| text.set(e.value()) }
            }
            div { class: "hint", "{t(lang, \"inserttext.hint\")}" }
        }
    }
}

/// The selected signature repeated on other pages.
#[component]
pub fn MultiPlaceDialog() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let mut all = use_signal(|| true);
    let mut range = use_signal(String::new);
    let count = st.page_count();
    rsx! {
        Frame { title: t(lang, "multiplace.title"),
            on_ok: move |_| {
                let pages = if all() { Ok((0..count).collect::<Vec<_>>()) } else { parse_page_range(&range(), count) };
                let pages = match pages { Ok(p) => p, Err(e) => return st.error(e) };
                let r = st.with_active(|t| {
                    let (p, i) = t.selected_annot.ok_or_else(|| folitecta_core::Error::Invalid("select a placed signature first".into()))?;
                    t.doc.repeat_signature_mark(p, i, &pages)
                });
                match r {
                    Some(Ok(n)) => {
                        crate::files::set_window_title(st);
                        st.set_dialog(Dialog::None);
                        st.toast(ToastKind::Ok, tf(lang, "toast.signature_repeated", &[("count", &n.to_string())]), "");
                    }
                    Some(Err(e)) => st.error(e),
                    None => {}
                }
            },
            div { class: "radio-col",
                label { class: "check", input { r#type: "radio", name: "mp", checked: all(), onchange: move |_| all.set(true) } span { "{t(lang, \"dialog.pages.all\")} ({count})" } }
                label { class: "check", input { r#type: "radio", name: "mp", checked: !all(), onchange: move |_| all.set(false) } span { "1-3, 7…" } }
            }
            if !all() {
                input { value: "{range}", placeholder: "1-3, 7", oninput: move |e| range.set(e.value()) }
            }
            div { class: "hint", "{t(lang, \"multiplace.hint\")}" }
        }
    }
}

/// Scripts run on document events.
#[component]
pub fn DocumentActionsDialog() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let actions = st.read_active(|t| t.doc.document_actions()).unwrap_or_default();
    let mut event = use_signal(|| "Open".to_string());
    let code_of = |e: &str| {
        actions
            .iter()
            .find(|(k, _)| k == e)
            .map(|(_, c)| c.clone())
            .unwrap_or_default()
    };
    let mut code = use_signal(|| code_of("Open"));
    let has: Vec<String> = actions.iter().map(|(k, _)| k.clone()).collect();
    let all = actions.clone();
    rsx! {
        Frame { title: t(lang, "actions.title"), wide: true, ok_label: Some(t(lang, "actions.save")),
            on_ok: move |_| {
                let r = st.with_active(|t| t.doc.set_document_action(&event(), &code()));
                match r {
                    Some(Err(e)) => st.error(e),
                    _ => {
                        crate::files::set_window_title(st);
                        st.toast(ToastKind::Ok, t(lang, "toast.action_saved"), "");
                    }
                }
            },
            div { class: "field",
                label { "{t(lang, \"actions.event\")}" }
                select {
                    onchange: move |e| {
                        let v = e.value();
                        let c = all.iter().find(|(k, _)| *k == v).map(|(_, c)| c.clone()).unwrap_or_default();
                        event.set(v);
                        code.set(c);
                    },
                    for (k, en) in DOCUMENT_EVENTS.iter() {
                        {
                            let mark = if has.iter().any(|h| h == k) { " ●" } else { "" };
                            rsx! { option { key: "{k}", value: "{k}", selected: event() == *k, "{ui_label(lang, \"actions.ev\", k, en)}{mark}" } }
                        }
                    }
                }
            }
            div { class: "field",
                label { "JavaScript" }
                textarea { class: "tall code", value: "{code}", spellcheck: false, oninput: move |e| code.set(e.value()) }
            }
            div { class: "hint", "{t(lang, \"actions.hint\")}" }
        }
    }
}
