//! Export Tables (FOL-L009, FOL-L030): the tables found in the text of the
//! pages, those chosen in the dialog, written as a workbook (XLSX) or as
//! CSV where the user chooses, never over the document itself.

use std::path::PathBuf;
use std::sync::Mutex;

use folitecta_core::{FoundTable, TableFormat};

use crate::commands::{doc_dialog, handlers};
use crate::i18n::{t, tf};
use crate::state::{AppState, Dialog, ToastKind};

/// The answer the self-test gives to the next save window.
static ANSWER: Mutex<Option<PathBuf>> = Mutex::new(None);

/// Let the next export be written to `path` without asking (the self-test).
pub fn answer_save(path: PathBuf) {
    *ANSWER.lock().unwrap_or_else(|e| e.into_inner()) = Some(path);
}

/// The tables of the active document on its pages `all` or the current one.
pub async fn find(st: AppState, all: bool) -> Vec<FoundTable> {
    let Some((copy, page, n)) = st.read_active(|t| (t.doc.duplicate(), t.page, t.doc.page_count())) else {
        return Vec::new();
    };
    let Ok(copy) = copy else { return Vec::new() };
    let range = if all { 0..n } else { page..(page + 1).min(n) };
    tokio::task::spawn_blocking(move || copy.page_tables(range))
        .await
        .unwrap_or_default()
}

/// Export `tables` of the active document in `format`.
pub fn export(st: AppState, format: TableFormat, tables: Vec<FoundTable>) {
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
    // Not tied to the dialog, which has closed.
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
        write_export(st, format, tables, target);
    });
}

/// The names of the sheets of `tables`: the page of each.
fn sheet_names(st: AppState, tables: &[FoundTable]) -> Vec<String> {
    let lang = st.lang();
    tables
        .iter()
        .map(|t| tf(lang, "sheets.sheet", &[("page", &(t.page + 1).to_string())]))
        .collect()
}

/// Write `tables` of the active document to `target` in `format`.
pub fn write_export(st: AppState, format: TableFormat, tables: Vec<FoundTable>, target: PathBuf) {
    let lang = st.lang();
    let Some((copy, path)) = st.read_active(|t| (t.doc.duplicate(), t.doc.path().map(|p| p.to_path_buf())))
    else {
        return;
    };
    let Ok(copy) = copy else { return };
    if path.as_ref().is_some_and(|p| *p == target) {
        st.toast(ToastKind::Warn, t(lang, "toast.office_not_over"), "");
        return;
    }
    let names = sheet_names(st, &tables);
    dioxus::core::spawn_forever(async move {
        let made = tokio::task::spawn_blocking(move || {
            let out = copy.export_tables(&tables, format, &names)?;
            folitecta_core::util::write_atomic(&target, &out.bytes)
                .map_err(|e| folitecta_core::Error::Invalid(format!("{}: {e}", target.display())))?;
            Ok::<_, folitecta_core::Error>((out, target))
        })
        .await;
        match made {
            Ok(Ok((out, target))) => {
                let detail = tf(
                    lang,
                    match format {
                        TableFormat::Xlsx => "sheets.report",
                        TableFormat::Csv => "sheets.report_csv",
                    },
                    &[
                        ("tables", &out.tables.to_string()),
                        ("rows", &out.rows.to_string()),
                        ("numbers", &out.numbers.to_string()),
                    ],
                );
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
            Ok(Err(e)) => st.core_error(&e),
            Err(_) => {}
        }
    });
}

handlers! {
    "document.exportTables" => |st| {
        if st.allowed(folitecta_core::Restricted::Copy) {
            doc_dialog(st, Dialog::ExportTables);
        }
    },
}
