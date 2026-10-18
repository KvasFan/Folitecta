//! How redacted areas look once applied, and the sets of redaction codes.

use dioxus::prelude::*;
use folitecta_core::redaction::{OVERLAY_FONTS, Overlay, RedactionLook};
use folitecta_core::registry;

use crate::commands::redaction::{
    CodeSet, code_sets, current_look, current_set, keep_look, save_code_sets, selected_mark,
};
use crate::i18n::{Lang, cmd_label, t};
use crate::state::{AppState, Dialog};
use crate::ui::dialogs::Frame;
use crate::util::{color_hex, parse_color};

fn label_of(lang: Lang, id: &str) -> String {
    registry()
        .get(id)
        .map(|c| cmd_label(lang, c))
        .unwrap_or_else(|| id.to_string())
}

const OVERLAYS: &[(Overlay, &str)] = &[
    (Overlay::Standard, "redaction.overlay.standard"),
    (Overlay::Blur, "redaction.overlay.blur"),
    (Overlay::BlurExtra, "redaction.overlay.blurExtra"),
    (Overlay::Pixelate, "redaction.overlay.pixelate"),
    (Overlay::PixelateExtra, "redaction.overlay.pixelateExtra"),
];

#[component]
pub fn RedactionLookDialog() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let mut look = use_signal(|| current_look(st));
    let sets = code_sets(st);
    let mut set = use_signal(|| current_set(st).min(sets.len().saturating_sub(1)));
    let on_mark = selected_mark(st).is_some();
    let l = look();
    let codes: Vec<String> = sets.get(set()).map(|s| s.codes.clone()).unwrap_or_default();
    let size_text = if l.font_size > 0.0 {
        l.font_size.to_string()
    } else {
        String::new()
    };
    let as_pictures = st.value("redact.method") == "picture";
    rsx! {
        Frame { title: label_of(lang, "document.redaction"),
            on_ok: move |_| {
                keep_look(st, &look());
                st.set_value("redact.codeset", set().to_string());
                st.set_dialog(Dialog::None);
            },
            p { class: "hint", if on_mark { "{t(lang, \"redact.for_mark\")}" } else { "{t(lang, \"redact.for_new\")}" } }
            div { class: "field", label { "{label_of(lang, \"redaction.overlayType\")}" }
                div { class: "radio-row",
                    for (o, id) in OVERLAYS.iter().copied() {
                        label { key: "{id}",
                            input { r#type: "radio", name: "redact-overlay", checked: l.overlay == o, onchange: move |_| look.write().overlay = o }
                            span { "{label_of(lang, id)}" }
                        }
                    }
                }
            }
            if l.overlay != Overlay::Standard {
                p { class: "hint", "{t(lang, \"redact.blur_hint\")}" }
            }
            div { class: "field-grid",
                div { class: "field", label { "{label_of(lang, \"overlay.fcolor\")}" }
                    input { r#type: "color", value: "{color_hex(l.fill)}", disabled: l.overlay != Overlay::Standard,
                        oninput: move |e| look.write().fill = parse_color(&e.value()) } }
                div { class: "field", label { "{label_of(lang, \"overlay.textColor\")}" }
                    input { r#type: "color", value: "{color_hex(l.text_color)}", oninput: move |e| look.write().text_color = parse_color(&e.value()) } }
            }
            div { class: "field", label { "{t(lang, \"redact.text\")}" }
                input { value: "{l.text}", placeholder: "{t(lang, \"redact.text_hint\")}", oninput: move |e| look.write().text = e.value() } }
            div { class: "field-grid",
                div { class: "field", label { "{t(lang, \"redact.code_set\")}" }
                    select { onchange: move |e| set.set(e.value().parse().unwrap_or(0)),
                        if sets.is_empty() { option { value: "0", "—" } }
                        for (i, s) in sets.iter().enumerate() {
                            option { key: "{i}", value: "{i}", selected: i == set(), "{s.name}" }
                        }
                    }
                }
                div { class: "field", label { "{t(lang, \"redact.code\")}" }
                    select { onchange: move |e| look.write().text = e.value(),
                        option { value: "", "—" }
                        for c in codes.iter() {
                            option { key: "{c}", value: "{c}", selected: *c == l.text, "{c}" }
                        }
                    }
                }
            }
            div { class: "row",
                button { class: "tbtn", onclick: move |_| st.set_dialog(Dialog::RedactionCode { set: false }), "{label_of(lang, \"redaction.addTextCode\")}" }
                button { class: "tbtn", onclick: move |_| st.set_dialog(Dialog::RedactionCode { set: true }), "{label_of(lang, \"redaction.addTextCodeSet\")}" }
            }
            div { class: "field-grid",
                div { class: "field", label { "{label_of(lang, \"overlay.fontName\")}" }
                    select { onchange: move |e| look.write().font = e.value(),
                        for (name, _) in OVERLAY_FONTS.iter().copied() {
                            option { key: "{name}", value: "{name}", selected: name == l.font, "{name}" }
                        }
                    }
                }
                div { class: "field", label { "{label_of(lang, \"overlay.fontSize\")}" }
                    input { r#type: "number", min: "0", max: "200", step: "1", value: "{size_text}", placeholder: "{t(lang, \"redact.fit\")}",
                        oninput: move |e| look.write().font_size = e.value().replace(',', ".").parse::<f32>().unwrap_or(0.0).clamp(0.0, 200.0) } }
            }
            label { class: "check",
                input { r#type: "checkbox", checked: l.repeat, onchange: move |_| { let r = !look.peek().repeat; look.write().repeat = r; } }
                span { "{label_of(lang, \"overlay.repeatText\")}" }
            }
            // How the marks are applied (for every mark of the documents).
            label { class: "check",
                input { r#type: "checkbox", checked: as_pictures,
                    onchange: move |_| st.set_value("redact.method", if as_pictures { "" } else { "picture" }) }
                span { "{t(lang, \"redact.as_pictures\")}" }
            }
            p { class: "hint", "{t(lang, \"redact.method_hint\")}" }
        }
    }
}

/// A new redaction code (in the set in use) or a new set of codes.
#[component]
pub fn RedactionCodeDialog(set: bool) -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let mut value = use_signal(String::new);
    let title = label_of(
        lang,
        if set {
            "redaction.addTextCodeSet"
        } else {
            "redaction.addTextCode"
        },
    );
    rsx! {
        Frame { title,
            on_ok: move |_| {
                let v = value().trim().to_string();
                if v.is_empty() {
                    return;
                }
                let mut sets = code_sets(st);
                if set {
                    sets.push(CodeSet { name: v, codes: Vec::new() });
                    st.set_value("redact.codeset", (sets.len() - 1).to_string());
                } else {
                    if sets.is_empty() {
                        sets.push(CodeSet { name: t(st.lang(), "redact.codes"), codes: Vec::new() });
                    }
                    let i = current_set(st).min(sets.len() - 1);
                    if !sets[i].codes.contains(&v) {
                        sets[i].codes.push(v.clone());
                    }
                    // The new code is the overlay text from now on.
                    let mut look: RedactionLook = current_look(st);
                    look.text = v;
                    keep_look(st, &look);
                }
                save_code_sets(st, &sets);
                st.set_dialog(Dialog::RedactionLook);
            },
            div { class: "field",
                label { if set { "{t(lang, \"redact.set_name\")}" } else { "{t(lang, \"redact.code\")}" } }
                input { value: "{value}", autofocus: true, placeholder: if set { "FOIA" } else { "(b)(6)" }, oninput: move |e| value.set(e.value()) }
            }
        }
    }
}
