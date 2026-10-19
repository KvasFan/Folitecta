//! Later revisions of a signed file that claim to add only validation
//! material: they are recognised from their structure, never from what they
//! say of themselves. A revision that changes what the document shows, by
//! whatever route, keeps the signature's "changed after it was signed" (or
//! the certification problem), whatever the store points to.

use std::collections::BTreeMap;

use folitecta_core::Document;
use folitecta_core::digsig::ocsp::{self, OcspAnswer, OcspTimes};
use folitecta_core::digsig::{
    CertProfile, Certificate, Check, DigitalId, ExtendedUsage, IdentityInfo, KeyKind, RevocationPool,
    SignOptions, SignatureStatus, cert_to_der, timestamp,
};
use lopdf::{Dictionary, Object, ObjectId, Stream, dictionary};

const DAY: i64 = 86_400;
const SIGN: i64 = 1_792_324_800;
const NOW: i64 = SIGN + 2 * DAY;

fn who(name: &str) -> IdentityInfo {
    IdentityInfo {
        name: name.into(),
        ..IdentityInfo::default()
    }
}

fn id(name: &str, usages: Vec<ExtendedUsage>) -> DigitalId {
    DigitalId::create_with(
        &who(name),
        KeyKind::EcP256,
        &CertProfile {
            not_before: SIGN - 400 * DAY,
            not_after: SIGN + 400 * DAY,
            usages,
            ..CertProfile::default()
        },
    )
    .unwrap()
}

struct Fixture {
    signed: Vec<u8>,
    trusted: Vec<Certificate>,
    der: Vec<u8>,
    ca: DigitalId,
    signer: DigitalId,
}

/// A signed blank page (certified when `certify`), whose page also points
/// to an annotation that does not exist yet.
fn fixture(certify: u8) -> Fixture {
    let ca = DigitalId::create_with(
        &who("Revision Root CA"),
        KeyKind::EcP256,
        &CertProfile {
            not_before: SIGN - 400 * DAY,
            not_after: SIGN + 400 * DAY,
            ca: true,
            ..CertProfile::default()
        },
    )
    .unwrap();
    let mut s = ca
        .issue(
            &who("Revision Signer"),
            KeyKind::EcP256,
            &CertProfile {
                not_before: SIGN - DAY,
                not_after: SIGN + 100 * DAY,
                ..CertProfile::default()
            },
        )
        .unwrap();
    // The authority comes from the trusted list.
    s.chain.clear();
    let t = id("Revision Authority", vec![ExtendedUsage::TimeStamping]);
    let mut doc = Document::new_blank(1, 300.0, 300.0).unwrap();
    let page = doc.page_id(0).unwrap();
    doc.inner_mut()
        .get_dictionary_mut(page)
        .unwrap()
        .set("Annots", vec![Object::Reference((900, 0))]);
    doc.commit().unwrap();
    let opts = SignOptions {
        time: SIGN as u64,
        certify,
        ..SignOptions::default()
    };
    let mut send = |req: &[u8]| timestamp::respond(&t, req, SIGN as u64, 1);
    let signed = doc.sign_with(&s, &opts, Some(&mut send)).unwrap();
    Fixture {
        signed,
        trusted: vec![ca.certificate.clone(), t.certificate.clone()],
        der: cert_to_der(&t.certificate),
        ca,
        signer: s,
    }
}

/// An incremental update of `base` holding the objects `f` gives (with the
/// trailer it may change).
fn update(
    base: &[u8],
    f: impl FnOnce(&lopdf::Document, &mut BTreeMap<ObjectId, Object>, &mut Dictionary),
) -> Vec<u8> {
    let prev = lopdf::Document::load_mem(base).unwrap();
    let mut inc = lopdf::IncrementalDocument::create_from(base.to_vec(), prev.clone());
    f(
        &prev,
        &mut inc.new_document.objects,
        &mut inc.new_document.trailer,
    );
    let top = inc.new_document.objects.keys().map(|k| k.0).max().unwrap_or(0);
    inc.new_document.max_id = inc.new_document.max_id.max(top).max(prev.max_id);
    let mut out = Vec::new();
    inc.save_to(&mut out).unwrap();
    out
}

fn root(doc: &lopdf::Document) -> ObjectId {
    doc.trailer.get(b"Root").unwrap().as_reference().unwrap()
}

