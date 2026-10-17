//! Macro picker of page-mark dialogs, the macro guide and the external
//! application launcher.

use dioxus::prelude::*;
use folitecta_core::MACROS;

use crate::commands::macros::{LAUNCH_MACROS, insert_token, launch};
use crate::i18n::{Lang, t, ui_label};
use crate::state::{AppState, Dialog};

/// Name of a macro token: `{bates:6}` is `bates`.
fn macro_name(token: &str) -> &str {
    let inner = token.trim_start_matches('{').trim_end_matches('}');
    inner.split(':').next().unwrap_or(inner)
}

/// Localized description of a page-mark macro.
pub fn macro_desc(lang: Lang, token: &str, en: &str) -> String {
    ui_label(lang, "macrodesc", macro_name(token), en)
}

/// The part of a token the user is expected to change (`6` in `{bates:6}`).
fn editable_part(token: &str) -> Option<&str> {
    let (_, arg) = token.split_once(':')?;
    Some(arg.trim_end_matches('}'))
}

/// Drop-down that inserts a macro into the dialog's text field focused last.
#[component]
pub fn MacroPicker(#[props(default = false)] launch: bool) -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let items: Vec<(String, String)> = if launch {
        LAUNCH_MACROS
            .iter()
            .map(|(tok, key)| (tok.to_string(), t(lang, key)))
            .collect()
    } else {
        MACROS
            .iter()
            .map(|(tok, en)| (tok.to_string(), macro_desc(lang, tok, en)))
            .collect()
    };
    rsx! {
        div { class: "field macro-picker",
            label { "{t(lang, \"macro.insert\")}" }
            select {
                onchange: move |e| {
                    let v = e.value();
                    if !v.is_empty() {
                        insert_token(st, &v, editable_part(&v));
                    }
                },
                option { value: "", selected: true, "{t(lang, \"macro.choose\")}" }
                for (tok, desc) in items {
                    option { key: "{tok}", value: "{tok}", "{tok}  {desc}" }
                }
            }
        }
    }
}

#[component]
pub fn MacroGuideDialog() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let mut sample = use_signal(|| "{title} - {page}/{pages} - {date}".to_string());
    let page = st.current_page();
    let expand = move |text: &str| {
        st.read_active(|t| t.doc.expand_macros(text, page, 1))
            .unwrap_or_default()
    };
    let rows: Vec<(String, String, String)> = MACROS
        .iter()
        .map(|(tok, en)| (tok.to_string(), macro_desc(lang, tok, en), expand(tok)))
        .collect();
    let has_doc = st.has_doc();
    let preview = expand(&sample());
    rsx! {
        crate::ui::dialogs::Frame { title: t(lang, "macro.guide.title"), wide: true,
            div { class: "hint", "{t(lang, \"macro.guide.hint\")}" }
            div { class: "macro-table",
                div { class: "macro-row head",
                    span { "{t(lang, \"macro.guide.macro\")}" }
                    span { "{t(lang, \"macro.guide.desc\")}" }
                    span { "{t(lang, \"macro.guide.value\")}" }
                    span {}
                }
                for (tok, desc, value) in rows {
                    {
                        let copy = tok.clone();
                        rsx! {
                            div { class: "macro-row", key: "{tok}",
                                code { "{tok}" }
                                span { "{desc}" }
                                span { class: "macro-value", title: "{value}", "{value}" }
                                button { class: "tbtn", title: "{t(lang, \"macro.copy\")}",
                                    onclick: move |_| crate::util::copy_to_clipboard(&copy),
                                    "{t(lang, \"macro.copy\")}"
                                }
                            }
                        }
                    }
                }
            }
            if has_doc {
                div { class: "field",
                    label { "{t(lang, \"macro.guide.try\")}" }
                    input { value: "{sample}", oninput: move |e| sample.set(e.value()) }
                }
                div { class: "macro-preview", "{preview}" }
            }
        }
    }
}

#[component]
pub fn LaunchAppDialog() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let saved = |k: &str| st.ui.peek().value(k).unwrap_or_default().to_string();
    let mut program = use_signal(|| saved("launch.program"));
    let mut args = use_signal(|| {
        let a = saved("launch.args");
        if a.is_empty() { "\"{path}\"".to_string() } else { a }
    });
    let browse = move |_| {
        spawn(async move {
            if let Some(f) = rfd::AsyncFileDialog::new().pick_file().await {
                program.set(f.path().display().to_string());
            }
        });
    };
    rsx! {
        crate::ui::dialogs::Frame { title: t(lang, "launch.title"), wide: true,
            ok_label: Some(t(lang, "launch.run")),
            on_ok: move |_| {
                st.set_value("launch.program", program().trim().to_string());
                st.set_value("launch.args", args().trim().to_string());
                st.set_dialog(Dialog::None);
                launch(st, program(), args());
            },
            div { class: "field",
                label { "{t(lang, \"launch.program\")}" }
                div { class: "row", style: "gap:6px",
                    input { style: "flex:1;min-width:0", value: "{program}", autofocus: true, oninput: move |e| program.set(e.value()) }
                    button { class: "tbtn", onclick: browse, "{t(lang, \"launch.browse\")}" }
                }
            }
            div { class: "field",
                label { "{t(lang, \"launch.args\")}" }
                input { r#type: "text", value: "{args}", oninput: move |e| args.set(e.value()) }
            }
            MacroPicker { launch: true }
            div { class: "hint", "{t(lang, \"launch.hint\")}" }
        }
    }
}
