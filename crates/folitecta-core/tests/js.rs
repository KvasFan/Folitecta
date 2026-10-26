//! The JavaScript of form fields: a calculate script of its own, a format
//! script that changes only the text shown, a validate script that refuses
//! a value with its alert; scripts kept in the file, removed again, and an
//! endless script stopped by the engine.

use folitecta_core::js::run_script;
use folitecta_core::{Document, FieldProps, NewField};
use lopdf::{Object, ObjectId};

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

/// The text drawn by the normal appearance of `id`.
fn shown(doc: &Document, id: ObjectId) -> String {
    let inner = doc.inner();
    let ap = inner.get_dictionary(id).unwrap().get(b"AP").unwrap();
    let ap = match ap {
        Object::Reference(r) => inner.get_dictionary(*r).unwrap(),
        Object::Dictionary(d) => d,
        _ => panic!("no appearance"),
    };
    let n = ap.get(b"N").unwrap().as_reference().unwrap();
    let stream = inner.get_object(n).unwrap().as_stream().unwrap();
    let bytes = stream
        .decompressed_content()
        .unwrap_or_else(|_| stream.content.clone());
    String::from_utf8_lossy(&bytes).into_owned()
}

#[test]
fn a_calculate_script_of_its_own() {
    let mut doc = Document::new_blank(1, 400.0, 400.0).unwrap();
    let net = field(&mut doc, "net", 300.0);
    let gross = field(&mut doc, "gross", 260.0);
    doc.set_field_script(
        gross,
        "C",
        Some("var n = Number(this.getField('net').value); event.value = (n * 1.22).toFixed(2);"),
    )
    .unwrap();
    assert_eq!(doc.calculation_order(), vec![gross]);
    // Not one of the simple calculations: read back as a script.
    assert_eq!(doc.field_calculation(gross), None);
    doc.set_field_value(net, "100").unwrap();
    assert_eq!(value(&doc, "gross"), "122.00");
    // Kept in the file.
    let again = Document::from_bytes(doc.bytes().to_vec(), None, None).unwrap();
    assert!(again.field_script(gross, b"C").unwrap().contains("1.22"));
    // Removed: no longer calculated.
    doc.set_field_script(gross, "C", None).unwrap();
    assert!(doc.calculation_order().is_empty());
    assert_eq!(doc.field_script(gross, b"C"), None);
    doc.undo().unwrap();
    assert!(doc.field_script(gross, b"C").is_some());
    assert!(doc.set_field_script(gross, "X", Some("1")).is_err());
}

#[test]
fn a_format_script_changes_only_what_is_shown() {
    let mut doc = Document::new_blank(1, 400.0, 400.0).unwrap();
    let amount = field(&mut doc, "amount", 300.0);
    doc.set_field_script(amount, "F", Some("AFNumber_Format(2, 0, 0, 0, \"€\", true);"))
        .unwrap();
    doc.set_field_value(amount, "1234.5").unwrap();
    assert_eq!(value(&doc, "amount"), "1234.5", "the value stays");
    let text = shown(&doc, amount);
    assert!(text.contains("1,234.50"), "{text}");
}

#[test]
fn a_validate_script_refuses_a_value_with_its_alert() {
    let mut doc = Document::new_blank(1, 400.0, 400.0).unwrap();
    let age = field(&mut doc, "age", 300.0);
    doc.set_field_script(
        age,
        "V",
        Some("if (Number(event.value) > 120) { app.alert('Too old'); event.rc = false; }"),
    )
    .unwrap();
    doc.set_field_value(age, "42").unwrap();
    let err = doc.set_field_value(age, "300").unwrap_err().to_string();
    assert!(err.contains("Too old"), "{err}");
    assert_eq!(value(&doc, "age"), "42");
    // The ranges of forms.
    doc.set_field_script(age, "V", Some("AFRange_Validate(true, 0, true, 120);"))
        .unwrap();
    assert!(doc.set_field_value(age, "-1").is_err());
    assert!(doc.set_field_value(age, "18").is_ok());
}

#[test]
fn an_endless_script_is_stopped() {
    let ctx = folitecta_core::js::field_context(&[]);
    let r = run_script("while (true) {}", &ctx, None);
    assert!(!r.ok);
    assert!(r.error.is_some());
    let r = run_script("function f() { return f(); } f()", &ctx, None);
    assert!(!r.ok);
    // Nothing of the system in reach.
    let r = run_script(
        "typeof require + typeof fetch + typeof XMLHttpRequest",
        &ctx,
        None,
    );
    assert_eq!(r.result.as_deref(), Some("undefinedundefinedundefined"));
}
