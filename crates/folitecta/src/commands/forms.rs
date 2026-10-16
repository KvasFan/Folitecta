//! Form menu: highlighting, resetting and exporting field data.

use super::{confirm, handlers, no_doc};
use crate::files;
use crate::state::{AppState, ConfirmAction};

fn toggle_highlight(st: AppState) {
    st.with_active(|t| t.highlight_fields = !t.highlight_fields);
}

handlers! {
    "forms.reset" => |st| { if !no_doc(st) { confirm(st, "cmd.forms.reset", "dialog.confirm.reset_form", ConfirmAction::ResetForm) } },
    "forms.exportData" => |st| {
        if no_doc(st) { return; }
        let text = st.read_active(|t| t.doc.form_values().into_iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>().join("\n")).unwrap_or_default();
        files::save_text_file(st, "form-data.txt", text);
    },
    "forms.highlight" => toggle_highlight,
    "forms.highlight.all" => toggle_highlight,
}
