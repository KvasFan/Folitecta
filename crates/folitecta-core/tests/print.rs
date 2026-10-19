//! Print files: built from the plan, checked, reopened and drawn. The test
//! documents are made here with lopdf (coloured pages, annotations with each
//! print flag, layers, protected files).

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

use folitecta_core::print::{
    self, Binding, BookletSides, BuildOptions, Content, Layout, NupOrder, Paper, PrintOptions,
    PrintPermission, PrintPlan, PrintSource, Sizing,
};
use folitecta_core::{Document, Error, RenderOpts, Renderer, SecurityPermissions};
use lopdf::{Dictionary, Object, ObjectId, Stream, dictionary};

const A4: (f32, f32) = (595.28, 841.89);

fn folder(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("folitecta-print-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A PDF of pages of the given sizes and contents; `tweak` adds to it.
fn pdf_with(
    pages: Vec<(f32, f32, String)>,
    tweak: impl FnOnce(&mut lopdf::Document, &[ObjectId]),
) -> Vec<u8> {
    let mut doc = lopdf::Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    let font = doc.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica", "Encoding" => "WinAnsiEncoding",
    });
    let mut kids = Vec::new();
    let mut ids = Vec::new();
    for (w, h, content) in pages {
        let c = doc.add_object(Stream::new(dictionary! {}, content.into_bytes()));
        let id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), w.into(), h.into()],
            "Contents" => c,
            "Resources" => dictionary! { "Font" => dictionary! { "F1" => font } },
        });
        kids.push(Object::Reference(id));
        ids.push(id);
    }
    let count = kids.len() as i64;
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! { "Type" => "Pages", "Kids" => kids, "Count" => count }),
    );
    let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", catalog);
    tweak(&mut doc, &ids);
    let mut out = Vec::new();
    doc.save_to(&mut out).unwrap();
    out
}

fn open(bytes: Vec<u8>) -> Document {
    Document::from_bytes(bytes, None, None).unwrap()
}

const COLOURS: [[f32; 3]; 4] = [[1.0, 0.0, 0.0], [0.0, 0.6, 0.0], [0.0, 0.0, 1.0], [1.0, 0.8, 0.0]];

/// A page filled with a colour, a black square at its top left and a line of
/// text in the shared font.
fn coloured(w: f32, h: f32, c: [f32; 3]) -> (f32, f32, String) {
    (
        w,
        h,
        format!(
            "{} {} {} rg 0 0 {w} {h} re f 0 g 0 {} 60 60 re f BT /F1 12 Tf 80 40 Td (Folitecta) Tj ET",
            c[0],
            c[1],
            c[2],
            h - 60.0
        ),
    )
}

fn coloured_doc(n: usize, w: f32, h: f32) -> Document {
    open(pdf_with(
        (0..n).map(|i| coloured(w, h, COLOURS[i % 4])).collect(),
        |_, _| {},
    ))
}

fn a4() -> Paper {
    print::paper_for_key("iso_a4_210x297mm").unwrap()
}

fn options(layout: Layout, paper: Paper) -> PrintOptions {
    PrintOptions {
        layout,
        paper,
        ..Default::default()
    }
}

fn source(doc: &Document) -> PrintSource {
    PrintSource::load(doc.print_seed()).unwrap()
}

fn build(src: &PrintSource, plan: &PrintPlan, content: Content) -> Arc<Vec<u8>> {
    let o = BuildOptions {
        content,
        compress: true,
    };
    let bytes = src
        .build(plan, 0..plan.sheets.len(), &o, &mut |_, _| true)
        .unwrap()
        .unwrap();
    let bytes = Arc::new(bytes);
    print::validate(&bytes, plan, 0..plan.sheets.len()).unwrap();
    bytes
}

fn plan_of(src: &PrintSource, pages: &[usize], opts: &PrintOptions) -> PrintPlan {
    print::plan(pages, src.geoms(), opts).unwrap()
}

/// The colour at a fraction of the width and height of a rendered sheet (from its top left).
fn pixel(bytes: &Arc<Vec<u8>>, sheet: usize, scale: f32, fx: f32, fy: f32) -> [u8; 3] {
    let img = Renderer::new(bytes.clone())
        .unwrap()
        .render_rgba(sheet, scale, RenderOpts::default())
        .unwrap();
    let (w, h) = img.dimensions();
    let x = ((w as f32 * fx) as u32).min(w - 1);
    let y = ((h as f32 * fy) as u32).min(h - 1);
    let p = img.get_pixel(x, y).0;
    [p[0], p[1], p[2]]
}

fn is(c: [u8; 3], want: [f32; 3]) -> bool {
    c.iter()
        .zip(want)
        .all(|(a, b)| (f32::from(*a) - b * 255.0).abs() <= 12.0)
}

