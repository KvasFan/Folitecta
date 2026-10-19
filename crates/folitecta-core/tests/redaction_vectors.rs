//! Redaction of vector paths and of the content of reused drawings (forms,
//! inline pictures, Type 3 text, hidden and clip-only text): every test
//! saves the result and reads the saved bytes again.

use folitecta_core::Document;
use folitecta_core::redaction::{
    PictureReason, RedactionMethod, RedactionOptions, SanitizeOptions, check_redacted_bytes,
};
use lopdf::{Dictionary, Object, Stream, dictionary};

mod redaction_util;
use redaction_util::*;

fn apply(doc: &mut Document) -> folitecta_core::redaction::RedactionReport {
    doc.apply_redactions_with(None, RedactionMethod::Content).unwrap()
}

fn mark(doc: &mut Document, page: usize, r: [f32; 4]) {
    doc.add_redaction(page, r, "Reviewer").unwrap();
}

fn px(img: &image::RgbaImage, x: u32, y: u32) -> [u8; 3] {
    let p = img.get_pixel(x, y);
    [p[0], p[1], p[2]]
}

fn near(a: [u8; 3], b: [u8; 3]) -> bool {
    (0..3).all(|k| (a[k] as i32 - b[k] as i32).abs() < 40)
}

#[test]
fn a_line_and_a_rectangle_under_a_mark_lose_their_geometry_there() {
    let mut doc = doc_with(
        400.0,
        400.0,
        &["0.9 g 0 0 400 400 re f 0 0 1 RG 4 w 50 200 m 350 200 l S 1 0 0 rg 100 100 200 50 re f"],
        |_| Dictionary::new(),
    );
    mark(&mut doc, 0, [150.0, 90.0, 250.0, 210.0]);
    let report = apply(&mut doc);
    assert_eq!(report.paths, 3, "{report:?}");
    assert!(report.pages_as_pictures.is_empty(), "{report:?}");
    assert!(report.verified);
    let bytes = saved(&mut doc, "lines");
    let p = painted(&bytes, 0);
    // The stroke keeps its parts outside the mark widened by half its width.
    let pieces: Vec<&Vec<Pt>> = p.strokes.iter().flat_map(|(s, _)| s.iter()).collect();
    assert_eq!(pieces.len(), 2, "{pieces:?}");
    let xs: Vec<(f64, f64)> = pieces.iter().map(|s| (s[0].0, s[s.len() - 1].0)).collect();
    assert!(
        (xs[0].0 - 50.0).abs() < 0.01 && (xs[0].1 - 148.0).abs() < 0.01,
        "{xs:?}"
    );
    assert!(
        (xs[1].0 - 252.0).abs() < 0.01 && (xs[1].1 - 350.0).abs() < 0.01,
        "{xs:?}"
    );
    for s in &pieces {
        assert!(s.iter().all(|q| !(q.0 > 148.01 && q.0 < 251.99)), "{s:?}");
    }
    let m = [150.0, 90.0, 250.0, 210.0];
    let red = [1.0, 0.0, 0.0];
    assert!(fill_area(&p, red, Some(m)) < 0.01);
    assert!((fill_area(&p, red, None) - 5000.0).abs() < 1.0);
    let bg = [0.9, 0.9, 0.9];
    assert!(fill_area(&p, bg, Some(m)) < 0.01);
    assert!((fill_area(&p, bg, None) - (160000.0 - 12000.0)).abs() < 1.0);
    let img = render(&bytes, 0);
    assert!(near(px(&img, 60, 200), [0, 0, 255]), "{:?}", px(&img, 60, 200));
    assert!(near(px(&img, 280, 275), [255, 0, 0]), "{:?}", px(&img, 280, 275));
    assert!(near(px(&img, 200, 250), [0, 0, 0]), "{:?}", px(&img, 200, 250));
    // The saved file passes the check.
    let mut d2 = Document::from_bytes(bytes.clone(), None, None).unwrap();
    let prepared = d2.prepare_save().unwrap();
    assert!(
        check_redacted_bytes(&prepared.bytes, None, &report)
            .unwrap()
            .passed()
    );
}

#[test]
fn a_rotated_matrix_keeps_its_cm_and_loses_only_the_part_under_the_mark() {
    let mut doc = doc_with(
        400.0,
        400.0,
        &["q 0.7071 0.7071 -0.7071 0.7071 200 200 cm -100 -5 200 10 re f Q"],
        |_| Dictionary::new(),
    );
    let before = painted(&saved(&mut doc, "rot0"), 0);
    let bar = before.fills[0].0[0].clone();
    let m = [180.0, 180.0, 220.0, 220.0];
    let inside = area(&clip(&bar, m)).abs();
    assert!(inside > 100.0);
    mark(&mut doc, 0, [180.0, 180.0, 220.0, 220.0]);
    let report = apply(&mut doc);
    assert!(report.pages_as_pictures.is_empty(), "{report:?}");
    let bytes = saved(&mut doc, "rot");
    let p = painted(&bytes, 0);
    assert!(p.operators.iter().any(|o| o == "cm"));
    let black = [0.0, 0.0, 0.0];
    assert!(fill_area(&p, black, Some(m)) < 0.01);
    assert!((fill_area(&p, black, None) - (area(&bar).abs() - inside)).abs() < 0.5);
}

/// A circle of four Bézier curves.
fn circle(cx: f64, cy: f64, r: f64) -> String {
    let k = 0.5523 * r;
    format!(
        "{} {} m {} {} {} {} {} {} c {} {} {} {} {} {} c {} {} {} {} {} {} c {} {} {} {} {} {} c S",
        cx + r,
        cy,
        cx + r,
        cy + k,
        cx + k,
        cy + r,
        cx,
        cy + r,
        cx - k,
        cy + r,
        cx - r,
        cy + k,
        cx - r,
        cy,
        cx - r,
        cy - k,
        cx - k,
        cy - r,
        cx,
        cy - r,
        cx + k,
        cy - r,
        cx + r,
        cy - k,
        cx + r,
        cy
    )
}