fn content_of_page(doc: &lopdf::Document) -> ObjectId {
    let page = *doc.get_pages().values().next().unwrap();
    doc.get_dictionary(page)
        .unwrap()
        .get(b"Contents")
        .unwrap()
        .as_reference()
        .unwrap()
}

fn cert_stream(der: &[u8]) -> Object {
    let mut s = Stream::new(Dictionary::new(), der.to_vec());
    let _ = s.compress();
    Object::Stream(s)
}

/// The catalog of `prev` with `/DSS` set to `dss`.
fn catalog_with(prev: &lopdf::Document, dss: Object) -> Object {
    let mut cat = prev.get_dictionary(root(prev)).unwrap().clone();
    cat.set("DSS", dss);
    Object::Dictionary(cat)
}

/// A store with one certificate, as another producer may write it.
fn material(prev: &lopdf::Document, objects: &mut BTreeMap<ObjectId, Object>, der: &[u8]) -> ObjectId {
    let n = prev.max_id;
    let cert = (n + 1, 0);
    let dss = (n + 2, 0);
    objects.insert(cert, cert_stream(der));
    objects.insert(
        dss,
        Object::Dictionary(dictionary! { "Type" => "DSS", "Certs" => vec![Object::Reference(cert)] }),
    );
    objects.insert(root(prev), catalog_with(prev, Object::Reference(dss)));
    dss
}

fn status(bytes: &[u8], trusted: &[Certificate]) -> SignatureStatus {
    let doc = Document::from_bytes(bytes.to_vec(), None, None).unwrap();
    doc.validate_signatures_with(trusted, &RevocationPool::default(), NOW)
        .into_iter()
        .find(|s| !s.document_time_stamp)
        .unwrap()
}

/// The update is not one of validation material: the signature reports a
/// change (the certification problem when it certifies).
fn assert_changed(name: &str, bytes: &[u8], f: &Fixture) {
    assert!(
        bytes.starts_with(&f.signed),
        "{name}: an update of the signed file"
    );
    let s = status(bytes, &f.trusted);
    assert!(s.valid, "{name}: {:?}", s.problems);
    assert!(
        !s.later_revisions_material,
        "{name}: taken for validation material"
    );
    assert_eq!(s.checks.integrity, Check::Warning, "{name}");
    let expected = if s.certify == Some(1) {
        "certification does not allow"
    } else {
        "(in a later revision)"
    };
    assert!(
        s.problems.iter().any(|p| p.contains(expected)),
        "{name}: {:?}",
        s.problems
    );
}

fn assert_material(name: &str, bytes: &[u8], f: &Fixture) {
    let s = status(bytes, &f.trusted);
    assert!(s.valid && s.later_revisions_material, "{name}: {:?}", s.problems);
    assert_eq!(s.checks.integrity, Check::Passed, "{name}");
    assert!(s.problems.is_empty(), "{name}: {:?}", s.problems);
}

