//! Comment styles: reading, changing and regenerating appearances.

use folitecta_core::{AnnotStyle, Document, Markup, Shape};

fn blank() -> Document {
    Document::new_blank(1, 300.0, 300.0).unwrap()
}

/// RGBA of the pixel at (x, y) in PDF points (origin bottom-left) of page 0 at 1x.
fn pixel(doc: &Document, x: f32, y: f32) -> [u8; 4] {
    let r = doc.render_page_png(0, 1.0).unwrap();
    let img = image::load_from_memory(&r.png).unwrap().to_rgba8();
    let (px, py) = (x.round() as u32, (300.0 - y).round() as u32);
    img.get_pixel(px.min(img.width() - 1), py.min(img.height() - 1)).0
}

#[test]
fn style_round_trip_and_rendering() {
    let mut doc = blank();
    doc.add_shape(
        0,
        Shape::Square,
        [50.0, 50.0, 150.0, 150.0],
        [0.0, 0.0, 0.0],
        None,
        2.0,
        "t",
        "",
    )
    .unwrap();
    let style = AnnotStyle {
        color: Some([1.0, 0.0, 0.0]),
        fill: Some([0.0, 0.0, 1.0]),
        opacity: 1.0,
        width: 4.0,
        dashed: false,
        ..AnnotStyle::default()
    };
    doc.set_annotation_style(0, 0, &style).unwrap();
    let back = doc.annotation_style(0, 0).unwrap();
    assert_eq!(back.color, Some([1.0, 0.0, 0.0]));
    assert_eq!(back.fill, Some([0.0, 0.0, 1.0]));
    assert_eq!(back.width, 4.0);
    // Centre is filled blue, the border red.
    let centre = pixel(&doc, 100.0, 100.0);
    assert!(centre[2] > 200 && centre[0] < 60, "{centre:?}");
    let edge = pixel(&doc, 51.5, 100.0);
    assert!(edge[0] > 200 && edge[2] < 80, "{edge:?}");
    // One undo step for the style change.
    doc.undo().unwrap();
    assert_eq!(doc.annotation_style(0, 0).unwrap().fill, None);
}

#[test]
fn opacity_is_drawn() {
    let mut doc = blank();
    let style = AnnotStyle {
        color: None,
        fill: Some([0.0, 0.0, 0.0]),
        opacity: 0.5,
        width: 0.0,
        ..AnnotStyle::default()
    };
    doc.add_shape_styled(0, Shape::Square, [50.0, 50.0, 150.0, 150.0], &style, "t", "")
        .unwrap();
    let p = pixel(&doc, 100.0, 100.0);
    assert!(
        p[0] > 100 && p[0] < 160,
        "half transparent black over white: {p:?}"
    );
}

#[test]
fn lines_with_endings_polylines_and_polygons() {
    let mut doc = blank();
    doc.add_line(
        0,
        (50.0, 50.0),
        (200.0, 50.0),
        [0.0, 0.0, 0.0],
        2.0,
        true,
        "t",
        "",
    )
    .unwrap();
    let s = doc.annotation_style(0, 0).unwrap();
    assert_eq!(
        (s.line_start.as_str(), s.line_end.as_str()),
        ("None", "OpenArrow")
    );
    let mut style = s.clone();
    style.line_start = "ClosedArrow".into();
    style.fill = Some([0.0, 0.6, 0.0]);
    doc.set_annotation_style(0, 0, &style).unwrap();
    let rect = doc.annotations(0).unwrap()[0].rect;
    assert!(
        rect[0] < 45.0 && rect[2] > 205.0,
        "rect makes room for the endings: {rect:?}"
    );

    let style = AnnotStyle {
        color: Some([0.0, 0.0, 1.0]),
        fill: Some([1.0, 1.0, 0.0]),
        width: 1.0,
        ..AnnotStyle::default()
    };
    doc.add_polyline(
        0,
        &[(60.0, 100.0), (120.0, 160.0), (180.0, 100.0)],
        true,
        &style,
        "t",
        "",
    )
    .unwrap();
    doc.add_polyline(
        0,
        &[(60.0, 200.0), (120.0, 260.0), (180.0, 200.0)],
        false,
        &style,
        "t",
        "",
    )
    .unwrap();
    let kinds: Vec<String> = doc
        .annotations(0)
        .unwrap()
        .into_iter()
        .map(|a| a.subtype)
        .collect();
    assert_eq!(kinds, ["Line", "Polygon", "PolyLine"]);
    // Inside the polygon is the yellow fill.
    let p = pixel(&doc, 120.0, 120.0);
    assert!(p[0] > 200 && p[1] > 200 && p[2] < 80, "{p:?}");
    assert!(
        doc.add_polyline(0, &[(0.0, 0.0), (1.0, 1.0)], true, &style, "t", "")
            .is_err()
    );
}