#[test]
fn a_stroked_curve_is_flattened_only_where_it_meets_a_mark() {
    let content = format!("{} {}", circle(300.0, 300.0, 50.0), circle(100.0, 100.0, 30.0));
    let mut doc = doc_with(400.0, 400.0, &[&content], |_| Dictionary::new());
    mark(&mut doc, 0, [240.0, 280.0, 270.0, 320.0]);
    let report = apply(&mut doc);
    assert!(report.pages_as_pictures.is_empty(), "{report:?}");
    let bytes = saved(&mut doc, "curve");
    let p = painted(&bytes, 0);
    // The other circle is still written with curves.
    assert!(p.operators.iter().filter(|o| *o == "c").count() >= 4);
    // No point of the cut circle inside the mark widened by half the line width.
    let wide = [239.5, 279.5, 270.5, 320.5];
    let near_big = |q: &Pt| (((q.0 - 300.0).powi(2) + (q.1 - 300.0).powi(2)).sqrt() - 50.0).abs() < 5.0;
    let mut length = 0.0;
    for (subs, _) in &p.strokes {
        for s in subs.iter().filter(|s| s.iter().all(near_big)) {
            for q in s {
                assert!(
                    !(q.0 > wide[0] + 0.01
                        && q.0 < wide[2] - 0.01
                        && q.1 > wide[1] + 0.01
                        && q.1 < wide[3] - 0.01),
                    "{q:?}"
                );
            }
            length += s
                .windows(2)
                .map(|w| ((w[1].0 - w[0].0).powi(2) + (w[1].1 - w[0].1).powi(2)).sqrt())
                .sum::<f64>();
        }
    }
    // The arc inside the widened mark, measured on the circle.
    let mut inside = 0.0;
    let steps = 100_000;
    for i in 0..steps {
        let a = i as f64 / steps as f64 * std::f64::consts::TAU;
        let (x, y) = (300.0 + 50.0 * a.cos(), 300.0 + 50.0 * a.sin());
        if x > wide[0] && x < wide[2] && y > wide[1] && y < wide[3] {
            inside += std::f64::consts::TAU * 50.0 / steps as f64;
        }
    }
    let expected = std::f64::consts::TAU * 50.0 - inside;
    assert!(
        (length - expected).abs() / expected < 0.01,
        "{length} vs {expected}"
    );
}

/// A gray picture of `w` × `h` pixels, all `v`.
fn gray(d: &mut lopdf::Document, w: i64, h: i64, v: u8) -> lopdf::ObjectId {
    d.add_object(Stream::new(
        dictionary! {
            "Type" => "XObject", "Subtype" => "Image", "Width" => w, "Height" => h,
            "ColorSpace" => "DeviceGray", "BitsPerComponent" => 8,
        },
        vec![v; (w * h) as usize],
    ))
}

#[test]
fn a_clip_under_a_mark_never_reveals_what_it_hid() {
    let mut doc = doc_with(
        400.0,
        400.0,
        &["q 140 140 20 20 re W n q 300 0 0 300 50 50 cm /Im Do Q Q"],
        |d| {
            let im = gray(d, 30, 30, 0);
            dictionary! { "XObject" => dictionary! { "Im" => im } }
        },
    );
    let before = render(&saved(&mut doc, "clip0"), 0);
    mark(&mut doc, 0, [100.0, 100.0, 200.0, 200.0]);
    let report = apply(&mut doc);
    assert!(report.pages_as_pictures.is_empty(), "{report:?}");
    let bytes = saved(&mut doc, "clip");
    let streams = streams_of(&bytes);
    assert!(
        streams.iter().any(|s| holds(s, b"-20000 -20000 m")),
        "an empty clip far away"
    );
    let after = render(&bytes, 0);
    // Outside the mark, what the clip hid stays hidden.
    for (x, y) in [(60, 60), (300, 300), (60, 340), (250, 150)] {
        assert_eq!(px(&before, x, y), px(&after, x, y), "({x}, {y})");
        assert!(near(px(&after, x, y), [255, 255, 255]));
    }
}

/// A form with text and a line, and a font for it.
fn shared_form(d: &mut lopdf::Document, text: &str) -> lopdf::ObjectId {
    let f = helvetica(d);
    d.add_object(Stream::new(
        dictionary! {
            "Type" => "XObject", "Subtype" => "Form", "BBox" => vec![0.into(), 0.into(), 400.into(), 400.into()],
            "Resources" => dictionary! { "Font" => dictionary! { "F1" => f } },
        },
        format!("BT /F1 12 Tf 50 300 Td ({text}) Tj ET 50 280 m 300 280 l S").into_bytes(),
    ))
}

#[test]
fn a_shared_form_loses_text_under_a_mark_on_one_page_only() {
    let mut doc = built(|d| {
        let fm = shared_form(d, "Secret plan for Monday");
        let res = dictionary! { "XObject" => dictionary! { "Fm" => fm } };
        vec![("/Fm Do".into(), res.clone()), ("/Fm Do".into(), res)]
    });
    let before = doc.text_layer(0).unwrap();
    let r = pdf_rect(&doc, 0, word_box(&before, "Secret"));
    mark(&mut doc, 0, r);
    let report = apply(&mut doc);
    assert!(report.pages_as_pictures.is_empty(), "{report:?}");
    assert_eq!((report.forms, report.glyphs), (1, 6), "{report:?}");
    let after = doc.text_layer(0).unwrap();
    assert!(!after.text.contains("Secret"), "{:?}", after.text);
    let (b, a) = (word_box(&before, "plan"), word_box(&after, "plan"));
    assert!((b[0] - a[0]).abs() < 0.5, "{b:?} -> {a:?}");
    assert!(doc.text_layer(1).unwrap().text.contains("Secret plan"));
    let bytes = saved(&mut doc, "shared");
    let lo = lopdf::Document::load_mem(&bytes).unwrap();
    let pages: Vec<_> = lo.get_pages().into_values().collect();
    let fm = |pid| {
        let res = page_resources_of(&lo, pid);
        let xo = res.get(b"XObject").unwrap().as_dict().unwrap().clone();
        xo.iter()
            .map(|(_, v)| v.as_reference().unwrap())
            .collect::<Vec<_>>()
    };
    let (f0, f1) = (fm(pages[0]), fm(pages[1]));
    assert_eq!(f0.len(), 1);
    assert_ne!(f0, f1);
    let holding = streams_of(&bytes).iter().filter(|s| holds(s, b"Secret")).count();
    assert_eq!(holding, 1, "only page 1's form holds the word");
    // No picture was added to page 0.
    let s0 = lo.get_object(f0[0]).unwrap().as_stream().unwrap();
    assert_eq!(s0.dict.get(b"Subtype").unwrap().as_name().unwrap(), b"Form");
}

