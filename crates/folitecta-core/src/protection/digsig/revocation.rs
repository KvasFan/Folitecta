//! Revocation of the certificates of signatures: OCSP answers (RFC 6960) and
//! revocation lists (RFC 5280) found in the signature (the revocation
//! information attribute of ISO 32000-1 12.8.3.3.2 and the lists of the CMS
//! structure), in its time stamp, in the document security store, or fetched
//! online when the user turned that on. Each certificate of the chain up to
//! a trusted certificate is checked for the time of the signature.
//!
//! The rules, for a signature checked for the time `at`, proven to exist at
//! `exist` (the time of a trusted time stamp, of a later document time stamp,
//! or now when nothing proves it):
//! - a revocation at or before `at` fails the check (a warning when the
//!   signing time is not known at all);
//! - a later revocation passes only when it comes after `exist`;
//! - "not revoked" counts only from data issued after `exist` (data issued
//!   before cannot show that nothing happened in between), and from data
//!   issued after the certificate expired only when the issuer keeps expired
//!   certificates on its lists (ExpiredCertsOnCRL, archive cutoff);
//! - a certificate on hold at `at` fails when the hold is still listed after
//!   `at`, and cannot be determined when it was released at an unknown time;
//! - anything else "cannot be determined", never "good".

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

use const_oid::ObjectIdentifier;
use der::{Decode, Encode};
use sha2::{Digest, Sha256};
use x509_cert::Certificate;

use super::cms_sig::issued_by;
use super::crl::{self, ParsedCrl};
use super::der_walk::{element, elements, expect, tlv};
use super::id::{AD_OCSP, AUTHORITY_INFO_ACCESS, CRL_DISTRIBUTION_POINTS, cert_info, extension_value};
use super::ocsp::{self, ParsedOcsp};
use super::pdf::Check;

/// Clock difference tolerated between Folitecta and the authorities (s).
pub const CLOCK_SKEW: i64 = 300;
/// Signed attribute of the revocation information of a signature (ISO
/// 32000-1 12.8.3.3.2).
pub(crate) const ATTR_REVOCATION_ARCHIVE: ObjectIdentifier =
    ObjectIdentifier::new_unwrap("1.2.840.113583.1.1.8");
/// OCSP answers among the revocation information of a CMS structure (RFC 5940).
const RI_OCSP_RESPONSE: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.3.6.1.5.5.7.16.2");
/// At most this many revocation data are read for one validation.
const MAX_EVIDENCE: usize = 4000;
/// At most this many candidate issuers are verified for one certificate.
const MAX_ISSUER_TRIES: usize = 16;

/// Revocation data: DER revocation lists and OCSP answers.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RevocationArchive {
    /// DER CertificateList.
    pub crls: Vec<Vec<u8>>,
    /// DER OCSPResponse.
    pub ocsps: Vec<Vec<u8>>,
}

impl RevocationArchive {
    pub fn is_empty(&self) -> bool {
        self.crls.is_empty() && self.ocsps.is_empty()
    }

    /// Bytes of all the data.
    pub fn byte_len(&self) -> usize {
        self.crls.iter().chain(self.ocsps.iter()).map(Vec::len).sum()
    }

    pub(crate) fn push_crl(&mut self, der: Vec<u8>) {
        if !self.crls.contains(&der) {
            self.crls.push(der);
        }
    }

    pub(crate) fn push_ocsp(&mut self, der: Vec<u8>) {
        if !self.ocsps.contains(&der) {
            self.ocsps.push(der);
        }
    }

    pub(crate) fn extend(&mut self, other: &RevocationArchive) {
        for c in &other.crls {
            self.push_crl(c.clone());
        }
        for o in &other.ocsps {
            self.push_ocsp(o.clone());
        }
    }

    /// The DER value of the signed attribute: `SEQUENCE { crl [0] EXPLICIT
    /// SEQUENCE OF CertificateList OPTIONAL, ocsp [1] EXPLICIT SEQUENCE OF
    /// OCSPResponse OPTIONAL }`.
    pub(crate) fn attribute_value(&self) -> Vec<u8> {
        let mut parts = Vec::new();
        if !self.crls.is_empty() {
            let items: Vec<&[u8]> = self.crls.iter().map(Vec::as_slice).collect();
            parts.push(tlv(0xA0, &[&tlv(0x30, &items)]));
        }
        if !self.ocsps.is_empty() {
            let items: Vec<&[u8]> = self.ocsps.iter().map(Vec::as_slice).collect();
            parts.push(tlv(0xA1, &[&tlv(0x30, &items)]));
        }
        let refs: Vec<&[u8]> = parts.iter().map(Vec::as_slice).collect();
        tlv(0x30, &refs)
    }

