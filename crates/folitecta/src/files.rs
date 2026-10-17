//! File operations: open, save, export, with native dialogs.

use std::path::{Path, PathBuf};

use dioxus::prelude::*;
use folitecta_core::{Document, Error as CoreError, ImageFormat};

use crate::i18n::{t, tf};
use crate::state::{AfterUnsaved, AppState, Dialog, DocTab, ToastKind};

fn pdf_dialog() -> rfd::AsyncFileDialog {
    rfd::AsyncFileDialog::new()
        .add_filter("PDF documents", &["pdf"])
        .add_filter("All files", &["*"])
}

/// Open a document from disk and show it in a new tab.
pub fn open_path(st: AppState, path: PathBuf, password: Option<&str>) {
    // Already open? Just activate it.
    let existing = st
        .docs
        .read()
        .iter()
        .position(|t| t.doc.path() == Some(path.as_path()));
    if let Some(idx) = existing {
        st.set_active(idx);
        return;
    }
    let result = match password {
        Some(pw) => Document::open_with_password(&path, pw),
        None => Document::open(&path),
    };
    match result {
        Ok(doc) => {
            let name = doc.display_name();
            let (title, pages) = (doc.info().title, doc.page_count());
            let id = st.alloc_id();
            st.docs_mut().push(DocTab::new(id, doc));
            let idx = st.docs.read().len() - 1;
            st.set_active(idx);
            st.ui_mut().push_recent(&path);
            crate::commands::history::record_open(st, &path, &title, pages);
            crate::commands::attachments::auto_show(st);
            st.save_prefs();
            set_window_title(st);
            st.toast(
                ToastKind::Ok,
                tf(st.lang(), "toast.opened", &[("name", &name)]),
                "",
            );
        }
        Err(CoreError::PasswordRequired) => st.set_dialog(Dialog::Password(path)),
        Err(e) => st.error(format!("{}: {e}", path.display())),
    }
}

/// Show the open dialog and open every selected file.
pub fn open_dialog(st: AppState) {
    spawn(async move {
        if let Some(files) = pdf_dialog().set_title("Open PDF").pick_files().await {
            for f in files {
                open_path(st, f.path().to_path_buf(), None);
            }
        }
    });
}

/// Create a blank document tab.
pub fn new_blank(st: AppState, pages: usize, width: f32, height: f32) {
    match Document::new_blank(pages, width, height) {
        Ok(mut doc) => {
            doc.clear_history();
            let id = st.alloc_id();
            st.docs_mut().push(DocTab::new(id, doc));
            let idx = st.docs.read().len() - 1;
            st.set_active(idx);
            set_window_title(st);
        }
        Err(e) => st.error(e),
    }
}

pub fn set_window_title(st: AppState) {
    let title = match st.read_active(|t| (t.title(), t.doc.is_dirty())) {
        Some((name, dirty)) => format!("{}{} - Folitecta", name, if dirty { " •" } else { "" }),
        None => "Folitecta".to_string(),
    };
    dioxus::desktop::window().set_title(&title);
}

/// Save the active document; falls back to Save As when it has no path.
pub fn save(st: AppState) {
    let has_path = st.read_active(|t| t.doc.path().is_some()).unwrap_or(false);
    if !has_path {
        save_as(st);
        return;
    }
    let result = st.with_active(|t| t.doc.save());
    match result {
        Some(Ok(())) => {
            let name = st.read_active(|t| t.title()).unwrap_or_default();
            st.toast(
                ToastKind::Ok,
                tf(st.lang(), "toast.saved", &[("name", &name)]),
                "",
            );
            set_window_title(st);
            // Saved: its recovery copy goes.
            crate::recovery::checkpoint(st);
        }
        Some(Err(e)) => st.error(e),
        None => st.toast(ToastKind::Warn, t(st.lang(), "toast.no_document"), ""),
    }
}

pub(crate) fn suggested_name(st: AppState, suffix: &str) -> String {
    st.read_active(|t| {
        let stem = t
            .doc
            .path()
            .and_then(|p| p.file_stem())
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "document".into());
        format!("{stem}{suffix}.pdf")
    })
    .unwrap_or_else(|| format!("document{suffix}.pdf"))
}