fn page_resources_of(lo: &lopdf::Document, pid: lopdf::ObjectId) -> Dictionary {
    let d = lo.get_dictionary(pid).unwrap();
    match d.get(b"Resources").unwrap() {
        Object::Reference(r) => lo.get_dictionary(*r).unwrap().clone(),
        Object::Dictionary(d) => d.clone(),
        _ => Dictionary::new(),
    }
}

/// A document built with lopdf by `build`, which returns the content of each
/// page and its resources.
fn built(build: impl FnOnce(&mut lopdf::Document) -> Vec<(String, Dictionary)>) -> Document {
    let mut d = lopdf::Document::with_version("1.7");
    let pages_id = d.new_object_id();
    let pages = build(&mut d);
    let mut kids = Vec::new();
    for (content, res) in pages {
        let cid = d.add_object(Stream::new(dictionary! {}, content.into_bytes()));
        let pid = d.add_object(dictionary! {
            "Type" => "Page", "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 400.into(), 400.into()],
            "Contents" => cid, "Resources" => res,
        });
        kids.push(Object::Reference(pid));
    }
    let n = kids.len() as i64;
    d.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! { "Type" => "Pages", "Kids" => kids, "Count" => n }),
    );
    let cat = d.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    d.trailer.set("Root", cat);
    let mut out = Vec::new();
    d.save_to(&mut out).unwrap();
    Document::from_bytes(out, None, None).unwrap()
}

#[test]
fn a_form_drawn_twice_on_a_page_is_copied_for_the_drawing_under_the_mark() {
    let mut doc = built(|d| {
        let f = helvetica(d);
        let fm = d.add_object(Stream::new(
            dictionary! {
                "Type" => "XObject", "Subtype" => "Form", "BBox" => vec![0.into(), 0.into(), 200.into(), 50.into()],
                "Resources" => dictionary! { "Font" => dictionary! { "F1" => f } },
            },
            b"BT /F1 12 Tf 10 20 Td (Secret code) Tj ET".to_vec(),
        ));
        vec![(
            "q 1 0 0 1 0 300 cm /Fm Do Q q 1 0 0 1 0 100 cm /Fm Do Q".into(),
            dictionary! { "XObject" => dictionary! { "Fm" => fm } },
        )]
    });
    let layer = doc.text_layer(0).unwrap();
    let r = pdf_rect(&doc, 0, word_box_nth(&layer, "Secret", 0));
    mark(&mut doc, 0, r);
    let report = apply(&mut doc);
    assert!(report.pages_as_pictures.is_empty(), "{report:?}");
    let after = doc.text_layer(0).unwrap();
    assert_eq!(after.text.matches("Secret").count(), 1, "{:?}", after.text);
    let bytes = saved(&mut doc, "twice");
    let streams = streams_of(&bytes);
    let page = streams.iter().find(|s| holds(s, b"Do")).unwrap();
    let text = String::from_utf8_lossy(page);
    assert!(text.contains("/FoliRdF"), "{text}");
    assert!(text.contains("/Fm Do"), "{text}");
}

#[test]
fn nested_forms_are_redacted_through_their_chain() {
    let mut doc = built(|d| {
        let f = helvetica(d);
        let inner = d.add_object(Stream::new(
            dictionary! {
                "Type" => "XObject", "Subtype" => "Form", "BBox" => vec![0.into(), 0.into(), 200.into(), 100.into()],
                "Matrix" => vec![1.into(), 0.into(), 0.into(), 1.into(), 10.into(), 10.into()],
                "Resources" => dictionary! { "Font" => dictionary! { "F1" => f } },
            },
            b"BT /F1 10 Tf 5 50 Td (Nested secret) Tj ET 0 0 1 rg 5 5 60 20 re f".to_vec(),
        ));
        let outer = d.add_object(Stream::new(
            dictionary! {
                "Type" => "XObject", "Subtype" => "Form", "BBox" => vec![0.into(), 0.into(), 200.into(), 200.into()],
                "Matrix" => vec![2.into(), 0.into(), 0.into(), 2.into(), 0.into(), 0.into()],
                "Resources" => dictionary! { "XObject" => dictionary! { "In" => inner } },
            },
            b"/In Do".to_vec(),
        ));
        vec![(
            "/Out Do".into(),
            dictionary! { "XObject" => dictionary! { "Out" => outer } },
        )]
    });
    let layer = doc.text_layer(0).unwrap();
    let w = word_box(&layer, "secret");
    let r = pdf_rect(&doc, 0, w);
    mark(&mut doc, 0, r);
    // The blue rectangle below the word, partly under the mark too.
    mark(&mut doc, 0, [40.0, 20.0, 80.0, 40.0]);
    let report = apply(&mut doc);
    assert!(report.pages_as_pictures.is_empty(), "{report:?}");
    assert_eq!(report.forms, 2, "{report:?}");
    let after = doc.text_layer(0).unwrap();
    assert!(
        after.text.contains("Nested") && !after.text.contains("secret"),
        "{:?}",
        after.text
    );
    let bytes = saved(&mut doc, "nested");
    assert!(streams_of(&bytes).iter().all(|s| !holds(s, b"secret")));
    let p = painted(&bytes, 0);
    assert!(fill_area(&p, [0.0, 0.0, 1.0], Some([40.0, 20.0, 80.0, 40.0])) < 0.01);
    assert!(fill_area(&p, [0.0, 0.0, 1.0], None) > 100.0);
}