    /// The data of the value of the signed attribute, each item as received;
    /// what cannot be read is left out.
    pub(crate) fn from_attribute(der: &[u8]) -> RevocationArchive {
        let mut out = RevocationArchive::default();
        let Some(outer) = expect(der, 0x30) else {
            return out;
        };
        for part in elements(outer.value).unwrap_or_default() {
            let Some(list) = expect(part.value, 0x30) else {
                continue;
            };
            for item in elements(list.value).unwrap_or_default() {
                match part.tag {
                    0xA0 => out.push_crl(item.whole.to_vec()),
                    0xA1 => out.push_ocsp(item.whole.to_vec()),
                    _ => {}
                }
            }
        }
        out
    }

    /// The revocation information of a CMS structure (the value of its
    /// `crls` field): revocation lists, and OCSP answers in the RFC 5940
    /// form; each item as received.
    pub(crate) fn from_signed_data_field(value: &[u8]) -> RevocationArchive {
        let mut out = RevocationArchive::default();
        for item in elements(value).unwrap_or_default() {
            match item.tag {
                0x30 => out.push_crl(item.whole.to_vec()),
                0xA1 => {
                    let Some(oid) = expect(item.value, 0x06) else {
                        continue;
                    };
                    if ObjectIdentifier::from_der(oid.whole).ok() == Some(RI_OCSP_RESPONSE) {
                        if let Some(info) = element(oid.rest) {
                            out.push_ocsp(info.whole.to_vec());
                        }
                    }
                }
                _ => {}
            }
        }
        out
    }
}

/// Revocation data fetched online for a document (kept by the interface for
/// its tab), and the requests that failed.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RevocationPool {
    pub data: RevocationArchive,
    pub failures: Vec<FetchFailure>,
}

/// Why a request for revocation data gave nothing usable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FetchProblem {
    /// The server could not be reached or answered with an error (the
    /// transport's own text).
    Transport(String),
    /// The answer was received but cannot be used.
    Answer(Box<RevocationNote>),
    /// The answer is about another certificate.
    AnotherCertificate,
    /// Not asked: the run reached its limit of requests or time.
    NotAsked,
}

/// A request that gave nothing usable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FetchFailure {
    /// SHA-256 (hexadecimal) of the certificate the request was for.
    pub cert: String,
    pub url: String,
    pub problem: FetchProblem,
}

/// One request Folitecta would send: an OCSP request (POST of `ocsp`) or the
/// download of a revocation list (GET).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RevocationRequest {
    /// SHA-256 (hexadecimal) of the certificate asked about.
    pub cert: String,
    /// Name of the certificate asked about.
    pub subject: String,
    pub url: String,
    pub ocsp: Option<Vec<u8>>,
}

/// Sends a request and returns the body of the answer.
pub type RevocationTransport<'a> = dyn FnMut(&RevocationRequest) -> crate::Result<Vec<u8>> + 'a;

/// What an online run did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FetchSummary {
    /// Certificates asked about.
    pub certificates: usize,
    /// Certificates whose state was received.
    pub decided: usize,
    /// Requests sent.
    pub requests: usize,
    /// Requests that gave nothing usable.
    pub failures: usize,
}

/// The state of a certificate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RevocationState {
    /// Not revoked at the time checked.
    Good,
    /// Revoked at `time` (Unix seconds) for `reason` (RFC 5280 5.3.1).
    Revoked { time: i64, reason: Option<u8> },
    /// Cannot be determined.
    Unknown,
}

/// Where the data that decided came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RevocationSource {
    Signature,
    TimeStamp,
    /// The document security store of the file.
    Dss,
    /// Fetched online.
    Fetched,
}

/// The kind of data that decided.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RevocationKind {
    Ocsp,
    Crl,
}

/// Why the state of a certificate cannot be determined (or a reservation).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RevocationNote {
    NoData,
    IssuerMissing,
    /// Data issued before the time checked.
    IssuedBefore,
    /// Data issued before the signature is proven to exist.
    NotAfterProof,
    /// Data issued after the certificate expired, which need not list it.
    AfterExpiry,
    FromFuture,
    /// The answer says the state at a time after it was given.
    InconsistentTimes,
    BadSignature,
    NotAuthorized,
    ResponderNotValid,
    /// The OCSP responder is a delegated one whose own revocation is not checked.
    ResponderUnchecked,
    UnknownToResponder,
    Refused,
    Unreadable,
    UnsupportedList,
    UnsupportedOcsp,
    /// The certificate was on hold; when the hold ended is not known.
    HoldUnknown,
    /// The chain does not end at a trusted certificate.
    Untrusted,
    /// Revoked, and the signing time is not known.
    SigningTimeUnknown,
    FetchFailed {
        url: String,
        problem: FetchProblem,
    },
}

