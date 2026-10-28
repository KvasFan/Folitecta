//! Filling form fields directly on the page.

use dioxus::prelude::*;
use folitecta_core::{FieldKind, PageWidget};

use crate::files;
use crate::i18n::t;
use crate::state::{AppState, ToastKind};

/// Set the value of a field; `false` when it was refused (said in a toast).
fn set_value(st: AppState, field: (u32, u16), value: &str) -> bool {
    match st.with_active(|t| t.doc.set_field_value(field, value)) {
        Some(Err(e)) => {
            st.error(e);
            false
        }
        _ => {
            files::set_window_title(st);
            true
        }
    }
}

/// Act on a click on a field: toggle check boxes, pick radio buttons, or open
/// an editor over text, combo and list fields.
pub fn fill_field(st: AppState, page: usize, w: &PageWidget) {
    if w.read_only {
        st.toast(
            ToastKind::Info,
            t(st.lang(), "toast.field_read_only"),
            w.name.clone(),
        );
        return;
    }
    match w.kind {
        FieldKind::Text | FieldKind::ComboBox | FieldKind::ListBox => {
            st.set_editing_field(Some((page, w.index)))
        }
        FieldKind::CheckBox => {
            let next = if w.value == w.on_state {
                "Off".to_string()
            } else {
                w.on_state.clone()
            };
            set_value(st, w.field, &next);
        }
        FieldKind::RadioButton => {
            set_value(st, w.field, &w.on_state);
        }
        FieldKind::PushButton => st.toast(ToastKind::Info, t(st.lang(), "toast.button_actions"), ""),
        FieldKind::Signature => st.toast(ToastKind::Info, t(st.lang(), "toast.signature_field"), ""),
        FieldKind::Unknown(_) => {}
    }
}

/// Editor shown over a text, combo or list field while it is being filled.
#[component]
pub fn FieldInput(
    page: usize,
    widget: PageWidget,
    left: f32,
    top: f32,
    width: f32,
    height: f32,
    zoom: f32,
) -> Element {
    let st = use_context::<AppState>();
    let mut draft = use_signal(|| widget.value.clone());
    let mut done = use_signal(|| false);
    let field = widget.field;
    let original = use_signal(|| widget.value.clone());
    // A date field: its format, for the calendar.
    let date_format = use_hook(|| {
        st.read_active(|t| match t.doc.field_format(field) {
            folitecta_core::FieldFormat::Date(f) => Some(f),
            _ => None,
        })
        .flatten()
    });
    let mut calendar = use_signal(|| false);
    // A value the field refuses keeps the editor open with what was typed.
    let mut commit = move |value: String| {
        if *done.peek() {
            return;
        }
        if value != *original.peek() && !set_value(st, field, &value) {
            draft.set(value);
            return;
        }
        done.set(true);
        st.set_editing_field(None);
    };
    let mut cancel = move || {
        done.set(true);
        st.set_editing_field(None);
    };
    let font = (if widget.multiline {
        10.0 * zoom
    } else {
        (height * 0.62).min(15.0 * zoom)
    })
    .clamp(9.0, 22.0);
    let style = format!(
        "left:{left}px;top:{top}px;width:{}px;height:{}px;font-size:{font}px",
        width.max(24.0),
        height.max(14.0)
    );
    let max_len = if widget.max_len > 0 {
        widget.max_len.to_string()
    } else {
        String::new()
    };
    match widget.kind {
        FieldKind::Text if widget.multiline => rsx! {
            textarea { class: "field-input", style: "{style}", autofocus: true, value: "{draft}",
                oninput: move |e| draft.set(e.value()),
                onkeydown: move |e| {
                    e.stop_propagation();
                    if e.key() == Key::Escape { cancel(); }
                    if e.key() == Key::Enter && e.modifiers().ctrl() { commit(draft()); }
                },
                onblur: move |_| commit(draft()),
            }
        },
        FieldKind::Text => rsx! {
            input { class: "field-input", style: "{style}", autofocus: true, value: "{draft}", maxlength: "{max_len}",
                oninput: move |e| draft.set(e.value()),
                onkeydown: move |e| {
                    e.stop_propagation();
                    if e.key() == Key::Escape { cancel(); }
                    if e.key() == Key::Enter || e.key() == Key::Tab { commit(draft()); }
                },
                onblur: move |_| commit(draft()),
            }
            if let Some(fmt) = date_format.clone() {
                // Pressing it keeps the focus in the field.
                button { class: "ibtn field-calendar-btn", title: "{t(st.lang(), \"calendar.open\")}",
                    style: "left:{left + width.max(24.0) + 2.0}px;top:{top}px;height:{height.max(14.0)}px",
                    onmousedown: move |e| { e.prevent_default(); e.stop_propagation(); },
                    onclick: move |_| calendar.set(!calendar()),
                    crate::icons::Icon { name: "calendar" }
                }
                if calendar() {
                    crate::ui::features::calendar::DateCalendar {
                        format: fmt, value: draft(), left, top: top + height.max(14.0) + 2.0,
                        on_pick: move |v: String| commit(v),
                    }
                }
            }
        },
        FieldKind::ComboBox => {
            let options = widget.options.clone();
            let current = widget.value.clone();
            rsx! {
                select { class: "field-input", style: "{style}", autofocus: true,
                    onchange: move |e| commit(e.value()),
                    onkeydown: move |e| { e.stop_propagation(); if e.key() == Key::Escape { cancel(); } },
                    onblur: move |_| cancel(),
                    if !options.contains(&current) {
                        option { value: "{current}", selected: true, "{current}" }
                    }
                    for o in options.iter() {
                        option { key: "{o}", value: "{o}", selected: *o == current, "{o}" }
                    }
                }
            }
        }
        FieldKind::ListBox => {
            let options = widget.options.clone();
            let selected: Vec<String> = widget
                .value
                .split(", ")
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect();
            rsx! {
                div { class: "field-input field-list", style: "{style};height:auto;max-height:{(height * 3.0).max(80.0)}px", tabindex: "0",
                    onmousedown: move |e| e.stop_propagation(),
                    onkeydown: move |e| { e.stop_propagation(); if e.key() == Key::Escape { cancel(); } },
                    for o in options.iter().cloned() {
                        {
                            let on = selected.contains(&o);
                            let sel = selected.clone();
                            rsx! {
                                div { key: "{o}", class: if on { "field-option on" } else { "field-option" },
                                    onclick: move |e| {
                                        e.stop_propagation();
                                        // Ctrl adds or removes options; a plain click picks one.
                                        let value = if e.modifiers().ctrl() {
                                            let mut s = sel.clone();
                                            if let Some(i) = s.iter().position(|x| *x == o) { s.remove(i); } else { s.push(o.clone()); }
                                            s.join("\n")
                                        } else {
                                            o.clone()
                                        };
                                        commit(value);
                                    },
                                    "{o}"
                                }
                            }
                        }
                    }
                }
            }
        }
        _ => rsx! {},
    }
}