#[test]
fn a_form_drawing_itself_falls_back_to_a_picture() {
    let mut lo = lopdf::Document::with_version("1.7");
    let f = helvetica(&mut lo);
    let fm = lo.new_object_id();
    lo.objects.insert(
        fm,
        Object::Stream(Stream::new(
            dictionary! {
                "Type" => "XObject", "Subtype" => "Form", "BBox" => vec![0.into(), 0.into(), 400.into(), 400.into()],
                "Resources" => dictionary! { "XObject" => dictionary! { "Fm" => fm }, "Font" => dictionary! { "F1" => f } },
            },
            b"BT /F1 12 Tf 50 300 Td (Loop text) Tj ET /Fm Do".to_vec(),
        )),
    );
    let pages_id = lo.new_object_id();
    let cid = lo.add_object(Stream::new(dictionary! {}, b"/Fm Do".to_vec()));
    let pid = lo.add_object(dictionary! {
        "Type" => "Page", "Parent" => pages_id, "MediaBox" => vec![0.into(), 0.into(), 400.into(), 400.into()],
        "Contents" => cid, "Resources" => dictionary! { "XObject" => dictionary! { "Fm" => fm } },
    });
    lo.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! { "Type" => "Pages", "Kids" => vec![pid.into()], "Count" => 1 }),
    );
    let cat = lo.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    lo.trailer.set("Root", cat);
    let mut buf = Vec::new();
    lo.save_to(&mut buf).unwrap();
    let mut doc = Document::from_bytes(buf, None, None).unwrap();
    mark(&mut doc, 0, [40.0, 290.0, 120.0, 320.0]);
    let report = apply(&mut doc);
    assert_eq!(report.reasons, vec![(0, PictureReason::Nesting)], "{report:?}");
    assert_eq!(report.pages_as_pictures, vec![0]);
}

#[test]
fn inline_pictures_under_a_mark_are_covered() {
    let mut doc = doc_with(
        400.0,
        400.0,
        &["q 100 0 0 100 100 100 cm BI /W 2 /H 2 /CS /G /BPC 8 /F /AHx ID FFFFFFFF> EI Q"],
        |_| Dictionary::new(),
    );
    mark(&mut doc, 0, [100.0, 100.0, 150.0, 200.0]);
    let report = apply(&mut doc);
    assert_eq!(report.pictures, 1, "{report:?}");
    assert!(report.pages_as_pictures.is_empty(), "{report:?}");
    let bytes = saved(&mut doc, "inline");
    let doc2 = Document::from_bytes(bytes.clone(), None, None).unwrap();
    let lo = lopdf::Document::load_mem(&bytes).unwrap();
    let pid = *lo.get_pages().get(&1).unwrap();
    for sid in lo.get_page_contents(pid) {
        let s = lo.get_object(sid).unwrap().as_stream().unwrap();
        let data = s.decompressed_content().unwrap_or_else(|_| s.content.clone());
        assert!(!holds(&data, b"BI "), "{}", String::from_utf8_lossy(&data));
    }
    let px = doc2.image_pixels(0, 0).unwrap();
    assert_eq!(px.get_pixel(0, 0).0[..3], [0, 0, 0]);
    assert_eq!(px.get_pixel(1, 1).0[..3], [255, 255, 255]);
}

#[test]
fn inline_stencils_and_filter_chains_under_a_mark_are_covered() {
    // A stencil (painted where 0) and a picture with two filters.
    let data = {
        use std::io::Write;
        let mut e = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        e.write_all(&[200u8; 4]).unwrap();
        let z = e.finish().unwrap();
        z.iter().map(|b| format!("{b:02X}")).collect::<String>()
    };
    let content = format!(
        "q 100 0 0 100 50 50 cm BI /W 8 /H 1 /IM true ID \x00 EI Q 0 0 1 rg q 100 0 0 100 200 200 cm BI /W 2 /H 2 /CS /G /BPC 8 /F [/AHx /Fl] ID {data}> EI Q"
    );
    let mut doc = doc_with(400.0, 400.0, &[&content], |_| Dictionary::new());
    mark(&mut doc, 0, [50.0, 50.0, 300.0, 300.0]);
    let report = apply(&mut doc);
    assert_eq!(report.pictures, 2, "{report:?}");
    assert!(report.pages_as_pictures.is_empty(), "{report:?}");
}

