//! Interaction step: Clean Up Scans — a document of scanned sheets (a
//! letter, a blank side, a letter fed askew, and a page that is not a scan)
//! is looked at; the blank and the askew pages are listed with their
//! pictures, a stricter threshold leaves the askew one out, and the clean-up
//! deletes the blank page and straightens the other in one undo step.

use folitecta_core::imaging::{skew_angle, turn};

use crate::commands::scans::SCAN;

use super::*;

fn scanned_sheets() -> Option<folitecta_core::Document> {
    let text = |lines: usize| {
        (0..lines)
            .map(|i| format!("Line {i} of the letter: the quick brown fox jumps over the lazy dog."))
            .collect::<Vec<_>>()
            .join("\n")
    };
    let source = format!("{}\u{c}\u{c}{}\u{c}{}", text(40), text(40), text(3));
    let mut doc = folitecta_core::Document::from_text(&source, Default::default()).ok()?;
    doc.rasterize_pages(&[0, 1, 2], 100.0, false).ok()?;
    let px = doc.image_pixels(2, 0).ok()?;
    let colors = doc.page_images(2).ok()?.first()?.colors;
    let turned = turn(&px, 2.0, image::Rgba([255, 255, 255, 255]));
    doc.replace_page_image(2, 0, &turned, [1.0, 0.0, 0.0, 1.0, 0.0, 0.0], colors)
        .ok()?;
    doc.clear_history();
    Some(doc)
}

async fn rows() -> usize {
    eval("return document.querySelectorAll('.dialog .scans-row').length;")
        .await
        .ok()
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as usize
}

async fn ok_enabled() -> bool {
    eval("const b = document.querySelector('.dialog-foot .tbtn.primary'); return !!b && !b.disabled;")
        .await
        .ok()
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

pub(super) async fn run(cx: &mut Ctx<'_>) {
    let st = cx.st;
    let Some(doc) = scanned_sheets() else {
        cx.record("clean up scans", false, "the document could not be made".into());
        return;
    };
    crate::files::open_document_tab(st, doc);
    let tab = st.read_active(|t| t.id).unwrap_or(0);
    pause(600).await;

    crate::commands::dispatch(st, "document.cleanUpScans");
    for _ in 0..100 {
        pause(200).await;
        if !SCAN.peek().running {
            break;
        }
    }
    pause(400).await;
    let looked = SCAN.peek().looks.len();
    let listed = rows().await;
    let thumbs = eval("return [...document.querySelectorAll('.dialog .scans-thumb')].every(i => i.complete && i.naturalWidth > 0);")
        .await
        .ok()
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let can_apply = ok_enabled().await;
    let _ = capture_window(&cx.dir.join("interaction-scans.png"));
    // A stricter angle leaves the askew page out; back to the default.
    let _ = eval("const i = document.querySelector('.dialog .scans-skew'); if (i) { i.value = '5'; i.dispatchEvent(new Event('input', { bubbles: true })); }").await;
    pause(300).await;
    let strict = rows().await;
    let _ = eval("const i = document.querySelector('.dialog .scans-skew'); if (i) { i.value = '0.3'; i.dispatchEvent(new Event('input', { bubbles: true })); }").await;
    pause(300).await;
    let _ = eval("document.querySelector('.dialog-foot .tbtn.primary')?.click();").await;
    pause(900).await;
    let closed = matches!((st.dialog)(), Dialog::None);
    let pages = st.read_active(|t| t.doc.page_count()).unwrap_or(0);
    let straight = st
        .read_active(|t| t.doc.image_pixels(1, 0).ok().map(|px| skew_angle(&px)))
        .flatten()
        .unwrap_or(99.0);
    crate::commands::dispatch(st, "undo");
    pause(500).await;
    let undone = st.read_active(|t| t.doc.page_count()).unwrap_or(0);
    crate::files::close_tab(st, tab, true, crate::state::AfterUnsaved::Close);
    pause(300).await;
    cx.record(
        "clean up scans: a blank and an askew sheet found with their pictures, a stricter angle leaves one out, cleaned in one undo step",
        looked == 3
            && listed == 2
            && thumbs
            && can_apply
            && strict == 1
            && closed
            && pages == 3
            && straight.abs() < 0.3
            && undone == 4,
        format!(
            "looked at {looked}, listed {listed}, pictures {thumbs}, can apply {can_apply}, strict {strict}, closed {closed}, pages {pages}, straight {straight}, after undo {undone}"
        ),
    );
}
