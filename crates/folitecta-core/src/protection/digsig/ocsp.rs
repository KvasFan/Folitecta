//! OCSP (RFC 6960): the request for the state of one certificate, the checks
//! of an answer for a certificate, and a small responder for the tests and
//! the self-test.
//!
//! Answers are read with the DER walker from the bytes as they were received:
//! the signature of the responder covers those bytes, and times with
//! fractions of a second are accepted.

use const_oid::ObjectIdentifier;
use der::{Decode, Encode};
use spki::AlgorithmIdentifierOwned;
use x509_cert::Certificate;
use x509_cert::ext::Extensions;
use x509_cert::name::Name;

use super::cms_sig::{digest_of_sig_alg, digest_with, issued_by, verify_with_cert};
use super::der_walk::{element, elements, expect, generalized, generalized_time, tlv};
use super::id::{
    DigitalId, KP_OCSP_SIGNING, OCSP_NO_CHECK, SHA1_OID, alg_null, cert_info, extension_value,
    has_extended_usage, key_hash,
};
use super::revocation::{CLOCK_SKEW, Finding, RevocationNote};
use crate::error::{Error, Result};

/// Type of the basic OCSP answer (id-pkix-ocsp-basic).
const OCSP_BASIC: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.3.6.1.5.5.7.48.1.1");
/// Single extension of an answer: the oldest revocation the responder still
/// keeps (RFC 6960 4.4.4).
const ARCHIVE_CUTOFF: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.3.6.1.5.5.7.48.1.6");

/// What a local responder answers for a serial number.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OcspAnswer {
    Good,
    Revoked { time: i64, reason: Option<u8> },
    Unknown,
}

/// The times of an answer of a local responder (Unix seconds).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OcspTimes {
    pub produced_at: i64,
    pub this_update: i64,
    pub next_update: Option<i64>,
}

fn bad(e: impl std::fmt::Display) -> Error {
    Error::Invalid(format!("invalid OCSP data: {e}"))
}

/// The DER of the issuer name of `cert` (what the issuer name hash covers).
fn issuer_name_der(cert: &Certificate) -> Vec<u8> {
    cert.tbs_certificate.issuer.to_der().unwrap_or_default()
}

/// The value of the public key of `issuer` (what the issuer key hash covers).
fn issuer_key(issuer: &Certificate) -> &[u8] {
    issuer
        .tbs_certificate
        .subject_public_key_info
        .subject_public_key
        .raw_bytes()
}

/// The CertID of `cert` with SHA-1 (RFC 5019).
fn cert_id(cert: &Certificate, issuer: &Certificate) -> Result<Vec<u8>> {
    let alg = alg_null(SHA1_OID).to_der().map_err(bad)?;
    let name_hash = digest_with(SHA1_OID, &[&issuer_name_der(cert)]).unwrap_or_default();
    let key_hash = digest_with(SHA1_OID, &[issuer_key(issuer)]).unwrap_or_default();
    let serial = cert.tbs_certificate.serial_number.to_der().map_err(bad)?;
    Ok(tlv(
        0x30,
        &[&alg, &tlv(0x04, &[&name_hash]), &tlv(0x04, &[&key_hash]), &serial],
    ))
}

/// The OCSP request for the state of `cert`, issued by `issuer`: one SHA-1
/// CertID, no nonce, no extensions, unsigned (so the same request is sent
/// every time). It carries only digests of the issuer's name and key and
/// the serial number of the certificate.
pub fn request(cert: &Certificate, issuer: &Certificate) -> Result<Vec<u8>> {
    let request = tlv(0x30, &[&cert_id(cert, issuer)?]);
    let list = tlv(0x30, &[&request]);
    let tbs = tlv(0x30, &[&list]);
    Ok(tlv(0x30, &[&tbs]))
}

/// An OCSP answer without a basic response: the responder refused (`status`
/// 1 malformed request, 2 internal error, 3 try later, 5 signature required,
/// 6 unauthorized).
pub fn refusal(status: u8) -> Result<Vec<u8>> {
    Ok(tlv(0x30, &[&tlv(0x0A, &[&[status]])]))
}

