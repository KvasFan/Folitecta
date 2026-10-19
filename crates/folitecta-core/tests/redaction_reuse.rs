//! Redaction of what drawings share: patterns and Type 3 fonts used by
//! other paints, pages and forms, the masks of pictures and the pictures of
//! glyphs listed in the resources, and a long document. Every test saves the
//! result and reads the saved bytes again.

use std::time::{Duration, Instant};

use folitecta_core::Document;
use folitecta_core::redaction::{
    PictureReason, RedactionMethod, RedactionOptions, RedactionReport, SanitizeOptions, check_redacted_bytes,
};
use lopdf::{Dictionary, Object, ObjectId, Stream, dictionary};

mod redaction_util;
use redaction_util::*;

fn apply(doc: &mut Document) -> RedactionReport {
    doc.apply_redactions_with(None, RedactionMethod::Content).unwrap()
}

fn mark(doc: &mut Document, page: usize, r: [f32; 4]) {
    doc.add_redaction(page, r, "Reviewer").unwrap();
}

/// The saved bytes, after the check of what a save writes passed.
fn checked(doc: &mut Document, report: &RedactionReport, name: &str) -> Vec<u8> {
    let prepared = doc.prepare_save().unwrap();
    let check = check_redacted_bytes(&prepared.bytes, prepared.key.as_ref(), report).unwrap();
    assert!(check.passed(), "{name}: {check:?}");
    saved(doc, name)
}

fn n(s: &str) -> Object {
    Object::Name(s.as_bytes().to_vec())
}

/// Pixels that differ between two renders outside the marks (display space,
/// grown by two pixels).
fn changed_outside(a: &image::RgbaImage, b: &image::RgbaImage, marks: &[[f32; 4]]) -> usize {
    let mut n = 0;
    for y in 0..a.height().min(b.height()) {
        for x in 0..a.width().min(b.width()) {
            let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
            if marks
                .iter()
                .any(|m| fx > m[0] - 2.0 && fx < m[2] + 2.0 && fy > m[1] - 2.0 && fy < m[3] + 2.0)
            {
                continue;
            }
            let (p, q) = (a.get_pixel(x, y), b.get_pixel(x, y));
            if (0..3).any(|k| (p[k] as i32 - q[k] as i32).abs() > 40) {
                n += 1;
            }
        }
    }
    n
}

/// A mark of a 400 × 400 page in display space.
fn shown(r: [f32; 4]) -> [f32; 4] {
    [r[0], 400.0 - r[3], r[2], 400.0 - r[1]]
}

fn images(bytes: &[u8]) -> Vec<lopdf::Stream> {
    let doc = lopdf::Document::load_mem(bytes).unwrap();
    doc.objects
        .values()
        .filter_map(|o| o.as_stream().ok())
        .filter(|s| s.dict.get(b"Subtype").ok().and_then(|v| v.as_name().ok()) == Some(b"Image".as_slice()))
        .cloned()
        .collect()
}

// ---- Patterns ------------------------------------------------------------------------

/// A tiling pattern with one cell as large as the page: art A (under the
/// mark) and art B (elsewhere).
fn big_cell(d: &mut lopdf::Document) -> ObjectId {
    d.add_object(Stream::new(
        dictionary! {
            "Type" => "Pattern", "PatternType" => 1, "PaintType" => 1, "TilingType" => 1,
            "BBox" => vec![0.into(), 0.into(), 400.into(), 400.into()], "XStep" => 400, "YStep" => 400,
            "Resources" => dictionary! {},
        },
        b"0 0 0 rg 150.123457 300 m 160.765432 350 l 170.111111 300 l h f 300 300 m 310.987654 350 l 320 300 l h f"
            .to_vec(),
    ))
}

const ART_MARK: [f32; 4] = [130.0, 280.0, 190.0, 370.0];

