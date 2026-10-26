//! Documents from a scanner: pages as large as the paper scanned (from the
//! resolution given or written in the picture), the backs of the sheets put
//! back between the fronts.

use std::io::Cursor;

use folitecta_core::{Document, ScannedPicture, picture_dpi};
use image::{ImageFormat, RgbImage};

fn picture(w: u32, h: u32, shade: u8, format: ImageFormat) -> Vec<u8> {
    let img = RgbImage::from_pixel(w, h, image::Rgb([shade, shade, shade]));
    let mut out = Cursor::new(Vec::new());
    img.write_to(&mut out, format).unwrap();
    out.into_inner()
}

/// A JPEG picture saying it has `dpi` dots per inch.
fn jpeg_at(w: u32, h: u32, dpi: u16) -> Vec<u8> {
    let mut data = picture(w, h, 200, ImageFormat::Jpeg);
    let at = data
        .windows(5)
        .position(|x| x == b"JFIF\0")
        .expect("a JFIF header");
    data[at + 7] = 1;
    data[at + 8..at + 10].copy_from_slice(&dpi.to_be_bytes());
    data[at + 10..at + 12].copy_from_slice(&dpi.to_be_bytes());
    data
}

/// A PNG picture with a `pHYs` chunk of `dpi` (its checksum is not looked at).
fn png_at(w: u32, h: u32, dpi: f32) -> Vec<u8> {
    let data = picture(w, h, 120, ImageFormat::Png);
    let ppm = (dpi / 0.0254).round() as u32;
    let mut chunk = Vec::new();
    chunk.extend(9u32.to_be_bytes());
    chunk.extend(b"pHYs");
    chunk.extend(ppm.to_be_bytes());
    chunk.extend(ppm.to_be_bytes());
    chunk.push(1);
    chunk.extend(crc(&chunk[4..]).to_be_bytes());
    // After the signature (8) and IHDR (25).
    let mut out = data[..33].to_vec();
    out.extend(chunk);
    out.extend(&data[33..]);
    out
}

fn crc(bytes: &[u8]) -> u32 {
    let mut c = 0xFFFF_FFFFu32;
    for b in bytes {
        c ^= u32::from(*b);
        for _ in 0..8 {
            c = if c & 1 != 0 {
                0xEDB8_8320 ^ (c >> 1)
            } else {
                c >> 1
            };
        }
    }
    !c
}

#[test]
fn the_resolution_is_read_from_the_pictures() {
    assert_eq!(picture_dpi(&jpeg_at(40, 40, 200)), Some(200.0));
    let png = png_at(40, 40, 150.0);
    assert!((picture_dpi(&png).unwrap() - 150.0).abs() < 0.5);
    assert!(
        Document::image_size(&png).is_ok(),
        "the PNG with pHYs still reads"
    );
    let mut bmp = picture(40, 40, 90, ImageFormat::Bmp);
    bmp[38..42].copy_from_slice(&11811u32.to_le_bytes());
    assert!((picture_dpi(&bmp).unwrap() - 300.0).abs() < 0.5);
    assert_eq!(picture_dpi(&picture(40, 40, 90, ImageFormat::Png)), None);
}

#[test]
fn pages_are_as_large_as_the_paper_scanned() {
    let doc = Document::from_scans(vec![
        // Letter at 100 dpi, given.
        ScannedPicture {
            data: picture(850, 1100, 230, ImageFormat::Png),
            dpi: 100.0,
        },
        // A5 sideways at 200 dpi, written in the picture.
        ScannedPicture {
            data: jpeg_at(1654, 1165, 200),
            dpi: 0.0,
        },
        // No resolution: 300.
        ScannedPicture {
            data: picture(600, 900, 230, ImageFormat::Png),
            dpi: 0.0,
        },
    ])
    .unwrap();
    assert_eq!(doc.page_count(), 3);
    let sizes: Vec<(f32, f32)> = doc.page_sizes().iter().map(|s| (s.width, s.height)).collect();
    let near = |a: (f32, f32), b: (f32, f32)| (a.0 - b.0).abs() < 0.6 && (a.1 - b.1).abs() < 0.6;
    assert!(near(sizes[0], (612.0, 792.0)), "{sizes:?}");
    assert!(near(sizes[1], (595.4, 419.4)), "{sizes:?}");
    assert!(near(sizes[2], (144.0, 216.0)), "{sizes:?}");
    assert!(!doc.can_undo() && doc.path().is_none());
    assert!(Document::from_scans(Vec::new()).is_err());
}

#[test]
fn backs_scanned_after_the_fronts_go_between_them() {
    // Three fronts and two backs, told apart by their widths.
    let widths = [100u32, 200, 300, 400, 500];
    let pictures = widths
        .iter()
        .map(|w| ScannedPicture {
            data: picture(*w, 100, 200, ImageFormat::Png),
            dpi: 72.0,
        })
        .collect();
    let mut doc = Document::from_scans(pictures).unwrap();
    doc.interleave_backs(3, true).unwrap();
    let order: Vec<u32> = doc.page_sizes().iter().map(|s| s.width.round() as u32).collect();
    // The stack turned over: the second back scanned is the first sheet's.
    assert_eq!(order, vec![100, 500, 200, 400, 300]);
    doc.undo().unwrap();
    doc.interleave_backs(3, false).unwrap();
    let order: Vec<u32> = doc.page_sizes().iter().map(|s| s.width.round() as u32).collect();
    assert_eq!(order, vec![100, 400, 200, 500, 300]);
    assert!(doc.interleave_backs(1, false).is_err());
}
