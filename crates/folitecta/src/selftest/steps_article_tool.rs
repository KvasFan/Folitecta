//! Interaction step: article_tool — two columns of a page drawn with the
//! Article tool, Enter, a title: a thread of two parts in that order.

use crate::state::Zoom;

use super::*;

pub(super) async fn run(cx: &mut Ctx<'_>) {
    let st = cx.st;
    let Ok(doc) = folitecta_core::Document::new_blank(1, 595.0, 842.0) else {
        cx.record("article tool", false, "the document could not be made".into());
        return;
    };
    crate::files::open_document_tab(st, doc);
    let tab = st.read_active(|t| t.id).unwrap_or(0);
    st.with_active(|t| t.zoom = Zoom::Fixed(1.0));
    pause(600).await;
    crate::commands::dispatch(st, "tool.article");
    pause(200).await;
    // The left column, then the right one.
    let _ = pointer_path(0, 1.0, &line((50.0, 80.0), (280.0, 700.0), 6), false).await;
    pause(300).await;
    let _ = pointer_path(0, 1.0, &line((310.0, 80.0), (540.0, 700.0), 6), false).await;
    pause(300).await;
    let drawn = crate::commands::articles::DRAFT.peek().len();
    // The parts drawn are shown on the page, numbered.
    let shown = eval("return [...document.querySelectorAll('#pg-0 .article-part.drawing')].map(p => p.textContent.trim()).join(',');")
        .await
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default();
    let _ = eval("document.querySelector('.app')?.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));").await;
    pause(400).await;
    let asked = matches!(*st.dialog.peek(), Dialog::ArticleTitle);
    let _ = eval(
        "const i = document.querySelector('.dialog input'); if (i) { i.value = 'Columns'; i.dispatchEvent(new Event('input', { bubbles: true })); }",
    )
    .await;
    pause(200).await;
    let _ = eval("document.querySelector('.dialog-foot .tbtn.primary')?.click();").await;
    pause(500).await;
    let articles = st.read_active(|t| t.doc.articles()).unwrap_or_default();
    let tool_after = *st.tool.peek();

    // One more part from the Articles pane, then the first part read later.
    let (was_open, was_pane) = {
        let ui = st.ui.peek();
        (ui.show_left_pane, ui.left_pane.clone())
    };
    crate::commands::show_pane(st, crate::state::Pane::Articles);
    pause(400).await;
    let _ = eval("document.querySelector('.articles-list .article-add-parts')?.click();").await;
    pause(300).await;
    let _ = pointer_path(0, 1.0, &line((50.0, 720.0), (540.0, 800.0), 6), false).await;
    pause(300).await;
    let _ = eval("document.querySelector('.app')?.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));").await;
    pause(400).await;
    let added = st
        .read_active(|t| t.doc.articles().first().map(|a| a.beads.len()))
        .flatten();
    let _ = eval("document.querySelector('.articles-list .article-part-later')?.click();").await;
    pause(400).await;
    // The right column (x 310) now first.
    let moved = st
        .read_active(|t| {
            t.doc
                .articles()
                .first()
                .and_then(|a| a.beads.first().map(|b| b.1[0]))
        })
        .flatten()
        .unwrap_or_default();
    {
        let mut ui = st.ui_mut();
        ui.show_left_pane = was_open;
        ui.left_pane = was_pane;
    }
    crate::files::close_tab(st, tab, true, crate::state::AfterUnsaved::Close);
    pause(300).await;
    // Left part first: the first bead lies left of the second.
    let ordered = articles
        .first()
        .is_some_and(|a| a.beads.len() == 2 && a.beads[0].1[0] < a.beads[1].1[0]);
    cx.record(
        "article tool: two parts drawn, Enter asks the title, the thread runs through them in order; one more added from the Articles pane, the first moved later",
        drawn == 2
            && shown == "1,2"
            && asked
            && articles.len() == 1
            && articles[0].title == "Columns"
            && ordered
            && tool_after == crate::state::Tool::Hand
            && added == Some(3)
            && (moved - 310.0).abs() < 5.0,
        format!(
            "drawn {drawn}, shown {shown:?}, asked {asked}, articles {articles:?}, tool after {tool_after:?}, parts after adding {added:?}, first part at x {moved}"
        ),
    );
}
