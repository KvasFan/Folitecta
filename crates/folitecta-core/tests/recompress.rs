//! Recompression of pictures: an estimate that changes nothing, presets that
//! scale pictures down to a resolution where they are drawn largest, one
//! selected picture changed without the others, transparent pictures and
//! results that would not be smaller left alone, the file opened again and
//! drawn.

use std::io::Cursor;

use folitecta_core::{Document, Kept, RecompressPreset, RecompressScope, RecompressSettings, Renderer};
use image::{ImageFormat, RgbImage, RgbaImage};

/// A photograph-like picture (noise over a gradient), as PNG.
fn photo(w: u32, h: u32, seed: u32) -> Vec<u8> {
    let mut x = seed.wrapping_mul(2654435761).wrapping_add(1);
    let img = RgbImage::from_fn(w, h, |i, j| {
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        let n = (x & 0x3f) as u8;
        image::Rgb([
            ((i * 200 / w) as u8).saturating_add(n),
            ((j * 200 / h) as u8).saturating_add(n / 2),
            120u8.saturating_add(n),
        ])
    });
    let mut out = Cursor::new(Vec::new());
    img.write_to(&mut out, ImageFormat::Png).unwrap();
    out.into_inner()
}

/// Two pages: a large picture filling the first (about 300 dpi) and a small
/// one drawn in a corner of it; a third picture on the second page.
fn document() -> Document {
    let mut doc = Document::from_images(vec![photo(2200, 2800, 1), photo(900, 700, 2)]).unwrap();
    // A small picture in a corner of the first page: 600 px over 2 inches.
    doc.add_image(0, photo(600, 450, 3), Some([40.0, 40.0, 184.0, 148.0]))
        .unwrap();
    doc.clear_history();
    doc
}

fn object_bytes(doc: &Document, id: (u32, u16)) -> Vec<u8> {
    doc.inner()
        .get_object(id)
        .unwrap()
        .as_stream()
        .unwrap()
        .content
        .clone()
}

#[test]
fn an_estimate_tells_picture_by_picture_and_changes_nothing() {
    let doc = document();
    let before = doc.bytes();
    let report = doc
        .estimate_recompress(&RecompressScope::All, RecompressPreset::Screen.settings())
        .unwrap();
    assert_eq!(doc.bytes(), before, "nothing changed");
    assert_eq!(report.items.len(), 3, "{report:?}");
    assert_eq!(report.images, 3);
    assert!(
        report.after * 4 < report.before,
        "{} -> {}",
        report.before,
        report.after
    );
    for item in &report.items {
        let dpi = item.dpi.unwrap();
        let scaled = item.size_after.0 as f32 / item.size_before.0 as f32 * dpi;
        assert!((scaled - 96.0).abs() < 4.0, "{item:?}: {scaled} dpi");
        assert!(item.kept.is_none() && item.page.is_some());
    }
    // Archive keeps the resolution.
    let archive = doc
        .estimate_recompress(&RecompressScope::All, RecompressPreset::Archive.settings())
        .unwrap();
    assert!(archive.items.iter().all(|i| i.size_after == i.size_before));
}

#[test]
fn one_picture_changes_without_the_others() {
    let mut doc = document();
    let images = doc.page_images(0).unwrap();
    assert_eq!(images.len(), 2);
    let (big, small) = (images[0].object, images[1].object);
    let big_before = object_bytes(&doc, big);
    let report = doc
        .recompress(
            &RecompressScope::Picture { page: 0, index: 1 },
            RecompressPreset::Ebook.settings(),
        )
        .unwrap();
    assert_eq!(report.images, 1);
    assert_eq!(report.items[0].object, small);
    assert_eq!(
        object_bytes(&doc, big),
        big_before,
        "the other picture is untouched"
    );
    // 300 dpi down to 150.
    assert_eq!(report.items[0].size_after, (300, 225));
    assert!(doc.can_undo());
    doc.undo().unwrap();
    assert_ne!(doc.page_images(0).unwrap()[1].width, 300);
    assert!(
        doc.estimate_recompress(
            &RecompressScope::Picture { page: 0, index: 5 },
            RecompressPreset::Screen.settings()
        )
        .is_err()
    );
}

#[test]
fn recompressed_pages_open_again_and_draw() {
    let mut doc = document();
    let report = doc
        .recompress(
            &RecompressScope::Pages(vec![1]),
            RecompressPreset::Print.settings(),
        )
        .unwrap();
    assert_eq!(report.items.len(), 1, "only the second page: {report:?}");
    let reopened = Document::from_bytes(doc.bytes().to_vec(), None, None).unwrap();
    let im = &reopened.page_images(1).unwrap()[0];
    assert_eq!(im.filter, "DCTDecode");
    assert_eq!((im.width, im.height), report.items[0].size_after);
    let renderer = Renderer::new(reopened.bytes()).unwrap();
    for page in 0..2 {
        assert!(renderer.render_page(page, 0.2).is_ok());
    }
}

#[test]
fn transparent_pictures_and_larger_results_are_left_alone() {
    // A picture with transparency, placed from a PNG with an alpha channel.
    let mut rgba = Cursor::new(Vec::new());
    RgbaImage::from_fn(300, 200, |i, _| image::Rgba([200, 30, 30, (i % 255) as u8]))
        .write_to(&mut rgba, ImageFormat::Png)
        .unwrap();
    let mut doc = Document::new_blank(1, 595.0, 842.0).unwrap();
    doc.add_image(0, rgba.into_inner(), None).unwrap();
    // A small flat picture: JPEG at 100 is larger than its compressed pixels.
    let flat = {
        let img = RgbImage::from_pixel(64, 64, image::Rgb([250, 250, 250]));
        let mut out = Cursor::new(Vec::new());
        img.write_to(&mut out, ImageFormat::Png).unwrap();
        out.into_inner()
    };
    doc.add_image(0, flat, Some([300.0, 300.0, 364.0, 364.0]))
        .unwrap();
    doc.clear_history();
    let before = doc.bytes();
    let report = doc
        .recompress(
            &RecompressScope::All,
            RecompressSettings {
                quality: 100,
                max_dpi: 0.0,
                max_px: 0,
            },
        )
        .unwrap();
    let kept: Vec<Option<Kept>> = report.items.iter().map(|i| i.kept).collect();
    assert!(kept.contains(&Some(Kept::Mask)), "{kept:?}");
    assert!(kept.contains(&Some(Kept::NotSmaller)), "{kept:?}");
    assert_eq!(report.images, 0);
    assert_eq!(doc.bytes(), before, "a worse result is never written");
    assert!(!doc.can_undo());
}