const WHITE: [f32; 3] = [1.0, 1.0, 1.0];

#[test]
fn the_print_file_reopens_with_the_planned_sheets() {
    let doc = coloured_doc(4, A4.0, A4.1);
    let (revision, dirty) = (doc.revision(), doc.is_dirty());
    let src = source(&doc);
    let opts = options(
        Layout::NUp {
            per_sheet: 2,
            order: NupOrder::Horizontal,
            gutter: 0.0,
            borders: true,
            auto_rotate: true,
        },
        a4(),
    );
    let plan = plan_of(&src, &[0, 1, 2, 3], &opts);
    let bytes = build(&src, &plan, Content::DocumentAndComments);
    let path = folder("reopen").join("sheets.pdf");
    std::fs::write(&path, bytes.as_slice()).unwrap();
    let back = Document::open(&path).unwrap();
    assert_eq!(back.page_count(), 2);
    for p in 0..2 {
        let size = back.page_size(p).unwrap();
        assert!((size.media_box[2] - A4.0).abs() < 0.01 && (size.media_box[3] - A4.1).abs() < 0.01);
        let id = back.page_id(p).unwrap();
        assert!(!back.inner().get_dictionary(id).unwrap().has(b"Annots"));
    }
    // The font shared by the four pages is copied once.
    let fonts = back
        .inner()
        .objects
        .values()
        .filter(|o| o.as_dict().is_ok_and(|d| d.has_type(b"Font")))
        .count();
    assert_eq!(fonts, 1);
    // The document did not change.
    assert_eq!((doc.revision(), doc.is_dirty()), (revision, dirty));
}

#[test]
fn several_pages_per_sheet_go_to_their_places() {
    let doc = coloured_doc(4, A4.0, A4.1);
    let src = source(&doc);
    let nup = |per_sheet, order| {
        options(
            Layout::NUp {
                per_sheet,
                order,
                gutter: 0.0,
                borders: false,
                auto_rotate: true,
            },
            a4(),
        )
    };
    // Two pages: across the paper, page 1 on the left of the turned sheet,
    // which is the bottom half of the upright paper.
    let plan = plan_of(&src, &[0, 1], &nup(2, NupOrder::Horizontal));
    let bytes = build(&src, &plan, Content::Document);
    assert!(is(pixel(&bytes, 0, 0.25, 0.5, 0.75), COLOURS[0]));
    assert!(is(pixel(&bytes, 0, 0.25, 0.5, 0.25), COLOURS[1]));
    let plan = plan_of(&src, &[0, 1, 2, 3], &nup(4, NupOrder::Horizontal));
    let bytes = build(&src, &plan, Content::Document);
    assert!(is(pixel(&bytes, 0, 0.25, 0.25, 0.3), COLOURS[0]));
    assert!(is(pixel(&bytes, 0, 0.25, 0.75, 0.3), COLOURS[1]));
    assert!(is(pixel(&bytes, 0, 0.25, 0.25, 0.8), COLOURS[2]));
    assert!(is(pixel(&bytes, 0, 0.25, 0.75, 0.8), COLOURS[3]));
    let plan = plan_of(&src, &[0, 1, 2, 3], &nup(4, NupOrder::Vertical));
    let bytes = build(&src, &plan, Content::Document);
    assert!(is(pixel(&bytes, 0, 0.25, 0.25, 0.8), COLOURS[1]));
    assert!(is(pixel(&bytes, 0, 0.25, 0.75, 0.3), COLOURS[2]));
}

#[test]
fn a_booklet_starts_with_a_blank_half_and_page_one() {
    let doc = coloured_doc(3, A4.0, A4.1);
    let src = source(&doc);
    let opts = options(
        Layout::Booklet {
            binding: Binding::Left,
            sides: BookletSides::Both,
            sheets_per_booklet: 0,
            back_cover: false,
            gutter: 0.0,
        },
        a4(),
    );
    let plan = plan_of(&src, &[0, 1, 2], &opts);
    assert_eq!(plan.sheets.len(), 2);
    let bytes = build(&src, &plan, Content::Document);
    // The left half of the turned layout is the bottom half of the paper.
    assert!(is(pixel(&bytes, 0, 0.25, 0.5, 0.75), WHITE));
    assert!(is(pixel(&bytes, 0, 0.25, 0.5, 0.25), COLOURS[0]));
    // The back: page 2 on the left, page 3 on the right.
    assert!(is(pixel(&bytes, 1, 0.25, 0.5, 0.75), COLOURS[1]));
    assert!(is(pixel(&bytes, 1, 0.25, 0.5, 0.25), COLOURS[2]));
}

