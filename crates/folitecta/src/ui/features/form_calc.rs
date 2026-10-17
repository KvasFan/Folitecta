//! Field calculations: the calculation of the selected field (in the field
//! properties) and the order in which the calculated fields are calculated.

use dioxus::prelude::*;
use folitecta_core::{CalcOp, Calculation, FieldKind, registry};

use crate::files;
use crate::i18n::{cmd_label, t};
use crate::icons::Icon;
use crate::state::{AppState, Dialog};
use crate::ui::dialogs::Frame;

/// The calculation of a text field or combo box: what is done with which
/// fields. Every change is applied (and calculated) at once.
#[component]
pub fn FieldCalculation(field: (u32, u16), page: usize, index: usize) -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let (calc, others) = st
        .read_active(|t| {
            let me = t
                .doc
                .form_fields()
                .into_iter()
                .find(|f| f.id == field)
                .map(|f| f.name);
            let others: Vec<String> = t
                .doc
                .form_fields()
                .into_iter()
                .filter(|f| Some(&f.name) != me.as_ref())
                .filter(|f| !matches!(f.kind, FieldKind::PushButton | FieldKind::Signature))
                .map(|f| f.name)
                .collect();
            (t.doc.field_calculation(field), others)
        })
        .unwrap_or_default();
    let set = move |c: Option<Calculation>| {
        let r = st.with_active(|t| t.doc.set_field_calculation(field, c.as_ref()));
        // Editing refreshes the document, which clears the selection: restore it.
        st.with_active(|t| t.selected_annot = Some((page, index)));
        match r {
            Some(Err(e)) => st.error(e),
            _ => files::set_window_title(st),
        }
    };
    let chosen = calc.clone();
    let none = calc.is_none();
    let current = calc.as_ref().map(|c| c.op);
    rsx! {
        div { class: "prop-head", "{t(lang, \"field.calculation\")}" }
        div { class: "style-row",
            label { "{t(lang, \"field.calc.value\")}" }
            select { class: "wide",
                onchange: move |e| {
                    let fields = chosen.as_ref().map(|c| c.fields.clone()).unwrap_or_default();
                    set(CalcOp::from_id(&e.value()).map(|op| Calculation { op, fields }));
                },
                option { value: "", selected: none, "{t(lang, \"field.calc.none\")}" }
                for op in CalcOp::ALL {
                    {
                        let name = t(lang, &format!("field.calc.{}", op.id().to_lowercase()));
                        let selected = current == Some(op);
                        rsx! { option { key: "{op.id()}", value: "{op.id()}", selected, "{name}" } }
                    }
                }
            }
        }
        if let Some(c) = calc {
            if others.is_empty() {
                p { class: "hint", "{t(lang, \"field.calc.no_fields\")}" }
            }
            div { class: "calc-fields",
                for name in others {
                    {
                        let on = c.fields.contains(&name);
                        let mut next = c.clone();
                        if on {
                            next.fields.retain(|f| f != &name);
                        } else {
                            next.fields.push(name.clone());
                        }
                        rsx! {
                            label { class: "check mini", key: "{name}",
                                input { r#type: "checkbox", checked: on, onchange: move |_| set(Some(next.clone())) }
                                span { "{name}" }
                            }
                        }
                    }
                }
            }
            button { class: "tbtn", onclick: move |_| st.set_dialog(Dialog::CalcOrder),
                "{cmd_label(lang, registry().get(\"forms.calcOrder\").unwrap())}" }
        }
    }
}

/// The calculated fields, moved up and down to set the order in which they
/// are calculated: a field sees the values calculated before it.
#[component]
pub fn CalcOrderDialog() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let mut order = use_signal(|| {
        st.read_active(|t| {
            t.doc
                .calculated_fields()
                .into_iter()
                .map(|f| (f.id, f.name))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
    });
    let title = registry()
        .get("forms.calcOrder")
        .map(|c| cmd_label(lang, c))
        .unwrap_or_default();
    let n = order.read().len();
    rsx! {
        Frame { title,
            on_ok: move |_| {
                let ids: Vec<(u32, u16)> = order.read().iter().map(|(id, _)| *id).collect();
                match st.with_active(|t| t.doc.set_calculation_order(&ids)) {
                    Some(Err(e)) => st.error(e),
                    _ => {
                        files::set_window_title(st);
                        st.set_dialog(Dialog::None);
                    }
                }
            },
            if n == 0 {
                p { class: "hint", "{t(lang, \"field.calc_order.none\")}" }
            } else {
                p { class: "hint", "{t(lang, \"field.calc_order.hint\")}" }
                div { class: "calc-order",
                    for (i, (_, name)) in order().into_iter().enumerate() {
                        div { class: "calc-order-row", key: "{name}",
                            span { class: "calc-order-num", "{i + 1}" }
                            span { class: "calc-order-name", "{name}" }
                            button { class: "ibtn", title: "{t(lang, \"field.calc_order.up\")}", disabled: i == 0,
                                onclick: move |_| order.write().swap(i, i - 1), Icon { name: "up" } }
                            button { class: "ibtn", title: "{t(lang, \"field.calc_order.down\")}", disabled: i + 1 == n,
                                onclick: move |_| order.write().swap(i, i + 1), Icon { name: "down" } }
                        }
                    }
                }
            }
        }
    }
}
