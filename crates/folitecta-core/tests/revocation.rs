//! Revocation of signature certificates (OCSP answers, revocation lists)
//! checked offline from the data at hand, fetched through a transport given
//! by the caller, and validation material written to the document security
//! store as an incremental update that keeps the signatures valid.
//!
//! Every certificate has a fixed validity around 2026-10-18T12:00Z and every
//! check runs at a fixed time: nothing depends on the day the tests run.

use folitecta_core::digsig::crl::{self, RevokedEntry};
use folitecta_core::digsig::ocsp::{self, OcspAnswer, OcspTimes};
use folitecta_core::digsig::revocation::{crl_urls, ocsp_urls};
use folitecta_core::digsig::{
    CertProfile, Certificate, Check, DigitalId, ExtendedUsage, FetchProblem, IdentityInfo, KeyKind,
    RevocationArchive, RevocationKind, RevocationNote, RevocationPool, RevocationRequest, RevocationSource,
    RevocationState, SignOptions, SignatureStatus, TimeBasis, cert_to_der, timestamp,
};
use folitecta_core::{Document, Error};

const DAY: i64 = 86_400;
/// 2026-10-18T12:00Z.
const SIGN: i64 = 1_792_324_800;
const NOW: i64 = SIGN + 2 * DAY;

fn who(name: &str) -> IdentityInfo {
    IdentityInfo {
        name: name.into(),
        organization: "Example Revocation".into(),
        ..IdentityInfo::default()
    }
}

fn profile(from: i64, to: i64) -> CertProfile {
    CertProfile {
        not_before: from,
        not_after: to,
        ..CertProfile::default()
    }
}

fn ca() -> DigitalId {
    DigitalId::create_with(
        &who("Example Root CA"),
        KeyKind::EcP256,
        &CertProfile {
            ca: true,
            ..profile(SIGN - 400 * DAY, SIGN + 3650 * DAY)
        },
    )
    .unwrap()
}

fn signer_until(ca: &DigitalId, name: &str, until: i64) -> DigitalId {
    ca.issue(
        &who(name),
        KeyKind::EcP256,
        &CertProfile {
            ocsp_url: Some("http://ocsp.example.org/".into()),
            crl_url: Some("http://crl.example.org/ca.crl".into()),
            ..profile(SIGN - 30 * DAY, until)
        },
    )
    .unwrap()
}

fn signer(ca: &DigitalId) -> DigitalId {
    signer_until(ca, "Example Signer", SIGN + 365 * DAY)
}

