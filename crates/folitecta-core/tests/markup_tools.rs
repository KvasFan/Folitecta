//! Clouds, measurements, callouts, text insertion and replacement, page
//! attachments and the ink eraser.

use folitecta_core::{AnnotStyle, Document, MeasureKind, MeasureScale};

fn blank() -> Document {
    Document::new_blank(1, 400.0, 400.0).unwrap()
}

fn style() -> AnnotStyle {
    AnnotStyle {
        color: Some([0.8, 0.1, 0.1]),
        width: 1.5,
        font_size: 10.0,
        ..AnnotStyle::default()
    }
}

#[test]
fn clouds_are_polygons_with_scalloped_borders() {
    let mut doc = blank();
    doc.add_cloud(
        0,
        &[(50.0, 50.0), (250.0, 50.0), (250.0, 200.0), (50.0, 200.0)],
        &style(),
        "t",
    )
    .unwrap();
    let a = &doc.annotations(0).unwrap()[0];
    assert_eq!(a.subtype, "Polygon");
    // The bumps extend beyond the vertices.
    assert!(a.rect[0] < 45.0 && a.rect[3] > 205.0, "{:?}", a.rect);
    let _ = doc.render_page_png(0, 1.0).unwrap();
    assert!(
        doc.add_cloud(0, &[(0.0, 0.0), (1.0, 1.0)], &style(), "t")
            .is_err()
    );
}

#[test]
fn measurements_label_their_values() {
    let mut doc = blank();
    let mm = MeasureScale::default();
    // 72 pt = 25.4 mm.
    doc.add_measurement(
        0,
        MeasureKind::Distance,
        &[(100.0, 100.0), (172.0, 100.0)],
        &style(),
        &mm,
        "t",
    )
    .unwrap();
    // A 1 cm = 1 m scale and a 72 x 72 pt square: 2.54 m x 2.54 m = 6.45 m².
    let m = MeasureScale::from_ratio(1.0, "cm", 1.0, "m", 2);
    doc.add_measurement(
        0,
        MeasureKind::Area,
        &[(100.0, 200.0), (172.0, 200.0), (172.0, 272.0), (100.0, 272.0)],
        &style(),
        &m,
        "t",
    )
    .unwrap();
    doc.add_measurement(
        0,
        MeasureKind::Perimeter,
        &[(200.0, 100.0), (272.0, 100.0), (272.0, 172.0)],
        &style(),
        &mm,
        "t",
    )
    .unwrap();
    let texts: Vec<String> = doc
        .annotations(0)
        .unwrap()
        .into_iter()
        .map(|a| a.contents)
        .collect();
    assert_eq!(texts[0], "25.4 mm");
    assert_eq!(texts[1], "6.45 m\u{b2}");
    assert_eq!(texts[2], "50.8 mm");
    let list = doc.measurements();
    assert_eq!(list.len(), 3);
    assert_eq!(list[1].1, "Area");
    // Moving a measurement keeps its value; resizing updates it.
    doc.move_annotation(0, 0, 10.0, 10.0).unwrap();
    assert_eq!(doc.annotations(0).unwrap()[0].contents, "25.4 mm");
    let r = doc.annotations(0).unwrap()[0].rect;
    doc.set_annotation_rect(0, 0, [r[0], r[1], r[0] + (r[2] - r[0]) * 2.0, r[3]])
        .unwrap();
    assert_ne!(doc.annotations(0).unwrap()[0].contents, "25.4 mm");
}

#[test]
fn callouts_point_at_a_spot() {
    let mut doc = blank();
    let s = AnnotStyle {
        color: Some([0.2, 0.2, 0.2]),
        fill: Some([1.0, 1.0, 0.8]),
        line_end: "OpenArrow".into(),
        font_size: 11.0,
        ..AnnotStyle::default()
    };
    doc.add_callout(
        0,
        (50.0, 50.0),
        [150.0, 150.0, 300.0, 200.0],
        "Look here",
        &s,
        "t",
    )
    .unwrap();
    let a = &doc.annotations(0).unwrap()[0];
    assert_eq!(a.subtype, "FreeText");
    assert!(
        a.rect[0] < 50.0 && a.rect[1] < 50.0 && a.rect[2] >= 300.0,
        "rect covers the leader and the box: {:?}",
        a.rect
    );
    let back = doc.annotation_style(0, 0).unwrap();
    assert_eq!(back.line_end, "OpenArrow");
    let _ = doc.render_page_png(0, 1.0).unwrap();
}

#[test]
fn insert_and_replace_marks() {
    let mut doc = blank();
    doc.add_caret(0, (100.0, 100.0), 12.0, "missing word", [0.1, 0.4, 0.9], "t")
        .unwrap();
    let quad = [50.0, 220.0, 150.0, 220.0, 50.0, 208.0, 150.0, 208.0];
    doc.add_replacement(0, &[quad], "new text", [0.1, 0.4, 0.9], "t")
        .unwrap();
    let kinds: Vec<String> = doc
        .annotations(0)
        .unwrap()
        .into_iter()
        .map(|a| a.subtype)
        .collect();
    assert_eq!(kinds, ["Caret", "StrikeOut", "Caret"]);
    assert_eq!(doc.annotations(0).unwrap()[2].contents, "new text");
    // The replacement is one undo step.
    doc.undo().unwrap();
    assert_eq!(doc.annotations(0).unwrap().len(), 1);
}

#[test]
fn files_attached_to_a_page() {
    let mut doc = blank();
    doc.add_file_attachment(
        0,
        (300.0, 380.0),
        "notes.txt",
        b"hello".to_vec(),
        "Reviewer notes",
        "t",
    )
    .unwrap();
    let atts = doc.attachments();
    assert_eq!(atts.len(), 1, "{atts:?}");
    assert_eq!(atts[0].name, "notes.txt");
    assert_eq!(atts[0].page, Some(0));
    assert_eq!(doc.attachment_data(&atts[0]).unwrap(), b"hello");
}

#[test]
fn eraser_splits_and_removes_strokes() {
    let mut doc = blank();
    let line: Vec<(f32, f32)> = (0..=20).map(|i| (50.0 + i as f32 * 10.0, 100.0)).collect();
    doc.add_ink(0, &[line], [0.0, 0.0, 0.0], 2.0, "t").unwrap();
    doc.add_ink(
        0,
        &[vec![(50.0, 300.0), (60.0, 300.0), (70.0, 300.0)]],
        [0.0, 0.0, 0.0],
        2.0,
        "t",
    )
    .unwrap();
    // Cut the first stroke in the middle.
    let changed = doc.erase_ink(0, &[(150.0, 120.0), (150.0, 80.0)], 6.0).unwrap();
    assert_eq!(changed, 1);
    assert_eq!(doc.annotations(0).unwrap().len(), 2);
    // Wipe the second stroke completely.
    let changed = doc.erase_ink(0, &[(40.0, 300.0), (80.0, 300.0)], 6.0).unwrap();
    assert_eq!(changed, 1);
    assert_eq!(doc.annotations(0).unwrap().len(), 1);
    assert_eq!(doc.erase_ink(0, &[(390.0, 390.0)], 3.0).unwrap(), 0);
}