fn attacks(f: &Fixture) {
    let der = f.der.clone();
    // A page content stream changed and listed in the store.
    let listed = update(&f.signed, |prev, objects, _| {
        let content = content_of_page(prev);
        objects.insert(
            content,
            Object::Stream(Stream::new(
                Dictionary::new(),
                b"BT /F1 24 Tf 20 20 Td (CHANGED) Tj ET".to_vec(),
            )),
        );
        let dss = (prev.max_id + 1, 0);
        objects.insert(
            dss,
            Object::Dictionary(dictionary! { "Certs" => vec![Object::Reference(content)] }),
        );
        objects.insert(root(prev), catalog_with(prev, Object::Reference(dss)));
    });
    assert_changed("content listed in /Certs", &listed, f);
    // The same through a key of the store nobody reads.
    let other_key = update(&f.signed, |prev, objects, _| {
        let content = content_of_page(prev);
        objects.insert(
            content,
            Object::Stream(Stream::new(
                Dictionary::new(),
                b"0 0 1 rg 0 0 300 300 re f".to_vec(),
            )),
        );
        objects.insert(
            root(prev),
            catalog_with(
                prev,
                Object::Dictionary(dictionary! { "X" => Object::Reference(content) }),
            ),
        );
    });
    assert_changed("content under another key of /DSS", &other_key, f);
    // The catalog changes something else too.
    let open_action = update(&f.signed, |prev, objects, _| {
        material(prev, objects, &der);
        let r = root(prev);
        let page = *prev.get_pages().values().next().unwrap();
        if let Some(Object::Dictionary(c)) = objects.get_mut(&r) {
            c.set(
                "OpenAction",
                vec![Object::Reference(page), Object::Name(b"Fit".to_vec())],
            );
        }
    });
    assert_changed("catalog with an open action", &open_action, f);
    // An extension entry that points into the document.
    let extensions = update(&f.signed, |prev, objects, _| {
        material(prev, objects, &der);
        let r = root(prev);
        let pages = prev.get_dictionary(r).unwrap().get(b"Pages").unwrap().clone();
        if let Some(Object::Dictionary(c)) = objects.get_mut(&r) {
            c.set("Extensions", dictionary! { "ESIC" => pages });
        }
    });
    assert_changed("/Extensions pointing at the pages", &extensions, f);
    // An object the signed page already pointed to, missing then, now
    // defined and listed in the store.
    let dangling = update(&f.signed, |prev, objects, _| {
        let annot = (900, 0);
        objects.insert(
            annot,
            Object::Dictionary(dictionary! {
                "Type" => "Annot", "Subtype" => "FreeText",
                "Rect" => vec![10.into(), 10.into(), 200.into(), 60.into()],
                "Contents" => Object::string_literal("ADDED"),
                "DA" => Object::string_literal("/Helv 12 Tf 0 g"),
            }),
        );
        let dss = (prev.max_id.max(900) + 1, 0);
        objects.insert(
            dss,
            Object::Dictionary(dictionary! { "Certs" => vec![Object::Reference(annot)] }),
        );
        objects.insert(root(prev), catalog_with(prev, Object::Reference(dss)));
    });
    assert_changed("dangling reference defined", &dangling, f);
    // A content stream rewritten with the type of a cross-reference stream
    // or of an object stream (written under another name of the same length,
    // then renamed: the PDF library does not write such objects itself).
    for (kind, placeholder) in [("XRef", "XRfx"), ("ObjStm", "ObjStx")] {
        let mut retyped = update(&f.signed, |prev, objects, _| {
            material(prev, objects, &der);
            let content = content_of_page(prev);
            objects.insert(
                content,
                Object::Stream(Stream::new(
                    dictionary! { "Type" => Object::Name(placeholder.as_bytes().to_vec()) },
                    b"0 0 1 rg 0 0 300 300 re f".to_vec(),
                )),
            );
        });
        let name = format!("/{placeholder}");
        let at = retyped
            .windows(name.len())
            .rposition(|w| w == name.as_bytes())
            .unwrap();
        retyped[at + 1..at + name.len()].copy_from_slice(kind.as_bytes());
        assert_changed(kind, &retyped, f);
    }
    // The document information changes more than its dates.
    let title = update(&f.signed, |prev, objects, _| {
        material(prev, objects, &der);
        let info = prev.trailer.get(b"Info").unwrap().as_reference().unwrap();
        let mut d = prev.get_dictionary(info).unwrap().clone();
        d.set("Title", Object::string_literal("Another title"));
        objects.insert(info, Object::Dictionary(d));
    });
    assert_changed("title", &title, f);
    // Objects appended after the last end of a revision.
    let mut trailing = update(&f.signed, |prev, objects, _| {
        material(prev, objects, &der);
    });
    let content = content_of_page(&lopdf::Document::load_mem(&f.signed).unwrap());
    trailing.extend_from_slice(
        format!(
            "\n{} 0 obj\n<< /Length 25 >>\nstream\n0 0 1 rg 0 0 300 300 re f\nendstream\nendobj\n",
            content.0
        )
        .as_bytes(),
    );
    assert_changed("trailing objects", &trailing, f);
    // An object written in the update that its cross-reference section does
    // not point to (another reader rebuilding the table would see it).
    let hidden_id = 777;
    let mut hidden = update(&f.signed, |prev, objects, _| {
        material(prev, objects, &der);
        objects.insert(
            (hidden_id, 0),
            Object::Stream(Stream::new(
                Dictionary::new(),
                b"0 0 1 rg 0 0 300 300 re f".to_vec(),
            )),
        );
    });
    let header = format!("{hidden_id} 0 obj");
    let renamed = format!("{:>width$} 0 obj", content.0, width = hidden_id.to_string().len());
    let at = hidden
        .windows(header.len())
        .rposition(|w| w == header.as_bytes())
        .unwrap();
    hidden[at..at + header.len()].copy_from_slice(renamed.as_bytes());
    assert_changed("object hidden from the cross-reference section", &hidden, f);
}

