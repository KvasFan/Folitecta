//! Time stamps (RFC 3161): the request sent to a time stamp authority, the
//! checks of its answer, the checks of a time stamp token over some data, and
//! a small authority that answers requests with a digital ID (for tests and
//! for trying the time stamp settings without a server).
//!
//! The transport is the caller's: a function that sends the DER request and
//! returns the DER answer (an HTTP POST of `application/timestamp-query`).

use cms::content_info::ContentInfo;
use const_oid::ObjectIdentifier;
use der::asn1::{Any, BitString, GeneralizedTime, Int, OctetString};
use der::{Decode, Encode, Sequence};
use rand_core::{OsRng, RngCore};
use spki::AlgorithmIdentifierOwned;
use x509_cert::Certificate;
use x509_cert::ext::Extensions;

use super::cms_sig::{
    SignedContent, attribute_archive, certificates_of, digest_with, find_signer, name_of, sign_content,
    signed_data_of, trim_der, verify_with_cert,
};
use super::der_walk::{element, expect, generalized_time};
use super::id::{
    DigitalId, ID_SIGNED_DATA, KP_TIME_STAMPING, SHA256_OID, alg_null, cert_info, has_extended_usage,
};
use super::revocation::RevocationArchive;
use crate::error::{Error, Result};

/// Content type of the time stamp information signed by an authority.
pub(crate) const ID_CT_TST_INFO: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.9.16.1.4");
/// Unsigned attribute of a signer that holds the time stamp of its signature.
pub(crate) const ATTR_TIME_STAMP_TOKEN: ObjectIdentifier =
    ObjectIdentifier::new_unwrap("1.2.840.113549.1.9.16.2.14");
const ATTR_MESSAGE_DIGEST: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.9.4");
/// Policy of the stamps of the local authority, under the enterprise number
/// kept for documentation (RFC 5612).
const LOCAL_POLICY: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.3.6.1.4.1.32473.1");

/// Sends a time stamp request (DER) to the authority and returns its answer (DER).
pub type Transport<'a> = dyn FnMut(&[u8]) -> Result<Vec<u8>> + 'a;

fn bad(e: impl std::fmt::Display) -> Error {
    Error::Invalid(format!("invalid time stamp data: {e}"))
}

#[derive(Clone, Debug, Eq, PartialEq, Sequence)]
struct MessageImprint {
    hash_algorithm: AlgorithmIdentifierOwned,
    hashed_message: OctetString,
}

#[derive(Clone, Debug, Eq, PartialEq, Sequence)]
struct TimeStampReq {
    version: u8,
    message_imprint: MessageImprint,
    #[asn1(optional = "true")]
    req_policy: Option<ObjectIdentifier>,
    #[asn1(optional = "true")]
    nonce: Option<Int>,
    #[asn1(default = "Default::default")]
    cert_req: bool,
    #[asn1(context_specific = "0", tag_mode = "IMPLICIT", optional = "true")]
    extensions: Option<Extensions>,
}

#[derive(Clone, Debug, Eq, PartialEq, Sequence)]
struct PkiStatusInfo {
    status: u32,
    #[asn1(optional = "true")]
    status_string: Option<Vec<String>>,
    #[asn1(optional = "true")]
    fail_info: Option<BitString>,
}

#[derive(Clone, Debug, Eq, PartialEq, Sequence)]
struct TimeStampResp {
    status: PkiStatusInfo,
    #[asn1(optional = "true")]
    time_stamp_token: Option<Any>,
}

/// The time stamp information written by the local authority.
#[derive(Clone, Debug, Eq, PartialEq, Sequence)]
struct TstInfoOut {
    version: u8,
    policy: ObjectIdentifier,
    message_imprint: MessageImprint,
    serial_number: u64,
    gen_time: GeneralizedTime,
    #[asn1(optional = "true")]
    nonce: Option<Int>,
}

/// The parts of a time stamp information that are checked.
struct TstInfo {
    policy: ObjectIdentifier,
    imprint: MessageImprint,
    serial: Vec<u8>,
    time: i64,
    nonce: Option<Vec<u8>>,
}