impl RevocationNote {
    /// Key of the note (the interface text is `sig.rev_note_<key>`).
    pub fn key(&self) -> &'static str {
        match self {
            RevocationNote::NoData => "no_data",
            RevocationNote::IssuerMissing => "issuer_missing",
            RevocationNote::IssuedBefore => "issued_before",
            RevocationNote::NotAfterProof => "not_after_proof",
            RevocationNote::AfterExpiry => "after_expiry",
            RevocationNote::FromFuture => "from_future",
            RevocationNote::InconsistentTimes => "inconsistent_times",
            RevocationNote::BadSignature => "bad_signature",
            RevocationNote::NotAuthorized => "not_authorized",
            RevocationNote::ResponderNotValid => "responder_not_valid",
            RevocationNote::ResponderUnchecked => "responder_unchecked",
            RevocationNote::UnknownToResponder => "unknown_to_responder",
            RevocationNote::Refused => "refused",
            RevocationNote::Unreadable => "unreadable",
            RevocationNote::UnsupportedList => "unsupported_list",
            RevocationNote::UnsupportedOcsp => "unsupported_ocsp",
            RevocationNote::HoldUnknown => "hold_unknown",
            RevocationNote::Untrusted => "untrusted",
            RevocationNote::SigningTimeUnknown => "signing_time_unknown",
            RevocationNote::FetchFailed { .. } => "fetch_failed",
        }
    }

    /// English text of the note.
    pub fn text(&self) -> String {
        match self {
            RevocationNote::NoData => "there are no revocation data for this certificate".into(),
            RevocationNote::IssuerMissing => "the certificate of its issuer is missing".into(),
            RevocationNote::IssuedBefore => "the revocation data were issued before the time checked".into(),
            RevocationNote::NotAfterProof => {
                "the revocation data were issued before the signature is proven to exist".into()
            }
            RevocationNote::AfterExpiry => {
                "the revocation data were issued after the certificate expired and need not list it".into()
            }
            RevocationNote::FromFuture => "the revocation data are dated in the future".into(),
            RevocationNote::InconsistentTimes => {
                "the OCSP answer gives the state at a time after it was given".into()
            }
            RevocationNote::BadSignature => {
                "the revocation data were changed or are not signed by their issuer".into()
            }
            RevocationNote::NotAuthorized => {
                "the revocation data are signed by an authority that may not give them".into()
            }
            RevocationNote::ResponderNotValid => {
                "the certificate of the OCSP responder was not valid when it answered".into()
            }
            RevocationNote::ResponderUnchecked => {
                "the revocation of the OCSP responder's own certificate is not checked".into()
            }
            RevocationNote::UnknownToResponder => "the OCSP responder does not know the certificate".into(),
            RevocationNote::Refused => "the OCSP responder refused to answer".into(),
            RevocationNote::Unreadable => "the revocation data cannot be read".into(),
            RevocationNote::UnsupportedList => {
                "the revocation list is of a kind Folitecta does not read (delta, indirect, partitioned by reasons, or naming its distribution point relative to its issuer)"
                    .into()
            }
            RevocationNote::UnsupportedOcsp => "the OCSP answer is of a kind Folitecta does not read".into(),
            RevocationNote::HoldUnknown => {
                "the certificate was on hold and when the hold ended is not known".into()
            }
            RevocationNote::Untrusted => "the chain does not end at a trusted certificate".into(),
            RevocationNote::SigningTimeUnknown => "the signing time is not known".into(),
            RevocationNote::FetchFailed { url, problem } => format!("{url}: {}", problem_text(problem)),
        }
    }
}

/// English text of a fetch problem.
pub fn problem_text(problem: &FetchProblem) -> String {
    match problem {
        FetchProblem::Transport(detail) => detail.clone(),
        FetchProblem::Answer(note) => note.text(),
        FetchProblem::AnotherCertificate => "the answer is about another certificate".into(),
        FetchProblem::NotAsked => "not asked: the limit of requests or time of the run was reached".into(),
    }
}

/// The revocation of one certificate of a chain.
#[derive(Debug, Clone, PartialEq)]
pub struct CertRevocation {
    pub subject: String,
    /// Serial number (hexadecimal).
    pub serial: String,
    /// SHA-256 of the certificate (hexadecimal).
    pub cert_sha256: String,
    pub state: RevocationState,
    /// What the state means for the signature: passed, a reservation, failed.
    pub verdict: Check,
    pub source: Option<RevocationSource>,
    pub kind: Option<RevocationKind>,
    /// Name of the OCSP responder or list issuer that decided.
    pub authority: String,
    pub this_update: Option<i64>,
    pub next_update: Option<i64>,
    /// The http(s) addresses of OCSP responders and revocation lists the
    /// certificate names.
    pub addresses: Vec<String>,
    pub notes: Vec<RevocationNote>,
}

/// What the time checked rests on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TimeBasis {
    /// The time of a valid time stamp of a trusted authority (proven).
    TimeStamp,
    /// The time given by the signer (or by the authority of a document time
    /// stamp), not proven.
    Claimed,
    /// Now: the signing time is not known.
    #[default]
    Now,
}