#[test]
fn a_pattern_removed_with_its_paint_does_not_stay_for_another_paint_of_the_page() {
    // Two fills with one big cell: the one over art A goes whole, the other
    // still paints with the pattern, which holds art A.
    let mut doc = built(|d| {
        let pat = big_cell(d);
        vec![(
            "/Pattern cs /P1 scn 140 290 40 70 re f 290 290 40 70 re f".into(),
            dictionary! { "Pattern" => dictionary! { "P1" => pat } },
        )]
    });
    mark(&mut doc, 0, ART_MARK);
    let report = apply(&mut doc);
    assert_eq!(report.reasons, vec![(0, PictureReason::Pattern)], "{report:?}");
    let bytes = checked(&mut doc, &report, "pattern-two-fills");
    assert!(streams_of(&bytes).iter().all(|s| !holds(s, b"160.765432")));

    // The other paint is in a form of the page.
    let mut doc = built(|d| {
        let pat = big_cell(d);
        let fm = d.add_object(Stream::new(
            dictionary! {
                "Type" => "XObject", "Subtype" => "Form",
                "BBox" => vec![280.into(), 280.into(), 340.into(), 370.into()],
                "Resources" => dictionary! { "Pattern" => dictionary! { "P1" => pat } },
            },
            b"/Pattern cs /P1 scn 290 290 40 70 re f".to_vec(),
        ));
        vec![(
            "/Pattern cs /P1 scn 140 290 40 70 re f /Fm Do".into(),
            dictionary! { "Pattern" => dictionary! { "P1" => pat }, "XObject" => dictionary! { "Fm" => fm } },
        )]
    });
    mark(&mut doc, 0, ART_MARK);
    let report = apply(&mut doc);
    assert_eq!(report.reasons, vec![(0, PictureReason::Pattern)], "{report:?}");
    let bytes = checked(&mut doc, &report, "pattern-form");
    assert!(streams_of(&bytes).iter().all(|s| !holds(s, b"160.765432")));

    // A mesh shading: its triangles are data of the pattern.
    let mut doc = built(|d| {
        let mut data = Vec::new();
        let enc = |v: f64| (((v / 400.0) * 65535.0).round() as u16).to_be_bytes();
        for (x, y, c) in [
            (150.0, 300.0, [255u8, 0, 0]),
            (170.0, 300.0, [255, 0, 0]),
            (160.0, 350.0, [255, 0, 0]),
            (300.0, 300.0, [0, 0, 255]),
            (320.0, 300.0, [0, 0, 255]),
            (310.0, 350.0, [0, 0, 255]),
        ] {
            data.push(0u8);
            data.extend(enc(x));
            data.extend(enc(y));
            data.extend(c);
        }
        let sh = d.add_object(Stream::new(
            dictionary! {
                "ShadingType" => 4, "ColorSpace" => "DeviceRGB", "BitsPerCoordinate" => 16,
                "BitsPerComponent" => 8, "BitsPerFlag" => 8,
                "Decode" => vec![0.into(), 400.into(), 0.into(), 400.into(), 0.into(), 1.into(), 0.into(), 1.into(), 0.into(), 1.into()],
            },
            data,
        ));
        let pat = d.add_object(dictionary! { "Type" => "Pattern", "PatternType" => 2, "Shading" => sh });
        vec![(
            "/Pattern cs /P1 scn 140 290 40 70 re f 290 290 40 70 re f".into(),
            dictionary! { "Pattern" => dictionary! { "P1" => pat } },
        )]
    });
    mark(&mut doc, 0, ART_MARK);
    let report = apply(&mut doc);
    assert_eq!(report.reasons, vec![(0, PictureReason::Shading)], "{report:?}");
    let bytes = checked(&mut doc, &report, "pattern-mesh");
    let lo = lopdf::Document::load_mem(&bytes).unwrap();
    assert!(
        lo.objects
            .values()
            .filter_map(|o| o.as_stream().ok())
            .all(|s| !s.dict.has(b"ShadingType")),
        "the mesh stayed"
    );

    // A small motif the other fill shows whole: the pattern stays and the
    // page keeps its content.
    let mut doc = built(|d| {
        let pat = d.add_object(Stream::new(
            dictionary! {
                "Type" => "Pattern", "PatternType" => 1, "PaintType" => 1, "TilingType" => 1,
                "BBox" => vec![0.into(), 0.into(), 10.into(), 10.into()], "XStep" => 10, "YStep" => 10,
                "Resources" => dictionary! {},
            },
            b"0 0 0 rg 2 2 4 4 re f".to_vec(),
        ));
        vec![(
            "/Pattern cs /P1 scn 140 290 40 70 re f 290 290 40 70 re f".into(),
            dictionary! { "Pattern" => dictionary! { "P1" => pat } },
        )]
    });
    mark(&mut doc, 0, ART_MARK);
    let report = apply(&mut doc);
    assert!(report.pages_as_pictures.is_empty(), "{report:?}");
    let bytes = checked(&mut doc, &report, "pattern-motif");
    assert!(streams_of(&bytes).iter().any(|s| holds(s, b"2 2 4 4 re f")));
}

