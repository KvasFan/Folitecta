//! Interaction step: sanitize — Remove Hidden Information shows what a
//! document holds of each part (its author, an attached file); removing
//! them leaves no author and no attachment, one undo step brings them back.

use super::*;

async fn text(js: &str) -> String {
    eval(js)
        .await
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default()
}

pub(super) async fn run(cx: &mut Ctx<'_>) {
    let st = cx.st;
    let made = folitecta_core::Document::new_blank(1, 400.0, 400.0).and_then(|mut d| {
        d.add_attachment("notes.txt", b"private notes".to_vec(), "", None)?;
        d.set_info("Plan", "A. Writer", "", "")?;
        d.clear_history();
        Ok(d)
    });
    let Ok(doc) = made else {
        cx.record("sanitize", false, "the document could not be made".into());
        return;
    };
    crate::files::open_document_tab(st, doc);
    let tab = st.read_active(|t| t.id).unwrap_or(0);
    pause(400).await;
    crate::commands::dispatch(st, "document.sanitize");
    pause(400).await;
    let found = text(
        "return Array.from(document.querySelectorAll('.dialog .sanitize-part')).map(e => e.dataset.part + ':' + e.querySelector('.sanitize-found').textContent).join('|');",
    )
    .await;
    let _ = text("document.querySelector('.dialog-foot .tbtn.danger')?.click(); return '';").await;
    pause(400).await;
    let gone = st
        .read_active(|t| t.doc.attachments().is_empty() && t.doc.info().author.is_empty())
        .unwrap_or(false);
    crate::commands::dispatch(st, "undo");
    pause(300).await;
    let back = st
        .read_active(|t| t.doc.attachments().len() == 1 && t.doc.info().author == "A. Writer")
        .unwrap_or(false);
    crate::files::close_tab(st, tab, true, crate::state::AfterUnsaved::Close);
    pause(300).await;
    cx.record(
        "remove hidden information: each part with what the document holds; removed, then brought back by one undo",
        found.split('|').count() == 4 && found.contains("attachments:(1") && gone && back,
        format!("found {found:?}, gone {gone}, back {back}"),
    );
}
