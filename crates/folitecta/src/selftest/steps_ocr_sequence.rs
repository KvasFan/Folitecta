//! Interaction step: ocr_sequence — a step of action sequences recognizes the
//! text of a page that is a picture, and the text can be found.

use super::*;

pub(super) async fn run(cx: &mut Ctx<'_>) {
    let st = cx.st;
    let made = folitecta_core::Document::from_text(
        "Invoice number 2026\nThe quick brown fox jumps over the lazy dog.",
        folitecta_core::TextLayout::default(),
    )
    .and_then(|mut d| {
        d.rasterize_pages(&[0], 150.0, false)?;
        d.clear_history();
        Ok(d)
    });
    let Ok(doc) = made else {
        cx.record("recognition in a sequence: the document", false, String::new());
        return;
    };
    crate::files::open_document_tab(st, doc);
    let tab = st.read_active(|t| t.id).unwrap_or(0);
    pause(500).await;
    crate::commands::dispatch(st, "tools.actionSequences");
    pause(400).await;
    // Only the recognition: the steps there are taken away first.
    for _ in 0..10 {
        if eval("const b = document.querySelector('.dialog .actions-step .actions-remove'); if (b) { b.click(); return true; } return false;")
            .await
            .ok()
            .and_then(|v| v.as_bool())
            != Some(true)
        {
            break;
        }
        pause(150).await;
    }
    let _ = eval(
        "const s = document.querySelector('.dialog .actions-add-step'); s.value = 'ocr'; s.dispatchEvent(new Event('change', { bubbles: true }));",
    )
    .await;
    pause(200).await;
    let _ = eval("document.querySelector('.dialog .actions-add-btn')?.click();").await;
    pause(300).await;
    let _ = eval("document.querySelector('.dialog .actions-run-here')?.click();").await;
    let mut text = String::new();
    for _ in 0..100 {
        pause(200).await;
        text = st
            .read_active(|t| t.doc.page_text(0))
            .unwrap_or_default()
            .to_lowercase();
        if text.contains("quick") {
            break;
        }
    }
    st.set_dialog(crate::state::Dialog::None);
    crate::files::close_tab(st, tab, true, crate::state::AfterUnsaved::Close);
    pause(300).await;
    cx.record(
        "recognition in a sequence: the text of a page that is a picture is read",
        text.contains("quick") && text.contains("invoice"),
        format!("{text:?}"),
    );
}