/// A self-signed time stamp authority (trusted by the checks that need a
/// proven signing time).
fn tsa() -> DigitalId {
    DigitalId::create_with(
        &who("Example Time Authority"),
        KeyKind::EcP256,
        &CertProfile {
            usages: vec![ExtendedUsage::TimeStamping],
            ..profile(SIGN - 400 * DAY, SIGN + 3650 * DAY)
        },
    )
    .unwrap()
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

fn opts(archive: RevocationArchive) -> SignOptions {
    SignOptions {
        time: SIGN as u64,
        revocation_archive: archive,
        ..SignOptions::default()
    }
}

/// `doc` signed by `s` at SIGN, with a time stamp of `stamp` at SIGN when given.
fn sign_doc(doc: &Document, s: &DigitalId, archive: RevocationArchive, stamp: Option<&DigitalId>) -> Vec<u8> {
    match stamp {
        Some(t) => {
            let mut send = |req: &[u8]| timestamp::respond(t, req, SIGN as u64, 1);
            doc.sign_with(s, &opts(archive), Some(&mut send)).unwrap()
        }
        None => doc.sign(s, &opts(archive)).unwrap(),
    }
}

fn sign(s: &DigitalId, archive: RevocationArchive, stamp: Option<&DigitalId>) -> Vec<u8> {
    sign_doc(&blank(), s, archive, stamp)
}

fn ocsp_for(
    responder: &DigitalId,
    issuer: &DigitalId,
    cert: &Certificate,
    answer: OcspAnswer,
    this: i64,
    next: Option<i64>,
) -> Vec<u8> {
    let req = ocsp::request(cert, &issuer.certificate).unwrap();
    ocsp::respond(
        responder,
        &issuer.certificate,
        &req,
        &|_| answer,
        OcspTimes {
            produced_at: this,
            this_update: this,
            next_update: next,
        },
    )
    .unwrap()
}

fn ocsps(list: Vec<Vec<u8>>) -> RevocationArchive {
    RevocationArchive {
        crls: Vec::new(),
        ocsps: list,
    }
}

fn crls(list: Vec<Vec<u8>>) -> RevocationArchive {
    RevocationArchive {
        crls: list,
        ocsps: Vec::new(),
    }
}

fn pool(data: RevocationArchive) -> RevocationPool {
    RevocationPool {
        data,
        failures: Vec::new(),
    }
}

fn check(doc: &Document, trusted: &[Certificate], pool: &RevocationPool) -> SignatureStatus {
    let s = doc.validate_signatures_with(trusted, pool, NOW);
    assert_eq!(s.len(), 1, "{s:?}");
    s.into_iter().next().unwrap()
}

fn none() -> RevocationPool {
    RevocationPool::default()
}

#[test]
fn issued_ids_carry_their_authority_and_addresses() {
    let ca = ca();
    let s = signer(&ca);
    assert!(folitecta_core::digsig::cms_sig::issued_by(
        &s.certificate,
        &ca.certificate
    ));
    assert_eq!(s.chain, vec![ca.certificate.clone()]);
    let info = s.info();
    assert_eq!(info.not_before_unix, SIGN - 30 * DAY);
    assert_eq!(info.not_after_unix, SIGN + 365 * DAY);
    assert!(!info.self_signed && ca.info().self_signed);
    assert_eq!(ocsp_urls(&s.certificate), ["http://ocsp.example.org/"]);
    assert_eq!(crl_urls(&s.certificate), ["http://crl.example.org/ca.crl"]);
    assert!(timestamp::allows_time_stamping(&tsa().certificate));
    assert!(!timestamp::allows_time_stamping(&s.certificate));
    // An empty validity is refused.
    assert!(DigitalId::create_with(&who("Empty"), KeyKind::EcP256, &profile(SIGN, SIGN)).is_err());
    // Validity after 2049 is written as a GeneralizedTime and read back.
    let late = DigitalId::create_with(&who("Late"), KeyKind::EcP256, &profile(SIGN, 2_600_000_000)).unwrap();
    assert_eq!(late.info().not_after_unix, 2_600_000_000);
}

#[test]
fn ocsp_good_in_the_signature_passes_with_a_proven_time() {
    let ca = ca();
    let s = signer(&ca);
    let t = tsa();
    let good = ocsp_for(
        &ca,
        &ca,
        &s.certificate,
        OcspAnswer::Good,
        SIGN - 60,
        Some(SIGN + 7 * DAY),
    );
    let trusted = [ca.certificate.clone(), t.certificate.clone()];
    let doc = reopen(sign(&s, ocsps(vec![good.clone()]), Some(&t)));
    let st = check(&doc, &trusted, &none());
    assert!(st.valid, "{:?}", st.problems);
    assert_eq!(st.checks.identity, Check::Passed);
    assert_eq!(st.checks.time, Check::Passed);
    assert_eq!(st.checks.revocation, Check::Passed, "{:?}", st.revocation);
    assert_eq!(st.revocation.len(), 1, "the root is not checked");
    let e = &st.revocation[0];
    assert_eq!(e.subject, "Example Signer");
    assert_eq!(e.state, RevocationState::Good);
    assert_eq!(e.source, Some(RevocationSource::Signature));
    assert_eq!(e.kind, Some(RevocationKind::Ocsp));
    assert_eq!(
        (e.this_update, e.next_update),
        (Some(SIGN - 60), Some(SIGN + 7 * DAY))
    );
    assert_eq!(e.authority, "Example Root CA");
    assert_eq!(st.revocation_at, Some(SIGN));
    assert_eq!(st.revocation_basis, TimeBasis::TimeStamp);
    // The same answer without a time stamp: the signer's time is not proven,
    // so an answer issued before now cannot show the certificate was good
    // when the signature was really made.
    let claimed = reopen(sign(&s, ocsps(vec![good]), None));
    let st = check(&claimed, &trusted, &none());
    assert_eq!(st.checks.revocation, Check::Warning);
    assert_eq!(st.revocation_basis, TimeBasis::Claimed);
    assert_eq!(st.revocation[0].state, RevocationState::Unknown);
    assert!(
        st.revocation[0].notes.contains(&RevocationNote::NotAfterProof),
        "{:?}",
        st.revocation
    );
    // A chain that does not end at a trusted certificate proves nothing.
    let st = check(&doc, std::slice::from_ref(&t.certificate), &none());
    assert_eq!(st.checks.identity, Check::Warning);
    assert_eq!(st.checks.revocation, Check::Warning);
    assert!(st.revocation[0].notes.contains(&RevocationNote::Untrusted));
}

#[test]
fn revoked_before_signing_fails() {
    let ca = ca();
    let s = signer(&ca);
    let list = crl::make(
        &ca,
        &[RevokedEntry::of(&s.certificate, SIGN - 3600, Some(1))],
        SIGN - 1800,
        Some(SIGN + 7 * DAY),
        1,
    )
    .unwrap();
    let doc = reopen(sign(&s, crls(vec![list]), None));
    let st = check(&doc, std::slice::from_ref(&ca.certificate), &none());
    assert_eq!(st.checks.integrity, Check::Passed);
    assert_eq!(st.checks.revocation, Check::Failed);
    let e = &st.revocation[0];
    assert_eq!(
        e.state,
        RevocationState::Revoked {
            time: SIGN - 3600,
            reason: Some(1)
        }
    );
    assert_eq!(e.verdict, Check::Failed);
    assert_eq!(
        (e.kind, e.source),
        (Some(RevocationKind::Crl), Some(RevocationSource::Signature))
    );
}

#[test]
fn revocation_after_the_signing_time_depends_on_its_proof() {
    let ca = ca();
    let s = signer(&ca);
    let t = tsa();
    let trusted = [ca.certificate.clone(), t.certificate.clone()];
    let list = crl::make(
        &ca,
        &[RevokedEntry::of(&s.certificate, SIGN + DAY, Some(4))],
        SIGN + DAY + 60,
        Some(SIGN + 8 * DAY),
        2,
    )
    .unwrap();
    let fetched = pool(crls(vec![list]));
    let stamped = reopen(sign(&s, RevocationArchive::default(), Some(&t)));
    let st = check(&stamped, &trusted, &fetched);
    assert_eq!(st.checks.time, Check::Passed);
    assert_eq!(st.checks.revocation, Check::Passed, "{:?}", st.revocation);
    let e = &st.revocation[0];
    assert_eq!(
        e.state,
        RevocationState::Revoked {
            time: SIGN + DAY,
            reason: Some(4)
        }
    );
    assert_eq!(e.verdict, Check::Passed);
    assert_eq!(e.source, Some(RevocationSource::Fetched));
    assert_eq!(
        (st.revocation_basis, st.revocation_at),
        (TimeBasis::TimeStamp, Some(SIGN))
    );
    // Without a time stamp the signature may have been made after the revocation.
    let plain = reopen(sign(&s, RevocationArchive::default(), None));
    let st = check(&plain, &trusted, &fetched);
    assert_eq!(st.checks.revocation, Check::Warning);
    assert_eq!(st.revocation_basis, TimeBasis::Claimed);
}

#[test]
fn missing_unknown_old_and_future_data_cannot_be_determined() {
    let ca = ca();
    let s = signer(&ca);
    let t = tsa();
    let trusted = [ca.certificate.clone(), t.certificate.clone()];
    let doc = reopen(sign(&s, RevocationArchive::default(), Some(&t)));
    let unknown = |p: RevocationPool, note: RevocationNote| {
        let st = check(&doc, &trusted, &p);
        assert_eq!(st.checks.revocation, Check::Warning, "{note:?}");
        let e = &st.revocation[0];
        assert_eq!(e.state, RevocationState::Unknown, "{note:?}");
        assert!(e.notes.contains(&note), "{note:?}: {:?}", e.notes);
    };
    unknown(none(), RevocationNote::NoData);
    unknown(
        pool(ocsps(vec![ocsp_for(
            &ca,
            &ca,
            &s.certificate,
            OcspAnswer::Unknown,
            SIGN,
            None,
        )])),
        RevocationNote::UnknownToResponder,
    );
    unknown(
        pool(ocsps(vec![ocsp_for(
            &ca,
            &ca,
            &s.certificate,
            OcspAnswer::Good,
            SIGN - 10 * DAY,
            Some(SIGN - 3 * DAY),
        )])),
        RevocationNote::IssuedBefore,
    );
    unknown(
        pool(crls(vec![
            crl::make(&ca, &[], SIGN - 10 * DAY, Some(SIGN - 3 * DAY), 1).unwrap(),
        ])),
        RevocationNote::IssuedBefore,
    );
    unknown(
        pool(ocsps(vec![ocsp_for(
            &ca,
            &ca,
            &s.certificate,
            OcspAnswer::Good,
            NOW + DAY,
            None,
        )])),
        RevocationNote::FromFuture,
    );
    // A refused answer cannot be tied to a certificate: no data.
    unknown(
        pool(ocsps(vec![ocsp::refusal(3).unwrap()])),
        RevocationNote::NoData,
    );
}

#[test]
fn responders_must_be_the_issuer_or_authorized_by_it() {
    let ca = ca();
    let s = signer(&ca);
    let t = tsa();
    let trusted = [ca.certificate.clone(), t.certificate.clone()];
    let doc = reopen(sign(&s, RevocationArchive::default(), Some(&t)));
    let with = |responder: &DigitalId, at: i64| {
        let answer = ocsp_for(responder, &ca, &s.certificate, OcspAnswer::Good, at, None);
        check(&doc, &trusted, &pool(ocsps(vec![answer])))
    };
    let delegate = |name: &str, usages: Vec<ExtendedUsage>, nocheck: bool, until: i64| {
        ca.issue(
            &who(name),
            KeyKind::EcP256,
            &CertProfile {
                usages,
                ocsp_no_check: nocheck,
                ..profile(SIGN - DAY, until)
            },
        )
        .unwrap()
    };
    let unrelated = DigitalId::create_with(
        &who("Unrelated Responder"),
        KeyKind::EcP256,
        &profile(SIGN - DAY, SIGN + DAY),
    )
    .unwrap();
    let st = with(&unrelated, SIGN + 60);
    assert!(
        st.revocation[0].notes.contains(&RevocationNote::NotAuthorized),
        "{:?}",
        st.revocation
    );
    let plain = delegate("Plain Certificate", Vec::new(), false, SIGN + 30 * DAY);
    let st = with(&plain, SIGN + 60);
    assert!(
        st.revocation[0].notes.contains(&RevocationNote::NotAuthorized),
        "{:?}",
        st.revocation
    );
    let ok = delegate(
        "Delegated Responder",
        vec![ExtendedUsage::OcspSigning],
        true,
        SIGN + 30 * DAY,
    );
    let st = with(&ok, SIGN + 60);
    assert_eq!(st.checks.revocation, Check::Passed, "{:?}", st.revocation);
    assert_eq!(st.revocation[0].authority, "Delegated Responder");
    // Without the "no check" extension its own revocation would need a check.
    let unchecked = delegate(
        "Unchecked Responder",
        vec![ExtendedUsage::OcspSigning],
        false,
        SIGN + 30 * DAY,
    );
    let st = with(&unchecked, SIGN + 60);
    assert_eq!(st.checks.revocation, Check::Warning);
    assert_eq!(st.revocation[0].state, RevocationState::Good);
    assert!(
        st.revocation[0]
            .notes
            .contains(&RevocationNote::ResponderUnchecked)
    );
    let expired = delegate("Expired Responder", vec![ExtendedUsage::OcspSigning], true, SIGN);
    let st = with(&expired, SIGN + 3600);
    assert!(
        st.revocation[0]
            .notes
            .contains(&RevocationNote::ResponderNotValid),
        "{:?}",
        st.revocation
    );
}

/// The first `pattern` in `data` after `from` changed to `to`.
fn changed(mut data: Vec<u8>, pattern: &[u8], to: &[u8]) -> Vec<u8> {
    let at = data.windows(pattern.len()).position(|w| w == pattern).unwrap();
    data[at..at + to.len()].copy_from_slice(to);
    data
}

#[test]
fn changed_data_is_not_used() {
    // The data are changed before they go into the signature (changing them
    // in a signed file would break the signature itself).
    let ca = ca();
    let s = signer(&ca);
    let t = tsa();
    let trusted = [ca.certificate.clone(), t.certificate.clone()];
    let good = ocsp_for(&ca, &ca, &s.certificate, OcspAnswer::Good, SIGN - 60, None);
    let bad_ocsp = changed(good, b"2026", b"2027");
    let st = check(
        &reopen(sign(&s, ocsps(vec![bad_ocsp]), Some(&t))),
        &trusted,
        &none(),
    );
    assert_eq!(st.checks.revocation, Check::Warning);
    assert!(
        st.revocation[0].notes.contains(&RevocationNote::BadSignature),
        "{:?}",
        st.revocation
    );
    let list = crl::make(&ca, &[], SIGN - 60, None, 1).unwrap();
    let bad_list = changed(list, b"2610", b"2611");
    let st = check(
        &reopen(sign(&s, crls(vec![bad_list]), Some(&t))),
        &trusted,
        &none(),
    );
    assert!(
        st.revocation[0].notes.contains(&RevocationNote::BadSignature),
        "{:?}",
        st.revocation
    );
}

#[test]
fn self_signed_and_directly_trusted_certificates() {
    let own = DigitalId::create_with(
        &who("Self Signer"),
        KeyKind::EcP256,
        &profile(SIGN - DAY, SIGN + DAY),
    )
    .unwrap();
    let st = check(
        &reopen(sign(&own, RevocationArchive::default(), None)),
        &[],
        &none(),
    );
    assert_eq!(st.checks.revocation, Check::NotChecked);
    assert!(st.revocation.is_empty());
    let ca = ca();
    let mut s = signer(&ca);
    // A trusted certificate whose issuer is not at hand: not applicable.
    let direct = [s.certificate.clone()];
    let alone = {
        let mut s = s.clone();
        s.chain.clear();
        reopen(sign(&s, RevocationArchive::default(), None))
    };
    let st = check(&alone, &direct, &none());
    assert_eq!(st.checks.identity, Check::Passed);
    assert_eq!(st.checks.revocation, Check::NotChecked);
    assert!(st.revocation.is_empty());
    // With its issuer in the signature, it is checked.
    s.chain = vec![ca.certificate.clone()];
    let st = check(
        &reopen(sign(&s, RevocationArchive::default(), None)),
        &direct,
        &none(),
    );
    assert_eq!(st.revocation.len(), 1);
    assert_eq!(st.checks.revocation, Check::Warning);
}

#[test]
fn an_issuer_that_is_missing_leaves_revocation_undetermined() {
    let ca = ca();
    let mut s = signer(&ca);
    s.chain.clear();
    let st = check(
        &reopen(sign(&s, RevocationArchive::default(), None)),
        &[],
        &none(),
    );
    assert_eq!(st.checks.revocation, Check::Warning);
    assert_eq!(
        st.revocation[0].notes,
        vec![RevocationNote::IssuerMissing, RevocationNote::Untrusted]
    );
}

#[test]
fn expired_certificates_need_lists_that_keep_them() {
    let ca = ca();
    let s = signer_until(&ca, "Short Lived Signer", SIGN + DAY);
    let t = tsa();
    let trusted = [ca.certificate.clone(), t.certificate.clone()];
    let doc = reopen(sign(&s, RevocationArchive::default(), Some(&t)));
    // A list issued after the certificate expired need not list it.
    let later = crl::make(&ca, &[], NOW - 60, None, 5).unwrap();
    let st = check(&doc, &trusted, &pool(crls(vec![later])));
    assert_eq!(st.revocation[0].state, RevocationState::Unknown);
    assert!(
        st.revocation[0].notes.contains(&RevocationNote::AfterExpiry),
        "{:?}",
        st.revocation
    );
    // Unless the issuer keeps expired certificates on its lists.
    let keeps = x509_cert::ext::Extension {
        extn_id: const_oid::ObjectIdentifier::new_unwrap("2.5.29.60"),
        critical: false,
        extn_value: der::asn1::OctetString::new(
            der::Encode::to_der(
                &der::asn1::GeneralizedTime::from_unix_duration(std::time::Duration::from_secs(
                    (SIGN - 10 * DAY) as u64,
                ))
                .unwrap(),
            )
            .unwrap(),
        )
        .unwrap(),
    };
    let kept = crl::make_with(&ca, &[], NOW - 60, None, 6, vec![keeps]).unwrap();
    let st = check(&doc, &trusted, &pool(crls(vec![kept])));
    assert_eq!(
        st.revocation[0].state,
        RevocationState::Good,
        "{:?}",
        st.revocation
    );
    assert_eq!(st.checks.revocation, Check::Passed);
}

#[test]
fn certificates_on_hold() {
    let ca = ca();
    let s = signer(&ca);
    let t = tsa();
    let trusted = [ca.certificate.clone(), t.certificate.clone()];
    let doc = reopen(sign(&s, RevocationArchive::default(), Some(&t)));
    let hold = |this: i64| {
        crl::make(
            &ca,
            &[RevokedEntry::of(&s.certificate, SIGN - DAY, Some(6))],
            this,
            None,
            3,
        )
        .unwrap()
    };
    // Still on hold after the signing time: failed.
    let st = check(&doc, &trusted, &pool(crls(vec![hold(SIGN + 3600)])));
    assert_eq!(st.checks.revocation, Check::Failed);
    // On hold before, and good only long after: when the hold ended is not known.
    let later_good = ocsp_for(&ca, &ca, &s.certificate, OcspAnswer::Good, SIGN + DAY, None);
    let st = check(
        &doc,
        &trusted,
        &pool(RevocationArchive {
            crls: vec![hold(SIGN - DAY + 60)],
            ocsps: vec![later_good],
        }),
    );
    assert_eq!(st.checks.revocation, Check::Warning);
    assert!(
        st.revocation[0].notes.contains(&RevocationNote::HoldUnknown),
        "{:?}",
        st.revocation
    );
    // Released by the signing time: good.
    let released = ocsp_for(&ca, &ca, &s.certificate, OcspAnswer::Good, SIGN - 60, None);
    let st = check(
        &doc,
        &trusted,
        &pool(RevocationArchive {
            crls: vec![hold(SIGN - DAY + 60)],
            ocsps: vec![released],
        }),
    );
    assert_eq!(st.checks.revocation, Check::Passed, "{:?}", st.revocation);
}

fn temp(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("folitecta-rev-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(name)
}

fn sha1_hex(data: &[u8]) -> String {
    use sha1::Digest;
    sha1::Sha1::digest(data)
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect()
}

/// The decoded `/Contents` of the first signature written in `raw` (the
/// whole hexadecimal string, padding included).
fn contents_of(raw: &[u8]) -> Vec<u8> {
    let mut from = 0;
    let open = loop {
        let at = from + raw[from..].windows(9).position(|w| w == b"/Contents").unwrap() + 9;
        let next = at + raw[at..].iter().position(|c| !c.is_ascii_whitespace()).unwrap();
        if raw[next] == b'<' && raw[next + 1].is_ascii_hexdigit() {
            break next;
        }
        from = at;
    };
    let close = open + raw[open..].iter().position(|c| *c == b'>').unwrap();
    let hex = std::str::from_utf8(&raw[open + 1..close]).unwrap();
    (0..hex.len() / 2)
        .map(|i| u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).unwrap())
        .collect()
}

#[test]
fn validation_material_round_trip_keeps_the_signature_valid() {
    let ca = ca();
    let mut s = signer(&ca);
    // The authority comes from the trusted list, not from the signature.
    s.chain.clear();
    let t = tsa();
    let trusted = [ca.certificate.clone(), t.certificate.clone()];
    let signed = sign(&s, RevocationArchive::default(), Some(&t));
    let mut doc = reopen(signed.clone());
    let before = check(&doc, &trusted, &none());
    assert_eq!(before.checks.revocation, Check::Warning);
    assert_eq!(before.revocation[0].notes, vec![RevocationNote::NoData]);
    let answer = ocsp_for(
        &ca,
        &ca,
        &s.certificate,
        OcspAnswer::Good,
        SIGN + 3600,
        Some(SIGN + 7 * DAY),
    );
    let fetched = pool(ocsps(vec![answer.clone()]));
    let report = doc.add_validation_material(&trusted, &fetched, NOW).unwrap();
    assert!(report.changed);
    assert_eq!((report.signatures, report.skipped), (1, 0));
    assert_eq!(
        (
            report.certs_added,
            report.ocsps_added,
            report.crls_added,
            report.vri_written
        ),
        (1, 1, 0, 1)
    );
    assert!(report.missing.is_empty());
    assert!(doc.is_dirty());
    assert_eq!(doc.dss().ocsps, vec![answer.clone()]);
    doc.undo().unwrap();
    assert!(doc.dss().is_empty());
    doc.redo().unwrap();
    assert_eq!(doc.dss().ocsps.len(), 1);
    let path = temp("material.pdf");
    doc.save_as(&path).unwrap();
    let saved = std::fs::read(&path).unwrap();
    assert!(saved.starts_with(&signed), "the signed bytes stay a prefix");
    // The update holds the catalog, the document information and the store
    // only (undo and redo rewrote nothing else).
    let raw = lopdf::Document::load_mem(&saved).unwrap();
    let root = raw.trailer.get(b"Root").unwrap().as_reference().unwrap();
    let info = raw.trailer.get(b"Info").unwrap().as_reference().unwrap();
    let cat = raw.get_dictionary(root).unwrap();
    let dss_id = cat.get(b"DSS").unwrap().as_reference().unwrap();
    let dss = raw.get_dictionary(dss_id).unwrap();
    let certs = dss.get(b"Certs").unwrap().as_array().unwrap();
    let answers = dss.get(b"OCSPs").unwrap().as_array().unwrap();
    assert!(dss.get(b"CRLs").is_err());
    assert_eq!((certs.len(), answers.len()), (1, 1));
    let content = |o: &lopdf::Object| {
        raw.get_object(o.as_reference().unwrap())
            .unwrap()
            .as_stream()
            .unwrap()
            .get_plain_content()
            .unwrap()
    };
    assert_eq!(content(&answers[0]), answer);
    assert_eq!(content(&certs[0]), cert_to_der(&ca.certificate));
    let vri_id = dss.get(b"VRI").unwrap().as_reference().unwrap();
    let vri = raw.get_dictionary(vri_id).unwrap();
    assert_eq!(vri.len(), 1);
    let (key, entry) = vri.iter().next().unwrap();
    assert_eq!(String::from_utf8_lossy(key), sha1_hex(&contents_of(&signed)));
    let entry_id = entry.as_reference().unwrap();
    let entry = raw.get_dictionary(entry_id).unwrap();
    assert_eq!(
        entry.get(b"OCSP").unwrap().as_array().unwrap(),
        &vec![answers[0].clone()]
    );
    assert_eq!(
        entry.get(b"Cert").unwrap().as_array().unwrap(),
        &vec![certs[0].clone()]
    );
    assert_eq!(entry.get(b"TU").unwrap().as_str().unwrap(), b"D:20261020120000Z");
    let esic = cat
        .get(b"Extensions")
        .unwrap()
        .as_dict()
        .unwrap()
        .get(b"ESIC")
        .unwrap()
        .as_dict()
        .unwrap();
    assert_eq!(esic.get(b"ExtensionLevel").unwrap().as_i64().unwrap(), 1);
    let allowed = [
        root,
        info,
        dss_id,
        vri_id,
        entry_id,
        certs[0].as_reference().unwrap(),
        answers[0].as_reference().unwrap(),
    ];
    let update = &saved[signed.len()..];
    for id in raw.objects.keys() {
        let header = format!("\n{} {} obj", id.0, id.1);
        let written = update.windows(header.len()).any(|w| w == header.as_bytes());
        let xref = raw.get_object(*id).ok().and_then(|o| o.type_name().ok()) == Some(b"XRef".as_slice());
        assert!(
            !written || allowed.contains(id) || xref,
            "object {id:?} was rewritten"
        );
    }
    // Reopened, the signature passes from the store, with no network.
    let back = Document::open(&path).unwrap();
    let st = check(&back, &trusted, &none());
    assert!(st.valid && !st.covers_document, "{:?}", st.problems);
    assert!(st.later_revisions_material && !st.later_revisions_time_stamped);
    assert_eq!(st.checks.integrity, Check::Passed);
    assert_eq!(st.checks.revocation, Check::Passed, "{:?}", st.revocation);
    assert_eq!(st.revocation[0].source, Some(RevocationSource::Dss));
    assert!(st.problems.is_empty(), "{:?}", st.problems);
    // Nothing new to add a second time.
    let mut again = Document::open(&path).unwrap();
    let report = again.add_validation_material(&trusted, &none(), NOW).unwrap();
    assert!(!report.changed && !again.is_dirty());
}

#[test]
fn material_is_allowed_after_certification() {
    let ca = ca();
    let s = signer(&ca);
    let t = tsa();
    let trusted = [ca.certificate.clone(), t.certificate.clone()];
    let good = ocsp_for(&ca, &ca, &s.certificate, OcspAnswer::Good, SIGN - 60, None);
    let options = SignOptions {
        certify: 1,
        ..opts(ocsps(vec![good]))
    };
    let mut send = |req: &[u8]| timestamp::respond(&t, req, SIGN as u64, 1);
    let signed = blank().sign_with(&s, &options, Some(&mut send)).unwrap();
    let mut doc = reopen(signed);
    let report = doc.add_validation_material(&trusted, &none(), NOW).unwrap();
    assert!(report.changed);
    let path = temp("certified.pdf");
    doc.save_as(&path).unwrap();
    let st = check(&Document::open(&path).unwrap(), &trusted, &none());
    assert_eq!(st.certify, Some(1));
    assert!(st.later_revisions_material);
    assert_eq!(st.checks.integrity, Check::Passed);
    assert!(
        !st.problems.iter().any(|p| p.contains("certification")),
        "{:?}",
        st.problems
    );
}

fn material_file(name: &str) -> (std::path::PathBuf, Vec<Certificate>, DigitalId) {
    let ca = ca();
    let s = signer(&ca);
    let t = tsa();
    let trusted = vec![ca.certificate.clone(), t.certificate.clone()];
    let mut doc = reopen(sign(&s, RevocationArchive::default(), Some(&t)));
    let answer = ocsp_for(&ca, &ca, &s.certificate, OcspAnswer::Good, SIGN + 60, None);
    doc.add_validation_material(&trusted, &pool(ocsps(vec![answer])), NOW)
        .unwrap();
    let path = temp(name);
    doc.save_as(&path).unwrap();
    (path, trusted, t)
}

#[test]
fn material_then_time_stamp_or_other_edits() {
    let (path, trusted, t) = material_file("then.pdf");
    let doc = Document::open(&path).unwrap();
    let mut send = |req: &[u8]| timestamp::respond(&t, req, (SIGN + 3600) as u64, 9);
    let stamped = reopen(doc.time_stamp(&mut send).unwrap());
    let all = stamped.validate_signatures_with(&trusted, &none(), NOW);
    let sig = all.iter().find(|s| !s.document_time_stamp).unwrap();
    let ts = all.iter().find(|s| s.document_time_stamp).unwrap();
    assert!(sig.later_revisions_time_stamped && !sig.later_revisions_material);
    assert_eq!(sig.checks.integrity, Check::Warning);
    assert!(
        sig.problems
            .iter()
            .any(|p| p.contains("add document time stamps")),
        "{:?}",
        sig.problems
    );
    assert!(ts.valid && ts.covers_document, "{:?}", ts.problems);
    // Another edit after the material is a change.
    let mut edited = Document::open(&path).unwrap();
    edited
        .add_note(0, 60.0, 60.0, "later", "Bob", [1.0, 0.8, 0.2])
        .unwrap();
    let path2 = temp("then-edited.pdf");
    edited.save_as(&path2).unwrap();
    let st = check(&Document::open(&path2).unwrap(), &trusted, &none());
    assert!(!st.later_revisions_material);
    assert_eq!(st.checks.integrity, Check::Warning);
    assert!(
        st.problems.iter().any(|p| p.ends_with("(in a later revision)")),
        "{:?}",
        st.problems
    );
    // Saving without changes adds the date of the save only: still unchanged.
    let mut resaved = Document::open(&path).unwrap();
    let path3 = temp("then-resaved.pdf");
    resaved.save_as(&path3).unwrap();
    let st = check(&Document::open(&path3).unwrap(), &trusted, &none());
    assert!(st.later_revisions_material, "{:?}", st.problems);
    assert_eq!(st.checks.integrity, Check::Passed);
}

#[test]
fn signatures_of_two_signers_get_their_own_entries() {
    let ca = ca();
    let first = signer(&ca);
    let second = signer_until(&ca, "Second Signer", SIGN + 365 * DAY);
    let t = tsa();
    let trusted = [ca.certificate.clone(), t.certificate.clone()];
    let once = reopen(sign(&first, RevocationArchive::default(), Some(&t)));
    let twice = reopen(sign_doc(&once, &second, RevocationArchive::default(), Some(&t)));
    let answers = ocsps(vec![
        ocsp_for(&ca, &ca, &first.certificate, OcspAnswer::Good, SIGN + 60, None),
        ocsp_for(&ca, &ca, &second.certificate, OcspAnswer::Good, SIGN + 60, None),
    ]);
    let mut doc = twice;
    let report = doc
        .add_validation_material(&trusted, &pool(answers), NOW)
        .unwrap();
    assert_eq!(
        (report.signatures, report.vri_written, report.ocsps_added),
        (2, 2, 2)
    );
    let path = temp("two.pdf");
    doc.save_as(&path).unwrap();
    let back = Document::open(&path).unwrap();
    assert_eq!(back.dss().vri.len(), 2);
    let all = back.validate_signatures_with(&trusted, &none(), NOW);
    assert_eq!(all.len(), 2);
    for s in &all {
        assert_eq!(
            s.checks.revocation,
            Check::Passed,
            "{}: {:?}",
            s.field,
            s.revocation
        );
    }
    // The second signature is a later change for the first (not analysed);
    // the material after both changes nothing for the second.
    let last = all
        .iter()
        .find(|s| s.revision_len > all.iter().map(|x| x.revision_len).min().unwrap())
        .unwrap();
    assert!(
        last.later_revisions_material && last.checks.integrity == Check::Passed,
        "{:?}",
        last.problems
    );
}

#[test]
fn untrusted_chains_get_no_material() {
    let ca = ca();
    let s = signer(&ca);
    let answer = ocsp_for(&ca, &ca, &s.certificate, OcspAnswer::Good, SIGN + 60, None);
    let mut doc = reopen(sign(&s, RevocationArchive::default(), None));
    let report = doc
        .add_validation_material(&[], &pool(ocsps(vec![answer])), NOW)
        .unwrap();
    assert!(!report.changed && !doc.is_dirty());
    assert_eq!((report.signatures, report.skipped), (0, 1));
}

#[test]
fn documents_without_a_signed_file_get_no_material() {
    // Not signed.
    let mut plain = blank();
    let err = plain.add_validation_material(&[], &none(), NOW).unwrap_err();
    assert!(err.to_string().contains("no signatures"), "{err}");
    // Signed, cleared, and the clearing undone: the signed file is still the
    // base of the update, so the signatures stay valid.
    let ca = ca();
    let s = signer(&ca);
    let t = tsa();
    let trusted = [ca.certificate.clone(), t.certificate.clone()];
    let mut doc = reopen(sign(&s, RevocationArchive::default(), Some(&t)));
    let answer = ocsp_for(&ca, &ca, &s.certificate, OcspAnswer::Good, SIGN + 60, None);
    doc.add_validation_material(&trusted, &pool(ocsps(vec![answer.clone()])), NOW)
        .unwrap();
    assert_eq!(doc.clear_signatures().unwrap(), 1);
    assert!(
        doc.dss().is_empty(),
        "clearing the signatures removes their material"
    );
    let path = temp("cleared.pdf");
    doc.save_copy(&path).unwrap();
    let cleared = std::fs::read(&path).unwrap();
    assert!(
        !cleared.windows(15).any(|w| w == b"Example Root CA"),
        "no certificate stays in the file"
    );
    doc.undo().unwrap();
    doc.undo().unwrap();
    assert!(doc.has_signatures() && doc.dss().is_empty());
    doc.add_validation_material(&trusted, &pool(ocsps(vec![answer])), NOW)
        .unwrap();
    let path = temp("uncleared.pdf");
    doc.save_as(&path).unwrap();
    let st = check(&Document::open(&path).unwrap(), &trusted, &none());
    assert!(st.valid && st.later_revisions_material, "{:?}", st.problems);
    // A plan made for another state is refused.
    let mut moved = Document::open(&path).unwrap();
    let plan = moved
        .validation_job()
        .plan_material(&trusted, &none(), NOW)
        .unwrap();
    moved.add_note(0, 10.0, 10.0, "x", "Y", [1.0, 0.0, 0.0]).unwrap();
    assert!(matches!(
        moved.apply_validation_material(&plan, NOW),
        Err(Error::Invalid(_))
    ));
}

#[test]
fn protected_documents_get_material_through_an_encrypted_update() {
    let ca = ca();
    let s = signer(&ca);
    let t = tsa();
    let trusted = [ca.certificate.clone(), t.certificate.clone()];
    let plain = temp("protect-plain.pdf");
    blank()
        .save_encrypted(&plain, "u", "o", Default::default(), Default::default())
        .unwrap();
    let protected = Document::open_with_password(&plain, "u").unwrap();
    let mut send = |req: &[u8]| timestamp::respond(&t, req, SIGN as u64, 1);
    let signed = protected
        .sign_to_file(&s, &opts(RevocationArchive::default()), Some(&mut send))
        .unwrap()
        .bytes;
    let mut doc = Document::from_bytes(signed.clone(), None, Some("u")).unwrap();
    assert!(doc.is_protected());
    let answer = ocsp_for(&ca, &ca, &s.certificate, OcspAnswer::Good, SIGN + 60, None);
    let report = doc
        .add_validation_material(&trusted, &pool(ocsps(vec![answer.clone()])), NOW)
        .unwrap();
    assert!(report.changed);
    let path = temp("protected-material.pdf");
    doc.save_as(&path).unwrap();
    let saved = std::fs::read(&path).unwrap();
    assert!(saved.starts_with(&signed));
    // The store is encrypted with the document: its answer is not in the file as it is.
    assert!(!saved.windows(answer.len()).any(|w| w == answer.as_slice()));
    assert!(matches!(Document::open(&path), Err(Error::PasswordRequired)));
    let back = Document::open_with_password(&path, "u").unwrap();
    assert_eq!(back.dss().ocsps, vec![answer]);
    let st = check(&back, &trusted, &none());
    assert!(st.valid, "{:?}", st.problems);
    assert!(st.later_revisions_material, "{:?}", st.problems);
    assert_eq!(st.checks.integrity, Check::Passed);
    assert_eq!(st.checks.revocation, Check::Passed, "{:?}", st.revocation);
}

#[test]
fn online_requests_ask_ocsp_then_the_list_and_never_guess() {
    let ca = ca();
    let s = signer(&ca);
    let t = tsa();
    let trusted = [ca.certificate.clone(), t.certificate.clone()];
    let doc = reopen(sign(&s, RevocationArchive::default(), Some(&t)));
    let requests = doc.revocation_requests(&trusted, &none(), NOW);
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].url, "http://ocsp.example.org/");
    assert_eq!(
        requests[0].ocsp,
        Some(ocsp::request(&s.certificate, &ca.certificate).unwrap())
    );
    assert_eq!(requests[0].subject, "Example Signer");
    assert_eq!(
        (requests[1].url.as_str(), requests[1].ocsp.is_none()),
        ("http://crl.example.org/ca.crl", true)
    );
    // A chain that does not end at a trusted certificate: nothing is sent.
    assert!(
        doc.revocation_requests(std::slice::from_ref(&t.certificate), &none(), NOW)
            .is_empty()
    );
    let mut count = 0;
    let mut never = |_: &RevocationRequest| -> folitecta_core::Result<Vec<u8>> {
        count += 1;
        Ok(Vec::new())
    };
    let mut p = none();
    doc.fetch_revocation(std::slice::from_ref(&t.certificate), &mut p, NOW, &mut never);
    assert_eq!(count, 0);
    // The responder refuses; the list answers.
    let list = crl::make(&ca, &[], NOW - 60, Some(NOW + 7 * DAY), 9).unwrap();
    let mut sent = Vec::new();
    let mut send = |r: &RevocationRequest| -> folitecta_core::Result<Vec<u8>> {
        sent.push(r.url.clone());
        Ok(if r.ocsp.is_some() {
            ocsp::refusal(6).unwrap()
        } else {
            list.clone()
        })
    };
    let mut p = none();
    let summary = doc.fetch_revocation(&trusted, &mut p, NOW, &mut send);
    assert_eq!(
        (
            summary.certificates,
            summary.decided,
            summary.requests,
            summary.failures
        ),
        (1, 1, 2, 1)
    );
    assert_eq!(
        sent,
        ["http://ocsp.example.org/", "http://crl.example.org/ca.crl"]
    );
    assert_eq!((p.data.crls.len(), p.data.ocsps.len()), (1, 0));
    assert!(p.failures.is_empty());
    let st = check(&doc, &trusted, &p);
    assert_eq!(st.checks.revocation, Check::Passed, "{:?}", st.revocation);
    assert_eq!(
        (st.revocation[0].source, st.revocation[0].kind),
        (Some(RevocationSource::Fetched), Some(RevocationKind::Crl))
    );
    // Garbage and an unreachable server: nothing is guessed.
    let mut broken = |r: &RevocationRequest| -> folitecta_core::Result<Vec<u8>> {
        if r.ocsp.is_some() {
            Ok(b"garbage".to_vec())
        } else {
            Err(Error::Invalid("connection refused".into()))
        }
    };
    let mut p = none();
    let summary = doc.fetch_revocation(&trusted, &mut p, NOW, &mut broken);
    assert_eq!(
        (
            summary.certificates,
            summary.decided,
            summary.requests,
            summary.failures
        ),
        (1, 0, 2, 2)
    );
    assert!(p.data.is_empty());
    assert_eq!(
        p.failures[0].problem,
        FetchProblem::Answer(Box::new(RevocationNote::Unreadable))
    );
    assert!(matches!(&p.failures[1].problem, FetchProblem::Transport(e) if e.contains("connection refused")));
    let st = check(&doc, &trusted, &p);
    assert_eq!(st.checks.revocation, Check::Warning);
    let fetch_notes = st.revocation[0]
        .notes
        .iter()
        .filter(|n| matches!(n, RevocationNote::FetchFailed { .. }))
        .count();
    assert_eq!(fetch_notes, 2);
    // An answer about another certificate.
    let other = signer_until(&ca, "Other Signer", SIGN + DAY);
    let wrong = ocsp_for(&ca, &ca, &other.certificate, OcspAnswer::Good, NOW, None);
    let mut answers = |r: &RevocationRequest| -> folitecta_core::Result<Vec<u8>> {
        if r.ocsp.is_some() {
            Ok(wrong.clone())
        } else {
            Err(Error::Invalid("down".into()))
        }
    };
    let mut p = none();
    doc.fetch_revocation(&trusted, &mut p, NOW, &mut answers);
    assert_eq!(p.failures[0].problem, FetchProblem::AnotherCertificate);
}