pub fn save_as(st: AppState) {
    if !st.has_doc() {
        st.toast(ToastKind::Warn, t(st.lang(), "toast.no_document"), "");
        return;
    }
    let name = suggested_name(st, "");
    spawn(async move {
        let Some(file) = pdf_dialog()
            .set_title("Save As")
            .set_file_name(&name)
            .save_file()
            .await
        else {
            return;
        };
        let path = file.path().to_path_buf();
        match st.with_active(|t| t.doc.save_as(&path)) {
            Some(Ok(())) => {
                st.ui_mut().push_recent(&path);
                st.save_prefs();
                set_window_title(st);
                // Saved: its recovery copy goes.
                crate::recovery::checkpoint(st);
                st.toast(
                    ToastKind::Ok,
                    tf(
                        st.lang(),
                        "toast.saved",
                        &[(
                            "name",
                            &path
                                .file_name()
                                .map(|n| n.to_string_lossy().to_string())
                                .unwrap_or_default(),
                        )],
                    ),
                    "",
                );
            }
            Some(Err(e)) => st.error(e),
            None => {}
        }
    });
}

pub fn save_copy(st: AppState) {
    if !st.has_doc() {
        st.toast(ToastKind::Warn, t(st.lang(), "toast.no_document"), "");
        return;
    }
    let name = suggested_name(st, "-copy");
    spawn(async move {
        let Some(file) = pdf_dialog()
            .set_title("Save a Copy")
            .set_file_name(&name)
            .save_file()
            .await
        else {
            return;
        };
        let path = file.path().to_path_buf();
        match st.with_active(|t| t.doc.save_copy(&path)) {
            Some(Ok(())) => st.toast(
                ToastKind::Ok,
                tf(st.lang(), "toast.saved", &[("name", &path.display().to_string())]),
                "",
            ),
            Some(Err(e)) => st.error(e),
            None => {}
        }
    });
}

pub fn save_all(st: AppState) {
    let n = st.docs.read().len();
    let mut errors = Vec::new();
    for i in 0..n {
        let mut docs = st.docs_mut();
        let tab = &mut docs[i];
        if tab.doc.path().is_some() && tab.doc.is_dirty() {
            if let Err(e) = tab.doc.save() {
                errors.push(e.to_string());
            }
        }
    }
    for e in errors {
        st.error(e);
    }
    set_window_title(st);
    crate::recovery::checkpoint(st);
}

/// Close a tab by id; asks about unsaved changes unless `force`.
pub fn close_tab(st: AppState, id: u64, force: bool, then: AfterUnsaved) {
    let info = st.docs.read().iter().position(|t| t.id == id).map(|idx| {
        (
            idx,
            st.docs.read()[idx].doc.is_dirty(),
            st.docs.read()[idx].doc.path().map(Path::to_path_buf),
        )
    });
    let Some((idx, dirty, path)) = info else { return };
    if dirty && !force {
        st.set_dialog(Dialog::Unsaved { tab: id, then });
        return;
    }
    let tab = st.docs_mut().remove(idx);
    if let Some(p) = path {
        crate::commands::history::record_close(st, &p, tab.page, &tab.doc);
        st.closed_mut().push(p);
    }
    // The recovery copy of a closed document goes.
    crate::recovery::checkpoint(st);
    let n = st.docs.read().len();
    let active = (st.active)();
    if n == 0 {
        st.set_active(0);
    } else if active >= n {
        st.set_active(n - 1);
    } else if active > idx {
        st.set_active(active - 1);
    }
    set_window_title(st);
}

pub fn close_active(st: AppState) {
    if let Some(id) = st.read_active(|t| t.id) {
        close_tab(st, id, false, AfterUnsaved::Close);
    }
}

pub fn close_all(st: AppState) {
    let ids: Vec<(u64, bool)> = st.docs.read().iter().map(|t| (t.id, t.doc.is_dirty())).collect();
    if let Some((id, _)) = ids.iter().find(|(_, dirty)| *dirty) {
        st.set_dialog(Dialog::Unsaved {
            tab: *id,
            then: AfterUnsaved::CloseAll,
        });
        return;
    }
    for (id, _) in ids {
        close_tab(st, id, true, AfterUnsaved::Close);
    }
}