#[test]
fn type3_text_under_a_mark_is_removed_and_the_rest_keeps_its_place() {
    let mut doc = built(|d| {
        let a = d.add_object(Stream::new(
            dictionary! {},
            b"600 0 0 0 500 700 d1 0 0 500 700 re f".to_vec(),
        ));
        let b = d.add_object(Stream::new(
            dictionary! {},
            b"600 0 0 0 500 700 d1 0 0 500 350 re f".to_vec(),
        ));
        let tu = d.add_object(Stream::new(
            dictionary! {},
            b"/CIDInit /ProcSet findresource begin 12 dict begin begincmap 2 beginbfchar <61> <0061> <62> <0062> endbfchar endcmap end end".to_vec(),
        ));
        let f = d.add_object(dictionary! {
            "Type" => "Font", "Subtype" => "Type3",
            "FontBBox" => vec![0.into(), 0.into(), 600.into(), 700.into()],
            "FontMatrix" => vec![0.001.into(), 0.into(), 0.into(), 0.001.into(), 0.into(), 0.into()],
            "CharProcs" => dictionary! { "a" => a, "b" => b },
            "Encoding" => dictionary! { "Type" => "Encoding", "Differences" => vec![97.into(), Object::Name(b"a".to_vec()), Object::Name(b"b".to_vec())] },
            "FirstChar" => 97, "LastChar" => 98, "Widths" => vec![600.into(), 600.into()],
            "ToUnicode" => tu, "Resources" => dictionary! {},
        });
        vec![(
            "BT /T3 20 Tf 50 300 Td (abab) Tj 0 -50 Td [(ab) -500 (ab)] TJ ET".into(),
            dictionary! { "Font" => dictionary! { "T3" => f } },
        )]
    });
    let before = doc.text_layer(0).unwrap();
    let origins =
        |l: &folitecta_core::PageText| -> Vec<[f32; 4]> { l.glyphs.iter().map(|g| g.bbox).collect() };
    let o_before = origins(&before);
    assert_eq!(o_before.len(), 8, "{:?}", before.text);
    // The first two glyphs of the first line: 50 .. 50 + 2 × 12.
    mark(&mut doc, 0, [50.5, 295.0, 72.0, 315.0]);
    let report = apply(&mut doc);
    assert!(report.pages_as_pictures.is_empty(), "{report:?}");
    assert_eq!(report.glyphs, 2, "{report:?}");
    let after = doc.text_layer(0).unwrap();
    let o_after = origins(&after);
    assert_eq!(o_after.len(), 6);
    for g in &o_after {
        assert!(
            o_before
                .iter()
                .any(|b| (b[0] - g[0]).abs() < 0.3 && (b[1] - g[1]).abs() < 0.3),
            "{g:?}"
        );
    }
}

/// A layer (OCG), off by default.
fn off_layer(d: &mut lopdf::Document) -> lopdf::ObjectId {
    d.add_object(dictionary! { "Type" => "OCG", "Name" => Object::string_literal("Hidden") })
}

#[test]
fn hidden_layer_and_clip_only_text_under_a_mark_is_removed() {
    let mut lo = lopdf::Document::with_version("1.7");
    let f = helvetica(&mut lo);
    let ocg = off_layer(&mut lo);
    let not = lo.add_object(
        dictionary! { "Type" => "OCMD", "VE" => vec![Object::Name(b"Not".to_vec()), ocg.into()] },
    );
    let all_off =
        lo.add_object(dictionary! { "Type" => "OCMD", "OCGs" => vec![ocg.into()], "P" => "AllOff" });
    let any_off =
        lo.add_object(dictionary! { "Type" => "OCMD", "OCGs" => vec![ocg.into()], "P" => "AnyOff" });
    let content = "BT /F1 12 Tf /OC /PB BDC 50 350 Td (Hidden secret) Tj EMC ET \
                   BT /F1 12 Tf 7 Tr 50 300 Td (Clipped secret) Tj ET \
                   BT /F1 12 Tf /OC /PAll BDC 50 250 Td (Alloff secret) Tj EMC ET \
                   BT /F1 12 Tf /OC /PAny BDC 50 200 Td (Anyoff secret) Tj EMC ET \
                   BT /F1 12 Tf /OC /PNot BDC 50 150 Td (Notve secret) Tj EMC ET";
    let pages_id = lo.new_object_id();
    let cid = lo.add_object(Stream::new(dictionary! {}, content.as_bytes().to_vec()));
    let pid = lo.add_object(dictionary! {
        "Type" => "Page", "Parent" => pages_id, "MediaBox" => vec![0.into(), 0.into(), 400.into(), 400.into()],
        "Contents" => cid,
        "Resources" => dictionary! {
            "Font" => dictionary! { "F1" => f },
            "Properties" => dictionary! { "PB" => ocg, "PAll" => all_off, "PAny" => any_off, "PNot" => not },
        },
    });
    lo.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! { "Type" => "Pages", "Kids" => vec![pid.into()], "Count" => 1 }),
    );
    let cat = lo.add_object(dictionary! {
        "Type" => "Catalog", "Pages" => pages_id,
        "OCProperties" => dictionary! { "OCGs" => vec![ocg.into()], "D" => dictionary! { "OFF" => vec![ocg.into()] } },
    });
    lo.trailer.set("Root", cat);
    let mut buf = Vec::new();
    lo.save_to(&mut buf).unwrap();
    let original = buf.clone();
    let mut doc = Document::from_bytes(buf, None, None).unwrap();
    // One mark over each line (the words "secret" sit at x 50 + width of the first word).
    for y in [345.0, 295.0, 245.0, 195.0, 145.0] {
        mark(&mut doc, 0, [40.0, y, 200.0, y + 15.0]);
    }
    let report = apply(&mut doc);
    assert!(report.pages_as_pictures.is_empty(), "{report:?}");
    assert!(report.verified);
    assert_eq!(report.glyphs, 13 + 14 + 13 + 13 + 12, "{report:?}");
    let bytes = saved(&mut doc, "hidden");
    for s in streams_of(&bytes) {
        assert!(!holds(&s, b"secret") && !holds(&s, &hex("secret")));
    }
    // The check of the original file finds the text under the marks.
    let check = check_redacted_bytes(&original, None, &report).unwrap();
    assert!(
        check
            .problems
            .contains(&folitecta_core::redaction::CheckProblem::Text { page: 0 }),
        "{check:?}"
    );
}

