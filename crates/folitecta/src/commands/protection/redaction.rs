//! How redacted areas look once applied (cover, overlay text, redaction
//! codes): the look of new marks and of the selected mark, and the sets of
//! redaction codes.

use folitecta_core::redaction::{
    Overlay, RedactionLook, RedactionMethod, SANITIZE_BY_DEFAULT, SanitizeOptions,
};
use serde::{Deserialize, Serialize};

use crate::commands::{handlers, no_doc};
use crate::i18n::t;
use crate::state::{AppState, Dialog};

/// A named set of redaction codes (the reasons areas are redacted).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CodeSet {
    pub name: String,
    pub codes: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
struct StoredLook {
    overlay: String,
    fill: [f32; 3],
    text: String,
    font: String,
    font_size: f32,
    text_color: [f32; 3],
    repeat: bool,
}

fn overlay_name(o: Overlay) -> &'static str {
    match o {
        Overlay::Standard => "standard",
        Overlay::Blur => "blur",
        Overlay::BlurExtra => "blurExtra",
        Overlay::Pixelate => "pixelate",
        Overlay::PixelateExtra => "pixelateExtra",
    }
}

fn overlay_of(name: &str) -> Overlay {
    Overlay::ALL
        .into_iter()
        .find(|o| overlay_name(*o) == name)
        .unwrap_or_default()
}

/// The look given to new redaction marks.
pub fn default_look(st: AppState) -> RedactionLook {
    let Some(s) = serde_json::from_str::<StoredLook>(&st.value("redact.look")).ok() else {
        return RedactionLook::default();
    };
    RedactionLook {
        overlay: overlay_of(&s.overlay),
        fill: s.fill,
        text: s.text,
        font: if s.font.is_empty() {
            "Helvetica".into()
        } else {
            s.font
        },
        font_size: s.font_size.max(0.0),
        text_color: s.text_color,
        repeat: s.repeat,
    }
}

pub fn save_default_look(st: AppState, look: &RedactionLook) {
    let s = StoredLook {
        overlay: overlay_name(look.overlay).into(),
        fill: look.fill,
        text: look.text.clone(),
        font: look.font.clone(),
        font_size: look.font_size,
        text_color: look.text_color,
        repeat: look.repeat,
    };
    if let Ok(json) = serde_json::to_string(&s) {
        st.set_value("redact.look", json);
    }
}

/// The sanitizing parts kept in the preferences (missing ones take their default).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
struct StoredSanitize {
    metadata: bool,
    attachments: bool,
    hidden: bool,
    quoting: bool,
    scripts: bool,
}

impl Default for StoredSanitize {
    fn default() -> Self {
        StoredSanitize {
            metadata: SANITIZE_BY_DEFAULT,
            attachments: SANITIZE_BY_DEFAULT,
            hidden: SANITIZE_BY_DEFAULT,
            quoting: SANITIZE_BY_DEFAULT,
            scripts: SANITIZE_BY_DEFAULT,
        }
    }
}

/// The sanitizing chosen for redactions (the placeholder in the language of
/// the interface).
pub fn sanitize_options(st: AppState) -> SanitizeOptions {
    let s: StoredSanitize = serde_json::from_str(&st.value("redact.sanitize")).unwrap_or_default();
    SanitizeOptions {
        metadata: s.metadata,
        attachments: s.attachments,
        hidden: s.hidden,
        quoting: s.quoting,
        scripts: s.scripts,
        placeholder: t(st.lang(), "redact.placeholder"),
        skip_terms: Vec::new(),
    }
}

pub fn save_sanitize_options(st: AppState, o: &SanitizeOptions) {
    let s = StoredSanitize {
        metadata: o.metadata,
        attachments: o.attachments,
        hidden: o.hidden,
        quoting: o.quoting,
        scripts: o.scripts,
    };
    if let Ok(json) = serde_json::to_string(&s) {
        st.set_value("redact.sanitize", json);
    }
}

/// Whether applying a redaction saves the result as a new file and checks
/// it first (on unless turned off).
pub fn save_copy_after_apply(st: AppState) -> bool {
    st.value("redact.save_copy") != "off"
}

pub fn set_save_copy_after_apply(st: AppState, on: bool) {
    st.set_value("redact.save_copy", if on { "on" } else { "off" });
}

/// How the marks are applied: by content, or every marked page as a picture.
pub fn redaction_method(st: AppState) -> RedactionMethod {
    if st.value("redact.method") == "picture" {
        RedactionMethod::Picture
    } else {
        RedactionMethod::Content
    }
}