/// Continue after the unsaved-changes dialog.
pub fn resolve_unsaved(st: AppState, tab: u64, then: AfterUnsaved, save_first: bool) {
    if save_first {
        let idx = st.docs.read().iter().position(|t| t.id == tab);
        if let Some(idx) = idx {
            st.set_active(idx);
            let has_path = st.read_active(|t| t.doc.path().is_some()).unwrap_or(false);
            if !has_path {
                // Save As is asynchronous; the user can close again afterwards.
                save_as(st);
                return;
            }
            if let Some(Err(e)) = st.with_active(|t| t.doc.save()) {
                st.error(e);
                return;
            }
        }
    }
    close_tab(st, tab, true, then.clone());
    match then {
        AfterUnsaved::Close => {}
        AfterUnsaved::CloseAll => close_all(st),
        AfterUnsaved::Exit => exit_app(st),
    }
}

pub fn exit_app(st: AppState) {
    let dirty: Option<u64> = st.docs.read().iter().find(|t| t.doc.is_dirty()).map(|t| t.id);
    match dirty {
        Some(id) => st.set_dialog(Dialog::Unsaved {
            tab: id,
            then: AfterUnsaved::Exit,
        }),
        None => {
            // Left on purpose: nothing to recover next time.
            crate::recovery::forget_session();
            dioxus::desktop::window().close()
        }
    }
}

pub fn revert(st: AppState) {
    let path = st.read_active(|t| t.doc.path().map(Path::to_path_buf)).flatten();
    let Some(path) = path else {
        st.toast(ToastKind::Warn, t(st.lang(), "toast.no_document"), "");
        return;
    };
    match Document::open(&path) {
        Ok(doc) => {
            st.with_active(|t| {
                t.doc = doc;
                t.revision = u64::MAX; // force refresh
            });
            set_window_title(st);
            // Back to the file: no changes left to recover.
            crate::recovery::checkpoint(st);
        }
        Err(e) => st.error(e),
    }
}

pub fn reopen_closed(st: AppState) {
    let path = st.closed_mut().pop();
    match path {
        Some(p) => open_path(st, p, None),
        None => st.toast(ToastKind::Info, t(st.lang(), "label.none"), ""),
    }
}

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
                    "",
                ),
                Err(e) => st.error(e),
            },
            Some(Err(e)) => st.error(e),
            None => {}
        }
    });
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
                for (i, mut part) in parts.into_iter().enumerate() {
                    let path = dir.join(format!("{stem}-{:03}.pdf", i + 1));
                    match part.save_as(&path) {
                        Ok(()) => count += 1,
                        Err(e) => st.error(e),
                    }
                }
                st.toast(
                    ToastKind::Ok,
                    tf(
                        st.lang(),
                        "toast.exported",
                        &[("count", &count.to_string()), ("dir", &dir.display().to_string())],
                    ),
                    "",
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
                st.error(e);
            }
        }
        Err(e) => st.error(format!("{}: {e}", path.display())),
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

// ---- Images ------------------------------------------------------------------

/// Extensions of the picture files that can be read.
pub const PICTURE_EXTS: [&str; 8] = ["png", "jpg", "jpeg", "gif", "bmp", "tif", "tiff", "webp"];

fn image_dialog() -> rfd::AsyncFileDialog {
    rfd::AsyncFileDialog::new()
        .add_filter("Images", &PICTURE_EXTS)
        .add_filter("All files", &["*"])
}

