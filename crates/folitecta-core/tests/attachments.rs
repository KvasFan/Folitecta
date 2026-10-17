//! Attachments: details, editing, replacing contents.

use folitecta_core::Document;
use folitecta_core::attachments::{RELATIONSHIPS, is_executable_name};

fn reopened(doc: &Document) -> Document {
    Document::from_bytes(doc.bytes().to_vec(), None, None).unwrap()
}

#[test]
fn attachments_are_described_edited_and_replaced() {
    let mut doc = Document::new_blank(1, 200.0, 200.0).unwrap();
    doc.add_attachment(
        "notes.txt",
        b"first version".repeat(40),
        "Draft",
        Some("text/plain"),
    )
    .unwrap();
    doc.add_attachment("data.csv", b"a,b\n1,2\n".to_vec(), "", None)
        .unwrap();
    let atts = doc.attachments();
    assert_eq!(atts.len(), 2);
    let notes = atts.iter().find(|a| a.name == "notes.txt").unwrap().clone();
    assert_eq!(notes.size, Some(13 * 40));
    assert!(!notes.created.is_empty());
    assert_eq!(notes.mime, "text/plain");
    assert!(notes.relationship.is_empty());

    // Rename, describe and relate.
    doc.set_attachment_info(&notes, "report-notes.txt", "Final notes", "Supplement")
        .unwrap();
    let doc2 = reopened(&doc);
    let renamed = doc2
        .attachments()
        .into_iter()
        .find(|a| a.name == "report-notes.txt")
        .expect("renamed");
    assert_eq!(renamed.description, "Final notes");
    assert_eq!(renamed.relationship, "Supplement");
    assert_eq!(renamed.key.as_deref(), Some(b"report-notes.txt".as_slice()));
    let af = doc2
        .inner()
        .catalog()
        .unwrap()
        .get(b"AF")
        .unwrap()
        .as_array()
        .unwrap()
        .len();
    assert_eq!(af, 1);
    // Names must stay unique.
    assert!(doc.set_attachment_info(&renamed, "data.csv", "", "").is_err());
    assert!(
        doc.set_attachment_info(&renamed, "x.txt", "", "Nonsense")
            .is_err()
    );

    // New contents under the same name.
    let current = doc
        .attachments()
        .into_iter()
        .find(|a| a.name == "report-notes.txt")
        .unwrap();
    let second = b"second version ".repeat(30);
    doc.replace_attachment_data(&current, second.clone()).unwrap();
    let after = doc
        .attachments()
        .into_iter()
        .find(|a| a.name == "report-notes.txt")
        .unwrap();
    assert_eq!(doc.attachment_data(&after).unwrap(), second);
    assert_eq!(after.size, Some(second.len() as u64));
    assert_eq!(after.description, "Final notes");
    assert!(after.compressed.is_some());
    doc.undo().unwrap();
    let back = doc
        .attachments()
        .into_iter()
        .find(|a| a.name == "report-notes.txt")
        .unwrap();
    assert_eq!(doc.attachment_data(&back).unwrap().len(), 13 * 40);
    assert!(RELATIONSHIPS.contains(&"Source"));
}

#[test]
fn executables_are_recognized() {
    for risky in ["setup.EXE", "run.sh", "macro.docm", "page.html", "noextension"] {
        assert!(is_executable_name(risky), "{risky}");
    }
    for safe in ["report.pdf", "photo.jpg", "data.csv", "notes.txt"] {
        assert!(!is_executable_name(safe), "{safe}");
    }
}