/// A tiling pattern of 100 × 100 cells drawing a small motif at 40–60.
fn motif(d: &mut lopdf::Document) -> ObjectId {
    d.add_object(Stream::new(
        dictionary! {
            "Type" => "Pattern", "PatternType" => 1, "PaintType" => 1, "TilingType" => 1,
            "BBox" => vec![0.into(), 0.into(), 100.into(), 100.into()], "XStep" => 100, "YStep" => 100,
            "Resources" => dictionary! {},
        },
        b"0 0 0 rg 40.123457 40 m 50.765432 60 l 60.111111 40 l h f".to_vec(),
    ))
}

#[test]
fn only_filled_rectangles_under_rectangular_clips_show_a_motif_elsewhere() {
    // The fill under the mark goes whole; what else paints with the pattern
    // must show a whole motif for the pattern to stay.
    let cases = [
        // An even-odd frame whose box holds whole cells, its ink none.
        ("frame", "250 250 140 140 re 260 260 120 120 re f*", false),
        // A rectangle under a triangular clip.
        (
            "triangle-clip",
            "q 250 250 m 390 250 l 320 390 l h W n 250 250 140 140 re f Q",
            false,
        ),
        // A rectangle under a rectangular clip: it shows whole motifs.
        (
            "rect-clip",
            "q 250 250 140 140 re W n 250 250 140 140 re f Q",
            true,
        ),
    ];
    for (name, kept, shown) in cases {
        let mut doc = built(|d| {
            let pat = motif(d);
            vec![(
                format!("/Pattern cs /P1 scn 130 230 40 40 re f {kept}"),
                dictionary! { "Pattern" => dictionary! { "P1" => pat } },
            )]
        });
        mark(&mut doc, 0, [120.0, 220.0, 180.0, 280.0]);
        let report = apply(&mut doc);
        let bytes = checked(&mut doc, &report, &format!("motif-{name}"));
        if shown {
            assert!(report.pages_as_pictures.is_empty(), "{name}: {report:?}");
        } else {
            assert_eq!(
                report.reasons,
                vec![(0, PictureReason::Pattern)],
                "{name}: {report:?}"
            );
            assert!(
                streams_of(&bytes).iter().all(|s| !holds(s, b"50.765432")),
                "{name}"
            );
        }
    }
}

/// A shading of `kind` (2 axial, 3 radial) with `function`, as a pattern.
fn gradient(d: &mut lopdf::Document, kind: i64, coords: Vec<Object>, function: Object) -> ObjectId {
    d.add_object(dictionary! {
        "ShadingType" => kind, "ColorSpace" => "DeviceRGB", "Coords" => coords,
        "Function" => function, "Extend" => vec![true.into(), true.into()],
    })
}

fn exponential(c0: [f32; 3], c1: [f32; 3]) -> Object {
    Object::Dictionary(dictionary! {
        "FunctionType" => 2, "Domain" => vec![0.into(), 1.into()], "N" => 1,
        "C0" => c0.iter().map(|v| Object::Real(*v)).collect::<Vec<_>>(),
        "C1" => c1.iter().map(|v| Object::Real(*v)).collect::<Vec<_>>(),
    })
}

