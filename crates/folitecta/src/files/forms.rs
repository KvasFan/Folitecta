//! Form data: exported, imported, and filled copies made from the rows of a table.

use super::*;

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
        let mut detail = String::new();
        for (i, row) in rows.iter().enumerate() {
            let copy = st.read_active(|t| t.doc.filled_copy(row));
            let path = folder.join(row_file_name(row, &stem, i + 1));
            match copy {
                Some(Ok((mut doc, _))) => match doc.save_as(&path) {
                    Ok(()) => {
                        written += 1;
                        detail = super::protected_detail(st, &doc);
                    }
                    Err(e) => return st.core_error(&e),
                },
                Some(Err(e)) => return st.core_error(&e),
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
            detail,
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