#[test]
fn document_time_stamps_check_their_authority() {
    let ca = ca();
    let authority = ca
        .issue(
            &who("Example Issued Authority"),
            KeyKind::EcP256,
            &CertProfile {
                usages: vec![ExtendedUsage::TimeStamping],
                crl_url: Some("http://crl.example.org/ca.crl".into()),
                ..profile(SIGN - 30 * DAY, SIGN + 365 * DAY)
            },
        )
        .unwrap();
    let in_token = crls(vec![
        crl::make(&ca, &[], SIGN - 60, Some(SIGN + 7 * DAY), 3).unwrap(),
    ]);
    let mut first = |req: &[u8]| timestamp::respond_with(&authority, req, SIGN as u64, 1, &in_token);
    let stamped = reopen(blank().time_stamp(&mut first).unwrap());
    let trusted = [ca.certificate.clone()];
    let st = check(&stamped, &trusted, &none());
    assert!(st.document_time_stamp && st.valid, "{:?}", st.problems);
    assert_eq!(
        (st.revocation_basis, st.revocation_at),
        (TimeBasis::Claimed, Some(SIGN))
    );
    // The time of the stamp is only its authority's word: a list issued
    // before now does not show the authority was good when it stamped.
    assert_eq!(st.checks.revocation, Check::Warning);
    // A later document time stamp of a trusted authority proves the first
    // existed by then: the list in the token is enough.
    let t = tsa();
    let mut second = |req: &[u8]| timestamp::respond(&t, req, (SIGN + 60) as u64, 2);
    let both = reopen(stamped.time_stamp(&mut second).unwrap());
    let trusted = [ca.certificate.clone(), t.certificate.clone()];
    let all = both.validate_signatures_with(&trusted, &none(), NOW);
    let st = all
        .iter()
        .find(|s| s.signer == "Example Issued Authority")
        .unwrap();
    assert_eq!(st.checks.revocation, Check::Passed, "{:?}", st.revocation);
    assert_eq!(
        (st.revocation[0].source, st.revocation[0].kind),
        (Some(RevocationSource::TimeStamp), Some(RevocationKind::Crl))
    );
    // A revocation of the authority after the proof passes; without the
    // proof it cannot be determined.
    let revoked = crls(vec![
        crl::make(
            &ca,
            &[RevokedEntry::of(&authority.certificate, SIGN + DAY, Some(4))],
            SIGN + DAY + 60,
            None,
            4,
        )
        .unwrap(),
    ]);
    let all = both.validate_signatures_with(&trusted, &pool(revoked.clone()), NOW);
    let st = all
        .iter()
        .find(|s| s.signer == "Example Issued Authority")
        .unwrap();
    assert_eq!(st.checks.revocation, Check::Passed, "{:?}", st.revocation);
    let st = check(&stamped, &trusted, &pool(revoked));
    assert_eq!(st.checks.revocation, Check::Warning);
}

