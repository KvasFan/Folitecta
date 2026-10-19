//! Interaction step: redaction by content of a document whose marked page
//! draws a reused drawing (an overlay shared by two pages, with a barcode
//! drawing nested in it), with document information, an attachment, a
//! bookmark quoting the code and a hidden layer. The marks are applied
//! through the apply dialog (sanitizing on), the result is saved as a new
//! file through its check, and the file is read again.

use folitecta_core::redaction::{RedactionOptions, SanitizeOptions};
use folitecta_core::{BarcodeKind, Document, TextLayout};

use crate::i18n::t;
use crate::state::AfterUnsaved;

use super::*;

async fn count(selector: &str) -> u64 {
    eval(&format!("return document.querySelectorAll('{selector}').length;"))
        .await
        .ok()
        .and_then(|v| v.as_u64())
        .unwrap_or(0)
}

/// The document of the step, built with the public operations.
fn fixture() -> Option<Document> {
    let mut code = Document::from_text("Confidential code 4711 for Monday", TextLayout::default()).ok()?;
    code.add_barcode(
        0,
        [72.0, 140.0, 272.0, 200.0],
        BarcodeKind::Code128,
        "4711",
        false,
    )
    .ok()?;
    let mut doc = Document::from_text(
        "Page one with the plan\u{c}Page two with the plan",
        TextLayout::default(),
    )
    .ok()?;
    doc.overlay_document(&code, &[0, 1], false).ok()?;
    doc.set_info("Plan 4711", "Reviewer", "", "").ok()?;
    doc.add_attachment("notes.txt", b"notes about 4711".to_vec(), "", Some("text/plain"))
        .ok()?;
    doc.add_bookmark("Code 4711", 0, &[]).ok()?;
    let hidden = Document::from_text("Hidden layer notes", TextLayout::default()).ok()?;
    let layer = doc.import_as_layer(&hidden, &[1], false, "Notes").ok()?;
    doc.set_layer_visible(layer, false).ok()?;
    doc.clear_history();
    Some(doc)
}

/// The PDF rectangle of the first occurrence of `word` on a page.
fn word_rect(doc: &Document, page: usize, word: &str) -> Option<[f32; 4]> {
    let layer = doc.text_layer(page)?;
    let at = layer.text.find(word)?;
    let start = layer.text[..at].chars().count();
    let boxes: Vec<[f32; 4]> = (start..start + word.chars().count())
        .filter_map(|i| *layer.char_glyph.get(i)?)
        .map(|g| layer.glyphs[g].bbox)
        .collect();
    let d = [
        boxes.iter().map(|b| b[0]).fold(f32::MAX, f32::min),
        boxes.iter().map(|b| b[1]).fold(f32::MAX, f32::min),
        boxes.iter().map(|b| b[2]).fold(f32::MIN, f32::max),
        boxes.iter().map(|b| b[3]).fold(f32::MIN, f32::max),
    ];
    Some(display_rect(doc, page, d))
}

/// A display rectangle (y down) of a page in PDF space.
fn display_rect(doc: &Document, page: usize, d: [f32; 4]) -> [f32; 4] {
    let Ok(size) = doc.page_size(page) else {
        return d;
    };
    let a = folitecta_core::marks::display_to_pdf(&size, d[0], size.height - d[3]);
    let b = folitecta_core::marks::display_to_pdf(&size, d[2], size.height - d[1]);
    [a.0.min(b.0), a.1.min(b.1), a.0.max(b.0), a.1.max(b.1)]
}

