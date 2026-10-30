//! The permissions of a protected file kept by Folitecta: opened with the
//! user password, a change of a kind the security settings forbid is taken
//! back and named; the owner password lifts the restrictions.

use folitecta_core::testdocs::form_doc;
use folitecta_core::{Document, EncryptionProfile, Error, MarkPosition, Restricted, SecurityPermissions};

fn protected(name: &str, permissions: SecurityPermissions, profile: EncryptionProfile) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("folitecta-permissions-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(format!("{name}.pdf"));
    let doc = Document::from_bytes(form_doc(2, 2), None, None).unwrap();
    doc.save_encrypted(&path, "u", "o", permissions, profile).unwrap();
    path
}

fn refused(r: folitecta_core::Result<()>) -> Option<Restricted> {
    match r {
        Err(Error::NotPermitted(kind)) => Some(kind),
        Err(e) => panic!("another error: {e}"),
        Ok(()) => None,
    }
}

#[test]
fn only_filling_in_forms_is_allowed() {
    let only_fill = SecurityPermissions {
        print: true,
        modify: false,
        copy: false,
        annotate: false,
        fill_forms: true,
        assemble: false,
    };
    for profile in EncryptionProfile::ALL {
        let path = protected(&format!("fill-{}", profile.key()), only_fill, profile);
        let mut doc = Document::open_with_password(&path, "u").unwrap();
        assert_eq!(
            doc.restrictions(),
            [
                Restricted::Modify,
                Restricted::Annotate,
                Restricted::Assemble,
                Restricted::Copy
            ]
        );
        let field = doc.form_fields()[0].id;
        assert_eq!(refused(doc.set_field_value(field, "Filled")), None);
        assert_eq!(doc.form_fields()[0].value, "Filled");
        let before = doc.bytes().to_vec();
        let undo = doc.can_undo();

        let note = doc.add_note(0, 40.0, 700.0, "a note", "Reviewer", [1.0, 0.9, 0.2]);
        assert_eq!(refused(note.map(|_| ())), Some(Restricted::Annotate));
        assert_eq!(refused(doc.rotate_pages(&[0], 90)), Some(Restricted::Assemble));
        assert_eq!(refused(doc.delete_pages(&[1])), Some(Restricted::Assemble));
        assert_eq!(
            refused(doc.add_bookmark("Start", 0, &[])),
            Some(Restricted::Assemble)
        );
        let numbers = doc.add_page_numbers(
            &[0, 1],
            "{page}",
            1,
            MarkPosition::BottomCenter,
            10.0,
            20.0,
            [0.0; 3],
        );
        assert_eq!(refused(numbers), Some(Restricted::Modify));
        // Nothing of them was kept.
        assert_eq!(doc.bytes().to_vec(), before, "{profile:?}");
        assert_eq!(doc.can_undo(), undo);
        assert_eq!(doc.page_count(), 2);

        // The owner password lifts the restrictions.
        assert!(!doc.unlock_owner("u"));
        assert!(doc.unlock_owner("o"));
        assert!(doc.restrictions().is_empty());
        assert_eq!(refused(doc.rotate_pages(&[0], 90)), None);
    }
}

#[test]
fn comments_allow_filling_in_and_nothing_else() {
    let only_comments = SecurityPermissions {
        print: true,
        modify: false,
        copy: true,
        annotate: true,
        fill_forms: false,
        assemble: false,
    };
    let path = protected("comments", only_comments, EncryptionProfile::Aes256);
    let mut doc = Document::open_with_password(&path, "u").unwrap();
    assert_eq!(doc.restrictions(), [Restricted::Modify, Restricted::Assemble]);
    let note = doc.add_note(0, 40.0, 700.0, "a note", "Reviewer", [1.0, 0.9, 0.2]);
    assert_eq!(refused(note.map(|_| ())), None);
    let field = doc.form_fields()[0].id;
    assert_eq!(refused(doc.set_field_value(field, "Filled")), None);
    assert_eq!(refused(doc.rotate_pages(&[0], 90)), Some(Restricted::Assemble));
    // Opened with the owner password, nothing is restricted.
    let mut owner = Document::open_with_password(&path, "o").unwrap();
    assert!(owner.restrictions().is_empty());
    assert_eq!(refused(owner.rotate_pages(&[0], 90)), None);
}

#[test]
fn assembling_pages_alone() {
    let only_pages = SecurityPermissions {
        print: true,
        modify: false,
        copy: true,
        annotate: false,
        fill_forms: false,
        assemble: true,
    };
    let path = protected("pages", only_pages, EncryptionProfile::Aes128);
    let mut doc = Document::open_with_password(&path, "u").unwrap();
    assert_eq!(refused(doc.rotate_pages(&[0], 90)), None);
    assert_eq!(refused(doc.add_bookmark("Start", 0, &[])), None);
    assert_eq!(refused(doc.delete_pages(&[1])), None);
    assert_eq!(doc.page_count(), 1);
    let field = doc.form_fields()[0].id;
    assert_eq!(
        refused(doc.set_field_value(field, "Filled")),
        Some(Restricted::FillForms)
    );
}

#[test]
fn unprotected_documents_and_copies_are_not_restricted() {
    let mut doc = Document::from_bytes(form_doc(1, 1), None, None).unwrap();
    assert!(doc.restrictions().is_empty());
    assert_eq!(refused(doc.rotate_pages(&[0], 90)), None);
}