#[test]
fn poster_tiles_show_the_quarters_of_the_page() {
    let (w, h) = A4;
    let quarters = format!(
        "1 0 0 rg 0 {y} {x} {y} re f 0 0.6 0 rg {x} {y} {x} {y} re f 0 0 1 rg 0 0 {x} {y} re f 1 0.8 0 rg {x} 0 {x} {y} re f",
        x = w / 2.0,
        y = h / 2.0
    );
    let doc = open(pdf_with(vec![(w, h, quarters)], |_, _| {}));
    let src = source(&doc);
    let opts = options(
        Layout::Poster {
            scale_pct: 200.0,
            overlap: 0.0,
            cut_marks: true,
            labels: true,
        },
        a4(),
    );
    let plan = plan_of(&src, &[0], &opts);
    assert_eq!(plan.sheets.len(), 4);
    let bytes = build(&src, &plan, Content::Document);
    for (tile, colour) in COLOURS.iter().enumerate() {
        assert!(is(pixel(&bytes, tile, 0.25, 0.5, 0.5), *colour), "tile {tile}");
    }
}

#[test]
fn a_landscape_page_turns_with_its_top_to_the_left() {
    let doc = coloured_doc(1, 842.0, 595.0);
    let src = source(&doc);
    let plan = plan_of(&src, &[0], &options(Layout::default(), a4()));
    assert!(plan.sheets[0].landscape);
    let bytes = build(&src, &plan, Content::Document);
    // The black square of the top left corner is at the bottom left of the upright sheet.
    assert!(is(pixel(&bytes, 0, 0.25, 0.03, 0.97), [0.0, 0.0, 0.0]));
    assert!(is(pixel(&bytes, 0, 0.25, 0.97, 0.03), COLOURS[0]));
}

/// An appearance stream filling a 50 x 50 box with a colour.
fn appearance(doc: &mut lopdf::Document, c: [f32; 3]) -> ObjectId {
    doc.add_object(Stream::new(
        dictionary! { "Type" => "XObject", "Subtype" => "Form", "BBox" => vec![0.into(), 0.into(), 50.into(), 50.into()] },
        format!("{} {} {} rg 0 0 50 50 re f", c[0], c[1], c[2]).into_bytes(),
    ))
}

fn annot(
    doc: &mut lopdf::Document,
    page: ObjectId,
    subtype: &str,
    flags: i64,
    at: (f32, f32),
    c: [f32; 3],
) -> ObjectId {
    let ap = appearance(doc, c);
    let id = doc.add_object(dictionary! {
        "Type" => "Annot",
        "Subtype" => Object::Name(subtype.as_bytes().to_vec()),
        "F" => flags,
        "P" => page,
        "Rect" => vec![at.0.into(), at.1.into(), (at.0 + 50.0).into(), (at.1 + 50.0).into()],
        "AP" => dictionary! { "N" => ap },
    });
    add_annot(doc, page, id);
    id
}

fn add_annot(doc: &mut lopdf::Document, page: ObjectId, id: ObjectId) {
    let page = doc.get_dictionary_mut(page).unwrap();
    let mut list = page
        .get(b"Annots")
        .ok()
        .and_then(|a| a.as_array().ok())
        .cloned()
        .unwrap_or_default();
    list.push(Object::Reference(id));
    page.set("Annots", Object::Array(list));
}

const RED: [f32; 3] = [1.0, 0.0, 0.0];
const GREEN: [f32; 3] = [0.0, 1.0, 0.0];
const BLUE: [f32; 3] = [0.0, 0.0, 1.0];
const MAGENTA: [f32; 3] = [1.0, 0.0, 1.0];
const CYAN: [f32; 3] = [0.0, 1.0, 1.0];
const YELLOW: [f32; 3] = [1.0, 1.0, 0.0];
const GREY: [f32; 3] = [0.8, 0.8, 0.8];

