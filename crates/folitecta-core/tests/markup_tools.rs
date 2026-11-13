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
fn measurements_use_their_separators() {
    use folitecta_core::markup_tools::format_measure;
    assert_eq!(format_measure(1234567.5, 2, ",", "."), "1.234.567,50");
    assert_eq!(format_measure(1234.5, 1, ".", ""), "1234.5");
    assert_eq!(format_measure(-999.95, 1, ".", " "), "-1 000.0");
    assert_eq!(format_measure(12.0, 0, ",", "'"), "12");
    assert_eq!(format_measure(-0.01, 1, ".", ""), "0.0");
    let mut doc = blank();
    // 1 cm on paper = 10 m: 72 pt (2.54 cm) is 25.4 m, written 25,40 m.
    let scale = MeasureScale::from_ratio(1.0, "cm", 10.0, "m", 2).with_separators(",", ".");
    doc.add_measurement(
        0,
        MeasureKind::Distance,
        &[(100.0, 100.0), (172.0, 100.0)],
        &style(),
        &scale,
        "t",
    )
    .unwrap();
    // A long one: 7200 pt = 254 cm = 2540 m.
    doc.add_measurement(
        0,
        MeasureKind::Distance,
        &[(0.0, 50.0), (7200.0, 50.0)],
        &style(),
        &scale,
        "t",
    )
    .unwrap();
    let texts: Vec<String> = doc
        .annotations(0)
        .unwrap()
        .into_iter()
        .map(|a| a.contents)
        .collect();
    assert_eq!(texts, vec!["25,40 m", "2.540,00 m"]);
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

#[test]
fn angles_radii_and_volumes_are_measured() {
    let mut doc = blank();
    let mm = MeasureScale::default();
    // A right angle and a 45 degree one.
    doc.add_angle_measure(
        0,
        [(200.0, 100.0), (100.0, 100.0), (100.0, 200.0)],
        &style(),
        &mm,
        "t",
    )
    .unwrap();
    doc.add_angle_measure(
        0,
        [(200.0, 300.0), (100.0, 300.0), (150.0, 350.0)],
        &style(),
        &mm,
        "t",
    )
    .unwrap();
    // A circle of 36 pt radius: 12.7 mm, 25.4 mm across.
    doc.add_radius_measure(0, (300.0, 300.0), (300.0, 336.0), &style(), &mm, "t")
        .unwrap();
    // 72 x 72 pt at 1 cm = 1 m is 6.45 m²; 2 m deep: 12.90 m³.
    let m = MeasureScale::from_ratio(1.0, "cm", 1.0, "m", 2).with_separators(",", ".");
    let square = [(250.0, 50.0), (322.0, 50.0), (322.0, 122.0), (250.0, 122.0)];
    doc.add_volume_measure(0, &square, 2.0, &style(), &m, "t")
        .unwrap();
    let annots = doc.annotations(0).unwrap();
    let texts: Vec<&str> = annots.iter().map(|a| a.contents.as_str()).collect();
    assert_eq!(
        texts,
        [
            "90.0\u{b0}",
            "45.0\u{b0}",
            "R 12.7 mm \u{b7} \u{d8} 25.4 mm",
            "12,90 m\u{b3}"
        ]
    );
    // The circle is inside the comment's rectangle.
    let r = annots[2].rect;
    assert!(
        r[0] <= 264.0 && r[1] <= 264.0 && r[2] >= 336.0 && r[3] >= 336.0,
        "{r:?}"
    );
    let kinds: Vec<String> = doc.measurements().into_iter().map(|m| m.1).collect();
    assert_eq!(kinds, ["Angle", "Angle", "Radius", "Volume"]);
    // A circle without size and a volume without depth are refused, nothing added.
    assert!(
        doc.add_radius_measure(0, (10.0, 10.0), (10.0, 10.2), &style(), &mm, "t")
            .is_err()
    );
    assert!(
        doc.add_volume_measure(0, &square, 0.0, &style(), &m, "t")
            .is_err()
    );
    assert_eq!(doc.annotations(0).unwrap().len(), 4);

    // The depth of the volume changed: 3 m deep is 19.35 m³, one undo step.
    assert_eq!(doc.volume_depth(0, 3), Some(2.0));
    assert_eq!(doc.volume_depth(0, 2), None);
    doc.set_volume_depth(0, 3, 3.0).unwrap();
    assert_eq!(doc.volume_depth(0, 3), Some(3.0));
    assert_eq!(doc.annotations(0).unwrap()[3].contents, "19,35 m\u{b3}");
    assert!(doc.set_volume_depth(0, 3, -1.0).is_err());
    assert!(doc.set_volume_depth(0, 2, 3.0).is_err());
    doc.undo().unwrap();
    assert_eq!(doc.annotations(0).unwrap()[3].contents, "12,90 m\u{b3}");
}

#[test]
fn measurements_are_listed_with_their_places_and_totals() {
    use folitecta_core::{measure_parts, measure_totals};
    let mut doc = blank();
    let mm = MeasureScale::default();
    for (a, b) in [((100.0, 100.0), (172.0, 100.0)), ((100.0, 150.0), (136.0, 150.0))] {
        doc.add_measurement(0, MeasureKind::Distance, &[a, b], &style(), &mm, "t")
            .unwrap();
    }
    let list = doc.measurement_list();
    assert_eq!(list.len(), 2);
    assert_eq!(
        (list[1].page, list[1].index, list[1].kind.as_str()),
        (0, 1, "Distance")
    );
    assert!(list[0].rect[2] > list[0].rect[0]);
    // 25.4 mm and 12.7 mm.
    let totals = measure_totals(list.iter().map(|m| m.value.as_str()));
    assert_eq!(totals.len(), 1);
    assert!(
        (totals[0].0 - 38.1).abs() < 1e-9 && totals[0].1 == "mm",
        "{totals:?}"
    );
    assert_eq!(measure_parts("12,5 cm"), Some((12.5, "cm".into())));
    assert_eq!(measure_parts("1.234,5 m\u{b2}"), Some((1234.5, "m\u{b2}".into())));
    assert_eq!(measure_parts("1,234 ft"), Some((1234.0, "ft".into())));
    assert_eq!(measure_parts("45°"), Some((45.0, "°".into())));
    assert_eq!(measure_parts("abc"), None);
    // Values that cannot be read add up to nothing.
    assert!(measure_totals(["3 mm", "?"]).is_empty());
    // Different units: a total each.
    assert_eq!(
        measure_totals(["3 mm", "2 m", "1 mm"]),
        vec![(4.0, "mm".to_string()), (2.0, "m".to_string())]
    );
}

#[test]
fn a_measurement_measured_again_with_another_scale() {
    use folitecta_core::{AnnotStyle, MeasureKind, MeasureScale};
    let mut doc = Document::new_blank(1, 595.0, 842.0).unwrap();
    // 72 points: 25.4 mm at 1:1.
    doc.add_measurement(
        0,
        MeasureKind::Distance,
        &[(100.0, 100.0), (172.0, 100.0)],
        &AnnotStyle::default(),
        &MeasureScale::default(),
        "R",
    )
    .unwrap();
    let index = doc.annotations(0).unwrap().len() - 1;
    let before = doc.measurements()[0].2.clone();
    assert!(before.starts_with("25.4"), "{before}");
    // 1 mm on paper = 10 cm: 254 cm.
    let scale = MeasureScale::from_ratio(1.0, "mm", 10.0, "cm", 0);
    doc.set_measure_scale(0, index, &scale).unwrap();
    let after = doc.measurements()[0].2.clone();
    assert_eq!(after, "254 cm");
    doc.undo().unwrap();
    assert_eq!(doc.measurements()[0].2, before);
}
