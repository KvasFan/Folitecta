//! Barcode dialogs: the kind and content of a new barcode, and the barcodes
//! read on the pages.

use dioxus::prelude::*;
use folitecta_core::{BarcodeKind, FoundBarcode};

use crate::commands::barcodes::reveal;
use crate::i18n::{t, tf};
use crate::state::{AppState, Dialog, ToastKind, Tool};
use crate::ui::dialogs::Frame;

/// What the kind of barcode can hold.
fn hint_key(kind: BarcodeKind) -> &'static str {
    match kind {
        BarcodeKind::Ean13 | BarcodeKind::Ean8 | BarcodeKind::UpcA | BarcodeKind::UpcE | BarcodeKind::Itf => {
            "barcode.hint.digits"
        }
        BarcodeKind::Code39 | BarcodeKind::Code93 => "barcode.hint.upper",
        BarcodeKind::Codabar => "barcode.hint.codabar",
        BarcodeKind::Code128 => "barcode.hint.ascii",
        _ => "barcode.hint.any",
    }
}

/// The kind and content of a barcode drawn at `rect` (display points, y down).
#[component]
pub fn BarcodeDialog(page: usize, rect: Vec<f32>) -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let mut kind = use_signal(|| {
        st.ui
            .peek()
            .value("barcode.kind")
            .and_then(BarcodeKind::from_id)
            .unwrap_or(BarcodeKind::Qr)
    });
    let mut text = use_signal(String::new);
    let mut caption = use_signal(|| st.ui.peek().value("barcode.caption") != Some("0"));
    rsx! {
        Frame { title: t(lang, "barcode.title"),
            on_ok: move |_| {
                let k = kind();
                let r = [rect[0], rect[1], rect[2], rect[3]];
                let content = text().trim().to_string();
                match st.with_active(|t| t.doc.add_barcode(page, r, k, &content, caption())) {
                    Some(Ok(())) => {
                        st.set_value("barcode.kind", k.id());
                        st.set_value("barcode.caption", if caption() { "1" } else { "0" });
                        crate::files::set_window_title(st);
                        st.set_tool(Tool::Hand);
                        st.set_dialog(Dialog::None);
                    }
                    Some(Err(e)) => st.error(e),
                    None => {}
                }
            },
            div { class: "field-grid",
                div { class: "field", label { "{t(lang, \"barcode.kind\")}" }
                    select { onchange: move |e| { if let Some(k) = BarcodeKind::from_id(&e.value()) { kind.set(k) } },
                        for k in BarcodeKind::ALL {
                            option { key: "{k.id()}", value: "{k.id()}", selected: k == kind(), "{k.label()}" }
                        }
                    }
                }
                div { class: "field", label { "{t(lang, \"barcode.content\")}" }
                    input { r#type: "text", value: "{text}", autofocus: true, oninput: move |e| text.set(e.value()) }
                }
            }
            p { class: "hint", "{t(lang, hint_key(kind()))}" }
            if kind().is_linear() {
                label { class: "check",
                    input { r#type: "checkbox", checked: caption(), onchange: move |_| caption.set(!caption()) }
                    span { "{t(lang, \"barcode.caption\")}" }
                }
            }
        }
    }
}

/// The barcodes read on the pages: what they hold, where they are.
#[component]
pub fn BarcodesDialog(found: Vec<FoundBarcode>) -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let title = tf(lang, "barcode.found", &[("count", &found.len().to_string())]);
    let all = found
        .iter()
        .map(|f| f.text.clone())
        .collect::<Vec<_>>()
        .join("\n");
    let copied = move || st.toast(ToastKind::Ok, t(st.lang(), "toast.barcode_copied"), "");
    rsx! {
        Frame { title, wide: true, ok_label: Some(t(lang, "barcode.copy_all")),
            on_ok: move |_| {
                crate::clipboard::set_text(&all);
                copied();
                st.set_dialog(Dialog::None);
            },
            div { class: "barcode-list",
                for (i, f) in found.into_iter().enumerate() {
                    {
                        let meta = tf(lang, "barcode.where", &[("kind", &f.kind), ("page", &(f.page + 1).to_string())]);
                        let (shown, text) = (f.clone(), f.text.clone());
                        rsx! {
                            div { class: "barcode-row", key: "{i}",
                                div { class: "barcode-text",
                                    span { class: "barcode-value", "{f.text}" }
                                    span { class: "muted", "{meta}" }
                                }
                                button { class: "tbtn barcode-show", onclick: move |_| reveal(st, &shown),
                                    "{t(lang, \"barcode.show\")}" }
                                button { class: "tbtn barcode-copy", onclick: move |_| { crate::clipboard::set_text(&text); copied(); },
                                    "{t(lang, \"barcode.copy\")}" }
                            }
                        }
                    }
                }
            }
        }
    }
}
