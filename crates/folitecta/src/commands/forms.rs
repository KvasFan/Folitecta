//! Form menu: field tools, editing mode, data exchange, highlighting, tab order and reset.

use dioxus::prelude::ReadableExt;

use super::{confirm, core_result, handlers, no_doc, select_tool};
use crate::files;
use crate::i18n::{t, tf};
use crate::state::{AppState, ConfirmAction, ToastKind, Tool};

fn toggle_highlight(st: AppState) {
    st.with_active(|t| t.highlight_fields = !t.highlight_fields);
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
    "tool.editFields.mode.date" => |st| select_tool(st, Tool::FieldText),
    "tool.editFields.mode.check" => |st| select_tool(st, Tool::FieldCheck),
    "tool.editFields.mode.radio" => |st| select_tool(st, Tool::FieldRadio),
    "tool.editFields.mode.combo" => |st| select_tool(st, Tool::FieldCombo),
    "tool.editFields.mode.list" => |st| select_tool(st, Tool::FieldList),
    "tool.editFields.mode.button" => |st| select_tool(st, Tool::FieldButton),
    "tool.editFields.mode.signature" => |st| select_tool(st, Tool::FieldSignature),
    "tool.editFields.keepMode" => |st| { let on = !super::option_on(st, "tool.keepSelected"); st.set_option("tool.keepSelected", on); },
    "tool.editFields.renameField" => |st| with_selected_field(st, |st, _| show_properties(st)),
    "tool.editFields.setAsRequired" => |st| with_selected_field(st, toggle_required),
    "forms.reset" => |st| { if !no_doc(st) { confirm(st, "cmd.forms.reset", "dialog.confirm.reset_form", ConfirmAction::ResetForm) } },
    "forms.reset.selected" => |st| with_selected_field(st, reset_field),
    "forms.exportData" => |st| { if !no_doc(st) { files::export_form_data(st) } },
    "forms.importData" => |st| { if !no_doc(st) { files::import_form_data(st) } },
    "forms.highlight" => toggle_highlight,
    "forms.highlight.all" => toggle_highlight,
    "fieldsView.tabOrder.row" => |st| tab_order(st, "R"),
    "fieldsView.tabOrder.column" => |st| tab_order(st, "C"),
    "fieldsView.tabOrder.structure" => |st| tab_order(st, "S"),
    "fieldsView.tabOrder.unspecified" => |st| tab_order(st, ""),
}
