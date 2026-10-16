//! Tests for annotation copy/move/resize, vector flattening and image stamps.

use folitecta_core::{Document, FlattenScope, Markup, Shape};

fn doc_with_comments() -> Document {
    let mut doc = Document::new_blank(2, 595.0, 842.0).unwrap();
    doc.add_shape(
        0,
        Shape::Square,
        [100.0, 100.0, 200.0, 180.0],
        [1.0, 0.0, 0.0],
        Some([1.0, 1.0, 0.0]),
        2.0,
        "me",
        "box",
    )
    .unwrap();
    doc.add_free_text(
        0,
        [100.0, 600.0, 300.0, 650.0],
        "Hello",
        "me",
        14.0,
        [0.0, 0.0, 1.0],
        true,
    )
    .unwrap();
    doc.add_ink(0, &[vec![(50.0, 50.0), (80.0, 90.0)]], [0.0, 0.0, 0.0], 2.0, "me")
        .unwrap();
    doc.add_text_markup(
        0,
        Markup::Highlight,
        &[[100.0, 700.0, 200.0, 700.0, 100.0, 690.0, 200.0, 690.0]],
        [1.0, 1.0, 0.0],
        "me",
        "",
    )
    .unwrap();
    doc.clear_history();
    doc
}

fn png_rgba(w: u32, h: u32) -> Vec<u8> {
    let mut img = image::RgbaImage::new(w, h);
    for (x, _y, p) in img.enumerate_pixels_mut() {
        *p = if x < w / 2 {
            image::Rgba([0, 160, 0, 255])
        } else {
            image::Rgba([0, 0, 0, 0])
        };
    }
    let mut out = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(img)
        .write_to(&mut out, image::ImageFormat::Png)
        .unwrap();
    out.into_inner()
}

#[test]
fn duplicate_move_and_resize() {
    let mut doc = doc_with_comments();
    doc.duplicate_annotation(0, 0, 1, 10.0, -10.0).unwrap();
    let copy = &doc.annotations(1).unwrap()[0];
    assert_eq!(copy.subtype, "Square");
    assert_eq!(copy.rect, [110.0, 90.0, 210.0, 170.0]);
    assert_eq!(copy.contents, "box");

    doc.move_annotation(1, 0, 5.0, 5.0).unwrap();
    assert_eq!(doc.annotations(1).unwrap()[0].rect, [115.0, 95.0, 215.0, 175.0]);

    doc.set_annotation_rect(0, 3, [100.0, 680.0, 300.0, 710.0])
        .unwrap();
    let hl = &doc.annotations(0).unwrap()[3];
    assert_eq!(hl.rect, [100.0, 680.0, 300.0, 710.0]);

    // Ink geometry follows the move.
    doc.move_annotation(0, 2, 100.0, 0.0).unwrap();
    let ink_rect = doc.annotations(0).unwrap()[2].rect;
    assert!(ink_rect[0] > 140.0, "{ink_rect:?}");
    assert!(doc.set_annotation_rect(0, 0, [0.0, 0.0, 0.0, 0.0]).is_err());

    // Undo restores the original geometry.
    doc.undo().unwrap();
    assert!(doc.annotations(0).unwrap()[2].rect[0] < 60.0);
}

#[test]
fn snapshot_pastes_into_another_document() {
    let src = doc_with_comments();
    let snap = src.annotation_snapshot(0, 1).unwrap();
    let mut dst = Document::new_blank(1, 300.0, 300.0).unwrap();
    dst.paste_annotation_snapshot(0, &snap, 0.0, -500.0).unwrap();
    let a = &dst.annotations(0).unwrap()[0];
    assert_eq!(a.subtype, "FreeText");
    assert_eq!(a.contents, "Hello");
    // The appearance (with its font resource) came along and renders.
    let img = dst.render_page_png(0, 1.0).unwrap();
    assert_eq!(img.width, 300);
    let reopened = Document::from_bytes(dst.bytes().as_ref().clone(), None, None).unwrap();
    assert_eq!(reopened.annotations(0).unwrap().len(), 1);
}

#[test]
fn vector_flatten_keeps_appearance_and_text() {
    let mut doc = doc_with_comments();
    let before = doc.render_page_png(0, 1.0).unwrap();
    let n = doc.flatten(&[0], FlattenScope::Comments).unwrap();
    assert_eq!(n, 4);
    assert!(doc.annotations(0).unwrap().is_empty());
    assert!(
        doc.page_text(0).contains("Hello"),
        "free text became real page text: {:?}",
        doc.page_text(0)
    );
    let after = doc.render_page_png(0, 1.0).unwrap();
    let a = image::load_from_memory(&before.png).unwrap().to_rgb8();
    let b = image::load_from_memory(&after.png).unwrap().to_rgb8();
    // The yellow filled rectangle (100 x 80 pt) must look the same before and after.
    let yellow = |img: &image::RgbImage| {
        img.pixels()
            .filter(|p| p[0] > 200 && p[1] > 200 && p[2] < 100)
            .count()
    };
    let (yb, ya) = (yellow(&a), yellow(&b));
    assert!(yb > 5000, "rendered comment before: {yb}");
    assert!(
        ya * 100 >= yb * 95 && ya * 95 <= yb * 100,
        "flattened shape still drawn: {ya} vs {yb}"
    );
    assert_eq!(doc.marks_on_page(0), vec!["Flatten"]);
}

#[test]
fn image_stamp_with_transparency() {
    let mut doc = Document::new_blank(1, 400.0, 400.0).unwrap();
    doc.add_image_stamp(
        0,
        [100.0, 100.0, 300.0, 200.0],
        png_rgba(40, 20),
        "me",
        "signature",
    )
    .unwrap();
    let a = &doc.annotations(0).unwrap()[0];
    assert_eq!(a.subtype, "Stamp");
    let img = doc.render_page_png(0, 1.0).unwrap();
    let px = image::load_from_memory(&img.png).unwrap().to_rgb8();
    // left half green, right half transparent (white page shows through)
    let green = px.get_pixel(150, 250);
    let white = px.get_pixel(250, 250);
    assert!(green[1] > 120 && green[0] < 60, "left half is green: {green:?}");
    assert!(
        white[0] > 240 && white[1] > 240 && white[2] > 240,
        "right half is transparent: {white:?}"
    );
}

#[test]
fn pages_from_images() {
    let doc = Document::from_images(vec![png_rgba(300, 100), png_rgba(100, 300)]).unwrap();
    assert_eq!(doc.page_count(), 2);
    assert!(doc.page_size(0).unwrap().width > doc.page_size(0).unwrap().height);
    assert_eq!(doc.image_count(1), 1);
    assert!(!doc.can_undo());
    assert!(Document::from_images(vec![]).is_err());
    assert_eq!(Document::image_size(&png_rgba(30, 20)).unwrap(), (30, 20));
}
