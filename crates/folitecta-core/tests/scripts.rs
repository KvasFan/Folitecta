use folitecta_core::Document;

#[test]
fn named_document_scripts_are_written_and_read() {
    let mut doc = Document::new_blank(1, 200.0, 200.0).unwrap();
    assert!(doc.document_scripts().is_empty());
    doc.set_document_scripts(&[
        ("totals".into(), "var x = 1;".into()),
        ("Init".into(), "app.alert('Ciao è');".into()),
        ("".into(), "ignored".into()),
        ("Init".into(), "repeated".into()),
    ])
    .unwrap();
    assert_eq!(
        doc.document_scripts(),
        vec![
            ("Init".to_string(), "app.alert('Ciao è');".to_string()),
            ("totals".to_string(), "var x = 1;".to_string()),
        ]
    );
    // Saved and loaded again.
    let path = std::env::temp_dir().join("folitecta-scripts-test.pdf");
    doc.save_as(&path).unwrap();
    let again = Document::open(&path).unwrap();
    assert_eq!(again.document_scripts().len(), 2);
    let _ = std::fs::remove_file(&path);
    doc.set_document_scripts(&[]).unwrap();
    assert!(doc.document_scripts().is_empty());
    doc.undo().unwrap();
    assert_eq!(doc.document_scripts().len(), 2);
}
