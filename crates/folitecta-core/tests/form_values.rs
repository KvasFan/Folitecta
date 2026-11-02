//! Values of fields that come from a file go through the checks of their
//! fields, and values too long for a field are told apart.

use folitecta_core::form_design::FieldProps;
use folitecta_core::testdocs::form_doc;
use folitecta_core::{Document, FieldFormat, FormDataFormat};

fn form() -> Document {
    Document::from_bytes(form_doc(1, 2), None, None).unwrap()
}

fn number(doc: &mut Document, name: &str) -> lopdf::ObjectId {
    let id = doc.form_fields().into_iter().find(|f| f.name == name).unwrap().id;
    let format = FieldFormat::Number {
        decimals: 2,
        separators: 0,
        parentheses: false,
        currency: String::new(),
        before: false,
    };
    let props = FieldProps {
        format,
        ..doc.field_props(id).unwrap()
    };
    doc.set_field_props(id, &props).unwrap();
    id
}

#[test]
fn imported_values_are_checked_like_typed_ones() {
    let mut doc = form();
    let id = number(&mut doc, "p1f1");
    let before = doc.form_fields()[0].value.clone();
    let csv = "p1f1,p1f2\nnot a number,Ada\n";
    let report = doc.import_form_data(FormDataFormat::Csv, csv.as_bytes()).unwrap();
    assert_eq!(report.filled, 1);
    assert_eq!(report.refused.len(), 1, "{report:?}");
    assert_eq!(report.refused[0].0, "p1f1");
    let value =
        |d: &Document, i: lopdf::ObjectId| d.form_fields().into_iter().find(|f| f.id == i).unwrap().value;
    assert_eq!(value(&doc, id), before, "the field stays as it was");
    assert_eq!(doc.form_fields()[1].value, "Ada");

    // One a number field takes; an empty one empties it.
    let report = doc
        .import_form_data(FormDataFormat::Csv, b"p1f1\n12.5\n")
        .unwrap();
    assert!(report.refused.is_empty(), "{report:?}");
    assert_eq!(value(&doc, id), "12.5");
    let report = doc
        .import_form_data(FormDataFormat::Csv, b"p1f1,p1f2\n,x\n")
        .unwrap();
    assert!(report.refused.is_empty(), "{report:?}");
    assert_eq!(value(&doc, id), "");
}

#[test]
fn copies_filled_from_a_table_say_the_same() {
    let mut doc = form();
    number(&mut doc, "p1f1");
    let before = doc.form_fields()[0].value.clone();
    let (copy, report) = doc
        .filled_copy(&[("p1f1".to_string(), "words".to_string())])
        .unwrap();
    assert_eq!(report.filled, 0);
    assert_eq!(report.refused.len(), 1);
    assert_eq!(copy.form_fields()[0].value, before);
}

#[test]
fn values_too_long_for_their_field_are_told() {
    let mut doc = form();
    let id = doc.form_fields()[0].id;
    doc.set_field_value(id, "Short").unwrap();
    assert!(!doc.field_overflows(id));
    let long = "A value far too long for a field two hundred points wide, on and on and on and on and on";
    doc.set_field_value(id, long).unwrap();
    assert!(doc.field_overflows(id));
    assert_eq!(doc.form_fields()[0].value, long, "all of it stays");
    doc.set_field_value(id, "").unwrap();
    assert!(!doc.field_overflows(id));

    let csv = format!("p1f1\n\"{long}\"\n");
    let report = doc.import_form_data(FormDataFormat::Csv, csv.as_bytes()).unwrap();
    assert_eq!(report.overflowing, ["p1f1"]);
}