#[test]
fn only_printable_annotations_print() {
    let mut ids = HashMap::new();
    let bytes = pdf_with(
        vec![(400.0, 400.0, "0.8 g 0 0 400 400 re f".into())],
        |d, pages| {
            let p = pages[0];
            ids.insert("print", annot(d, p, "Square", 4, (10.0, 10.0), RED));
            annot(d, p, "Square", 0, (100.0, 10.0), GREEN);
            annot(d, p, "Square", 6, (200.0, 10.0), BLUE);
            annot(d, p, "Square", 36, (300.0, 10.0), MAGENTA);
            annot(d, p, "Popup", 4, (10.0, 100.0), BLUE);
            annot(d, p, "Link", 4, (100.0, 100.0), BLUE);
            // A check box shown in its "On" state.
            let on = appearance(d, CYAN);
            let off = appearance(d, YELLOW);
            let w = d.add_object(dictionary! {
            "Type" => "Annot", "Subtype" => "Widget", "FT" => "Btn", "T" => Object::string_literal("check"),
            "F" => 4, "P" => p, "V" => "On", "AS" => "On",
            "Rect" => vec![200.into(), 100.into(), 250.into(), 150.into()],
            "AP" => dictionary! { "N" => dictionary! { "On" => on, "Off" => off } },
        });
            add_annot(d, p, w);
            // A check box without a state: its "Off" appearance.
            let on = appearance(d, CYAN);
            let off = appearance(d, YELLOW);
            let w2 = d.add_object(dictionary! {
            "Type" => "Annot", "Subtype" => "Widget", "FT" => "Btn", "T" => Object::string_literal("check2"),
            "F" => 4, "P" => p,
            "Rect" => vec![300.into(), 100.into(), 350.into(), 150.into()],
            "AP" => dictionary! { "N" => dictionary! { "On" => on, "Off" => off } },
        });
            add_annot(d, p, w2);
            let form =
                d.add_object(dictionary! { "Fields" => vec![Object::Reference(w), Object::Reference(w2)] });
            let root = d.trailer.get(b"Root").unwrap().as_reference().unwrap();
            d.get_dictionary_mut(root).unwrap().set("AcroForm", form);
        },
    );
    let doc = open(bytes);
    let paper = Paper::new("square", 400.0, 400.0);
    let actual = Layout::Single {
        sizing: Sizing::Actual,
        auto_rotate: false,
        centre: true,
        paper_per_page: false,
    };
    let src = source(&doc);
    let plan = plan_of(&src, &[0], &options(actual.clone(), paper.clone()));
    // At (x, y) of the page (PDF space, y up).
    let at = |b: &Arc<Vec<u8>>, x: f32, y: f32| pixel(b, 0, 1.0, x / 400.0, 1.0 - y / 400.0);
    let all = build(&src, &plan, Content::DocumentAndComments);
    assert!(is(at(&all, 35.0, 35.0), RED), "printable comment");
    assert!(is(at(&all, 125.0, 35.0), GREY), "not printable");
    assert!(is(at(&all, 225.0, 35.0), GREY), "printable but hidden");
    assert!(
        is(at(&all, 325.0, 35.0), MAGENTA),
        "print only, not shown on screen"
    );
    assert!(is(at(&all, 35.0, 125.0), GREY), "pop-ups never print");
    assert!(is(at(&all, 125.0, 125.0), GREY), "links never print");
    assert!(is(at(&all, 225.0, 125.0), CYAN), "the state of the check box");
    assert!(is(at(&all, 325.0, 125.0), YELLOW), "no state: its Off appearance");
    let document = build(&src, &plan, Content::Document);
    assert!(is(at(&document, 35.0, 35.0), GREY));
    assert!(is(at(&document, 325.0, 35.0), GREY));
    assert!(is(at(&document, 225.0, 125.0), CYAN));
    let fields = build(&src, &plan, Content::FormFieldsOnly);
    assert!(is(at(&fields, 125.0, 300.0), WHITE), "no page content");
    assert!(is(at(&fields, 35.0, 35.0), WHITE));
    assert!(is(at(&fields, 225.0, 125.0), CYAN));
    // Comments hidden by the filter of the view do not print.
    let mut seed = doc.print_seed();
    seed.hidden = HashSet::from([ids["print"]]);
    let filtered = PrintSource::load(seed).unwrap();
    let hidden = build(&filtered, &plan, Content::DocumentAndComments);
    assert!(is(at(&hidden, 35.0, 35.0), GREY));
}

