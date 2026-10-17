use folitecta_core::Document;
use folitecta_core::calculator::{Tape, TapeLine};

#[test]
fn calculator_comments_keep_their_tape() {
    let mut doc = Document::new_blank(1, 595.0, 842.0).unwrap();
    let mut tape = Tape::default();
    tape.lines.push(TapeLine::new('+', 120.0));
    tape.lines.push(TapeLine::new('+', 30.5));
    let id = doc
        .add_calculator(0, [100.0, 700.0, 260.0, 760.0], &tape, "Reviewer")
        .unwrap();
    let annots = doc.annotations(0).unwrap();
    assert_eq!(annots.len(), 1);
    assert_eq!(annots[0].id, Some(id));
    // The top stays where it was placed, the height follows the tape.
    let r = annots[0].rect;
    assert!((r[3] - 760.0).abs() < 0.01);
    assert!((r[3] - r[1] - tape.height()).abs() < 0.01);
    assert!(annots[0].contents.contains("150.50 T"));
    assert_eq!(doc.calculator_at(0, 0), Some(tape.clone()));
    assert_eq!(doc.calculators(), vec![(0, 0)]);

    // More lines: taller, same top.
    tape.lines.push(TapeLine::new('-', 50.0));
    tape.lines.push(TapeLine::new('=', 0.0));
    tape.show_date = true;
    tape.date = "2026-10-17 10:00".into();
    doc.set_calculator(0, 0, &tape).unwrap();
    let r2 = doc.annotations(0).unwrap()[0].rect;
    assert!((r2[3] - 760.0).abs() < 0.01);
    assert!(r2[3] - r2[1] > r[3] - r[1]);
    assert_eq!(doc.calculator_at(0, 0).unwrap().total(), 100.5);

    // Moving keeps the tape and draws it again.
    doc.move_annotation(0, 0, 20.0, -40.0).unwrap();
    assert_eq!(doc.calculator_at(0, 0), Some(tape));
    let text = doc.page_text(0);
    assert!(text.is_empty() || !text.contains("Accounting"));
    // One undo step per edit.
    doc.undo().unwrap();
    doc.undo().unwrap();
    assert_eq!(doc.calculator_at(0, 0).unwrap().lines.len(), 2);
    assert!(doc.set_calculator(0, 1, &Tape::default()).is_err());
}
