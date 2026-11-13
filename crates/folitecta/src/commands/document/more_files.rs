//! New documents from a rich text file and from the selection, and Batch
//! Link: web links made from the addresses written in several files, each
//! kept in a copy next to it.

use dioxus::prelude::*;

use crate::commands::handlers;
use crate::i18n::{t, tf};
use crate::state::{AppState, ToastKind};

/// File › New › From Rich Text: a document of the text of an RTF file.
fn new_from_rtf(st: AppState) {
    spawn(async move {
        let Some(f) = rfd::AsyncFileDialog::new()
            .add_filter("RTF", &["rtf"])
            .pick_file()
            .await
        else {
            return;
        };
        let data = f.read().await;
        let text = folitecta_core::doctools::rtf_to_text(&String::from_utf8_lossy(&data));
        if text.trim().is_empty() {
            st.toast(ToastKind::Info, t(st.lang(), "toast.text_empty"), "");
            return;
        }
        match folitecta_core::Document::from_text(&text, folitecta_core::TextLayout::default()) {
            Ok(mut doc) => {
                let stem = f
                    .path()
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_default();
                doc.set_name_hint(&stem);
                crate::files::open_document_tab(st, doc);
            }
            Err(e) => st.error(e),
        }
    });
}

/// Web links from the addresses written in the files chosen, each file kept
/// in a copy named `<name>-links.pdf`.
fn batch_links(st: AppState) {
    spawn(async move {
        let Some(files) = rfd::AsyncFileDialog::new()
            .add_filter("PDF", &["pdf", "PDF"])
            .set_title(t(st.lang(), "dialog.batch_links.choose"))
            .pick_files()
            .await
        else {
            return;
        };
        let paths: Vec<std::path::PathBuf> = files.into_iter().map(|f| f.path().to_path_buf()).collect();
        let done = tokio::task::spawn_blocking(move || batch_links_in(&paths)).await;
        let Ok((files, links, errors)) = done else { return };
        for e in errors {
            st.error(e);
        }
        st.toast(
            ToastKind::Ok,
            tf(
                st.lang(),
                "toast.batch_links",
                &[("files", &files.to_string()), ("links", &links.to_string())],
            ),
            "",
        );
    });
}

/// The work of Batch Link: files written, links made, errors.
pub fn batch_links_in(paths: &[std::path::PathBuf]) -> (usize, usize, Vec<String>) {
    let (mut files, mut links, mut errors) = (0, 0, Vec::new());
    for path in paths {
        let r = folitecta_core::Document::open(path).and_then(|mut doc| {
            let pages: Vec<usize> = (0..doc.page_count()).collect();
            let n = doc.create_links_from_urls(&pages)?;
            let stem = path
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            doc.save_as(path.with_file_name(format!("{stem}-links.pdf")))?;
            Ok(n)
        });
        match r {
            Ok(n) => {
                files += 1;
                links += n;
            }
            Err(e) => errors.push(format!("{}: {e}", path.display())),
        }
    }
    (files, links, errors)
}

handlers! {
    "newDocRTF" => new_from_rtf,
    "newDocSelection" => |st| crate::commands::dispatch(st, "document.createNewDocFromSel"),
    "link.batch" => batch_links,
}