#[test]
fn stale_field_appearances_are_rebuilt_when_the_form_asks() {
    let bytes = pdf_with(vec![(300.0, 200.0, String::new())], |d, pages| {
        let p = pages[0];
        let stale = d.add_object(Stream::new(
            dictionary! { "Type" => "XObject", "Subtype" => "Form", "BBox" => vec![0.into(), 0.into(), 200.into(), 40.into()] },
            b"1 0 0 rg 0 0 200 40 re f".to_vec(),
        ));
        let helv = d.add_object(dictionary! {
            "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica", "Encoding" => "WinAnsiEncoding",
        });
        let w = d.add_object(dictionary! {
            "Type" => "Annot", "Subtype" => "Widget", "FT" => "Tx", "T" => Object::string_literal("name"),
            "V" => Object::string_literal("Hello"), "DA" => Object::string_literal("/Helv 12 Tf 0 g"),
            "F" => 4, "P" => p,
            "Rect" => vec![50.into(), 50.into(), 250.into(), 90.into()],
            "AP" => dictionary! { "N" => stale },
        });
        add_annot(d, p, w);
        let form = d.add_object(dictionary! {
            "Fields" => vec![Object::Reference(w)],
            "NeedAppearances" => true,
            "DR" => dictionary! { "Font" => dictionary! { "Helv" => helv } },
            "DA" => Object::string_literal("/Helv 12 Tf 0 g"),
        });
        let root = d.trailer.get(b"Root").unwrap().as_reference().unwrap();
        d.get_dictionary_mut(root).unwrap().set("AcroForm", form);
    });
    let doc = open(bytes);
    let src = source(&doc);
    let plan = plan_of(
        &src,
        &[0],
        &options(
            Layout::Single {
                sizing: Sizing::Actual,
                auto_rotate: false,
                centre: true,
                paper_per_page: false,
            },
            Paper::new("p", 200.0, 300.0),
        ),
    );
    let out = build(&src, &plan, Content::Document);
    let img = Renderer::new(out)
        .unwrap()
        .render_rgba(0, 1.0, RenderOpts::default())
        .unwrap();
    let red = img
        .pixels()
        .filter(|p| p.0[0] > 200 && p.0[1] < 60 && p.0[2] < 60)
        .count();
    assert_eq!(red, 0, "the stale red appearance is replaced");
    let dark = img
        .pixels()
        .filter(|p| p.0[0] < 100 && p.0[1] < 100 && p.0[2] < 100)
        .count();
    assert!(dark > 20, "the value is drawn");
}

/// A document with one red square in a layer (`/OC /oc BDC`), the layer made by `make`.
fn layered(make: impl FnOnce(&mut lopdf::Document) -> (ObjectId, ObjectId, Vec<ObjectId>)) -> Document {
    open(pdf_with(
        vec![(100.0, 100.0, "/OC /oc BDC 1 0 0 rg 0 0 100 100 re f EMC".into())],
        |d, pages| {
            let (props, ocg_for_d, ocgs) = make(d);
            let page = d.get_dictionary_mut(pages[0]).unwrap();
            let mut res = page.get(b"Resources").unwrap().as_dict().unwrap().clone();
            res.set("Properties", dictionary! { "oc" => props });
            page.set("Resources", res);
            let refs: Vec<Object> = ocgs.iter().map(|i| Object::Reference(*i)).collect();
            let mut dd = Dictionary::new();
            dd.set("ON", refs.clone());
            dd.set(
                "AS",
                vec![Object::Dictionary(dictionary! {
                    "Event" => "Print", "OCGs" => vec![Object::Reference(ocg_for_d)], "Category" => vec![Object::Name(b"Print".to_vec())],
                })],
            );
            let props = dictionary! { "OCGs" => refs, "D" => dd };
            let root = d.trailer.get(b"Root").unwrap().as_reference().unwrap();
            d.get_dictionary_mut(root).unwrap().set("OCProperties", props);
        },
    ))
}

fn one_to_one() -> PrintOptions {
    options(
        Layout::Single {
            sizing: Sizing::Actual,
            auto_rotate: false,
            centre: true,
            paper_per_page: false,
        },
        Paper::new("p", 100.0, 100.0),
    )
}

#[test]
fn layers_print_in_their_print_state() {
    // A layer shown on the screen but not printed.
    let doc = layered(|d| {
        let ocg = d.add_object(dictionary! {
            "Type" => "OCG", "Name" => Object::string_literal("Screen only"),
            "Usage" => dictionary! { "Print" => dictionary! { "PrintState" => "OFF" } },
        });
        (ocg, ocg, vec![ocg])
    });
    let screen = Renderer::new(doc.bytes()).unwrap();
    let shown = screen
        .render_rgba(0, 1.0, RenderOpts::default())
        .unwrap()
        .get_pixel(50, 50)
        .0;
    assert!(is([shown[0], shown[1], shown[2]], RED));
    let src = source(&doc);
    let plan = plan_of(&src, &[0], &one_to_one());
    let out = build(&src, &plan, Content::Document);
    assert!(is(pixel(&out, 0, 1.0, 0.5, 0.5), WHITE));
    let back = lopdf::Document::load_mem(&out).unwrap();
    let props = back
        .catalog()
        .unwrap()
        .get(b"OCProperties")
        .unwrap()
        .as_dict()
        .unwrap()
        .clone();
    let d = props.get(b"D").unwrap().as_dict().unwrap();
    assert!(!d.has(b"AS"));
    assert_eq!(d.get(b"OFF").unwrap().as_array().unwrap().len(), 1);

    // A layer without a print state prints as the view shows it.
    let plain = layered(|d| {
        let ocg = d.add_object(dictionary! { "Type" => "OCG", "Name" => Object::string_literal("Notes") });
        (ocg, ocg, vec![ocg])
    });
    let src = source(&plain);
    let out = build(&src, &plan_of(&src, &[0], &one_to_one()), Content::Document);
    assert!(is(pixel(&out, 0, 1.0, 0.5, 0.5), RED));
    let layer = plain.layers()[0].id;
    let mut seed = plain.print_seed();
    seed.layers = HashMap::from([(layer, false)]);
    let src = PrintSource::load(seed).unwrap();
    let out = build(&src, &plan_of(&src, &[0], &one_to_one()), Content::Document);
    assert!(is(pixel(&out, 0, 1.0, 0.5, 0.5), WHITE));

    // A membership dictionary keeps its policy: "all off" with its layer on hides.
    let policy = layered(|d| {
        let ocg = d.add_object(dictionary! { "Type" => "OCG", "Name" => Object::string_literal("Draft") });
        let md = d.add_object(
            dictionary! { "Type" => "OCMD", "OCGs" => vec![Object::Reference(ocg)], "P" => "AllOff" },
        );
        (md, ocg, vec![ocg])
    });
    let src = source(&policy);
    let out = build(&src, &plan_of(&src, &[0], &one_to_one()), Content::Document);
    assert!(is(pixel(&out, 0, 1.0, 0.5, 0.5), WHITE));
}

