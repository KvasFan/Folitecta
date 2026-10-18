//! Interaction step 54: text recognition — a page that is only a picture of
//! its text is read with OCR Pages, after which a search finds its words; an
//! area of it read with the area tool shows its text, which is copied and
//! written on the page.

use crate::commands::ocr::JOB;

use super::*;

pub(super) async fn run(cx: &mut Ctx<'_>) {
    let st = cx.st;
    // 54. A page made a picture of itself: no text to find.
    let made = folitecta_core::Document::from_text(
        "The quick brown fox jumps over the lazy dog.\nIl gatto \u{e8} sul tavolo, perch\u{e9} fa caldo.",
        Default::default(),
    )
    .and_then(|mut d| {
        d.rasterize_pages(&[0], 150.0, false)?;
        d.clear_history();
        Ok(d)
    });
    let Ok(doc) = made else {
        cx.record("text recognition", false, "the document could not be made".into());
        return;
    };
    crate::files::open_document_tab(st, doc);
    let tab = st.read_active(|t| t.id).unwrap_or(0);
    pause(600).await;
    let before = st.read_active(|t| t.doc.page_text(0)).unwrap_or_default();

    crate::commands::dispatch(st, "document.OCRPages");
    pause(400).await;
    let _ = capture_window(&cx.dir.join("interaction-ocr-dialog.png"));
    let _ = document::eval("document.querySelector('.dialog-foot .tbtn.primary')?.click();").await;
    // Read away from the interface thread (at most 40 s, the models load first).
    let mut started = false;
    for _ in 0..200 {
        pause(200).await;
        started |= JOB.peek().running;
        let done = st.read_active(|t| t.doc.has_ocr_text(0)).unwrap_or(false);
        if done && !JOB.peek().running {
            break;
        }
    }
    let text = st.read_active(|t| t.doc.page_text(0)).unwrap_or_default();
    let found = st
        .read_active(|t| t.doc.search("lazy", Default::default()).len())
        .unwrap_or(0);
    let closed = matches!((st.dialog)(), Dialog::None);
    let _ = capture_window(&cx.dir.join("interaction-ocr-done.png"));

    // An area: the first line.
    let line = st
        .read_active(|t| {
            t.doc
                .search("quick", Default::default())
                .first()
                .map(|h| h.rects[0])
        })
        .flatten()
        .map(|r| [r[0] - 60.0, r[1] - 6.0, r[2] + 330.0, r[3] + 6.0]);
    let mut area_text = String::new();
    if let Some(rect) = line {
        crate::commands::ocr::read_area(st, 0, rect);
        for _ in 0..100 {
            pause(200).await;
            if let Dialog::OcrText { text } = (st.dialog)() {
                area_text = text;
                break;
            }
        }
    }
    pause(400).await;
    let _ = capture_window(&cx.dir.join("interaction-ocr-area.png"));
    let _ = document::eval("document.querySelector('.dialog .ocr-copy')?.click();").await;
    pause(300).await;
    let _ = document::eval("document.querySelector('.dialog .ocr-write')?.click();").await;
    pause(500).await;
    let streams = st
        .read_active(|t| t.doc.marks_on_page(0).iter().filter(|m| *m == "OCR").count())
        .unwrap_or(0);
    st.set_dialog(Dialog::None);
    crate::files::close_tab(st, tab, true, crate::state::AfterUnsaved::Close);
    pause(300).await;

    cx.record(
        "text recognition: a picture of a page read with OCR Pages and found by a search; an area read, copied and written on the page",
        before.trim().is_empty()
            && started
            && text.contains("quick brown fox")
            && text.contains("perch\u{e9}")
            && found == 1
            && closed
            && area_text.contains("quick brown fox")
            && streams == 2,
        format!(
            "before {before:?}, started {started}, text {text:?}, found {found}, closed {closed}, area {area_text:?}, streams {streams}"
        ),
    );
}