#[test]
fn changes_disguised_as_material_are_changes() {
    attacks(&fixture(0));
}

#[test]
fn changes_disguised_as_material_break_certification() {
    attacks(&fixture(1));
}

#[test]
fn material_of_other_producers_is_recognised_and_kept() {
    let f = fixture(1);
    let der = f.der.clone();
    // A store with an indirect array, an entry with a time stamp, a key
    // Folitecta does not know, an item it cannot decode, and an object
    // nothing reaches.
    let mut entry_key = String::new();
    let theirs = update(&f.signed, |prev, objects, _| {
        let n = prev.max_id;
        let cert = (n + 1, 0);
        let certs = (n + 2, 0);
        let junk = (n + 3, 0);
        let vri = (n + 4, 0);
        let dss = (n + 5, 0);
        objects.insert(cert, cert_stream(&der));
        objects.insert(
            junk,
            Object::Stream(Stream::new(Dictionary::new(), b"not a certificate".to_vec())),
        );
        objects.insert(
            certs,
            Object::Array(vec![Object::Reference(cert), Object::Reference(junk)]),
        );
        entry_key = "0123456789ABCDEF0123456789ABCDEF01234567".into();
        objects.insert(
            vri,
            Object::Dictionary(dictionary! {
                entry_key.as_str() => dictionary! { "Cert" => vec![Object::Reference(cert)], "TS" => Object::Reference(junk) },
            }),
        );
        objects.insert(
            dss,
            Object::Dictionary(dictionary! {
                "Type" => "DSS", "Certs" => Object::Reference(certs), "VRI" => Object::Reference(vri), "Foo" => 1,
            }),
        );
        objects.insert((n + 6, 0), Object::Dictionary(dictionary! { "Unused" => true }));
        objects.insert(root(prev), catalog_with(prev, Object::Reference(dss)));
    });
    assert_material("material of another producer", &theirs, &f);
    let doc = Document::from_bytes(theirs.clone(), None, None).unwrap();
    let dss = doc.dss();
    assert_eq!(dss.certs, vec![der.clone(), b"not a certificate".to_vec()]);
    assert_eq!(dss.skipped, 0, "an undecodable item is read as bytes");
    assert!(dss.vri.get(&entry_key).is_some_and(|e| e.ts));
    // Folitecta's material on top keeps all of it.
    let mut doc = doc;
    let answer = ocsp::respond(
        &f.ca,
        &f.ca.certificate,
        &ocsp::request(&f.signer.certificate, &f.ca.certificate).unwrap(),
        &|_| OcspAnswer::Good,
        OcspTimes {
            produced_at: SIGN + 60,
            this_update: SIGN + 60,
            next_update: None,
        },
    )
    .unwrap();
    let pool = RevocationPool {
        data: folitecta_core::digsig::RevocationArchive {
            crls: Vec::new(),
            ocsps: vec![answer],
        },
        failures: Vec::new(),
    };
    let report = doc.add_validation_material(&f.trusted, &pool, NOW).unwrap();
    assert!(
        report.changed && report.certs_added == 1 && report.vri_written == 1,
        "{report:?}"
    );
    let path = std::env::temp_dir().join(format!("folitecta-theirs-{}.pdf", std::process::id()));
    doc.save_as(&path).unwrap();
    let saved = std::fs::read(&path).unwrap();
    assert!(saved.starts_with(&theirs));
    let raw = lopdf::Document::load_mem(&saved).unwrap();
    let dss = raw
        .get_dictionary(
            raw.get_dictionary(root(&raw))
                .unwrap()
                .get(b"DSS")
                .unwrap()
                .as_reference()
                .unwrap(),
        )
        .unwrap();
    assert_eq!(dss.get(b"Foo").unwrap().as_i64().unwrap(), 1);
    let certs = raw
        .get_object(dss.get(b"Certs").unwrap().as_reference().unwrap())
        .unwrap()
        .as_array()
        .unwrap();
    assert_eq!(certs.len(), 3, "the existing items stay, the new one follows");
    let vri = raw
        .get_dictionary(dss.get(b"VRI").unwrap().as_reference().unwrap())
        .unwrap();
    assert_eq!(vri.len(), 2, "the entry of the other producer stays");
    assert_material("material on top of another producer's", &saved, &f);
    let _ = std::fs::remove_file(&path);
}
