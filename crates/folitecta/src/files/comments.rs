//! Comments exported to and imported from XFDF, FDF and PDF files.

use super::*;

/// Save comments as XFDF or FDF (by the chosen extension): every comment, or
/// only the listed `(page, index)` pairs.
pub fn export_comments(st: AppState, only: Option<Vec<(usize, usize)>>) {
    let count = st
        .read_active(|t| t.doc.exchange_comment_count(only.as_deref()))
        .unwrap_or(0);
    if count == 0 {
        st.toast(ToastKind::Info, t(st.lang(), "toast.no_comments"), "");
        return;
    }
    let name = suggested_name(st, "-comments").replace(".pdf", ".xfdf");
    spawn(async move {
        let Some(file) = rfd::AsyncFileDialog::new()
            .set_title("Export comments")
            .add_filter("XFDF", &["xfdf"])
            .add_filter("FDF", &["fdf"])
            .set_file_name(&name)
            .save_file()
            .await
        else {
            return;
        };
        let path = file.path().to_path_buf();
        let ext = path
            .extension()
            .map(|e| e.to_string_lossy().to_string())
            .unwrap_or_default();
        let format = match folitecta_core::CommentFormat::from_extension(&ext) {
            Some(folitecta_core::CommentFormat::Fdf) => folitecta_core::CommentFormat::Fdf,
            _ => folitecta_core::CommentFormat::Xfdf,
        };
        let data = st.read_active(|t| t.doc.export_comments(format, only.as_deref()));
        match data {
            Some(Ok(bytes)) => match std::fs::write(&path, bytes) {
                Ok(()) => st.toast(
                    ToastKind::Ok,
                    tf(
                        st.lang(),
                        "toast.comments_exported",
                        &[
                            ("count", &count.to_string()),
                            ("name", &path.display().to_string()),
                        ],
                    ),
                    "",
                ),
                Err(e) => st.error(e),
            },
            Some(Err(e)) => st.error(e),
            None => {}
        }
    });
}

/// Import comments from an XFDF, FDF or PDF file.
pub fn import_comments(st: AppState) {
    spawn(async move {
        let Some(file) = rfd::AsyncFileDialog::new()
            .set_title("Import comments")
            .add_filter("Comments", &["xfdf", "fdf", "xml", "pdf"])
            .add_filter("All files", &["*"])
            .pick_file()
            .await
        else {
            return;
        };
        let data = file.read().await;
        match st.with_active(|t| t.doc.import_comments(&data)) {
            Some(Ok(n)) => {
                set_window_title(st);
                st.toast(
                    ToastKind::Ok,
                    tf(st.lang(), "toast.comments_imported", &[("count", &n.to_string())]),
                    "",
                );
            }
            Some(Err(e)) => st.core_error(&e),
            None => {}
        }
    });
}
