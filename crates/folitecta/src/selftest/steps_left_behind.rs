//! Interaction step: left_behind — Remove Cropped Content asks first, then
//! takes the text beyond the crop box out of the file, one undo step; a PDF
//! file imported among the templates of the user is listed under its name.

use folitecta_core::Document;

use super::*;

pub(super) async fn run(cx: &mut Ctx<'_>) {
    let st = cx.st;
    let Ok(doc) = Document::from_bytes(folitecta_core::testdocs::cropped_text_doc(), None, None) else {
        cx.record("left behind: the document", false, String::new());
        return;
    };
    crate::files::open_document_tab(st, doc);
    let tab = st.read_active(|t| t.id);
    pause(600).await;
    crate::commands::dispatch(st, "document.removeCroppedContent");
    pause(400).await;
    let asked = matches!(*st.dialog.peek(), Dialog::Confirm { .. });
    let _ = eval("document.querySelector('.dialog-foot .tbtn.danger, .dialog-foot .tbtn.primary')?.click();")
        .await;
    let text = || st.read_active(|t| t.doc.page_text(0)).unwrap_or_default();
    let mut after = String::new();
    for _ in 0..30 {
        pause(200).await;
        after = text();
        if !after.contains("HIDDENOUTSIDE") {
            break;
        }
    }
    let _ = st.with_active(|t| t.doc.undo());
    pause(300).await;
    let undone = text().contains("HIDDENOUTSIDE");
    cx.record(
        "left behind: Remove Cropped Content asks, then takes the hidden text out, one undo step",
        asked && after.contains("Visible") && !after.contains("HIDDENOUTSIDE") && undone,
        format!("asked {asked}, text after {after:?}, undone {undone}"),
    );
    if let Some(id) = tab {
        crate::files::close_tab(st, id, true, crate::state::AfterUnsaved::Close);
        pause(300).await;
    }

    let file = cx.dir.join("Imported form.pdf");
    let written = std::fs::write(&file, folitecta_core::testdocs::form_doc(1, 2)).is_ok();
    crate::commands::templates::import_mine_file(st, &file);
    crate::commands::templates::load_mine();
    let mut listed = false;
    for _ in 0..30 {
        pause(100).await;
        listed = crate::commands::templates::MINE
            .peek()
            .as_ref()
            .is_some_and(|l| l.iter().any(|m| m.info.name == "Imported form"));
        if listed {
            break;
        }
    }
    cx.record(
        "left behind: a PDF file imported among the templates is listed under its name",
        written && listed,
        format!("written {written}, listed {listed}"),
    );
    let path = crate::commands::templates::MINE.peek().as_ref().and_then(|l| {
        l.iter()
            .find(|m| m.info.name == "Imported form")
            .map(|m| m.path.clone())
    });
    if let Some(path) = path {
        crate::commands::templates::remove_mine(st, path);
        pause(300).await;
    }
}
