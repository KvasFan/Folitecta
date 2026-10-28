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
            Some(Ok(bytes)) => match folitecta_core::util::write_atomic(&path, bytes) {
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
        import_comment_data(st, data);
    });
}

/// Import the comments of the data of an XFDF, FDF or PDF file. When some of
/// them cannot be drawn, nothing is imported and a confirmation names them
/// and offers to import the others.
pub fn import_comment_data(st: AppState, data: Vec<u8>) {
    match st.with_active(|t| t.doc.import_comments(&data)) {
        Some(Ok(n)) => {
            set_window_title(st);
            st.toast(
                ToastKind::Ok,
                tf(st.lang(), "toast.comments_imported", &[("count", &n.to_string())]),
                "",
            );
        }
        Some(Err(CoreError::Undrawable(list))) => {
            let lang = st.lang();
            st.set_dialog(Dialog::Confirm {
                title: t(lang, "cmd.comments.import"),
                body: tf(
                    lang,
                    "dialog.import_undrawable.body",
                    &[
                        ("count", &list.len().to_string()),
                        ("list", &crate::i18n::undrawable_list(lang, &list)),
                    ],
                ),
                action: crate::state::ConfirmAction::ImportCommentsSkipping(std::sync::Arc::new(data)),
            });
        }
        Some(Err(e)) => st.core_error(&e),
        None => {}
    }
}

/// Import the comments of `data` that can be drawn, leaving out the others.
pub fn import_drawable_comments(st: AppState, data: &[u8]) {
    match st.with_active(|t| t.doc.import_comments_skipping(data)) {
        Some(Ok((n, skipped))) => {
            st.set_dialog(Dialog::None);
            set_window_title(st);
            st.toast(
                ToastKind::Ok,
                tf(
                    st.lang(),
                    "toast.comments_imported_skipped",
                    &[("count", &n.to_string()), ("skipped", &skipped.len().to_string())],
                ),
                "",
            );
        }
        Some(Err(e)) => st.core_error(&e),
        None => st.set_dialog(Dialog::None),
    }
}
