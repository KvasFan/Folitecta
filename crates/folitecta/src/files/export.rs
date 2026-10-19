//! Pages and text written to other files: pictures of pages, extracted and split documents, text, embedded pictures, attachments.

use super::*;

/// Export selected pages as images into a chosen folder.
pub fn export_images(st: AppState, pages: Vec<usize>, dpi: f32, format: ImageFormat) {
    spawn(async move {
        let Some(dir) = rfd::AsyncFileDialog::new()
            .set_title("Choose a folder for the images")
            .pick_folder()
            .await
        else {
            return;
        };
        let dir = dir.path().to_path_buf();
        let prefix = st
            .read_active(|t| {
                t.doc
                    .path()
                    .and_then(|p| p.file_stem())
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| "page".into())
            })
            .unwrap_or_else(|| "page".into());
        st.set_busy(true);
        let result = st.read_active(|t| t.doc.export_pages_images(&pages, &dir, dpi, format, &prefix));
        st.set_busy(false);
        match result {
            Some(Ok(files)) => st.toast(
                ToastKind::Ok,
                tf(
                    st.lang(),
                    "toast.exported",
                    &[
                        ("count", &files.len().to_string()),
                        ("dir", &dir.display().to_string()),
                    ],
                ),
                "",
            ),
            Some(Err(e)) => st.error(e),
            None => {}
        }
    });
}

/// Extract pages into a new file chosen by the user.
pub fn extract_pages(st: AppState, pages: Vec<usize>) {
    let name = suggested_name(st, "-extract");
    spawn(async move {
        let Some(file) = pdf_dialog()
            .set_title("Extract pages to")
            .set_file_name(&name)
            .save_file()
            .await
        else {
            return;
        };
        let path = file.path().to_path_buf();
        let result = st.read_active(|t| t.doc.extract_pages(&pages));
        match result {
            Some(Ok(mut part)) => match part.save_as(&path) {
                Ok(()) => st.toast(
                    ToastKind::Ok,
                    tf(st.lang(), "toast.saved", &[("name", &path.display().to_string())]),
                    protected_detail(st, &part),
                ),
                Err(e) => st.core_error(&e),
            },
            Some(Err(e)) => st.core_error(&e),
            None => {}
        }
    });
}

/// The detail of the message about files made from a protected document.
pub(crate) fn protected_detail(st: AppState, doc: &Document) -> String {
    if doc.is_protected() {
        t(st.lang(), "toast.protected_kept")
    } else {
        String::new()
    }
}

/// Split the active document into files of `every` pages inside a chosen folder.
pub fn split_document(st: AppState, every: usize) {
    spawn(async move {
        let Some(dir) = rfd::AsyncFileDialog::new()
            .set_title("Choose a folder for the parts")
            .pick_folder()
            .await
        else {
            return;
        };
        let dir = dir.path().to_path_buf();
        let stem = st
            .read_active(|t| {
                t.doc
                    .path()
                    .and_then(|p| p.file_stem())
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| "part".into())
            })
            .unwrap_or_else(|| "part".into());
        let result = st.read_active(|t| t.doc.split_every(every));
        match result {
            Some(Ok(parts)) => {
                let mut count = 0;
                let detail = parts.first().map(|p| protected_detail(st, p)).unwrap_or_default();
                for (i, mut part) in parts.into_iter().enumerate() {
                    let path = dir.join(format!("{stem}-{:03}.pdf", i + 1));
                    match part.save_as(&path) {
                        Ok(()) => count += 1,
                        Err(e) => st.core_error(&e),
                    }
                }
                st.toast(
                    ToastKind::Ok,
                    tf(
                        st.lang(),
                        "toast.exported",
                        &[("count", &count.to_string()), ("dir", &dir.display().to_string())],
                    ),
                    detail,
                );
            }
            Some(Err(e)) => st.error(e),
            None => {}
        }
    });
}

/// Pick a PDF and open the "insert pages" dialog for it.
pub fn pick_file_to_insert(st: AppState) {
    spawn(async move {
        if let Some(file) = pdf_dialog().set_title("Insert pages from").pick_file().await {
            st.set_dialog(Dialog::MergeFile(file.path().to_path_buf()));
        }
    });
}