#[test]
fn gradients_shown_elsewhere_are_proven_or_send_the_page_to_pictures() {
    // A radial gradient whose stitched function paints a band (radii 30 to
    // 60) only under the mark.
    let stitched = Object::Dictionary(dictionary! {
        "FunctionType" => 3, "Domain" => vec![0.into(), 1.into()],
        "Functions" => vec![
            exponential([1.0, 0.0, 0.0], [1.0, 0.0, 0.0]),
            exponential([0.123457, 0.654321, 0.987654], [0.123457, 0.654321, 0.987654]),
            exponential([0.0, 0.0, 1.0], [0.0, 0.0, 1.0]),
        ],
        "Bounds" => vec![Object::Real(0.2), Object::Real(0.4)],
        "Encode" => vec![0.into(), 1.into(), 0.into(), 1.into(), 0.into(), 1.into()],
    });
    let radial = |function: Object| {
        built(move |d| {
            let coords = vec![
                200.into(),
                200.into(),
                0.into(),
                200.into(),
                200.into(),
                150.into(),
            ];
            let sh = gradient(d, 3, coords, function);
            let pat = d.add_object(dictionary! { "Type" => "Pattern", "PatternType" => 2, "Shading" => sh });
            vec![(
                "/Pattern cs /P1 scn 0 0 400 400 re f".into(),
                dictionary! { "Pattern" => dictionary! { "P1" => pat } },
            )]
        })
    };
    let mut doc = radial(stitched);
    mark(&mut doc, 0, [130.0, 130.0, 270.0, 270.0]);
    let report = apply(&mut doc);
    assert_eq!(report.reasons, vec![(0, PictureReason::Shading)], "{report:?}");
    let bytes = checked(&mut doc, &report, "radial-stitched");
    assert!(absent(&bytes, "0.123457"));
    // One exponential function: what it paints under the mark follows from
    // what shows around it.
    let mut doc = radial(exponential([1.0, 0.0, 0.0], [0.0, 0.0, 1.0]));
    mark(&mut doc, 0, [130.0, 130.0, 270.0, 270.0]);
    let report = apply(&mut doc);
    assert!(report.pages_as_pictures.is_empty(), "{report:?}");
    checked(&mut doc, &report, "radial-exponential");

    // An axial gradient painted under a clip: what shows outside the mark is
    // known for a rectangular clip only.
    for (name, clip, edited) in [
        ("axial-triangle", "50 50 m 350 50 l 200 350 l h W n", false),
        ("axial-rect", "50 50 300 300 re W n", true),
    ] {
        let mut doc = built(|d| {
            let sh = gradient(
                d,
                2,
                vec![0.into(), 0.into(), 400.into(), 0.into()],
                exponential([1.0, 0.0, 0.0], [0.0, 0.0, 1.0]),
            );
            vec![(
                format!("q {clip} /Sh0 sh Q"),
                dictionary! { "Shading" => dictionary! { "Sh0" => sh } },
            )]
        });
        mark(&mut doc, 0, [150.0, 100.0, 250.0, 200.0]);
        let report = apply(&mut doc);
        if edited {
            assert!(report.pages_as_pictures.is_empty(), "{name}: {report:?}");
        } else {
            assert_eq!(
                report.reasons,
                vec![(0, PictureReason::Shading)],
                "{name}: {report:?}"
            );
        }
        checked(&mut doc, &report, name);
    }
}

#[test]
fn a_pattern_another_page_paints_with_stays_for_that_page() {
    // As with forms drawn on other pages, the pattern stays for the page that
    // still paints with it; the redacted page no longer lists it.
    let mut doc = built(|d| {
        let pat = big_cell(d);
        let res = dictionary! { "Pattern" => dictionary! { "P1" => pat } };
        vec![
            ("/Pattern cs /P1 scn 140 290 40 70 re f".into(), res.clone()),
            ("/Pattern cs /P1 scn 290 290 40 70 re f".into(), res),
        ]
    });
    let before = render(&doc.bytes(), 1);
    mark(&mut doc, 0, ART_MARK);
    let report = apply(&mut doc);
    assert!(report.pages_as_pictures.is_empty(), "{report:?}");
    let bytes = checked(&mut doc, &report, "pattern-other-page");
    assert_eq!(changed_outside(&before, &render(&bytes, 1), &[]), 0);
    let lo = lopdf::Document::load_mem(&bytes).unwrap();
    let first = lo.get_pages()[&1];
    let res = lo.get_dictionary(first).unwrap().get(b"Resources").unwrap();
    let res = match res {
        Object::Reference(r) => lo.get_dictionary(*r).unwrap(),
        o => o.as_dict().unwrap(),
    };
    assert!(
        res.get(b"Pattern")
            .ok()
            .and_then(|p| p.as_dict().ok())
            .is_none_or(|p| !p.has(b"P1"))
    );
}

