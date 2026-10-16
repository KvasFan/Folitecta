//! Digital IDs, signing, validation, incremental updates and certification.

use folitecta_core::Document;
use folitecta_core::digsig::{DigitalId, IdentityInfo, KeyKind, SignOptions, SignPlacement};

fn identity(name: &str) -> IdentityInfo {
    IdentityInfo {
        name: name.into(),
        organization: "Example, Inc.".into(),
        unit: "Legal".into(),
        email: "signer@example.org".into(),
        country: "IT".into(),
    }
}

fn ec_id(name: &str) -> DigitalId {
    DigitalId::create_self_signed(&identity(name), KeyKind::EcP256, 3).unwrap()
}

fn blank() -> Document {
    let mut doc = Document::new_blank(2, 400.0, 500.0).unwrap();
    doc.add_note(0, 30.0, 470.0, "draft", "Ann", [1.0, 0.8, 0.2])
        .unwrap();
    doc
}

fn reopen(bytes: Vec<u8>) -> Document {
    Document::from_bytes(bytes, None, None).unwrap()
}

#[test]
fn self_signed_ids_round_trip_through_pkcs12() {
    for kind in [KeyKind::EcP256, KeyKind::Rsa2048] {
        let id = DigitalId::create_self_signed(&identity("Ada Rossi"), kind, 2).unwrap();
        let info = id.info();
        assert_eq!(info.common_name, "Ada Rossi");
        assert_eq!(info.organization, "Example, Inc.");
        assert_eq!(info.email, "signer@example.org");
        assert_eq!(info.country, "IT");
        assert!(info.self_signed);
        assert!(info.not_after_unix > info.not_before_unix);
        let pfx = id.to_pkcs12("s3cret").unwrap();
        let back = DigitalId::from_pkcs12(&pfx, "s3cret").unwrap();
        assert_eq!(back.certificate, id.certificate);
        let err = DigitalId::from_pkcs12(&pfx, "wrong").unwrap_err().to_string();
        assert!(err.contains("password"), "{err}");
    }
    assert!(DigitalId::from_pkcs12(b"not a pfx", "x").is_err());
}

#[test]
fn signing_and_validation() {
    let id = ec_id("Signer One");
    let doc = blank();
    let opts = SignOptions {
        reason: "Approval".into(),
        location: "Milano".into(),
        placement: SignPlacement::New {
            page: 0,
            rect: [40.0, 40.0, 240.0, 100.0],
        },
        ..SignOptions::default()
    };
    let signed = doc.sign(&id, &opts).unwrap();
    let doc = reopen(signed.clone());
    assert!(doc.has_signatures());
    let status = doc.validate_signatures(&[]);
    assert_eq!(status.len(), 1);
    let s = &status[0];
    assert!(s.valid, "{:?}", s.problems);
    assert!(s.covers_document);
    assert!(!s.trusted);
    assert_eq!(s.signer, "Signer One");
    assert_eq!(s.reason, "Approval");
    assert_eq!(s.page, Some(0));
    assert_eq!(s.cert.as_ref().unwrap().common_name, "Signer One");
    // Trusting the certificate makes the identity trusted.
    let trusted = doc.validate_signatures(std::slice::from_ref(&id.certificate));
    assert!(trusted[0].trusted);
    // The visible signature renders.
    doc.render_page_png(0, 1.0).unwrap();
    // Tampering with a signed byte breaks the signature.
    let mut tampered = signed.clone();
    let pos = tampered.windows(5).position(|w| w == b"draft").unwrap();
    tampered[pos] = b'D';
    let bad = reopen(tampered).validate_signatures(&[]);
    assert!(!bad[0].valid);
    assert!(!bad[0].problems.is_empty());
}

#[test]
fn rsa_signatures_validate_too() {
    let id = DigitalId::create_self_signed(&identity("Rsa Signer"), KeyKind::Rsa2048, 1).unwrap();
    let signed = blank().sign(&id, &SignOptions::default()).unwrap();
    let s = reopen(signed).validate_signatures(&[]);
    assert!(s[0].valid, "{:?}", s[0].problems);
    assert_eq!(s[0].digest_algorithm, "SHA-256");
}

