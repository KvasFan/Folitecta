//! Time stamps (RFC 3161): requests and tokens, document time stamps,
//! signatures with time stamps and the separate checks of a signature.

use folitecta_core::Document;
use folitecta_core::digsig::timestamp::{self, TimeStampRequest};
use folitecta_core::digsig::{Check, DigitalId, IdentityInfo, KeyKind, SignOptions, SignPlacement};

/// 18 October 2026, 12:00:00 UTC.
const NOON: u64 = 1_792_324_800;

fn identity(name: &str) -> IdentityInfo {
    IdentityInfo {
        name: name.into(),
        organization: "Example Time".into(),
        ..IdentityInfo::default()
    }
}

fn authority() -> DigitalId {
    DigitalId::create_time_stamp_authority(&identity("Example Time Authority"), KeyKind::EcP256, 5).unwrap()
}

/// A transport that answers with the local authority, counting the requests.
fn local(tsa: &DigitalId, count: &mut u32) -> impl FnMut(&[u8]) -> folitecta_core::Result<Vec<u8>> {
    move |req| {
        *count += 1;
        timestamp::respond(tsa, req, NOON, u64::from(*count))
    }
}

fn blank() -> Document {
    let mut doc = Document::new_blank(1, 300.0, 300.0).unwrap();
    doc.add_note(0, 30.0, 270.0, "draft", "Ann", [1.0, 0.8, 0.2])
        .unwrap();
    doc
}

fn reopen(bytes: Vec<u8>) -> Document {
    Document::from_bytes(bytes, None, None).unwrap()
}

#[test]
fn tokens_answer_their_request() {
    let tsa = authority();
    assert!(timestamp::allows_time_stamping(&tsa.certificate));
    let request = TimeStampRequest::new(&[b"some ", b"data"]).unwrap();
    let response = timestamp::respond(&tsa, &request.der, NOON, 7).unwrap();
    let token = request.token(&response).unwrap();
    let check = timestamp::check_token(&token, &[b"some data"]).unwrap();
    assert!(check.valid(), "{:?}", check.problems);
    assert!(check.usage_ok);
    assert_eq!(check.time, NOON as i64);
    assert_eq!(check.serial, "07");
    assert_eq!(check.authority.as_ref(), Some(&tsa.certificate));
    // The token is not one of other data.
    let other = timestamp::check_token(&token, &[b"other data"]).unwrap();
    assert!(!other.imprint_ok && !other.valid());
    // A changed token no longer verifies.
    let mut changed = token.clone();
    let at = changed
        .windows(4)
        .position(|w| w == b"2026")
        .expect("time in the token");
    changed[at + 3] = b'5';
    let bad = timestamp::check_token(&changed, &[b"some data"]).unwrap();
    assert!(
        !bad.signature_ok,
        "a changed time must break the signature of the authority"
    );
    // An answer to another request (other nonce) is refused.
    let second = TimeStampRequest::new(&[b"some data"]).unwrap();
    let answer = timestamp::respond(&tsa, &second.der, NOON, 8).unwrap();
    let err = request.token(&answer).unwrap_err().to_string();
    assert!(err.contains("does not belong"), "{err}");
    // A refusal of the authority becomes an error with its text.
    let refused = timestamp::respond(&tsa, b"not a request", NOON, 9).unwrap();
    let err = request.token(&refused).unwrap_err().to_string();
    assert!(err.contains("refused"), "{err}");
}

#[test]
fn authorities_need_the_time_stamping_usage() {
    let plain = DigitalId::create_self_signed(&identity("Not An Authority"), KeyKind::EcP256, 2).unwrap();
    assert!(!timestamp::allows_time_stamping(&plain.certificate));
    let request = TimeStampRequest::new(&[b"x"]).unwrap();
    let token = request
        .token(&timestamp::respond(&plain, &request.der, NOON, 1).unwrap())
        .unwrap();
    let check = timestamp::check_token(&token, &[b"x"]).unwrap();
    assert!(check.valid());
    assert!(!check.usage_ok);
    assert!(
        check
            .problems
            .iter()
            .any(|p| p.contains("not meant for time stamps")),
        "{:?}",
        check.problems
    );
}

