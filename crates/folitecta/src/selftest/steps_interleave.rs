//! Interaction step: interleave — two fronts then their backs scanned from
//! the last: Interleave Fronts and Backs puts each back after its front.

use super::*;

pub(super) async fn run(cx: &mut Ctx<'_>) {
    let st = cx.st;
    let Ok(doc) = folitecta_core::Document::from_text(
        "Front one\u{c}Front two\u{c}Back two\u{c}Back one",
        Default::default(),
    ) else {
        cx.record("interleave", false, "the document could not be made".into());
        return;
    };
    crate::files::open_document_tab(st, doc);
    let tab = st.read_active(|t| t.id).unwrap_or(0);
    pause(400).await;
    crate::commands::dispatch(st, "document.interleavePages");
    pause(400).await;
    let _ = eval("document.querySelector('.dialog-foot .tbtn.primary')?.click();").await;
    pause(500).await;
    let order: Vec<String> = st
        .read_active(|t| {
            (0..t.doc.page_count())
                .map(|p| t.doc.page_text(p).trim().to_string())
                .collect()
        })
        .unwrap_or_default();
    crate::files::close_tab(st, tab, true, crate::state::AfterUnsaved::Close);
    pause(300).await;
    cx.record(
        "interleave: two fronts and their backs scanned from the last, each back put after its front",
        order == ["Front one", "Back one", "Front two", "Back two"],
        format!("order {order:?}"),
    );
}
