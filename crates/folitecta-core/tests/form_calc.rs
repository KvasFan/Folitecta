//! Field calculations: simple calculations written as calculate actions and
//! computed when values change, in calculation order, within the same undo
//! step.

use folitecta_core::{CalcOp, Calculation, Document, FieldProps, NewField};
use lopdf::ObjectId;

fn field(doc: &mut Document, name: &str, y: f32) -> ObjectId {
    let props = FieldProps {
        name: name.into(),
        ..FieldProps::default()
    };
    doc.add_form_field(0, NewField::Text, [40.0, y, 200.0, y + 20.0], &props)
        .unwrap()
}

fn value(doc: &Document, name: &str) -> String {
    doc.form_fields()
        .into_iter()
        .find(|f| f.name == name)
        .map(|f| f.value)
        .unwrap_or_default()
}

fn calc(op: CalcOp, fields: &[&str]) -> Calculation {
    Calculation {
        op,
        fields: fields.iter().map(|f| f.to_string()).collect(),
    }
}

#[test]
fn calculated_fields_follow_their_fields() {
    let mut doc = Document::new_blank(1, 400.0, 400.0).unwrap();
    field(&mut doc, "price", 300.0);
    field(&mut doc, "qty", 270.0);
    let total = field(&mut doc, "total", 240.0);
    doc.set_field_calculation(total, Some(&calc(CalcOp::Product, &["price", "qty"])))
        .unwrap();
    // Read back as written, and as other viewers read it.
    assert_eq!(
        doc.field_calculation(total),
        Some(calc(CalcOp::Product, &["price", "qty"]))
    );
    assert_eq!(
        doc.calculate_script(total).as_deref(),
        Some("AFSimple_Calculate(\"PRD\", new Array(\"price\", \"qty\"));")
    );
    assert_eq!(doc.calculation_order(), vec![total]);
    assert_eq!(value(&doc, "total"), "0");

    let id = |doc: &Document, n: &str| doc.form_fields().into_iter().find(|f| f.name == n).unwrap().id;
    let price = id(&doc, "price");
    doc.set_field_value(price, "12,5").unwrap();
    doc.set_field_values(&[("qty".into(), "4".into())]).unwrap();
    assert_eq!(value(&doc, "total"), "50");
    // One undo takes back the value and what was calculated from it.
    doc.undo().unwrap();
    assert_eq!(
        (value(&doc, "qty"), value(&doc, "total")),
        (String::new(), "0".to_string())
    );
    doc.redo().unwrap();
    assert_eq!(value(&doc, "total"), "50");
    // Reset: the calculation follows the emptied fields.
    doc.reset_form().unwrap();
    assert_eq!(value(&doc, "total"), "0");
    // A field cannot use its own value; only text fields and combo boxes are calculated.
    assert!(
        doc.set_field_calculation(total, Some(&calc(CalcOp::Sum, &["total"])))
            .is_err()
    );
    // No calculation any more.
    doc.set_field_calculation(total, None).unwrap();
    assert_eq!(doc.field_calculation(total), None);
    assert!(doc.calculation_order().is_empty());
}

#[test]
fn the_calculation_order_decides_what_each_field_sees() {
    let mut doc = Document::new_blank(1, 400.0, 400.0).unwrap();
    field(&mut doc, "row.1", 300.0);
    field(&mut doc, "row.2", 270.0);
    let total = field(&mut doc, "total", 240.0);
    let double = field(&mut doc, "double", 210.0);
    let avg = field(&mut doc, "avg", 180.0);
    // A group name stands for every field in it.
    doc.set_field_calculation(total, Some(&calc(CalcOp::Sum, &["row"])))
        .unwrap();
    doc.set_field_calculation(double, Some(&calc(CalcOp::Sum, &["total", "row"])))
        .unwrap();
    doc.set_field_calculation(avg, Some(&calc(CalcOp::Average, &["row"])))
        .unwrap();
    let names: Vec<String> = doc.calculated_fields().into_iter().map(|f| f.name).collect();
    assert_eq!(names, ["total", "double", "avg"]);
    doc.set_field_values(&[("row.1".into(), "1".into()), ("row.2".into(), "2".into())])
        .unwrap();
    assert_eq!(
        (value(&doc, "total"), value(&doc, "double"), value(&doc, "avg")),
        ("3".into(), "6".into(), "1.5".into())
    );
    // Calculated before the total, the double sees the total of before.
    doc.set_calculation_order(&[double, total, avg]).unwrap();
    doc.set_field_values(&[("row.1".into(), "10".into())]).unwrap();
    assert_eq!(
        (value(&doc, "total"), value(&doc, "double")),
        ("12".into(), "15".into())
    );
    doc.undo().unwrap();
    doc.undo().unwrap();
    assert_eq!(doc.calculation_order(), vec![total, double, avg]);
}
