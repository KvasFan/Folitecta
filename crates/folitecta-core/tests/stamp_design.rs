//! Designed stamps: every element kind, fields filled when placed, templates.

use std::collections::HashMap;

use base64::Engine;
use folitecta_core::Document;
use folitecta_core::stamp_design::{StampDesign, StampElement, StampValues, TEMPLATES};

fn png() -> String {
    let img = image::RgbaImage::from_fn(20, 10, |x, _| {
        image::Rgba([0, 0, 255, if x % 2 == 0 { 255 } else { 90 }])
    });
    let mut out = std::io::Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png).unwrap();
    base64::engine::general_purpose::STANDARD.encode(out.into_inner())
}

fn values() -> StampValues {
    StampValues {
        name: "Ann Example".into(),
        email: "ann@example.org".into(),
        corporation: "Example Ltd".into(),
        login: "ann".into(),
        counter: 7,
        custom: HashMap::from([("Invoice".to_string(), "INV-42".to_string())]),
        now: (2026, 10, 17, 14, 5, 0),
    }
}

#[test]
fn every_element_is_drawn_and_fields_are_filled() {
    let mut design = StampDesign {
        width: 240.0,
        height: 120.0,
        background: Some([1.0, 1.0, 0.9]),
        elements: Vec::new(),
    };
    for kind in ["rect", "oval", "line", "polyline", "polygon", "curve"] {
        let mut e = StampElement::new(kind, 240.0, 120.0);
        e.opacity = 0.8;
        e.fill = Some([0.9, 0.9, 1.0]);
        design.elements.push(e);
    }
    design.elements.push(StampElement {
        kind: "image".into(),
        rect: [5.0, 5.0, 40.0, 20.0],
        image: png(),
        ..Default::default()
    });
    let field = |f: &str, fmt: &str, text: &str| StampElement {
        kind: "field".into(),
        field: f.into(),
        format: fmt.into(),
        text: text.into(),
        ..StampElement::new("field", 240.0, 120.0)
    };
    design.elements.push(StampElement {
        text: "PAID\nin full".into(),
        ..StampElement::new("text", 240.0, 120.0)
    });
    design.elements.push(field("date", "DD/MM/YYYY HH:mm", ""));
    design.elements.push(field("name", "", "by "));
    design.elements.push(field("counter", "4", "No. "));
    design.elements.push(field("custom", "", "Invoice"));
    design
        .elements
        .push(field("macros", "", "page {page} of {pages}"));
    assert_eq!(design.custom_fields(), ["Invoice"]);
    assert!(design.has_counter());

    let mut doc = Document::new_blank(2, 400.0, 400.0).unwrap();
    doc.add_designed_stamp(
        1,
        [50.0, 50.0, 290.0, 170.0],
        &design,
        &values(),
        "Ann",
        "Paid stamp",
    )
    .unwrap();
    let ap = appearance_on(&doc, 1);
    for text in [
        "(PAID)",
        "(in full)",
        "(17/10/2026 14:05)",
        "(by Ann Example)",
        "(No. 0007)",
        "(INV-42)",
        "(page 2 of 2)",
    ] {
        assert!(ap.contains(text), "{text} missing in {ap}");
    }
    assert!(ap.contains(" c ") && ap.contains(" re ") && ap.contains("Do"));
    let a = &doc.annotations(1).unwrap()[0];
    assert_eq!(a.subtype, "Stamp");
    assert!(a.contents.contains("INV-42"));
    doc.render_page_png(1, 1.0).unwrap();
    let svg = design.svg(&values());
    assert!(svg.contains("Ann Example") && svg.contains("<ellipse") && svg.contains("<image"));
}

fn appearance_on(doc: &Document, page: usize) -> String {
    let id = doc.annotations(page).unwrap()[0].id.unwrap();
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
    String::from_utf8_lossy(&s.decompressed_content().unwrap_or_else(|_| s.content.clone())).to_string()
}

#[test]
fn templates_and_cropping() {
    for name in TEMPLATES {
        let d = StampDesign::template(name).expect(name);
        assert!(!d.elements.is_empty());
        let mut doc = Document::new_blank(1, 300.0, 300.0).unwrap();
        doc.add_designed_stamp(
            0,
            [10.0, 10.0, 10.0 + d.width, 10.0 + d.height],
            &d,
            &values(),
            "Ann",
            name,
        )
        .unwrap();
    }
    assert!(StampDesign::template("nothing").is_none());
    let mut d = StampDesign::default();
    d.elements.push(StampElement {
        kind: "rect".into(),
        rect: [50.0, 30.0, 40.0, 20.0],
        line_width: 2.0,
        ..Default::default()
    });
    d.crop_to_content();
    assert_eq!((d.width, d.height), (48.0, 28.0));
    assert_eq!(d.elements[0].rect[0], 4.0);
    let empty = StampDesign::default();
    let mut doc = Document::new_blank(1, 300.0, 300.0).unwrap();
    assert!(
        doc.add_designed_stamp(0, [0.0, 0.0, 50.0, 50.0], &empty, &values(), "", "x")
            .is_err()
    );
}
