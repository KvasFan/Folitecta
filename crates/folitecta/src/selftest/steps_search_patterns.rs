//! Interaction step: search_patterns — a regular expression typed in the
//! search pane finds what it says; one that is not valid finds nothing.

use folitecta_core::{Document, TextLayout};

use super::*;

async fn type_query(query: &str) {
    let _ = eval(&format!(
        "const i = document.querySelector('.search-box input'); if (i) {{ i.value = {}; i.dispatchEvent(new Event('input', {{ bubbles: true }})); i.dispatchEvent(new KeyboardEvent('keydown', {{ key: 'Enter', bubbles: true }})); }}",
        serde_json::to_string(query).unwrap_or_default()
    ))
    .await;
    pause(600).await;
}

pub(super) async fn run(cx: &mut Ctx<'_>) {
    let st = cx.st;
    let Ok(doc) = Document::from_text(
        "Write to anna@example.org or to 12/03/2026.",
        TextLayout::default(),
    ) else {
        cx.record("search patterns: the document", false, String::new());
        return;
    };
    crate::files::open_document_tab(st, doc);
    let tab = st.read_active(|t| t.id);
    {
        let mut ui = st.ui_mut();
        ui.show_left_pane = true;
        ui.left_pane = "search".into();
    }
    st.set_value("search.regex", "1");
    pause(600).await;
    let hits = || st.read_active(|t| t.search.hits.len()).unwrap_or(0);
    type_query(r"\d{2}/\d{2}/\d{4}").await;
    let found = hits();
    type_query("(unclosed").await;
    let none = hits();
    cx.record(
        "search patterns: a regular expression finds its text, an invalid one finds nothing",
        found == 1 && none == 0,
        format!("found {found}, with an invalid expression {none}"),
    );
    type_query("").await;
    st.set_value("search.regex", "0");
    st.ui_mut().left_pane = "thumbnails".into();
    if let Some(id) = tab {
        crate::files::close_tab(st, id, true, crate::state::AfterUnsaved::Close);
        pause(300).await;
    }
}
