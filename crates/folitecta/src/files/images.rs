//! Picture files: placed on pages, attached, made into stamps, inserted as pages, or made into new documents (text files and e-mail messages too).

use super::*;

/// Extensions of the picture files that can be read.
pub const PICTURE_EXTS: [&str; 8] = ["png", "jpg", "jpeg", "gif", "bmp", "tif", "tiff", "webp"];

pub(crate) fn image_dialog() -> rfd::AsyncFileDialog {
    rfd::AsyncFileDialog::new()
        .add_filter("Images", &PICTURE_EXTS)
        .add_filter("All files", &["*"])
}

pub(crate) async fn read_files(files: Vec<rfd::FileHandle>) -> Vec<Vec<u8>> {
    let mut out = Vec::with_capacity(files.len());
    for f in files {
        out.push(f.read().await);
    }
    out
}

/// Choose a picture, then let the user drag the area where it goes.
pub fn pick_image_to_place(st: AppState) {
    spawn(async move {
        let Some(file) = image_dialog().set_title("Choose an image").pick_file().await else {
            return;
        };
        let data = file.read().await;
        match Document::image_size(&data) {
            Ok(_) => {
                st.set_pending_image(Some(data));
                st.set_tool(crate::state::Tool::PlaceImage);
                st.toast(ToastKind::Info, t(st.lang(), "toast.place_image"), "");
            }
            Err(e) => st.error(e),
        }
    });
}

/// Choose a file and attach it to `page` as a paper-clip comment at `at` (PDF space).
pub fn pick_page_attachment(st: AppState, page: usize, at: (f32, f32)) {
    spawn(async move {
        let Some(file) = rfd::AsyncFileDialog::new()
            .set_title("Attach a file to the page")
            .pick_file()
            .await
        else {
            return;
        };
        let data = file.read().await;
        let name = file.file_name();
        let author = st.ui.peek().author.clone();
        match st.with_active(|t| t.doc.add_file_attachment(page, at, &name, data, "", &author)) {
            Some(Ok(_)) => {
                set_window_title(st);
                if !crate::commands::option_on(st, "tool.keepSelected") {
                    st.set_tool(crate::state::Tool::Hand);
                }
            }
            Some(Err(e)) => st.error(e),
            None => {}
        }
    });
}

/// Choose a picture to use as a stamp, then click or drag on the page.
pub fn pick_image_stamp(st: AppState) {
    spawn(async move {
        let Some(file) = image_dialog().set_title("Choose a stamp image").pick_file().await else {
            return;
        };
        let data = file.read().await;
        match Document::image_size(&data) {
            Ok(_) => {
                crate::commands::stamps::clear_pending();
                st.set_pending_image(Some(data));
                st.set_value("stamp.kind", "image");
                st.set_tool(crate::state::Tool::Stamp);
                st.toast(ToastKind::Info, t(st.lang(), "toast.place_stamp"), "");
            }
            Err(e) => st.error(e),
        }
    });
}

/// Insert one page per chosen picture after the current page.
pub fn insert_image_pages(st: AppState) {
    spawn(async move {
        let Some(files) = image_dialog()
            .set_title("Insert images as pages")
            .pick_files()
            .await
        else {
            return;
        };
        let images = read_files(files).await;
        let at = st.current_page() + 1;
        match st.with_active(|t| t.doc.insert_image_pages(at, images)) {
            Some(Ok(n)) => {
                set_window_title(st);
                st.toast(
                    ToastKind::Ok,
                    tf(st.lang(), "toast.pages_inserted", &[("count", &n.to_string())]),
                    "",
                );
            }
            Some(Err(e)) => st.error(e),
            None => {}
        }
    });
}

/// New document with one page per picture.
pub fn new_from_images(st: AppState) {
    spawn(async move {
        let Some(files) = image_dialog()
            .set_title("Create a PDF from images")
            .pick_files()
            .await
        else {
            return;
        };
        let images = read_files(files).await;
        match Document::from_images(images) {
            Ok(doc) => {
                let id = st.alloc_id();
                st.docs_mut().push(DocTab::new(id, doc));
                let n = st.docs.read().len();
                st.set_active(n - 1);
                set_window_title(st);
            }
            Err(e) => st.error(e),
        }
    });
}

/// The words in front of the header lines of a message made into a document.
pub fn email_labels(lang: crate::i18n::Lang) -> folitecta_core::email::EmailLabels {
    folitecta_core::email::EmailLabels {
        from: t(lang, "label.mail_from"),
        to: t(lang, "label.mail_to"),
        cc: t(lang, "label.mail_cc"),
        date: t(lang, "label.mail_date"),
        attachments: t(lang, "label.mail_attachments"),
    }
}

/// A new document showing the e-mail message in `data` (an `.eml` file
/// named `name`): its header lines, its text and its attachments embedded.
pub fn open_email_bytes(st: AppState, data: &[u8], name: &str) {
    let made = folitecta_core::email::read_email(data).and_then(|msg| {
        Document::from_email(
            &msg,
            &email_labels(st.lang()),
            folitecta_core::TextLayout::default(),
        )
    });
    match made {
        Ok(mut doc) => {
            let stem = name.rsplit_once('.').map(|(s, _)| s).unwrap_or(name);
            doc.set_name_hint(&format!("{stem}.pdf"));
            open_document_tab(st, doc);
            st.toast(
                ToastKind::Ok,
                tf(st.lang(), "toast.opened", &[("name", name)]),
                "",
            );
        }
        Err(e) => st.error(format!("{name}: {e}")),
    }
}

/// Open an `.eml` file as a new document.
pub fn open_email(st: AppState, path: &Path) {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    match std::fs::read(path) {
        Ok(data) => open_email_bytes(st, &data, &name),
        Err(e) => st.error(format!("{}: {e}", path.display())),
    }
}

/// New documents from e-mail messages (`.eml` files).
pub fn new_from_email(st: AppState) {
    spawn(async move {
        let Some(files) = rfd::AsyncFileDialog::new()
            .set_title("Create a PDF from e-mail messages")
            .add_filter("E-mail", &["eml"])
            .add_filter("All files", &["*"])
            .pick_files()
            .await
        else {
            return;
        };
        for f in files {
            open_email(st, f.path());
        }
    });
}

/// New document from a plain text file (UTF-8, or Latin-1 when not valid UTF-8).
pub fn new_from_text_file(st: AppState) {
    spawn(async move {
        let Some(file) = rfd::AsyncFileDialog::new()
            .set_title("Create a PDF from a text file")
            .add_filter("Text", &["txt", "text", "md", "csv", "log", "ini", "json", "xml"])
            .add_filter("All files", &["*"])
            .pick_file()
            .await
        else {
            return;
        };
        let bytes = file.read().await;
        let text = match String::from_utf8(bytes) {
            Ok(s) => s.trim_start_matches('\u{feff}').to_string(),
            Err(e) => e.into_bytes().iter().map(|b| *b as char).collect(),
        };
        match Document::from_text(&text, folitecta_core::TextLayout::default()) {
            Ok(mut doc) => {
                let stem = file
                    .path()
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| "text".into());
                doc.set_path(None);
                let _ = doc.set_info(&stem, "", "", "");
                doc.clear_history();
                let id = st.alloc_id();
                st.docs_mut().push(DocTab::new(id, doc));
                let n = st.docs.read().len();
                st.set_active(n - 1);
                set_window_title(st);
            }
            Err(e) => st.error(e),
        }
    });
}