async fn read_files(files: Vec<rfd::FileHandle>) -> Vec<Vec<u8>> {
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

// ---- Form data -----------------------------------------------------------------

/// Save the form values as XFDF, FDF, CSV or tab-separated text (by the chosen extension).
pub fn export_form_data(st: AppState) {
    let name = suggested_name(st, "-data").replace(".pdf", ".xfdf");
    let data_for = move |format: folitecta_core::FormDataFormat| {
        st.read_active(|t| t.doc.export_form_data(format))
            .unwrap_or_default()
    };
    spawn(async move {
        let Some(file) = rfd::AsyncFileDialog::new()
            .set_title("Export form data")
            .add_filter("XFDF", &["xfdf"])
            .add_filter("FDF", &["fdf"])
            .add_filter("CSV", &["csv"])
            .add_filter("Text", &["txt"])
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
        let format = folitecta_core::FormDataFormat::from_extension(&ext)
            .unwrap_or(folitecta_core::FormDataFormat::Xfdf);
        match std::fs::write(&path, data_for(format)) {
            Ok(()) => st.toast(
                ToastKind::Ok,
                tf(st.lang(), "toast.saved", &[("name", &path.display().to_string())]),
                "",
            ),
            Err(e) => st.error(e),
        }
    });
}

/// Fill the form from an XFDF, FDF, CSV or text file.
pub fn import_form_data(st: AppState) {
    spawn(async move {
        let Some(file) = rfd::AsyncFileDialog::new()
            .set_title("Import form data")
            .add_filter("Form data", &["xfdf", "xml", "fdf", "csv", "txt"])
            .add_filter("All files", &["*"])
            .pick_file()
            .await
        else {
            return;
        };
        let ext = file
            .path()
            .extension()
            .map(|e| e.to_string_lossy().to_string())
            .unwrap_or_default();
        let data = file.read().await;
        let format =
            folitecta_core::FormDataFormat::from_extension(&ext).unwrap_or(if data.starts_with(b"%FDF") {
                folitecta_core::FormDataFormat::Fdf
            } else {
                folitecta_core::FormDataFormat::Xfdf
            });
        match st.with_active(|t| t.doc.import_form_data(format, &data)) {
            Some(Ok(n)) => {
                set_window_title(st);
                st.toast(
                    ToastKind::Ok,
                    tf(st.lang(), "toast.fields_filled", &[("count", &n.to_string())]),
                    "",
                );
            }
            Some(Err(e)) => st.error(e),
            None => {}
        }
    });
}

/// A file name made safe for every system (no path separators or reserved characters).
fn safe_file_name(s: &str) -> String {
    let cleaned: String = s
        .trim()
        .chars()
        .map(|c| {
            if "\\/:*?\"<>|".contains(c) || c.is_control() {
                '_'
            } else {
                c
            }
        })
        .collect();
    cleaned.trim_matches(['.', ' ']).to_string()
}

/// The file name of row `n` (from 1) of a table: the value of a File column
/// when there is one, else the document name and the row number.
fn row_file_name(row: &[(String, String)], stem: &str, n: usize) -> String {
    let named = row
        .iter()
        .find(|(k, _)| {
            matches!(
                k.to_lowercase().as_str(),
                "file" | "filename" | "file name" | "file_name"
            )
        })
        .map(|(_, v)| safe_file_name(v.trim_end_matches(".pdf")))
        .filter(|v| !v.is_empty());
    format!("{}.pdf", named.unwrap_or_else(|| format!("{stem}-{n:03}")))
}

/// Populate Forms: one filled copy of the document for every row of a CSV
/// or tab-separated table (the first row names the fields), in a folder.
pub fn populate_forms(st: AppState) {
    let fields = st.read_active(|t| t.doc.form_fields().len()).unwrap_or(0);
    if fields == 0 {
        st.toast(ToastKind::Info, t(st.lang(), "toast.no_form_fields"), "");
        return;
    }
    let stem = suggested_name(st, "").trim_end_matches(".pdf").to_string();
    spawn(async move {
        let Some(table) = rfd::AsyncFileDialog::new()
            .set_title(t(st.lang(), "dialog.populate.table"))
            .add_filter("CSV", &["csv"])
            .add_filter("Text", &["txt", "tsv"])
            .pick_file()
            .await
        else {
            return;
        };
        let ext = table
            .path()
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        let format = if ext == "csv" {
            folitecta_core::FormDataFormat::Csv
        } else {
            folitecta_core::FormDataFormat::Text
        };
        let rows = match folitecta_core::form_data::form_rows(format, &table.read().await) {
            Ok(r) => r,
            Err(e) => return st.error(e),
        };
        let Some(folder) = rfd::AsyncFileDialog::new()
            .set_title(t(st.lang(), "dialog.populate.folder"))
            .pick_folder()
            .await
        else {
            return;
        };
        let folder = folder.path().to_path_buf();
        let mut written = 0;
        for (i, row) in rows.iter().enumerate() {
            let copy = st.read_active(|t| t.doc.filled_copy(row));
            let path = folder.join(row_file_name(row, &stem, i + 1));
            match copy {
                Some(Ok((mut doc, _))) => match doc.save_as(&path) {
                    Ok(()) => written += 1,
                    Err(e) => return st.error(e),
                },
                Some(Err(e)) => return st.error(e),
                None => return,
            }
        }
        st.toast(
            ToastKind::Ok,
            tf(
                st.lang(),
                "toast.forms_populated",
                &[
                    ("count", &written.to_string()),
                    ("folder", &folder.display().to_string()),
                ],
            ),
            "",
        );
    });
}

#[cfg(test)]
mod populate_tests {
    use super::*;

    #[test]
    fn rows_name_their_files() {
        let row = |pairs: &[(&str, &str)]| -> Vec<(String, String)> {
            pairs
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect()
        };
        assert_eq!(row_file_name(&row(&[("Name", "Ada")]), "form", 7), "form-007.pdf");
        assert_eq!(
            row_file_name(&row(&[("File", "a/b: c.pdf")]), "form", 1),
            "a_b_ c.pdf"
        );
        assert_eq!(
            row_file_name(&row(&[("filename", "  ")]), "form", 2),
            "form-002.pdf"
        );
    }
}

/// Replace the active tab with the file at path (used after signing, when the
/// saved file is the document to continue with).
pub fn replace_active_with(st: AppState, path: PathBuf) {
    if let Some(i) = st.active_index() {
        st.docs_mut().remove(i);
        let n = st.docs.read().len();
        if n > 0 {
            st.set_active(i.min(n - 1));
        }
    }
    open_path(st, path, None);
}

/// Open a document created in memory (untitled) in a new tab.
pub fn open_document_tab(st: AppState, mut doc: Document) {
    doc.clear_history();
    let id = st.alloc_id();
    st.docs_mut().push(DocTab::new(id, doc));
    let n = st.docs.read().len();
    st.set_active(n - 1);
    set_window_title(st);
}

// ---- Comment exchange ----------------------------------------------------------

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
            Some(Err(e)) => st.error(e),
            None => {}
        }
    });
}

