//! Tests for crop-to-content, duplicates, tiles, overlays, transitions,
//! backgrounds, page labels, single-annotation flattening and content guards.

use folitecta_core::marks::{MarkPosition, TextMarkOptions};
use folitecta_core::{Document, Shape, Sides};

fn doc_with_text() -> Document {
    let mut doc = Document::new_blank(3, 595.0, 842.0).unwrap();
    doc.add_text_watermark(
        &[0, 1, 2],
        &TextMarkOptions {
            text: "SAME".into(),
            rotation: 0.0,
            font_size: 40.0,
            opacity: 1.0,
            color: [0.0, 0.0, 0.0],
            ..Default::default()
        },
    )
    .unwrap();
    doc.add_text_watermark(
        &[2],
        &TextMarkOptions {
            text: "different".into(),
            rotation: 0.0,
            font_size: 20.0,
            opacity: 1.0,
            color: [0.0, 0.0, 0.0],
            position: MarkPosition::TopLeft,
            ..Default::default()
        },
    )
    .unwrap();
    doc.clear_history();
    doc
}

#[test]
fn crop_to_content() {
    let mut doc = doc_with_text();
    let bounds = doc.content_bounds(0).unwrap().unwrap();
    assert!(
        bounds[0] > 150.0 && bounds[2] < 450.0 && bounds[1] > 350.0 && bounds[3] < 480.0,
        "{bounds:?}"
    );
    let n = doc.crop_to_white_margins(&[0], 10.0, Sides::ALL).unwrap();
    assert_eq!(n, 1);
    let s = doc.page_size(0).unwrap();
    assert!(s.width < 300.0 && s.height < 120.0, "{s:?}");
    assert!(doc.page_text(0).contains("SAME"));
    let mut blank = Document::new_blank(1, 200.0, 200.0).unwrap();
    assert_eq!(blank.content_bounds(0).unwrap(), None);
    assert_eq!(blank.crop_to_white_margins(&[0], 0.0, Sides::ALL).unwrap(), 0);
}

#[test]
fn finds_duplicate_pages() {
    let doc = doc_with_text();
    let groups = doc.duplicate_page_groups().unwrap();
    assert_eq!(groups, vec![vec![0, 1]]);
}

#[test]
fn split_into_tiles() {
    let mut doc = doc_with_text();
    doc.split_pages_grid(&[1], 2, 2).unwrap();
    assert_eq!(doc.page_count(), 6);
    let s = doc.page_size(1).unwrap();
    assert!(
        (s.width - 297.5).abs() < 0.1 && (s.height - 421.0).abs() < 0.1,
        "{s:?}"
    );
    // The watermark sits in the middle of the page: the lower-right tile holds part of it.
    let tiles: Vec<_> = (1..5).map(|p| doc.page_size(p).unwrap().crop_box).collect();
    assert_ne!(tiles[0], tiles[3]);
}

#[test]
fn overlay_and_background() {
    let mut letterhead = Document::new_blank(1, 595.0, 842.0).unwrap();
    letterhead
        .add_header_footer(
            &[0],
            ["ACME LETTERHEAD", "", ""],
            ["", "", ""],
            14.0,
            30.0,
            [0.0, 0.0, 0.0],
        )
        .unwrap();
    let mut doc = doc_with_text();
    doc.overlay_document(&letterhead, &[0, 2], true).unwrap();
    assert!(doc.page_text(0).contains("ACME LETTERHEAD"));
    assert!(!doc.page_text(1).contains("ACME"));
    assert!(doc.marks_on_page(0).contains(&"Underlay".to_string()));
    doc.add_background_color(&[1], [0.9, 0.95, 1.0], 1.0).unwrap();
    let img = doc.render_page_png(1, 0.2).unwrap();
    let px = image::load_from_memory(&img.png).unwrap().to_rgb8();
    let corner = px.get_pixel(2, 2);
    assert!(corner[2] > 240 && corner[0] < 240, "page tinted blue: {corner:?}");
    assert!(
        doc.page_text(1).contains("SAME"),
        "content still drawn over the background"
    );
    assert_eq!(doc.remove_marks(&[1], Some("Background")).unwrap(), 1);
}

#[test]
fn guard_protects_added_content_from_leaked_state() {
    // A page whose content flips the coordinate system without restoring it.
    let mut doc = Document::new_blank(1, 200.0, 200.0).unwrap();
    let page_id = doc.page_ids()[0];
    doc.inner_mut()
        .change_page_content(page_id, b"1 0 0 -1 0 200 cm".to_vec())
        .unwrap();
    doc.commit().unwrap();
    doc.add_header_footer(
        &[0],
        ["TOPLEFT", "", ""],
        ["", "", ""],
        12.0,
        10.0,
        [0.0, 0.0, 0.0],
    )
    .unwrap();
    let layer = doc.text_layer(0).unwrap();
    assert!(layer.text.contains("TOPLEFT"));
    assert!(
        layer.glyphs[0].bbox[1] < 40.0,
        "header stays at the top: {:?}",
        layer.glyphs[0].bbox
    );
}

#[test]
fn transitions_and_labels() {
    let mut doc = doc_with_text();
    doc.set_page_transition(&[0, 1], Some("Dissolve"), 1.5, Some(4.0))
        .unwrap();
    assert_eq!(doc.page_transition(0), Some(("Dissolve".to_string(), 1.5)));
    assert!(doc.set_page_transition(&[0], Some("Explode"), 1.0, None).is_err());
    doc.set_page_transition(&[0], None, 0.0, None).unwrap();
    assert_eq!(doc.page_transition(0), None);

    assert_eq!(doc.page_labels(), vec!["1", "2", "3"]);
    doc.set_page_labels(&[(0, "r", "", 1), (2, "D", "A-", 5)])
        .unwrap();
    assert_eq!(doc.page_labels(), vec!["i", "ii", "A-5"]);
    let reopened = Document::from_bytes(doc.bytes().as_ref().clone(), None, None).unwrap();
    assert_eq!(reopened.page_labels(), vec!["i", "ii", "A-5"]);
}

#[test]
fn flatten_single_annotation_and_normalize() {
    let mut doc = doc_with_text();
    doc.add_shape(
        0,
        Shape::Square,
        [10.0, 10.0, 60.0, 60.0],
        [1.0, 0.0, 0.0],
        None,
        2.0,
        "me",
        "",
    )
    .unwrap();
    doc.add_shape(
        0,
        Shape::Circle,
        [100.0, 10.0, 160.0, 60.0],
        [0.0, 0.0, 1.0],
        None,
        2.0,
        "me",
        "",
    )
    .unwrap();
    doc.flatten_annotation(0, 0).unwrap();
    let left = doc.annotations(0).unwrap();
    assert_eq!(left.len(), 1);
    assert_eq!(left[0].subtype, "Circle");

    doc.resize_pages(&[1], 300.0, 300.0, true).unwrap();
    assert_eq!(doc.normalize_page_sizes().unwrap(), 1);
    assert_eq!(doc.page_size(1).unwrap().width, 595.0);
}
