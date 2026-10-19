//! New password protection: AES-256 (the default) and AES-128, their
//! encryption dictionaries, passwords and permissions, checked in the raw
//! files, reopened, and decrypted independently by the renderer.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use folitecta_core::{Document, EncryptionProfile, Error, RenderOpts, Renderer, SecurityPermissions};
use lopdf::{Object, dictionary};

fn folder(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("folitecta-profiles-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn contains(hay: &[u8], needle: &[u8]) -> bool {
    hay.windows(needle.len()).any(|w| w == needle)
}

/// A document whose page and note carry marker texts.
fn marked() -> Document {
    let mut doc = Document::from_text("MARKER-PAGE-7Q on the page", Default::default()).unwrap();
    doc.add_note(
        0,
        40.0,
        700.0,
        "MARKER-NOTE-7Q in a note",
        "Reviewer",
        [1.0, 0.9, 0.2],
    )
    .unwrap();
    doc
}

fn no_printing() -> SecurityPermissions {
    SecurityPermissions {
        print: false,
        ..SecurityPermissions::default()
    }
}

/// The encryption dictionary of a file that needs a password to open.
fn encryption_dict(raw: &[u8]) -> lopdf::Dictionary {
    let doc = lopdf::Document::load_mem(raw).unwrap();
    doc.get_encrypted().unwrap().clone()
}

/// `(V, R, P)` of an encrypted file.
fn p_of(raw: &[u8]) -> (i64, i64, i64) {
    let doc = lopdf::Document::load_mem(raw).unwrap();
    let d = match &doc.encryption_state {
        Some(state) => state.encode().unwrap(),
        None => doc.get_encrypted().unwrap().clone(),
    };
    let n = |k: &[u8]| d.get(k).unwrap().as_i64().unwrap();
    (n(b"V"), n(b"R"), n(b"P"))
}

fn str_len(d: &lopdf::Dictionary, key: &[u8]) -> usize {
    d.get(key).unwrap().as_str().unwrap().len()
}

fn name<'a>(d: &'a lopdf::Dictionary, path: &[&[u8]]) -> &'a [u8] {
    let mut cur = d;
    for key in &path[..path.len() - 1] {
        cur = cur.get(key).unwrap().as_dict().unwrap();
    }
    cur.get(path[path.len() - 1]).unwrap().as_name().unwrap()
}

/// The page renders with dark pixels (text) through the renderer opened
/// with `password`.
fn renders_with(raw: &[u8], password: &str) -> bool {
    let r = Renderer::new_with_password(Arc::new(raw.to_vec()), password).unwrap();
    let img = r.render_rgba(0, 1.0, RenderOpts::default()).unwrap();
    img.pixels().any(|p| p[0] < 128)
}

fn extension_level(doc: &Document) -> Option<i64> {
    let inner = doc.inner();
    let ext = inner.catalog().ok()?.get(b"Extensions").ok()?;
    let ext = folitecta_core::util::deref(inner, ext).as_dict().ok()?;
    ext.get(b"ADBE")
        .ok()?
        .as_dict()
        .ok()?
        .get(b"ExtensionLevel")
        .ok()?
        .as_i64()
        .ok()
}

