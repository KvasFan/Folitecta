//! Redaction and the sheet beyond the crop box: text drawn outside the
//! visible page goes with the marks of its page, unless asked to stay.

use folitecta_core::Document;
use folitecta_core::redaction::{RedactionOptions, SanitizeOptions};
use folitecta_core::testdocs::cropped_text_doc;

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
