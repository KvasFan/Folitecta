//! Interaction step: page_boxes — the window of the boxes of a page gives a
//! page a trim box, the view shows it on request; and a double click on a
//! paragraph of the reflow view reads aloud from there.

use folitecta_core::pages::PageBox;
use folitecta_core::{Document, TextLayout};

use super::*;

pub(super) async fn run(cx: &mut Ctx<'_>) {
    let st = cx.st;
    let Ok(doc) = Document::from_text(
        "First paragraph.\n\nSecond paragraph.\n\nThird paragraph.",
        TextLayout::default(),
    ) else {
        cx.record("page boxes: the document", false, String::new());
        return;
    };
    crate::files::open_document_tab(st, doc);
    let tab = st.read_active(|t| t.id);
    pause(600).await;

    crate::commands::dispatch(st, "document.pageBoxes");
    pause(400).await;
    let window = matches!(*st.dialog.peek(), Dialog::PageBoxes);
    let _ = eval(
        "const n = document.querySelectorAll('.dialog input[type=number]'); if (n[0]) { n[0].value = '20'; n[0].dispatchEvent(new Event('input', { bubbles: true })); }",
    )
    .await;
    pause(200).await;
    let _ = eval("document.querySelector('.dialog-foot .tbtn.primary')?.click();").await;
    pause(500).await;
    let media = st.read_active(|t| t.doc.page_boxes(0).ok()).flatten();
    let set = media.is_some_and(|b| b.trim.is_some_and(|r| (r[0] - b.media[0] - 20.0).abs() < 0.01));
    crate::commands::dispatch(st, "cropPage.showAllBoxes");
    pause(400).await;
    let shown = eval("return document.querySelectorAll('#pg-0 .page-box.trim').length;")
        .await
        .ok()
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    crate::commands::dispatch(st, "cropPage.hideAllBoxes");
    pause(400).await;
    let hidden = eval("return document.querySelectorAll('#pg-0 .page-box').length;")
        .await
        .ok()
        .and_then(|v| v.as_u64())
        .unwrap_or(9);
    let art = st
        .with_active(|t| {
            t.doc
                .set_page_box(&[0], PageBox::Art, Some([500.0, 0.0, 500.0, 0.0]))
        })
        .is_some_and(|r| r.is_err());
    cx.record(
        "page boxes: a trim box set from its window, shown on request, one that does not fit refused",
        window && set && shown == 1 && hidden == 0 && art,
        format!("window {window}, set {set}, shown {shown}, after hiding {hidden}, refused {art}"),
    );

    crate::speech::set_stand_in(true);
    crate::commands::dispatch(st, "view.reflow");
    pause(700).await;
    let _ = eval(
        "const p = [...document.querySelectorAll('#reflow p')].find(e => e.textContent.includes('Second')); if (p) p.dispatchEvent(new MouseEvent('dblclick', { bubbles: true }));",
    )
    .await;
    pause(600).await;
    let said = crate::speech::stood_in();
    crate::commands::read_aloud::stop(st);
    crate::speech::set_stand_in(false);
    crate::commands::dispatch(st, "view.reflow");
    pause(300).await;
    cx.record(
        "reading: a double click on a paragraph reads from there",
        said.first().is_some_and(|s| s.starts_with("Second")),
        format!("{said:?}"),
    );
    if let Some(id) = tab {
        crate::files::close_tab(st, id, true, crate::state::AfterUnsaved::Close);
        pause(300).await;
    }
}
