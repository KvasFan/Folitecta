//! Library of handwritten signatures and initials, and the signer's profile.

use base64::Engine;
use dioxus::html::input_data::MouseButton;
use dioxus::html::{InteractionElementOffset, PointerInteraction};
use dioxus::prelude::*;

use crate::commands::signs::{SavedMark, add_to_library, library, remove_from_library, start_placing};
use crate::i18n::t;
use crate::icons::Icon;
use crate::state::{AppState, Dialog};

const PAD_W: f64 = 420.0;
const PAD_H: f64 = 140.0;

/// SVG polylines of strokes in a `w` x `h` box (coordinates 0..1).
fn stroke_paths(strokes: &[Vec<(f32, f32)>], w: f64, h: f64) -> Vec<String> {
    strokes
        .iter()
        .map(|s| {
            s.iter()
                .map(|(x, y)| format!("{:.1},{:.1}", *x as f64 * w, *y as f64 * h))
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect()
}

#[component]
fn MarkPreview(mark: SavedMark) -> Element {
    match mark.kind.as_str() {
        "typed" => rsx! { span { class: "typed-sig", "{mark.text}" } },
        "image" => rsx! { img { class: "sig-img", src: "data:image/png;base64,{mark.image}", alt: "" } },
        _ => {
            // Fit the strokes into the preview box (coordinates share one unit).
            let pts: Vec<&(f32, f32)> = mark.strokes.iter().flatten().collect();
            let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
            for (x, y) in &pts {
                x0 = x0.min(*x);
                y0 = y0.min(*y);
                x1 = x1.max(*x);
                y1 = y1.max(*y);
            }
            let (w, h) = ((x1 - x0).max(0.01) as f64, (y1 - y0).max(0.01) as f64);
            let (bw, bh) = (150.0f64, 46.0f64);
            let s = (bw / w).min(bh / h);
            let paths: Vec<String> = mark
                .strokes
                .iter()
                .map(|st| {
                    st.iter()
                        .map(|(x, y)| format!("{:.1},{:.1}", (*x - x0) as f64 * s, (*y - y0) as f64 * s))
                        .collect::<Vec<_>>()
                        .join(" ")
                })
                .collect();
            rsx! {
                svg { class: "sig-svg", width: "{bw}", height: "{bh}", view_box: "-3 -3 {bw + 6.0} {bh + 6.0}",
                    for (i, p) in paths.iter().enumerate() {
                        polyline { key: "{i}", points: "{p}", fill: "none", stroke: "#1c2a6b", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round" }
                    }
                }
            }
        }
    }
}
#[component]
pub fn SignatureLibraryDialog() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let mut refresh = use_signal(|| 0u32);
    let _ = refresh();
    let marks = library();
    let mut tab = use_signal(|| "draw".to_string());
    let mut initials = use_signal(|| false);
    let mut strokes: Signal<Vec<Vec<(f32, f32)>>> = use_signal(Vec::new);
    let mut drawing = use_signal(|| false);
    let mut typed = use_signal(|| st.ui.peek().author.clone());
    let mut image: Signal<Option<Vec<u8>>> = use_signal(|| None);
    let get = |k: &str| {
        st.ui
            .peek()
            .value(&format!("profile.{k}"))
            .unwrap_or_default()
            .to_string()
    };
    let mut name = use_signal(|| st.ui.peek().author.clone());
    let mut email = use_signal(|| get("email"));
    let mut company = use_signal(|| get("company"));
    let mut initials_text = use_signal(|| get("initials"));
    let paths = stroke_paths(&strokes(), PAD_H, PAD_H);
    let can_save = match tab().as_str() {
        "draw" => !strokes().is_empty(),
        "type" => !typed().trim().is_empty(),
        _ => image().is_some(),
    };
    let save = move |_| {
        let mark = SavedMark {
            id: 0,
            initials: initials(),
            kind: match tab().as_str() {
                "type" => "typed",
                "image" => "image",
                _ => "strokes",
            }
            .to_string(),
            strokes: strokes(),
            text: typed().trim().to_string(),
            image: image()
                .map(|d| base64::engine::general_purpose::STANDARD.encode(d))
                .unwrap_or_default(),
            color: [0.05, 0.1, 0.35],
        };
        match add_to_library(mark) {
            Ok(_) => {
                strokes.set(Vec::new());
                image.set(None);
                refresh += 1;
            }
            Err(e) => st.error(e),
        }
    };
    rsx! {
        crate::ui::dialogs::Frame { title: t(lang, "signs.title"), wide: true,
            on_ok: move |_| {
                st.ui_mut().author = name().trim().to_string();
                st.set_value("profile.email", email().trim().to_string());
                st.set_value("profile.company", company().trim().to_string());
                st.set_value("profile.initials", initials_text().trim().to_string());
                st.set_dialog(Dialog::None);
            },
            ok_label: Some(t(lang, "dialog.ok")),
            if marks.is_empty() {
                div { class: "hint", "{t(lang, \"signs.empty\")}" }
            }
            div { class: "sig-library",
                for m in marks.iter().cloned() {
                    {
                        let m2 = m.clone();
                        let id = m.id;
                        rsx! {
                            div { class: "sig-card", key: "{m.id}",
                                div { class: "sig-preview", MarkPreview { mark: m.clone() } }
                                div { class: "row", style: "gap:4px",
                                    span { class: "chip", if m.initials { "{t(lang, \"signs.initials\")}" } else { "{t(lang, \"signs.signature\")}" } }
                                    div { class: "spacer" }
                                    button { class: "tbtn primary", onclick: move |_| { st.set_dialog(Dialog::None); start_placing(st, m2.clone()); }, "{t(lang, \"signs.place\")}" }
                                    button { class: "ibtn danger", title: "{t(lang, \"label.delete\")}", onclick: move |_| { remove_from_library(id); refresh += 1; }, Icon { name: "delete" } }
                                }
                            }
                        }
                    }
                }
            }
            div { class: "prop-head", "{t(lang, \"signs.add\")}" }
            div { class: "seg-buttons",
                for (v, key) in [("draw", "signs.draw"), ("type", "signs.type"), ("image", "signs.image")] {
                    button { class: if tab() == v { "tbtn on" } else { "tbtn" }, key: "{v}", onclick: move |_| tab.set(v.to_string()), "{t(lang, key)}" }
                }
                label { class: "check mini", input { r#type: "checkbox", checked: initials(), onchange: move |_| initials.set(!initials()) } span { "{t(lang, \"signs.initials\")}" } }
            }
            match tab().as_str() {
                "type" => rsx! {
                    div { class: "field", input { value: "{typed}", oninput: move |e| typed.set(e.value()) } }
                    div { class: "typed-sig big", "{typed}" }
                },
                "image" => rsx! {
                    div { class: "row",
                        button { class: "tbtn",
                            onclick: move |_| {
                                spawn(async move {
                                    if let Some(f) = rfd::AsyncFileDialog::new().add_filter("Images", &["png", "jpg", "jpeg"]).pick_file().await {
                                        image.set(Some(f.read().await));
                                    }
                                });
                            },
                            Icon { name: "image" } "{t(lang, \"sig.image\")}"
                        }
                        if image().is_some() { span { class: "chip", "✓" } }
                    }
                },
                _ => rsx! {
                    div { class: "sign-pad", style: "width:{PAD_W}px;height:{PAD_H}px",
                        onmousedown: move |e| {
                            if e.trigger_button() == Some(MouseButton::Primary) {
                                let p = e.element_coordinates();
                                drawing.set(true);
                                strokes.with_mut(|s| s.push(vec![((p.x / PAD_H) as f32, (p.y / PAD_H) as f32)]));
                            }
                        },
                        onmousemove: move |e| {
                            if drawing() {
                                let p = e.element_coordinates();
                                strokes.with_mut(|s| { if let Some(last) = s.last_mut() { last.push(((p.x.clamp(0.0, PAD_W) / PAD_H) as f32, (p.y.clamp(0.0, PAD_H) / PAD_H) as f32)); } });
                            }
                        },
                        onmouseup: move |_| drawing.set(false),
                        onmouseleave: move |_| drawing.set(false),
                        svg { width: "{PAD_W}", height: "{PAD_H}",
                            line { x1: "16", y1: "{PAD_H - 28.0}", x2: "{PAD_W - 16.0}", y2: "{PAD_H - 28.0}", stroke: "#c8cbd6", stroke_width: "1" }
                            for (i, p) in paths.iter().enumerate() {
                                polyline { key: "{i}", points: "{p}", fill: "none", stroke: "#1c2a6b", stroke_width: "2.5", stroke_linecap: "round", stroke_linejoin: "round" }
                            }
                        }
                    }
                    div { class: "row", button { class: "tbtn", onclick: move |_| strokes.set(Vec::new()), "{t(lang, \"signs.clear\")}" } }
                },
            }
            div { class: "row", button { class: "tbtn primary", disabled: !can_save, onclick: save, Icon { name: "save" } "{t(lang, \"signs.save\")}" } }
            div { class: "prop-head", "{t(lang, \"signs.profile\")}" }
            div { class: "field-grid",
                div { class: "field", label { "{t(lang, \"sig.name\")}" } input { value: "{name}", oninput: move |e| name.set(e.value()) } }
                div { class: "field", label { "{t(lang, \"signs.initials\")}" } input { value: "{initials_text}", oninput: move |e| initials_text.set(e.value()) } }
                div { class: "field", label { "E-mail" } input { value: "{email}", oninput: move |e| email.set(e.value()) } }
                div { class: "field", label { "{t(lang, \"signs.company\")}" } input { value: "{company}", oninput: move |e| company.set(e.value()) } }
            }
        }
    }
}
