use folitecta_core::Document;
use folitecta_core::imaging::{Edit, Format, Work};
use folitecta_core::pictures::Colors;
use image::{Rgba, RgbaImage};
use lopdf::{Object, Stream, dictionary};

fn sample_pixels() -> RgbaImage {
    RgbaImage::from_fn(40, 20, |x, y| {
        if x == 0 && y == 0 {
            Rgba([0, 0, 0, 0])
        } else if x < 20 {
            Rgba([200, 30, 30, 255])
        } else {
            Rgba([20, 40, (y * 10) as u8, 255])
        }
    })
}

fn png(px: &RgbaImage) -> Vec<u8> {
    let mut out = std::io::Cursor::new(Vec::new());
    px.write_to(&mut out, image::ImageFormat::Png).unwrap();
    out.into_inner()
}

fn doc_with_picture() -> Document {
    let mut doc = Document::new_blank(1, 595.0, 842.0).unwrap();
    doc.add_image(0, png(&sample_pixels()), Some([100.0, 200.0, 300.0, 300.0]))
        .unwrap();
    doc
}

fn close(a: [f32; 4], b: [f32; 4]) -> bool {
    a.iter().zip(b).all(|(x, y)| (x - y).abs() < 0.01)
}

#[test]
fn pictures_are_found_where_they_are_drawn() {
    let doc = doc_with_picture();
    let list = doc.page_images(0).unwrap();
    assert_eq!(list.len(), 1);
    let im = &list[0];
    assert_eq!((im.width, im.height), (40, 20));
    assert!(close(im.rect, [100.0, 200.0, 300.0, 300.0]), "{:?}", im.rect);
    assert_eq!(im.colors, Colors::Color);
    assert_eq!(doc.image_at(0, 150.0, 250.0), Some(0));
    assert_eq!(doc.image_at(0, 50.0, 50.0), None);
    assert_eq!(doc.image_pixels(0, 0).unwrap(), sample_pixels());
}

#[test]
fn an_edited_picture_takes_the_place_of_the_old_one() {
    let mut doc = doc_with_picture();
    let im = doc.page_images(0).unwrap()[0].clone();
    let mut work = Work::new(doc.image_pixels(0, 0).unwrap(), im.matrix, im.colors);
    work.apply(Edit::RotateRight);
    let rotated = work.pixels.clone();
    doc.replace_page_image(0, 0, &work.pixels, work.placement, work.colors)
        .unwrap();
    let list = doc.page_images(0).unwrap();
    assert_eq!(list.len(), 1);
    let after = &list[0];
    assert_eq!((after.width, after.height), (20, 40));
    assert_eq!(
        after.object, im.object,
        "a picture used once is replaced in place"
    );
    // Same centre (200, 250), sides swapped: 200 × 100 becomes 100 × 200.
    assert!(
        close(after.rect, [150.0, 150.0, 250.0, 350.0]),
        "{:?}",
        after.rect
    );
    assert_eq!(doc.image_pixels(0, 0).unwrap(), rotated);
    // The edit is one step of undo.
    doc.undo().unwrap();
    assert_eq!(doc.image_pixels(0, 0).unwrap(), sample_pixels());
    assert!(close(
        doc.page_images(0).unwrap()[0].rect,
        [100.0, 200.0, 300.0, 300.0]
    ));

    // Cropped to the right half: it stays where that half was.
    let mut work = Work::new(sample_pixels(), im.matrix, im.colors);
    work.apply(Edit::Crop([20, 0, 40, 20]));
    work.apply(Edit::Convert(Format::Gray));
    doc.replace_page_image(0, 0, &work.pixels, work.placement, work.colors)
        .unwrap();
    let after = doc.page_images(0).unwrap()[0].clone();
    assert!(
        close(after.rect, [200.0, 200.0, 300.0, 300.0]),
        "{:?}",
        after.rect
    );
    assert_eq!(after.colors, Colors::Gray);
    let saved = doc.bytes().to_vec();
    let reopened = Document::from_bytes(saved, None, None).unwrap();
    let px = reopened.image_pixels(0, 0).unwrap();
    assert_eq!(px.dimensions(), (20, 20));
    assert!(px.pixels().all(|p| p[0] == p[1] && p[1] == p[2]));
}

