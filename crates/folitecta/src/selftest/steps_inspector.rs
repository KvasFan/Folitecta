//! Interaction step: inspector — the Object Inspector starts at the
//! trailer; /Root opens the catalog with its /Pages; Current page shows the
//! page object with its /MediaBox; Back returns.

use super::*;

async fn text(js: &str) -> String {
    eval(js)
        .await
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default()
}

async fn keys() -> String {
    text("return Array.from(document.querySelectorAll('.dialog .inspect-entry')).map(e => e.dataset.key).join(' ');").await
}

pub(super) async fn run(cx: &mut Ctx<'_>) {
    let st = cx.st;
    let Ok(doc) = folitecta_core::Document::from_text("Objects of a file", Default::default()) else {
        cx.record("inspector", false, "the document could not be made".into());
        return;
    };
    crate::files::open_document_tab(st, doc);
    let tab = st.read_active(|t| t.id).unwrap_or(0);
    pause(400).await;
    crate::commands::dispatch(st, "document.inspectObjects");
    pause(400).await;
    let trailer = keys().await;
    let _ = text("document.querySelector('.dialog .inspect-entry[data-key=\"/Root\"]')?.click(); return '';")
        .await;
    pause(300).await;
    let catalog = keys().await;
    let _ = text("document.querySelector('.dialog .inspect-page')?.click(); return '';").await;
    pause(300).await;
    let page = keys().await;
    let _ = capture_window(&cx.dir.join("interaction-inspector.png"));
    let _ = text("document.querySelector('.dialog .inspect-back')?.click(); return '';").await;
    pause(300).await;
    let back = keys().await;
    st.set_dialog(crate::state::Dialog::None);
    crate::files::close_tab(st, tab, true, crate::state::AfterUnsaved::Close);
    pause(300).await;
    cx.record(
        "object inspector: the trailer, the catalog by /Root, the current page with its /MediaBox, Back",
        trailer.contains("/Root")
            && catalog.contains("/Pages")
            && page.contains("/MediaBox")
            && back == catalog,
        format!("trailer {trailer:?}, catalog {catalog:?}, page {page:?}, back {back:?}"),
    );
}
