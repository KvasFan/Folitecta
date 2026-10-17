//! Fields pane commands: selection by type, name groups, sorting and the
//! manual tab order.

use std::collections::HashSet;

use dioxus::prelude::ReadableExt;
use folitecta_core::{FieldKind, FormField};

use crate::commands::{core_result, handlers, no_doc};
use crate::i18n::{t, tf};
use crate::state::{AppState, Dialog, DocTab, Pane, ToastKind};

/// "alphabetic", "pages" or "" (document order).
pub fn sort_mode(st: AppState) -> String {
    st.ui.peek().value("fields.sort").unwrap_or("").to_string()
}

/// Fields listed by page in tab order, to be moved by hand.
pub fn manual_order(st: AppState) -> bool {
    st.ui.peek().value("fields.manual") == Some("1")
}

/// Name groups (prefixes of dotted names) that hold several fields.
pub fn group_keys(fields: &[FormField]) -> HashSet<String> {
    let mut out = HashSet::new();
    for f in fields {
        let parts: Vec<&str> = f.name.split('.').collect();
        for n in 1..parts.len() {
            out.insert(parts[..n].join("."));
        }
    }
    out
}

/// Select a field in the pane and its first widget on the page.
pub fn select_field(t: &mut DocTab, field: &FormField, add: bool) {
    if add {
        if let Some(i) = t.selected_fields.iter().position(|x| *x == field.id) {
            t.selected_fields.remove(i);
            return;
        }
        t.selected_fields.push(field.id);
    } else {
        t.selected_fields = vec![field.id];
    }
    select_widget(t, field);
}

/// Select the first widget of a field on its page.
fn select_widget(t: &mut DocTab, field: &FormField) {
    if let Some(p) = field.page {
        let index = t.doc.annotations(p).ok().and_then(|a| {
            a.iter()
                .position(|x| x.id.is_some_and(|id| field.widgets.contains(&id)))
        });
        if let Some(i) = index {
            t.selected_annot = Some((p, i));
        }
    }
}

fn select_kind(st: AppState, pick: fn(&FormField) -> bool) {
    if no_doc(st) {
        return;
    }
    let first = st
        .with_active(|t| {
            let fields: Vec<FormField> = t.doc.form_fields().into_iter().filter(pick).collect();
            t.selected_fields = fields.iter().map(|f| f.id).collect();
            if let Some(f) = fields.first() {
                select_widget(t, f);
            }
            (fields.len(), fields.first().and_then(|f| f.page))
        })
        .unwrap_or((0, None));
    if let (_, Some(p)) = first {
        st.navigate(p, true);
    }
    crate::commands::show_pane(st, Pane::Fields);
    st.toast(
        ToastKind::Info,
        tf(
            st.lang(),
            "toast.fields_selected",
            &[("count", &first.0.to_string())],
        ),
        "",
    );
}

fn set_sort(st: AppState, mode: &str) {
    let next = if sort_mode(st) == mode { "" } else { mode };
    st.set_value("fields.sort", next);
}

fn collapse_all(st: AppState, collapse: bool) {
    if no_doc(st) {
        return;
    }
    st.with_active(|t| {
        t.collapsed_fields = if collapse {
            group_keys(&t.doc.form_fields())
        } else {
            HashSet::new()
        };
    });
}

fn manual(st: AppState) {
    if no_doc(st) {
        return;
    }
    let r = st.with_active(|t| t.doc.set_tab_order(""));
    core_result(st, r);
    st.set_value("fields.manual", "1");
    crate::commands::show_pane(st, Pane::Fields);
    st.toast(ToastKind::Info, t(st.lang(), "toast.tab_order_manual"), "");
}

/// Move a field's widget in the tab order of its page.
pub fn move_in_tab_order(st: AppState, widget: (u32, u16), delta: i32) {
    let r = st.with_active(|t| t.doc.move_widget_in_tab_order(widget, delta).map(|_| ()));
    core_result(st, r);
}

/// Check states of the fields pane options.
pub fn fields_state(st: AppState, id: &str) -> Option<bool> {
    match id {
        "fieldsView.sort.alphabetic" => Some(sort_mode(st) == "alphabetic"),
        "fieldsView.sort.pages" => Some(sort_mode(st) == "pages"),
        _ => None,
    }
}

handlers! {
    "fieldsView.collapseAll" => |st| collapse_all(st, true),
    "fieldsView.expandAll" => |st| collapse_all(st, false),
    "fieldsView.options" => |st| st.set_dialog(Dialog::FieldsOptions),
    "fieldsView.select.barcode" => |st| select_kind(st, |f| f.barcode),
    "fieldsView.select.button" => |st| select_kind(st, |f| f.kind == FieldKind::PushButton),
    "fieldsView.select.check" => |st| select_kind(st, |f| f.kind == FieldKind::CheckBox),
    "fieldsView.select.combo" => |st| select_kind(st, |f| f.kind == FieldKind::ComboBox),
    "fieldsView.select.list" => |st| select_kind(st, |f| f.kind == FieldKind::ListBox),
    "fieldsView.select.radio" => |st| select_kind(st, |f| f.kind == FieldKind::RadioButton),
    "fieldsView.select.signature" => |st| select_kind(st, |f| f.kind == FieldKind::Signature),
    "fieldsView.select.text" => |st| select_kind(st, |f| f.kind == FieldKind::Text && !f.barcode),
    "fieldsView.sort.alphabetic" => |st| set_sort(st, "alphabetic"),
    "fieldsView.sort.pages" => |st| set_sort(st, "pages"),
    "fieldsView.tabOrder.manually" => manual,
}