/// The sample picture drawn a second time, smaller, at (400, 600).
fn doc_drawing_twice() -> Document {
    let mut doc = doc_with_picture();
    // Draw the same picture a second time, smaller, elsewhere.
    let name = {
        let res = doc.page_images(0).unwrap();
        let id = res[0].object;
        let inner = doc.inner();
        let page = inner.get_pages()[&1];
        let resources = inner
            .get_dictionary(page)
            .unwrap()
            .get(b"Resources")
            .unwrap()
            .clone();
        let xobjects = match resources {
            Object::Dictionary(d) => d.get(b"XObject").unwrap().as_dict().unwrap().clone(),
            Object::Reference(r) => inner
                .get_dictionary(r)
                .unwrap()
                .get(b"XObject")
                .unwrap()
                .as_dict()
                .unwrap()
                .clone(),
            _ => panic!(),
        };
        xobjects
            .iter()
            .find(|(_, v)| matches!(v, Object::Reference(r) if *r == id))
            .map(|(k, _)| String::from_utf8_lossy(k).into_owned())
            .unwrap()
    };
    let page = doc.inner().get_pages()[&1];
    let extra = doc.inner_mut().add_object(Stream::new(
        dictionary! {},
        format!("q 40 0 0 20 400 600 cm /{name} Do Q\n").into_bytes(),
    ));
    let contents = doc
        .inner()
        .get_dictionary(page)
        .unwrap()
        .get(b"Contents")
        .unwrap()
        .clone();
    let mut list = match contents {
        Object::Array(a) => a,
        other => vec![other],
    };
    list.push(Object::Reference(extra));
    doc.inner_mut()
        .get_dictionary_mut(page)
        .unwrap()
        .set("Contents", Object::Array(list));
    doc.commit().unwrap();
    doc
}

#[test]
fn a_picture_drawn_twice_is_kept_for_the_other_drawing() {
    let mut doc = doc_drawing_twice();
    assert_eq!(doc.page_images(0).unwrap().len(), 2);

    let mut work = Work::new(
        sample_pixels(),
        doc.page_images(0).unwrap()[1].matrix,
        Colors::Color,
    );
    work.apply(Edit::Invert);
    doc.replace_page_image(0, 1, &work.pixels, work.placement, work.colors)
        .unwrap();
    let list = doc.page_images(0).unwrap();
    assert_eq!(list.len(), 2);
    assert_ne!(list[0].object, list[1].object);
    assert_eq!(doc.image_pixels(0, 0).unwrap(), sample_pixels());
    assert_eq!(doc.image_pixels(0, 1).unwrap(), work.pixels);
    assert!(
        close(list[1].rect, [400.0, 600.0, 440.0, 620.0]),
        "{:?}",
        list[1].rect
    );
}

#[test]
fn a_picture_changed_everywhere() {
    let mut doc = doc_drawing_twice();
    let first = doc.page_images(0).unwrap()[0].clone();
    let mut work = Work::new(sample_pixels(), first.matrix, Colors::Color);
    work.apply(Edit::RotateLeft);
    let n = doc
        .replace_image_everywhere(0, 0, &work.pixels, work.placement, work.colors)
        .unwrap();
    assert_eq!(n, 2);
    let list = doc.page_images(0).unwrap();
    assert_eq!(list[0].object, list[1].object);
    assert!(list.iter().all(|im| (im.width, im.height) == (20, 40)));
    // Both drawings turned around their own centres.
    assert!(
        close(list[0].rect, [150.0, 150.0, 250.0, 350.0]),
        "{:?}",
        list[0].rect
    );
    assert!(
        close(list[1].rect, [410.0, 590.0, 430.0, 630.0]),
        "{:?}",
        list[1].rect
    );
    assert_eq!(doc.image_pixels(0, 1).unwrap(), work.pixels);
    doc.undo().unwrap();
    assert_eq!(doc.image_pixels(0, 1).unwrap(), sample_pixels());
}

