//! Formats and checks of text fields: chosen in the properties, kept as the
//! standard scripts (and read back from those other programs write), shown
//! in the appearance, and applied when a value is committed.

use folitecta_core::form_format::{print_date, scan_date};
use folitecta_core::{Document, FieldCheck, FieldFormat, FieldProps, NewField};
use lopdf::ObjectId;

fn blank() -> Document {
    Document::new_blank(1, 595.0, 842.0).unwrap()
}

fn add(doc: &mut Document, kind: NewField, props: FieldProps) -> ObjectId {
    doc.add_form_field(0, kind, [100.0, 700.0, 300.0, 722.0], &props)
        .unwrap()
}

/// What the appearance of the field's widget draws (its text).
fn shown(doc: &Document, field: ObjectId) -> String {
    let w = doc.field_widgets(field)[0];
    let d = doc.inner().get_dictionary(w).unwrap();
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
    let data = s.decompressed_content().unwrap_or_else(|_| s.content.clone());
    String::from_utf8_lossy(&data).to_string()
}

fn number_eur() -> FieldFormat {
    FieldFormat::Number {
        decimals: 2,
        separators: 2,
        parentheses: false,
        currency: " €".into(),
        before: false,
    }
}

#[test]
fn scripts_and_formats_stand_for_each_other() {
    for f in [
        number_eur(),
        FieldFormat::Number {
            decimals: 0,
            separators: 0,
            parentheses: true,
            currency: "$".into(),
            before: true,
        },
        FieldFormat::Percent {
            decimals: 1,
            separators: 3,
        },
        FieldFormat::Date("d mmmm yyyy".into()),
        FieldFormat::Time("h:MM tt".into()),
        FieldFormat::Special(2),
    ] {
        let (format, keystroke) = f.scripts().unwrap();
        assert_eq!(
            FieldFormat::from_scripts(Some(&format), Some(&keystroke)),
            f,
            "{format}"
        );
    }
    // As other programs write them.
    assert_eq!(
        FieldFormat::from_scripts(
            Some("AFNumber_Format(2, 0, 0, 0, \"€\", true);"),
            Some("AFNumber_Keystroke(2, 0, 0, 0, \"€\", true);")
        ),
        FieldFormat::Number {
            decimals: 2,
            separators: 0,
            parentheses: false,
            currency: "€".into(),
            before: true
        }
    );
    assert_eq!(
        FieldFormat::from_scripts(Some("AFDate_Format(2);"), Some("AFDate_Keystroke(2);")),
        FieldFormat::Date("mm/dd/yy".into())
    );
    assert_eq!(FieldFormat::from_scripts(None, None), FieldFormat::None);
    // Anything else is a script of its own.
    assert_eq!(
        FieldFormat::from_scripts(Some("event.value = event.value.toUpperCase();"), None),
        FieldFormat::Custom
    );
    assert_eq!(
        FieldFormat::from_scripts(
            Some("AFDate_FormatEx('dd/mm/yyyy');"),
            Some("if (x) event.rc = false;")
        ),
        FieldFormat::Custom
    );
    assert_eq!(
        FieldFormat::from_scripts(None, Some("event.rc = true;")),
        FieldFormat::Custom
    );
    let range = FieldCheck::Range {
        min: Some(0.0),
        max: Some(12.5),
    };
    assert_eq!(FieldCheck::from_script(range.script().as_deref()), range);
    assert_eq!(
        FieldCheck::from_script(Some("AFRange_Validate(false, 0, true, 100);")),
        FieldCheck::Range {
            min: None,
            max: Some(100.0)
        }
    );
    assert_eq!(
        FieldCheck::from_script(Some("app.alert('no');")),
        FieldCheck::Custom
    );
}