/// The http(s) addresses of OCSP responders a certificate names (authority
/// information access), in order.
pub fn ocsp_urls(cert: &Certificate) -> Vec<String> {
    let Some(v) = extension_value(cert, AUTHORITY_INFO_ACCESS) else {
        return Vec::new();
    };
    let Ok(aia) = x509_cert::ext::pkix::AuthorityInfoAccessSyntax::from_der(v) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for d in aia.0.iter().filter(|d| d.access_method == AD_OCSP) {
        if let x509_cert::ext::pkix::name::GeneralName::UniformResourceIdentifier(u) = &d.access_location {
            push_http(&mut out, u.as_str());
        }
    }
    out
}

/// The http(s) addresses of the revocation lists a certificate names (CRL
/// distribution points), in order; points of indirect or partitioned lists
/// are left out.
pub fn crl_urls(cert: &Certificate) -> Vec<String> {
    use x509_cert::ext::pkix::name::{DistributionPointName, GeneralName};
    let Some(v) = extension_value(cert, CRL_DISTRIBUTION_POINTS) else {
        return Vec::new();
    };
    let Ok(points) = x509_cert::ext::pkix::CrlDistributionPoints::from_der(v) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for p in points
        .0
        .iter()
        .filter(|p| p.crl_issuer.is_none() && p.reasons.is_none())
    {
        if let Some(DistributionPointName::FullName(names)) = &p.distribution_point {
            for n in names {
                if let GeneralName::UniformResourceIdentifier(u) = n {
                    push_http(&mut out, u.as_str());
                }
            }
        }
    }
    out
}

fn push_http(out: &mut Vec<String>, url: &str) {
    let lower = url.trim().to_ascii_lowercase();
    if (lower.starts_with("http://") || lower.starts_with("https://")) && !out.iter().any(|u| u == url.trim())
    {
        out.push(url.trim().to_string());
    }
}

/// What one revocation datum says about one certificate.
#[derive(Debug, Clone)]
pub(crate) enum Finding {
    /// The datum is not about the certificate.
    NotAbout,
    Good {
        this: i64,
        next: Option<i64>,
        authority: String,
        /// A delegated OCSP responder.
        responder: Option<Certificate>,
        /// Revoked certificates stay listed when they expired after this
        /// time (ExpiredCertsOnCRL, archive cutoff).
        expired_since: Option<i64>,
        responder_unchecked: bool,
    },
    Revoked {
        time: i64,
        reason: Option<u8>,
        this: i64,
        next: Option<i64>,
        authority: String,
        responder: Option<Certificate>,
    },
    /// The datum is about the certificate but cannot be used.
    Unusable(RevocationNote),
}

pub(crate) fn sha256(data: &[u8]) -> [u8; 32] {
    Sha256::digest(data).into()
}

pub(crate) fn hex(data: &[u8]) -> String {
    data.iter().map(|b| format!("{b:02X}")).collect()
}

fn cert_sha(cert: &Certificate) -> [u8; 32] {
    sha256(&cert.to_der().unwrap_or_default())
}

/// A revocation datum read once for a validation.
enum Parsed {
    Ocsp(Box<ParsedOcsp>),
    Crl(Box<ParsedCrl>),
    Bad,
}

struct Item {
    kind: RevocationKind,
    parsed: Parsed,
}

/// What a datum says about a certificate, by the SHA-256 of the datum, of the
/// certificate and of its issuer.
type FindingMemo = HashMap<([u8; 32], [u8; 32], [u8; 32]), Finding>;

/// The revocation data of one validation: each read once, and what each says
/// about a certificate worked out once.
#[derive(Default)]
pub(crate) struct EvidenceStore {
    items: HashMap<[u8; 32], Item>,
    memo: RefCell<FindingMemo>,
}

/// One datum available to a signature, and where it came from.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Evidence {
    pub(crate) source: RevocationSource,
    pub(crate) kind: RevocationKind,
    sha: [u8; 32],
}

impl EvidenceStore {
    fn add(&mut self, kind: RevocationKind, der: &[u8]) -> Option<[u8; 32]> {
        let sha = sha256(der);
        if !self.items.contains_key(&sha) {
            if self.items.len() >= MAX_EVIDENCE {
                return None;
            }
            let parsed = match kind {
                RevocationKind::Ocsp => ocsp::parse(der).map_or(Parsed::Bad, |p| Parsed::Ocsp(Box::new(p))),
                RevocationKind::Crl => crl::parse(der).map_or(Parsed::Bad, |p| Parsed::Crl(Box::new(p))),
            };
            self.items.insert(sha, Item { kind, parsed });
        }
        Some(sha)
    }