#[test]
fn document_time_stamps() {
    let tsa = authority();
    let mut count = 0;
    let stamped = blank().time_stamp(&mut local(&tsa, &mut count)).unwrap();
    assert_eq!(count, 1);
    let doc = reopen(stamped);
    let status = doc.validate_signatures(&[]);
    assert_eq!(status.len(), 1);
    let s = &status[0];
    assert!(s.document_time_stamp);
    assert!(s.valid, "{:?}", s.problems);
    assert!(s.covers_document);
    assert_eq!(s.sub_filter, "ETSI.RFC3161");
    assert_eq!(s.signer, "Example Time Authority");
    let ts = s.time_stamp.as_ref().expect("time stamp");
    assert!(ts.valid && !ts.trusted);
    assert_eq!(ts.time_unix, NOON as i64);
    assert_eq!(s.checks.integrity, Check::Passed);
    assert_eq!(s.checks.time, Check::Warning, "the authority is not trusted yet");
    assert_eq!(s.checks.revocation, Check::NotChecked);
    // Trusting the authority makes the time trusted.
    let trusted = doc.validate_signatures(std::slice::from_ref(&tsa.certificate));
    assert!(trusted[0].time_stamp.as_ref().unwrap().trusted);
    assert_eq!(trusted[0].checks.time, Check::Passed);
    assert_eq!(trusted[0].checks.identity, Check::Passed);
}

#[test]
fn signatures_with_time_stamps() {
    let tsa = authority();
    let signer = DigitalId::create_self_signed(&identity("Signer With Time"), KeyKind::Rsa2048, 2).unwrap();
    let opts = SignOptions {
        placement: SignPlacement::Invisible,
        time: NOON - 60,
        ..SignOptions::default()
    };
    let mut count = 0;
    let signed = blank()
        .sign_with(&signer, &opts, Some(&mut local(&tsa, &mut count)))
        .unwrap();
    assert_eq!(count, 1);
    let doc = reopen(signed.clone());
    let trusted = [signer.certificate.clone(), tsa.certificate.clone()];
    let status = doc.validate_signatures(&trusted);
    let s = &status[0];
    assert!(!s.document_time_stamp);
    assert!(s.valid, "{:?}", s.problems);
    let ts = s.time_stamp.as_ref().expect("time stamp of the signature");
    assert!(ts.valid && ts.trusted, "{:?}", ts.problems);
    assert_eq!(ts.authority, "Example Time Authority");
    assert_eq!(s.checks.integrity, Check::Passed);
    assert_eq!(s.checks.identity, Check::Passed);
    assert_eq!(s.checks.time, Check::Passed);
    // Without a time stamp the time comes from the signer's computer.
    let plain = reopen(blank().sign(&signer, &opts).unwrap());
    let p = &plain.validate_signatures(&trusted)[0];
    assert!(p.valid && p.time_stamp.is_none());
    assert_eq!(p.checks.time, Check::Warning);
    // A document time stamp added later keeps the signature intact.
    let mut count = 0;
    let both = reopen(doc.time_stamp(&mut local(&tsa, &mut count)).unwrap());
    let status = both.validate_signatures(&trusted);
    assert_eq!(status.len(), 2);
    let first = status.iter().find(|s| !s.document_time_stamp).unwrap();
    let stamp = status.iter().find(|s| s.document_time_stamp).unwrap();
    assert!(first.valid, "{:?}", first.problems);
    assert_eq!(first.revision_len, signed.len());
    assert!(!first.covers_document && first.later_revisions_time_stamped);
    assert_eq!(first.checks.integrity, Check::Warning);
    assert!(
        first
            .problems
            .iter()
            .any(|p| p.contains("add document time stamps")),
        "{:?}",
        first.problems
    );
    assert!(stamp.valid && stamp.covers_document, "{:?}", stamp.problems);
    // A later revision with other changes is not one of time stamps only.
    let mut edited = reopen(both.bytes().to_vec());
    edited
        .add_note(0, 60.0, 60.0, "later", "Bob", [1.0, 0.8, 0.2])
        .unwrap();
    let path = std::env::temp_dir().join(format!("folitecta-ts-{}.pdf", std::process::id()));
    edited.save_as(&path).unwrap();
    let later = Document::open(&path).unwrap().validate_signatures(&trusted);
    let _ = std::fs::remove_file(&path);
    assert!(later.iter().all(|s| s.valid && !s.later_revisions_time_stamped));
}

#[test]
fn failing_transports_leave_no_file() {
    let signer = DigitalId::create_self_signed(&identity("Signer"), KeyKind::EcP256, 2).unwrap();
    let mut down = |_: &[u8]| -> folitecta_core::Result<Vec<u8>> {
        Err(folitecta_core::Error::Invalid(
            "the server cannot be reached".into(),
        ))
    };
    let err = blank()
        .sign_with(&signer, &SignOptions::default(), Some(&mut down))
        .unwrap_err()
        .to_string();
    assert!(err.contains("cannot be reached"), "{err}");
    assert!(blank().time_stamp(&mut down).is_err());
}