pub(super) async fn run(cx: &mut Ctx<'_>) {
    let (st, dir) = (cx.st, cx.dir);
    let Some(mut doc) = fixture() else {
        cx.record(
            "redaction: the document of the step is built",
            false,
            String::new(),
        );
        return;
    };
    let marks = (|| {
        let code = word_rect(&doc, 0, "4711")?;
        let bars = display_rect(&doc, 0, [60.0, 130.0, 285.0, 210.0]);
        doc.add_redaction(0, code, "Reviewer").ok()?;
        doc.add_redaction(0, bars, "Reviewer").ok()?;
        Some(())
    })();
    if marks.is_none() {
        cx.record("redaction: the marks are drawn", false, String::new());
        return;
    }
    let saved_prefs = (st.value("redact.save_copy"), st.value("redact.sanitize"));
    st.set_value("redact.save_copy", "off");
    st.set_value("redact.sanitize", "");
    crate::files::open_document_tab(st, doc);
    pause(800).await;
    let Some(tab) = st.read_active(|t| t.id) else {
        cx.record("redaction: the document opens", false, String::new());
        return;
    };

    // The document changes while a redaction is applied: nothing is adopted.
    crate::commands::protect::apply_redactions_now(
        st,
        None,
        RedactionOptions {
            sanitize: SanitizeOptions::none(),
            ..RedactionOptions::default()
        },
        false,
    );
    let noted = st
        .with_active(|t| {
            t.doc
                .add_note(1, 300.0, 300.0, "meanwhile", "Reviewer", [1.0, 1.0, 0.0])
                .is_ok()
        })
        .unwrap_or(false);
    let mut waited = 0;
    while st.read_active(|t| t.redacting).unwrap_or(false) && waited < 100 {
        pause(100).await;
        waited += 1;
    }
    let kept = st.read_active(|t| t.doc.pending_redactions().len()).unwrap_or(0);
    let told = st
        .toasts
        .read()
        .iter()
        .any(|m| m.title == t(st.lang(), "toast.redaction_changed"));
    let _ = st.with_active(|t| t.doc.undo());
    cx.record(
        "redaction: a document changed while it is applied keeps its marks and says so",
        noted && kept == 2 && told,
        format!("note {noted}, marks {kept}, told {told}"),
    );

    // The apply dialog: what goes, the sanitizing with its counts.
    crate::commands::dispatch(st, "document.applyRedactions");
    pause(600).await;
    let open = matches!(*st.dialog.read(), Dialog::ApplyRedactions { page: None });
    let boxes = count(".dialog input[type=checkbox]").await;
    let warnings = count(".dialog .callout.warn").await;
    // The preview of what sanitizing removes arrives from another thread.
    let mut counted = false;
    for _ in 0..60 {
        let text = eval("return document.querySelector('.dialog .sanitize-checks')?.innerText || '';")
            .await
            .ok()
            .and_then(|v| v.as_str().map(str::to_string))
            .unwrap_or_default();
        if !text.is_empty() && !text.contains(&t(st.lang(), "redact.counting")) {
            counted = true;
            break;
        }
        pause(250).await;
    }
    let terms = count(".dialog .redaction-terms input[type=checkbox]").await;
    let _ = capture_window(&dir.join("interaction-redaction-dialog.png"));
    cx.record(
        "redaction: the apply dialog shows the sanitizing, its counts and the words looked for",
        open && boxes >= 6 && warnings == 0 && counted && terms >= 1,
        format!("open {open}, {boxes} check boxes, {warnings} warnings, counted {counted}, {terms} words"),
    );

    // Apply: on another thread, adopted as one step.
    let _ = eval("document.querySelector('.dialog-foot .tbtn.danger')?.click(); return true;").await;
    let mut applied = false;
    for _ in 0..100 {
        pause(200).await;
        let done = st
            .read_active(|t| t.doc.pending_redactions().is_empty() && !t.redacting)
            .unwrap_or(false);
        if done {
            applied = true;
            break;
        }
    }
    let (text0, text1, report) = st
        .read_active(|t| {
            (
                t.doc.page_text(0),
                t.doc.page_text(1),
                t.redaction.as_ref().map(|r| r.report.clone()),
            )
        })
        .unwrap_or_default();
    let report = report.unwrap_or_default();
    let content = applied
        && !text0.contains("4711")
        && text1.contains("4711")
        && report.pages_as_pictures.is_empty()
        && report.forms >= 2
        && report.paths >= 1;
    cx.record(
        "redaction: the code and its barcode leave page 1 only, the page stays text and drawings",
        content,
        format!(
            "applied {applied}, page 1 without the code {}, page 2 with it {}, pictures {:?}, forms {}, drawings {}",
            !text0.contains("4711"),
            text1.contains("4711"),
            report.pages_as_pictures,
            report.forms,
            report.paths
        ),
    );

    // Saved as a new file through the check, and read again.
    let path = dir.join("redacted.pdf");
    let check = crate::commands::protect::save_redacted_copy_to(st, tab, path.clone()).await;
    let passed = check.as_ref().is_ok_and(|c| c.passed());
    let placeholder = t(st.lang(), "redact.placeholder");
    let reread = Document::open(&path).ok().map(|d| {
        (
            d.attachments().is_empty(),
            !d.info().title.contains("4711"),
            d.outline().first().map(|i| i.title.clone()).unwrap_or_default(),
            d.layers().is_empty(),
        )
    });
    let good = reread
        .as_ref()
        .is_some_and(|(a, ti, b, l)| *a && *ti && *b == format!("Code {placeholder}") && *l);
    cx.record(
        "redaction: the checked copy has no attachment, layer or quote of the code",
        passed && good,
        format!("check {check:?}, read again {reread:?}"),
    );
    let _ = capture_window(&dir.join("interaction-redaction.png"));
    crate::files::close_tab(st, tab, true, AfterUnsaved::Close);
    st.set_value("redact.save_copy", &saved_prefs.0);
    st.set_value("redact.sanitize", &saved_prefs.1);
    pause(300).await;
}
