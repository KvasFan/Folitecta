//! Form menu: field tools, editing mode, data exchange, highlighting, tab order and reset.

use dioxus::prelude::ReadableExt;
use folitecta_core::{FieldKind, FormField};

use crate::commands::{confirm, core_result, handlers, no_doc, select_tool};
use crate::files;
use crate::i18n::{t, tf};
use crate::state::{AppState, ConfirmAction, Dialog, DocTab, ToastKind, Tool};

fn toggle_highlight(st: AppState) {
    st.with_active(|t| t.highlight_fields = !t.highlight_fields);
}

/// Kinds of field highlighted on their own (Form › Highlight Fields),
/// by the last word of their command ids.
pub const FIELD_TYPES: &[&str] = &[
    "textBox",
    "checkBox",
    "radioButton",
    "comboBox",
    "listBox",
    "pushButton",
    "signature",
    "barcode",
];

fn type_of(f: &FormField) -> &'static str {
    match f.kind {
        _ if f.barcode => "barcode",
        FieldKind::Text => "textBox",
        FieldKind::CheckBox => "checkBox",
        FieldKind::RadioButton => "radioButton",
        FieldKind::ComboBox => "comboBox",
        FieldKind::ListBox => "listBox",
        FieldKind::PushButton => "pushButton",
        FieldKind::Signature => "signature",
        FieldKind::Unknown(_) => "",
    }
}

/// Whether the field is among those highlighted.
pub fn highlighted(t: &DocTab, f: &FormField) -> bool {
    (t.highlight_kinds.is_empty() || t.highlight_kinds.contains(&type_of(f)))
        && (!t.highlight_required || f.required)
}

/// Every field highlighted (again), or none when that is already so.
fn highlight_all(st: AppState) {
    st.with_active(|t| {
        let all = t.highlight_fields && t.highlight_kinds.is_empty() && !t.highlight_required;
        t.highlight_fields = !all;
        t.highlight_kinds.clear();
        t.highlight_required = false;
    });
}

/// One more or one fewer kind of field highlighted.
fn highlight_kind(st: AppState, kind: &'static str) {
    st.with_active(|t| {
        if !t.highlight_fields {
            t.highlight_kinds.clear();
        }
        match t.highlight_kinds.iter().position(|k| *k == kind) {
            Some(i) => {
                t.highlight_kinds.remove(i);
            }
            None => t.highlight_kinds.push(kind),
        }
        t.highlight_fields = !t.highlight_kinds.is_empty() || t.highlight_required;
    });
}

fn highlight_required(st: AppState) {
    st.with_active(|t| {
        t.highlight_required = !(t.highlight_fields && t.highlight_required);
        t.highlight_fields = t.highlight_required || !t.highlight_kinds.is_empty();
    });
}

/// Check states of the highlighting commands.
pub fn forms_state(st: AppState, id: &str) -> Option<bool> {
    let rest = id.strip_prefix("forms.highlight.")?;
    st.read_active(|t| {
        let on = t.highlight_fields;
        match rest {
            "all" => on && t.highlight_kinds.is_empty() && !t.highlight_required,
            "onlyrequired" => on && t.highlight_required,
            kind => on && FIELD_TYPES.contains(&kind) && t.highlight_kinds.contains(&kind),
        }
    })
}

/// The field of the selected widget, if a widget is selected.
fn selected_field(st: AppState) -> Option<(u32, u16)> {
    st.read_active(|t| {
        let (p, i) = t.selected_annot?;
        let wid = t
            .doc
            .annotations(p)
            .ok()?
            .into_iter()
            .find(|a| a.index == i && a.subtype == "Widget")?
            .id?;
        t.doc
            .form_fields()
            .into_iter()
            .find(|f| t.doc.field_widgets(f.id).contains(&wid))
            .map(|f| f.id)
    })
    .flatten()
}

fn with_selected_field(st: AppState, f: impl FnOnce(AppState, (u32, u16))) {
    if no_doc(st) {
        return;
    }
    match selected_field(st) {
        Some(id) => f(st, id),
        None => st.toast(ToastKind::Info, t(st.lang(), "toast.select_field_first"), ""),
    }
}

fn toggle_required(st: AppState, id: (u32, u16)) {
    let r = st.with_active(|t| {
        let mut props = t.doc.field_props(id)?;
        props.required = !props.required;
        t.doc.set_field_props(id, &props)
    });
    core_result(st, r);
}

