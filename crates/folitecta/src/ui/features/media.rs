//! Media player, list of the sound and video comments, and their properties.

use dioxus::prelude::*;
use folitecta_core::registry;

use crate::commands::dispatch;
use crate::commands::media::{PLAYER, PLAYER_READS_ADDRESS, close_player, load_player_media, play_item};
use crate::i18n::{Lang, cmd_label, t};
use crate::icons::Icon;
use crate::state::{AppState, Dialog};
use crate::ui::dialogs::Frame;
use crate::util::human_size;

fn label_of(lang: Lang, id: &str) -> String {
    registry()
        .get(id)
        .map(|c| cmd_label(lang, c))
        .unwrap_or_else(|| id.to_string())
}

/// The player of a sound or video comment (not modal).
#[component]
pub fn MediaPlayer() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let Some(p) = PLAYER() else {
        return rsx! {};
    };
    // The player belongs to its document.
    if st.read_active(|t| t.id) != Some(p.tab) {
        return rsx! {};
    }
    let src = format!("/media/{}", p.id);
    // Read by the element itself, or fetched by the page (Linux).
    let (direct, fetched) = if PLAYER_READS_ADDRESS {
        (Some(src), None)
    } else {
        (None, Some(src))
    };
    let video = p.kind == "video";
    let class = match (video, p.floating) {
        (_, true) => "media-player floating",
        (true, false) => "media-player video",
        _ => "media-player",
    };
    let title = if p.name.is_empty() {
        t(lang, "media.player")
    } else {
        p.name.clone()
    };
    let float_icon = if p.floating { "zoom-out" } else { "zoom-in" };
    let float_title = label_of(
        lang,
        if p.floating {
            "mediaAnnot.stopViewFloating"
        } else {
            "mediaAnnot.startViewFloating"
        },
    );
    let tools: [(&str, &str); 4] = [
        ("media.play_pause", "next"),
        ("media.stop", "close"),
        ("media.mute", "media"),
        ("mediaAnnot.properties", "info"),
    ];
    rsx! {
        div { class: "{class}", onclick: move |e| e.stop_propagation(), onmousedown: move |e| e.stop_propagation(),
            div { class: "media-head",
                Icon { name: "media" }
                span { class: "media-title", title: "{title}", "{title}" }
                div { class: "spacer" }
                button { class: "ibtn", title: "{float_title}",
                    onclick: move |_| dispatch(st, if p.floating { "mediaAnnot.stopViewFloating" } else { "mediaAnnot.startViewFloating" }),
                    Icon { name: float_icon } }
                if video {
                    button { class: "ibtn", title: "{label_of(lang, \"mediaAnnot.startFullScreen\")}", onclick: move |_| dispatch(st, "mediaAnnot.startFullScreen"), Icon { name: "fullscreen" } }
                }
                button { class: "ibtn", title: "{t(lang, \"dialog.close\")}", onclick: move |_| close_player(), "×" }
            }
            if video {
                video { id: "folitecta-media", src: direct.clone(), "data-src": fetched.clone(), controls: true, preload: "auto",
                    onmounted: move |_| load_player_media() }
            } else {
                audio { id: "folitecta-media", src: direct.clone(), "data-src": fetched.clone(), controls: true, preload: "auto",
                    onmounted: move |_| load_player_media() }
            }
            div { class: "media-tools",
                for (id, icon) in tools {
                    button { class: "ibtn", key: "{id}", title: "{label_of(lang, id)}", onclick: move |_| dispatch(st, id), Icon { name: icon } }
                }
                input { id: "folitecta-media-volume", r#type: "range", min: "0", max: "1", step: "0.05", value: "1", title: "{label_of(lang, \"volume\")}",
                    oninput: move |e| {
                        let v = e.value().parse::<f64>().unwrap_or(1.0).clamp(0.0, 1.0);
                        spawn(async move { let _ = document::eval(&format!("const m = document.getElementById('folitecta-media'); if (m) m.volume = {v};")).await; });
                    } }
            }
        }
    }
}

