//! Interaction step: duplicates — a document whose two pages each carry
//! their own copy of the same fonts and picture: Merge Duplicate Fonts and
//! Pictures keeps one of each, the pages read as before, Undo brings the
//! copies back.

use super::*;

fn pictures(st: AppState) -> usize {
    st.read_active(|t| {
        t.doc
            .inner()
            .objects
            .values()
            .filter(|o| {
                o.as_stream().is_ok_and(|s| {
                    s.dict.get(b"Subtype").ok().and_then(|n| n.as_name().ok()) == Some(b"Image")
                })
            })
            .count()
    })
    .unwrap_or(0)
}

fn fonts(st: AppState) -> usize {
    st.read_active(|t| {
        t.doc
            .inner()
            .objects
            .values()
            .filter(|o| {
                o.as_dict()
                    .is_ok_and(|d| d.get(b"Type").ok().and_then(|n| n.as_name().ok()) == Some(b"Font"))
            })
            .count()
    })
    .unwrap_or(0)
}

pub(super) async fn run(cx: &mut Ctx<'_>) {
    let st = cx.st;
    let Ok(doc) = folitecta_core::Document::from_bytes(folitecta_core::testdocs::repeated_doc(), None, None)
    else {
        cx.record("duplicates", false, "the document could not be made".into());
        return;
    };
    let text_before: Vec<String> = (0..2).map(|p| doc.page_text(p)).collect();
    crate::files::open_document_tab(st, doc);
    let tab = st.read_active(|t| t.id).unwrap_or(0);
    pause(400).await;
    let before = (pictures(st), fonts(st));
    crate::commands::dispatch(st, "document.mergeDuplicates");
    pause(400).await;
    let after = (pictures(st), fonts(st));
    let text_after: Vec<String> = st
        .read_active(|t| (0..2).map(|p| t.doc.page_text(p)).collect())
        .unwrap_or_default();
    crate::commands::dispatch(st, "undo");
    pause(400).await;
    let undone = (pictures(st), fonts(st));
    crate::files::close_tab(st, tab, true, crate::state::AfterUnsaved::Close);
    pause(300).await;
    cx.record(
        "merge duplicates: the second copy of each font and picture goes, the pages read as before, Undo brings the copies back",
        before == (2, 8) && after == (1, 4) && text_after == text_before && undone == before,
        format!("pictures and fonts before {before:?}, after {after:?}, undone {undone:?}, text kept {}", text_after == text_before),
    );
}
