//! Interaction step: lists_out — the results of a search leave as CSV with
//! those of comments and bookmarks, the comments of a document as CSV with
//! their authors, and the files of a session that are gone are named one by
//! one.

use folitecta_core::search::{Proximity, SearchScopes};
use folitecta_core::{Document, SearchOptions, TextLayout};

use super::*;

pub(super) async fn run(cx: &mut Ctx<'_>) {
    let st = cx.st;
    let made =
        Document::from_text("The marker word on the page.", TextLayout::default()).and_then(|mut d| {
            d.add_note(0, 40.0, 700.0, "=marker in a note", "Reviewer", [1.0, 0.9, 0.2])?;
            d.add_bookmark("marker bookmark", 0, &[])?;
            Ok(d)
        });
    let Ok(doc) = made else {
        cx.record("lists: the document", false, String::new());
        return;
    };
    crate::files::open_document_tab(st, doc);
    let tab = st.read_active(|t| t.id);
    pause(600).await;
    st.with_active(|t| {
        let scopes = SearchScopes {
            comments: true,
            bookmarks: true,
            ..SearchScopes::default()
        };
        t.search.hits = t.doc.search("marker", SearchOptions::default());
        t.search.other = t
            .doc
            .search_other("marker", SearchOptions::default(), Proximity::Phrase, &scopes);
    });
    let results = crate::commands::text::search::results_csv(st).unwrap_or_default();
    cx.record(
        "lists: the results of a search as CSV, with those of comments and bookmarks",
        results.lines().count() == 4
            && results.contains("Comments")
            && results.contains("Bookmarks")
            && results.contains("Page text"),
        results.clone(),
    );
    let comments = crate::commands::markup::comments::comments_csv(st).unwrap_or_default();
    cx.record(
        "lists: the comments as CSV, what a spreadsheet would run written as text",
        comments.lines().count() == 2
            && comments.contains("Reviewer")
            && comments.contains("\"'=marker in a note\""),
        comments.clone(),
    );

    let gone = cx.dir.join("gone-from-the-session.pdf");
    crate::commands::navigation::sessions::open_here(
        st,
        vec![crate::state::SessionFile {
            path: gone.clone(),
            page: 0,
        }],
    );
    pause(300).await;
    let named = st
        .toasts
        .peek()
        .iter()
        .any(|t| t.message.contains("gone-from-the-session.pdf"));
    cx.record(
        "lists: a file of a session that is gone is named",
        named,
        gone.display().to_string(),
    );
    if let Some(id) = tab {
        crate::files::close_tab(st, id, true, crate::state::AfterUnsaved::Close);
        pause(300).await;
    }
}