// ---- Printing ----------------------------------------------------------------

/// Lay the pages out in the print view, wait until they are rendered and open
/// the system print dialog.
pub fn start_print(
    st: AppState,
    pages: Vec<crate::state::PrintPage>,
    dpi: f32,
    annotations: bool,
    fit: bool,
) {
    if pages.is_empty() {
        return;
    }
    spawn(async move {
        prepare_print(st, pages, dpi, annotations, fit).await;
        // The system dialog is modal: the page waits until it is closed.
        dioxus::desktop::window().print();
    });
}

/// Lay the pages out in the hidden print view and wait until every page image
/// is decoded. Returns the number of pages ready.
pub async fn prepare_print(
    st: AppState,
    pages: Vec<crate::state::PrintPage>,
    dpi: f32,
    annotations: bool,
    fit: bool,
) -> usize {
    let id = st.alloc_id();
    let scale_milli = ((dpi / 72.0) * 1000.0).round().clamp(500.0, 6000.0) as u32;
    st.set_print_job(Some(crate::state::PrintJob {
        pages,
        scale_milli,
        annotations,
        fit,
        id,
    }));
    st.set_busy(true);
    let js = r#"
for (let i = 0; i < 200 && !document.querySelector('.print-view'); i++) { await new Promise(r => setTimeout(r, 30)); }
const imgs = [...document.querySelectorAll('.print-view img')];
await Promise.all(imgs.map(img => img.decode ? img.decode().catch(() => {}) : Promise.resolve()));
return imgs.filter(img => img.naturalWidth > 0).length;
"#;
    let ready = document::eval(js)
        .await
        .ok()
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as usize;
    st.set_busy(false);
    ready
}
// ---- Pages from another PDF ------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PickPurpose {
    Replace,
    Overlay,
}

