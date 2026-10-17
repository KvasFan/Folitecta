use folitecta_core::form_data::form_rows;
use folitecta_core::{Document, FieldProps, FormDataFormat, NewField};

#[test]
fn one_filled_copy_for_every_row_of_a_table() {
    let mut doc = Document::new_blank(1, 300.0, 400.0).unwrap();
    for (name, y) in [("Name", 300.0), ("City", 260.0)] {
        let props = FieldProps {
            name: name.into(),
            ..Default::default()
        };
        doc.add_form_field(0, NewField::Text, [20.0, y, 200.0, y + 20.0], &props)
            .unwrap();
    }
    let csv = "Name,City,Unknown\r\nAda,\"London, UK\",x\r\n\r\n\"Grace \"\"G\"\"\",Arlington,y\r\n";
    let rows = form_rows(FormDataFormat::Csv, csv.as_bytes()).unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0][1], ("City".to_string(), "London, UK".to_string()));
    let (copy, n) = doc.filled_copy(&rows[1]).unwrap();
    assert_eq!(n, 2, "the unknown column fills nothing");
    let value = |d: &Document, name: &str| {
        d.form_fields()
            .into_iter()
            .find(|f| f.name == name)
            .map(|f| f.value)
            .unwrap_or_default()
    };
    assert_eq!(value(&copy, "Name"), "Grace \"G\"");
    assert_eq!(value(&copy, "City"), "Arlington");
    // The document itself stays empty; the copy has nothing to undo.
    assert_eq!(value(&doc, "Name"), "");
    assert!(!copy.can_undo());
    // Tab-separated text, and tables without values.
    let rows = form_rows(FormDataFormat::Text, b"Name\tCity\nLin\tParis\n").unwrap();
    assert_eq!(rows[0][0].1, "Lin");
    assert!(form_rows(FormDataFormat::Csv, b"Name,City\n").is_err());
}
