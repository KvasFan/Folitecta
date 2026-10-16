//! Handwritten signatures and initials placed on pages.

use folitecta_core::{Document, SignatureMark};

fn png() -> Vec<u8> {
    let img = image::RgbaImage::from_fn(60, 20, |x, _| {
        image::Rgba([0, 0, 0, if x % 3 == 0 { 255 } else { 0 }])
    });
    let mut out = std::io::Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png).unwrap();
    out.into_inner()
}

#[test]
fn marks_are_placed_moved_applied_and_removed() {
    let mut doc = Document::new_blank(1, 400.0, 500.0).unwrap();
    let strokes = SignatureMark::Strokes {
        strokes: vec![
            vec![(0.0, 0.5), (0.3, 0.1), (0.6, 0.9), (1.0, 0.4)],
            vec![(0.2, 0.7)],
        ],
        color: [0.0, 0.1, 0.5],
        width: 0.04,
    };
    assert!(strokes.aspect() > 1.0);
    doc.add_signature_mark(0, [50.0, 50.0, 200.0, 100.0], &strokes, "Ann")
        .unwrap();
    doc.add_signature_mark(
        0,
        [50.0, 150.0, 250.0, 200.0],
        &SignatureMark::Typed {
            text: "Ann Example".into(),
            color: [0.0, 0.0, 0.0],
        },
        "Ann",
    )
    .unwrap();
    doc.add_signature_mark(
        0,
        [50.0, 250.0, 200.0, 300.0],
        &SignatureMark::Image(png()),
        "Ann",
    )
    .unwrap();
    assert!(
        doc.add_signature_mark(0, [0.0, 0.0, 1.0, 1.0], &strokes, "Ann")
            .is_err()
    );
    assert_eq!(doc.signature_marks().len(), 3);
    // Moving keeps the drawing (the appearance is not replaced by text).
    doc.move_annotation(0, 0, 10.0, 10.0).unwrap();
    let ap = |doc: &Document| {
        let id = doc.annotations(0).unwrap()[0].id.unwrap();
        doc.inner()
            .get_dictionary(id)
            .unwrap()
            .get(b"AP")
            .unwrap()
            .clone()
    };
    let before = ap(&doc);
    doc.move_annotation(0, 0, 5.0, 5.0).unwrap();
    assert_eq!(ap(&doc), before);
    doc.render_page_png(0, 1.0).unwrap();
    // Applying merges them into the page.
    assert_eq!(doc.apply_signature_marks().unwrap(), 3);
    assert!(doc.signature_marks().is_empty());
    assert!(doc.comments().is_empty());
    doc.undo().unwrap();
    assert_eq!(doc.remove_signature_marks().unwrap(), 3);
    assert!(doc.comments().is_empty());
}
