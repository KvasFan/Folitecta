//! The Convert Colours to Grey dialog: what would become grey and what
//! would be left as it is, then the conversion.

use dioxus::prelude::*;

use crate::commands::greyscale::{self, GREY};
use crate::i18n::{t, tf};
use crate::state::AppState;
use crate::ui::dialogs::Frame;

#[component]
pub fn ConvertGreyDialog() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let estimate = GREY.read().clone();
    let (summary, kept, ready) = match estimate.as_ref() {
        None => (t(lang, "grey.estimating"), Vec::new(), false),
        Some(Err(e)) => (e.clone(), Vec::new(), false),
        Some(Ok(r)) if r.is_empty() => (t(lang, "grey.nothing"), Vec::new(), false),
        Some(Ok(r)) => (
            tf(
                lang,
                "grey.will",
                &[
                    ("colours", &r.colours.to_string()),
                    ("pictures", &r.pictures.to_string()),
                    ("comments", &r.comments.to_string()),
                ],
            ),
            r.kept
                .iter()
                .map(|(k, n)| format!("{} ({n})", t(lang, &format!("grey.kept.{k}"))))
                .collect::<Vec<_>>(),
            true,
        ),
    };
    rsx! {
        Frame { title: t(lang, "grey.title"), ok_label: t(lang, "grey.convert"), ok_disabled: !ready,
            on_ok: move |_| greyscale::convert(st),
            div { class: "callout grey-summary", "{summary}" }
            if !kept.is_empty() {
                p { class: "hint grey-kept", "{t(lang, \"grey.kept\")} {kept.join(\", \")}" }
            }
            p { class: "hint", "{t(lang, \"grey.hint\")}" }
        }
    }
}