#[test]
fn text_boxes_wrap_and_follow_resizing() {
    let mut doc = blank();
    let text = "The quick brown fox jumps over the lazy dog and keeps running";
    doc.add_free_text(
        0,
        [20.0, 150.0, 120.0, 280.0],
        text,
        "t",
        12.0,
        [0.0, 0.0, 0.0],
        true,
    )
    .unwrap();
    let ap_lines = |doc: &Document| {
        let id = doc.annotations(0).unwrap()[0].id.unwrap();
        let d = doc.inner().get_dictionary(id).unwrap();
        let n = d
            .get(b"AP")
            .unwrap()
            .as_dict()
            .unwrap()
            .get(b"N")
            .unwrap()
            .as_reference()
            .unwrap();
        let s = doc.inner().get_object(n).unwrap().as_stream().unwrap();
        String::from_utf8_lossy(&s.content).matches(" Tm ").count()
    };
    let narrow = ap_lines(&doc);
    assert!(narrow >= 4, "wrapped into several lines: {narrow}");
    doc.set_annotation_rect(0, 0, [20.0, 150.0, 290.0, 280.0])
        .unwrap();
    let wide = ap_lines(&doc);
    assert!(wide < narrow, "wider box needs fewer lines: {wide} vs {narrow}");
    let mut style = doc.annotation_style(0, 0).unwrap();
    assert_eq!(style.font_size, 12.0);
    style.font_size = 18.0;
    style.align = 1;
    style.text_color = [0.8, 0.0, 0.0];
    doc.set_annotation_style(0, 0, &style).unwrap();
    let back = doc.annotation_style(0, 0).unwrap();
    assert_eq!(
        (back.font_size, back.align, back.text_color),
        (18.0, 1, [0.8, 0.0, 0.0])
    );
}

#[test]
fn markups_and_notes_restyle() {
    let mut doc = blank();
    let quad = [50.0, 120.0, 150.0, 120.0, 50.0, 100.0, 150.0, 100.0];
    for kind in [
        Markup::Highlight,
        Markup::Underline,
        Markup::StrikeOut,
        Markup::Squiggly,
    ] {
        doc.add_text_markup(0, kind, &[quad], [1.0, 0.8, 0.0], "t", "")
            .unwrap();
    }
    doc.add_note(0, 200.0, 250.0, "hello", "t", [1.0, 0.85, 0.2])
        .unwrap();
    for i in 0..5 {
        let mut s = doc.annotation_style(0, i).unwrap();
        s.color = Some([0.0, 0.5, 1.0]);
        s.opacity = 0.7;
        doc.set_annotation_style(0, i, &s).unwrap();
        assert_eq!(doc.annotation_style(0, i).unwrap().color, Some([0.0, 0.5, 1.0]));
    }
    // Pictures cannot be restyled except through their stamp appearance; links cannot at all.
    doc.add_uri_link(0, [10.0, 10.0, 40.0, 30.0], "https://example.org")
        .unwrap();
    assert!(doc.set_annotation_style(0, 5, &AnnotStyle::default()).is_err());
    let _ = doc.render_page_png(0, 1.0).unwrap();
}