// ---- Type 3 fonts ----------------------------------------------------------------------

/// Glyph `a` draws inner text (and box), `b` a box, `percent` a bar.
const PROC_A: &[u8] = b"500 0 0 0 500 700 d1 BT /F1 1 Tf (T3INNERSECRET) Tj ET 0 0 500 700 re f";
const LINE1: &str = "BT /T3 20 Tf 100 300 Td (abab) Tj ET\n";
const LINE2: &str = "BT /T3 20 Tf 100 100 Td (bb) Tj ET\n";
const LINE1_MARK: [f32; 4] = [95.0, 290.0, 145.0, 320.0];

fn differences() -> Object {
    Object::Dictionary(dictionary! { "Type" => "Encoding", "Differences" => vec![97.into(), n("a"), n("b")] })
}

fn with_base() -> Object {
    Object::Dictionary(dictionary! {
        "Type" => "Encoding", "BaseEncoding" => "WinAnsiEncoding",
        "Differences" => vec![97.into(), n("a"), n("b")],
    })
}

fn type3(d: &mut lopdf::Document, f1: ObjectId, enc: Object, extra: Dictionary, proc_a: &[u8]) -> ObjectId {
    let pa = d.add_object(Stream::new(dictionary! {}, proc_a.to_vec()));
    let pb = d.add_object(Stream::new(
        dictionary! {},
        b"500 0 0 0 500 700 d1 0 0 250 350 re f".to_vec(),
    ));
    let pp = d.add_object(Stream::new(
        dictionary! {},
        b"500 0 0 0 500 700 d1 0 0 500 100 re f".to_vec(),
    ));
    let mut res = dictionary! { "Font" => dictionary! { "F1" => f1 } };
    for (k, v) in extra.iter() {
        res.set(k.clone(), v.clone());
    }
    d.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "Type3",
        "FontBBox" => vec![0.into(), 0.into(), 500.into(), 700.into()],
        "FontMatrix" => vec![0.001.into(), 0.into(), 0.into(), 0.001.into(), 0.into(), 0.into()],
        "CharProcs" => dictionary! { "a" => pa, "b" => pb, "percent" => pp },
        "Encoding" => enc,
        "FirstChar" => 37, "LastChar" => 98, "Widths" => vec![Object::Integer(500); 62],
        "Resources" => res,
    })
}