#[test]
fn a_date_field_takes_only_dates_and_shows_them_its_way() {
    let mut doc = blank();
    let id = add(&mut doc, NewField::Date, FieldProps::default());
    let props = doc.field_props(id).unwrap();
    assert_eq!(props.name, "Date1");
    assert_eq!(props.format, FieldFormat::Date("dd/mm/yyyy".into()));
    let refused = doc.set_field_value(id, "31/04/2026").unwrap_err().to_string();
    assert!(
        refused.contains("dd/mm/yyyy") && refused.contains("Date1"),
        "{refused}"
    );
    assert!(doc.form_fields()[0].value.is_empty(), "nothing written");
    doc.set_field_value(id, "5/3/26").unwrap();
    assert!(shown(&doc, id).contains("05/03/2026"), "{}", shown(&doc, id));
    // Saved and reopened, the field is still a date field.
    let doc = Document::from_bytes(doc.bytes().to_vec(), None, None).unwrap();
    assert_eq!(doc.field_format(id), FieldFormat::Date("dd/mm/yyyy".into()));
    // Another format chosen when the field is made.
    let mut doc = blank();
    let props = FieldProps {
        format: FieldFormat::Date("d mmmm yyyy".into()),
        ..FieldProps::default()
    };
    let id = add(&mut doc, NewField::Date, props);
    doc.set_field_value(id, "5 marzo 2026").unwrap();
    assert!(shown(&doc, id).contains("5 March 2026"));
}

#[test]
fn formats_and_ranges_chosen_in_the_properties() {
    let mut doc = blank();
    let id = add(&mut doc, NewField::Text, FieldProps::default());
    let mut props = doc.field_props(id).unwrap();
    assert_eq!(
        (&props.format, &props.check),
        (&FieldFormat::None, &FieldCheck::None)
    );
    props.format = number_eur();
    props.check = FieldCheck::Range {
        min: Some(0.0),
        max: Some(5000.0),
    };
    doc.set_field_props(id, &props).unwrap();
    assert_eq!(doc.field_props(id).unwrap().format, number_eur());
    doc.set_field_value(id, "1234.5").unwrap();
    // The euro sign in the WinAnsi encoding of the appearance's font.
    assert!(
        shown(&doc, id).contains("(1.234,50 \\200)"),
        "{}",
        shown(&doc, id)
    );
    let words = doc.set_field_value(id, "a lot").unwrap_err().to_string();
    assert!(words.contains("format"), "{words}");
    let big = doc.set_field_value(id, "9000").unwrap_err().to_string();
    assert!(big.contains("5000"), "{big}");
    assert_eq!(doc.form_fields()[0].value, "1234.5");
    // One undo step took the format away again.
    doc.undo().unwrap();
    doc.undo().unwrap();
    assert_eq!(doc.field_props(id).unwrap().format, FieldFormat::None);
    // No format: the scripts go.
    let mut props = doc.field_props(id).unwrap();
    props.format = FieldFormat::Time("HH:MM".into());
    doc.set_field_props(id, &props).unwrap();
    props.format = FieldFormat::None;
    doc.set_field_props(id, &props).unwrap();
    assert!(doc.field_script(id, b"F").is_none() && doc.field_script(id, b"K").is_none());
}

#[test]
fn scripts_written_by_hand_stay_as_they_are() {
    let mut doc = blank();
    let id = add(&mut doc, NewField::Text, FieldProps::default());
    doc.set_field_script(id, "F", Some("event.value = '#' + event.value;"))
        .unwrap();
    doc.set_field_script(id, "V", Some("if (event.value == '0') event.rc = false;"))
        .unwrap();
    let mut props = doc.field_props(id).unwrap();
    assert_eq!(
        (&props.format, &props.check),
        (&FieldFormat::Custom, &FieldCheck::Custom)
    );
    props.tooltip = "Changed".into();
    doc.set_field_props(id, &props).unwrap();
    assert_eq!(
        doc.field_script(id, b"F").as_deref(),
        Some("event.value = '#' + event.value;")
    );
    assert!(doc.set_field_value(id, "0").is_err());
}

#[test]
fn dates_for_a_calendar() {
    assert_eq!(print_date("dd/mm/yyyy", 2026, 3, 5), "05/03/2026");
    assert_eq!(print_date("d mmmm yyyy", 2026, 12, 31), "31 December 2026");
    assert_eq!(scan_date("dd/mm/yyyy", "5/3/2026"), Some((2026, 3, 5)));
    assert_eq!(scan_date("mm/dd/yyyy", "02/30/2026"), None);
    assert_eq!(scan_date("dd/mm/yyyy", ""), None);
}
