//! Interaction step: paste_pages — pages 1 and 3 chosen in the thumbnails and
//! copied; Paste Pages puts them after the current page (one undo step);
//! Paste Pages Special puts two copies at the end. A picture of the clipboard
//! pasted as a stamp; what the loupe showed copied as a picture.

use super::*;

pub(super) async fn run(cx: &mut Ctx<'_>) {
    let st = cx.st;
    let Ok(doc) = folitecta_core::Document::from_bytes(folitecta_core::testdocs::text_doc(3), None, None)
    else {
        cx.record("paste pages", false, "the document could not be made".into());
        return;
    };
    let texts: Vec<String> = (0..3).map(|p| doc.page_text(p)).collect();
    crate::files::open_document_tab(st, doc);
    let tab = st.read_active(|t| t.id).unwrap_or(0);
    pause(500).await;
    let state = |st: AppState| {
        st.read_active(|t| {
            (0..t.doc.page_count())
                .map(|p| t.doc.page_text(p))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
    };
    st.with_active(|t| {
        t.selected_pages = vec![0, 2];
        t.selected_annot = None;
    });
    crate::commands::dispatch(st, "copy");
    pause(500).await;
    let copied = matches!(
        st.clip.peek().clone(),
        Some(crate::state::Clip::Pages { count: 2, .. })
    );
    // After page 1: 1, 1, 3, 2, 3.
    st.go_to_page(0);
    pause(200).await;
    crate::commands::dispatch(st, "pastePages");
    pause(600).await;
    let after = state(st);
    let pasted = after.len() == 5 && after[1] == texts[0] && after[2] == texts[2] && after[3] == texts[1];
    crate::commands::dispatch(st, "undo");
    pause(400).await;
    let undone = state(st).len() == 3;
    // Two copies at the end: 1, 2, 3, 1, 3, 1, 3.
    crate::commands::dispatch(st, "pastePagesSpecial");
    pause(500).await;
    let open = matches!((st.dialog)(), crate::state::Dialog::PastePages);
    let _ = eval("const c = document.querySelector('.dialog .paste-pages-where'); if (c) { c.value = 'end'; c.dispatchEvent(new Event('change', { bubbles: true })); }").await;
    pause(150).await;
    let _ = eval("const c = document.querySelector('.dialog input[type=number]'); if (c) { c.value = '2'; c.dispatchEvent(new Event('input', { bubbles: true })); }").await;
    pause(150).await;
    let _ = eval("document.querySelector('.dialog-foot .tbtn.primary')?.click();").await;
    pause(700).await;
    let end = state(st);
    let at_end = end.len() == 7
        && end[3] == texts[0]
        && end[4] == texts[2]
        && end[5] == texts[0]
        && end[6] == texts[2];
    // A picture of 120 × 80 pixels: a stamp of 90 × 60 points in the middle.
    let mut png = std::io::Cursor::new(Vec::new());
    let _ = image::RgbaImage::from_pixel(120, 80, image::Rgba([20, 120, 220, 255]))
        .write_to(&mut png, image::ImageFormat::Png);
    let put = crate::clipboard::set_png(&png.into_inner()).is_ok();
    crate::commands::dispatch(st, "pasteAsStamp");
    pause(500).await;
    let page = st.current_page();
    let stamp = st
        .read_active(|t| {
            t.doc
                .annotations(page)
                .ok()
                .and_then(|a| a.into_iter().rev().find(|x| x.subtype == "Stamp").map(|x| x.rect))
        })
        .flatten();
    let stamped = put
        && stamp.is_some_and(|r| ((r[2] - r[0]) - 90.0).abs() < 0.5 && ((r[3] - r[1]) - 60.0).abs() < 0.5);
    // The loupe over the page, then the snapshot of what it shows.
    st.set_loupe(Some((0, 200.0, 300.0, 0.0, 0.0)));
    st.set_loupe(None);
    crate::clipboard::set_text("nothing yet");
    crate::commands::dispatch(st, "loupeView.snapshot");
    pause(500).await;
    let mag = (crate::commands::current_zoom(st) * crate::ui::canvas::loupe_factor(st)).min(8.0);
    let expected = (240.0 / mag * (mag * 2.0).min(8.0)).round() as i64;
    let shot = crate::clipboard::get_png()
        .and_then(|p| image::load_from_memory(&p).ok())
        .map(|i| (i.width() as i64, i.height() as i64));
    let loupe = shot.is_some_and(|(w, _)| (w - expected).abs() <= 2);
    // What the window shows of the current page, copied.
    crate::clipboard::set_text("nothing yet");
    crate::commands::dispatch(st, "panZoomView.snapshot");
    pause(700).await;
    let view = crate::clipboard::get_png()
        .and_then(|p| image::load_from_memory(&p).ok())
        .map(|i| (i.width(), i.height()));
    let viewed = view.is_some_and(|(w, h)| w > 100 && h > 100);
    crate::files::close_tab(st, tab, true, crate::state::AfterUnsaved::Close);
    pause(300).await;
    cx.record(
        "paste as stamp: a clipboard picture of 120 × 80 pixels becomes a stamp of 90 × 60 points; the loupe's last view and the window's view copied as pictures",
        stamped && loupe && viewed,
        format!("stamp {stamp:?}, loupe snapshot {shot:?} (width {expected} wanted), view snapshot {view:?}"),
    );
    cx.record(
        "paste pages: two pages copied from the thumbnails go after the current page in one undo step, then twice at the end",
        copied && pasted && undone && open && at_end,
        format!(
            "copied {copied}, pasted {pasted} ({} pages), undone {undone}, dialog {open}, at the end {at_end} ({} pages)",
            after.len(),
            end.len()
        ),
    );
}
