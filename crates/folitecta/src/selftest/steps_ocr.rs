//! Interaction step 54: text recognition — a page that is only a picture of
//! its text is read with OCR Pages, after which a search finds its words; an
//! area of it read with the area tool shows its text, which is copied and
//! written on the page. The words read with doubt are reviewed one by one:
//! one corrected with Enter, one kept, one skipped and then taken away.

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
    let _ = eval("document.querySelector('.dialog-foot .tbtn.primary')?.click();").await;
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
    let _ = eval("document.querySelector('.dialog .ocr-copy')?.click();").await;
    pause(300).await;
    let _ = eval("document.querySelector('.dialog .ocr-write')?.click();").await;
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
    review(cx).await;
}

/// A picture of a page with three words written as read with doubt where
/// the picture shows them.
fn doubtful_document() -> Option<folitecta_core::Document> {
    use folitecta_core::{Document, OcrLine, OcrWord, PageOcr};
    let original =
        Document::from_text("The quick brown fox jumps over the lazy dog.", Default::default()).ok()?;
    let scale = 2.0;
    let word = |find: &str, text: &str, confidence: f32| -> Option<OcrWord> {
        let r = original.search(find, Default::default()).first()?.rects[0];
        let (x0, y0, x1, y1) = (r[0] * scale, r[1] * scale, r[2] * scale, r[3] * scale);
        Some(OcrWord {
            text: text.into(),
            quad: [[x0, y0], [x1, y0], [x1, y1], [x0, y1]],
            confidence,
        })
    };
    let words = vec![
        word("The", "The", 0.97)?,
        word("quick", "qiuck", 0.52)?,
        word("brown", "brovvn", 0.61)?,
        word("lazy", "1azy", 0.58)?,
    ];
    let line = OcrLine {
        text: "The qiuck brovvn 1azy".into(),
        quad: words[0].quad,
        words,
        confidence: 0.7,
    };
    let mut doc = Document::from_bytes(original.bytes().to_vec(), None, None).ok()?;
    doc.rasterize_pages(&[0], 150.0, false).ok()?;
    doc.add_ocr_text(&[PageOcr {
        page: 0,
        scale,
        lines: vec![line],
    }])
    .ok()?;
    doc.clear_history();
    Some(doc)
}

/// What the review dialog shows: the text field, the start of the picture, the status line.
async fn review_state() -> (String, String, String) {
    let field = eval("return document.querySelector('.dialog .ocr-review-text')?.value || '';").await;
    let picture = eval("return document.querySelector('.dialog .ocr-review-picture img')?.getAttribute('src')?.slice(0, 22) || '';").await;
    let status =
        eval("return document.querySelector('.dialog .ocr-review-status')?.textContent || '';").await;
    let text = |v: Result<serde_json::Value, _>| {
        v.ok()
            .and_then(|v| v.as_str().map(str::to_string))
            .unwrap_or_default()
    };
    (text(field), text(picture), text(status))
}

/// The review of the words read with doubt.
async fn review(cx: &mut Ctx<'_>) {
    let st = cx.st;
    let Some(doc) = doubtful_document() else {
        cx.record(
            "text recognition review",
            false,
            "the document could not be made".into(),
        );
        return;
    };
    crate::files::open_document_tab(st, doc);
    let tab = st.read_active(|t| t.id).unwrap_or(0);
    pause(600).await;
    let doubtful = || {
        crate::commands::ocr::doubtful_words(st)
            .into_iter()
            .map(|w| w.text)
            .collect::<Vec<_>>()
    };
    let at_start = doubtful();
    crate::commands::dispatch(st, "document.ocrReview");
    pause(900).await;
    let (first, picture, _) = review_state().await;
    let flashed = st.read_active(|t| t.flash.is_some()).unwrap_or(false);
    let _ = capture_window(&cx.dir.join("interaction-ocr-review.png"));
    // Corrected with Enter in the text field.
    let _ = eval(
        "const a = document.querySelector('.dialog .ocr-review-text'); if (a) { a.focus(); a.value = 'quick'; a.dispatchEvent(new Event('input', { bubbles: true })); }",
    )
    .await;
    pause(300).await;
    let _ = eval(
        "document.querySelector('.dialog .ocr-review-text')?.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));",
    )
    .await;
    pause(900).await;
    let after_correct = doubtful();
    let (second, _, _) = review_state().await;
    // Kept as read, then the last one skipped.
    let _ = eval("document.querySelector('.dialog .ocr-review-keep')?.click();").await;
    pause(700).await;
    let after_keep = doubtful();
    let _ = eval("document.querySelector('.dialog .ocr-review-skip')?.click();").await;
    pause(700).await;
    let (_, _, skipped_status) = review_state().await;
    let still_open = matches!((st.dialog)(), Dialog::OcrReview { skipped: 1 });
    // Shown again, and taken away.
    st.set_dialog(Dialog::None);
    pause(300).await;
    crate::commands::dispatch(st, "document.ocrReview");
    pause(900).await;
    let (again, _, _) = review_state().await;
    let _ = eval("document.querySelector('.dialog .ocr-review-remove')?.click();").await;
    pause(700).await;
    let (_, _, none_status) = review_state().await;
    let text = st.read_active(|t| t.doc.page_text(0)).unwrap_or_default();
    st.set_dialog(Dialog::None);
    crate::files::close_tab(st, tab, true, crate::state::AfterUnsaved::Close);
    pause(300).await;
    cx.record(
        "text recognition review: a doubtful word corrected with Enter, one kept, one skipped, shown again and taken away",
        at_start == ["qiuck", "brovvn", "1azy"]
            && first == "qiuck"
            && picture == "data:image/png;base64,"
            && flashed
            && after_correct == ["brovvn", "1azy"]
            && second == "brovvn"
            && after_keep == ["1azy"]
            && still_open
            && !skipped_status.is_empty()
            && again == "1azy"
            && !none_status.is_empty()
            && text.contains("The quick brovvn")
            && !text.contains("1azy"),
        format!(
            "start {at_start:?}, first {first:?}, picture {picture:?}, flashed {flashed}, after correct {after_correct:?}, second {second:?}, after keep {after_keep:?}, skipped {still_open} {skipped_status:?}, again {again:?}, none {none_status:?}, text {text:?}"
        ),
    );
}