#[test]
fn aes256_copy_needs_a_password_and_hides_its_content() {
    let dir = folder("aes256");
    let doc = marked();
    // The check below means something: the plain file shows the markers.
    assert!(contains(&doc.bytes(), b"MARKER-NOTE-7Q"));
    let p = dir.join("protected.pdf");
    doc.save_encrypted(
        &p,
        "user-pw",
        "owner-pw",
        no_printing(),
        EncryptionProfile::Aes256,
    )
    .unwrap();
    let raw = std::fs::read(&p).unwrap();
    assert!(!contains(&raw, b"MARKER-PAGE-7Q"));
    assert!(!contains(&raw, b"MARKER-NOTE-7Q"));
    assert_eq!(p_of(&raw), (5, 6, -2056));
    let d = encryption_dict(&raw);
    assert_eq!(name(&d, &[b"CF", b"StdCF", b"CFM"]), b"AESV3");
    assert_eq!(name(&d, &[b"StmF"]), b"StdCF");
    assert_eq!(name(&d, &[b"StrF"]), b"StdCF");
    assert_eq!(str_len(&d, b"O"), 48);
    assert_eq!(str_len(&d, b"U"), 48);
    assert_eq!(str_len(&d, b"OE"), 32);
    assert_eq!(str_len(&d, b"UE"), 32);
    assert_eq!(str_len(&d, b"Perms"), 16);
    assert!(raw.starts_with(b"%PDF-1.7"));

    assert!(matches!(Document::open(&p), Err(Error::PasswordRequired)));
    assert!(matches!(
        Document::open_with_password(&p, "wrong"),
        Err(Error::WrongPassword)
    ));
    for pw in ["user-pw", "owner-pw"] {
        let opened = Document::open_with_password(&p, pw).unwrap();
        assert!(opened.is_protected());
        assert_eq!(opened.encryption().unwrap().method, "AES-256");
        assert_eq!(opened.info().encryption, "AES-256");
        assert!(opened.page_text(0).contains("MARKER-PAGE-7Q"));
        let notes = opened.annotations(0).unwrap();
        assert!(notes.iter().any(|a| a.contents.contains("MARKER-NOTE-7Q")));
        assert_eq!(extension_level(&opened), Some(8));
    }
    // The renderer decrypts it on its own, only with the password.
    assert!(Renderer::new(Arc::new(raw.clone())).is_err());
    assert!(renders_with(&raw, "user-pw"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn aes128_stays_a_choice() {
    let dir = folder("aes128");
    let doc = marked();
    let p = dir.join("protected.pdf");
    doc.save_encrypted(
        &p,
        "user-pw",
        "owner-pw",
        no_printing(),
        EncryptionProfile::Aes128,
    )
    .unwrap();
    let raw = std::fs::read(&p).unwrap();
    assert!(!contains(&raw, b"MARKER-NOTE-7Q"));
    assert_eq!(p_of(&raw), (4, 4, -2056));
    let d = encryption_dict(&raw);
    assert_eq!(name(&d, &[b"CF", b"StdCF", b"CFM"]), b"AESV2");
    assert_eq!(d.get(b"Length").unwrap().as_i64().unwrap(), 128);
    for key in [&b"OE"[..], b"UE", b"Perms"] {
        assert!(d.get(key).is_err());
    }
    for pw in ["user-pw", "owner-pw"] {
        let opened = Document::open_with_password(&p, pw).unwrap();
        assert_eq!(opened.encryption().unwrap().method, "AES-128");
        assert!(opened.page_text(0).contains("MARKER-PAGE-7Q"));
    }
    assert!(renders_with(&raw, "user-pw"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn versions_are_raised_for_the_profile() {
    let dir = folder("versions");
    let mut doc = marked();
    doc.inner_mut().version = "1.4".into();
    doc.commit().unwrap();
    assert!(doc.bytes().starts_with(b"%PDF-1.4"));
    let a256 = dir.join("aes256.pdf");
    doc.save_encrypted(&a256, "u", "o", Default::default(), EncryptionProfile::Aes256)
        .unwrap();
    assert!(std::fs::read(&a256).unwrap().starts_with(b"%PDF-1.7"));
    assert_eq!(
        extension_level(&Document::open_with_password(&a256, "u").unwrap()),
        Some(8)
    );
    // AES-128 needs PDF 1.6 (crypt filters with AESV2) and no extension.
    let a128 = dir.join("aes128.pdf");
    doc.save_encrypted(&a128, "u", "o", Default::default(), EncryptionProfile::Aes128)
        .unwrap();
    assert!(std::fs::read(&a128).unwrap().starts_with(b"%PDF-1.6"));
    assert_eq!(
        extension_level(&Document::open_with_password(&a128, "u").unwrap()),
        None
    );
    // A 2.0 document needs no extension.
    doc.inner_mut().version = "2.0".into();
    doc.commit().unwrap();
    doc.save_encrypted(&a256, "u", "o", Default::default(), EncryptionProfile::Aes256)
        .unwrap();
    assert!(std::fs::read(&a256).unwrap().starts_with(b"%PDF-2.0"));
    assert_eq!(
        extension_level(&Document::open_with_password(&a256, "u").unwrap()),
        None
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn restrictions_without_an_open_password_still_encrypt() {
    let dir = folder("restrictions");
    let p = dir.join("restricted.pdf");
    marked()
        .save_encrypted(&p, "", "o-pw", no_printing(), EncryptionProfile::Aes256)
        .unwrap();
    let raw = std::fs::read(&p).unwrap();
    assert!(!contains(&raw, b"MARKER-NOTE-7Q"));
    assert_eq!(p_of(&raw), (5, 6, -2056));
    let opened = Document::open(&p).unwrap();
    assert!(opened.is_protected());
    assert!(opened.page_text(0).contains("MARKER-PAGE-7Q"));
    assert!(renders_with(&raw, ""));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn passwords_a_profile_cannot_keep_are_refused() {
    let dir = folder("passwords");
    let doc = marked();
    let refused = |user: &str, profile| {
        let p = dir.join("refused.pdf");
        let r = doc.save_encrypted(&p, user, "owner", Default::default(), profile);
        let ok = r.is_err() && !p.exists();
        if let Err(e) = r {
            // The message never quotes the password.
            assert!(!e.to_string().contains(user), "{e}");
        }
        ok
    };
    assert!(refused("密码", EncryptionProfile::Aes128));
    assert!(refused(&"a".repeat(33), EncryptionProfile::Aes128));
    assert!(refused(&"密".repeat(43), EncryptionProfile::Aes256));
    assert!(refused("pw\u{7}", EncryptionProfile::Aes256));
    let opens = |user: &str, profile| {
        let p = dir.join(format!("{}.pdf", profile_name(profile)));
        doc.save_encrypted(&p, user, "owner", Default::default(), profile)
            .unwrap();
        let ok = Document::open_with_password(&p, user).is_ok();
        let _ = std::fs::remove_file(&p);
        ok
    };
    assert!(opens("pässwörd€", EncryptionProfile::Aes128));
    assert!(opens(&"b".repeat(32), EncryptionProfile::Aes128));
    assert!(opens("密码", EncryptionProfile::Aes256));
    assert!(opens(&"密".repeat(42), EncryptionProfile::Aes256));
    // Characters SASLprep maps: the same typed text opens the file.
    assert!(opens("a\u{a0}b", EncryptionProfile::Aes256));
    let _ = std::fs::remove_dir_all(&dir);
}

fn profile_name(p: EncryptionProfile) -> &'static str {
    p.key()
}

#[test]
fn saving_an_aes256_file_keeps_its_protection() {
    let dir = folder("resave");
    let original = dir.join("protected.pdf");
    let mut two = marked();
    two.insert_blank_pages(1, 1, 300.0, 300.0).unwrap();
    two.save_encrypted(
        &original,
        "user-pw",
        "owner-pw",
        no_printing(),
        EncryptionProfile::Aes256,
    )
    .unwrap();
    let before = p_of(&std::fs::read(&original).unwrap());
    let mut doc = Document::open_with_password(&original, "user-pw").unwrap();
    doc.delete_pages(&[0]).unwrap();
    let saved = dir.join("saved.pdf");
    doc.save_as(&saved).unwrap();
    let raw = std::fs::read(&saved).unwrap();
    assert_eq!(p_of(&raw), before);
    assert!(matches!(Document::open(&saved), Err(Error::PasswordRequired)));
    let reopened = Document::open_with_password(&saved, "owner-pw").unwrap();
    assert_eq!(reopened.page_count(), 1);
    assert_eq!(reopened.encryption().unwrap().method, "AES-256");
    // The encryption dictionary of the file is written as it was.
    let d = encryption_dict(&raw);
    assert_eq!(d.get(b"Length").unwrap().as_i64().unwrap(), 256);
    assert_eq!(name(&d, &[b"CF", b"StdCF", b"AuthEvent"]), b"DocOpen");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn permissions_print_in_full_quality_and_stay_accessible() {
    let dir = folder("permissions");
    let p = dir.join("p.pdf");
    let perms = SecurityPermissions {
        copy: false,
        ..SecurityPermissions::default()
    };
    marked()
        .save_encrypted(&p, "u", "o", perms, EncryptionProfile::Aes256)
        .unwrap();
    let (_, _, bits) = p_of(&std::fs::read(&p).unwrap());
    assert_eq!(bits, -20);
    // Printing at full quality (bit 12) and for accessibility (bit 10).
    assert_ne!(bits & (1 << 11), 0);
    assert_ne!(bits & (1 << 9), 0);
    assert_eq!(
        Document::open_with_password(&p, "u")
            .unwrap()
            .encryption()
            .unwrap()
            .permissions,
        -20
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// A file encrypted for recipients' certificates (a handler Folitecta does
/// not read yet).
fn certificate_protected(dir: &Path) -> PathBuf {
    let mut doc = lopdf::Document::load_mem(&Document::new_blank(1, 200.0, 200.0).unwrap().bytes()).unwrap();
    let enc = doc.add_object(dictionary! {
        "Filter" => "Adobe.PubSec",
        "SubFilter" => "adbe.pkcs7.s5",
        "V" => 5,
        "R" => 5,
    });
    doc.trailer.set("Encrypt", Object::Reference(enc));
    let path = dir.join("certificates.pdf");
    doc.save(&path).unwrap();
    path
}

#[test]
fn certificate_protected_files_are_reported() {
    let dir = folder("pubsec");
    let p = certificate_protected(&dir);
    for result in [Document::open(&p), Document::open_with_password(&p, "anything")] {
        match result {
            Err(Error::UnsupportedProtection(what)) => {
                assert!(what.contains("certificates"));
                // The handler's product name is not shown.
                assert!(!what.contains("Adobe"));
            }
            other => panic!("expected UnsupportedProtection, got {other:?}"),
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn opened_rc4_files_keep_their_encryption() {
    let dir = folder("rc4");
    // RC4 files of older producers (written here with the PDF library).
    for (version, key_length) in [(1, 40usize), (2, 128)] {
        let plain = marked().bytes();
        let mut doc = lopdf::Document::load_mem(&plain).unwrap();
        let id = vec![7u8; 16];
        doc.trailer.set(
            "ID",
            Object::Array(vec![
                Object::String(id.clone(), lopdf::StringFormat::Hexadecimal),
                Object::String(id, lopdf::StringFormat::Hexadecimal),
            ]),
        );
        let state = if version == 1 {
            lopdf::EncryptionState::try_from(lopdf::EncryptionVersion::V1 {
                document: &doc,
                owner_password: "owner",
                user_password: "user",
                permissions: lopdf::Permissions::default(),
            })
        } else {
            lopdf::EncryptionState::try_from(lopdf::EncryptionVersion::V2 {
                document: &doc,
                owner_password: "owner",
                user_password: "user",
                key_length,
                permissions: lopdf::Permissions::default(),
            })
        }
        .unwrap();
        doc.encrypt(&state).unwrap();
        let original = dir.join(format!("rc4-{key_length}.pdf"));
        doc.save(&original).unwrap();
        let mut opened = Document::open_with_password(&original, "user").unwrap();
        assert_eq!(
            opened.encryption().unwrap().method,
            format!("RC4 {key_length}-bit")
        );
        opened.rotate_pages(&[0], 90).unwrap();
        let saved = dir.join(format!("rc4-{key_length}-saved.pdf"));
        opened.save_as(&saved).unwrap();
        let raw = std::fs::read(&saved).unwrap();
        assert!(!contains(&raw, b"MARKER-NOTE-7Q"));
        assert_eq!(p_of(&raw).0, version);
        // The owner password opens it too, decrypted right (not into noise).
        let again = Document::open_with_password(&saved, "owner").unwrap();
        assert_eq!(again.page_size(0).unwrap().rotation, 90);
        assert!(again.page_text(0).contains("MARKER-PAGE-7Q"));
        assert!(renders_with(&raw, "user"));
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// A file protected with `user` / `owner` by the PDF library itself: RC4
/// 40-bit (V1), RC4 128-bit (V2) or AES-128 (V4), as older producers write.
fn library_protected(kind: u8) -> Vec<u8> {
    let plain = marked().bytes();
    let mut doc = lopdf::Document::load_mem(&plain).unwrap();
    let id = vec![3u8; 16];
    doc.trailer.set(
        "ID",
        Object::Array(vec![
            Object::String(id.clone(), lopdf::StringFormat::Hexadecimal),
            Object::String(id, lopdf::StringFormat::Hexadecimal),
        ]),
    );
    let (owner_password, user_password) = ("owner", "user");
    let state = match kind {
        1 => lopdf::EncryptionState::try_from(lopdf::EncryptionVersion::V1 {
            document: &doc,
            owner_password,
            user_password,
            permissions: lopdf::Permissions::default(),
        }),
        2 => lopdf::EncryptionState::try_from(lopdf::EncryptionVersion::V2 {
            document: &doc,
            owner_password,
            user_password,
            key_length: 128,
            permissions: lopdf::Permissions::default(),
        }),
        _ => {
            let mut filters: std::collections::BTreeMap<
                Vec<u8>,
                Arc<dyn lopdf::encryption::crypt_filters::CryptFilter>,
            > = std::collections::BTreeMap::new();
            filters.insert(
                b"StdCF".to_vec(),
                Arc::new(lopdf::encryption::crypt_filters::Aes128CryptFilter),
            );
            lopdf::EncryptionState::try_from(lopdf::EncryptionVersion::V4 {
                document: &doc,
                encrypt_metadata: true,
                crypt_filters: filters,
                stream_filter: b"StdCF".to_vec(),
                string_filter: b"StdCF".to_vec(),
                owner_password,
                user_password,
                permissions: lopdf::Permissions::default(),
            })
        }
    }
    .unwrap();
    doc.encrypt(&state).unwrap();
    let mut out = Vec::new();
    doc.save_to(&mut out).unwrap();
    out
}

#[test]
fn owner_passwords_decrypt_and_save_right() {
    // The PDF library makes the file key of revisions 2 to 4 from any
    // password as if it were the user password: opened with the owner
    // password, the content was noise, and a saved copy kept the noise.
    let dir = folder("owner");
    let mut files: Vec<(String, Vec<u8>, &str, &str)> = (1..=4)
        .filter(|k| *k != 3)
        .map(|k| (format!("library-{k}"), library_protected(k), "user", "owner"))
        .collect();
    for profile in EncryptionProfile::ALL {
        let p = dir.join(format!("{}.pdf", profile.key()));
        marked()
            .save_encrypted(&p, "user-pw", "owner-pw", no_printing(), profile)
            .unwrap();
        files.push((
            profile.key().to_string(),
            std::fs::read(&p).unwrap(),
            "user-pw",
            "owner-pw",
        ));
    }
    for (name, raw, user, owner) in files {
        let mut doc = Document::from_bytes(raw.clone(), None, Some(owner)).unwrap();
        assert!(doc.page_text(0).contains("MARKER-PAGE-7Q"), "{name}");
        assert!(
            doc.annotations(0)
                .unwrap()
                .iter()
                .any(|a| a.contents.contains("MARKER-NOTE-7Q")),
            "{name}"
        );
        let copy = dir.join(format!("{name}-copy.pdf"));
        doc.save_copy(&copy).unwrap();
        let saved = std::fs::read(&copy).unwrap();
        assert_eq!(p_of(&saved), p_of(&raw), "{name}");
        for pw in [user, owner] {
            let again = Document::from_bytes(saved.clone(), None, Some(pw)).unwrap();
            assert!(again.page_text(0).contains("MARKER-PAGE-7Q"), "{name} {pw}");
        }
        assert!(renders_with(&saved, user), "{name}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// The page text and the note of `doc` are the ones `marked` wrote.
fn readable(doc: &Document) -> bool {
    doc.page_text(0).contains("MARKER-PAGE-7Q")
        && doc
            .annotations(0)
            .unwrap()
            .iter()
            .any(|a| a.contents.contains("MARKER-NOTE-7Q"))
}

/// `marked` protected with RC4 128-bit by the PDF library (as older
/// producers write), with these passwords.
fn rc4_with(user: &str, owner: &str) -> Vec<u8> {
    let plain = marked().bytes();
    let mut doc = lopdf::Document::load_mem(&plain).unwrap();
    let id = vec![6u8; 16];
    doc.trailer.set(
        "ID",
        Object::Array(vec![
            Object::String(id.clone(), lopdf::StringFormat::Hexadecimal),
            Object::String(id, lopdf::StringFormat::Hexadecimal),
        ]),
    );
    let state = lopdf::EncryptionState::try_from(lopdf::EncryptionVersion::V2 {
        document: &doc,
        owner_password: owner,
        user_password: user,
        key_length: 128,
        permissions: lopdf::Permissions::default(),
    })
    .unwrap();
    doc.encrypt(&state).unwrap();
    let mut out = Vec::new();
    doc.save_to(&mut out).unwrap();
    out
}

#[test]
fn passwords_outside_ascii_open_the_content_and_keep_it() {
    // Revisions 2 to 4 encode passwords in PDFDocEncoding; the PDF library
    // made the key from their UTF-8 bytes, so such files opened as noise and
    // a save wrote the noise.
    let dir = folder("latin");
    let mut files: Vec<(String, Vec<u8>, &str, &str)> = Vec::new();
    for (user, owner) in [("pässwörd€", "città perché"), ("città perché", "Łódka-owner")] {
        for profile in EncryptionProfile::ALL {
            let p = dir.join("source.pdf");
            marked()
                .save_encrypted(&p, user, owner, no_printing(), profile)
                .unwrap();
            files.push((
                format!("{} {user}", profile.key()),
                std::fs::read(&p).unwrap(),
                user,
                owner,
            ));
        }
        files.push((format!("rc4 {user}"), rc4_with(user, owner), user, owner));
    }
    for (name, raw, user, owner) in files {
        for pw in [user, owner] {
            let mut doc = Document::from_bytes(raw.clone(), None, Some(pw)).unwrap();
            assert!(readable(&doc), "{name}, opened with {pw}");
            doc.add_note(0, 60.0, 600.0, "after", "Bob", [0.3, 0.6, 1.0])
                .unwrap();
            let saved = dir.join("saved.pdf");
            doc.save_as(&saved).unwrap();
            let bytes = std::fs::read(&saved).unwrap();
            assert_eq!(p_of(&bytes), p_of(&raw), "{name}");
            for again in [user, owner] {
                let back = Document::from_bytes(bytes.clone(), None, Some(again)).unwrap();
                assert!(readable(&back), "{name}, saved after {pw}, opened with {again}");
                assert_eq!(back.annotations(0).unwrap().len(), 2, "{name}");
            }
        }
        assert!(matches!(
            Document::from_bytes(raw.clone(), None, Some("wrong")),
            Err(Error::WrongPassword)
        ));
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn owner_passwords_of_version_4_files_without_a_length() {
    // Version 4 fixes the key at 128 bits, so /Length may be left out.
    let dir = folder("nolength");
    let p = dir.join("aes128.pdf");
    marked()
        .save_encrypted(
            &p,
            "user-pw",
            "owner-pw",
            no_printing(),
            EncryptionProfile::Aes128,
        )
        .unwrap();
    let mut raw = std::fs::read(&p).unwrap();
    let standard = raw.windows(9).position(|w| w == b"/Standard").unwrap();
    let open = raw[..standard].iter().rposition(|c| *c == b'<').unwrap() - 1;
    let at = open + raw[open..].windows(11).position(|w| w == b"/Length 128").unwrap();
    raw[at..at + 11].copy_from_slice(b"           ");
    assert!(encryption_dict(&raw).get(b"Length").is_err());
    for pw in ["user-pw", "owner-pw"] {
        let doc = Document::from_bytes(raw.clone(), None, Some(pw)).unwrap();
        assert!(readable(&doc), "{pw}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn protected_copies_always_render() {
    // Binary strings are written in hexadecimal: in literal strings, readers
    // turn line ends among the encrypted bytes into other bytes (about one
    // copy in a thousand failed to open in the renderer before).
    let plain = marked().bytes();
    for i in 0..1000 {
        let profile = if i % 4 == 3 {
            EncryptionProfile::Aes128
        } else {
            EncryptionProfile::Aes256
        };
        let raw =
            folitecta_core::security::protected_copy(&plain, "user-pw", "owner-pw", no_printing(), profile)
                .unwrap();
        let r = Renderer::new_with_password(Arc::new(raw), "user-pw");
        assert!(r.is_ok_and(|r| r.page_count() == 1), "copy {i} ({profile:?})");
    }
}

/// `marked`, with an XMP metadata stream, protected by the PDF library in
/// one of the ways other producers protect files.
fn library_profile(kind: &str) -> Vec<u8> {
    use lopdf::encryption::crypt_filters::{
        Aes128CryptFilter, Aes256CryptFilter, CryptFilter, IdentityCryptFilter,
    };
    let mut source = marked();
    let meta = source.inner_mut().add_object(lopdf::Stream::new(
        dictionary! { "Type" => "Metadata", "Subtype" => "XML" },
        b"<x:xmpmeta>MARKER-XMP</x:xmpmeta>".to_vec(),
    ));
    source.inner_mut().catalog_mut().unwrap().set("Metadata", meta);
    source.commit().unwrap();
    let mut doc = lopdf::Document::load_mem(&source.bytes()).unwrap();
    let id = vec![4u8; 16];
    doc.trailer.set(
        "ID",
        Object::Array(vec![
            Object::String(id.clone(), lopdf::StringFormat::Hexadecimal),
            Object::String(id, lopdf::StringFormat::Hexadecimal),
        ]),
    );
    let mut filters: std::collections::BTreeMap<Vec<u8>, Arc<dyn CryptFilter>> = Default::default();
    let (owner_password, user_password) = ("owner-pw", "user-pw");
    let permissions = lopdf::Permissions::default();
    let state = match kind {
        "r5" => {
            filters.insert(b"StdCF".to_vec(), Arc::new(Aes256CryptFilter));
            #[allow(deprecated)]
            lopdf::EncryptionState::try_from(lopdf::EncryptionVersion::R5 {
                encrypt_metadata: true,
                crypt_filters: filters,
                file_encryption_key: &[7u8; 32],
                stream_filter: b"StdCF".to_vec(),
                string_filter: b"StdCF".to_vec(),
                owner_password,
                user_password,
                permissions,
            })
        }
        _ => {
            filters.insert(b"StdCF".to_vec(), Arc::new(Aes128CryptFilter));
            let string_filter = if kind == "identity" {
                filters.insert(b"Identity".to_vec(), Arc::new(IdentityCryptFilter));
                b"Identity".to_vec()
            } else {
                b"StdCF".to_vec()
            };
            lopdf::EncryptionState::try_from(lopdf::EncryptionVersion::V4 {
                document: &doc,
                encrypt_metadata: kind != "metadata-plain",
                crypt_filters: filters,
                stream_filter: b"StdCF".to_vec(),
                string_filter,
                owner_password,
                user_password,
                permissions,
            })
        }
    }
    .unwrap();
    doc.encrypt(&state).unwrap();
    if kind == "attachments-apart" {
        let enc = doc.trailer.get(b"Encrypt").unwrap().as_reference().unwrap();
        let d = doc.get_dictionary_mut(enc).unwrap();
        d.set("EFF", Object::Name(b"EmbeddedFiles".to_vec()));
    }
    let mut out = Vec::new();
    doc.save_to(&mut out).unwrap();
    out
}

#[test]
fn opened_profiles_are_kept_or_refused() {
    let dir = folder("opened");
    // Kept: AES-256 of revision 5, and AES-128 with the metadata left plain.
    for (kind, method) in [("r5", "AES-256 (R5)"), ("metadata-plain", "AES-128")] {
        let raw = library_profile(kind);
        for pw in ["user-pw", "owner-pw"] {
            let mut doc = Document::from_bytes(raw.clone(), None, Some(pw)).unwrap();
            assert!(readable(&doc), "{kind} {pw}");
            assert_eq!(doc.encryption().unwrap().method, method);
            doc.rotate_pages(&[0], 90).unwrap();
            let p = dir.join(format!("{kind}.pdf"));
            doc.save_as(&p).unwrap();
            let saved = std::fs::read(&p).unwrap();
            let (v, r, _) = p_of(&saved);
            assert_eq!((v, r), (p_of(&raw).0, p_of(&raw).1), "{kind}");
            // The metadata stays readable without the password exactly when
            // the file says so.
            assert_eq!(
                contains(&saved, b"MARKER-XMP"),
                kind == "metadata-plain",
                "{kind}"
            );
            assert!(!contains(&saved, b"MARKER-NOTE-7Q"));
            let back = Document::from_bytes(saved, None, Some("user-pw")).unwrap();
            assert!(readable(&back), "{kind}");
            assert_eq!(back.page_size(0).unwrap().rotation, 90);
        }
    }
    // Refused: strings left unencrypted (Identity), attachments encrypted
    // apart. They open; protected files are not written; an unrestricted
    // copy stays an explicit choice.
    for kind in ["identity", "attachments-apart"] {
        let raw = library_profile(kind);
        let mut doc = Document::from_bytes(raw, None, Some("user-pw")).unwrap();
        assert!(doc.page_text(0).contains("MARKER-PAGE-7Q"), "{kind}");
        let p = dir.join(format!("{kind}.pdf"));
        assert!(
            matches!(doc.save_as(&p), Err(Error::ProtectionNotKept(_))),
            "{kind}"
        );
        assert!(!p.exists());
        doc.save_unprotected_copy(&p).unwrap();
        assert!(Document::open(&p).is_ok());
    }
    let _ = std::fs::remove_dir_all(&dir);
}
