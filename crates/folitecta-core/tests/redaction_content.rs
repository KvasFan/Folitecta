use std::path::PathBuf;

use folitecta_core::digsig::{DigitalId, IdentityInfo, KeyKind, SignOptions};
use folitecta_core::form_design::{FieldProps, NewField};
use folitecta_core::marks::display_to_pdf;
use folitecta_core::redaction::{RedactionLook, RedactionMethod};
use folitecta_core::{Document, PageText, TextLayout};
use lopdf::dictionary;

mod redaction_util;

fn sample() -> Document {
    Document::from_text(
        "Secret plan for Monday\nPublic notes stay here",
        TextLayout::default(),
    )
    .unwrap()
}

fn temp(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("folitecta-redact-{name}-{}.pdf", std::process::id()))
}

/// Display bounds (y down) of the first occurrence of `word` in the text of a page.
fn word_box(layer: &PageText, word: &str) -> [f32; 4] {
    let at = layer
        .text
        .find(word)
        .unwrap_or_else(|| panic!("{word} not in {:?}", layer.text));
    let start = layer.text[..at].chars().count();
    let boxes: Vec<[f32; 4]> = (start..start + word.chars().count())
        .filter_map(|i| layer.char_glyph[i])
        .map(|g| layer.glyphs[g].bbox)
        .collect();
    [
        boxes.iter().map(|b| b[0]).fold(f32::MAX, f32::min),
        boxes.iter().map(|b| b[1]).fold(f32::MAX, f32::min),
        boxes.iter().map(|b| b[2]).fold(f32::MIN, f32::max),
        boxes.iter().map(|b| b[3]).fold(f32::MIN, f32::max),
    ]
}

/// The PDF rectangle of a display rectangle of `page`.
fn pdf_rect(doc: &Document, page: usize, d: [f32; 4]) -> [f32; 4] {
    let size = doc.page_size(page).unwrap();
    let a = display_to_pdf(&size, d[0], size.height - d[3]);
    let b = display_to_pdf(&size, d[2], size.height - d[1]);
    [a.0.min(b.0), a.1.min(b.1), a.0.max(b.0), a.1.max(b.1)]
}

/// Every stream of a saved file, decompressed when it can be.
fn streams(path: &PathBuf) -> Vec<Vec<u8>> {
    let doc = lopdf::Document::load(path).unwrap();
    doc.objects
        .values()
        .filter_map(|o| o.as_stream().ok())
        .map(|s| s.decompressed_content().unwrap_or_else(|_| s.content.clone()))
        .collect()
}

fn holds(hay: &[u8], needle: &[u8]) -> bool {
    hay.windows(needle.len()).any(|w| w == needle)
}

fn hex(text: &str) -> Vec<u8> {
    text.bytes()
        .map(|b| format!("{b:02X}"))
        .collect::<String>()
        .into_bytes()
}

#[test]
fn text_under_a_mark_leaves_the_file_and_the_rest_stays_in_place() {
    let mut doc = sample();
    let before = doc.text_layer(0).unwrap();
    let r = pdf_rect(&doc, 0, word_box(&before, "Secret"));
    doc.add_redaction(0, r, "Reviewer").unwrap();
    let report = doc.apply_redactions_with(None, RedactionMethod::Content).unwrap();
    assert_eq!(report.areas, 1);
    assert_eq!(report.glyphs, 6);
    assert!(report.pages_as_pictures.is_empty(), "{report:?}");
    let after = doc.text_layer(0).unwrap();
    assert!(!after.text.contains("Secret"), "{:?}", after.text);
    assert!(after.text.contains("plan for Monday"), "{:?}", after.text);
    assert!(after.text.contains("Public notes stay here"), "{:?}", after.text);
    // The text after the removed word did not move.
    let (b, a) = (word_box(&before, "plan"), word_box(&after, "plan"));
    assert!(
        (b[0] - a[0]).abs() < 0.5 && (b[1] - a[1]).abs() < 0.5,
        "{b:?} -> {a:?}"
    );
    // In the saved file the word is in no stream, as text or as codes.
    let path = temp("text");
    doc.save_as(&path).unwrap();
    for s in streams(&path) {
        assert!(!holds(&s, b"Secret") && !holds(&s, &hex("Secret")));
    }
    assert!(
        Document::open(&path)
            .unwrap()
            .page_text(0)
            .contains("Public notes")
    );
    let _ = std::fs::remove_file(&path);
}

#[test]
fn pixels_under_a_mark_are_covered_and_the_rest_of_the_picture_stays() {
    let mut doc = Document::new_blank(1, 400.0, 400.0).unwrap();
    let white = image::RgbImage::from_pixel(100, 100, image::Rgb([255, 255, 255]));
    let mut png = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(white)
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
    doc.add_image(0, png.into_inner(), Some([100.0, 100.0, 300.0, 300.0]))
        .unwrap();
    // The left half of the picture.
    doc.add_redaction(0, [100.0, 100.0, 200.0, 300.0], "Reviewer")
        .unwrap();
    let report = doc.apply_redactions_with(None, RedactionMethod::Content).unwrap();
    assert_eq!(report.pictures, 1, "{report:?}");
    assert!(report.pages_as_pictures.is_empty());
    let px = doc.image_pixels(0, 0).unwrap();
    assert_eq!(px.get_pixel(10, 50).0[..3], [0, 0, 0]);
    assert_eq!(px.get_pixel(90, 50).0[..3], [255, 255, 255]);
}

