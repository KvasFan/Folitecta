//! Interaction step: find_fields — Find Form Fields on a flat form lists
//! its four places to write in, named by their words (the rule under a
//! heading is no place); Add Fields makes them fields in one undo step.

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
    let Ok(doc) = folitecta_core::Document::from_bytes(folitecta_core::testdocs::flat_form_doc(), None, None)
    else {
        cx.record("find form fields", false, "the document could not be made".into());
        return;
    };
    crate::files::open_document_tab(st, doc);
    let tab = st.read_active(|t| t.id).unwrap_or(0);
    pause(400).await;
    crate::commands::dispatch(st, "forms.findFields");
    let mut items = String::new();
    for _ in 0..40 {
        pause(150).await;
        items = text(
            "return Array.from(document.querySelectorAll('.dialog .find-field')).map(e => e.textContent.trim()).join('|');",
        )
        .await;
        if !items.is_empty() {
            break;
        }
    }
    let _ = text("document.querySelector('.dialog-foot .tbtn.primary')?.click(); return '';").await;
    pause(400).await;
    let names: Vec<String> = st
        .read_active(|t| t.doc.form_fields().into_iter().map(|f| f.name).collect())
        .unwrap_or_default();
    crate::commands::dispatch(st, "undo");
    pause(300).await;
    let undone = st
        .read_active(|t| t.doc.form_fields().is_empty())
        .unwrap_or(false);
    crate::files::close_tab(st, tab, true, crate::state::AfterUnsaved::Close);
    pause(300).await;
    cx.record(
        "find form fields: four places to write in listed with their words; added as fields, one undo step",
        items.split('|').count() == 4
            && items.contains("Comments")
            && names == ["Name", "Date", "I agree to the terms", "Comments"]
            && undone,
        format!("items {items:?}, fields {names:?}, undone {undone}"),
    );
}
