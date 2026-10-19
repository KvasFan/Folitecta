//! Protection kept by every write path: documents made from a protected
//! document are protected like it, content of protected files does not go
//! into unprotected documents, and an unrestricted copy stays an explicit
//! choice.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use folitecta_core::combine::Source;
use folitecta_core::digsig::{DigitalId, IdentityInfo, KeyKind, SignOptions};
use folitecta_core::{
    Document, EncryptionProfile, Error, FieldProps, NewField, SecurityPermissions, SummaryOptions,
};

const USER: &str = "user-pw";

fn folder(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("folitecta-gates-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn contains(hay: &[u8], needle: &[u8]) -> bool {
    hay.windows(needle.len()).any(|w| w == needle)
}

/// A two-page document with a note, a text field and a layer.
fn source_document() -> Document {
    let mut doc = Document::from_text("MARKER-GATE page text", Default::default()).unwrap();
    doc.insert_blank_pages(1, 1, 300.0, 300.0).unwrap();
    doc.add_note(0, 40.0, 700.0, "MARKER-GATE-NOTE", "Reviewer", [1.0, 0.9, 0.2])
        .unwrap();
    let props = FieldProps {
        name: "Name".into(),
        ..Default::default()
    };
    doc.add_form_field(0, NewField::Text, [20.0, 600.0, 200.0, 620.0], &props)
        .unwrap();
    doc.add_layer("Layer", &[]).unwrap();
    doc
}

/// `doc` saved protected (with `user` as the password to open) and opened.
fn protect(doc: &Document, dir: &Path, name: &str, user: &str) -> (Document, PathBuf) {
    let path = dir.join(name);
    doc.save_encrypted(
        &path,
        user,
        "owner-pw",
        SecurityPermissions::default(),
        EncryptionProfile::Aes256,
    )
    .unwrap();
    let opened = if user.is_empty() {
        Document::open(&path).unwrap()
    } else {
        Document::open_with_password(&path, user).unwrap()
    };
    (opened, path)
}

/// `doc` saved to `dir/name` needs the password and opens with it.
fn saved_protected(mut doc: Document, dir: &Path, name: &str) {
    assert!(doc.is_protected(), "{name}");
    let path = dir.join(name);
    doc.save_as(&path).unwrap();
    assert!(
        !contains(&std::fs::read(&path).unwrap(), b"MARKER-GATE-NOTE"),
        "{name}"
    );
    assert!(
        matches!(Document::open(&path), Err(Error::PasswordRequired)),
        "{name}"
    );
    Document::open_with_password(&path, USER).unwrap();
}

#[test]
fn documents_made_from_protected_ones_keep_the_protection() {
    let dir = folder("derived");
    let (prot, _) = protect(&source_document(), &dir, "protected.pdf", USER);
    saved_protected(prot.extract_pages(&[0]).unwrap(), &dir, "extract.pdf");
    let parts = prot.split_every(1).unwrap();
    assert_eq!(parts.len(), 2);
    for (i, part) in parts.into_iter().enumerate() {
        saved_protected(part, &dir, &format!("part-{i}.pdf"));
    }
    let (filled, n) = prot
        .filled_copy(&[("Name".to_string(), "Ada".to_string())])
        .unwrap();
    assert_eq!(n, 1);
    saved_protected(filled, &dir, "filled.pdf");
    saved_protected(prot.duplicate().unwrap(), &dir, "duplicate.pdf");
    saved_protected(
        prot.summarize_comments(&SummaryOptions::default()).unwrap(),
        &dir,
        "summary.pdf",
    );
    saved_protected(
        prot.extract_layers(None, &HashMap::new()).unwrap(),
        &dir,
        "layers.pdf",
    );
    // A report made from a protected document's text takes its protection.
    let mut report = Document::from_text("MARKER-GATE-NOTE quoted", Default::default()).unwrap();
    report.inherit_protection(prot.protection().unwrap());
    saved_protected(report, &dir, "report.pdf");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn signed_protected_duplicates_keep_their_signatures() {
    let dir = folder("duplicate");
    let id = DigitalId::create_self_signed(
        &IdentityInfo {
            name: "Dup Signer".into(),
            ..IdentityInfo::default()
        },
        KeyKind::EcP256,
        2,
    )
    .unwrap();
    let (prot, _) = protect(&source_document(), &dir, "protected.pdf", USER);
    let signed = prot.sign(&id, &SignOptions::default()).unwrap();
    let opened = Document::from_bytes(signed.clone(), None, Some(USER)).unwrap();
    let mut dup = opened.duplicate().unwrap();
    dup.add_note(1, 30.0, 200.0, "in the copy", "Ann", [1.0, 0.0, 0.0])
        .unwrap();
    let path = dir.join("dup.pdf");
    dup.save_as(&path).unwrap();
    let saved = std::fs::read(&path).unwrap();
    assert!(saved.starts_with(&signed));
    let v = Document::from_bytes(saved, None, Some(USER))
        .unwrap()
        .validate_signatures(&[]);
    assert!(v[0].valid, "{v:?}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn pages_of_protected_files_do_not_go_into_unprotected_documents() {
    let dir = folder("imports");
    // Restrictions only: the file opens without a password.
    let (prot, prot_path) = protect(&source_document(), &dir, "restricted.pdf", "");
    let mut plain = Document::new_blank(1, 300.0, 300.0).unwrap();
    let before = plain.bytes();
    assert!(matches!(
        plain.insert_document(0, &prot, None),
        Err(Error::ProtectedSource)
    ));
    assert!(matches!(
        plain.overlay_document(&prot, &[0], false),
        Err(Error::ProtectedSource)
    ));
    assert!(matches!(
        plain.import_as_layer(&prot, &[0], false, "Imported"),
        Err(Error::ProtectedSource)
    ));
    assert!(matches!(
        plain.replace_pages(0, 1, &prot, &[0]),
        Err(Error::ProtectedSource)
    ));
    let data = std::fs::read(&prot_path).unwrap();
    assert!(matches!(
        plain.import_comments(&data),
        Err(Error::ProtectedSource)
    ));
    // A comment copied from the protected document is not pasted.
    let note = prot.annotation_snapshot(0, 0).unwrap();
    assert!(matches!(
        plain.paste_annotation_snapshot(0, &note, 0.0, 0.0),
        Err(Error::ProtectedSource)
    ));
    // Nothing changed, nothing to undo.
    assert_eq!(plain.bytes(), before);
    assert!(plain.layers().is_empty());
    assert!(!plain.can_undo());
    // Combining reports the protected file and keeps the others.
    let plain_path = dir.join("plain.pdf");
    Document::new_blank(2, 300.0, 300.0)
        .unwrap()
        .save_as(&plain_path)
        .unwrap();
    let (combined, problems) = Document::combine(&[
        Source {
            path: prot_path.clone(),
            pages: None,
        },
        Source {
            path: plain_path,
            pages: None,
        },
    ])
    .unwrap();
    assert_eq!(combined.page_count(), 2);
    assert!(
        problems
            .iter()
            .any(|(p, e)| *p == prot_path && e.contains("protected")),
        "{problems:?}"
    );
    // A protected document may take content of a plain one.
    let (mut target, _) = protect(&source_document(), &dir, "target.pdf", USER);
    let plain = Document::new_blank(1, 300.0, 300.0).unwrap();
    target.insert_document(0, &plain, None).unwrap();
    assert_eq!(target.page_count(), 3);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_unrestricted_copy_stays_an_explicit_choice() {
    let dir = folder("unrestricted");
    let id = DigitalId::create_self_signed(
        &IdentityInfo {
            name: "Unrestricted".into(),
            ..IdentityInfo::default()
        },
        KeyKind::EcP256,
        2,
    )
    .unwrap();
    let (prot, _) = protect(&source_document(), &dir, "protected.pdf", USER);
    let signed = prot.sign(&id, &SignOptions::default()).unwrap();
    let mut doc = Document::from_bytes(signed, None, Some(USER)).unwrap();
    // Never copied plain behind the user's back (recovery copies).
    assert!(doc.copy_bytes().is_none());
    let path = dir.join("plain.pdf");
    doc.save_unprotected_copy(&path).unwrap();
    let plain = Document::open(&path).unwrap();
    assert!(!plain.is_protected());
    assert!(contains(&std::fs::read(&path).unwrap(), b"MARKER-GATE-NOTE"));
    assert!(doc.is_protected());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn attachments_of_protected_copies_are_encrypted() {
    let dir = folder("attachments");
    let mut doc = source_document();
    doc.add_attachment(
        "notes.txt",
        b"MARKER-ATTACHED content".to_vec(),
        "MARKER-DESCRIPTION",
        None,
    )
    .unwrap();
    let path = dir.join("protected.pdf");
    doc.save_encrypted(
        &path,
        USER,
        "owner-pw",
        SecurityPermissions::default(),
        EncryptionProfile::Aes256,
    )
    .unwrap();
    let raw = std::fs::read(&path).unwrap();
    assert!(!contains(&raw, b"MARKER-ATTACHED"));
    assert!(!contains(&raw, b"MARKER-DESCRIPTION"));
    // Known limit: strings inside stream dictionaries (the /Params of an
    // embedded file: its dates and checksum) are not encrypted by the PDF
    // library, and not decrypted when read.
    let opened = Document::open_with_password(&path, USER).unwrap();
    let att = opened.attachments();
    assert_eq!(att.len(), 1);
    assert_eq!(
        opened.attachment_data(&att[0]).unwrap(),
        b"MARKER-ATTACHED content"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
