//! Properties of the selected form field (properties pane, form editing mode).

use dioxus::prelude::*;
use folitecta_core::{FieldKind, FieldProps};

use crate::files;
use crate::i18n::t;
use crate::state::AppState;
use crate::util::{color_hex, parse_color};

/// Selected widget: page, annotation index, field id, field kind and properties.
type Selection = (usize, usize, (u32, u16), FieldKind, FieldProps);

fn selection(st: AppState) -> Option<Selection> {
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
        let props = t.doc.field_props(f.id).ok()?;
        Some((p, i, f.id, f.kind, props))
    })
    .flatten()
}

#[component]
pub fn FieldEditor() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let Some((page, index, id, kind, props)) = selection(st) else {
        return rsx! {};
    };
    let apply = move |p: FieldProps| {
        let r = st.with_active(|t| t.doc.set_field_props(id, &p));
        // Editing refreshes the document, which clears the selection: restore it.
        st.with_active(|t| t.selected_annot = Some((page, index)));
        match r {
            Some(Err(e)) => st.error(e),
            _ => files::set_window_title(st),
        }
    };
    let p = props.clone();
    let is_text = kind == FieldKind::Text;
    let is_choice = matches!(kind, FieldKind::ComboBox | FieldKind::ListBox);
    let is_toggle = matches!(kind, FieldKind::CheckBox | FieldKind::RadioButton);
    let options_text = props.options.join("\n");
    rsx! {
        div { class: "style-editor field-editor",
            div { class: "prop-head", "{t(lang, \"field.title\")} · {kind.label()}" }
            div { class: "style-row",
                label { "{t(lang, \"field.name\")}" }
                input { class: "wide", value: "{props.name}", onchange: { let p = p.clone(); move |e: Event<FormData>| { let mut n = p.clone(); n.name = e.value(); apply(n); } } }
            }
            div { class: "style-row",
                label { "{t(lang, \"field.tooltip\")}" }
                input { class: "wide", value: "{props.tooltip}", onchange: { let p = p.clone(); move |e: Event<FormData>| { let mut n = p.clone(); n.tooltip = e.value(); apply(n); } } }
            }
            div { class: "check-row",
                label { class: "check mini", input { r#type: "checkbox", checked: props.required, onchange: { let p = p.clone(); move |_| { let mut n = p.clone(); n.required = !n.required; apply(n); } } } span { "{t(lang, \"label.required\")}" } }
                label { class: "check mini", input { r#type: "checkbox", checked: props.read_only, onchange: { let p = p.clone(); move |_| { let mut n = p.clone(); n.read_only = !n.read_only; apply(n); } } } span { "{t(lang, \"label.read_only\")}" } }
            }
            if is_text {
                div { class: "check-row",
                    label { class: "check mini", input { r#type: "checkbox", checked: props.multiline, onchange: { let p = p.clone(); move |_| { let mut n = p.clone(); n.multiline = !n.multiline; apply(n); } } } span { "{t(lang, \"field.multiline\")}" } }
                    label { class: "check mini", input { r#type: "checkbox", checked: props.password, onchange: { let p = p.clone(); move |_| { let mut n = p.clone(); n.password = !n.password; apply(n); } } } span { "{t(lang, \"field.password\")}" } }
                    label { class: "check mini", input { r#type: "checkbox", checked: props.comb, onchange: { let p = p.clone(); move |_| { let mut n = p.clone(); n.comb = !n.comb; if n.comb && n.max_len == 0 { n.max_len = 10; } apply(n); } } } span { "{t(lang, \"field.comb\")}" } }
                }
                div { class: "style-row",
                    label { "{t(lang, \"field.max_len\")}" }
                    input { r#type: "number", min: "0", max: "10000", value: "{props.max_len}", onchange: { let p = p.clone(); move |e: Event<FormData>| { if let Ok(v) = e.value().parse::<u32>() { let mut n = p.clone(); n.max_len = v; apply(n); } } } }
                }
            }
            if is_choice {
                div { class: "style-row tall",
                    label { "{t(lang, \"field.options\")}" }
                    textarea { class: "wide", rows: "4", value: "{options_text}", onchange: { let p = p.clone(); move |e: Event<FormData>| { let mut n = p.clone(); n.options = e.value().lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect(); apply(n); } } }
                }
                div { class: "row",
                    button { class: "tbtn field-items-btn", onclick: move |_| crate::commands::dispatch(st, "editFields.items"),
                        "{t(lang, \"field.edit_items\")}" }
                }
                div { class: "check-row",
                    if kind == FieldKind::ComboBox {
                        label { class: "check mini", input { r#type: "checkbox", checked: props.editable, onchange: { let p = p.clone(); move |_| { let mut n = p.clone(); n.editable = !n.editable; apply(n); } } } span { "{t(lang, \"field.editable\")}" } }
                    } else {
                        label { class: "check mini", input { r#type: "checkbox", checked: props.multi_select, onchange: { let p = p.clone(); move |_| { let mut n = p.clone(); n.multi_select = !n.multi_select; apply(n); } } } span { "{t(lang, \"field.multi_select\")}" } }
                    }
                }
            }
            if is_text || is_choice {
                div { class: "style-row",
                    label { "{t(lang, \"field.default\")}" }
                    input { class: "wide", value: "{props.default_value}", onchange: { let p = p.clone(); move |e: Event<FormData>| { let mut n = p.clone(); n.default_value = e.value(); apply(n); } } }
                }
            }
            if is_toggle {
                div { class: "style-row",
                    label { "{t(lang, \"field.export_value\")}" }
                    input { class: "wide", value: "{props.export_value}", onchange: { let p = p.clone(); move |e: Event<FormData>| { let mut n = p.clone(); n.export_value = e.value(); apply(n); } } }
                }
            }
            if kind == FieldKind::PushButton {
                div { class: "style-row",
                    label { "{t(lang, \"field.caption\")}" }
                    input { class: "wide", value: "{props.caption}", onchange: { let p = p.clone(); move |e: Event<FormData>| { let mut n = p.clone(); n.caption = e.value(); apply(n); } } }
                }
                ButtonPictures { field: id }
            }
            if is_text {
                crate::ui::features::field_format::FieldFormatRows { current: props.clone(), apply: move |n: FieldProps| apply(n) }
            }
            if matches!(kind, FieldKind::Text | FieldKind::ComboBox) {
                crate::ui::features::form_calc::FieldCalculation { field: id, page, index }
                // Its own key: another field starts with its own scripts.
                {rsx! { crate::ui::features::form_calc::FieldScripts { key: "{id.0}-{id.1}", field: id, page, index } }}
            }
            div { class: "prop-head", "{t(lang, \"field.appearance\")}" }
            div { class: "style-row",
                label { "{t(lang, \"style.font_size\")}" }
                input { r#type: "number", min: "0", max: "72", step: "1", value: "{props.font_size}", title: "{t(lang, \"field.auto_size\")}",
                    onchange: { let p = p.clone(); move |e: Event<FormData>| { if let Ok(v) = e.value().replace(',', ".").parse::<f32>() { let mut n = p.clone(); n.font_size = v.clamp(0.0, 72.0); apply(n); } } } }
            }
            div { class: "style-row",
                label { "{t(lang, \"style.text_color\")}" }
                input { r#type: "color", value: "{color_hex(props.text_color)}", onchange: { let p = p.clone(); move |e: Event<FormData>| { let mut n = p.clone(); n.text_color = parse_color(&e.value()); apply(n); } } }
            }
            div { class: "style-row",
                label { "{t(lang, \"style.border_color\")}" }
                input { r#type: "color", value: "{color_hex(props.border_color.unwrap_or([0.45, 0.45, 0.5]))}", onchange: { let p = p.clone(); move |e: Event<FormData>| { let mut n = p.clone(); n.border_color = Some(parse_color(&e.value())); apply(n); } } }
                label { class: "check mini", input { r#type: "checkbox", checked: props.border_color.is_none(), onchange: { let p = p.clone(); move |_| { let mut n = p.clone(); n.border_color = if n.border_color.is_some() { None } else { Some([0.45, 0.45, 0.5]) }; apply(n); } } } span { "{t(lang, \"style.none\")}" } }
            }
            div { class: "style-row",
                label { "{t(lang, \"style.fill\")}" }
                input { r#type: "color", value: "{color_hex(props.fill_color.unwrap_or([1.0, 1.0, 1.0]))}", onchange: { let p = p.clone(); move |e: Event<FormData>| { let mut n = p.clone(); n.fill_color = Some(parse_color(&e.value())); apply(n); } } }
                label { class: "check mini", input { r#type: "checkbox", checked: props.fill_color.is_none(), onchange: { let p = p.clone(); move |_| { let mut n = p.clone(); n.fill_color = if n.fill_color.is_some() { None } else { Some([1.0, 1.0, 1.0]) }; apply(n); } } } span { "{t(lang, \"style.none\")}" } }
            }
            div { class: "style-row",
                label { "{t(lang, \"style.width\")}" }
                input { r#type: "number", min: "0", max: "10", step: "0.5", value: "{props.border_width}", onchange: { let p = p.clone(); move |e: Event<FormData>| { if let Ok(v) = e.value().replace(',', ".").parse::<f32>() { let mut n = p.clone(); n.border_width = v.clamp(0.0, 10.0); apply(n); } } } }
            }
            if is_text {
                div { class: "style-row",
                    label { "{t(lang, \"style.align\")}" }
                    div { class: "seg-buttons",
                        for (a, key) in [(0u8, "style.left"), (1, "style.center"), (2, "style.right")] {
                            button { class: if props.align == a { "tbtn on" } else { "tbtn" }, key: "{a}",
                                onclick: { let p = p.clone(); move |_| { let mut n = p.clone(); n.align = a; apply(n); } },
                                "{t(lang, key)}"
                            }
                        }
                    }
                }
            }
            div { class: "row",
                button { class: "tbtn danger",
                    onclick: move |_| {
                        match st.with_active(|t| t.doc.delete_form_field(id)) {
                            Some(Err(e)) => st.error(e),
                            _ => files::set_window_title(st),
                        }
                    },
                    "{t(lang, \"field.delete\")}"
                }
            }
        }
    }
}

/// The pictures of a push button for each state, and how it reacts to a click.
#[component]
fn ButtonPictures(field: (u32, u16)) -> Element {
    use crate::commands::buttons;
    use folitecta_core::{ButtonBehaviour, ButtonState};
    let st = use_context::<AppState>();
    let lang = st.lang();
    let (icons, behaviour) = st
        .read_active(|t| (t.doc.button_icons(field), t.doc.button_behaviour(field)))
        .unwrap_or(([false; 3], ButtonBehaviour::Invert));
    let states = [
        (ButtonState::Up, "field.picture_up", icons[0]),
        (ButtonState::Down, "field.picture_down", icons[1]),
        (ButtonState::Rollover, "field.picture_over", icons[2]),
    ];
    rsx! {
        div { class: "prop-head", "{t(lang, \"field.pictures\")}" }
        for (state, key, has) in states {
            div { class: "style-row button-picture", key: "{key}",
                label { "{t(lang, key)}" }
                button { class: "tbtn", onclick: move |_| buttons::choose_icon(st, state), "{t(lang, \"field.choose_picture\")}" }
                if has {
                    button { class: "tbtn danger", onclick: move |_| buttons::remove_icon(st, state), "{t(lang, \"field.remove_picture\")}" }
                }
            }
        }
        div { class: "style-row",
            label { "{t(lang, \"field.behaviour\")}" }
            select { class: "button-behaviour",
                onchange: move |e| {
                    if let Some(b) = ButtonBehaviour::ALL.into_iter().find(|b| b.id() == e.value()) {
                        buttons::set_behaviour(st, b);
                    }
                },
                for b in ButtonBehaviour::ALL {
                    option { key: "{b.id()}", value: "{b.id()}", selected: b == behaviour,
                        {t(lang, &["field.behaviour_", b.id()].concat())}
                    }
                }
            }
        }
    }
}

/// The items of a combo box or list box: a list to add to, delete from,
/// reorder and keep sorted, written to the field with OK.
#[component]
pub fn FieldItemsDialog() -> Element {
    use crate::commands::field_items::{self as items, ITEMS};
    let st = use_context::<AppState>();
    let lang = st.lang();
    let Some(edit) = ITEMS.read().clone() else {
        return rsx! {};
    };
    let label = |id: &str| {
        folitecta_core::registry()
            .get(id)
            .map(|c| crate::i18n::cmd_label(lang, c))
            .unwrap_or_default()
    };
    let has_sel = edit.selected.is_some();
    let can_move = has_sel && !edit.sorted;
    rsx! {
        crate::ui::dialogs::Frame { title: t(lang, "field.items_title"), on_ok: move |_| items::apply(st),
            div { class: "row field-items-new",
                input { class: "grow field-items-draft", value: "{edit.draft}", placeholder: "{t(lang, \"field.new_item\")}",
                    oninput: move |e| { if let Some(x) = ITEMS.write().as_mut() { x.draft = e.value(); } },
                    onkeydown: move |e| {
                        if e.key() == Key::Enter {
                            e.stop_propagation();
                            items::add(st);
                        }
                    },
                }
                button { class: "tbtn field-items-add", onclick: move |_| items::add(st), "{label(\"editFields.items.add\")}" }
            }
            div { class: "field-items-list",
                if edit.items.is_empty() {
                    p { class: "muted", "{t(lang, \"field.no_items\")}" }
                }
                for (i, item) in edit.items.iter().enumerate() {
                    button { key: "{i}-{item}", class: if edit.selected == Some(i) { "field-item on" } else { "field-item" },
                        onclick: move |_| { if let Some(x) = ITEMS.write().as_mut() { x.selected = Some(i); } },
                        "{item}"
                    }
                }
            }
            div { class: "row",
                button { class: "tbtn field-items-up", disabled: !can_move, onclick: move |_| items::move_by(st, -1), "{label(\"editFields.items.moveUp\")}" }
                button { class: "tbtn field-items-down", disabled: !can_move, onclick: move |_| items::move_by(st, 1), "{label(\"editFields.items.moveDown\")}" }
                button { class: "tbtn danger field-items-delete", disabled: !has_sel, onclick: move |_| items::delete(st), "{label(\"editFields.items.delete\")}" }
                label { class: "check",
                    input { r#type: "checkbox", class: "field-items-sorted", checked: edit.sorted, onchange: move |_| items::toggle_sorted(st) }
                    span { "{label(\"editFields.items.sorted\")}" }
                }
            }
        }
    }
}