#[test]
fn material_with_other_unsaved_edits_is_a_change() {
    let ca = ca();
    let s = signer(&ca);
    let t = tsa();
    let trusted = [ca.certificate.clone(), t.certificate.clone()];
    let mut doc = reopen(sign(&s, RevocationArchive::default(), Some(&t)));
    doc.add_note(0, 60.0, 60.0, "unsaved", "Bob", [1.0, 0.8, 0.2])
        .unwrap();
    let answer = ocsp_for(&ca, &ca, &s.certificate, OcspAnswer::Good, SIGN + 60, None);
    doc.add_validation_material(&trusted, &pool(ocsps(vec![answer])), NOW)
        .unwrap();
    let path = temp("unsaved-edits.pdf");
    doc.save_as(&path).unwrap();
    let st = check(&Document::open(&path).unwrap(), &trusted, &none());
    assert!(!st.later_revisions_material);
    assert_eq!(st.checks.integrity, Check::Warning);
    // The material itself is read: the revocation passes from the store.
    assert_eq!(st.checks.revocation, Check::Passed);
}

#[test]
fn rsa_authorities_sign_answers_and_lists() {
    let ca = DigitalId::create_with(
        &who("Example RSA Root"),
        KeyKind::Rsa2048,
        &CertProfile {
            ca: true,
            ..profile(SIGN - 400 * DAY, SIGN + 3650 * DAY)
        },
    )
    .unwrap();
    let s = ca
        .issue(
            &who("Example RSA Signer"),
            KeyKind::Rsa2048,
            &profile(SIGN - DAY, SIGN + 30 * DAY),
        )
        .unwrap();
    let t = tsa();
    let trusted = [ca.certificate.clone(), t.certificate.clone()];
    let doc = reopen(sign(&s, RevocationArchive::default(), Some(&t)));
    let answer = ocsp_for(&ca, &ca, &s.certificate, OcspAnswer::Good, SIGN + 60, None);
    let st = check(&doc, &trusted, &pool(ocsps(vec![answer])));
    assert_eq!(st.checks.revocation, Check::Passed, "{:?}", st.revocation);
    let list = crl::make(
        &ca,
        &[RevokedEntry::of(&s.certificate, SIGN - 60, Some(1))],
        SIGN,
        None,
        1,
    )
    .unwrap();
    let st = check(&doc, &trusted, &pool(crls(vec![list])));
    assert_eq!(st.checks.revocation, Check::Failed, "{:?}", st.revocation);
}