    /// The data of `sources` in order (the first source of a datum names
    /// it), read into the store.
    pub(crate) fn gather(&mut self, sources: &[(RevocationSource, &RevocationArchive)]) -> Vec<Evidence> {
        let mut seen = HashSet::new();
        let mut out = Vec::new();
        for (source, archive) in sources {
            for (kind, list) in [
                (RevocationKind::Ocsp, &archive.ocsps),
                (RevocationKind::Crl, &archive.crls),
            ] {
                for der in list {
                    let Some(sha) = self.add(kind, der) else {
                        continue;
                    };
                    if seen.insert(sha) {
                        out.push(Evidence {
                            source: *source,
                            kind: self.items[&sha].kind,
                            sha,
                        });
                    }
                }
            }
        }
        out
    }

    /// The DER to keep for a datum (an OCSP answer as an OCSPResponse).
    pub(crate) fn kept_der(&self, e: &Evidence) -> Option<Vec<u8>> {
        match &self.items.get(&e.sha)?.parsed {
            Parsed::Ocsp(p) => Some(p.response.clone()),
            Parsed::Crl(p) => Some(p.der.clone()),
            Parsed::Bad => None,
        }
    }

    fn finding(
        &self,
        e: &Evidence,
        cert: &Certificate,
        cert_sha: [u8; 32],
        issuer: &Certificate,
        issuer_sha: [u8; 32],
        available: &[Certificate],
        now: i64,
    ) -> Finding {
        let key = (e.sha, cert_sha, issuer_sha);
        if let Some(f) = self.memo.borrow().get(&key) {
            return f.clone();
        }
        let f = match self.items.get(&e.sha).map(|i| &i.parsed) {
            Some(Parsed::Ocsp(p)) => p.finding(cert, issuer, available, now),
            Some(Parsed::Crl(p)) => p.finding(cert, issuer, now),
            // Unreadable data cannot be tied to a certificate.
            _ => Finding::NotAbout,
        };
        self.memo.borrow_mut().insert(key, f.clone());
        f
    }
}

/// The time a signature is checked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TimeFrame {
    /// The time checked.
    pub(crate) at: i64,
    pub(crate) basis: TimeBasis,
    /// When the signature is proven to exist (`at` when the time is proven,
    /// else the time of a later document time stamp, else now).
    pub(crate) exist: i64,
    /// The time of the validation.
    pub(crate) now: i64,
}

/// The revocation of a chain of certificates.
#[derive(Debug, Clone, Default)]
pub(crate) struct ChainCheck {
    pub(crate) entries: Vec<CertRevocation>,
    pub(crate) check: Check,
    /// The chain ends at a trusted certificate.
    pub(crate) trusted: bool,
    /// The certificate of each entry and its issuer.
    pub(crate) subjects: Vec<(Certificate, Option<Certificate>)>,
    /// The data that decided, as kept (kind, DER, where it came from).
    pub(crate) used: Vec<(RevocationKind, Vec<u8>, RevocationSource)>,
    /// The certificates of the chain (the trusted one included) and of the
    /// delegated OCSP responders that decided.
    pub(crate) certs: Vec<Certificate>,
}

fn is_trusted(cert: &Certificate, trusted: &[Certificate]) -> bool {
    trusted.iter().any(|t| {
        t == cert
            || (t.tbs_certificate.subject == cert.tbs_certificate.subject
                && t.tbs_certificate.subject_public_key_info == cert.tbs_certificate.subject_public_key_info)
    })
}

fn self_signed(cert: &Certificate) -> bool {
    cert.tbs_certificate.subject == cert.tbs_certificate.issuer
}

/// The issuer of `cert` among `pool` (trusted certificates first).
fn find_issuer<'a>(cert: &Certificate, pool: &'a [Certificate]) -> Option<&'a Certificate> {
    pool.iter()
        .filter(|c| *c != cert && c.tbs_certificate.subject == cert.tbs_certificate.issuer)
        .take(MAX_ISSUER_TRIES)
        .find(|c| issued_by(cert, c))
}

/// The certificates whose revocation is checked, each with its issuer, and
/// whether the chain ends at a trusted certificate. The walk stops at a
/// trusted certificate (checked itself when it is not self-signed and its
/// issuer is at hand) and at a self-signed one (never checked).
fn walk(
    first: &Certificate,
    pool: &[Certificate],
    trusted: &[Certificate],
) -> (Vec<(Certificate, Option<Certificate>)>, bool) {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    let mut cur = first.clone();
    seen.insert(cert_sha(&cur));
    for _ in 0..8 {
        if is_trusted(&cur, trusted) {
            if !self_signed(&cur) {
                if let Some(issuer) = find_issuer(&cur, pool) {
                    out.push((cur, Some(issuer.clone())));
                }
            }
            return (out, true);
        }
        if self_signed(&cur) {
            break;
        }
        let issuer = find_issuer(&cur, pool).cloned();
        out.push((cur.clone(), issuer.clone()));
        match issuer {
            Some(i) if seen.insert(cert_sha(&i)) => cur = i,
            _ => break,
        }
    }
    (out, false)
}

struct Good {
    this: i64,
    next: Option<i64>,
    authority: String,
    evidence: Evidence,
    responder: Option<Certificate>,
    unchecked: bool,
}

