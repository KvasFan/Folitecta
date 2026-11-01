//! Text in form fields beyond WinAnsi: drawn with the embedded font, the
//! standard font left as it was for the rest; values it cannot draw stay as
//! typed, for readers that can.

use folitecta_core::Document;
use folitecta_core::testdocs::{Builder, form_doc};
use lopdf::{Object, ObjectId, dictionary};

/// The names of the fonts of the normal appearance of `widget`.
fn fonts(doc: &Document, widget: ObjectId) -> Vec<String> {
    let bytes = doc.bytes().to_vec();
    let file = lopdf::Document::load_mem(&bytes).unwrap();
    let d = file.get_dictionary(widget).unwrap();
    let ap = d.get(b"AP").unwrap().as_dict().unwrap();
    let n = ap.get(b"N").unwrap().as_reference().unwrap();
    let stream = file.get_object(n).unwrap().as_stream().unwrap();
    let res = stream.dict.get(b"Resources").unwrap().as_dict().unwrap();
    res.get(b"Font")
        .unwrap()
        .as_dict()
        .unwrap()
        .iter()
        .map(|(k, _)| String::from_utf8_lossy(k).to_string())
        .collect()
}

#[test]
fn values_beyond_winansi_use_the_embedded_font() {
    let mut doc = Document::from_bytes(form_doc(1, 2), None, None).unwrap();
    let field = doc.form_fields()[0].id;
    let widget = doc.field_widgets(field)[0];
    doc.set_field_value(field, "plain").unwrap();
    assert_eq!(fonts(&doc, widget), ["Helv"]);
    doc.set_field_value(field, "Łódź Ελληνικά").unwrap();
    assert_eq!(fonts(&doc, widget), ["FoliUni"]);
    assert_eq!(doc.form_fields()[0].value, "Łódź Ελληνικά");
    // Saved and read again, the value stays; undo brings the font back.
    let again = Document::from_bytes(doc.bytes().to_vec(), None, None).unwrap();
    assert_eq!(again.form_fields()[0].value, "Łódź Ελληνικά");
    doc.undo().unwrap();
    assert_eq!(fonts(&doc, widget), ["Helv"]);
}

#[test]
fn values_that_cannot_be_drawn_stay_as_typed() {
    let mut doc = Document::from_bytes(form_doc(1, 2), None, None).unwrap();
    let field = doc.form_fields()[0].clone();
    for value in ["漢字", "سلام"] {
        doc.set_field_value(field.id, value).unwrap();
        assert_eq!(doc.form_fields()[0].value, value);
    }
    // Other readers are asked to draw it, and the file reads back the same.
    let again = Document::from_bytes(doc.bytes().to_vec(), None, None).unwrap();
    assert_eq!(again.form_fields()[0].value, "سلام");
    let bytes = doc.bytes().to_vec();
    let file = lopdf::Document::load_mem(&bytes).unwrap();
    let form = file.catalog().unwrap().get(b"AcroForm").unwrap();
    let form = match form {
        Object::Reference(id) => file.get_dictionary(*id).unwrap(),
        other => other.as_dict().unwrap(),
    };
    assert!(form.get(b"NeedAppearances").unwrap().as_bool().unwrap());
}

#[test]
fn hebrew_values_are_drawn() {
    let mut doc = Document::from_bytes(form_doc(1, 2), None, None).unwrap();
    let field = doc.form_fields()[0].id;
    let widget = doc.field_widgets(field)[0];
    doc.set_field_value(field, "שלום").unwrap();
    assert_eq!(fonts(&doc, widget), ["FoliUni"]);
    assert_eq!(doc.form_fields()[0].value, "שלום");
}

#[test]
fn a_list_a_file_already_holds_can_still_be_used() {
    let mut b = Builder::new();
    let list = b.doc.add_object(dictionary! {
        "Type" => "Annot", "Subtype" => "Widget", "FT" => "Ch", "F" => 4,
        "T" => Object::string_literal("List"),
        "Rect" => vec![20.into(), 200.into(), 180.into(), 280.into()],
        "Opt" => vec![lopdf::text_string("One"), lopdf::text_string("漢字")],
    });
    b.page(
        (300.0, 300.0),
        Vec::new(),
        lopdf::Dictionary::new(),
        vec![Object::Reference(list)],
    );
    b.catalog_entry("AcroForm", dictionary! { "Fields" => vec![list.into()] }.into());
    let mut doc = Document::from_bytes(b.finish(), None, None).unwrap();
    doc.set_field_value(list, "One").unwrap();
    assert_eq!(doc.form_fields()[0].value, "One");
}

#[test]
fn hebrew_drawn_on_a_page_reads_in_its_own_order() {
    use folitecta_core::MarkPosition;
    let mut doc = Document::new_blank(1, 400.0, 500.0).unwrap();
    doc.add_page_numbers(
        &[0],
        "שלום {page}",
        1,
        MarkPosition::BottomCenter,
        12.0,
        20.0,
        [0.0; 3],
    )
    .unwrap();
    let text = doc.page_text(0);
    assert!(text.contains("שלום"), "{text:?}");
    let hits = doc.search("שלום", Default::default());
    assert_eq!(hits.len(), 1);
    assert_eq!(doc.text_range(0, hits[0].start, hits[0].end), "שלום");
    assert!(!hits[0].rects.is_empty());
}
