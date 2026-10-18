//! Tests for the 0.0.2 operations: images, rasterizing, redaction, stamps, scripts, security.

use folitecta_core::marks::TextMarkOptions;
use folitecta_core::{Document, STAMP_PRESETS, SecurityPermissions};

fn sample() -> Document {
    let mut doc = Document::new_blank(2, 595.0, 842.0).expect("blank document");
    doc.add_header_footer(
        &[0, 1],
        ["Secret report", "", ""],
        ["", "Page {page}", ""],
        12.0,
        30.0,
        [0.0, 0.0, 0.0],
    )
    .unwrap();
    doc.add_text_watermark(
        &[1],
        &TextMarkOptions {
            text: "VISIBLE".into(),
            rotation: 0.0,
            ..TextMarkOptions::default()
        },
    )
    .unwrap();
    doc.clear_history();
    doc
}

fn png_bytes(w: u32, h: u32, rgb: [u8; 3]) -> Vec<u8> {
    let img = image::RgbImage::from_pixel(w, h, image::Rgb(rgb));
    let mut out = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(img)
        .write_to(&mut out, image::ImageFormat::Png)
        .unwrap();
    out.into_inner()
}

fn jpeg_bytes(w: u32, h: u32) -> Vec<u8> {
    let img = image::RgbImage::from_pixel(w, h, image::Rgb([30, 60, 200]));
    let mut out = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(img)
        .write_to(&mut out, image::ImageFormat::Jpeg)
        .unwrap();
    out.into_inner()
}

#[test]
fn place_images_and_image_pages() {
    let mut doc = sample();
    let rect = doc
        .add_image(0, png_bytes(200, 100, [220, 40, 40]), None)
        .unwrap();
    assert!(
        rect[2] - rect[0] > 300.0,
        "image scaled to 60% of the page width: {rect:?}"
    );
    doc.add_image(0, jpeg_bytes(64, 64), Some([50.0, 50.0, 150.0, 150.0]))
        .unwrap();
    assert_eq!(doc.image_count(0), 2);
    assert!(doc.marks_on_page(0).iter().filter(|m| *m == "Image").count() == 2);

    // Rotated page: the picture is still placed through the display mapping.
    doc.rotate_pages(&[1], 90).unwrap();
    doc.add_image(1, png_bytes(100, 100, [0, 120, 0]), None).unwrap();
    assert_eq!(doc.image_count(1), 1);

    let n = doc
        .insert_image_pages(
            1,
            vec![png_bytes(300, 100, [0, 0, 0]), png_bytes(100, 300, [0, 0, 0])],
        )
        .unwrap();
    assert_eq!(n, 2);
    assert_eq!(doc.page_count(), 4);
    assert!(
        doc.page_size(1).unwrap().width > doc.page_size(1).unwrap().height,
        "landscape page for a wide picture"
    );
    assert!(doc.page_size(2).unwrap().width < doc.page_size(2).unwrap().height);
    assert_eq!(doc.image_count(1), 1);

    // The rendered page shows the red picture.
    let img = doc.render_page_png(0, 0.5).unwrap();
    let decoded = image::load_from_memory(&img.png).unwrap().to_rgb8();
    let reddish = decoded.pixels().filter(|p| p[0] > 180 && p[1] < 90).count();
    assert!(reddish > 1000, "red pixels: {reddish}");

    let reopened = Document::from_bytes(doc.bytes().as_ref().clone(), None, None).unwrap();
    assert_eq!(reopened.page_count(), 4);
    assert_eq!(reopened.image_count(0), 2);
}

#[test]
fn rasterize_and_flatten() {
    let mut doc = sample();
    doc.add_note(0, 100.0, 700.0, "note", "me", [1.0, 0.9, 0.2])
        .unwrap();
    assert!(doc.page_text(0).contains("Secret"));
    doc.rasterize_pages(&[0], 96.0, false).unwrap();
    assert!(doc.page_text(0).is_empty(), "text is gone after rasterizing");
    assert_eq!(doc.image_count(0), 1);
    assert_eq!(
        doc.annotations(0).unwrap().len(),
        1,
        "comments survive a plain rasterization"
    );
    let s = doc.page_size(0).unwrap();
    assert_eq!((s.width, s.height), (595.0, 842.0));

    doc.flatten_comments(&[0]).unwrap();
    assert!(
        doc.annotations(0).unwrap().is_empty(),
        "flattening removes the comment objects"
    );
    let img = doc.render_page_png(0, 0.5).unwrap();
    let decoded = image::load_from_memory(&img.png).unwrap().to_rgb8();
    let yellow = decoded
        .pixels()
        .filter(|p| p[0] > 200 && p[1] > 170 && p[2] < 120)
        .count();
    assert!(yellow > 20, "the note icon was baked into the pixels: {yellow}");

    // Rotated pages become upright image pages.
    doc.rotate_pages(&[1], 90).unwrap();
    doc.rasterize_pages(&[1], 72.0, false).unwrap();
    let s = doc.page_size(1).unwrap();
    assert_eq!(s.rotation, 0);
    assert!(s.width > s.height);
}