struct Revoked {
    time: i64,
    reason: Option<u8>,
    this: i64,
    next: Option<i64>,
    authority: String,
    evidence: Evidence,
    responder: Option<Certificate>,
}

/// Check every certificate of the chain of `first` (the signer, or the
/// authority of a time stamp) for the time `frame`, with `evidence` and the
/// online failures of the pool.
#[allow(clippy::too_many_arguments)]
pub(crate) fn check_chain(
    first: &Certificate,
    available: &[Certificate],
    trusted: &[Certificate],
    store: &EvidenceStore,
    evidence: &[Evidence],
    failures: &[FetchFailure],
    frame: TimeFrame,
) -> ChainCheck {
    let mut pool: Vec<Certificate> = Vec::new();
    for c in trusted.iter().chain(available.iter()) {
        if !pool.contains(c) {
            pool.push(c.clone());
        }
    }
    let (subjects, chain_trusted) = walk(first, &pool, trusted);
    let mut out = ChainCheck {
        trusted: chain_trusted,
        ..ChainCheck::default()
    };
    for (cert, issuer) in &subjects {
        for c in std::iter::once(cert).chain(issuer.iter()) {
            if !out.certs.contains(c) {
                out.certs.push(c.clone());
            }
        }
        let mut entry = decide(cert, issuer.as_ref(), &pool, store, evidence, failures, frame);
        if !chain_trusted {
            entry.0.notes.push(RevocationNote::Untrusted);
            if entry.0.verdict == Check::Passed {
                entry.0.verdict = Check::Warning;
            }
        }
        if let Some((e, responder)) = entry.1 {
            if let Some(der) = store.kept_der(&e) {
                if !out.used.iter().any(|(_, d, _)| *d == der) {
                    out.used.push((e.kind, der, e.source));
                }
            }
            if let Some(r) = responder {
                if !out.certs.contains(&r) {
                    out.certs.push(r);
                }
            }
        }
        out.entries.push(entry.0);
    }
    out.subjects = subjects;
    out.check = overall(&out.entries);
    out
}

/// The overall check of the entries of a chain.
pub(crate) fn overall(entries: &[CertRevocation]) -> Check {
    if entries.is_empty() {
        Check::NotChecked
    } else if entries.iter().any(|e| e.verdict == Check::Failed) {
        Check::Failed
    } else if entries.iter().any(|e| e.verdict != Check::Passed) {
        Check::Warning
    } else {
        Check::Passed
    }
}

