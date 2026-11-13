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
