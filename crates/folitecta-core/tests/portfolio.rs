//! Portfolios: files in folders, columns, sort order, initial file, view.

use folitecta_core::Document;
use folitecta_core::portfolio::SchemaField;

fn reopened(doc: &Document) -> Document {
    Document::from_bytes(doc.bytes().to_vec(), None, None).unwrap()
}

#[test]
fn portfolios_hold_files_in_folders() {
    let inner = Document::from_text("Inner report", Default::default()).unwrap();
    let mut doc = Document::new_portfolio(
        vec![
            ("report.pdf".into(), inner.bytes().to_vec()),
            ("data.csv".into(), b"a,b\n1,2\n".to_vec()),
        ],
        "Project files",
    )
    .unwrap();
    assert!(doc.is_portfolio());
    assert!(doc.page_text(0).contains("Project files"));
    let p = doc.portfolio().unwrap();
    assert_eq!(p.view, "D");
    assert_eq!(
        p.schema.iter().map(|f| f.kind.as_str()).collect::<Vec<_>>(),
        ["F", "Desc", "ModDate", "Size"]
    );
    assert_eq!(p.files_in(None).len(), 2);
    assert_eq!(p.files_in(None)[0].name, "data.csv", "sorted by name");

    // Folders: create, nest, add files, rename.
    let specs = doc.portfolio_create_folder("Specs", None).unwrap();
    let drafts = doc.portfolio_create_folder("Drafts", Some(specs)).unwrap();
    doc.portfolio_add_file("v1.txt", b"one".to_vec(), Some(drafts))
        .unwrap();
    doc.portfolio_add_file("index.txt", b"idx".to_vec(), Some(specs))
        .unwrap();
    assert!(doc.portfolio_add_file("x.txt", b"x".to_vec(), Some(99)).is_err());
    doc.portfolio_edit_folder(specs, "Specifications", "All the specs")
        .unwrap();
    let p = reopened(&doc).portfolio().unwrap();
    let names: Vec<&str> = p.subfolders(None).iter().map(|f| f.name.as_str()).collect();
    assert_eq!(names, ["Specifications"]);
    assert_eq!(p.subfolders(Some(specs))[0].name, "Drafts");
    assert_eq!(p.files_in(Some(drafts))[0].name, "v1.txt");
    assert_eq!(p.files_in(Some(specs))[0].name, "index.txt");
    assert_eq!(p.files_in(None).len(), 2);

    // Deleting a folder removes what is inside it.
    let removed = doc.portfolio_delete(&[], &[specs]).unwrap();
    assert_eq!(removed, 4, "two folders and two files");
    let p = doc.portfolio().unwrap();
    assert!(p.subfolders(None).is_empty());
    assert_eq!(p.items.len(), 2);
    doc.undo().unwrap();
    assert_eq!(doc.portfolio().unwrap().items.len(), 4);
}

#[test]
fn columns_sort_initial_file_and_view() {
    let mut doc = Document::new_portfolio(
        vec![
            ("b.txt".into(), b"bbbbbbbbbb".to_vec()),
            ("a.txt".into(), b"a".to_vec()),
        ],
        "Cover",
    )
    .unwrap();
    let mut schema = doc.portfolio().unwrap().schema;
    schema.push(SchemaField {
        key: "Owner".into(),
        name: "Owner".into(),
        kind: "S".into(),
        visible: true,
        editable: true,
    });
    schema.push(SchemaField {
        key: "Pages".into(),
        name: "Pages".into(),
        kind: "N".into(),
        visible: true,
        editable: true,
    });
    doc.set_portfolio_schema(&schema).unwrap();
    let dup = vec![schema[0].clone(), schema[0].clone()];
    assert!(doc.set_portfolio_schema(&dup).is_err());
    let keys: Vec<Vec<u8>> = doc
        .portfolio()
        .unwrap()
        .items
        .iter()
        .map(|i| i.key.clone())
        .collect();
    doc.set_portfolio_item_value(&keys[0], "Owner", "Ann").unwrap();
    doc.set_portfolio_item_value(&keys[0], "Pages", "12").unwrap();
    assert!(doc.set_portfolio_item_value(&keys[0], "Pages", "twelve").is_err());
    doc.set_portfolio_sort("Size", false).unwrap();
    doc.set_portfolio_initial(Some(&keys[1])).unwrap();
    doc.set_portfolio_view("T").unwrap();
    assert!(doc.set_portfolio_view("X").is_err());
    let p = reopened(&doc).portfolio().unwrap();
    assert_eq!(p.schema.len(), 6);
    assert_eq!(p.sort, Some(("Size".into(), false)));
    assert_eq!(p.initial.as_deref(), Some(keys[1].as_slice()));
    assert_eq!(p.view, "T");
    let first = p.items.iter().find(|i| i.key == keys[0]).unwrap();
    assert_eq!(first.fields.get("Owner").map(String::as_str), Some("Ann"));
    assert_eq!(first.fields.get("Pages").map(String::as_str), Some("12"));
    // Largest first.
    assert_eq!(p.files_in(None)[0].name, "b.txt");
    doc.remove_portfolio().unwrap();
    assert!(!doc.is_portfolio());
    assert_eq!(doc.attachments().len(), 2);
}