#[test]
fn gradients_and_masks_under_a_mark() {
    // (a) An axial gradient over the page: kept, clipped around the mark.
    let mut doc = built(|d| {
        let sh = d.add_object(dictionary! {
            "ShadingType" => 2, "ColorSpace" => "DeviceRGB",
            "Coords" => vec![0.into(), 0.into(), 400.into(), 0.into()],
            "Function" => dictionary! { "FunctionType" => 2, "Domain" => vec![0.into(), 1.into()], "C0" => vec![1.into(), 0.into(), 0.into()], "C1" => vec![0.into(), 0.into(), 1.into()], "N" => 1 },
            "Extend" => vec![true.into(), true.into()],
        });
        vec![(
            "/Sh sh".into(),
            dictionary! { "Shading" => dictionary! { "Sh" => sh } },
        )]
    });
    mark(&mut doc, 0, [150.0, 150.0, 250.0, 250.0]);
    let report = apply(&mut doc);
    assert!(report.pages_as_pictures.is_empty(), "{report:?}");
    let bytes = saved(&mut doc, "axial");
    assert!(streams_of(&bytes).iter().any(|s| holds(s, b"W n /Sh sh Q")));

    // An axial gradient with sampled data painted only under the mark goes.
    let mut doc = built(|d| {
        let f = d.add_object(Stream::new(
            dictionary! { "FunctionType" => 0, "Domain" => vec![0.into(), 1.into()], "Range" => vec![0.into(), 1.into()], "Size" => vec![8.into()], "BitsPerSample" => 8 },
            b"SAMPLEDX".to_vec(),
        ));
        let sh = d.add_object(dictionary! {
            "ShadingType" => 2, "ColorSpace" => "DeviceGray",
            "Coords" => vec![160.into(), 0.into(), 240.into(), 0.into()], "Function" => f,
        });
        vec![(
            "q 160 160 80 80 re W n /Sh sh Q".into(),
            dictionary! { "Shading" => dictionary! { "Sh" => sh } },
        )]
    });
    mark(&mut doc, 0, [150.0, 150.0, 250.0, 250.0]);
    let report = apply(&mut doc);
    assert!(report.pages_as_pictures.is_empty(), "{report:?}");
    let bytes = saved(&mut doc, "sampled");
    assert!(
        streams_of(&bytes).iter().all(|s| !holds(s, b"SAMPLEDX")),
        "the gradient and its data are gone"
    );

    // (b) A mesh gradient under the mark: the page becomes a picture.
    let mut doc = built(|d| {
        let sh = d.add_object(Stream::new(
            dictionary! { "ShadingType" => 4, "ColorSpace" => "DeviceGray", "BitsPerCoordinate" => 8, "BitsPerComponent" => 8, "BitsPerFlag" => 8, "Decode" => vec![0.into(), 400.into(), 0.into(), 400.into(), 0.into(), 1.into()] },
            vec![0, 0, 0, 0, 0, 255, 0, 0, 0, 255, 255, 0],
        ));
        vec![(
            "/Sh sh".into(),
            dictionary! { "Shading" => dictionary! { "Sh" => sh } },
        )]
    });
    mark(&mut doc, 0, [150.0, 150.0, 250.0, 250.0]);
    let report = apply(&mut doc);
    assert_eq!(report.reasons, vec![(0, PictureReason::Shading)], "{report:?}");

    // (c) A soft mask drawing text, active while a rectangle is filled under the mark.
    let mut doc = built(|d| {
        let f = helvetica(d);
        let g = d.add_object(Stream::new(
            dictionary! {
                "Type" => "XObject", "Subtype" => "Form", "BBox" => vec![0.into(), 0.into(), 400.into(), 400.into()],
                "Group" => dictionary! { "S" => "Transparency", "CS" => "DeviceGray" },
                "Resources" => dictionary! { "Font" => dictionary! { "F1" => f } },
            },
            b"BT /F1 40 Tf 160 190 Td (MASK) Tj ET".to_vec(),
        ));
        let gs = d.add_object(dictionary! { "Type" => "ExtGState", "SMask" => dictionary! { "Type" => "Mask", "S" => "Luminosity", "G" => g } });
        vec![(
            "q /M gs 0 0 1 rg 100 100 200 200 re f Q".into(),
            dictionary! { "ExtGState" => dictionary! { "M" => gs } },
        )]
    });
    mark(&mut doc, 0, [150.0, 150.0, 250.0, 250.0]);
    let report = apply(&mut doc);
    assert_eq!(report.reasons, vec![(0, PictureReason::SoftMask)], "{report:?}");
}

#[test]
fn many_marks_on_a_background_stay_bounded() {
    let mut doc = doc_with(600.0, 600.0, &["0.9 g 0 0 600 600 re f"], |_| Dictionary::new());
    for i in 0..10 {
        for j in 0..10 {
            let (x, y) = (20.0 + i as f32 * 55.0, 20.0 + j as f32 * 55.0);
            mark(&mut doc, 0, [x, y, x + 30.0, y + 30.0]);
        }
    }
    let report = apply(&mut doc);
    assert!(report.pages_as_pictures.is_empty(), "{report:?}");
    let bytes = saved(&mut doc, "many");
    assert!(bytes.len() < 400_000, "{}", bytes.len());
    let p = painted(&bytes, 0);
    let bg = [0.9, 0.9, 0.9];
    assert!((fill_area(&p, bg, None) - (360000.0 - 100.0 * 900.0)).abs() < 5.0);
}

#[test]
fn a_page_rotated_by_90_degrees_gets_its_lines_cut_in_page_space() {
    let mut doc = doc_with(400.0, 300.0, &["0 0 1 RG 2 w 50 150 m 350 150 l S"], |_| {
        Dictionary::new()
    });
    doc.rotate_pages(&[0], 90).unwrap();
    mark(&mut doc, 0, [150.0, 100.0, 250.0, 200.0]);
    let report = apply(&mut doc);
    assert!(report.pages_as_pictures.is_empty(), "{report:?}");
    let bytes = saved(&mut doc, "rot90");
    let p = painted(&bytes, 0);
    let pieces: Vec<&Vec<Pt>> = p.strokes.iter().flat_map(|(s, _)| s.iter()).collect();
    assert_eq!(pieces.len(), 2, "{pieces:?}");
    for s in pieces {
        assert!(s.iter().all(|q| !(q.0 > 149.0 && q.0 < 251.0)), "{s:?}");
    }
}