#[test]
fn redaction_marks_and_apply() {
    let mut doc = sample();
    doc.add_redaction(0, [20.0, 780.0, 300.0, 830.0], "me").unwrap();
    doc.add_redaction(0, [20.0, 20.0, 100.0, 60.0], "me").unwrap();
    doc.add_redaction(1, [20.0, 20.0, 100.0, 60.0], "me").unwrap();
    assert_eq!(doc.pending_redactions().len(), 3);
    assert_eq!(doc.redaction_rects(0).len(), 2);
    assert!(doc.page_text(0).contains("Secret"));

    let applied = doc.apply_redactions(Some(&[0])).unwrap();
    assert_eq!(applied, 2);
    let text = doc.page_text(0);
    assert!(
        !text.contains("Secret"),
        "the header text under the box cannot be extracted any more: {text}"
    );
    assert!(
        text.contains("Page 1"),
        "the text outside the boxes stays text: {text}"
    );
    assert_eq!(doc.pending_redactions().len(), 1, "page 2 mark is still pending");
    let img = doc.render_page_png(0, 0.5).unwrap();
    let decoded = image::load_from_memory(&img.png).unwrap().to_rgb8();
    let black = decoded
        .pixels()
        .filter(|p| p[0] < 40 && p[1] < 40 && p[2] < 40)
        .count();
    assert!(black > 2000, "black boxes are painted: {black}");

    doc.undo().unwrap();
    assert!(
        doc.page_text(0).contains("Secret"),
        "undo restores the original page"
    );

    // Drawn as a picture: nothing of the page stays text.
    let report = doc
        .apply_redactions_with(Some(&[0]), folitecta_core::redaction::RedactionMethod::Picture)
        .unwrap();
    assert_eq!(report.areas, 2);
    assert_eq!(report.pages_as_pictures, vec![0]);
    assert!(doc.page_text(0).is_empty());
}

#[test]
fn stamps() {
    let mut doc = sample();
    for (i, text) in STAMP_PRESETS.iter().take(3).enumerate() {
        doc.add_stamp(
            0,
            [50.0, 600.0 - i as f32 * 60.0, 300.0, 650.0 - i as f32 * 60.0],
            text,
            [0.8, 0.1, 0.1],
            "me",
        )
        .unwrap();
    }
    let annots = doc.annotations(0).unwrap();
    assert_eq!(annots.len(), 3);
    assert!(annots.iter().all(|a| a.subtype == "Stamp"));
    assert_eq!(annots[0].contents, "APPROVED");
    assert!(
        doc.add_stamp(0, [0.0, 0.0, 10.0, 10.0], "   ", [0.0; 3], "me")
            .is_err()
    );
}

#[test]
fn scripts_listing_and_removal() {
    let mut doc = sample();
    assert!(doc.scripts().is_empty());
    // Inject a document script and a JavaScript open action through the object model.
    {
        let inner = doc.inner_mut();
        let action = inner.add_object(lopdf::dictionary! { "S" => "JavaScript", "JS" => lopdf::Object::string_literal("app.alert('hi');") });
        let tree = inner.add_object(lopdf::dictionary! { "Names" => vec![lopdf::Object::string_literal("init"), lopdf::Object::Reference(action)] });
        let names = inner.add_object(lopdf::dictionary! { "JavaScript" => tree });
        let open = inner.add_object(lopdf::dictionary! { "S" => "JavaScript", "JS" => lopdf::Object::string_literal("this.print();") });
        let catalog = inner.catalog_mut().unwrap();
        catalog.set("Names", lopdf::Object::Reference(names));
        catalog.set("OpenAction", lopdf::Object::Reference(open));
    }
    doc.commit().unwrap();
    let scripts = doc.scripts();
    assert_eq!(scripts.len(), 2);
    assert_eq!(scripts[0].name, "init");
    assert!(scripts[0].code.contains("alert"));
    assert_eq!(scripts[1].source, "Open action");
    assert_eq!(doc.remove_scripts().unwrap(), 2);
    assert!(doc.scripts().is_empty());
    let reopened = Document::from_bytes(doc.bytes().as_ref().clone(), None, None).unwrap();
    assert!(reopened.scripts().is_empty());
}

#[test]
fn encrypted_copy_requires_the_password() {
    let doc = sample();
    let dir = std::env::temp_dir().join(format!("folitecta-sec-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("secret.pdf");
    doc.save_encrypted(
        &path,
        "user",
        "owner",
        SecurityPermissions {
            copy: false,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(
        matches!(
            Document::open(&path),
            Err(folitecta_core::Error::PasswordRequired)
        ),
        "opening without a password fails"
    );
    let opened = Document::open_with_password(&path, "user").unwrap();
    assert_eq!(opened.page_count(), 2);
    assert!(opened.info().encrypted);
    assert!(
        opened.page_text(0).contains("Secret"),
        "content is intact after decryption"
    );
    let owner = Document::open_with_password(&path, "owner").unwrap();
    assert_eq!(owner.page_count(), 2);
    assert!(
        doc.save_encrypted(dir.join("x.pdf"), "", "", SecurityPermissions::default())
            .is_err()
    );
    let _ = std::fs::remove_dir_all(&dir);
}
