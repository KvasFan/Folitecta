use std::path::PathBuf;

use folitecta_core::{Document, EncryptionProfile, Error, SecurityPermissions};

fn folder(name: &str, profile: EncryptionProfile) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "folitecta-protect-{name}-{}-{}",
        profile.key(),
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// The `/P` permissions of an encrypted file (0 when it is not encrypted) and
/// whether `owner` opens it.
fn protection_of(path: &PathBuf, owner: &str) -> (i64, bool) {
    let doc = lopdf::Document::load_mem(&std::fs::read(path).unwrap()).unwrap();
    // Without an open password the file is decrypted at once; its encryption is then the state.
    let dict = match &doc.encryption_state {
        Some(state) => state.encode().ok(),
        None => doc.get_encrypted().ok().cloned(),
    };
    let p = dict
        .and_then(|d| d.get(b"P").ok().and_then(|p| p.as_i64().ok()))
        .unwrap_or(0);
    (p, Document::open_with_password(path, owner).is_ok())
}

fn no_printing() -> SecurityPermissions {
    SecurityPermissions {
        print: false,
        ..SecurityPermissions::default()
    }
}

#[test]
fn a_password_protected_file_stays_protected_after_saving() {
    for profile in EncryptionProfile::ALL {
        password_protected_file_stays_protected(profile);
    }
}

fn password_protected_file_stays_protected(profile: EncryptionProfile) {
    let dir = folder("password", profile);
    let original = dir.join("protected.pdf");
    Document::new_blank(2, 300.0, 300.0)
        .unwrap()
        .save_encrypted(&original, "user-pw", "owner-pw", no_printing(), profile)
        .unwrap();
    let (p_before, _) = protection_of(&original, "owner-pw");

    let mut doc = Document::open_with_password(&original, "user-pw").unwrap();
    assert!(doc.is_protected());
    doc.delete_pages(&[1]).unwrap();
    let saved = dir.join("saved.pdf");
    doc.save_as(&saved).unwrap();

    // Still encrypted: no password, no document.
    assert!(matches!(Document::open(&saved), Err(Error::PasswordRequired)));
    // The same user and owner passwords open it, with the edit in it.
    assert_eq!(
        Document::open_with_password(&saved, "user-pw")
            .unwrap()
            .page_count(),
        1
    );
    assert_eq!(
        Document::open_with_password(&saved, "owner-pw")
            .unwrap()
            .page_count(),
        1
    );
    // The permissions did not change.
    let (p_after, _) = protection_of(&saved, "owner-pw");
    assert_eq!(p_after, p_before);
    assert_ne!(p_after, 0);
    // Saving again to the same file keeps it protected too.
    doc.save().unwrap();
    assert!(matches!(Document::open(&saved), Err(Error::PasswordRequired)));
    // A copy is protected as well.
    let copy = dir.join("copy.pdf");
    doc.save_copy(&copy).unwrap();
    assert!(matches!(Document::open(&copy), Err(Error::PasswordRequired)));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn restrictions_without_an_open_password_are_kept() {
    for profile in EncryptionProfile::ALL {
        restrictions_are_kept(profile);
    }
}

fn restrictions_are_kept(profile: EncryptionProfile) {
    let dir = folder("restricted", profile);
    let original = dir.join("restricted.pdf");
    Document::new_blank(1, 300.0, 300.0)
        .unwrap()
        .save_encrypted(&original, "", "owner-only", no_printing(), profile)
        .unwrap();
    let (p_before, _) = protection_of(&original, "owner-only");
    // Opens without a password, as before.
    let mut doc = Document::open(&original).unwrap();
    assert!(doc.is_protected());
    doc.rotate_pages(&[0], 90).unwrap();
    let saved = dir.join("saved.pdf");
    doc.save_as(&saved).unwrap();
    let (p_after, owner_ok) = protection_of(&saved, "owner-only");
    assert_eq!(p_after, p_before);
    assert!(owner_ok);
    let text = String::from_utf8_lossy(&std::fs::read(&saved).unwrap()).to_string();
    assert!(text.contains("/Encrypt"), "the restrictions were dropped");
    assert_eq!(Document::open(&saved).unwrap().page_count(), 1);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_unrestricted_copy_is_an_explicit_choice() {
    for profile in EncryptionProfile::ALL {
        unrestricted_copy_is_explicit(profile);
    }
}

fn unrestricted_copy_is_explicit(profile: EncryptionProfile) {
    let dir = folder("unrestricted", profile);
    let original = dir.join("protected.pdf");
    Document::new_blank(1, 300.0, 300.0)
        .unwrap()
        .save_encrypted(&original, "user-pw", "owner-pw", no_printing(), profile)
        .unwrap();
    let mut doc = Document::open_with_password(&original, "user-pw").unwrap();
    let plain = dir.join("plain.pdf");
    doc.save_unprotected_copy(&plain).unwrap();
    let reopened = Document::open(&plain).unwrap();
    assert!(!reopened.is_protected());
    assert_eq!(reopened.page_count(), 1);
    // The document itself keeps its protection.
    assert!(doc.is_protected());
    // Plain files stay plain.
    let mut fresh = Document::new_blank(1, 100.0, 100.0).unwrap();
    assert!(!fresh.is_protected());
    let p = dir.join("fresh.pdf");
    fresh.save_as(&p).unwrap();
    assert!(!Document::open(&p).unwrap().is_protected());
    let _ = std::fs::remove_dir_all(&dir);
}
