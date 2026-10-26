//! Colours to grey: an invoice (coloured lines, filled fields) with a
//! coloured picture, a JPEG picture and a red comment becomes grey — its
//! content, drawings, pictures and comment colours — and draws with grey
//! pixels only; an estimate changes nothing; Undo brings the colours back;
//! spot colours are left and counted.

use std::io::Cursor;

use folitecta_core::{Document, Renderer, Template, TemplateOptions};
use lopdf::{Object, dictionary};

fn picture(format: image::ImageFormat) -> Vec<u8> {
    let img = image::RgbImage::from_fn(80, 60, |i, j| image::Rgb([(i * 3) as u8, 40, (j * 4) as u8]));
    let mut out = Cursor::new(Vec::new());
    img.write_to(&mut out, format).unwrap();
    out.into_inner()
}

fn coloured() -> Document {
    let mut doc = Document::from_template(Template::Invoice, &TemplateOptions::default()).unwrap();
    doc.add_image(
        0,
        picture(image::ImageFormat::Png),
        Some([300.0, 600.0, 380.0, 660.0]),
    )
    .unwrap();
    doc.add_image(
        0,
        picture(image::ImageFormat::Jpeg),
        Some([400.0, 600.0, 480.0, 660.0]),
    )
    .unwrap();
    doc.add_note(0, 200.0, 200.0, "a red note", "Tester", [1.0, 0.0, 0.0])
        .unwrap();
    doc.clear_history();
    doc
}

/// The largest difference between the channels of any pixel of page 0.
fn colourfulness(doc: &Document) -> u8 {
    let page = Renderer::new(doc.bytes()).unwrap().render_page(0, 0.6).unwrap();
    let img = image::load_from_memory(&page.png).unwrap().to_rgb8();
    img.pixels()
        .map(|p| {
            let [r, g, b] = p.0;
            r.max(g).max(b) - r.min(g).min(b)
        })
        .max()
        .unwrap()
}

#[test]
fn a_coloured_document_becomes_grey() {
    let mut doc = coloured();
    assert!(colourfulness(&doc) > 60, "the document has colours to begin with");
    let before = doc.bytes();
    let estimate = doc.estimate_grey().unwrap();
    assert_eq!(doc.bytes(), before, "an estimate changes nothing");
    assert!(
        estimate.colours > 0 && estimate.pictures == 2 && estimate.comments > 0,
        "{estimate:?}"
    );

    let report = doc.convert_to_grey().unwrap();
    assert_eq!(report.pictures, 2);
    assert!(
        colourfulness(&doc) <= 3,
        "only grey pixels: {}",
        colourfulness(&doc)
    );
    // The pictures are grey, the note's colour a grey.
    let inner = doc.inner();
    let images: Vec<&lopdf::Stream> = inner
        .objects
        .values()
        .filter_map(|o| o.as_stream().ok())
        .filter(|s| s.dict.get(b"Subtype").ok().and_then(|n| n.as_name().ok()) == Some(b"Image".as_slice()))
        .collect();
    assert!(images.iter().all(|s| {
        s.dict.get(b"SMask").is_ok()
            || s.dict.get(b"ColorSpace").ok().and_then(|c| c.as_name().ok()) == Some(b"DeviceGray".as_slice())
    }));
    let note = doc
        .annotations(0)
        .unwrap()
        .into_iter()
        .find(|a| a.contents == "a red note")
        .unwrap();
    let c = inner
        .get_dictionary(note.id.unwrap())
        .unwrap()
        .get(b"C")
        .unwrap()
        .as_array()
        .unwrap();
    assert_eq!(c.len(), 1);
    // Saved and opened again: still grey. Undo: colours again.
    let reopened = Document::from_bytes(doc.bytes().to_vec(), None, None).unwrap();
    assert!(colourfulness(&reopened) <= 3);
    doc.undo().unwrap();
    assert!(colourfulness(&doc) > 60);
    // A grey document has nothing more to convert.
    let mut again = reopened;
    assert!(again.convert_to_grey().unwrap().is_empty());
    assert!(!again.can_undo());
}

#[test]
fn spot_colours_are_left_and_counted() {
    let mut doc = Document::new_blank(1, 200.0, 200.0).unwrap();
    let pid = doc.page_ids()[0];
    let inner = doc.inner_mut();
    let tint = inner.add_object(dictionary! {
        "FunctionType" => 2, "Domain" => vec![0.into(), 1.into()],
        "C0" => vec![0.into(), 0.into(), 0.into(), 0.into()],
        "C1" => vec![0.into(), 1.into(), 0.into(), 0.into()], "N" => 1,
    });
    let spot = Object::Array(vec![
        Object::Name(b"Separation".to_vec()),
        Object::Name(b"PANTONE 123".to_vec()),
        Object::Name(b"DeviceCMYK".to_vec()),
        Object::Reference(tint),
    ]);
    let content = inner.add_object(lopdf::Stream::new(
        dictionary! {},
        b"/CS0 cs 1 sc 10 10 50 50 re f 1 0 0 rg 100 100 50 50 re f".to_vec(),
    ));
    let page = inner.get_dictionary_mut(pid).unwrap();
    page.set("Contents", Object::Reference(content));
    page.set(
        "Resources",
        dictionary! { "ColorSpace" => dictionary! { "CS0" => spot } },
    );
    doc.commit().unwrap();
    doc.clear_history();
    let report = doc.convert_to_grey().unwrap();
    assert_eq!(report.colours, 1, "{report:?}");
    assert_eq!(report.kept.get("spot"), Some(&1), "{report:?}");
    let text = String::from_utf8_lossy(&doc.inner().get_page_content(pid)).to_string();
    assert!(text.contains("/CS0 cs") && !text.contains(" rg"), "{text}");
}