/// Choose a PDF and open the dialog that uses it.
pub fn pick_file_for(st: AppState, purpose: PickPurpose) {
    spawn(async move {
        let title = match purpose {
            PickPurpose::Replace => "Replace pages with pages from",
            PickPurpose::Overlay => "Overlay pages from",
        };
        if let Some(file) = pdf_dialog().set_title(title).pick_file().await {
            let path = file.path().to_path_buf();
            st.set_dialog(match purpose {
                PickPurpose::Replace => Dialog::ReplacePages(path),
                PickPurpose::Overlay => Dialog::OverlayFile(path),
            });
        }
    });
}

/// Add consecutive Bates numbers to several files, written next to the originals
/// with a `-bates` suffix.
pub fn bates_multiple(st: AppState) {
    spawn(async move {
        let Some(files) = pdf_dialog()
            .set_title("Bates numbering: choose the documents")
            .pick_files()
            .await
        else {
            return;
        };
        // The format and the next number of the Bates dialog.
        let format = crate::commands::macros::bates_format(st);
        let mut next = crate::commands::macros::bates_next(st);
        let first = next;
        let mut written = 0;
        for (docnumber, f) in files.into_iter().enumerate() {
            let path = f.path().to_path_buf();
            match Document::open(&path) {
                Ok(mut doc) => {
                    let pages: Vec<usize> = (0..doc.page_count()).collect();
                    let n = pages.len() as i64;
                    let template = format.replace("{docnumber}", &(docnumber + 1).to_string());
                    if let Err(e) = doc.add_page_numbers(
                        &pages,
                        &template,
                        next,
                        folitecta_core::MarkPosition::BottomRight,
                        9.0,
                        24.0,
                        [0.0, 0.0, 0.0],
                    ) {
                        st.error(format!("{}: {e}", path.display()));
                        continue;
                    }
                    let stem = path
                        .file_stem()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_else(|| "document".into());
                    let out = path.with_file_name(format!("{stem}-bates.pdf"));
                    match doc.save_as(&out) {
                        Ok(()) => {
                            written += 1;
                            next += n;
                        }
                        Err(e) => st.error(e),
                    }
                }
                Err(e) => st.error(format!("{}: {e}", path.display())),
            }
        }
        if next != first {
            st.set_value("bates.next", next.to_string());
        }
        st.toast(
            ToastKind::Ok,
            tf(
                st.lang(),
                "toast.bates_done",
                &[("count", &written.to_string()), ("last", &(next - 1).to_string())],
            ),
            "",
        );
    });
}

// ---- Settings --------------------------------------------------------------------

pub fn export_settings(st: AppState) {
    let json = serde_json::to_string_pretty(&*st.ui.read()).unwrap_or_default();
    spawn(async move {
        let Some(file) = rfd::AsyncFileDialog::new()
            .add_filter("Settings", &["json"])
            .set_file_name("folitecta-settings.json")
            .save_file()
            .await
        else {
            return;
        };
        match std::fs::write(file.path(), json) {
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

pub fn import_settings(st: AppState) {
    spawn(async move {
        let Some(file) = rfd::AsyncFileDialog::new()
            .add_filter("Settings", &["json"])
            .pick_file()
            .await
        else {
            return;
        };
        let text = String::from_utf8_lossy(&file.read().await).to_string();
        match serde_json::from_str::<crate::state::UiPrefs>(&text) {
            Ok(prefs) => {
                *st.ui_mut() = prefs;
                st.save_prefs();
                st.toast(ToastKind::Ok, t(st.lang(), "toast.settings_imported"), "");
            }
            Err(e) => st.error(e),
        }
    });
}

/// Write an encrypted copy of the active document.
pub fn save_encrypted(st: AppState, user: String, owner: String, perms: folitecta_core::SecurityPermissions) {
    let name = suggested_name(st, "-protected");
    spawn(async move {
        let Some(file) = pdf_dialog()
            .set_title("Save protected copy")
            .set_file_name(&name)
            .save_file()
            .await
        else {
            return;
        };
        let path = file.path().to_path_buf();
        match st.read_active(|t| t.doc.save_encrypted(&path, &user, &owner, perms)) {
            Some(Ok(())) => st.toast(
                ToastKind::Ok,
                tf(st.lang(), "toast.saved", &[("name", &path.display().to_string())]),
                t(st.lang(), "toast.protected_note"),
            ),
            Some(Err(e)) => st.error(e),
            None => {}
        }
    });
}
