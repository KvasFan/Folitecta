//! Attachments pane commands: open, save, edit, columns and showing the pane
//! for documents with attachments.

use folitecta_core::Attachment;
use folitecta_core::attachments::is_executable_name;

use crate::commands::{handlers, no_doc, option_on};
use crate::files;
use crate::i18n::{t, tf};
use crate::state::{AppState, Dialog, Pane, ToastKind};

/// Columns of the attachments pane: option id and i18n key of the header.
pub const COLUMNS: &[(&str, &str)] = &[
    ("attachmentsView.hdr.name", "attach.col.name"),
    ("attachmentsView.hdr.description", "attach.col.description"),
    ("attachmentsView.hdr.modified", "attach.col.modified"),
    ("attachmentsView.hdr.size", "attach.col.size"),
    ("attachmentsView.hdr.compressed", "attach.col.compressed"),
    ("attachmentsView.hdr.location", "attach.col.location"),
    ("attachmentsView.hdr.type", "attach.col.type"),
    ("attachmentsView.hdr.relationship", "attach.col.relationship"),
];

/// The selected attachment.
pub fn selected(st: AppState) -> Option<(usize, Attachment)> {
    st.read_active(|t| {
        let i = t.selected_attachment?;
        t.doc.attachments().get(i).cloned().map(|a| (i, a))
    })
    .flatten()
}

fn with_selected(st: AppState, f: impl FnOnce(AppState, usize, Attachment)) {
    if no_doc(st) {
        return;
    }
    match selected(st) {
        Some((i, a)) => f(st, i, a),
        None => st.toast(ToastKind::Info, t(st.lang(), "toast.select_attachment"), ""),
    }
}

/// A file name safe to create in a folder.
fn safe_file_name(name: &str) -> String {
    // The last part of a path written with either separator, on every system.
    let base = name.rsplit(['/', '\\']).next().unwrap_or_default();
    let cleaned: String = base
        .chars()
        .map(|c| {
            if c.is_control() || "<>:\"/\\|?*".contains(c) {
                '_'
            } else {
                c
            }
        })
        .collect();
    let cleaned = cleaned.trim().trim_matches('.').to_string();
    if cleaned.is_empty() {
        "attachment".into()
    } else {
        cleaned
    }
}

/// Open an attachment with the system's application for its type. Files that
/// can run code are not opened: they can be saved instead.
pub fn open_attachment(st: AppState, att: Attachment) {
    if is_executable_name(&att.name) {
        st.toast(
            ToastKind::Warn,
            tf(st.lang(), "toast.attachment_risky", &[("name", &att.name)]),
            "",
        );
        return;
    }
    let data = match st.read_active(|t| t.doc.attachment_data(&att).map(|d| (d, t.doc.is_protected()))) {
        Some(Ok(d)) => d,
        Some(Err(e)) => return st.error(e),
        None => return,
    };
    let (data, protected) = data;
    // A folder of the user's own; the one of protected documents goes as
    // soon as no protected document is open.
    let kind = if protected {
        crate::private_files::PROTECTED
    } else {
        crate::private_files::ATTACHMENTS
    };
    let r = crate::private_files::private_dir(kind)
        .map(|dir| dir.join(safe_file_name(&att.name)))
        .and_then(|path| crate::private_files::write_private_file(&path, &data).map(|_| path))
        .map_err(|e| e.to_string())
        .and_then(|path| crate::util::open_external(&path.display().to_string()));
    match r {
        Err(e) => st.error(e),
        Ok(()) if protected => st.toast(ToastKind::Info, t(st.lang(), "toast.protected_attachment"), ""),
        Ok(()) => {}
    }
}

/// Show the attachments pane for a document that has attachments (when the option is on).
pub fn auto_show(st: AppState) {
    if !option_on(st, "attachmentsView.autoShowPane") {
        return;
    }
    if st
        .read_active(|t| !t.doc.attachments().is_empty())
        .unwrap_or(false)
    {
        let mut ui = st.ui_mut();
        ui.show_left_pane = true;
        ui.left_pane = Pane::Attachments.id();
    }
}

handlers! {
    "attachmentsView.new" => |st| { if !no_doc(st) { files::add_attachment(st) } },
    "attachmentsView.open" => |st| with_selected(st, |st, _, a| open_attachment(st, a)),
    "attachmentsView.save" => |st| with_selected(st, |st, _, a| files::save_attachment(st, a)),
    "attachmentsView.edit" => |st| with_selected(st, |st, i, _| st.set_dialog(Dialog::AttachmentEdit(i))),
    "attachmentsView.options" => |st| st.set_dialog(Dialog::AttachmentOptions),
}

#[cfg(test)]
mod tests {
    use super::safe_file_name;

    #[test]
    fn attachment_names_are_made_safe() {
        assert_eq!(safe_file_name("../../etc/passwd"), "passwd");
        assert_eq!(safe_file_name("a:b*c?.txt"), "a_b_c_.txt");
        assert_eq!(safe_file_name(".."), "attachment");
        assert_eq!(safe_file_name("report.pdf"), "report.pdf");
    }
}
