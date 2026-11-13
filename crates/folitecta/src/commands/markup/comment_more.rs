//! More of the comments: their labels (Edit Label, Delete Label), their
//! text (Add Text), a comment on the selected text, the information of a
//! measurement copied, the lists of comment and tool styles, and the rows
//! of data of page marks (Macros › Row Data).

use dioxus::prelude::*;

use crate::commands::{handlers, no_doc};
use crate::i18n::{t, tf};
use crate::state::{AppState, Dialog, ToastKind};

/// The comment chosen: page, index, its text and label.
fn chosen(st: AppState) -> Option<(usize, usize, String, String)> {
    if no_doc(st) {
        return None;
    }
    let found = st
        .read_active(|t| {
            let (p, i) = t.selected_annot?;
            let a = t.doc.annotations(p).ok()?.into_iter().find(|a| a.index == i)?;
            Some((p, i, a.contents, a.subject))
        })
        .flatten();
    if found.is_none() {
        st.toast(ToastKind::Info, t(st.lang(), "toast.select_comment_first"), "");
    }
    found
}

fn add_text(st: AppState) {
    if let Some((page, index, contents, _)) = chosen(st) {
        st.set_dialog(Dialog::AnnotationText {
            page,
            index,
            contents,
        });
    }
}

fn edit_label(st: AppState) {
    if let Some((page, index, _, label)) = chosen(st) {
        st.set_dialog(Dialog::CommentLabel { page, index, label });
    }
}

/// Give the comment its label (empty: none), keeping it chosen.
pub fn set_label(st: AppState, page: usize, index: usize, label: &str) {
    let r = st.with_active(|t| t.doc.set_annotation_subject(page, index, label));
    st.with_active(|t| t.selected_annot = Some((page, index)));
    match r {
        Some(Ok(())) => crate::files::set_window_title(st),
        Some(Err(e)) => st.error(st.core_error_text(&e)),
        None => {}
    }
}

fn delete_label(st: AppState) {
    if let Some((page, index, _, label)) = chosen(st) {
        if label.is_empty() {
            st.toast(ToastKind::Info, t(st.lang(), "toast.no_label"), "");
        } else {
            set_label(st, page, index, "");
        }
    }
}

/// A highlight over the selected text, its note written at once.
fn comment_selection(st: AppState) {
    let page = st.read_active(|t| t.text_sel.map(|s| s.page)).flatten();
    let before = page.and_then(|p| st.read_active(|t| t.doc.annotations(p).map(|a| a.len()).unwrap_or(0)));
    crate::commands::comments::markup_selection(st, Some(folitecta_core::Markup::Highlight));
    let (Some(page), Some(before)) = (page, before) else {
        return;
    };
    let after = st
        .read_active(|t| t.doc.annotations(page).map(|a| a.len()).unwrap_or(0))
        .unwrap_or(0);
    if after > before {
        let index = after - 1;
        st.with_active(|t| t.selected_annot = Some((page, index)));
        st.set_dialog(Dialog::AnnotationText {
            page,
            index,
            contents: String::new(),
        });
    }
}

/// Measure Info › Copy: what the chosen measurement measures, its value and
/// page, as text.
fn copy_measure(st: AppState) {
    if no_doc(st) {
        return;
    }
    let found = st
        .read_active(|t| {
            let (p, i) = t.selected_annot?;
            t.doc
                .measurement_list()
                .into_iter()
                .find(|m| m.page == p && m.index == i)
        })
        .flatten();
    match found {
        Some(m) => {
            let lang = st.lang();
            let kind = crate::i18n::annot_name(lang, &m.kind);
            let text = tf(
                lang,
                "measure.info",
                &[
                    ("kind", &kind),
                    ("value", &m.value),
                    ("page", &(m.page + 1).to_string()),
                ],
            );
            crate::clipboard::set_text(&text);
            st.toast(ToastKind::Ok, t(lang, "toast.copied"), text);
        }
        None => st.toast(ToastKind::Info, t(st.lang(), "toast.select_measure_first"), ""),
    }
}

/// The rows of data and the region of page marks, as chosen in Row Data ›
/// Options, given to the macros (at the start and after each change).
pub fn apply_macro_data(st: AppState) {
    let path = st.value("macro.rowdata.file");
    let data = (!path.is_empty())
        .then(|| std::fs::read(&path).ok())
        .flatten()
        .map(|b| folitecta_core::marks::RowData::from_csv(&String::from_utf8_lossy(&b)));
    folitecta_core::marks::set_row_data(data);
    let region = st.value("macro.region");
    folitecta_core::marks::set_region((!region.trim().is_empty()).then_some(region));
}

/// The columns of the rows of data chosen.
pub fn row_columns(st: AppState) -> Vec<String> {
    let path = st.value("macro.rowdata.file");
    std::fs::read(&path)
        .map(|b| folitecta_core::marks::RowData::from_csv(&String::from_utf8_lossy(&b)).columns)
        .unwrap_or_default()
}

/// Choose the CSV file of the rows of data.
pub fn choose_rows_file(st: AppState) {
    spawn(async move {
        let Some(file) = rfd::AsyncFileDialog::new()
            .add_filter("CSV", &["csv", "txt", "tsv"])
            .pick_file()
            .await
        else {
            return;
        };
        st.set_value("macro.rowdata.file", file.path().display().to_string());
        apply_macro_data(st);
    });
}

/// Insert `{rowdata:<first column>}` (or `{rowdata:Name}`) with the name chosen.
fn row_by_name(st: AppState) {
    let first = row_columns(st).into_iter().find(|c| !c.trim().is_empty());
    match first {
        Some(name) => crate::commands::macros::insert_owned(
            st,
            format!("{{rowdata:{}}}", name.trim()),
            Some(name.trim().to_string()),
        ),
        None => crate::commands::macros::insert_owned(st, "{rowdata:Name}".into(), Some("Name".into())),
    }
}

handlers! {
    "annot.addText" => add_text,
    "annot.editLabel" => edit_label,
    "annot.deleteLabel" => delete_label,
    "textsel.comment" => comment_selection,
    "measureInfo.copy" => copy_measure,
    "annotStylesList" => |st| st.set_dialog(Dialog::StyleList(false)),
    "toolStylesList" => |st| st.set_dialog(Dialog::StyleList(true)),
    "macro.rowData.options" => |st| st.set_dialog(Dialog::RowData),
    "macro.rowIndex" => |st| crate::commands::macros::insert_owned(st, "{rowindex}".into(), None),
    "macro.rowDataIndex" => |st| crate::commands::macros::insert_owned(st, "{rowdata:1}".into(), Some("1".into())),
    "macro.rowDataName" => row_by_name,
}
