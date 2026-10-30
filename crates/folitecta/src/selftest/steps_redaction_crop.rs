//! Interaction step: redaction_crop — a page whose sheet reaches beyond its
//! crop box, with text there: Apply Redactions names it and, left chosen,
//! removes that text with the marked words.

use folitecta_core::Document;

use super::*;

fn cropped() -> Option<Document> {
    let mut doc = Document::from_bytes(folitecta_core::testdocs::cropped_text_doc(), None, None).ok()?;
    doc.add_redaction(0, [15.0, 95.0, 160.0, 115.0], "Reviewer")
        .ok()?;
    Some(doc)
}

pub(super) async fn run(cx: &mut Ctx<'_>) {
    let st = cx.st;
    let Some(doc) = cropped() else {
        cx.record(
            "redaction beyond the crop box: the document",
            false,
            String::new(),
        );
        return;
    };
    crate::files::open_document_tab(st, doc);
    let tab = st.read_active(|t| t.id);
    pause(600).await;
    crate::commands::dispatch(st, "document.applyRedactions");
    pause(1200).await;
    let offered =
        eval("return document.querySelectorAll('.dialog .redact-beyond-crop input:checked').length;")
            .await
            .ok()
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
    let _ = eval("document.querySelector('.dialog-foot .tbtn.danger, .dialog-foot .tbtn.primary')?.click();")
        .await;
    let mut text = String::new();
    for _ in 0..30 {
        pause(300).await;
        text = st.read_active(|t| t.doc.page_text(0)).unwrap_or_default();
        if !text.contains("Visible") {
            break;
        }
    }
    // A dialog that follows the redaction (saving a copy) is left.
    st.set_dialog(Dialog::None);
    cx.record(
        "redaction beyond the crop box: offered, chosen, and the hidden text goes with the marked words",
        offered == 1 && !text.contains("Visible") && !text.contains("HIDDENOUTSIDE"),
        format!("offered {offered}, text after {text:?}"),
    );
    if let Some(id) = tab {
        crate::files::close_tab(st, id, true, crate::state::AfterUnsaved::Close);
        pause(300).await;
    }
}
