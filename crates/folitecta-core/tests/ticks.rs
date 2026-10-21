//! Tick marks: checks, crosses, dots and the texts of a sequence, plain or
//! in a circle, kept as data and drawn again when resized; text beyond
//! WinAnsi in the embedded font, text that cannot be drawn refused.

use folitecta_core::{Document, Error, FlattenScope, Sequence, TickMark, TickShape};

fn mark(shape: TickShape, text: &str) -> TickMark {
    TickMark {
        shape,
        text: text.into(),
        size: 24.0,
        color: [0.1, 0.5, 0.1],
    }
}

#[test]
fn tick_marks_are_placed_listed_and_found_after_flattening() {
    let mut doc = Document::new_blank(1, 400.0, 400.0).unwrap();
    let marks = [
        mark(TickShape::Check, ""),
        mark(TickShape::Cross, ""),
        mark(TickShape::Dot, ""),
        mark(TickShape::Circled, &Sequence::Numbers.label(12)),
        mark(TickShape::Text, &Sequence::Upper.label(28)),
    ];
    for (i, m) in marks.iter().enumerate() {
        doc.add_tick_mark(0, (60.0 + 60.0 * i as f32, 200.0), m, "t")
            .unwrap();
    }
    let annots = doc.annotations(0).unwrap();
    let texts: Vec<&str> = annots.iter().map(|a| a.contents.as_str()).collect();
    assert_eq!(texts, ["\u{2713}", "\u{2717}", "\u{2022}", "12", "AB"]);
    // Centred where they were placed, the circle wider for its text.
    let r = annots[0].rect;
    assert!((r[2] - r[0] - 24.0).abs() < 0.01, "{r:?}");
    // The mark is kept as data.
    assert_eq!(doc.tick_mark_at(0, 3), Some(marks[3].clone()));
    assert!(doc.tick_mark_at(0, 5).is_none());
    doc.flatten(&[0], FlattenScope::Comments).unwrap();
    let text = doc.page_text(0);
    assert!(text.contains("12") && text.contains("AB"), "{text}");
}

#[test]
fn a_resized_tick_mark_is_drawn_again_for_its_rectangle() {
    let mut doc = Document::new_blank(1, 400.0, 400.0).unwrap();
    doc.add_tick_mark(0, (100.0, 100.0), &mark(TickShape::Circled, "7"), "t")
        .unwrap();
    let before = doc.annotations(0).unwrap()[0].rect;
    doc.set_annotation_rect(0, 0, [50.0, 50.0, 150.0, 150.0]).unwrap();
    let after = doc.annotations(0).unwrap()[0].rect;
    assert_ne!(before, after);
    assert_eq!(doc.tick_mark_at(0, 0).unwrap().text, "7");
    doc.flatten(&[0], FlattenScope::Comments).unwrap();
    assert!(doc.page_text(0).contains('7'));
}

#[test]
fn tick_texts_beyond_winansi_and_refusals() {
    let mut doc = Document::new_blank(1, 400.0, 400.0).unwrap();
    doc.add_tick_mark(0, (100.0, 100.0), &mark(TickShape::Circled, "Ω"), "t")
        .unwrap();
    doc.flatten(&[0], FlattenScope::Comments).unwrap();
    assert!(doc.page_text(0).contains('Ω'), "{}", doc.page_text(0));
    let mut doc = Document::new_blank(1, 400.0, 400.0).unwrap();
    let err = doc
        .add_tick_mark(0, (100.0, 100.0), &mark(TickShape::Text, "漢"), "t")
        .unwrap_err();
    assert!(matches!(err, Error::Glyphs(_)), "{err:?}");
    assert!(
        doc.add_tick_mark(0, (100.0, 100.0), &mark(TickShape::Text, "  "), "t")
            .is_err()
    );
    assert!(doc.annotations(0).unwrap().is_empty());
    assert!(!doc.can_undo());
}