/// Read a time stamp information, accepting fields Folitecta does not check.
fn parse_tst_info(data: &[u8]) -> Option<TstInfo> {
    let body = expect(data, 0x30)?.value;
    let version = expect(body, 0x02)?;
    if version.value != [1] {
        return None;
    }
    let policy = expect(version.rest, 0x06)?;
    let imprint = expect(policy.rest, 0x30)?;
    let serial = expect(imprint.rest, 0x02)?;
    let gen_time = expect(serial.rest, 0x18)?;
    // accuracy (SEQUENCE), ordering (BOOLEAN), nonce (INTEGER), tsa [0], extensions [1]
    let mut nonce = None;
    let mut rest = gen_time.rest;
    while let Some(e) = element(rest) {
        if e.tag == 0x02 {
            nonce = Some(e.value.to_vec());
        }
        rest = e.rest;
    }
    Some(TstInfo {
        policy: ObjectIdentifier::from_der(policy.whole).ok()?,
        imprint: MessageImprint::from_der(imprint.whole).ok()?,
        serial: serial.value.to_vec(),
        time: generalized_time(gen_time.value)?,
        nonce,
    })
}

fn hex(data: &[u8]) -> String {
    data.iter().map(|b| format!("{b:02X}")).collect()
}

/// Whether `cert` is meant for time stamps (its extended key usage lists
/// time stamping).
pub fn allows_time_stamping(cert: &Certificate) -> bool {
    has_extended_usage(cert, KP_TIME_STAMPING)
}

/// Result of checking a time stamp token.
#[derive(Debug, Clone, PartialEq)]
pub struct TimeStampCheck {
    /// Time of the stamp (Unix seconds, UTC).
    pub time: i64,
    /// The authority's certificate, when the token carries it.
    pub authority: Option<Certificate>,
    /// Every certificate carried by the token.
    pub certificates: Vec<Certificate>,
    /// The token is for the data checked (its message imprint matches).
    pub imprint_ok: bool,
    /// The token was signed by the authority's key and not changed since.
    pub signature_ok: bool,
    /// The authority's certificate is meant for time stamps.
    pub usage_ok: bool,
    /// Policy of the authority under which the stamp was given.
    pub policy: String,
    /// Serial number of the stamp (hexadecimal).
    pub serial: String,
    /// Digest algorithm of the stamped data.
    pub digest_algorithm: String,
    /// Problems found while checking.
    pub problems: Vec<String>,
    /// Revocation data carried by the token (the lists of its CMS structure
    /// and the revocation information attribute of its signer).
    pub revocation: RevocationArchive,
    nonce: Option<Vec<u8>>,
}

impl TimeStampCheck {
    /// A token that cannot be read at all.
    pub(crate) fn unreadable(problem: String) -> TimeStampCheck {
        TimeStampCheck {
            time: 0,
            authority: None,
            certificates: Vec::new(),
            imprint_ok: false,
            signature_ok: false,
            usage_ok: false,
            policy: String::new(),
            serial: String::new(),
            digest_algorithm: String::new(),
            problems: vec![problem],
            revocation: RevocationArchive::default(),
            nonce: None,
        }
    }

    /// The token is for the data and verifies with the authority's key.
    pub fn valid(&self) -> bool {
        self.imprint_ok && self.signature_ok
    }
}

/// Check a time stamp token (a CMS signed-data of a time stamp information)
/// over `parts` (concatenated).
pub fn check_token(token: &[u8], parts: &[&[u8]]) -> Result<TimeStampCheck> {
    check_with(token, &|oid| digest_with(oid, parts))
}