#[component]
pub fn MediaListDialog() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let items = st.read_active(|t| t.doc.media_items()).unwrap_or_default();
    rsx! {
        Frame { title: label_of(lang, "media.list"), wide: true,
            if items.is_empty() { p { class: "muted", "{t(lang, \"media.none\")}" } }
            div { class: "media-list",
                for m in items.into_iter() {
                    {
                        let (page, index) = (m.page, m.index);
                        let kind = t(lang, &format!("media.kind.{}", m.kind));
                        let name = if m.title.is_empty() { m.file_name.clone() } else { format!("{} — {}", m.title, m.file_name) };
                        rsx! {
                            div { class: "media-row", key: "{page}-{index}",
                                Icon { name: "media" }
                                div { class: "media-text",
                                    span { class: "media-name", title: "{name}", "{name}" }
                                    span { class: "muted", "{kind} · {m.mime} · {human_size(m.size as u64)} · {t(lang, \"status.page\")} {page + 1}" }
                                }
                                button { class: "tbtn", onclick: move |_| { st.set_dialog(Dialog::None); st.go_to_page(page); play_item(st, page, index); }, "{label_of(lang, \"media.play\")}" }
                                button { class: "ibtn", title: "{label_of(lang, \"mediaAnnot.properties\")}", onclick: move |_| st.set_dialog(Dialog::MediaProps { page, index }), Icon { name: "info" } }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
pub fn MediaPropsDialog(page: usize, index: usize) -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let item = st
        .read_active(|t| {
            t.doc
                .media_items()
                .into_iter()
                .find(|m| m.page == page && m.index == index)
        })
        .flatten();
    let description = st
        .read_active(|t| {
            t.doc
                .annotations(page)
                .ok()
                .and_then(|a| a.into_iter().nth(index))
                .map(|a| a.contents)
        })
        .flatten()
        .unwrap_or_default();
    let Some(item) = item else {
        return rsx! { Frame { title: label_of(lang, "mediaAnnot.properties"), p { "{t(lang, \"toast.select_media\")}" } } };
    };
    let mut title = use_signal(|| item.title.clone());
    let mut desc = use_signal(|| description.clone());
    let video = item.kind == "video";
    rsx! {
        Frame { title: label_of(lang, "mediaAnnot.properties"),
            on_ok: move |_| {
                let r = st.with_active(|t| t.doc.set_media_text(page, index, &title(), &desc()));
                match r {
                    Some(Ok(())) => { crate::files::set_window_title(st); st.set_dialog(Dialog::None); }
                    Some(Err(e)) => st.error(e),
                    None => {}
                }
            },
            div { class: "field", label { "{t(lang, \"media.title\")}" }
                input { value: "{title}", oninput: move |e| title.set(e.value()) } }
            div { class: "field", label { "{t(lang, \"media.description\")}" }
                textarea { rows: "3", value: "{desc}", oninput: move |e| desc.set(e.value()) } }
            div { class: "props",
                div { class: "prop", span { class: "k", "{t(lang, \"media.file\")}" } span { class: "v", "{item.file_name}" } }
                div { class: "prop", span { class: "k", "{t(lang, \"media.type\")}" } span { class: "v", "{item.mime}" } }
                div { class: "prop", span { class: "k", "{t(lang, \"media.size\")}" } span { class: "v", "{human_size(item.size as u64)}" } }
                div { class: "prop", span { class: "k", "{t(lang, \"status.page\")}" } span { class: "v", "{page + 1}" } }
            }
            div { class: "row", style: "gap:6px;flex-wrap:wrap",
                button { class: "tbtn", onclick: move |_| { st.set_dialog(Dialog::None); play_item(st, page, index); }, "{label_of(lang, \"media.play\")}" }
                button { class: "tbtn", onclick: move |_| { st.set_dialog(Dialog::None); st.with_active(|t| t.selected_annot = Some((page, index))); dispatch(st, if video { "mediaAnnot.saveVideo" } else { "mediaAnnot.saveAudio" }); },
                    if video { "{label_of(lang, \"mediaAnnot.saveVideo\")}" } else { "{label_of(lang, \"mediaAnnot.saveAudio\")}" } }
            }
        }
    }
}