/// A successful OCSP answer around a basic response.
pub(crate) fn wrap_basic(basic: &[u8]) -> Vec<u8> {
    let oid = OCSP_BASIC.to_der().unwrap_or_default();
    let bytes = tlv(0x30, &[&oid, &tlv(0x04, &[basic])]);
    tlv(0x30, &[&tlv(0x0A, &[&[0]]), &tlv(0xA0, &[&bytes])])
}

/// The CertIDs of a request (their DER as received).
fn requested_ids(request: &[u8]) -> Option<Vec<Vec<u8>>> {
    let outer = expect(request, 0x30)?;
    let tbs = expect(outer.value, 0x30)?;
    // version [0], requestorName [1], then the list of requests.
    let mut rest = tbs.value;
    while let Some(e) = element(rest).filter(|e| e.tag == 0xA0 || e.tag == 0xA1) {
        rest = e.rest;
    }
    let list = expect(rest, 0x30)?;
    elements(list.value)?
        .iter()
        .map(|r| expect(r.value, 0x30).map(|id| id.whole.to_vec()))
        .collect()
}

/// The parts of a CertID.
#[derive(Debug, Clone)]
struct CertIdParts {
    hash: ObjectIdentifier,
    name_hash: Vec<u8>,
    key_hash: Vec<u8>,
    serial: Vec<u8>,
}

fn cert_id_parts(der: &[u8]) -> Option<CertIdParts> {
    let id = expect(der, 0x30)?;
    let alg = expect(id.value, 0x30)?;
    let name = expect(alg.rest, 0x04)?;
    let key = expect(name.rest, 0x04)?;
    let serial = expect(key.rest, 0x02)?;
    Some(CertIdParts {
        hash: AlgorithmIdentifierOwned::from_der(alg.whole).ok()?.oid,
        name_hash: name.value.to_vec(),
        key_hash: key.value.to_vec(),
        serial: serial.value.to_vec(),
    })
}

/// The answer of a local OCSP responder with the digital ID `responder` for
/// certificates issued by `issuer`: `answer` gives the state of each serial
/// number asked about (big-endian bytes); a CertID of another issuer gets
/// "unknown"; an unreadable request a refusal. When the responder is not the
/// issuer, its certificate goes with the answer.
pub fn respond(
    responder: &DigitalId,
    issuer: &Certificate,
    request: &[u8],
    answer: &dyn Fn(&[u8]) -> OcspAnswer,
    times: OcspTimes,
) -> Result<Vec<u8>> {
    let Some(ids) = requested_ids(request).filter(|ids| !ids.is_empty()) else {
        return refusal(1);
    };
    let mut singles = Vec::new();
    for id in &ids {
        let state = match cert_id_parts(id) {
            Some(p) => {
                let name_hash =
                    digest_with(p.hash, &[&issuer.tbs_certificate.subject.to_der().map_err(bad)?]);
                let key_hash = digest_with(p.hash, &[issuer_key(issuer)]);
                if name_hash.as_deref() == Some(p.name_hash.as_slice())
                    && key_hash.as_deref() == Some(p.key_hash.as_slice())
                {
                    answer(&p.serial)
                } else {
                    OcspAnswer::Unknown
                }
            }
            None => return refusal(1),
        };
        let status = match state {
            OcspAnswer::Good => vec![0x80, 0x00],
            OcspAnswer::Unknown => vec![0x82, 0x00],
            OcspAnswer::Revoked { time, reason } => {
                let mut parts = vec![generalized(time)];
                if let Some(r) = reason {
                    parts.push(tlv(0xA0, &[&tlv(0x0A, &[&[r]])]));
                }
                let refs: Vec<&[u8]> = parts.iter().map(Vec::as_slice).collect();
                tlv(0xA1, &refs)
            }
        };
        let mut parts = vec![id.clone(), status, generalized(times.this_update)];
        if let Some(next) = times.next_update {
            parts.push(tlv(0xA0, &[&generalized(next)]));
        }
        let refs: Vec<&[u8]> = parts.iter().map(Vec::as_slice).collect();
        singles.push(tlv(0x30, &refs));
    }
    let single_refs: Vec<&[u8]> = singles.iter().map(Vec::as_slice).collect();
    let responder_id = tlv(0xA2, &[&tlv(0x04, &[&key_hash(&responder.certificate)])]);
    let tbs = tlv(
        0x30,
        &[
            &responder_id,
            &generalized(times.produced_at),
            &tlv(0x30, &single_refs),
        ],
    );
    let alg = responder.key.cert_signature_alg().to_der().map_err(bad)?;
    let signature = responder.key.sign(&tbs)?;
    let mut sig_bits = vec![0u8];
    sig_bits.extend(signature);
    let mut parts = vec![tbs, alg, tlv(0x03, &[&sig_bits])];
    if responder.certificate != *issuer {
        let cert = responder.certificate.to_der().map_err(bad)?;
        parts.push(tlv(0xA0, &[&tlv(0x30, &[&cert])]));
    }
    let refs: Vec<&[u8]> = parts.iter().map(Vec::as_slice).collect();
    Ok(wrap_basic(&tlv(0x30, &refs)))
}