fn check_with(
    token: &[u8],
    digest_of: &dyn Fn(ObjectIdentifier) -> Option<Vec<u8>>,
) -> Result<TimeStampCheck> {
    let ci = ContentInfo::from_der(trim_der(token)).map_err(bad)?;
    if ci.content_type != ID_SIGNED_DATA {
        return Err(bad("the time stamp is not a signed-data structure"));
    }
    let (sd, field) = signed_data_of(&ci).map_err(bad)?;
    if sd.encap_content_info.econtent_type != ID_CT_TST_INFO {
        return Err(bad("the signature does not hold a time stamp"));
    }
    let tst_der = sd
        .encap_content_info
        .econtent
        .as_ref()
        .and_then(|e| e.decode_as::<OctetString>().ok())
        .ok_or_else(|| bad("the time stamp information is missing"))?;
    let tst =
        parse_tst_info(tst_der.as_bytes()).ok_or_else(|| bad("the time stamp information cannot be read"))?;
    let info = sd
        .signer_infos
        .0
        .iter()
        .next()
        .ok_or_else(|| bad("the time stamp has no signer"))?
        .clone();
    let certificates = certificates_of(&sd);
    let authority = find_signer(&sd, &info);
    let mut revocation = attribute_archive(&info);
    revocation.extend(&field);
    let mut problems = Vec::new();
    let imprint_ok = digest_of(tst.imprint.hash_algorithm.oid)
        .map(|d| d == tst.imprint.hashed_message.as_bytes())
        .unwrap_or(false);
    if !imprint_ok {
        problems.push("the time stamp was given for other data".into());
    }
    let digest_oid = info.digest_alg.oid;
    let content_digest = digest_with(digest_oid, &[tst_der.as_bytes()]);
    let (digest_ok, message) = match &info.signed_attrs {
        Some(attrs) => {
            let md = attrs
                .iter()
                .find(|a| a.oid == ATTR_MESSAGE_DIGEST)
                .and_then(|a| a.values.iter().next())
                .and_then(|v| v.decode_as::<OctetString>().ok());
            let ok = match (&md, &content_digest) {
                (Some(m), Some(d)) => m.as_bytes() == d.as_slice(),
                _ => false,
            };
            (ok, attrs.to_der().map_err(bad)?)
        }
        None => (true, tst_der.as_bytes().to_vec()),
    };
    let signature_ok = digest_ok
        && authority
            .as_ref()
            .map(|cert| {
                verify_with_cert(
                    cert,
                    &info.signature_algorithm,
                    digest_oid,
                    &message,
                    info.signature.as_bytes(),
                )
            })
            .unwrap_or(false);
    let mut usage_ok = false;
    match &authority {
        None => problems.push("the certificate of the time stamp authority is missing".into()),
        Some(cert) => {
            if !signature_ok {
                problems.push("the time stamp was changed or was not signed by its authority".into());
            }
            usage_ok = allows_time_stamping(cert);
            if !usage_ok {
                problems.push("the certificate of the authority is not meant for time stamps".into());
            }
            let ci = cert_info(cert);
            if tst.time < ci.not_before_unix || tst.time > ci.not_after_unix {
                problems
                    .push("the certificate of the authority was not valid at the time of the stamp".into());
            }
        }
    }
    Ok(TimeStampCheck {
        time: tst.time,
        authority,
        certificates,
        imprint_ok,
        signature_ok,
        usage_ok,
        policy: tst.policy.to_string(),
        serial: hex(&tst.serial),
        digest_algorithm: name_of(tst.imprint.hash_algorithm.oid),
        problems,
        revocation,
        nonce: tst.nonce,
    })
}

/// A request for a time stamp of some data.
#[derive(Debug, Clone)]
pub struct TimeStampRequest {
    /// The DER bytes to send to the authority.
    pub der: Vec<u8>,
    digest_oid: ObjectIdentifier,
    digest: Vec<u8>,
    nonce: Vec<u8>,
}

impl TimeStampRequest {
    /// A request for a SHA-256 time stamp of `parts` (concatenated), with a
    /// random nonce, asking for the authority's certificate in the token.
    pub fn new(parts: &[&[u8]]) -> Result<TimeStampRequest> {
        let digest = digest_with(SHA256_OID, parts).ok_or_else(|| bad("digest"))?;
        let mut nonce = [0u8; 8];
        OsRng.fill_bytes(&mut nonce);
        // Positive and eight bytes long, so its encoding is the bytes themselves.
        nonce[0] = (nonce[0] & 0x7F) | 0x01;
        let req = TimeStampReq {
            version: 1,
            message_imprint: MessageImprint {
                hash_algorithm: alg_null(SHA256_OID),
                hashed_message: OctetString::new(digest.clone()).map_err(bad)?,
            },
            req_policy: None,
            nonce: Some(Int::new(&nonce).map_err(bad)?),
            cert_req: true,
            extensions: None,
        };
        Ok(TimeStampRequest {
            der: req.to_der().map_err(bad)?,
            digest_oid: SHA256_OID,
            digest,
            nonce: nonce.to_vec(),
        })
    }

