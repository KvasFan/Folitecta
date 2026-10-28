//! Interaction step: my_templates — File › Save as Template… saves a form
//! under a name; the gallery lists it under My templates with what it says
//! of itself; OK makes a new document from it (no file, the same fields)
//! and the template file stays as it was; the template is renamed, then
//! moved to the bin (the self-test's own).

use super::*;

async fn text(js: &str) -> String {
    eval(js)
        .await
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default()
}

async fn wait_for(js: &str) -> String {
    for _ in 0..40 {
        let t = text(js).await;
        if !t.trim().is_empty() {
            return t;
        }
        pause(150).await;
    }
    String::new()
}

async fn ok() {
    let _ = eval("document.querySelector('.dialog-foot .tbtn.primary')?.click();").await;
    pause(600).await;
}

pub(super) async fn run(cx: &mut Ctx<'_>) {
    let st = cx.st;
    let Ok(doc) = folitecta_core::Document::from_bytes(folitecta_core::testdocs::form_doc(1, 3), None, None)
    else {
        cx.record("my templates", false, "the document could not be made".into());
        return;
    };
    let fields = doc.form_fields().len();
    crate::files::open_document_tab(st, doc);
    let source_tab = st.read_active(|t| t.id).unwrap_or(0);
    pause(600).await;

    crate::commands::dispatch(st, "saveAsTemplate");
    pause(400).await;
    let _ = eval("const i = document.querySelector('.tpl-save-name'); if (i) { i.value = 'Order form'; i.dispatchEvent(new Event('input', { bubbles: true })); }").await;
    pause(200).await;
    ok().await;
    let dir = crate::commands::templates::templates_dir().unwrap_or_default();
    let files: Vec<std::path::PathBuf> = std::fs::read_dir(&dir)
        .map(|r| r.flatten().map(|e| e.path()).collect())
        .unwrap_or_default();
    let saved = files.len() == 1;
    let before = files
        .first()
        .and_then(|f| std::fs::read(f).ok())
        .unwrap_or_default();

    crate::commands::dispatch(st, "newDocTemplate");
    let card = wait_for("const c = document.querySelector('[data-template=\"mine:Order form\"]'); return c ? c.textContent : '';").await;
    let _ = eval("document.querySelector('[data-template=\"mine:Order form\"]')?.click();").await;
    pause(400).await;
    let about = text("return document.querySelector('.tpl-mine-about')?.textContent || '';").await;
    ok().await;
    pause(800).await;
    let made = st
        .read_active(|t| {
            (
                t.id != source_tab,
                t.doc.path().is_none(),
                t.doc.form_fields().len(),
            )
        })
        .unwrap_or_default();
    let unchanged = files
        .first()
        .and_then(|f| std::fs::read(f).ok())
        .unwrap_or_default()
        == before;
    cx.record(
        "my templates: a form saved as a template is listed in the gallery and makes new documents, the template unchanged",
        saved && card.contains("Order form") && about.contains(folitecta_core::VERSION) && made == (true, true, fields) && unchanged,
        format!("files {files:?}, card {card:?}, about {about:?}, new tab / no file / fields {made:?} of {fields}, unchanged {unchanged}"),
    );
    if let Some(id) = st.read_active(|t| t.id) {
        crate::files::close_tab(st, id, true, crate::state::AfterUnsaved::Close);
        pause(300).await;
    }

    // Renamed, then moved to the bin.
    crate::commands::dispatch(st, "newDocTemplate");
    let _ = wait_for(
        "const c = document.querySelector('[data-template=\"mine:Order form\"]'); return c ? 'yes' : '';",
    )
    .await;
    let _ = eval("document.querySelector('[data-template=\"mine:Order form\"]')?.click();").await;
    pause(300).await;
    let _ = eval("const i = document.querySelector('.tpl-mine-name'); if (i) { i.value = 'Orders'; i.dispatchEvent(new Event('change', { bubbles: true })); }").await;
    let renamed = wait_for(
        "const c = document.querySelector('[data-template=\"mine:Orders\"]'); return c ? 'yes' : '';",
    )
    .await
        == "yes";
    let _ = eval("document.querySelector('[data-template=\"mine:Orders\"]')?.click();").await;
    pause(300).await;
    let _ = eval("document.querySelector('.tpl-mine-remove')?.click();").await;
    pause(800).await;
    let gone = text("return document.querySelector('.tpl-mine') ? 'still' : '';")
        .await
        .is_empty();
    let left = std::fs::read_dir(&dir).map(|r| r.count()).unwrap_or(0);
    let _ = eval("document.querySelector('.dialog-foot .tbtn:not(.primary)')?.click();").await;
    pause(300).await;
    cx.record(
        "my templates: a template renamed in the gallery, then moved to the bin",
        renamed && gone && left == 0,
        format!("renamed {renamed}, group gone {gone}, files left {left}"),
    );
    crate::files::close_tab(st, source_tab, true, crate::state::AfterUnsaved::Close);
    pause(300).await;
}