#[test]
fn a_stroke_next_to_a_mark_leaves_no_ink_inside_it() {
    // The centre line runs 3 points below the mark; the line is 10 points wide.
    let mut doc = doc_with(400.0, 400.0, &["0 0 1 RG 10 w 50 197 m 350 197 l S"], |_| {
        Dictionary::new()
    });
    mark(&mut doc, 0, [150.0, 200.0, 250.0, 260.0]);
    let report = apply(&mut doc);
    assert!(report.pages_as_pictures.is_empty(), "{report:?}");
    let img = render(&without_covers(&saved(&mut doc, "ink")), 0);
    // Display space: page y 200..260 is display 140..200.
    for x in (152..248).step_by(4) {
        for y in (142..198).step_by(4) {
            assert!(near(px(&img, x, y), [255, 255, 255]), "ink at ({x}, {y})");
        }
    }
    // Outside the mark the line is still there.
    assert!(near(px(&img, 100, 203), [0, 0, 255]));
}

#[test]
fn clip_mode_text_that_loses_every_glyph_still_hides_what_it_hid() {
    // Text in clip mode over a picture: only the letters show the picture.
    let mut doc = doc_with(
        400.0,
        400.0,
        &["q BT /F1 60 Tf 7 Tr 50 200 Td (HIDE) Tj ET q 400 0 0 400 0 0 cm /Im Do Q Q"],
        |d| {
            let f = helvetica(d);
            let im = gray(d, 10, 10, 0);
            dictionary! { "Font" => dictionary! { "F1" => f }, "XObject" => dictionary! { "Im" => im } }
        },
    );
    let before = render(&saved(&mut doc, "clipmode0"), 0);
    mark(&mut doc, 0, [40.0, 180.0, 230.0, 270.0]);
    let report = apply(&mut doc);
    assert!(report.pages_as_pictures.is_empty(), "{report:?}");
    let after = render(&saved(&mut doc, "clipmode"), 0);
    for (x, y) in [(20, 20), (300, 300), (380, 100), (20, 380)] {
        assert_eq!(px(&before, x, y), px(&after, x, y), "({x}, {y})");
    }
}

#[test]
fn a_form_using_the_text_state_of_its_page_keeps_its_glyphs_in_place() {
    let mut doc = built(|d| {
        let f = helvetica(d);
        let fm = d.add_object(Stream::new(
            dictionary! {
                "Type" => "XObject", "Subtype" => "Form", "BBox" => vec![0.into(), 0.into(), 400.into(), 400.into()],
                "Resources" => dictionary! { "Font" => dictionary! { "F1" => f } },
            },
            b"BT 50 300 Td (Keep) Tj ( Gone) Tj ET".to_vec(),
        ));
        vec![(
            "BT /F1 14 Tf 2 Tc 3 Tw ET /Fm Do BT /F1 14 Tf 50 200 Td (After) Tj ET".into(),
            dictionary! { "XObject" => dictionary! { "Fm" => fm }, "Font" => dictionary! { "F1" => f } },
        )]
    });
    let before = doc.text_layer(0).unwrap();
    let r = pdf_rect(&doc, 0, word_box(&before, "Gone"));
    mark(&mut doc, 0, r);
    let report = apply(&mut doc);
    assert!(report.pages_as_pictures.is_empty(), "{report:?}");
    let after = doc.text_layer(0).unwrap();
    assert!(!after.text.contains("Gone"));
    let (b, a) = (word_box(&before, "Keep"), word_box(&after, "Keep"));
    assert!((b[0] - a[0]).abs() < 0.3, "{b:?} {a:?}");
}

#[test]
fn text_in_a_font_the_renderer_cannot_place_sends_the_page_to_pictures() {
    // No /F9 font: the renderer shows ASCII only; these bytes are not shown.
    let mut doc = doc_with(400.0, 400.0, &["BT /F9 12 Tf 50 300 Td <C8E9> Tj ET"], |_| {
        Dictionary::new()
    });
    mark(&mut doc, 0, [40.0, 290.0, 100.0, 320.0]);
    let report = apply(&mut doc);
    assert_eq!(report.reasons, vec![(0, PictureReason::Font)], "{report:?}");
}

#[test]
fn a_path_split_across_two_streams_falls_back() {
    let mut doc = built(|_| vec![("0 0 0 RG 50 200 m".into(), Dictionary::new())]);
    // A second content stream ends the path.
    let pid = doc.page_id(0).unwrap();
    let mut lo = lopdf::Document::load_mem(&doc.bytes()).unwrap();
    let first = lo.get_page_contents(pid)[0];
    let second = lo.add_object(Stream::new(dictionary! {}, b"350 200 l S".to_vec()));
    lo.get_dictionary_mut(pid)
        .unwrap()
        .set("Contents", vec![first.into(), second.into()]);
    let mut buf = Vec::new();
    lo.save_to(&mut buf).unwrap();
    doc = Document::from_bytes(buf, None, None).unwrap();
    mark(&mut doc, 0, [150.0, 150.0, 250.0, 250.0]);
    let report = apply(&mut doc);
    assert_eq!(report.reasons, vec![(0, PictureReason::Content)], "{report:?}");
}

#[test]
fn pictures_with_a_soft_mask_are_covered_with_their_mask() {
    let mut doc = doc_with(400.0, 400.0, &["q 200 0 0 200 100 100 cm /Im Do Q"], |d| {
        let mask = gray(d, 20, 20, 128);
        let im = d.add_object(Stream::new(
            dictionary! {
                "Type" => "XObject", "Subtype" => "Image", "Width" => 20, "Height" => 20,
                "ColorSpace" => "DeviceGray", "BitsPerComponent" => 8, "SMask" => mask,
            },
            vec![255u8; 400],
        ));
        dictionary! { "XObject" => dictionary! { "Im" => im } }
    });
    mark(&mut doc, 0, [100.0, 100.0, 200.0, 300.0]);
    let report = apply(&mut doc);
    assert_eq!(report.pictures, 1, "{report:?}");
    let px = doc.image_pixels(0, 0).unwrap();
    // Covered: opaque black; the rest keeps its transparency.
    assert_eq!(px.get_pixel(2, 10).0, [0, 0, 0, 255]);
    assert_eq!(px.get_pixel(18, 10).0[3], 128);
}