#[test]
fn later_edits_and_second_signatures_keep_earlier_signatures() {
    let first = ec_id("First");
    let second = ec_id("Second");
    let signed = blank().sign(&first, &SignOptions::default()).unwrap();
    // Edit and save: the update is appended, the first signature stays intact.
    let mut doc = reopen(signed.clone());
    doc.add_note(1, 50.0, 400.0, "after signing", "Bob", [0.3, 0.6, 1.0])
        .unwrap();
    let dir = std::env::temp_dir().join(format!("folitecta-sig-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("edited.pdf");
    doc.save_as(&path).unwrap();
    let saved = std::fs::read(&path).unwrap();
    assert!(
        saved.starts_with(&signed),
        "an incremental update keeps the signed bytes"
    );
    let edited = reopen(saved);
    assert_eq!(edited.comments().len(), 2);
    let s = edited.validate_signatures(&[]);
    assert!(s[0].valid && !s[0].covers_document, "{s:?}");
    // The signed revision opens as it was signed.
    let original = reopen(edited.signed_revision(&s[0].field).unwrap());
    assert_eq!(original.comments().len(), 1);
    // A second signature.
    let twice = edited.sign(&second, &SignOptions::default()).unwrap();
    let both = reopen(twice).validate_signatures(&[]);
    assert_eq!(both.len(), 2);
    assert!(both.iter().all(|s| s.valid), "{both:?}");
    assert_eq!(both.iter().filter(|s| s.covers_document).count(), 1);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn certification_and_clearing() {
    let id = ec_id("Certifier");
    let opts = SignOptions {
        certify: 1,
        ..SignOptions::default()
    };
    let signed = blank().sign(&id, &opts).unwrap();
    let mut doc = reopen(signed);
    let s = doc.validate_signatures(&[]);
    assert_eq!(s[0].certify, Some(1));
    assert!(s[0].valid && s[0].covers_document);
    // A certified document cannot be certified again.
    assert!(doc.sign(&id, &opts).is_err());
    // Changes after certification are reported.
    doc.add_note(0, 100.0, 100.0, "late", "Bob", [1.0, 0.0, 0.0])
        .unwrap();
    let dir = std::env::temp_dir().join(format!("folitecta-cert-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("certified.pdf");
    doc.save_copy(&path).unwrap();
    let changed = Document::open(&path).unwrap();
    let s = changed.validate_signatures(&[]);
    assert!(
        s[0].problems.iter().any(|p| p.contains("certification")),
        "{:?}",
        s[0].problems
    );
    // Clearing removes the signatures and the certification.
    let mut cleared = Document::open(&path).unwrap();
    assert_eq!(cleared.clear_signatures().unwrap(), 1);
    assert!(!cleared.has_signatures());
    assert_eq!(cleared.empty_signature_fields().len(), 1);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn existing_empty_fields_can_be_signed() {
    let mut doc = blank();
    doc.add_form_field(
        0,
        folitecta_core::NewField::Signature,
        [50.0, 50.0, 250.0, 110.0],
        &Default::default(),
    )
    .unwrap();
    let field = doc.empty_signature_fields().pop().unwrap();
    let id = ec_id("Field Signer");
    let signed = doc
        .sign(
            &id,
            &SignOptions {
                placement: SignPlacement::Field(field.clone()),
                ..SignOptions::default()
            },
        )
        .unwrap();
    let doc = reopen(signed);
    let s = doc.validate_signatures(&[]);
    assert_eq!(s[0].field, field);
    assert!(s[0].valid, "{:?}", s[0].problems);
    assert!(doc.empty_signature_fields().is_empty());
}

#[test]
fn ids_exported_by_other_software_are_read() {
    // Test-only self-signed IDs exported with the classic PKCS#12 encryption
    // (password "pw123").
    for (file, name, key) in [
        ("legacy-rsa.pfx", "Windows Test Signer", "RSA 2048"),
        ("legacy-ec.pfx", "Windows EC Signer", "ECDSA P-256"),
    ] {
        let data = std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures")
                .join(file),
        )
        .unwrap();
        let id = DigitalId::from_pkcs12(&data, "pw123").unwrap();
        assert_eq!(id.info().common_name, name);
        assert_eq!(id.info().key, key);
        assert!(DigitalId::from_pkcs12(&data, "nope").is_err());
        let signed = blank().sign(&id, &SignOptions::default()).unwrap();
        let s = reopen(signed).validate_signatures(&[]);
        assert!(s[0].valid, "{file}: {:?}", s[0].problems);
    }
}