/// The `/P` permissions of an encrypted file.
fn permissions_of(bytes: &[u8]) -> i64 {
    let doc = lopdf::Document::load_mem(bytes).unwrap();
    let dict = match &doc.encryption_state {
        Some(state) => state.encode().ok(),
        None => doc.get_encrypted().ok().cloned(),
    };
    dict.and_then(|d| d.get(b"P").ok().and_then(|p| p.as_i64().ok()))
        .unwrap_or(0)
}

#[test]
fn print_files_of_protected_documents_stay_protected() {
    let dir = folder("protected");
    let original = dir.join("protected.pdf");
    let mut blank = Document::new_blank(2, 300.0, 300.0).unwrap();
    blank.add_text_watermark(&[0, 1], &Default::default()).unwrap();
    blank
        .save_encrypted(&original, "user-pw", "owner-pw", SecurityPermissions::default())
        .unwrap();
    let doc = Document::open_with_password(&original, "user-pw").unwrap();
    assert_eq!(doc.print_permission(), PrintPermission::Allowed);
    let src = source(&doc);
    assert!(src.is_protected());
    let plan = plan_of(&src, &[0, 1], &options(Layout::default(), a4()));
    // The plain file is checked, then encrypted as the last step.
    let plain = build(&src, &plan, Content::DocumentAndComments);
    let saved = src.protect(&plain).unwrap();
    let path = dir.join("print.pdf");
    folitecta_core::document::write_pdf_file(&path, &saved).unwrap();
    assert!(matches!(Document::open(&path), Err(Error::PasswordRequired)));
    assert_eq!(
        Document::open_with_password(&path, "user-pw")
            .unwrap()
            .page_count(),
        2
    );
    assert_eq!(
        Document::open_with_password(&path, "owner-pw")
            .unwrap()
            .page_count(),
        2
    );
    assert_eq!(
        permissions_of(&saved),
        permissions_of(&std::fs::read(&original).unwrap())
    );
    // Unprotected sources are saved as built.
    let free = coloured_doc(1, 200.0, 200.0);
    let free_src = source(&free);
    assert_eq!(
        free_src.protect(b"%PDF-1.7 x %%EOF").unwrap(),
        b"%PDF-1.7 x %%EOF"
    );
}

#[test]
fn print_permissions_follow_the_security_settings() {
    let dir = folder("permissions");
    let path = dir.join("no-print.pdf");
    let no_print = SecurityPermissions {
        print: false,
        ..SecurityPermissions::default()
    };
    Document::new_blank(1, 200.0, 200.0)
        .unwrap()
        .save_encrypted(&path, "u", "o", no_print)
        .unwrap();
    let doc = Document::open_with_password(&path, "u").unwrap();
    assert_eq!(doc.print_permission(), PrintPermission::Forbidden);
    assert_eq!(source(&doc).permission(), PrintPermission::Forbidden);

    // Printing allowed at low quality only (revision 4).
    use lopdf::encryption::crypt_filters::{Aes128CryptFilter, CryptFilter};
    let mut copy = lopdf::Document::load_mem(&Document::new_blank(1, 200.0, 200.0).unwrap().bytes()).unwrap();
    let id = Object::String(vec![7; 16], lopdf::StringFormat::Hexadecimal);
    copy.trailer.set("ID", Object::Array(vec![id.clone(), id]));
    let mut filters: std::collections::BTreeMap<Vec<u8>, Arc<dyn CryptFilter>> = Default::default();
    filters.insert(b"StdCF".to_vec(), Arc::new(Aes128CryptFilter));
    let state = lopdf::EncryptionState::try_from(lopdf::EncryptionVersion::V4 {
        document: &copy,
        encrypt_metadata: true,
        crypt_filters: filters,
        stream_filter: b"StdCF".to_vec(),
        string_filter: b"StdCF".to_vec(),
        owner_password: "o",
        user_password: "u",
        permissions: lopdf::Permissions::PRINTABLE,
    })
    .unwrap();
    copy.encrypt(&state).unwrap();
    let low = dir.join("low.pdf");
    let mut out = Vec::new();
    copy.save_to(&mut out).unwrap();
    std::fs::write(&low, out).unwrap();
    let doc = Document::open_with_password(&low, "u").unwrap();
    assert_eq!(doc.print_permission(), PrintPermission::LowQuality);
    assert_eq!(
        coloured_doc(1, 100.0, 100.0).print_permission(),
        PrintPermission::Allowed
    );
}