/// The selected comment when it is a redaction mark: page and index.
pub fn selected_mark(st: AppState) -> Option<(usize, usize)> {
    st.read_active(|t| {
        let (p, i) = t.selected_annot?;
        let a = t.doc.annotations(p).ok()?.into_iter().find(|a| a.index == i)?;
        (a.subtype == "Redact").then_some((p, i))
    })
    .flatten()
}

/// The look being changed: the selected mark's, or the one of new marks.
pub fn current_look(st: AppState) -> RedactionLook {
    selected_mark(st)
        .and_then(|(p, i)| st.read_active(|t| t.doc.redaction_look(p, i)).flatten())
        .unwrap_or_else(|| default_look(st))
}

/// Keep a look: on the selected mark, and for the marks made from now on.
pub fn keep_look(st: AppState, look: &RedactionLook) {
    if let Some((p, i)) = selected_mark(st) {
        let r = st.with_active(|t| {
            let r = t.doc.set_redaction_look(p, i, look);
            t.selected_annot = Some((p, i));
            r
        });
        match r {
            Some(Err(e)) => st.error(e),
            _ => {
                st.with_active(|t| t.selected_annot = Some((p, i)));
                crate::files::set_window_title(st);
            }
        }
    }
    save_default_look(st, look);
}

pub fn code_sets(st: AppState) -> Vec<CodeSet> {
    serde_json::from_str(&st.value("redact.codesets")).unwrap_or_default()
}

pub fn save_code_sets(st: AppState, sets: &[CodeSet]) {
    if let Ok(json) = serde_json::to_string(sets) {
        st.set_value("redact.codesets", json);
    }
}

/// The set of codes in use.
pub fn current_set(st: AppState) -> usize {
    st.value("redact.codeset").parse().unwrap_or(0)
}

fn set_overlay(st: AppState, o: Overlay) {
    let mut look = current_look(st);
    look.overlay = o;
    keep_look(st, &look);
}

fn toggle_repeat(st: AppState) {
    let mut look = current_look(st);
    look.repeat = !look.repeat;
    keep_look(st, &look);
}

fn open(st: AppState) {
    if !no_doc(st) {
        st.set_dialog(Dialog::RedactionLook);
    }
}

/// Check states of the redaction look commands.
pub fn redaction_state(st: AppState, id: &str) -> Option<bool> {
    let look = || current_look(st);
    match id {
        "redaction.overlay.standard" => Some(look().overlay == Overlay::Standard),
        "redaction.overlay.blur" => Some(look().overlay == Overlay::Blur),
        "redaction.overlay.blurExtra" => Some(look().overlay == Overlay::BlurExtra),
        "redaction.overlay.pixelate" => Some(look().overlay == Overlay::Pixelate),
        "redaction.overlay.pixelateExtra" => Some(look().overlay == Overlay::PixelateExtra),
        "overlay.repeatText" => Some(look().repeat),
        _ => None,
    }
}

handlers! {
    "document.redaction" => open,
    "redaction.overlayType" => open,
    "overlay.fcolor" => open,
    "overlay.fontName" => open,
    "overlay.fontSize" => open,
    "overlay.textColor" => open,
    "overlay.repeatText" => toggle_repeat,
    "redaction.overlay.standard" => |st| set_overlay(st, Overlay::Standard),
    "redaction.overlay.blur" => |st| set_overlay(st, Overlay::Blur),
    "redaction.overlay.blurExtra" => |st| set_overlay(st, Overlay::BlurExtra),
    "redaction.overlay.pixelate" => |st| set_overlay(st, Overlay::Pixelate),
    "redaction.overlay.pixelateExtra" => |st| set_overlay(st, Overlay::PixelateExtra),
    "redaction.addTextCode" => |st| st.set_dialog(Dialog::RedactionCode { set: false }),
    "redaction.addTextCodeSet" => |st| st.set_dialog(Dialog::RedactionCode { set: true }),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitizing_is_on_unless_stored_off() {
        let s: StoredSanitize = serde_json::from_str("").unwrap_or_default();
        assert!(s.metadata && s.attachments && s.hidden && s.quoting && s.scripts);
        let s: StoredSanitize = serde_json::from_str(r#"{"scripts":false}"#).unwrap();
        assert!(s.metadata && !s.scripts);
    }

    #[test]
    fn overlay_names_round_trip() {
        for o in Overlay::ALL {
            assert_eq!(overlay_of(overlay_name(o)), o);
        }
        assert_eq!(overlay_of("unknown"), Overlay::Standard);
    }
}