#[test]
fn the_store_extension_follows_the_version() {
    for (version, esic, raised) in [("1.4", true, true), ("1.7", true, false), ("2.0", false, false)] {
        let ca = ca();
        let s = signer(&ca);
        let t = tsa();
        let trusted = [ca.certificate.clone(), t.certificate.clone()];
        let mut base = blank();
        base.inner_mut().version = version.into();
        base.commit().unwrap();
        let mut doc = reopen(sign_doc(&base, &s, RevocationArchive::default(), Some(&t)));
        let answer = ocsp_for(&ca, &ca, &s.certificate, OcspAnswer::Good, SIGN + 60, None);
        doc.add_validation_material(&trusted, &pool(ocsps(vec![answer])), NOW)
            .unwrap();
        let path = temp(&format!("version-{version}.pdf"));
        doc.save_as(&path).unwrap();
        let raw = lopdf::Document::load(&path).unwrap();
        let cat = raw.catalog().unwrap();
        let has_esic = cat
            .get(b"Extensions")
            .ok()
            .and_then(|e| e.as_dict().ok())
            .is_some_and(|e| e.has(b"ESIC"));
        assert_eq!(has_esic, esic, "{version}");
        let catalog_version = cat
            .get(b"Version")
            .ok()
            .and_then(|v| v.as_name().ok())
            .map(|v| v.to_vec());
        assert_eq!(catalog_version.is_some(), raised, "{version}");
        let st = check(&Document::open(&path).unwrap(), &trusted, &none());
        assert!(
            st.later_revisions_material && st.checks.integrity == Check::Passed,
            "{version}: {:?}",
            st.problems
        );
    }
}