#[test]
fn unusual_pictures_are_read_through_the_renderer() {
    let mut doc = Document::new_blank(1, 300.0, 300.0).unwrap();
    // A gray picture drawn inverted (Decode [1 0]) and a two-colour palette.
    let gray: Vec<u8> = (0..64u32).map(|i| (i * 4) as u8).collect();
    let inverted = doc.inner_mut().add_object(Stream::new(
        dictionary! {
            "Type" => "XObject", "Subtype" => "Image", "Width" => 8i64, "Height" => 8i64,
            "ColorSpace" => "DeviceGray", "BitsPerComponent" => 8i64,
            "Decode" => vec![1.into(), 0.into()],
        },
        gray.clone(),
    ));
    let palette = doc.inner_mut().add_object(Stream::new(
        dictionary! {
            "Type" => "XObject", "Subtype" => "Image", "Width" => 8i64, "Height" => 1i64,
            "ColorSpace" => vec![
                Object::Name(b"Indexed".to_vec()),
                Object::Name(b"DeviceRGB".to_vec()),
                1.into(),
                Object::String(vec![255, 0, 0, 0, 0, 255], lopdf::StringFormat::Hexadecimal),
            ],
            "BitsPerComponent" => 1i64,
        },
        vec![0b1010_1010],
    ));
    let page = doc.inner().get_pages()[&1];
    doc.inner_mut().add_xobject(page, "A", inverted).unwrap();
    doc.inner_mut().add_xobject(page, "B", palette).unwrap();
    let content = doc.inner_mut().add_object(Stream::new(
        dictionary! {},
        b"q 80 0 0 80 10 10 cm /A Do Q q 80 0 0 10 100 10 cm /B Do Q".to_vec(),
    ));
    doc.inner_mut()
        .get_dictionary_mut(page)
        .unwrap()
        .set("Contents", content);
    doc.commit().unwrap();

    let a = doc.image_pixels(0, 0).unwrap();
    for (i, p) in a.pixels().enumerate() {
        let want = 255 - gray[i] as i32;
        assert!((p[0] as i32 - want).abs() <= 1, "pixel {i}: {} vs {want}", p[0]);
        assert_eq!(p[3], 255);
    }
    let b = doc.image_pixels(0, 1).unwrap();
    assert_eq!(b.get_pixel(0, 0).0, [0, 0, 255, 255]);
    assert_eq!(b.get_pixel(1, 0).0, [255, 0, 0, 255]);
}

#[test]
fn jpeg_pictures_stay_jpeg() {
    let px = RgbaImage::from_fn(32, 16, |x, _| Rgba([(x * 8) as u8, 100, 160, 255]));
    let rgb = image::DynamicImage::ImageRgba8(px).to_rgb8();
    let mut jpg = std::io::Cursor::new(Vec::new());
    rgb.write_to(&mut jpg, image::ImageFormat::Jpeg).unwrap();
    let mut doc = Document::new_blank(1, 595.0, 842.0).unwrap();
    doc.add_image(0, jpg.into_inner(), Some([50.0, 50.0, 210.0, 130.0]))
        .unwrap();
    let im = doc.page_images(0).unwrap()[0].clone();
    assert_eq!(im.filter, "DCTDecode");
    let mut work = Work::new(doc.image_pixels(0, 0).unwrap(), im.matrix, im.colors);
    work.apply(Edit::FlipHorizontal);
    doc.replace_page_image(0, 0, &work.pixels, work.placement, work.colors)
        .unwrap();
    let after = doc.page_images(0).unwrap()[0].clone();
    assert_eq!(after.filter, "DCTDecode");
    assert!(close(after.rect, im.rect));
    // Black and white is one bit per pixel, without loss.
    work.apply(Edit::Convert(Format::Bilevel));
    doc.replace_page_image(0, 0, &work.pixels, work.placement, work.colors)
        .unwrap();
    let bw = doc.page_images(0).unwrap()[0].clone();
    assert_eq!((bw.colors, bw.bits), (Colors::BlackWhite, 1));
    assert_eq!(bw.filter, "FlateDecode");
}
