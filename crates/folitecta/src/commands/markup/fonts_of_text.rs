//! Format › Font: the font of the text of text boxes and typewriter text
//! (Helvetica, Times or Courier): the comment chosen gets it, or the text
//! boxes and typewriter text made next.

use crate::commands::handlers;
use crate::i18n::t;
use crate::state::{AppState, Dialog, ToastKind};

/// The fonts offered: value kept with the style and its name.
pub const FAMILIES: &[(&str, &str)] = &[("", "Helvetica"), ("Times", "Times"), ("Courier", "Courier")];

/// The text comment chosen (page, index), when it is a text box.
fn chosen_text(st: AppState) -> Option<(usize, usize)> {
    st.read_active(|t| {
        let (p, i) = t.selected_annot?;
        let a = t.doc.annotations(p).ok()?.into_iter().find(|a| a.index == i)?;
        (a.subtype == "FreeText").then_some((p, i))
    })
    .flatten()
}

/// The font in use: the chosen text box's, else that of new text boxes.
pub fn current(st: AppState) -> String {
    match chosen_text(st) {
        Some((p, i)) => st
            .read_active(|t| t.doc.annotation_style(p, i).ok().map(|s| s.font))
            .flatten()
            .unwrap_or_default(),
        None => crate::state::comment_styles::default_style(st, "FreeText").font,
    }
}

/// Give the chosen text box `font`, or the text boxes and typewriter text
/// made next.
pub fn apply(st: AppState, font: &str) {
    match chosen_text(st) {
        Some((p, i)) => {
            let style = st.read_active(|t| t.doc.annotation_style(p, i).ok()).flatten();
            let Some(mut style) = style else { return };
            style.font = font.to_string();
            let r = st.with_active(|t| t.doc.set_annotation_style(p, i, &style));
            st.with_active(|t| t.selected_annot = Some((p, i)));
            match r {
                Some(Ok(())) => crate::files::set_window_title(st),
                Some(Err(e)) => st.error(st.core_error_text(&e)),
                None => {}
            }
        }
        None => {
            for kind in ["FreeText", "FreeTextTypewriter"] {
                let mut s = crate::state::comment_styles::default_style(st, kind);
                s.font = font.to_string();
                crate::state::comment_styles::set_default_style(st, kind, &s);
            }
            st.toast(ToastKind::Ok, t(st.lang(), "style.default_saved"), "");
        }
    }
}

handlers! {
    "fontName" => |st| st.set_dialog(Dialog::FontFamily),
}