#[test]
fn replacement_text_around_removed_content_goes() {
    let mut doc = built(|d| {
        let f = helvetica(d);
        let fm = d.add_object(Stream::new(
            dictionary! {
                "Type" => "XObject", "Subtype" => "Form", "BBox" => vec![0.into(), 0.into(), 400.into(), 400.into()],
                "Resources" => dictionary! { "Font" => dictionary! { "F1" => f } },
            },
            b"0 0 0 rg 50 300 100 20 re f".to_vec(),
        ));
        vec![(
            "/Span <</ActualText (Secret name) /MCID 0>> BDC /Fm Do EMC /Span /P1 BDC BT /F1 12 Tf 50 200 Td (Other) Tj ET EMC".into(),
            dictionary! {
                "XObject" => dictionary! { "Fm" => fm }, "Font" => dictionary! { "F1" => f },
                "Properties" => dictionary! { "P1" => dictionary! { "ActualText" => Object::string_literal("Classified text") } },
            },
        )]
    });
    mark(&mut doc, 0, [40.0, 290.0, 160.0, 330.0]);
    mark(&mut doc, 0, [40.0, 195.0, 120.0, 215.0]);
    let report = apply(&mut doc);
    assert!(report.pages_as_pictures.is_empty(), "{report:?}");
    let bytes = saved(&mut doc, "actual");
    let raw = String::from_utf8_lossy(&bytes).to_lowercase();
    assert!(
        !raw.contains("secret name") && !raw.contains("classified"),
        "replacement texts are gone"
    );
    for s in streams_of(&bytes) {
        assert!(!holds(&s, b"Secret name") && !holds(&s, b"Classified"));
    }
}

#[test]
fn methods_and_options_default_to_content_with_sanitizing() {
    let o = RedactionOptions::default();
    assert_eq!(o.method, RedactionMethod::Content);
    assert_eq!(o.sanitize, SanitizeOptions::default());
    assert!(
        o.sanitize.metadata
            && o.sanitize.attachments
            && o.sanitize.hidden
            && o.sanitize.quoting
            && o.sanitize.scripts
    );
}

#[test]
fn a_font_used_only_by_removed_text_leaves_the_page() {
    let mut doc = built(|d| {
        let f1 = helvetica(d);
        let tu = d.add_object(Stream::new(dictionary! {}, b"ONLYFORSECRET".to_vec()));
        let f2 = d.add_object(dictionary! {
            "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Courier", "Encoding" => "WinAnsiEncoding", "ToUnicode" => tu,
        });
        vec![(
            "BT /F1 12 Tf 50 300 Td (Public words) Tj ET BT /F2 12 Tf 50 200 Td (Hidden) Tj ET".into(),
            dictionary! { "Font" => dictionary! { "F1" => f1, "F2" => f2 } },
        )]
    });
    let layer = doc.text_layer(0).unwrap();
    let r = pdf_rect(&doc, 0, word_box(&layer, "Hidden"));
    mark(&mut doc, 0, r);
    let report = apply(&mut doc);
    assert!(report.pages_as_pictures.is_empty(), "{report:?}");
    let bytes = saved(&mut doc, "font");
    let lo = lopdf::Document::load_mem(&bytes).unwrap();
    let pid = *lo.get_pages().get(&1).unwrap();
    let res = page_resources_of(&lo, pid);
    let fonts = res.get(b"Font").unwrap().as_dict().unwrap();
    // F2 only sizes the move that replaced the word: a bare font now.
    let f2 = match fonts.get(b"F2").unwrap() {
        Object::Reference(r) => lo.get_dictionary(*r).unwrap().clone(),
        Object::Dictionary(d) => d.clone(),
        _ => panic!(),
    };
    assert!(!f2.has(b"ToUnicode") && !f2.has(b"FontDescriptor"));
    assert!(streams_of(&bytes).iter().all(|s| !holds(s, b"ONLYFORSECRET")));
    assert!(doc.page_text(0).contains("Public words"));
}

#[test]
fn a_pattern_whose_cell_draws_text_deep_inside_forms_sends_the_page_to_pictures() {
    let mut doc = built(|d| {
        let f = helvetica(d);
        // Text six forms deep inside the cell of a tiling pattern.
        let mut inner = d.add_object(Stream::new(
            dictionary! {
                "Type" => "XObject", "Subtype" => "Form", "BBox" => vec![0.into(), 0.into(), 50.into(), 50.into()],
                "Resources" => dictionary! { "Font" => dictionary! { "F1" => f } },
            },
            b"BT /F1 8 Tf 2 2 Td (deep) Tj ET".to_vec(),
        ));
        for _ in 0..6 {
            inner = d.add_object(Stream::new(
                dictionary! {
                    "Type" => "XObject", "Subtype" => "Form", "BBox" => vec![0.into(), 0.into(), 50.into(), 50.into()],
                    "Resources" => dictionary! { "XObject" => dictionary! { "X" => inner } },
                },
                b"/X Do".to_vec(),
            ));
        }
        let pat = d.add_object(Stream::new(
            dictionary! {
                "PatternType" => 1, "PaintType" => 1, "TilingType" => 1,
                "BBox" => vec![0.into(), 0.into(), 50.into(), 50.into()], "XStep" => 50, "YStep" => 50,
                "Resources" => dictionary! { "XObject" => dictionary! { "X" => inner } },
            },
            b"/X Do".to_vec(),
        ));
        vec![(
            "/Pattern cs /P1 scn 100 100 200 200 re f".into(),
            dictionary! { "Pattern" => dictionary! { "P1" => pat } },
        )]
    });
    mark(&mut doc, 0, [150.0, 150.0, 250.0, 250.0]);
    let report = apply(&mut doc);
    assert_eq!(report.reasons, vec![(0, PictureReason::Pattern)], "{report:?}");
}
