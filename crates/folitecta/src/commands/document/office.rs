//! Export to Office Formats (FOL-L009): Word, rich text or a web page made
//! away from the interface and written where the user chooses, never over
//! the document itself; the report says what it holds and what it left out.

use std::path::PathBuf;
use std::sync::Mutex;

use folitecta_core::OfficeFormat;

use crate::commands::{doc_dialog, handlers};
use crate::i18n::{t, tf};
use crate::state::{AppState, Dialog, ToastKind};

/// The answer the self-test gives to the next save window.
static ANSWER: Mutex<Option<PathBuf>> = Mutex::new(None);

/// Let the next export be written to `path` without asking (the self-test).
pub fn answer_save(path: PathBuf) {
    *ANSWER.lock().unwrap_or_else(|e| e.into_inner()) = Some(path);
}

/// Export the active document (its pages `all` or the current one) in `format`.
pub fn export(st: AppState, format: OfficeFormat, all: bool, page_breaks: bool) {
    let Some(stem) = st.read_active(|t| {
        t.doc
            .path()
            .and_then(|p| p.file_stem())
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| t.title())
    }) else {
        return;
    };
    st.set_dialog(Dialog::None);
    // The dialog that asked has closed: the work is not tied to it (a task
    // of the dialog would end with it, before the save window opens).
    dioxus::core::spawn_forever(async move {
        let answer = ANSWER.lock().unwrap_or_else(|e| e.into_inner()).take();
        let target = match answer {
            Some(path) => path,
            None => {
                let Some(file) = rfd::AsyncFileDialog::new()
                    .add_filter(format.id().to_uppercase(), &[format.extension()])
                    .set_file_name(format!("{stem}.{}", format.extension()))
                    .save_file()
                    .await
                else {
                    return;
                };
                file.path().to_path_buf()
            }
        };
        write_export(st, format, all, page_breaks, target);
    });
}

/// Write the export of the active document to `target`.
pub fn write_export(st: AppState, format: OfficeFormat, all: bool, page_breaks: bool, target: PathBuf) {
    let lang = st.lang();
    let Some((copy, page, n, path)) = st.read_active(|t| {
        (
            t.doc.duplicate(),
            t.page,
            t.doc.page_count(),
            t.doc.path().map(|p| p.to_path_buf()),
        )
    }) else {
        return;
    };
    let Ok(copy) = copy else { return };
    if path.as_ref().is_some_and(|p| *p == target) {
        st.toast(ToastKind::Warn, t(lang, "toast.office_not_over"), "");
        return;
    }
    dioxus::core::spawn_forever(async move {
        let range = if all { 0..n } else { page..page + 1 };
        let made = tokio::task::spawn_blocking(move || {
            let out = copy.export_office(format, range, page_breaks)?;
            std::fs::write(&target, &out.bytes)
                .map_err(|e| folitecta_core::Error::Invalid(format!("{}: {e}", target.display())))?;
            Ok::<_, folitecta_core::Error>((out, target))
        })
        .await;
        match made {
            Ok(Ok((out, target))) => {
                let mut detail = tf(
                    lang,
                    "office.report",
                    &[
                        ("paragraphs", &out.paragraphs.to_string()),
                        ("headings", &out.headings.to_string()),
                    ],
                );
                for (what, k) in &out.left_out {
                    detail.push('\n');
                    detail.push_str(&tf(
                        lang,
                        &format!("office.left_out.{what}"),
                        &[("count", &k.to_string())],
                    ));
                }
                st.toast(
                    ToastKind::Ok,
                    tf(
                        lang,
                        "toast.office_done",
                        &[("file", &target.display().to_string())],
                    ),
                    detail,
                );
            }
            Ok(Err(e)) => st.error(st.core_error_text(&e)),
            Err(_) => {}
        }
    });
}

handlers! {
    "document.exportOffice" => |st| doc_dialog(st, Dialog::ExportOffice),
}