/// Who signed an answer.
#[derive(Debug, Clone)]
enum ResponderId {
    ByName(Name),
    ByKey(Vec<u8>),
}

/// The state given in a single answer.
#[derive(Debug, Clone, Copy)]
enum Status {
    Good,
    Revoked { time: Option<i64>, reason: Option<u8> },
    Unknown,
}

/// One answer about one certificate.
#[derive(Debug, Clone)]
struct Single {
    id: Option<CertIdParts>,
    status: Status,
    this_update: Option<i64>,
    /// `Some(None)` when the field is there but cannot be read.
    next_update: Option<Option<i64>>,
    archive_cutoff: Option<i64>,
}

/// A basic OCSP answer read for checking.
#[derive(Debug, Clone)]
pub(crate) struct ParsedOcsp {
    /// The answer as a whole OCSPResponse (a bare basic response is wrapped:
    /// the wrapper is not signed).
    pub(crate) response: Vec<u8>,
    tbs: Vec<u8>,
    sig_alg: AlgorithmIdentifierOwned,
    signature: Vec<u8>,
    responder: Option<ResponderId>,
    produced_at: Option<i64>,
    singles: Vec<Single>,
    /// Certificates sent with the answer (a delegated responder's).
    pub(crate) certs: Vec<Certificate>,
}

fn read_time(e: Option<super::der_walk::Element<'_>>) -> Option<i64> {
    e.filter(|e| e.tag == 0x18)
        .and_then(|e| generalized_time(e.value))
}

fn read_single(e: &super::der_walk::Element<'_>) -> Option<Single> {
    let id = expect(e.value, 0x30)?;
    let status_el = element(id.rest)?;
    let status = match status_el.tag {
        0x80 => Status::Good,
        0x82 => Status::Unknown,
        0xA1 => {
            let time = element(status_el.value);
            let reason = time
                .and_then(|t| expect(t.rest, 0xA0))
                .and_then(|r| expect(r.value, 0x0A))
                .and_then(|r| r.value.last().copied());
            Status::Revoked {
                time: read_time(time),
                reason,
            }
        }
        _ => return None,
    };
    let this = element(status_el.rest)?;
    let mut single = Single {
        id: cert_id_parts(id.whole),
        status,
        this_update: read_time(Some(this)),
        next_update: None,
        archive_cutoff: None,
    };
    let mut rest = this.rest;
    while let Some(f) = element(rest) {
        match f.tag {
            0xA0 => single.next_update = Some(read_time(element(f.value))),
            0xA1 => {
                if let Ok(exts) = Extensions::from_der(f.value) {
                    single.archive_cutoff = exts
                        .iter()
                        .find(|x| x.extn_id == ARCHIVE_CUTOFF)
                        .and_then(|x| expect(x.extn_value.as_bytes(), 0x18))
                        .and_then(|t| generalized_time(t.value));
                }
            }
            _ => {}
        }
        rest = f.rest;
    }
    Some(single)
}