#[test]
fn several_content_streams_are_joined_and_unreadable_ones_refused() {
    let split = |second: Stream| {
        open(pdf_with(vec![(100.0, 100.0, "1 0 0 rg".into())], |d, pages| {
            let first = d.get_page_contents(pages[0])[0];
            let s2 = d.add_object(second);
            d.get_dictionary_mut(pages[0])
                .unwrap()
                .set("Contents", vec![Object::Reference(first), Object::Reference(s2)]);
        }))
    };
    let doc = split(Stream::new(dictionary! {}, b"0 0 100 100 re f".to_vec()));
    let src = source(&doc);
    let plan = plan_of(&src, &[0], &one_to_one());
    let out = build(&src, &plan, Content::Document);
    assert!(is(pixel(&out, 0, 1.0, 0.5, 0.5), RED));
    let bad = split(Stream::new(
        dictionary! { "Filter" => "FlateDecode" },
        b"this is not compressed data".to_vec(),
    ));
    let src = source(&bad);
    let plan = plan_of(&src, &[0], &one_to_one());
    let err = src
        .build(
            &plan,
            0..1,
            &BuildOptions {
                content: Content::Document,
                compress: true,
            },
            &mut |_, _| true,
        )
        .unwrap_err();
    assert!(err.to_string().contains("cannot be read"), "{err}");
}

#[test]
fn cancelled_builds_give_nothing_and_damaged_files_fail_the_check() {
    let doc = coloured_doc(3, A4.0, A4.1);
    let src = source(&doc);
    let plan = plan_of(&src, &[0, 1, 2], &options(Layout::default(), a4()));
    let o = BuildOptions {
        content: Content::Document,
        compress: true,
    };
    let mut calls = 0;
    let cancelled = src
        .build(&plan, 0..3, &o, &mut |done, _| {
            calls += 1;
            done < 1
        })
        .unwrap();
    assert!(cancelled.is_none());
    assert_eq!(calls, 2);
    let good = build(&src, &plan, Content::Document);
    // One sheet of the three: only that one.
    let one = src.build(&plan, 1..2, &o, &mut |_, _| true).unwrap().unwrap();
    print::validate(&Arc::new(one), &plan, 1..2).unwrap();
    // A truncated file.
    let cut = Arc::new(good[..good.len() / 2].to_vec());
    assert!(print::validate(&cut, &plan, 0..3).is_err());
    // A sheet whose content cannot be decoded.
    let mut damaged = lopdf::Document::load_mem(&good).unwrap();
    let first = *damaged.get_pages().values().next().unwrap();
    let cid = damaged.get_page_contents(first)[0];
    if let Ok(Object::Stream(s)) = damaged.get_object_mut(cid) {
        s.dict.set("Filter", "FlateDecode");
        s.set_content(b"garbage that is no deflate stream".to_vec());
    }
    let mut bytes = Vec::new();
    damaged.save_to(&mut bytes).unwrap();
    let err = print::validate(&Arc::new(bytes), &plan, 0..3).unwrap_err();
    assert!(err.to_string().contains("cannot be decoded"), "{err}");
    // Another paper than planned.
    let other = options(
        Layout::default(),
        print::paper_for_key("na_letter_8.5x11in").unwrap(),
    );
    let letter = plan_of(&src, &[0, 1, 2], &other);
    assert!(print::validate(&good, &letter, 0..3).is_err());
}