    /// The token of the authority's `response`, once it is checked to answer
    /// this request (status, data, nonce, signature of the authority).
    pub fn token(&self, response: &[u8]) -> Result<Vec<u8>> {
        let resp = TimeStampResp::from_der(trim_der(response))
            .map_err(|_| Error::Invalid("the time stamp server did not answer with a time stamp".into()))?;
        if resp.status.status > 1 {
            let text = resp
                .status
                .status_string
                .map(|s| s.join(" "))
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| format!("status {}", resp.status.status));
            return Err(Error::Invalid(format!(
                "the time stamp authority refused the request: {text}"
            )));
        }
        let token = resp
            .time_stamp_token
            .ok_or_else(|| Error::Invalid("the time stamp authority sent no time stamp".into()))?
            .to_der()
            .map_err(bad)?;
        let check = check_with(&token, &|oid| {
            (oid == self.digest_oid).then(|| self.digest.clone())
        })?;
        if !check.imprint_ok {
            return Err(Error::Invalid(
                "the time stamp authority answered for other data".into(),
            ));
        }
        if check.nonce.as_deref() != Some(self.nonce.as_slice()) {
            return Err(Error::Invalid(
                "the answer of the time stamp authority does not belong to this request".into(),
            ));
        }
        if !check.signature_ok {
            return Err(Error::Invalid(format!(
                "the time stamp does not verify: {}",
                check.problems.join("; ")
            )));
        }
        Ok(token)
    }
}

/// A checked time stamp token of `parts` (concatenated) from the authority
/// reached by `send`.
pub fn stamp(parts: &[&[u8]], send: &mut Transport<'_>) -> Result<Vec<u8>> {
    let request = TimeStampRequest::new(parts)?;
    let response = send(&request.der)?;
    request.token(&response)
}

fn rejection(text: &str) -> Result<Vec<u8>> {
    TimeStampResp {
        status: PkiStatusInfo {
            status: 2,
            status_string: Some(vec![text.to_string()]),
            fail_info: None,
        },
        time_stamp_token: None,
    }
    .to_der()
    .map_err(bad)
}

/// The answer of a local time stamp authority with the digital ID
/// `authority` to `request`: a stamp of time `time` (Unix seconds) with
/// serial number `serial`, or a refusal for requests it cannot read.
pub fn respond(authority: &DigitalId, request: &[u8], time: u64, serial: u64) -> Result<Vec<u8>> {
    respond_with(authority, request, time, serial, &RevocationArchive::default())
}

/// [`respond`] with revocation data in the token: the lists in its CMS
/// structure, the OCSP answers in the revocation information attribute of
/// its signer.
pub fn respond_with(
    authority: &DigitalId,
    request: &[u8],
    time: u64,
    serial: u64,
    revocation: &RevocationArchive,
) -> Result<Vec<u8>> {
    let Ok(req) = TimeStampReq::from_der(request) else {
        return rejection("the request cannot be read");
    };
    if req.version != 1 {
        return rejection("unsupported request version");
    }
    if digest_with(req.message_imprint.hash_algorithm.oid, &[]).is_none() {
        return rejection("unsupported digest algorithm");
    }
    let tst = TstInfoOut {
        version: 1,
        policy: req.req_policy.unwrap_or(LOCAL_POLICY),
        message_imprint: req.message_imprint,
        serial_number: serial,
        gen_time: GeneralizedTime::from_unix_duration(std::time::Duration::from_secs(time)).map_err(bad)?,
        nonce: req.nonce,
    }
    .to_der()
    .map_err(bad)?;
    let answers = RevocationArchive {
        crls: Vec::new(),
        ocsps: revocation.ocsps.clone(),
    };
    let token = sign_content(
        authority,
        SignedContent {
            content_type: ID_CT_TST_INFO,
            parts: &[&tst],
            embed: true,
            signing_time: None,
            certificates: req.cert_req,
            revocation: (!answers.is_empty()).then_some(&answers),
            crls: &revocation.crls,
        },
        None,
    )?;
    TimeStampResp {
        status: PkiStatusInfo {
            status: 0,
            status_string: None,
            fail_info: None,
        },
        time_stamp_token: Some(Any::from_der(&token).map_err(bad)?),
    }
    .to_der()
    .map_err(bad)
}
