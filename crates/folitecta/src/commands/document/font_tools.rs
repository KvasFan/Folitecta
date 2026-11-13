//! Substitute Fonts: a font is replaced (Change Font, the Replace Fonts
//! window) and brought back as it was (Reset Font, Reset All Fonts); Font ›
//! Repair To Unicode CMap gives the fonts that lack one a map of their
//! characters.

use crate::commands::{doc_dialog, handlers, no_doc};
use crate::i18n::{t, tf};
use crate::state::{AppState, Dialog, ToastKind};

/// Bring back the fonts replaced (one by name, or all of them).
pub fn reset(st: AppState, name: Option<String>) {
    if no_doc(st) || !st.allowed(folitecta_core::Restricted::Modify) {
        return;
    }
    let none = st
        .read_active(|t| t.doc.substituted_fonts().is_empty())
        .unwrap_or(true);
    if none {
        st.toast(ToastKind::Info, t(st.lang(), "toast.no_replaced_fonts"), "");
        return;
    }
    match st.with_active(|t| t.doc.reset_substituted_fonts(name.as_deref())) {
        Some(Ok(n)) => {
            crate::files::set_window_title(st);
            st.toast(
                ToastKind::Ok,
                tf(st.lang(), "toast.fonts_back", &[("count", &n.to_string())]),
                "",
            );
        }
        Some(Err(e)) => st.error(st.core_error_text(&e)),
        None => {}
    }
}

fn reset_one(st: AppState) {
    if no_doc(st) {
        return;
    }
    let none = st
        .read_active(|t| t.doc.substituted_fonts().is_empty())
        .unwrap_or(true);
    if none {
        st.toast(ToastKind::Info, t(st.lang(), "toast.no_replaced_fonts"), "");
    } else {
        st.set_dialog(Dialog::ResetFonts);
    }
}

fn repair(st: AppState) {
    if no_doc(st) || !st.allowed(folitecta_core::Restricted::Modify) {
        return;
    }
    match st.with_active(|t| t.doc.repair_to_unicode()) {
        Some(Ok(0)) => st.toast(ToastKind::Info, t(st.lang(), "toast.unicode_maps_fine"), ""),
        Some(Ok(n)) => {
            crate::files::set_window_title(st);
            st.toast(
                ToastKind::Ok,
                tf(st.lang(), "toast.unicode_maps_made", &[("count", &n.to_string())]),
                "",
            );
        }
        Some(Err(e)) => st.error(st.core_error_text(&e)),
        None => {}
    }
}

handlers! {
    "saf.changeFont" => |st| doc_dialog(st, Dialog::ReplaceFonts),
    "saf.resetFont" => reset_one,
    "saf.resetAllFonts" => |st| reset(st, None),
    "font.repairToUnicodeCMap" => repair,
}