/// The revocation of `cert`, and the datum that decided (with the delegated
/// responder that gave it).
fn decide(
    cert: &Certificate,
    issuer: Option<&Certificate>,
    available: &[Certificate],
    store: &EvidenceStore,
    evidence: &[Evidence],
    failures: &[FetchFailure],
    frame: TimeFrame,
) -> (CertRevocation, Option<(Evidence, Option<Certificate>)>) {
    let info = cert_info(cert);
    let mut addresses = ocsp_urls(cert);
    addresses.extend(crl_urls(cert));
    let mut entry = CertRevocation {
        subject: if info.common_name.is_empty() {
            info.subject.clone()
        } else {
            info.common_name.clone()
        },
        serial: info.serial.clone(),
        cert_sha256: info.sha256.clone(),
        state: RevocationState::Unknown,
        verdict: Check::Warning,
        source: None,
        kind: None,
        authority: String::new(),
        this_update: None,
        next_update: None,
        addresses,
        notes: Vec::new(),
    };
    let Some(issuer) = issuer else {
        entry.notes.push(RevocationNote::IssuerMissing);
        return (entry, None);
    };
    let TimeFrame {
        at,
        basis,
        exist,
        now,
    } = frame;
    let (sha, issuer_sha) = (cert_sha(cert), cert_sha(issuer));
    let mut notes: Vec<RevocationNote> = Vec::new();
    let mut goods: Vec<Good> = Vec::new();
    let mut revs: Vec<Revoked> = Vec::new();
    for e in evidence {
        match store.finding(e, cert, sha, issuer, issuer_sha, available, now) {
            Finding::NotAbout => {}
            Finding::Unusable(n) => notes.push(n),
            Finding::Good {
                this,
                next,
                authority,
                responder,
                expired_since,
                responder_unchecked,
            } => {
                let expired =
                    this > info.not_after_unix && !expired_since.is_some_and(|t| t <= info.not_after_unix);
                if expired {
                    notes.push(RevocationNote::AfterExpiry);
                } else if this + CLOCK_SKEW < at {
                    notes.push(RevocationNote::IssuedBefore);
                } else if this + CLOCK_SKEW < exist {
                    notes.push(RevocationNote::NotAfterProof);
                } else {
                    goods.push(Good {
                        this,
                        next,
                        authority,
                        evidence: *e,
                        responder,
                        unchecked: responder_unchecked,
                    });
                }
            }
            Finding::Revoked {
                time,
                reason,
                this,
                next,
                authority,
                responder,
            } => revs.push(Revoked {
                time,
                reason,
                this,
                next,
                authority,
                evidence: *e,
                responder,
            }),
        }
    }
    let revoked = |entry: &mut CertRevocation, r: &Revoked, verdict: Check| {
        entry.state = RevocationState::Revoked {
            time: r.time,
            reason: r.reason,
        };
        entry.verdict = verdict;
        entry.source = Some(r.evidence.source);
        entry.kind = Some(r.evidence.kind);
        entry.authority = r.authority.clone();
        entry.this_update = Some(r.this);
        entry.next_update = r.next;
        Some((r.evidence, r.responder.clone()))
    };
    // A revocation at or before the time checked fails; a later one passes
    // only when the signature is proven to exist before it.
    let before = |entry: &mut CertRevocation| {
        if basis == TimeBasis::Now {
            entry.notes.push(RevocationNote::SigningTimeUnknown);
            Check::Warning
        } else {
            Check::Failed
        }
    };
    revs.sort_by_key(|r| r.time);
    if let Some(r) = revs.iter().find(|r| r.reason != Some(6)) {
        let verdict = if r.time <= at {
            before(&mut entry)
        } else if r.time > exist {
            Check::Passed
        } else {
            Check::Warning
        };
        let used = revoked(&mut entry, r, verdict);
        return (entry, used);
    }
    // Holds (certificateHold): released before the time checked when a
    // usable "not revoked" was issued after the hold and by that time.
    let mut hold_unknown = false;
    for h in revs.iter().filter(|r| r.reason == Some(6)) {
        if h.time <= at {
            let released = goods
                .iter()
                .any(|g| g.this >= h.time && g.this <= at + CLOCK_SKEW);
            if released {
                continue;
            }
            if h.this + CLOCK_SKEW >= at {
                // Still listed on hold after the time checked.
                let verdict = before(&mut entry);
                let used = revoked(&mut entry, h, verdict);
                return (entry, used);
            }
            hold_unknown = true;
        } else if h.time <= exist {
            // On hold after the claimed time but before the signature is
            // proven to exist: it may have been made during the hold.
            let used = revoked(&mut entry, h, Check::Warning);
            return (entry, used);
        }
    }
    if hold_unknown {
        entry.notes.push(RevocationNote::HoldUnknown);
        return (entry, None);
    }
    if let Some(g) = goods.iter().min_by_key(|g| g.this) {
        entry.state = RevocationState::Good;
        entry.verdict = if g.unchecked {
            Check::Warning
        } else {
            Check::Passed
        };
        if g.unchecked {
            entry.notes.push(RevocationNote::ResponderUnchecked);
        }
        entry.source = Some(g.evidence.source);
        entry.kind = Some(g.evidence.kind);
        entry.authority = g.authority.clone();
        entry.this_update = Some(g.this);
        entry.next_update = g.next;
        return (entry, Some((g.evidence, g.responder.clone())));
    }
    for n in notes {
        if !entry.notes.contains(&n) {
            entry.notes.push(n);
        }
    }
    if entry.notes.is_empty() {
        entry.notes.push(RevocationNote::NoData);
    }
    for f in failures.iter().filter(|f| f.cert == entry.cert_sha256) {
        let note = RevocationNote::FetchFailed {
            url: f.url.clone(),
            problem: f.problem.clone(),
        };
        if !entry.notes.contains(&note) {
            entry.notes.push(note);
        }
    }
    (entry, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn archive_attribute_round_trip() {
        let crl = vec![0x30, 0x03, 0x02, 0x01, 0x01];
        let ocsp = vec![0x30, 0x03, 0x0A, 0x01, 0x00];
        let a = RevocationArchive {
            crls: vec![crl.clone()],
            ocsps: vec![ocsp.clone()],
        };
        let value = a.attribute_value();
        assert_eq!(RevocationArchive::from_attribute(&value), a);
        assert!(RevocationArchive::from_attribute(b"junk").is_empty());
        // The CMS field: a list and an OCSP answer in the RFC 5940 form.
        let oid = RI_OCSP_RESPONSE.to_der().unwrap();
        let other = tlv(0xA1, &[&oid, &ocsp]);
        let field = [crl.clone(), other].concat();
        assert_eq!(RevocationArchive::from_signed_data_field(&field), a);
    }

    #[test]
    fn addresses_are_http_only() {
        let mut out = Vec::new();
        for u in [
            "ldap://x/y",
            "file:///etc/x",
            "http://a/b",
            "HTTPS://c/d",
            "http://a/b",
        ] {
            push_http(&mut out, u);
        }
        assert_eq!(out, ["http://a/b", "HTTPS://c/d"]);
    }

    const DAY: i64 = 86_400;
    const NOW: i64 = 1_792_324_800;

    fn who(name: &str) -> crate::digsig::IdentityInfo {
        crate::digsig::IdentityInfo {
            name: name.into(),
            ..Default::default()
        }
    }

    fn root(name: &str) -> crate::digsig::DigitalId {
        crate::digsig::DigitalId::create_with(
            &who(name),
            crate::digsig::KeyKind::EcP256,
            &crate::digsig::CertProfile {
                not_before: NOW - 100 * DAY,
                not_after: NOW + 100 * DAY,
                ca: true,
                ..Default::default()
            },
        )
        .unwrap()
    }

    fn leaf(ca: &crate::digsig::DigitalId) -> crate::digsig::DigitalId {
        ca.issue(
            &who("Unit Leaf"),
            crate::digsig::KeyKind::EcP256,
            &crate::digsig::CertProfile {
                not_before: NOW - 10 * DAY,
                not_after: NOW + 10 * DAY,
                ..Default::default()
            },
        )
        .unwrap()
    }

    #[test]
    fn a_revocation_without_a_signing_time_cannot_be_judged() {
        let ca = root("Unit Root");
        let s = leaf(&ca);
        let list = crl::make(
            &ca,
            &[crl::RevokedEntry::of(&s.certificate, NOW - DAY, Some(1))],
            NOW - 60,
            None,
            1,
        )
        .unwrap();
        let archive = RevocationArchive {
            crls: vec![list],
            ocsps: Vec::new(),
        };
        let mut store = EvidenceStore::default();
        let evidence = store.gather(&[(RevocationSource::Fetched, &archive)]);
        let trusted = [ca.certificate.clone()];
        let at = |basis, at| TimeFrame {
            at,
            basis,
            exist: NOW,
            now: NOW,
        };
        let now = check_chain(
            &s.certificate,
            &[],
            &trusted,
            &store,
            &evidence,
            &[],
            at(TimeBasis::Now, NOW),
        );
        assert_eq!(now.check, Check::Warning);
        assert!(now.entries[0].notes.contains(&RevocationNote::SigningTimeUnknown));
        let claimed = check_chain(
            &s.certificate,
            &[],
            &trusted,
            &store,
            &evidence,
            &[],
            at(TimeBasis::Claimed, NOW - 3600),
        );
        assert_eq!(claimed.check, Check::Failed);
    }

    #[test]
    fn the_walk_stops_at_a_trusted_root_before_a_cross_certificate() {
        use der::Encode;
        let r = root("Unit Trusted Root");
        let o = root("Unit Other Root");
        // The trusted root's name and key, issued by the other root.
        let tbs = x509_cert::certificate::TbsCertificate {
            version: x509_cert::certificate::Version::V3,
            serial_number: x509_cert::serial_number::SerialNumber::new(&[7]).unwrap(),
            signature: o.key.cert_signature_alg(),
            issuer: o.certificate.tbs_certificate.subject.clone(),
            validity: r.certificate.tbs_certificate.validity,
            subject: r.certificate.tbs_certificate.subject.clone(),
            subject_public_key_info: r.certificate.tbs_certificate.subject_public_key_info.clone(),
            issuer_unique_id: None,
            subject_unique_id: None,
            extensions: None,
        };
        let sig = o.key.sign(&tbs.to_der().unwrap()).unwrap();
        let cross = Certificate {
            tbs_certificate: tbs,
            signature_algorithm: o.key.cert_signature_alg(),
            signature: der::asn1::BitString::from_bytes(&sig).unwrap(),
        };
        assert!(issued_by(&cross, &o.certificate));
        let s = leaf(&r);
        let store = EvidenceStore::default();
        let frame = TimeFrame {
            at: NOW,
            basis: TimeBasis::TimeStamp,
            exist: NOW,
            now: NOW,
        };
        let c = check_chain(
            &s.certificate,
            &[cross, o.certificate.clone()],
            std::slice::from_ref(&r.certificate),
            &store,
            &[],
            &[],
            frame,
        );
        assert!(c.trusted);
        assert_eq!(c.entries.len(), 1, "only the signer is checked: {:?}", c.entries);
        assert_eq!(c.subjects[0].1.as_ref(), Some(&r.certificate));
    }

    #[test]
    fn overall_follows_the_entries() {
        let entry = |verdict| CertRevocation {
            subject: String::new(),
            serial: String::new(),
            cert_sha256: String::new(),
            state: RevocationState::Good,
            verdict,
            source: None,
            kind: None,
            authority: String::new(),
            this_update: None,
            next_update: None,
            addresses: Vec::new(),
            notes: Vec::new(),
        };
        assert_eq!(overall(&[]), Check::NotChecked);
        assert_eq!(overall(&[entry(Check::Passed)]), Check::Passed);
        assert_eq!(
            overall(&[entry(Check::Passed), entry(Check::Warning)]),
            Check::Warning
        );
        assert_eq!(
            overall(&[entry(Check::Warning), entry(Check::Failed)]),
            Check::Failed
        );
    }
}
