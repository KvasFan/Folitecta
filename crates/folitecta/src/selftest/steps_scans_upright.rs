//! Interaction step: scans_upright — three scanned sheets of a letter, the
//! second fed upside down and the third sideways, are looked at by Clean Up
//! Scans: the two are listed to be turned, the clean-up turns them upright
//! (the pictures stay as they are) in one undo step.

use crate::commands::scans::SCAN;

use super::*;

fn sheets() -> Option<folitecta_core::Document> {
    const WORDS: [&str; 24] = [
        "the", "quick", "brown", "fox", "jumps", "over", "lazy", "dogs", "while", "a", "kind", "old", "lady",
        "reads", "her", "daily", "paper", "by", "the", "bright", "window", "of", "its", "shop",
    ];
    let prose = (0..40)
        .map(|i| {
            let n = 9 + (i * 5) % 6;
            (0..n)
                .map(|k| WORDS[(i * 7 + k * 5) % WORDS.len()])
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect::<Vec<_>>()
        .join("\n");
    let source = [prose.as_str(), prose.as_str(), prose.as_str()].join("\u{c}");
    let mut doc = folitecta_core::Document::from_text(&source, Default::default()).ok()?;
    doc.rasterize_pages(&[0, 1, 2], 100.0, false).ok()?;
    for (page, turned) in [
        (
            1,
            image::imageops::rotate180 as fn(&image::RgbaImage) -> image::RgbaImage,
        ),
        (2, image::imageops::rotate90),
    ] {
        let px = turned(&doc.image_pixels(page, 0).ok()?);
        let colors = doc.page_images(page).ok()?.first()?.colors;
        doc.replace_page_image(page, 0, &px, [1.0, 0.0, 0.0, 1.0, 0.0, 0.0], colors)
            .ok()?;
    }
    doc.clear_history();
    Some(doc)
}

fn rotations(st: AppState) -> Vec<i32> {
    st.read_active(|t| t.doc.page_sizes().iter().map(|s| s.rotation).collect())
        .unwrap_or_default()
}

pub(super) async fn run(cx: &mut Ctx<'_>) {
    let st = cx.st;
    let Some(doc) = sheets() else {
        cx.record(
            "scans turned upright",
            false,
            "the document could not be made".into(),
        );
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
    let listed = eval("return document.querySelectorAll('.dialog .scans-row').length;")
        .await
        .ok()
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let offered = eval("return document.querySelectorAll('.dialog .scans-turn:checked').length;")
        .await
        .ok()
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let _ = eval("document.querySelector('.dialog-foot .tbtn.primary')?.click();").await;
    pause(900).await;
    let turned = rotations(st);
    crate::commands::dispatch(st, "undo");
    pause(500).await;
    let undone = rotations(st);
    crate::files::close_tab(st, tab, true, crate::state::AfterUnsaved::Close);
    pause(300).await;
    cx.record(
        "clean up scans: a sheet upside down and one sideways listed and turned upright in one undo step",
        listed == 2 && offered == 1 && turned == [0, 180, 270] && undone == [0, 0, 0],
        format!("listed {listed}, offered {offered}, turned {turned:?}, after undo {undone:?}"),
    );
}
