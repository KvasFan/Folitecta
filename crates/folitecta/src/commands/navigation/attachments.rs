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
    // An attachment of a protected document goes to a folder only the user
    // can read, removed when Folitecta is left with Exit.
    let dir = if protected {
        protected_attachments_dir()
    } else {
        std::env::temp_dir()
            .join("Folitecta")
            .join(format!("attachments-{}", std::process::id()))
    };
    let path = dir.join(safe_file_name(&att.name));
    let r = private_dir(&dir, protected)
        .and_then(|_| write_private(&path, &data, protected))
        .map_err(|e| e.to_string())
        .and_then(|_| crate::util::open_external(&path.display().to_string()));
    match r {
        Err(e) => st.error(e),
        Ok(()) if protected => st.toast(ToastKind::Info, t(st.lang(), "toast.protected_attachment"), ""),
        Ok(()) => {}
    }
}

/// Where attachments of protected documents are opened from.
fn protected_attachments_dir() -> std::path::PathBuf {
    std::env::temp_dir()
        .join("Folitecta")
        .join(format!("protected-{}", std::process::id()))
}

/// Remove the attachments of protected documents opened by this window.
pub fn forget_protected_attachments() {
    let _ = std::fs::remove_dir_all(protected_attachments_dir());
}

/// Create `dir`; `private`: readable by the user only.
fn private_dir(dir: &std::path::Path, private: bool) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    #[cfg(unix)]
    if private {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
    }
    #[cfg(not(unix))]
    let _ = private;
    Ok(())
}

/// Write `data` to `path`; `private`: readable by the user only.
fn write_private(path: &std::path::Path, data: &[u8], private: bool) -> std::io::Result<()> {
    std::fs::write(path, data)?;
    #[cfg(unix)]
    if private {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    }
    #[cfg(not(unix))]
    let _ = private;
    Ok(())
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