pub fn insert_from_file(st: AppState, path: PathBuf, at: usize) {
    match Document::open(&path) {
        Ok(other) => {
            if let Some(Err(e)) = st.with_active(|t| t.doc.insert_document(at, &other, None)) {
                st.core_error(&e);
            }
        }
        Err(e) => st.error(format!("{}: {}", path.display(), st.core_error_text(&e))),
    }
}

/// Export extracted text to a chosen file.
pub fn export_text(st: AppState) {
    let name = suggested_name(st, "").replace(".pdf", ".txt");
    spawn(async move {
        let Some(file) = rfd::AsyncFileDialog::new()
            .add_filter("Text", &["txt"])
            .set_file_name(&name)
            .save_file()
            .await
        else {
            return;
        };
        match st.read_active(|t| t.doc.export_text(file.path())) {
            Some(Ok(())) => st.toast(
                ToastKind::Ok,
                tf(
                    st.lang(),
                    "toast.saved",
                    &[("name", &file.path().display().to_string())],
                ),
                "",
            ),
            Some(Err(e)) => st.error(e),
            None => {}
        }
    });
}

/// Write arbitrary text to a user-chosen file (summaries, form data).
pub fn save_text_file(st: AppState, suggested: &str, contents: String) {
    let suggested = suggested.to_string();
    // The filter follows the extension of the suggested name.
    let ext = std::path::Path::new(&suggested)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("txt")
        .to_lowercase();
    let exts: Vec<String> = if ext == "txt" || ext == "csv" {
        vec!["txt".into(), "csv".into()]
    } else {
        vec![ext.clone()]
    };
    spawn(async move {
        let refs: Vec<&str> = exts.iter().map(String::as_str).collect();
        let Some(file) = rfd::AsyncFileDialog::new()
            .add_filter(ext.to_uppercase(), &refs)
            .set_file_name(&suggested)
            .save_file()
            .await
        else {
            return;
        };
        match std::fs::write(file.path(), contents) {
            Ok(()) => st.toast(
                ToastKind::Ok,
                tf(
                    st.lang(),
                    "toast.saved",
                    &[("name", &file.path().display().to_string())],
                ),
                "",
            ),
            Err(e) => st.error(e),
        }
    });
}

/// Extract embedded images of the given pages into a folder.
pub fn extract_images(st: AppState, pages: Vec<usize>) {
    spawn(async move {
        let Some(dir) = rfd::AsyncFileDialog::new()
            .set_title("Choose a folder for the images")
            .pick_folder()
            .await
        else {
            return;
        };
        let dir = dir.path().to_path_buf();
        match st.read_active(|t| t.doc.extract_images(&pages, &dir)) {
            Some(Ok(files)) => st.toast(
                ToastKind::Ok,
                tf(
                    st.lang(),
                    "toast.exported",
                    &[
                        ("count", &files.len().to_string()),
                        ("dir", &dir.display().to_string()),
                    ],
                ),
                "",
            ),
            Some(Err(e)) => st.error(e),
            None => {}
        }
    });
}

/// Attach a file chosen by the user to the active document.
pub fn add_attachment(st: AppState) {
    spawn(async move {
        let Some(file) = rfd::AsyncFileDialog::new()
            .set_title("Attach file")
            .pick_file()
            .await
        else {
            return;
        };
        if let Some(Err(e)) = st.with_active(|t| t.doc.add_attachment_from_path(file.path(), "")) {
            st.error(e);
        }
    });
}

/// Save an attachment to disk.
pub fn save_attachment(st: AppState, att: folitecta_core::Attachment) {
    spawn(async move {
        let Some(file) = rfd::AsyncFileDialog::new()
            .set_file_name(&att.name)
            .save_file()
            .await
        else {
            return;
        };
        match st.read_active(|t| t.doc.save_attachment(&att, file.path())) {
            Some(Ok(())) => st.toast(
                ToastKind::Ok,
                tf(
                    st.lang(),
                    "toast.saved",
                    &[("name", &file.path().display().to_string())],
                ),
                "",
            ),
            Some(Err(e)) => st.error(e),
            None => {}
        }
    });
}