/// Read a basic OCSP response (its DER as received).
fn read_basic(basic: &[u8]) -> Option<ParsedOcsp> {
    let outer = expect(basic, 0x30)?;
    let tbs = expect(outer.value, 0x30)?;
    let alg = expect(tbs.rest, 0x30)?;
    let sig = expect(alg.rest, 0x03)?;
    let certs = match expect(sig.rest, 0xA0).and_then(|c| expect(c.value, 0x30)) {
        Some(list) => elements(list.value)?
            .iter()
            .filter_map(|c| Certificate::from_der(c.whole).ok())
            .collect(),
        None => Vec::new(),
    };
    let mut rest = tbs.value;
    if let Some(v) = expect(rest, 0xA0) {
        rest = v.rest;
    }
    let rid = element(rest)?;
    let responder = match rid.tag {
        0xA1 => Name::from_der(rid.value).ok().map(ResponderId::ByName),
        0xA2 => expect(rid.value, 0x04).map(|k| ResponderId::ByKey(k.value.to_vec())),
        _ => None,
    };
    let produced = element(rid.rest)?;
    let responses = expect(produced.rest, 0x30)?;
    let singles = elements(responses.value)?
        .iter()
        .filter_map(read_single)
        .collect();
    Some(ParsedOcsp {
        response: wrap_basic(basic),
        tbs: tbs.whole.to_vec(),
        sig_alg: AlgorithmIdentifierOwned::from_der(alg.whole).ok()?,
        signature: sig.value.get(1..)?.to_vec(),
        responder,
        produced_at: read_time(Some(produced)),
        singles,
        certs,
    })
}

/// Read an OCSP answer: an OCSPResponse, or a bare basic response (some
/// producers store those).
pub(crate) fn parse(der: &[u8]) -> std::result::Result<ParsedOcsp, RevocationNote> {
    let der = super::cms_sig::trim_der(der);
    let Some(outer) = expect(der, 0x30).filter(|o| o.rest.is_empty()) else {
        return Err(RevocationNote::Unreadable);
    };
    let first = element(outer.value).ok_or(RevocationNote::Unreadable)?;
    if first.tag == 0x0A {
        // OCSPResponse { responseStatus, responseBytes [0] }
        if first.value != [0] {
            return Err(RevocationNote::Refused);
        }
        let bytes = expect(first.rest, 0xA0)
            .and_then(|b| expect(b.value, 0x30))
            .ok_or(RevocationNote::Unreadable)?;
        let oid = expect(bytes.value, 0x06).ok_or(RevocationNote::Unreadable)?;
        if ObjectIdentifier::from_der(oid.whole).ok() != Some(OCSP_BASIC) {
            return Err(RevocationNote::UnsupportedOcsp);
        }
        let basic = expect(oid.rest, 0x04).ok_or(RevocationNote::Unreadable)?;
        return read_basic(basic.value).ok_or(RevocationNote::Unreadable);
    }
    read_basic(der).ok_or(RevocationNote::Unreadable)
}

fn responder_matches(id: &ResponderId, cert: &Certificate) -> bool {
    match id {
        ResponderId::ByName(name) => cert.tbs_certificate.subject == *name,
        ResponderId::ByKey(hash) => key_hash(cert) == *hash,
    }
}

impl ParsedOcsp {
    /// The single answer about `cert` (issued by `issuer`).
    fn single_for(&self, cert: &Certificate, issuer: &Certificate) -> Option<&Single> {
        let serial = cert.tbs_certificate.serial_number.as_bytes();
        let name = issuer_name_der(cert);
        self.singles.iter().find(|s| {
            let Some(id) = &s.id else { return false };
            id.serial == serial
                && digest_with(id.hash, &[&name]).as_deref() == Some(id.name_hash.as_slice())
                && digest_with(id.hash, &[issuer_key(issuer)]).as_deref() == Some(id.key_hash.as_slice())
        })
    }