#[test]
fn type3_procedures_of_removed_glyphs_leave_every_copy_of_the_font() {
    // A base encoding with differences naming the codes shown.
    let mut doc = built(|d| {
        let f1 = helvetica(d);
        let f = type3(d, f1, with_base(), dictionary! {}, PROC_A);
        vec![(
            format!("{LINE1}{LINE2}"),
            dictionary! { "Font" => dictionary! { "T3" => f, "F1" => f1 } },
        )]
    });
    mark(&mut doc, 0, LINE1_MARK);
    let report = apply(&mut doc);
    assert!(report.pages_as_pictures.is_empty(), "{report:?}");
    assert_eq!(report.type3_kept, 0);
    let bytes = checked(&mut doc, &report, "t3-base");
    assert!(absent(&bytes, "T3INNERSECRET"));

    // The same font on another page showing `b` only: that page draws as it
    // did, and the procedure of `a` leaves the font everywhere.
    let mut doc = built(|d| {
        let f1 = helvetica(d);
        let f = type3(d, f1, differences(), dictionary! {}, PROC_A);
        let res = dictionary! { "Font" => dictionary! { "T3" => f, "F1" => f1 } };
        vec![(format!("{LINE1}{LINE2}"), res.clone()), (LINE2.into(), res)]
    });
    let before = render(&doc.bytes(), 1);
    mark(&mut doc, 0, LINE1_MARK);
    let report = apply(&mut doc);
    assert!(report.pages_as_pictures.is_empty(), "{report:?}");
    let bytes = checked(&mut doc, &report, "t3-shared");
    assert!(absent(&bytes, "T3INNERSECRET"));
    assert_eq!(changed_outside(&before, &render(&bytes, 1), &[]), 0);

    // The same font in a form of the page that no mark touches.
    let mut doc = built(|d| {
        let f1 = helvetica(d);
        let f = type3(d, f1, differences(), dictionary! {}, PROC_A);
        let fm = d.add_object(Stream::new(
            dictionary! {
                "Type" => "XObject", "Subtype" => "Form",
                "BBox" => vec![0.into(), 0.into(), 400.into(), 400.into()],
                "Resources" => dictionary! { "Font" => dictionary! { "T3" => f } },
            },
            LINE2.as_bytes().to_vec(),
        ));
        vec![(
            format!("{LINE1}/Fm Do\n"),
            dictionary! { "Font" => dictionary! { "T3" => f, "F1" => f1 }, "XObject" => dictionary! { "Fm" => fm } },
        )]
    });
    let before = render(&doc.bytes(), 0);
    mark(&mut doc, 0, LINE1_MARK);
    let report = apply(&mut doc);
    assert!(report.pages_as_pictures.is_empty(), "{report:?}");
    let bytes = checked(&mut doc, &report, "t3-form");
    assert!(absent(&bytes, "T3INNERSECRET"));
    assert_eq!(
        changed_outside(&before, &render(&without_covers(&bytes), 0), &[shown(LINE1_MARK)]),
        0
    );

    // A picture of the font's resources drawn by the removed glyph only.
    let mut doc = built(|d| {
        let f1 = helvetica(d);
        let mut px = vec![77u8; 44];
        for r in 0..4 {
            px[r * 11..r * 11 + 11].copy_from_slice(b"T3IMGSECRET");
        }
        let im = d.add_object(Stream::new(
            dictionary! { "Type" => "XObject", "Subtype" => "Image", "Width" => 11, "Height" => 4, "ColorSpace" => "DeviceGray", "BitsPerComponent" => 8 },
            px,
        ));
        let f = type3(
            d,
            f1,
            differences(),
            dictionary! { "XObject" => dictionary! { "ImA" => im } },
            b"500 0 d0 q 500 0 0 700 0 0 cm /ImA Do Q",
        );
        vec![(
            format!("{LINE1}{LINE2}"),
            dictionary! { "Font" => dictionary! { "T3" => f, "F1" => f1 } },
        )]
    });
    mark(&mut doc, 0, LINE1_MARK);
    let report = apply(&mut doc);
    assert!(report.pages_as_pictures.is_empty(), "{report:?}");
    let bytes = checked(&mut doc, &report, "t3-img");
    assert!(absent(&bytes, "T3IMGSECRET"));
    assert!(images(&bytes).is_empty());
}

#[test]
fn a_type3_font_whose_codes_cannot_be_told_is_reported_or_sends_its_page_to_pictures() {
    // `%` has no difference and no name the redaction knows in the base
    // encoding. Another page shows it: the font stays whole there, and the
    // report says so.
    let mut doc = built(|d| {
        let f1 = helvetica(d);
        let f = type3(d, f1, with_base(), dictionary! {}, PROC_A);
        let res = dictionary! { "Font" => dictionary! { "T3" => f, "F1" => f1 } };
        vec![
            (format!("{LINE1}{LINE2}"), res.clone()),
            ("BT /T3 20 Tf 100 100 Td (%) Tj ET".into(), res),
        ]
    });
    mark(&mut doc, 0, LINE1_MARK);
    let report = apply(&mut doc);
    assert!(report.pages_as_pictures.is_empty(), "{report:?}");
    assert_eq!(report.type3_kept, 1, "{report:?}");
    checked(&mut doc, &report, "t3-unknown-elsewhere");

    // The redacted page shows it: the page becomes a picture.
    let mut doc = built(|d| {
        let f1 = helvetica(d);
        let f = type3(d, f1, with_base(), dictionary! {}, PROC_A);
        vec![(
            format!("{LINE1}BT /T3 20 Tf 100 100 Td (%) Tj ET"),
            dictionary! { "Font" => dictionary! { "T3" => f, "F1" => f1 } },
        )]
    });
    mark(&mut doc, 0, LINE1_MARK);
    let report = apply(&mut doc);
    assert_eq!(report.reasons, vec![(0, PictureReason::Font)], "{report:?}");
    assert_eq!(report.type3_kept, 0);
    let bytes = checked(&mut doc, &report, "t3-unknown-here");
    assert!(absent(&bytes, "T3INNERSECRET"));
}

