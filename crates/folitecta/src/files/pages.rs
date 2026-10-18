//! Pages from another PDF: replacing and overlaying pages, Bates numbering of several files.

use super::*;

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