#[test]
fn comments_and_fields_over_a_mark_go_with_their_values() {
    let mut doc = Document::new_blank(1, 400.0, 400.0).unwrap();
    doc.add_note(0, 120.0, 140.0, "the secret is 42", "Reviewer", [1.0, 1.0, 0.0])
        .unwrap();
    doc.add_note(0, 330.0, 360.0, "keep me", "Reviewer", [1.0, 1.0, 0.0])
        .unwrap();
    let field = doc
        .add_form_field(
            0,
            NewField::Text,
            [110.0, 160.0, 190.0, 180.0],
            &FieldProps {
                name: "code".into(),
                ..FieldProps::default()
            },
        )
        .unwrap();
    doc.set_field_value(field, "classified").unwrap();
    doc.add_redaction(0, [100.0, 100.0, 200.0, 200.0], "Reviewer")
        .unwrap();
    let report = doc.apply_redactions_with(None, RedactionMethod::Content).unwrap();
    assert_eq!((report.comments, report.fields), (1, 1), "{report:?}");
    let annots = doc.annotations(0).unwrap();
    assert!(annots.iter().any(|a| a.contents == "keep me"));
    assert!(!annots.iter().any(|a| a.contents.contains("secret")));
    assert!(doc.form_fields().is_empty());
    let path = temp("fields");
    doc.save_as(&path).unwrap();
    for s in streams(&path) {
        assert!(!holds(&s, b"classified"));
    }
    let raw = std::fs::read(&path).unwrap();
    assert!(!holds(&raw, b"classified") && !holds(&raw, b"secret is 42"));
    let _ = std::fs::remove_file(&path);
}

#[test]
fn rotated_pages_get_their_covers_upright() {
    let mut doc = sample();
    doc.rotate_pages(&[0], 90).unwrap();
    let layer = doc.text_layer(0).unwrap();
    let r = pdf_rect(&doc, 0, word_box(&layer, "Monday"));
    let look = RedactionLook {
        text: "CODE-1".into(),
        ..RedactionLook::default()
    };
    doc.add_redaction_with(0, r, "Reviewer", &look).unwrap();
    let report = doc.apply_redactions_with(None, RedactionMethod::Content).unwrap();
    assert!(report.pages_as_pictures.is_empty(), "{report:?}");
    let after = doc.text_layer(0).unwrap();
    assert!(!after.text.contains("Monday"), "{:?}", after.text);
    assert!(after.text.contains("CODE-1"), "{:?}", after.text);
    assert!(after.text.contains("Secret plan for"), "{:?}", after.text);
    // The code is upright for the reader, inside the mark as shown.
    let code = word_box(&after, "CODE-1");
    let mark = word_box(&layer, "Monday");
    assert!(code[2] - code[0] > code[3] - code[1], "{code:?}");
    assert!(
        code[0] >= mark[0] - 1.0
            && code[2] <= mark[2] + 1.0
            && code[1] >= mark[1] - 1.0
            && code[3] <= mark[3] + 1.0,
        "{code:?} outside {mark:?}"
    );
}

#[test]
fn a_signed_document_is_rewritten_whole_after_redaction() {
    let id = DigitalId::create_self_signed(
        &IdentityInfo {
            name: "Reviewer".into(),
            ..IdentityInfo::default()
        },
        KeyKind::EcP256,
        1,
    )
    .unwrap();
    let signed = sample().sign(&id, &SignOptions::default()).unwrap();
    let mut doc = Document::from_bytes(signed, None, None).unwrap();
    assert!(doc.has_signatures());
    let r = pdf_rect(&doc, 0, word_box(&doc.text_layer(0).unwrap(), "Secret"));
    doc.add_redaction(0, r, "Reviewer").unwrap();
    doc.apply_redactions(None).unwrap();
    let path = temp("signed");
    doc.save_as(&path).unwrap();
    let raw = std::fs::read(&path).unwrap();
    // One revision only: the earlier one, with the word, is not kept.
    assert_eq!(raw.windows(5).filter(|w| *w == b"%%EOF").count(), 1);
    for s in streams(&path) {
        assert!(!holds(&s, b"Secret") && !holds(&s, &hex("Secret")));
    }
    let _ = std::fs::remove_file(&path);
}

#[test]
fn a_kern_after_a_removed_word_keeps_the_next_word_in_place() {
    let mut doc = redaction_util::doc_with(
        400.0,
        400.0,
        &["BT /F1 12 Tf 50 300 Td [(Secret) -250 (plan)] TJ ET"],
        |d| {
            let f = redaction_util::helvetica(d);
            dictionary! { "Font" => dictionary! { "F1" => f } }
        },
    );
    let before = doc.text_layer(0).unwrap();
    let r = pdf_rect(&doc, 0, word_box(&before, "Secret"));
    doc.add_redaction(0, r, "Reviewer").unwrap();
    let report = doc.apply_redactions_with(None, RedactionMethod::Content).unwrap();
    assert_eq!(report.glyphs, 6, "{report:?}");
    assert!(report.pages_as_pictures.is_empty(), "{report:?}");
    let after = doc.text_layer(0).unwrap();
    assert!(!after.text.contains("Secret"), "{:?}", after.text);
    let (b, a) = (word_box(&before, "plan"), word_box(&after, "plan"));
    assert!((b[0] - a[0]).abs() < 0.3, "{b:?} -> {a:?}");
}
