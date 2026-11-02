//! Redaction and the sheet beyond the crop box: text drawn outside the
//! visible page goes with the marks of its page, unless asked to stay.

use folitecta_core::Document;
use folitecta_core::redaction::RedactionMethod;
use folitecta_core::redaction::{RedactionOptions, SanitizeOptions};
use folitecta_core::testdocs::{cropped_pages_doc, cropped_text_doc};

fn cropped() -> Document {
    Document::from_bytes(cropped_text_doc(), None, None).unwrap()
}

fn options(beyond_crop: bool) -> RedactionOptions {
    RedactionOptions {
        sanitize: SanitizeOptions::none(),
        beyond_crop,
        ..RedactionOptions::default()
    }
}

#[test]
fn text_beyond_the_crop_box_goes_with_the_marks_of_its_page() {
    let mut doc = cropped();
    assert!(doc.page_text(0).contains("HIDDENOUTSIDE"), "{}", doc.page_text(0));
    assert_eq!(doc.redaction_summary(None).beyond_crop, 0);
    doc.add_redaction(0, [15.0, 95.0, 160.0, 115.0], "Reviewer")
        .unwrap();
    let summary = doc.redaction_summary(None);
    assert_eq!((summary.areas, summary.beyond_crop), (1, 1));
    let report = doc.apply_redactions_opts(None, &options(true)).unwrap();
    assert_eq!(report.areas, 1, "the strips are no marks of the person's");
    let text = doc.page_text(0);
    assert!(!text.contains("Visible"), "{text}");
    assert!(!text.contains("HIDDENOUTSIDE"), "{text}");
}

#[test]
fn asked_to_stay_it_stays() {
    let mut doc = cropped();
    doc.add_redaction(0, [15.0, 95.0, 160.0, 115.0], "Reviewer")
        .unwrap();
    doc.apply_redactions_opts(None, &options(false)).unwrap();
    let text = doc.page_text(0);
    assert!(!text.contains("Visible"), "{text}");
    assert!(text.contains("HIDDENOUTSIDE"), "{text}");
}

#[test]
fn removing_cropped_content_takes_it_out_of_the_file() {
    let mut doc = cropped();
    assert_eq!(doc.remove_cropped_content(&[0]).unwrap(), 1);
    let text = doc.page_text(0);
    assert!(text.contains("Visible"), "{text}");
    assert!(!text.contains("HIDDENOUTSIDE"), "{text}");
    let size = doc.page_size(0).unwrap();
    assert_eq!(size.media_box, size.crop_box);
    // Only the shrunk box would leave it in the content: it is gone from it.
    let bytes = doc.bytes().to_vec();
    let mut file = lopdf::Document::load_mem(&bytes).unwrap();
    file.decompress();
    let page = file.page_iter().next().unwrap();
    let content = String::from_utf8_lossy(&file.get_page_content(page)).to_string();
    assert!(content.contains("BT") || content.contains("Tj"), "{content}");
    assert!(!content.contains("HIDDENOUTSIDE"), "{content}");
    // One undo step brings all of it back.
    doc.undo().unwrap();
    assert!(doc.page_text(0).contains("HIDDENOUTSIDE"));
    assert!(!doc.can_undo());
    // Nothing to do on a page without a crop box.
    let mut plain = Document::new_blank(1, 300.0, 300.0).unwrap();
    assert_eq!(plain.remove_cropped_content(&[0]).unwrap(), 0);
}

#[test]
fn the_pages_without_marks_are_cleared_when_asked() {
    let marked = || {
        let mut doc = Document::from_bytes(cropped_pages_doc(), None, None).unwrap();
        doc.add_redaction(0, [15.0, 195.0, 160.0, 215.0], "Reviewer")
            .unwrap();
        doc
    };
    let everywhere = RedactionOptions {
        beyond_crop_everywhere: true,
        ..options(true)
    };

    let mut doc = marked();
    let summary = doc.redaction_summary(None);
    assert_eq!((summary.beyond_crop, summary.beyond_crop_elsewhere), (1, 1));
    assert_eq!(doc.redaction_summary(Some(&[0])).beyond_crop_elsewhere, 0);
    let report = doc.apply_redactions_opts(None, &everywhere).unwrap();
    assert_eq!(report.areas, 1);
    assert!(!doc.page_text(0).contains("HIDDENONE"));
    assert!(!doc.page_text(1).contains("HIDDENTWO"));
    assert!(doc.page_text(1).contains("Other words"), "{:?}", doc.page_text(1));
    // One step.
    doc.undo().unwrap();
    assert!(doc.page_text(1).contains("HIDDENTWO"));

    // Not asked for.
    let mut doc = marked();
    doc.apply_redactions_opts(None, &options(true)).unwrap();
    assert!(doc.page_text(1).contains("HIDDENTWO"));

    // Only the marks of a page applied: only that page.
    let mut doc = marked();
    doc.apply_redactions_opts(Some(&[0]), &everywhere).unwrap();
    assert!(doc.page_text(1).contains("HIDDENTWO"));

    // Pages drawn as pictures would lose their text: they stay.
    let mut doc = marked();
    let pictures = RedactionOptions {
        method: RedactionMethod::Picture,
        ..everywhere
    };
    doc.apply_redactions_opts(None, &pictures).unwrap();
    assert!(doc.page_text(1).contains("Other words"));
}