// ---- What the resources list -----------------------------------------------------------

const WORD_MARK: [f32; 4] = [45.0, 340.0, 160.0, 365.0];

#[test]
fn masks_and_glyph_pictures_listed_in_the_resources_stay_with_what_draws_them() {
    // The soft mask of a picture is listed in the page resources too.
    let mut doc = built(|d| {
        let f1 = helvetica(d);
        let sm = d.add_object(Stream::new(
            dictionary! { "Type" => "XObject", "Subtype" => "Image", "Width" => 2, "Height" => 2, "ColorSpace" => "DeviceGray", "BitsPerComponent" => 8 },
            vec![0, 255, 255, 0],
        ));
        let im = d.add_object(Stream::new(
            dictionary! { "Type" => "XObject", "Subtype" => "Image", "Width" => 2, "Height" => 2, "ColorSpace" => "DeviceGray", "BitsPerComponent" => 8, "SMask" => sm },
            vec![0, 0, 0, 0],
        ));
        vec![(
            "q 200 0 0 200 150 50 cm /Im Do Q BT /F1 12 Tf 50 350 Td (SECRETWORD) Tj ET".into(),
            dictionary! { "Font" => dictionary! { "F1" => f1 }, "XObject" => dictionary! { "Im" => im, "Sm" => sm } },
        )]
    });
    let before = render(&doc.bytes(), 0);
    mark(&mut doc, 0, WORD_MARK);
    let report = apply(&mut doc);
    assert!(report.pages_as_pictures.is_empty(), "{report:?}");
    let bytes = checked(&mut doc, &report, "smask-listed");
    assert!(absent(&bytes, "SECRETWORD"));
    let lo = lopdf::Document::load_mem(&bytes).unwrap();
    let masked: Vec<lopdf::Stream> = images(&bytes)
        .into_iter()
        .filter(|s| s.dict.has(b"SMask"))
        .collect();
    assert_eq!(masked.len(), 1);
    let sm = masked[0].dict.get(b"SMask").unwrap().as_reference().unwrap();
    assert!(lo.get_object(sm).is_ok());
    assert_eq!(
        changed_outside(&before, &render(&without_covers(&bytes), 0), &[shown(WORD_MARK)]),
        0
    );

    // A Type 3 font without resources draws a picture the page lists.
    let mut doc = built(|d| {
        let f1 = helvetica(d);
        let im = d.add_object(Stream::new(
            dictionary! { "Type" => "XObject", "Subtype" => "Image", "Width" => 1, "Height" => 1, "ColorSpace" => "DeviceRGB", "BitsPerComponent" => 8 },
            vec![255, 0, 0],
        ));
        let pa = d.add_object(Stream::new(dictionary! {}, b"500 0 d0 0 0 250 350 re f".to_vec()));
        let pb = d.add_object(Stream::new(
            dictionary! {},
            b"500 0 d0 q 500 0 0 700 0 0 cm /ImG Do Q".to_vec(),
        ));
        let f = d.add_object(dictionary! {
            "Type" => "Font", "Subtype" => "Type3",
            "FontBBox" => vec![0.into(), 0.into(), 500.into(), 700.into()],
            "FontMatrix" => vec![0.001.into(), 0.into(), 0.into(), 0.001.into(), 0.into(), 0.into()],
            "CharProcs" => dictionary! { "a" => pa, "b" => pb },
            "Encoding" => differences(),
            "FirstChar" => 97, "LastChar" => 98, "Widths" => vec![500.into(), 500.into()],
        });
        vec![(
            "BT /T3 40 Tf 150 150 Td (bb) Tj ET BT /F1 12 Tf 50 350 Td (SECRETWORD) Tj ET".into(),
            dictionary! { "Font" => dictionary! { "F1" => f1, "T3" => f }, "XObject" => dictionary! { "ImG" => im } },
        )]
    });
    let before = render(&doc.bytes(), 0);
    mark(&mut doc, 0, WORD_MARK);
    let report = apply(&mut doc);
    assert!(report.pages_as_pictures.is_empty(), "{report:?}");
    let bytes = checked(&mut doc, &report, "t3-inherit");
    assert!(absent(&bytes, "SECRETWORD"));
    assert_eq!(images(&bytes).len(), 1);
    assert_eq!(
        changed_outside(&before, &render(&without_covers(&bytes), 0), &[shown(WORD_MARK)]),
        0
    );
}

