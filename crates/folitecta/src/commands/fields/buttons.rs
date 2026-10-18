//! Push buttons: a picture chosen for each state (up, pressed, pointed
//! at) of the chosen button, and how it reacts to a click.

use dioxus::prelude::*;
use folitecta_core::{ButtonBehaviour, ButtonState, FieldKind};

use crate::commands::handlers;
use crate::files;
use crate::i18n::t;
use crate::state::{AppState, ToastKind};

/// The push button of the chosen widget: its page and index, and the field.
pub fn chosen_button(st: AppState) -> Option<(usize, usize, (u32, u16))> {
    st.read_active(|t| {
        let (p, i) = t.selected_annot?;
        let a = t
            .doc
            .annotations(p)
            .ok()?
            .into_iter()
            .find(|a| a.index == i && a.subtype == "Widget")?;
        let wid = a.id?;
        let f = t
            .doc
            .form_fields()
            .into_iter()
            .find(|f| t.doc.field_widgets(f.id).contains(&wid))?;
        (f.kind == FieldKind::PushButton).then_some((p, i, f.id))
    })
    .flatten()
}

fn done(st: AppState, page: usize, index: usize, r: Option<folitecta_core::Result<()>>) {
    // The edit clears the selection: the widget is chosen again.
    st.with_active(|t| t.selected_annot = Some((page, index)));
    match r {
        Some(Ok(())) => files::set_window_title(st),
        Some(Err(e)) => st.error(e),
        None => {}
    }
}

/// Choose a picture file for a state of the chosen push button.
pub fn choose_icon(st: AppState, state: ButtonState) {
    let Some((page, index, field)) = chosen_button(st) else {
        st.toast(ToastKind::Info, t(st.lang(), "toast.choose_button"), "");
        return;
    };
    spawn(async move {
        let Some(file) = files::image_dialog().pick_file().await else {
            return;
        };
        let data = file.read().await;
        let r = st.with_active(|t| t.doc.set_button_icon(field, state, Some(data)));
        done(st, page, index, r);
    });
}

/// Take away the picture of a state of the chosen push button.
pub fn remove_icon(st: AppState, state: ButtonState) {
    if let Some((page, index, field)) = chosen_button(st) {
        let r = st.with_active(|t| t.doc.set_button_icon(field, state, None));
        done(st, page, index, r);
    }
}

pub fn set_behaviour(st: AppState, b: ButtonBehaviour) {
    if let Some((page, index, field)) = chosen_button(st) {
        let r = st.with_active(|t| t.doc.set_button_behaviour(field, b));
        done(st, page, index, r);
    }
}

/// Show the properties of the chosen push button, where its behaviour is.
fn behaviour(st: AppState) {
    if chosen_button(st).is_some() {
        st.ui_mut().show_right_pane = true;
    } else {
        st.toast(ToastKind::Info, t(st.lang(), "toast.choose_button"), "");
    }
}

handlers! {
    "btn.upIcon" => |st| choose_icon(st, ButtonState::Up),
    "btn.downIcon" => |st| choose_icon(st, ButtonState::Down),
    "btn.rollOverIcon" => |st| choose_icon(st, ButtonState::Rollover),
    "btn.behaviour" => behaviour,
}
