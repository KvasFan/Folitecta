//! Interaction step: articles — a story running from page 1 to page 3 is
//! listed in the Articles pane with its two parts; a click on the second
//! shows page 3.

use crate::state::Pane;

use super::*;

pub(super) async fn run(cx: &mut Ctx<'_>) {
    let st = cx.st;
    let made = folitecta_core::Document::new_blank(3, 400.0, 500.0).and_then(|mut d| {
        d.add_article(
            "Story",
            &[(0, [36.0, 250.0, 364.0, 464.0]), (2, [36.0, 36.0, 364.0, 250.0])],
        )?;
        d.clear_history();
        Ok(d)
    });
    let Ok(doc) = made else {
        cx.record("articles", false, "the document could not be made".into());
        return;
    };
    let (was_open, was_pane) = {
        let ui = st.ui.peek();
        (ui.show_left_pane, ui.left_pane.clone())
    };
    crate::files::open_document_tab(st, doc);
    let tab = st.read_active(|t| t.id).unwrap_or(0);
    pause(400).await;
    crate::commands::show_pane(st, Pane::Articles);
    pause(500).await;
    let listed = eval("return [...document.querySelectorAll('.articles-list .article')].map(a => a.querySelector('.title')?.textContent.trim()).join('|');")
        .await
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default();
    let parts = eval("return document.querySelectorAll('.articles-list .article-bead').length;")
        .await
        .ok()
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let _ = eval("document.querySelectorAll('.articles-list .article-bead')[1]?.click();").await;
    pause(700).await;
    let page = st.current_page();
    {
        let mut ui = st.ui_mut();
        ui.show_left_pane = was_open;
        ui.left_pane = was_pane;
    }
    crate::files::close_tab(st, tab, true, crate::state::AfterUnsaved::Close);
    pause(300).await;
    cx.record(
        "articles: a story of two parts listed in the Articles pane; a click on the second shows its page",
        listed == "Story" && parts == 2 && page == 2,
        format!("listed {listed:?}, parts {parts}, page {page}"),
    );
}
