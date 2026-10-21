//! Interaction step: tables of contents — the headings of a report without
//! bookmarks are found and listed (one left out), made into nested
//! bookmarks, then listed on a contents page inserted at the start with a
//! link for every entry; Undo takes each step back.

use super::*;

fn report() -> Option<folitecta_core::Document> {
    let p = "The quick brown fox jumps over the lazy dog and runs into the woods again. ".repeat(14);
    let md = format!(
        "# Annual report\n\n{p}\n\n## Introduction\n\n{p}\n\n{p}\n\n## Results\n\n{p}\n\n### Sales\n\n{p}\n\n### Costs\n\n{p}\n"
    );
    let mut doc = folitecta_core::Document::from_markdown(&md, Default::default(), "").ok()?;
    doc.set_outline(&[]).ok()?;
    doc.clear_history();
    Some(doc)
}

async fn count(js: &str) -> i64 {
    eval(js).await.ok().and_then(|v| v.as_i64()).unwrap_or(-1)
}

pub(super) async fn run(cx: &mut Ctx<'_>) {
    let st = cx.st;
    let Some(doc) = report() else {
        cx.record(
            "tables of contents",
            false,
            "the document could not be made".into(),
        );
        return;
    };
    crate::files::open_document_tab(st, doc);
    let tab = st.read_active(|t| t.id).unwrap_or(0);
    pause(600).await;
    let pages = st.read_active(|t| t.doc.page_count()).unwrap_or(0);

    crate::commands::dispatch(st, "document.bookmarksFromHeadings");
    pause(600).await;
    let rows = count("return document.querySelectorAll('.dialog .contents-row').length;").await;
    let _ = capture_window(&cx.dir.join("interaction-headings.png"));
    // "Costs" left out.
    let _ = eval("document.querySelectorAll('.dialog .contents-row input')[4]?.click();").await;
    pause(300).await;
    let _ = eval("document.querySelector('.dialog-foot .tbtn.primary')?.click();").await;
    pause(700).await;
    let outline = st.read_active(|t| t.doc.outline()).unwrap_or_default();
    let nested = outline.len() == 1
        && outline[0].children.len() == 2
        && outline[0].children[1].children.len() == 1
        && outline[0].children[1].children[0].title == "Sales";

    crate::commands::dispatch(st, "document.insertContents");
    pause(500).await;
    let _ = eval("document.querySelector('.dialog-foot .tbtn.primary')?.click();").await;
    pause(800).await;
    let (after, text, links) = st
        .read_active(|t| {
            (
                t.doc.page_count(),
                t.doc.page_text(0),
                t.doc.links(0).map(|l| l.len()).unwrap_or(0),
            )
        })
        .unwrap_or_default();
    let _ = capture_window(&cx.dir.join("interaction-contents.png"));
    crate::commands::dispatch(st, "undo");
    pause(300).await;
    crate::commands::dispatch(st, "undo");
    pause(400).await;
    let undone = st
        .read_active(|t| (t.doc.page_count(), t.doc.outline().len()))
        .unwrap_or_default();
    crate::files::close_tab(st, tab, true, crate::state::AfterUnsaved::Close);
    pause(300).await;
    cx.record(
        "headings found and made into nested bookmarks, one left out; a contents page inserted with a link for every entry; both undone",
        rows == 5
            && nested
            && after == pages + 1
            && text.starts_with("Contents")
            && text.contains("Sales")
            && !text.contains("Costs")
            && links == 4
            && undone == (pages, 0),
        format!(
            "rows {rows}, nested {nested}, pages {pages} -> {after}, links {links}, undone {undone:?}, text {text:?}"
        ),
    );
}