    /// What this answer says about `cert`, issued by `issuer`, checked at
    /// `now` (RFC 6960 3.2): the responder must be the issuer or a
    /// certificate the issuer gave the OCSP signing usage, valid when it
    /// answered, and its signature must cover the answer as received.
    pub(crate) fn finding(
        &self,
        cert: &Certificate,
        issuer: &Certificate,
        available: &[Certificate],
        now: i64,
    ) -> Finding {
        let Some(single) = self.single_for(cert, issuer) else {
            return Finding::NotAbout;
        };
        let (Some(this), Some(produced)) = (single.this_update, self.produced_at) else {
            return Finding::Unusable(RevocationNote::Unreadable);
        };
        let next = match single.next_update {
            Some(None) => return Finding::Unusable(RevocationNote::Unreadable),
            Some(Some(n)) if n < this => return Finding::Unusable(RevocationNote::Unreadable),
            Some(n) => n,
            None => None,
        };
        let mut candidates: Vec<&Certificate> = Vec::new();
        for c in std::iter::once(issuer)
            .chain(self.certs.iter())
            .chain(available.iter())
        {
            if !candidates.contains(&c) {
                candidates.push(c);
            }
        }
        let named: Vec<&Certificate> = match &self.responder {
            Some(id) => candidates
                .iter()
                .copied()
                .filter(|c| responder_matches(id, c))
                .collect(),
            None => Vec::new(),
        };
        let tried = if named.is_empty() { candidates } else { named };
        let digest = digest_of_sig_alg(self.sig_alg.oid);
        // The strongest reason a candidate was refused.
        let mut refused = RevocationNote::NotAuthorized;
        let rank = |n: &RevocationNote| match n {
            RevocationNote::BadSignature => 3,
            RevocationNote::ResponderNotValid => 2,
            _ => 1,
        };
        let mut accepted: Option<(&Certificate, bool)> = None;
        for c in tried {
            let is_issuer =
                c.tbs_certificate.subject_public_key_info == issuer.tbs_certificate.subject_public_key_info;
            let reason = if !verify_with_cert(c, &self.sig_alg, digest, &self.tbs, &self.signature) {
                Some(RevocationNote::BadSignature)
            } else if !is_issuer && !(issued_by(c, issuer) && has_extended_usage(c, KP_OCSP_SIGNING)) {
                Some(RevocationNote::NotAuthorized)
            } else if !is_issuer && {
                let info = cert_info(c);
                produced < info.not_before_unix || produced > info.not_after_unix
            } {
                Some(RevocationNote::ResponderNotValid)
            } else {
                None
            };
            match reason {
                None => {
                    accepted = Some((c, !is_issuer));
                    break;
                }
                Some(r) if rank(&r) > rank(&refused) => refused = r,
                Some(_) => {}
            }
        }
        let Some((responder, delegated)) = accepted else {
            return Finding::Unusable(refused);
        };
        // The responder was checked when it answered (producedAt), and the
        // state it gives (thisUpdate) cannot be later than its answer: a
        // backdated answer of an expired responder is not used.
        if this > produced + CLOCK_SKEW {
            return Finding::Unusable(RevocationNote::InconsistentTimes);
        }
        if this > now + CLOCK_SKEW || produced > now + CLOCK_SKEW {
            return Finding::Unusable(RevocationNote::FromFuture);
        }
        let authority = cert_info(responder).common_name;
        let delegate = delegated.then(|| responder.clone());
        match single.status {
            Status::Good => Finding::Good {
                this,
                next,
                authority,
                responder: delegate,
                expired_since: single.archive_cutoff,
                responder_unchecked: delegated && extension_value(responder, OCSP_NO_CHECK).is_none(),
            },
            Status::Revoked {
                time: Some(time),
                reason,
            } => Finding::Revoked {
                time,
                reason,
                this,
                next,
                authority,
                responder: delegate,
            },
            Status::Revoked { time: None, .. } => Finding::Unusable(RevocationNote::Unreadable),
            Status::Unknown => Finding::Unusable(RevocationNote::UnknownToResponder),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::digsig::{CertProfile, ExtendedUsage, IdentityInfo, KeyKind};

    const SIGN: i64 = 1_792_324_800;
    const DAY: i64 = 86_400;

    fn who(name: &str) -> IdentityInfo {
        IdentityInfo {
            name: name.into(),
            ..Default::default()
        }
    }

    fn ca() -> DigitalId {
        DigitalId::create_with(
            &who("Unit Root CA"),
            KeyKind::EcP256,
            &CertProfile {
                not_before: SIGN - 400 * DAY,
                not_after: SIGN + 3650 * DAY,
                ca: true,
                ..Default::default()
            },
        )
        .unwrap()
    }

    fn signer(ca: &DigitalId) -> DigitalId {
        ca.issue(
            &who("Unit Signer"),
            KeyKind::EcP256,
            &CertProfile {
                not_before: SIGN - 30 * DAY,
                not_after: SIGN + 365 * DAY,
                ..Default::default()
            },
        )
        .unwrap()
    }

    fn times(this: i64) -> OcspTimes {
        OcspTimes {
            produced_at: this,
            this_update: this,
            next_update: Some(this + 7 * DAY),
        }
    }

    #[test]
    fn requests_and_answers_round_trip() {
        let ca = ca();
        let s = signer(&ca);
        let req = request(&s.certificate, &ca.certificate).unwrap();
        assert!(req.len() < 150, "{}", req.len());
        let ids = requested_ids(&req).unwrap();
        assert_eq!(ids.len(), 1);
        let parts = cert_id_parts(&ids[0]).unwrap();
        assert_eq!(parts.hash, SHA1_OID);
        assert_eq!(
            parts.serial,
            s.certificate.tbs_certificate.serial_number.as_bytes()
        );
        for (answer, expected) in [
            (OcspAnswer::Good, "good"),
            (
                OcspAnswer::Revoked {
                    time: SIGN - 10,
                    reason: Some(1),
                },
                "revoked",
            ),
            (OcspAnswer::Unknown, "unknown"),
        ] {
            let der = respond(&ca, &ca.certificate, &req, &|_| answer, times(SIGN)).unwrap();
            let parsed = parse(&der).unwrap();
            assert!(parsed.certs.is_empty());
            let f = parsed.finding(&s.certificate, &ca.certificate, &[], SIGN + DAY);
            match (expected, f) {
                ("good", Finding::Good { this, next, .. }) => {
                    assert_eq!((this, next), (SIGN, Some(SIGN + 7 * DAY)))
                }
                ("revoked", Finding::Revoked { time, reason, .. }) => {
                    assert_eq!((time, reason), (SIGN - 10, Some(1)))
                }
                ("unknown", Finding::Unusable(RevocationNote::UnknownToResponder)) => {}
                (e, f) => panic!("{e}: {f:?}"),
            }
        }
        // A request that cannot be read is refused (malformed request).
        let refused = respond(
            &ca,
            &ca.certificate,
            b"\x30\x00",
            &|_| OcspAnswer::Good,
            times(SIGN),
        )
        .unwrap();
        assert_eq!(refused, refusal(1).unwrap());
        assert!(matches!(parse(&refused), Err(RevocationNote::Refused)));
        // A delegated responder sends its certificate.
        let delegate = ca
            .issue(
                &who("Unit Responder"),
                KeyKind::EcP256,
                &CertProfile {
                    not_before: SIGN - DAY,
                    not_after: SIGN + 30 * DAY,
                    usages: vec![ExtendedUsage::OcspSigning],
                    ocsp_no_check: true,
                    ..Default::default()
                },
            )
            .unwrap();
        let der = respond(
            &delegate,
            &ca.certificate,
            &req,
            &|_| OcspAnswer::Good,
            times(SIGN),
        )
        .unwrap();
        let parsed = parse(&der).unwrap();
        assert_eq!(parsed.certs, vec![delegate.certificate.clone()]);
        match parsed.finding(&s.certificate, &ca.certificate, &[], SIGN) {
            Finding::Good {
                authority,
                responder_unchecked,
                ..
            } => {
                assert_eq!(authority, "Unit Responder");
                assert!(!responder_unchecked);
            }
            f => panic!("{f:?}"),
        }
    }

    #[test]
    fn fractional_times_are_read() {
        let ca = ca();
        let s = signer(&ca);
        // A basic response written by hand, with fractions of a second.
        let id = cert_id(&s.certificate, &ca.certificate).unwrap();
        let time = tlv(0x18, &[b"20261018120000.5Z"]);
        let single = tlv(0x30, &[&id, &[0x80, 0x00], &time]);
        let rid = tlv(0xA2, &[&tlv(0x04, &[&key_hash(&ca.certificate)])]);
        let tbs = tlv(0x30, &[&rid, &time, &tlv(0x30, &[&single])]);
        let sig = ca.key.sign(&tbs).unwrap();
        let mut bits = vec![0u8];
        bits.extend(sig);
        let alg = ca.key.cert_signature_alg().to_der().unwrap();
        let basic = tlv(0x30, &[&tbs, &alg, &tlv(0x03, &[&bits])]);
        // Stored bare (some producers do): it is wrapped for the DSS.
        let parsed = parse(&basic).unwrap();
        assert!(parse(&parsed.response).is_ok());
        assert!(matches!(
            parsed.finding(&s.certificate, &ca.certificate, &[], SIGN + DAY),
            Finding::Good {
                this: SIGN,
                next: None,
                ..
            }
        ));
    }

    #[test]
    fn answers_about_other_certificates_do_not_count() {
        let ca = ca();
        let s = signer(&ca);
        let other = signer(&ca);
        let req = request(&other.certificate, &ca.certificate).unwrap();
        let der = respond(&ca, &ca.certificate, &req, &|_| OcspAnswer::Good, times(SIGN)).unwrap();
        let parsed = parse(&der).unwrap();
        assert!(matches!(
            parsed.finding(&s.certificate, &ca.certificate, &[], SIGN),
            Finding::NotAbout
        ));
        // A SHA-256 CertID is matched too.
        let alg = alg_null(super::super::id::SHA256_OID).to_der().unwrap();
        let name = digest_with(super::super::id::SHA256_OID, &[&issuer_name_der(&s.certificate)]).unwrap();
        let key = digest_with(super::super::id::SHA256_OID, &[issuer_key(&ca.certificate)]).unwrap();
        let serial = s.certificate.tbs_certificate.serial_number.to_der().unwrap();
        let id = tlv(0x30, &[&alg, &tlv(0x04, &[&name]), &tlv(0x04, &[&key]), &serial]);
        let req = tlv(0x30, &[&tlv(0x30, &[&tlv(0x30, &[&tlv(0x30, &[&id])])])]);
        let der = respond(&ca, &ca.certificate, &req, &|_| OcspAnswer::Good, times(SIGN)).unwrap();
        assert!(matches!(
            parse(&der)
                .unwrap()
                .finding(&s.certificate, &ca.certificate, &[], SIGN),
            Finding::Good { .. }
        ));
        // An answer about two certificates: the one asked about is found.
        let two = tlv(
            0x30,
            &[&tlv(
                0x30,
                &[&tlv(
                    0x30,
                    &[
                        &tlv(0x30, &[&cert_id(&other.certificate, &ca.certificate).unwrap()]),
                        &tlv(0x30, &[&cert_id(&s.certificate, &ca.certificate).unwrap()]),
                    ],
                )],
            )],
        );
        let answer = |serial: &[u8]| {
            if serial == other.certificate.tbs_certificate.serial_number.as_bytes() {
                OcspAnswer::Revoked {
                    time: SIGN,
                    reason: None,
                }
            } else {
                OcspAnswer::Good
            }
        };
        let der = respond(&ca, &ca.certificate, &two, &answer, times(SIGN)).unwrap();
        let parsed = parse(&der).unwrap();
        assert!(matches!(
            parsed.finding(&s.certificate, &ca.certificate, &[], SIGN),
            Finding::Good { .. }
        ));
        assert!(matches!(
            parsed.finding(&other.certificate, &ca.certificate, &[], SIGN),
            Finding::Revoked { .. }
        ));
        // Refusals and other types of answers.
        assert!(matches!(
            parse(&refusal(3).unwrap()),
            Err(RevocationNote::Refused)
        ));
        assert!(matches!(parse(b"garbage"), Err(RevocationNote::Unreadable)));
    }
}
