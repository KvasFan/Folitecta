//! Interaction step: bookmark_labels — a document numbered i, ii, 1, 2 with
//! a bookmark given twice: Delete Duplicates takes the second one away and
//! keeps its child, a second time it finds nothing; a contents page put at
//! the start joins the front matter as "i", the pages after it move on, and
//! its entries carry the labels; Undo takes both back.

use super::*;

fn numbered() -> Option<folitecta_core::Document> {
    let mut doc = folitecta_core::Document::new_blank(4, 595.0, 842.0).ok()?;
    doc.set_page_labels(&[(0, "r", "", 1), (2, "D", "", 1)]).ok()?;
    doc.add_bookmark("Intro", 0, &[]).ok()?;
    doc.add_bookmark("Part A", 2, &[]).ok()?;
    doc.add_bookmark("Part A", 2, &[]).ok()?;
    doc.add_bookmark("Details", 3, &[2]).ok()?;
    doc.clear_history();
    Some(doc)
}

pub(super) async fn run(cx: &mut Ctx<'_>) {
    let st = cx.st;
    let Some(doc) = numbered() else {
        cx.record(
            "bookmarks and labels",
            false,
            "the document could not be made".into(),
        );
        return;
    };
    crate::files::open_document_tab(st, doc);
    let tab = st.read_active(|t| t.id).unwrap_or(0);
    pause(500).await;

    crate::commands::dispatch(st, "bookmarksView.deleteDuplicates");
    pause(400).await;
    let titles = |st: AppState| {
        st.read_active(|t| {
            t.doc
                .outline_flat()
                .into_iter()
                .map(|(d, t, _)| (d, t))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
    };
    let cleaned = titles(st);
    let one_left = cleaned
        == [
            (0, "Intro".to_string()),
            (0, "Part A".to_string()),
            (0, "Details".to_string()),
        ];
    // Nothing more to delete: no change.
    crate::commands::dispatch(st, "bookmarksView.deleteDuplicates");
    pause(300).await;
    let again = titles(st) == cleaned;

    crate::commands::dispatch(st, "document.insertContents");
    pause(500).await;
    let _ = eval("document.querySelector('.dialog-foot .tbtn.primary')?.click();").await;
    pause(800).await;
    let (labels, text) = st
        .read_active(|t| (t.doc.page_labels(), t.doc.page_text(0)))
        .unwrap_or_default();
    let numbered = labels == ["i", "ii", "iii", "1", "2"];
    // "Intro" now on "ii", "Part A" on "1".
    let after = |title: &str| {
        text.split(title)
            .nth(1)
            .and_then(|r| r.split_whitespace().find(|w| *w != ".").map(str::to_string))
            .unwrap_or_default()
    };
    let listed = after("Intro") == "ii" && after("Part A") == "1";

    crate::commands::dispatch(st, "undo");
    pause(300).await;
    crate::commands::dispatch(st, "undo");
    pause(400).await;
    let (undone_labels, undone_count) = st
        .read_active(|t| (t.doc.page_labels(), t.doc.bookmark_count()))
        .unwrap_or_default();
    let undone = undone_labels == ["i", "ii", "1", "2"] && undone_count == 4;
    crate::files::close_tab(st, tab, true, crate::state::AfterUnsaved::Close);
    pause(300).await;
    cx.record(
        "a bookmark given twice deleted with its child kept; contents at the start numbered with the labels, the pages after moved on; both undone",
        one_left && again && numbered && listed && undone,
        format!(
            "after delete {cleaned:?}, again {again}, labels {labels:?}, contents {text:?}, undone {undone_labels:?} with {undone_count} bookmarks"
        ),
    );
}
