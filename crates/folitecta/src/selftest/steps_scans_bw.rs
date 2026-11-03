//! Interaction step: scans_bw — a scanned page cleaned up with specks
//! removed and made black and white: its picture is one bit a pixel; Undo
//! brings the grey back.

use folitecta_core::pictures::Colors;

use crate::commands::scans::SCAN;

use super::*;

pub(super) async fn run(cx: &mut Ctx<'_>) {
    let st = cx.st;
    // A page of text: too little ink would make it a blank side.
    let letter = (0..40)
        .map(|i| format!("Line {i} of a scanned letter, with words enough to be a page."))
        .collect::<Vec<_>>()
        .join("\n");
    let made = folitecta_core::Document::from_text(&letter, Default::default()).and_then(|mut d| {
        d.rasterize_pages(&[0], 100.0, false)?;
        d.clear_history();
        Ok(d)
    });
    let Ok(doc) = made else {
        cx.record(
            "scans in black and white",
            false,
            "the document could not be made".into(),
        );
        return;
    };
    let saved = (st.value("scans.despeckle"), st.value("scans.blackWhite"));
    st.set_value("scans.despeckle", "0");
    st.set_value("scans.blackWhite", "0");
    crate::files::open_document_tab(st, doc);
    let tab = st.read_active(|t| t.id).unwrap_or(0);
    pause(500).await;
    let colors = |st: AppState| {
        st.read_active(|t| {
            t.doc
                .page_images(0)
                .ok()
                .and_then(|i| i.first().map(|i| i.colors))
        })
        .flatten()
    };
    let before = colors(st);
    crate::commands::dispatch(st, "document.cleanUpScans");
    for _ in 0..100 {
        pause(200).await;
        if !SCAN.peek().running {
            break;
        }
    }
    pause(300).await;
    let _ = eval("document.querySelector('.dialog .scans-despeckle')?.click(); document.querySelector('.dialog .scans-black-white')?.click();").await;
    pause(300).await;
    let _ = eval("document.querySelector('.dialog-foot .tbtn.primary')?.click();").await;
    pause(900).await;
    let after = colors(st);
    let said: Vec<String> = st
        .toasts
        .peek()
        .iter()
        .map(|t| format!("{}: {}", t.title, t.message))
        .collect();
    crate::commands::dispatch(st, "undo");
    pause(400).await;
    let undone = colors(st);
    st.set_value("scans.despeckle", saved.0);
    st.set_value("scans.blackWhite", saved.1);
    crate::files::close_tab(st, tab, true, crate::state::AfterUnsaved::Close);
    pause(300).await;
    cx.record(
        "clean up scans: specks removed and the page made black and white in one step, undone",
        before.is_some_and(|c| c != Colors::BlackWhite)
            && after == Some(Colors::BlackWhite)
            && undone == before,
        format!("before {before:?}, after {after:?}, undone {undone:?}, messages {said:?}"),
    );
}