#[test]
fn reading_the_store_is_bounded() {
    let ca = ca();
    let s = signer(&ca);
    let t = tsa();
    let trusted = [ca.certificate.clone(), t.certificate.clone()];
    let signed = sign(&s, RevocationArchive::default(), Some(&t));
    // Another producer's store: 1100 small items and a stream that inflates
    // to 80 MiB.
    let prev = lopdf::Document::load_mem(&signed).unwrap();
    let mut inc = lopdf::IncrementalDocument::create_from(signed.clone(), prev.clone());
    let mut n = prev.max_id;
    let mut items = Vec::new();
    for i in 0..1100u32 {
        n += 1;
        let s = lopdf::Stream::new(lopdf::Dictionary::new(), format!("item {i}").into_bytes());
        inc.new_document.objects.insert((n, 0), lopdf::Object::Stream(s));
        items.push(lopdf::Object::Reference((n, 0)));
    }
    let mut bomb = lopdf::Stream::new(lopdf::Dictionary::new(), vec![0u8; 80 * 1024 * 1024]);
    bomb.compress().unwrap();
    n += 1;
    inc.new_document
        .objects
        .insert((n, 0), lopdf::Object::Stream(bomb));
    let bomb_ref = lopdf::Object::Reference((n, 0));
    n += 1;
    let dss = (n, 0);
    inc.new_document.objects.insert(
        dss,
        lopdf::Object::Dictionary(lopdf::dictionary! { "Certs" => items, "CRLs" => vec![bomb_ref] }),
    );
    let root = prev.trailer.get(b"Root").unwrap().as_reference().unwrap();
    let mut cat = prev.get_dictionary(root).unwrap().clone();
    cat.set("DSS", lopdf::Object::Reference(dss));
    inc.new_document
        .objects
        .insert(root, lopdf::Object::Dictionary(cat));
    inc.new_document.max_id = n;
    let mut out = Vec::new();
    inc.save_to(&mut out).unwrap();
    let started = std::time::Instant::now();
    let doc = reopen(out);
    let read = doc.dss();
    assert_eq!(read.certs.len(), 1000);
    assert_eq!(read.crls.len(), 0);
    assert_eq!(read.skipped, 101);
    let st = check(&doc, &trusted, &none());
    assert!(st.valid && st.later_revisions_material, "{:?}", st.problems);
    assert!(
        started.elapsed() < std::time::Duration::from_secs(20),
        "{:?}",
        started.elapsed()
    );
}

#[test]
fn material_follows_cross_reference_tables_too() {
    let ca = ca();
    let s = signer(&ca);
    let t = tsa();
    let trusted = [ca.certificate.clone(), t.certificate.clone()];
    let mut base = blank();
    base.inner_mut().reference_table.cross_reference_type = lopdf::xref::XrefType::CrossReferenceTable;
    base.commit().unwrap();
    let signed = sign_doc(&base, &s, RevocationArchive::default(), Some(&t));
    assert!(
        signed.windows(5).any(|w| w == b"xref\n"),
        "a cross-reference table"
    );
    let mut doc = reopen(signed.clone());
    let answer = ocsp_for(&ca, &ca, &s.certificate, OcspAnswer::Good, SIGN + 60, None);
    doc.add_validation_material(&trusted, &pool(ocsps(vec![answer])), NOW)
        .unwrap();
    let path = temp("table.pdf");
    doc.save_as(&path).unwrap();
    let saved = std::fs::read(&path).unwrap();
    assert!(saved.starts_with(&signed));
    let st = check(&Document::open(&path).unwrap(), &trusted, &none());
    assert!(st.later_revisions_material, "{:?}", st.problems);
    assert_eq!(st.checks.revocation, Check::Passed);
}
