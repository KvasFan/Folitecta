//! Text in form fields beyond WinAnsi: drawn with the embedded font, the
//! standard font left as it was for the rest; text that cannot be drawn is
//! refused before anything changes, and what a file already holds stays
//! usable.

use folitecta_core::testdocs::{Builder, form_doc};
use folitecta_core::{Document, Error};
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
fn text_that_cannot_be_drawn_changes_nothing() {
    let mut doc = Document::from_bytes(form_doc(1, 2), None, None).unwrap();
    let field = doc.form_fields()[0].clone();
    let before = doc.bytes().to_vec();
    let undo = doc.can_undo();
    assert!(matches!(
        doc.set_field_value(field.id, "漢字"),
        Err(Error::Glyphs(_))
    ));
    assert!(matches!(
        doc.set_field_values(&[(field.name.clone(), "漢字".to_string())]),
        Err(Error::Glyphs(_))
    ));
    assert_eq!(doc.bytes().to_vec(), before);
    assert_eq!(doc.can_undo(), undo);
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
