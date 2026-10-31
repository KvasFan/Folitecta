//! Interaction step: names_text — a signature typed with letters beyond
//! WinAnsi becomes a comment of the page; one the embedded font cannot draw
//! is refused, names its characters, and adds nothing.

use folitecta_core::{Document, SignatureMark};

use super::*;

pub(super) async fn run(cx: &mut Ctx<'_>) {
    let st = cx.st;
    let Ok(doc) = Document::new_blank(1, 400.0, 500.0) else {
        cx.record("names: the document", false, String::new());
        return;
    };
    crate::files::open_document_tab(st, doc);
    let tab = st.read_active(|t| t.id);
    pause(500).await;
    let typed = |text: &str| SignatureMark::Typed {
        text: text.into(),
        color: [0.0, 0.0, 0.5],
    };
    let count = || {
        st.read_active(|t| t.doc.annotations(0).map(|a| a.len()).unwrap_or(0))
            .unwrap_or(0)
    };
    let placed = st
        .with_active(|t| {
            t.doc
                .add_signature_mark(0, [40.0, 300.0, 240.0, 360.0], &typed("Łukasz"), "Łukasz")
        })
        .is_some_and(|r| r.is_ok());
    pause(300).await;
    let after = count();
    if let Some(Err(e)) = st.with_active(|t| {
        t.doc
            .add_signature_mark(0, [40.0, 200.0, 240.0, 260.0], &typed("漢字"), "x")
    }) {
        st.core_error(&e);
    }
    pause(300).await;
    let said = st.toasts.peek().iter().any(|t| t.message.contains('漢'));
    let unchanged = count() == after;
    cx.record(
        "names: a typed signature beyond WinAnsi is placed, one that cannot be drawn is refused and named",
        placed && after == 1 && said && unchanged,
        format!("placed {placed}, comments {after}, named {said}, unchanged {unchanged}"),
    );
    if let Some(id) = tab {
        crate::files::close_tab(st, id, true, crate::state::AfterUnsaved::Close);
        pause(300).await;
    }
}
