//! Signing, certifying, time-stamping and saving protected documents: the
//! files stay encrypted like the document (signature values excepted),
//! earlier signatures stay valid through encrypted incremental updates, and
//! signatures validate after the file is opened again with its password.

use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock};
use std::time::{SystemTime, UNIX_EPOCH};

use folitecta_core::digsig::{
    Check, DigitalId, IdentityInfo, KeyKind, SignOptions, SignPlacement, SignatureStatus, timestamp,
};
use folitecta_core::{Document, EncryptionProfile, Error, RenderOpts, Renderer, SecurityPermissions};
use lopdf::xref::XrefType;

const USER: &str = "user-pw";
const OWNER: &str = "owner-pw";

static STAMP_TIME: LazyLock<u64> = LazyLock::new(|| {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock after 1970")
        .as_secs()
        + 3600
});

fn identity(name: &str) -> IdentityInfo {
    IdentityInfo {
        name: name.into(),
        organization: "Example Protected".into(),
        ..IdentityInfo::default()
    }
}

fn signer(name: &str) -> DigitalId {
    DigitalId::create_self_signed(&identity(name), KeyKind::EcP256, 3).unwrap()
}

fn folder(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("folitecta-psig-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn contains(hay: &[u8], needle: &[u8]) -> bool {
    hay.windows(needle.len()).any(|w| w == needle)
}

fn p_of(raw: &[u8]) -> (i64, i64, i64) {
    let doc = lopdf::Document::load_mem(raw).unwrap();
    let d = match &doc.encryption_state {
        Some(state) => state.encode().unwrap(),
        None => doc.get_encrypted().unwrap().clone(),
    };
    let n = |k: &[u8]| d.get(k).unwrap().as_i64().unwrap();
    (n(b"V"), n(b"R"), n(b"P"))
}

/// A protected document of two pages with a note, opened from its file
/// (`user` empty: restrictions only).
fn protected(dir: &Path, profile: EncryptionProfile, table: bool, user: &str) -> (Document, Vec<u8>) {
    let mut doc = Document::from_text("MARKER-PAGE-9Z on the first page", Default::default()).unwrap();
    doc.insert_blank_pages(1, 1, 400.0, 500.0).unwrap();
    doc.add_note(0, 40.0, 700.0, "MARKER-NOTE-9Z", "Reviewer", [1.0, 0.9, 0.2])
        .unwrap();
    if table {
        doc.inner_mut().reference_table.cross_reference_type = XrefType::CrossReferenceTable;
        doc.commit().unwrap();
    }
    let path = dir.join(format!("protected-{}-{table}.pdf", profile.key()));
    let perms = SecurityPermissions {
        print: false,
        ..SecurityPermissions::default()
    };
    doc.save_encrypted(&path, user, OWNER, perms, profile).unwrap();
    let raw = std::fs::read(&path).unwrap();
    let opened = if user.is_empty() {
        Document::open(&path).unwrap()
    } else {
        Document::open_with_password(&path, user).unwrap()
    };
    assert!(opened.is_protected());
    (opened, raw)
}

fn open_bytes(bytes: Vec<u8>, pw: &str) -> Document {
    Document::from_bytes(bytes, None, Some(pw)).unwrap()
}

/// The `/ByteRange` numbers written in a file (the first one found after `from`).
fn byte_range(raw: &[u8], from: usize) -> (usize, [usize; 4]) {
    let at = from + raw[from..].windows(10).position(|w| w == b"/ByteRange").unwrap();
    let open = at + raw[at..].iter().position(|c| *c == b'[').unwrap();
    let close = open + raw[open..].iter().position(|c| *c == b']').unwrap();
    let text = std::str::from_utf8(&raw[open + 1..close]).unwrap();
    let n: Vec<usize> = text.split_whitespace().map(|v| v.parse().unwrap()).collect();
    (open, [n[0], n[1], n[2], n[3]])
}

fn all_valid(s: &[SignatureStatus]) -> bool {
    s.iter().all(|x| x.valid)
}

fn sign_opts(reason: &str) -> SignOptions {
    SignOptions {
        reason: reason.into(),
        placement: SignPlacement::New {
            page: 0,
            rect: [40.0, 40.0, 240.0, 100.0],
        },
        ..SignOptions::default()
    }
}

#[test]
fn signing_protected_documents_keeps_them_encrypted() {
    let dir = folder("sign");
    let id = signer("Protected Signer");
    for profile in EncryptionProfile::ALL {
        for table in [false, true] {
            let (doc, original) = protected(&dir, profile, table, USER);
            let signed_file = doc
                .sign_to_file(&id, &sign_opts("MARKER-REASON-1"), None)
                .unwrap();
            assert!(signed_file.is_protected());
            let s = signed_file.bytes.clone();
            // Encrypted like the document: no password, no document.
            assert!(matches!(
                Document::from_bytes(s.clone(), None, None),
                Err(Error::PasswordRequired)
            ));
            assert!(!contains(&s, b"MARKER-REASON-1"), "{profile:?} {table}");
            assert!(!contains(&s, b"MARKER-NOTE-9Z"));
            assert!(contains(&s, b"/Encrypt"));
            assert_eq!(p_of(&s), p_of(&original));
            // The value the byte range leaves out is the signature, unencrypted.
            let (_, r) = byte_range(&s, 0);
            assert!(s[r[0] + r[1]..].starts_with(b"<30"));
            assert_eq!(r[2] + r[3], s.len());
            // The document the file opens as, without the password asked again.
            let in_place = signed_file.into_document(None).unwrap();
            assert!(in_place.is_protected());
            let v = in_place.validate_signatures(&[]);
            assert!(v.len() == 1 && v[0].valid && v[0].covers_document, "{v:?}");
            // Opened again with either password.
            for pw in [USER, OWNER] {
                let reopened = open_bytes(s.clone(), pw);
                assert!(reopened.is_protected());
                assert_eq!(reopened.encryption(), doc.encryption());
                let v = reopened.validate_signatures(std::slice::from_ref(&id.certificate));
                assert_eq!(v.len(), 1);
                assert!(
                    v[0].valid && v[0].covers_document,
                    "{profile:?} {table}: {:?}",
                    v[0].problems
                );
                assert_eq!(v[0].reason, "MARKER-REASON-1");
                assert_eq!(v[0].signer, "Protected Signer");
                assert_eq!(v[0].checks.integrity, Check::Passed);
                assert!(v[0].trusted);
            }
            assert!(matches!(
                Document::from_bytes(s.clone(), None, Some("wrong")),
                Err(Error::WrongPassword)
            ));
            // The renderer decrypts the signed file on its own.
            let r = Renderer::new_with_password(Arc::new(s.clone()), USER).unwrap();
            assert_eq!(r.page_count(), 2);
            r.render_rgba(0, 1.0, RenderOpts::default()).unwrap();
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn second_signatures_and_saves_of_protected_documents_keep_the_first() {
    let dir = folder("second");
    let (first, second) = (signer("First"), signer("Second"));
    for profile in EncryptionProfile::ALL {
        for table in [false, true] {
            let (doc, _) = protected(&dir, profile, table, USER);
            let s = doc.sign(&first, &SignOptions::default()).unwrap();
            // A second signature is an encrypted update of the signed file.
            let t = open_bytes(s.clone(), USER)
                .sign(&second, &SignOptions::default())
                .unwrap();
            assert!(t.starts_with(&s));
            assert!(contains(&t[s.len()..], b"/Encrypt"));
            let mut doc = open_bytes(t.clone(), USER);
            let v = doc.validate_signatures(&[]);
            assert_eq!(v.len(), 2);
            assert!(all_valid(&v), "{v:?}");
            assert_eq!(v.iter().filter(|x| x.covers_document).count(), 1);
            assert!(v.iter().any(|x| x.revision_len == s.len()));
            // Unchanged: a copy is the signed file itself.
            let copy = dir.join("copy.pdf");
            doc.save_copy(&copy).unwrap();
            assert_eq!(std::fs::read(&copy).unwrap(), t);
            // An edit saved: an encrypted update after the signed bytes.
            doc.add_note(1, 50.0, 400.0, "MARKER-LATE", "Bob", [0.3, 0.6, 1.0])
                .unwrap();
            let path = dir.join(format!("saved-{}-{table}.pdf", profile.key()));
            doc.save_as(&path).unwrap();
            let saved = std::fs::read(&path).unwrap();
            assert!(saved.starts_with(&t));
            assert!(!contains(&saved, b"MARKER-LATE"));
            let again = open_bytes(saved.clone(), USER);
            assert_eq!(again.comments().len(), 2);
            let v = again.validate_signatures(&[]);
            assert!(all_valid(&v) && v.iter().all(|x| !x.covers_document), "{v:?}");
            // Saved again, after more edits and an undo: each save follows the last.
            doc.add_note(1, 80.0, 300.0, "third", "Bob", [0.3, 0.6, 1.0])
                .unwrap();
            doc.save().unwrap();
            let third = std::fs::read(&path).unwrap();
            assert!(third.starts_with(&saved));
            doc.undo().unwrap();
            doc.save().unwrap();
            let fourth = std::fs::read(&path).unwrap();
            assert!(fourth.starts_with(&third));
            // Only what changed is appended.
            assert!(
                fourth.len() - third.len() < 20_000,
                "{}",
                fourth.len() - third.len()
            );
            let last = open_bytes(fourth, OWNER);
            assert_eq!(last.comments().len(), 2);
            let v = last.validate_signatures(&[]);
            assert!(v.len() == 2 && all_valid(&v), "{v:?}");
            // The first signed revision: encrypted, and as it was signed.
            let rev = last
                .signed_revision(&v.iter().find(|x| x.revision_len == s.len()).unwrap().field)
                .unwrap();
            assert_eq!(rev, s);
            assert!(matches!(
                Document::from_bytes(rev.clone(), None, None),
                Err(Error::PasswordRequired)
            ));
            assert_eq!(open_bytes(rev, USER).comments().len(), 1);
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn signed_files_opened_in_place_save_as_updates() {
    let dir = folder("inplace");
    let id = signer("In Place");
    let (doc, _) = protected(&dir, EncryptionProfile::Aes256, false, USER);
    let file = doc.sign_to_file(&id, &sign_opts("in place"), None).unwrap();
    let signed = file.bytes.clone();
    let path = dir.join("signed.pdf");
    std::fs::write(&path, &signed).unwrap();
    let mut doc = file.into_document(Some(path.clone())).unwrap();
    assert_eq!(doc.path(), Some(path.as_path()));
    assert!(!doc.is_dirty());
    doc.add_note(1, 50.0, 400.0, "after", "Bob", [0.3, 0.6, 1.0])
        .unwrap();
    doc.save().unwrap();
    let saved = std::fs::read(&path).unwrap();
    assert!(saved.starts_with(&signed));
    let v = open_bytes(saved, USER).validate_signatures(&[]);
    assert!(v.len() == 1 && v[0].valid && !v[0].covers_document, "{v:?}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn certified_protected_documents() {
    let dir = folder("certify");
    let id = signer("Certifier");
    for profile in EncryptionProfile::ALL {
        let (doc, _) = protected(&dir, profile, false, USER);
        let opts = SignOptions {
            certify: 3,
            ..SignOptions::default()
        };
        let c = doc.sign(&id, &opts).unwrap();
        let mut reopened = open_bytes(c.clone(), USER);
        assert!(reopened.inner().catalog().unwrap().has(b"Perms"));
        let v = reopened.validate_signatures(&[]);
        assert!(v[0].valid && v[0].certify == Some(3), "{v:?}");
        // Comments are allowed after this certification, in an update.
        reopened
            .add_note(1, 60.0, 300.0, "allowed", "Bob", [0.3, 0.6, 1.0])
            .unwrap();
        let path = dir.join("certified.pdf");
        reopened.save_as(&path).unwrap();
        let saved = std::fs::read(&path).unwrap();
        assert!(saved.starts_with(&c));
        let v = open_bytes(saved, USER).validate_signatures(&[]);
        assert!(v[0].valid && !v[0].covers_document, "{v:?}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn document_time_stamps_of_protected_documents() {
    let dir = folder("stamp");
    let id = signer("Stamped Signer");
    let tsa =
        DigitalId::create_time_stamp_authority(&identity("Local Authority"), KeyKind::EcP256, 5).unwrap();
    let mut serial = 0u64;
    let mut local = |req: &[u8]| {
        serial += 1;
        timestamp::respond(&tsa, req, *STAMP_TIME, serial)
    };
    let trusted = [id.certificate.clone(), tsa.certificate.clone()];
    for profile in EncryptionProfile::ALL {
        let (doc, _) = protected(&dir, profile, false, USER);
        // A time stamp of an unsigned protected document: written whole.
        let stamped_alone = doc.time_stamp(&mut local).unwrap();
        assert!(matches!(
            Document::from_bytes(stamped_alone.clone(), None, None),
            Err(Error::PasswordRequired)
        ));
        let v = open_bytes(stamped_alone, USER).validate_signatures(&trusted);
        assert!(v.len() == 1 && v[0].valid && v[0].document_time_stamp, "{v:?}");
        // A signature with its own time stamp, then a document time stamp.
        let signed = doc
            .sign_with(&id, &SignOptions::default(), Some(&mut local))
            .unwrap();
        let stamped = open_bytes(signed.clone(), USER).time_stamp(&mut local).unwrap();
        assert!(stamped.starts_with(&signed));
        assert!(contains(&stamped[signed.len()..], b"/Encrypt"));
        let v = open_bytes(stamped, OWNER).validate_signatures(&trusted);
        assert_eq!(v.len(), 2);
        assert!(all_valid(&v), "{v:?}");
        let stamp = v.iter().find(|x| x.document_time_stamp).unwrap();
        assert!(stamp.covers_document);
        let sig = v.iter().find(|x| !x.document_time_stamp).unwrap();
        assert!(sig.later_revisions_time_stamped, "{sig:?}");
        assert_eq!(sig.checks.time, Check::Passed);
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn restrictions_only_documents_sign_and_reopen_without_password() {
    let dir = folder("restricted");
    let id = signer("Restricted Signer");
    let (doc, original) = protected(&dir, EncryptionProfile::Aes256, false, "");
    let s = doc.sign(&id, &SignOptions::default()).unwrap();
    assert_eq!(p_of(&s), p_of(&original));
    let reopened = Document::from_bytes(s, None, None).unwrap();
    assert!(reopened.is_protected());
    let v = reopened.validate_signatures(&[]);
    assert!(v[0].valid && v[0].covers_document, "{v:?}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn opened_rc4_files_are_signed_with_their_encryption() {
    let id = signer("Rc4 Signer");
    let plain = Document::from_text("MARKER-RC4 text", Default::default())
        .unwrap()
        .bytes();
    let mut doc = lopdf::Document::load_mem(&plain).unwrap();
    let file_id = vec![9u8; 16];
    doc.trailer.set(
        "ID",
        lopdf::Object::Array(vec![
            lopdf::Object::String(file_id.clone(), lopdf::StringFormat::Hexadecimal),
            lopdf::Object::String(file_id, lopdf::StringFormat::Hexadecimal),
        ]),
    );
    let state = lopdf::EncryptionState::try_from(lopdf::EncryptionVersion::V2 {
        document: &doc,
        owner_password: OWNER,
        user_password: USER,
        key_length: 128,
        permissions: lopdf::Permissions::default(),
    })
    .unwrap();
    doc.encrypt(&state).unwrap();
    let mut rc4 = Vec::new();
    doc.save_to(&mut rc4).unwrap();
    let opened = open_bytes(rc4.clone(), USER);
    assert_eq!(opened.encryption().unwrap().method, "RC4 128-bit");
    let s = opened.sign(&id, &sign_opts("rc4 reason")).unwrap();
    assert_eq!(p_of(&s), p_of(&rc4));
    assert!(!contains(&s, b"rc4 reason"));
    let v = open_bytes(s.clone(), USER).validate_signatures(&[]);
    assert!(v[0].valid && v[0].covers_document, "{v:?}");
    assert_eq!(v[0].reason, "rc4 reason");
    // A second signature: an update encrypted with RC4 too.
    let t = open_bytes(s.clone(), USER)
        .sign(&signer("Second"), &SignOptions::default())
        .unwrap();
    assert!(t.starts_with(&s));
    let v = open_bytes(t, OWNER).validate_signatures(&[]);
    assert!(v.len() == 2 && all_valid(&v), "{v:?}");
}

#[test]
fn moved_byte_ranges_are_not_trusted() {
    let dir = folder("moved");
    let id = signer("Moved");
    // A signed plain file and a signed protected one.
    let plain = Document::new_blank(1, 300.0, 300.0)
        .unwrap()
        .sign(&id, &SignOptions::default())
        .unwrap();
    let (doc, _) = protected(&dir, EncryptionProfile::Aes256, false, USER);
    let encrypted = doc.sign(&id, &SignOptions::default()).unwrap();
    for (raw, password) in [(plain, None), (encrypted, Some(USER))] {
        // The gap starts one byte early (before the value's '<').
        let (open, r) = byte_range(&raw, 0);
        let old = format!("{} {}", r[0], r[1]);
        let new = format!("{} {}", r[0], r[1] - 1);
        assert_eq!(old.len(), new.len());
        let mut moved = raw.clone();
        moved[open + 1..open + 1 + old.len()].copy_from_slice(new.as_bytes());
        let reopened = Document::from_bytes(moved, None, password).unwrap();
        let v = reopened.validate_signatures(&[]);
        assert!(!v[0].valid);
        assert!(
            v[0].problems
                .iter()
                .any(|p| p.contains("not the value of this signature")),
            "{:?}",
            v[0].problems
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn reasons_that_look_like_syntax_keep_signatures_valid() {
    // The value of a signature is found by reading its dictionary token by
    // token: words and brackets inside its strings do not matter.
    let dir = folder("reasons");
    let id = signer("Reasons");
    let (protected_doc, _) = protected(&dir, EncryptionProfile::Aes256, false, USER);
    for reason in [
        "see obj 7 for details",
        "the endobj word",
        "1 0 obj << /Contents <00> >> endobj",
        "unbalanced ) and ( with \\ backslash",
    ] {
        let plain = Document::new_blank(1, 300.0, 300.0)
            .unwrap()
            .sign(&id, &sign_opts(reason))
            .unwrap();
        let v = Document::from_bytes(plain, None, None)
            .unwrap()
            .validate_signatures(&[]);
        assert!(
            v[0].valid && v[0].reason == reason,
            "plain {reason:?}: {:?}",
            v[0].problems
        );
        let s = protected_doc.sign(&id, &sign_opts(reason)).unwrap();
        let v = open_bytes(s, USER).validate_signatures(&[]);
        assert!(
            v[0].valid && v[0].reason == reason,
            "protected {reason:?}: {:?}",
            v[0].problems
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}