#[test]
fn sheets_print_as_pictures_of_the_same_size() {
    let doc = coloured_doc(2, A4.0, A4.1);
    let src = source(&doc);
    let plan = plan_of(&src, &[0, 1], &options(Layout::default(), a4()));
    let vector = build(&src, &plan, Content::Document);
    for gray in [false, true] {
        let raster = print::sheets_as_images(vector.clone(), 50.0, gray, &mut |_, _| true)
            .unwrap()
            .unwrap();
        let back = lopdf::Document::load_mem(&raster).unwrap();
        assert_eq!(back.get_pages().len(), 2);
        let images = back
            .objects
            .values()
            .filter_map(|o| o.as_stream().ok())
            .filter(|s| s.dict.get(b"Subtype").ok().and_then(|t| t.as_name().ok()) == Some(b"Image"))
            .count();
        assert_eq!(images, 2);
        let raster = Arc::new(raster);
        print::validate(&raster, &plan, 0..2).unwrap();
        let c = pixel(&raster, 0, 0.25, 0.5, 0.5);
        if gray {
            assert!(c[0] == c[1] && c[1] == c[2]);
        } else {
            assert!(is(c, COLOURS[0]));
        }
    }
    let mut calls = 0;
    let stopped = print::sheets_as_images(vector.clone(), 50.0, false, &mut |_, _| {
        calls += 1;
        false
    })
    .unwrap();
    assert!(stopped.is_none() && calls == 1);
    assert!(print::sheets_as_images(vector, 5.0, false, &mut |_, _| true).is_err());
}

/// How long a long document takes: the plan, the print file of every sheet
/// with its check, and one sheet of sixteen pages (the preview).
/// `cargo test --release -p folitecta-core --test print timing -- --ignored --nocapture`
#[test]
#[ignore]
fn timing_of_a_long_document() {
    use std::time::Instant;
    // 500 pages, each with its own picture of 600 x 800 pixels.
    let mut pages = Vec::new();
    for i in 0..500 {
        pages.push((
            A4.0,
            A4.1,
            format!("q 500 0 0 700 40 70 cm /Im{i} Do Q BT /F1 12 Tf 50 30 Td (Page {i}) Tj ET"),
        ));
    }
    let bytes = pdf_with(pages, |d, ids| {
        for (i, id) in ids.iter().enumerate() {
            let pixels: Vec<u8> = (0..600 * 800 * 3)
                .map(|k| ((k * 7 + i * 13) % 251) as u8)
                .collect();
            let mut enc = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast());
            std::io::Write::write_all(&mut enc, &pixels).unwrap();
            let img = d.add_object(Stream::new(
                dictionary! { "Type" => "XObject", "Subtype" => "Image", "Width" => 600, "Height" => 800,
                "ColorSpace" => "DeviceRGB", "BitsPerComponent" => 8, "Filter" => "FlateDecode" },
                enc.finish().unwrap(),
            ));
            let page = d.get_dictionary_mut(*id).unwrap();
            let mut res = page.get(b"Resources").unwrap().as_dict().unwrap().clone();
            let mut xo = Dictionary::new();
            xo.set(format!("Im{i}"), img);
            res.set("XObject", xo);
            page.set("Resources", res);
        }
    });
    eprintln!("document: {} MB", bytes.len() / 1_000_000);
    let doc = open(bytes);
    let t = Instant::now();
    let src = source(&doc);
    eprintln!("read for printing: {:?}", t.elapsed());
    let all: Vec<usize> = (0..500).collect();
    let t = Instant::now();
    let plan = plan_of(&src, &all, &options(Layout::default(), a4()));
    eprintln!("plan of 500 sheets: {:?}", t.elapsed());
    let t = Instant::now();
    let pdf = src
        .build(
            &plan,
            0..500,
            &BuildOptions {
                content: Content::DocumentAndComments,
                compress: true,
            },
            &mut |_, _| true,
        )
        .unwrap()
        .unwrap();
    eprintln!("print file: {:?}, {} MB", t.elapsed(), pdf.len() / 1_000_000);
    let pdf = Arc::new(pdf);
    let t = Instant::now();
    print::validate(&pdf, &plan, 0..500).unwrap();
    eprintln!("check: {:?}", t.elapsed());
    let nup = options(
        Layout::NUp {
            per_sheet: 16,
            order: NupOrder::Horizontal,
            gutter: 0.0,
            borders: false,
            auto_rotate: true,
        },
        a4(),
    );
    let plan = plan_of(&src, &all, &nup);
    let t = Instant::now();
    let one = src
        .build(
            &plan,
            0..1,
            &BuildOptions {
                content: Content::DocumentAndComments,
                compress: false,
            },
            &mut |_, _| true,
        )
        .unwrap()
        .unwrap();
    let png = Renderer::new(Arc::new(one))
        .unwrap()
        .render_page(0, 900.0 / A4.1)
        .unwrap();
    eprintln!(
        "preview of 16 pages on a sheet: {:?} ({}x{})",
        t.elapsed(),
        png.width,
        png.height
    );
}