// ---- A long document ------------------------------------------------------------------------

#[test]
fn a_long_document_sharing_its_drawings_is_redacted_and_checked_in_time() {
    let pages = 2000;
    let mut d = lopdf::Document::with_version("1.7");
    let f = helvetica(&mut d);
    let mut logo = String::new();
    for i in 0..2500 {
        logo.push_str(&format!(
            "{} 760 m {} 770 l {} 760 l h f\n",
            20 + i % 50,
            21 + i % 50,
            22 + i % 50
        ));
    }
    // A logo without resources of its own, drawn twice by a form that has
    // none either: both draw with the resources the pages share.
    let lg = d.add_object(Stream::new(
        dictionary! { "Type" => "XObject", "Subtype" => "Form", "BBox" => vec![0.into(), 750.into(), 600.into(), 800.into()] },
        logo.into_bytes(),
    ));
    let wrap = d.add_object(Stream::new(
        dictionary! { "Type" => "XObject", "Subtype" => "Form", "BBox" => vec![0.into(), 0.into(), 600.into(), 800.into()] },
        b"/Logo Do q 1 0 0 1 0 -20 cm /Logo Do Q".to_vec(),
    ));
    let shared = d.add_object(dictionary! {
        "Font" => dictionary! { "F1" => f }, "XObject" => dictionary! { "Logo" => lg, "Wrap" => wrap },
    });
    let pages_id = d.new_object_id();
    let mut kids = Vec::new();
    for i in 0..pages {
        let c = d.add_object(Stream::new(
            dictionary! {},
            format!("/Wrap Do BT /F1 12 Tf 100 600 Td (page {i} LONGSECRET) Tj ET").into_bytes(),
        ));
        kids.push(Object::Reference(d.add_object(dictionary! {
            "Type" => "Page", "Parent" => pages_id, "Contents" => c, "Resources" => shared,
            "MediaBox" => vec![0.into(), 0.into(), 600.into(), 800.into()],
        })));
    }
    d.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! { "Type" => "Pages", "Kids" => kids, "Count" => pages as i64 }),
    );
    let cat = d.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    d.trailer.set("Root", cat);
    let mut bytes = Vec::new();
    d.save_to(&mut bytes).unwrap();
    let mut doc = Document::from_bytes(bytes, None, None).unwrap();
    mark(&mut doc, 0, [95.0, 590.0, 400.0, 615.0]);
    let t = Instant::now();
    let report = doc
        .apply_redactions_opts(
            None,
            &RedactionOptions {
                method: RedactionMethod::Content,
                sanitize: SanitizeOptions::none(),
            },
        )
        .unwrap();
    let applied = t.elapsed();
    assert!(report.pages_as_pictures.is_empty(), "{report:?}");
    let t = Instant::now();
    let prepared = doc.prepare_save().unwrap();
    let check = check_redacted_bytes(&prepared.bytes, prepared.key.as_ref(), &report).unwrap();
    let checked_in = t.elapsed();
    assert!(check.passed(), "{check:?}");
    // The text of the redacted page and of the first others is searched for
    // quotes; the rest is reported as not searched.
    assert_eq!(check.quote_pages_skipped, pages - 100);
    let page0 = lopdf::Document::load_mem(&prepared.bytes).unwrap();
    let first = page0.get_pages()[&1];
    let content = page0.get_page_content(first);
    assert!(!holds(&content, b"LONGSECRET"));
    if !cfg!(debug_assertions) {
        assert!(applied < Duration::from_secs(10), "apply took {applied:?}");
        assert!(
            checked_in < Duration::from_secs(20),
            "the check took {checked_in:?}"
        );
    }
}