fn reset_field(st: AppState, id: (u32, u16)) {
    let r = st.with_active(|t| {
        let dv = t.doc.field_props(id).map(|p| p.default_value).unwrap_or_default();
        t.doc.set_field_value(id, &dv)
    });
    core_result(st, r);
}

fn tab_order(st: AppState, order: &str) {
    if no_doc(st) {
        return;
    }
    let order = order.to_string();
    st.set_value("fields.manual", "");
    let r = st.with_active(|t| t.doc.set_tab_order(&order));
    core_result(st, r);
    st.toast(
        ToastKind::Ok,
        tf(
            st.lang(),
            "toast.tab_order",
            &[(
                "order",
                &t(
                    st.lang(),
                    &format!("tab_order.{}", if order.is_empty() { "none" } else { &order }),
                ),
            )],
        ),
        "",
    );
}

fn show_properties(st: AppState) {
    if !st.ui.peek().show_right_pane {
        st.ui_mut().show_right_pane = true;
        st.save_prefs();
    }
}

handlers! {
    "tool.editFields" => |st| select_tool(st, Tool::EditFields),
    "tool.editFields.mode.select" => |st| select_tool(st, Tool::EditFields),
    "tool.editFields.addNewField" => |st| select_tool(st, Tool::FieldText),
    "tool.editFields.mode.text" => |st| select_tool(st, Tool::FieldText),
    "tool.editFields.mode.date" => |st| select_tool(st, Tool::FieldDate),
    "tool.editFields.mode.check" => |st| select_tool(st, Tool::FieldCheck),
    "tool.editFields.mode.radio" => |st| select_tool(st, Tool::FieldRadio),
    "tool.editFields.mode.combo" => |st| select_tool(st, Tool::FieldCombo),
    "tool.editFields.mode.list" => |st| select_tool(st, Tool::FieldList),
    "tool.editFields.mode.button" => |st| select_tool(st, Tool::FieldButton),
    "tool.editFields.mode.signature" => |st| select_tool(st, Tool::FieldSignature),
    "tool.editFields.keepMode" => |st| { let on = !crate::commands::option_on(st, "tool.keepSelected"); st.set_option("tool.keepSelected", on); },
    "tool.editFields.renameField" => |st| with_selected_field(st, |st, _| show_properties(st)),
    "tool.editFields.setAsRequired" => |st| with_selected_field(st, toggle_required),
    "forms.reset" => |st| { if !no_doc(st) { confirm(st, "cmd.forms.reset", "dialog.confirm.reset_form", ConfirmAction::ResetForm) } },
    "forms.reset.selected" => |st| with_selected_field(st, reset_field),
    "forms.exportData" => |st| { if !no_doc(st) { files::export_form_data(st) } },
    "forms.importData" => |st| { if !no_doc(st) { files::import_form_data(st) } },
    "forms.populate" => |st| { if !no_doc(st) { files::populate_forms(st) } },
    "forms.highlight" => toggle_highlight,
    "forms.highlight.all" => highlight_all,
    "forms.highlight.textBox" => |st| highlight_kind(st, "textBox"),
    "forms.highlight.checkBox" => |st| highlight_kind(st, "checkBox"),
    "forms.highlight.radioButton" => |st| highlight_kind(st, "radioButton"),
    "forms.highlight.comboBox" => |st| highlight_kind(st, "comboBox"),
    "forms.highlight.listBox" => |st| highlight_kind(st, "listBox"),
    "forms.highlight.pushButton" => |st| highlight_kind(st, "pushButton"),
    "forms.highlight.signature" => |st| highlight_kind(st, "signature"),
    "forms.highlight.barcode" => |st| highlight_kind(st, "barcode"),
    "forms.highlight.onlyrequired" => highlight_required,
    "forms.calcOrder" => |st| { if !no_doc(st) { st.set_dialog(Dialog::CalcOrder) } },
    "fieldsView.tabOrder.row" => |st| tab_order(st, "R"),
    "fieldsView.tabOrder.column" => |st| tab_order(st, "C"),
    "fieldsView.tabOrder.structure" => |st| tab_order(st, "S"),
    "fieldsView.tabOrder.unspecified" => |st| tab_order(st, ""),
}
