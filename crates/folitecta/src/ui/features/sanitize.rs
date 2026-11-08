//! The Remove Hidden Information dialog: the parts to remove, each with
//! what the document holds of it, removed in one undo step.

use dioxus::prelude::*;
use folitecta_core::redaction::{SanitizeOptions, SanitizeReport};

use crate::i18n::{t, tf};
use crate::state::{AppState, Dialog, ToastKind};
use crate::ui::dialogs::Frame;

/// Remove what `opts` chooses from the active document.
pub fn apply(st: AppState, opts: SanitizeOptions) {
    st.set_dialog(Dialog::None);
    let lang = st.lang();
    match st.with_active(|t| t.doc.sanitize(&opts)) {
        Some(Ok(r)) => {
            crate::files::set_window_title(st);
            let mut parts = Vec::new();
            if r.metadata {
                parts.push(t(lang, "sanitize.done.metadata"));
            }
            for (n, key) in [
                (r.attachments, "sanitize.done.attachments"),
                (r.hidden + r.hidden_layers, "sanitize.done.hidden"),
                (r.scripts, "sanitize.done.scripts"),
            ] {
                if n > 0 {
                    parts.push(tf(lang, key, &[("count", &n.to_string())]));
                }
            }
            if parts.is_empty() {
                st.toast(ToastKind::Info, t(lang, "sanitize.nothing"), "");
            } else {
                st.toast(ToastKind::Ok, t(lang, "sanitize.done"), parts.join(", "));
            }
        }
        Some(Err(e)) => st.core_error(&e),
        None => {}
    }
}

/// What the document holds of a part, for its line.
fn found(lang: crate::i18n::Lang, r: &SanitizeReport, part: &str) -> String {
    let n = match part {
        "metadata" => usize::from(r.metadata),
        "attachments" => r.attachments,
        "hidden" => r.hidden + r.hidden_layers,
        _ => r.scripts,
    };
    if n == 0 {
        t(lang, "sanitize.found_none")
    } else if part == "metadata" {
        t(lang, "sanitize.found_some")
    } else {
        tf(lang, "sanitize.found", &[("count", &n.to_string())])
    }
}

#[component]
pub fn SanitizeDialog() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let preview = use_hook(|| {
        st.read_active(|t| t.doc.sanitize_preview(&SanitizeOptions::default(), &[]))
            .unwrap_or_default()
    });
    let mut parts = use_signal(|| [true; 4]);
    const KEYS: [&str; 4] = ["metadata", "attachments", "hidden", "scripts"];
    let lines: Vec<(usize, &str, String, String)> = KEYS
        .iter()
        .enumerate()
        .map(|(i, key)| {
            (
                i,
                *key,
                t(lang, &format!("redact.sanitize.{key}")),
                found(lang, &preview, key),
            )
        })
        .collect();
    let chosen = parts();
    let opts = SanitizeOptions {
        metadata: chosen[0],
        attachments: chosen[1],
        hidden: chosen[2],
        scripts: chosen[3],
        ..SanitizeOptions::none()
    };
    rsx! {
        Frame { title: t(lang, "sanitize.title"), ok_label: t(lang, "sanitize.remove"), danger: true,
            ok_disabled: !chosen.iter().any(|c| *c),
            on_ok: move |_| apply(st, opts.clone()),
            for (i, key, label, held) in lines {
                label { key: "{key}", class: "check sanitize-part", "data-part": "{key}",
                    input { r#type: "checkbox", checked: chosen[i],
                        onchange: move |_| parts.with_mut(|p| p[i] = !p[i]) }
                    span { "{label} " }
                    span { class: "hint sanitize-found", "({held})" }
                }
            }
            p { class: "hint", "{t(lang, \"sanitize.hint\")}" }
        }
    }
}
