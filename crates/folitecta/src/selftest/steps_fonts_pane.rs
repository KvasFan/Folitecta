//! Interaction step: fonts_pane — the Fonts pane lists the fonts of the pages,
//! of forms and of comments, says which are not embedded, and a click goes to
//! the first page of a font. The document it was opened over stays on its page
//! when that tab goes.

use folitecta_core::Document;

use super::*;

pub(super) async fn run(cx: &mut Ctx<'_>) {
    let st = cx.st;
    let Ok(doc) = Document::from_bytes(folitecta_core::testdocs::fonts_doc(), None, None) else {
        cx.record("fonts pane: the document", false, String::new());
        return;
    };
    let before = st.current_page();
    let left = {
        let ui = st.ui.peek();
        (ui.show_left_pane, ui.left_pane.clone())
    };
    crate::files::open_document_tab(st, doc);
    let tab = st.read_active(|t| t.id);
    pause(600).await;
    let was_on = st.ui.peek().show_left_pane && st.ui.peek().left_pane == "fonts";
    if !was_on {
        crate::commands::dispatch(st, "view.fonts");
    }
    pause(600).await;
    let names =
        eval("return [...document.querySelectorAll('.font-row .title')].map(e => e.textContent.trim());")
            .await
            .ok()
            .and_then(|v| serde_json::from_value::<Vec<String>>(v).ok())
            .unwrap_or_default();
    let missing = eval("return document.querySelectorAll('.font-row.missing').length + document.querySelectorAll('.fonts-missing').length;")
        .await
        .ok()
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let _ = eval("[...document.querySelectorAll('.font-row')].find(e => e.textContent.includes('Elsewhere'))?.click();").await;
    pause(400).await;
    let page = st.current_page();
    cx.record(
        "fonts pane: fonts of pages, forms and comments, the one not embedded said, a click goes to its page",
        names == ["Body", "Drawn", "Elsewhere", "Helvetica"] && missing == 2 && page == 1,
        format!("names {names:?}, marked missing {missing}, page {page}"),
    );
    {
        let mut ui = st.ui_mut();
        ui.show_left_pane = left.0;
        ui.left_pane = left.1;
    }
    pause(200).await;
    if let Some(id) = tab {
        crate::files::close_tab(st, id, true, crate::state::AfterUnsaved::Close);
    }
    pause(800).await;
    let after = st.current_page();
    cx.record(
        "tabs: a document stays on its page when the tab over it closes",
        after == before,
        format!("page {before} before, {after} after"),
    );
}
