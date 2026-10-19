//! Certificate revocation lists (RFC 5280 5): the checks of a list for a
//! certificate, and a small issuer of lists for the tests and the self-test.
//!
//! Complete lists of the certificate's own issuer are read; delta lists,
//! indirect lists (entries of other issuers) and lists partitioned by reasons
//! are reported as of a kind Folitecta does not read.

use const_oid::ObjectIdentifier;
use der::asn1::OctetString;
use der::{Decode, Encode};
use x509_cert::Certificate;
use x509_cert::crl::{CertificateList, RevokedCert, TbsCertList};
use x509_cert::ext::Extension;
use x509_cert::ext::pkix::name::{DistributionPointName, GeneralName};
use x509_cert::serial_number::SerialNumber;

use super::cms_sig::{digest_of_sig_alg, verify_with_cert};
use super::der_walk::signed_part;
use super::id::{DigitalId, cert_info, is_ca, key_hash, may_sign_crls, subject_key_id, x509_time};
use super::revocation::{CLOCK_SKEW, Finding, RevocationNote, crl_urls};
use crate::error::{Error, Result};

const CRL_NUMBER: ObjectIdentifier = ObjectIdentifier::new_unwrap("2.5.29.20");
const REASON_CODE: ObjectIdentifier = ObjectIdentifier::new_unwrap("2.5.29.21");
const INVALIDITY_DATE: ObjectIdentifier = ObjectIdentifier::new_unwrap("2.5.29.24");
const DELTA_INDICATOR: ObjectIdentifier = ObjectIdentifier::new_unwrap("2.5.29.27");
/// Issuing distribution point (matched by value: the type of the library
/// names another identifier).
const ISSUING_DP: ObjectIdentifier = ObjectIdentifier::new_unwrap("2.5.29.28");
const AUTHORITY_KEY_ID: ObjectIdentifier = ObjectIdentifier::new_unwrap("2.5.29.35");
const FRESHEST_CRL: ObjectIdentifier = ObjectIdentifier::new_unwrap("2.5.29.46");
/// The list keeps the revoked certificates that expired since this date
/// (X.509 ExpiredCertsOnCRL).
const EXPIRED_CERTS_ON_CRL: ObjectIdentifier = ObjectIdentifier::new_unwrap("2.5.29.60");

fn bad(e: impl std::fmt::Display) -> Error {
    Error::Invalid(format!("invalid revocation list data: {e}"))
}

/// A certificate on a list made by [`make`].
#[derive(Debug, Clone, PartialEq)]
pub struct RevokedEntry {
    pub serial: SerialNumber,
    /// Revocation date (Unix seconds).
    pub time: i64,
    /// Reason code (RFC 5280 5.3.1).
    pub reason: Option<u8>,
    /// Date the key is known or suspected to be compromised.
    pub invalidity: Option<i64>,
    /// Other entry extensions.
    pub extensions: Vec<Extension>,
}

impl RevokedEntry {
    /// `cert` revoked at `time` for `reason`.
    pub fn of(cert: &Certificate, time: i64, reason: Option<u8>) -> RevokedEntry {
        RevokedEntry {
            serial: cert.tbs_certificate.serial_number.clone(),
            time,
            reason,
            invalidity: None,
            extensions: Vec::new(),
        }
    }
}

/// A complete (v2) revocation list of `issuer` listing `revoked`, with its
/// CRL number and authority key identifier, signed with the issuer's key.
pub fn make(
    issuer: &DigitalId,
    revoked: &[RevokedEntry],
    this_update: i64,
    next_update: Option<i64>,
    number: u64,
) -> Result<Vec<u8>> {
    make_with(issuer, revoked, this_update, next_update, number, Vec::new())
}

/// [`make`] with more list extensions.
pub fn make_with(
    issuer: &DigitalId,
    revoked: &[RevokedEntry],
    this_update: i64,
    next_update: Option<i64>,
    number: u64,
    extra: Vec<Extension>,
) -> Result<Vec<u8>> {
    let mut entries = Vec::new();
    for r in revoked {
        let mut exts = r.extensions.clone();
        if let Some(reason) = r.reason {
            exts.push(Extension {
                extn_id: REASON_CODE,
                critical: false,
                extn_value: OctetString::new(vec![0x0A, 0x01, reason]).map_err(bad)?,
            });
        }
        if let Some(t) = r.invalidity {
            exts.push(Extension {
                extn_id: INVALIDITY_DATE,
                critical: false,
                extn_value: OctetString::new(super::der_walk::generalized(t)).map_err(bad)?,
            });
        }
        entries.push(RevokedCert {
            serial_number: r.serial.clone(),
            revocation_date: x509_time(r.time)?,
            crl_entry_extensions: (!exts.is_empty()).then_some(exts),
        });
    }
    let aki = x509_cert::ext::pkix::AuthorityKeyIdentifier {
        key_identifier: Some(OctetString::new(key_hash(&issuer.certificate)).map_err(bad)?),
        authority_cert_issuer: None,
        authority_cert_serial_number: None,
    };
    let mut exts = vec![
        Extension {
            extn_id: CRL_NUMBER,
            critical: false,
            extn_value: OctetString::new(
                der::asn1::Uint::new(&number.to_be_bytes())
                    .map_err(bad)?
                    .to_der()
                    .map_err(bad)?,
            )
            .map_err(bad)?,
        },
        Extension {
            extn_id: AUTHORITY_KEY_ID,
            critical: false,
            extn_value: OctetString::new(aki.to_der().map_err(bad)?).map_err(bad)?,
        },
    ];
    exts.extend(extra);
    let alg = issuer.key.cert_signature_alg();
    let tbs = TbsCertList {
        version: x509_cert::Version::V2,
        signature: alg.clone(),
        issuer: issuer.certificate.tbs_certificate.subject.clone(),
        this_update: x509_time(this_update)?,
        next_update: next_update.map(x509_time).transpose()?,
        revoked_certificates: (!entries.is_empty()).then_some(entries),
        crl_extensions: Some(exts),
    };
    let signature = issuer.key.sign(&tbs.to_der().map_err(bad)?)?;
    CertificateList {
        tbs_cert_list: tbs,
        signature_algorithm: alg,
        signature: der::asn1::BitString::from_bytes(&signature).map_err(bad)?,
    }
    .to_der()
    .map_err(bad)
}

/// The DER of a list given as DER or PEM (`-----BEGIN X509 CRL-----`).
pub(crate) fn der_of(data: &[u8]) -> Option<Vec<u8>> {
    if data.first() == Some(&0x30) {
        return Some(super::cms_sig::trim_der(data).to_vec());
    }
    let text = std::str::from_utf8(data).ok()?;
    let start = text.find("-----BEGIN X509 CRL-----")? + 24;
    let end = start + text[start..].find("-----END X509 CRL-----")?;
    let b64: String = text[start..end].chars().filter(|c| !c.is_whitespace()).collect();
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.decode(b64).ok()
}

/// A revocation list read for checking.
#[derive(Debug, Clone)]
pub(crate) struct ParsedCrl {
    /// The list as received (DER).
    pub(crate) der: Vec<u8>,
    list: CertificateList,
    tbs: Vec<u8>,
}

/// Read a revocation list (DER or PEM). Lists without a version (v1) cannot
/// be read by the certificate library and are unreadable.
pub(crate) fn parse(data: &[u8]) -> std::result::Result<ParsedCrl, RevocationNote> {
    let der = der_of(data).ok_or(RevocationNote::Unreadable)?;
    let list = CertificateList::from_der(&der).map_err(|_| RevocationNote::Unreadable)?;
    let tbs = signed_part(&der).ok_or(RevocationNote::Unreadable)?.to_vec();
    Ok(ParsedCrl { der, list, tbs })
}

fn time(t: &x509_cert::time::Time) -> i64 {
    t.to_unix_duration().as_secs() as i64
}

/// URIs among general names.
fn uris(names: &[GeneralName]) -> Vec<String> {
    names
        .iter()
        .filter_map(|n| match n {
            GeneralName::UniformResourceIdentifier(u) => Some(u.to_string()),
            _ => None,
        })
        .collect()
}

impl ParsedCrl {
    /// What this list says about `cert`, issued by `issuer`, checked at
    /// `now` (a part of RFC 5280 6.3.3).
    pub(crate) fn finding(&self, cert: &Certificate, issuer: &Certificate, now: i64) -> Finding {
        let tbs = &self.list.tbs_cert_list;
        if tbs.issuer != cert.tbs_certificate.issuer {
            return Finding::NotAbout;
        }
        if tbs.signature != self.list.signature_algorithm {
            return Finding::Unusable(RevocationNote::BadSignature);
        }
        let mut expired_since = None;
        for ext in tbs.crl_extensions.iter().flatten() {
            let value = ext.extn_value.as_bytes();
            if ext.extn_id == DELTA_INDICATOR {
                return Finding::Unusable(RevocationNote::UnsupportedList);
            } else if ext.extn_id == ISSUING_DP {
                let Ok(idp) = x509_cert::ext::pkix::crl::dp::IssuingDistributionPoint::from_der(value) else {
                    return Finding::Unusable(RevocationNote::Unreadable);
                };
                if idp.indirect_crl || idp.only_some_reasons.is_some() {
                    return Finding::Unusable(RevocationNote::UnsupportedList);
                }
                if idp.only_contains_attribute_certs
                    || (idp.only_contains_ca_certs && !is_ca(cert))
                    || (idp.only_contains_user_certs && is_ca(cert))
                {
                    return Finding::NotAbout;
                }
                if let Some(DistributionPointName::FullName(names)) = &idp.distribution_point {
                    let listed = uris(names);
                    let own = crl_urls(cert);
                    if !listed.is_empty() && !own.is_empty() && !listed.iter().any(|u| own.contains(u)) {
                        return Finding::NotAbout;
                    }
                }
            } else if ext.extn_id == AUTHORITY_KEY_ID {
                let id = x509_cert::ext::pkix::AuthorityKeyIdentifier::from_der(value)
                    .ok()
                    .and_then(|a| a.key_identifier)
                    .map(|k| k.as_bytes().to_vec());
                let own = subject_key_id(issuer).unwrap_or_else(|| key_hash(issuer));
                if id.is_some_and(|id| id != own && id != key_hash(issuer)) {
                    // A list of another key of the same authority.
                    return Finding::NotAbout;
                }
            } else if ext.extn_id == EXPIRED_CERTS_ON_CRL {
                expired_since = der::asn1::GeneralizedTime::from_der(value)
                    .ok()
                    .map(|t| t.to_unix_duration().as_secs() as i64);
            } else if ext.extn_id == CRL_NUMBER || ext.extn_id == FRESHEST_CRL {
                // Read by nobody here: the list is complete either way.
            } else if ext.critical {
                return Finding::Unusable(RevocationNote::UnsupportedList);
            }
        }
        if !may_sign_crls(issuer) {
            return Finding::Unusable(RevocationNote::NotAuthorized);
        }
        let digest = digest_of_sig_alg(self.list.signature_algorithm.oid);
        if !verify_with_cert(
            issuer,
            &self.list.signature_algorithm,
            digest,
            &self.tbs,
            self.list.signature.raw_bytes(),
        ) {
            return Finding::Unusable(RevocationNote::BadSignature);
        }
        let this = time(&tbs.this_update);
        let next = tbs.next_update.as_ref().map(time);
        if next.is_some_and(|n| n < this) {
            return Finding::Unusable(RevocationNote::Unreadable);
        }
        if this > now + CLOCK_SKEW {
            return Finding::Unusable(RevocationNote::FromFuture);
        }
        let authority = cert_info(issuer).common_name;
        let serial = &cert.tbs_certificate.serial_number;
        for entry in tbs.revoked_certificates.iter().flatten() {
            if entry.serial_number != *serial {
                continue;
            }
            let mut reason = None;
            let mut invalidity = None;
            for ext in entry.crl_entry_extensions.iter().flatten() {
                let value = ext.extn_value.as_bytes();
                if ext.extn_id == REASON_CODE {
                    reason = value.last().copied().filter(|_| value.first() == Some(&0x0A));
                } else if ext.extn_id == INVALIDITY_DATE {
                    invalidity = der::asn1::GeneralizedTime::from_der(value)
                        .ok()
                        .map(|t| t.to_unix_duration().as_secs() as i64);
                } else if ext.critical {
                    // For example the certificate issuer of an indirect list.
                    return Finding::Unusable(RevocationNote::UnsupportedList);
                }
            }
            if reason == Some(8) {
                // Removed from the list: not revoked (delta lists only).
                break;
            }
            let revoked = time(&entry.revocation_date);
            return Finding::Revoked {
                time: invalidity.map_or(revoked, |i| i.min(revoked)),
                reason,
                this,
                next,
                authority,
                responder: None,
            };
        }
        Finding::Good {
            this,
            next,
            authority,
            responder: None,
            expired_since,
            responder_unchecked: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::digsig::{CertProfile, IdentityInfo, KeyKind};

    const SIGN: i64 = 1_792_324_800;
    const DAY: i64 = 86_400;

    fn who(name: &str) -> IdentityInfo {
        IdentityInfo {
            name: name.into(),
            ..Default::default()
        }
    }

    fn authority(ca: bool) -> DigitalId {
        DigitalId::create_with(
            &who("Unit List CA"),
            KeyKind::EcP256,
            &CertProfile {
                not_before: SIGN - 400 * DAY,
                not_after: SIGN + 3650 * DAY,
                ca,
                ..Default::default()
            },
        )
        .unwrap()
    }

    fn issued(ca: &DigitalId, name: &str) -> DigitalId {
        ca.issue(
            &who(name),
            KeyKind::EcP256,
            &CertProfile {
                not_before: SIGN - 30 * DAY,
                not_after: SIGN + 365 * DAY,
                crl_url: Some("http://crl.example.org/ca.crl".into()),
                ..Default::default()
            },
        )
        .unwrap()
    }

    fn ext(oid: &str, critical: bool, value: Vec<u8>) -> Extension {
        Extension {
            extn_id: ObjectIdentifier::new_unwrap(oid),
            critical,
            extn_value: OctetString::new(value).unwrap(),
        }
    }

    fn check(der: &[u8], cert: &Certificate, ca: &DigitalId) -> Finding {
        parse(der).unwrap().finding(cert, &ca.certificate, SIGN + DAY)
    }

    #[test]
    fn listed_and_unlisted_certificates() {
        let ca = authority(true);
        let a = issued(&ca, "Listed");
        let b = issued(&ca, "Not Listed");
        let c = issued(&ca, "Removed");
        let mut entry = RevokedEntry::of(&a.certificate, SIGN, Some(1));
        entry.invalidity = Some(SIGN - 3600);
        let removed = RevokedEntry::of(&c.certificate, SIGN - 60, Some(8));
        let der = make(&ca, &[entry, removed], SIGN + 60, Some(SIGN + 7 * DAY), 2).unwrap();
        match check(&der, &a.certificate, &ca) {
            Finding::Revoked { time, reason, .. } => assert_eq!((time, reason), (SIGN - 3600, Some(1))),
            f => panic!("{f:?}"),
        }
        assert!(matches!(
            check(&der, &b.certificate, &ca),
            Finding::Good { this, .. } if this == SIGN + 60
        ));
        assert!(matches!(check(&der, &c.certificate, &ca), Finding::Good { .. }));
        // A list of another issuer says nothing about the certificate.
        let other = DigitalId::create_with(
            &who("Other CA"),
            KeyKind::EcP256,
            &CertProfile {
                not_before: SIGN - DAY,
                not_after: SIGN + DAY,
                ca: true,
                ..Default::default()
            },
        )
        .unwrap();
        let foreign = make(&other, &[], SIGN, None, 1).unwrap();
        assert!(matches!(check(&foreign, &a.certificate, &ca), Finding::NotAbout));
        // PEM is read too.
        use base64::Engine;
        let pem = format!(
            "-----BEGIN X509 CRL-----\n{}\n-----END X509 CRL-----\n",
            base64::engine::general_purpose::STANDARD.encode(&der)
        );
        assert_eq!(der_of(pem.as_bytes()).unwrap(), der);
    }

    #[test]
    fn delta_indirect_and_unknown_critical_lists_are_not_used() {
        let ca = authority(true);
        let a = issued(&ca, "Subject");
        let unsupported = |extra: Vec<Extension>| {
            let der = make_with(&ca, &[], SIGN, None, 1, extra).unwrap();
            check(&der, &a.certificate, &ca)
        };
        // Delta list indicator.
        assert!(matches!(
            unsupported(vec![ext("2.5.29.27", true, vec![0x02, 0x01, 0x01])]),
            Finding::Unusable(RevocationNote::UnsupportedList)
        ));
        // Issuing distribution point: indirect list ([4] TRUE).
        assert!(matches!(
            unsupported(vec![ext("2.5.29.28", true, vec![0x30, 0x03, 0x84, 0x01, 0xFF])]),
            Finding::Unusable(RevocationNote::UnsupportedList)
        ));
        // Only some reasons ([3] BIT STRING).
        assert!(matches!(
            unsupported(vec![ext(
                "2.5.29.28",
                true,
                vec![0x30, 0x04, 0x83, 0x02, 0x07, 0x80]
            )]),
            Finding::Unusable(RevocationNote::UnsupportedList)
        ));
        // An unknown critical extension.
        assert!(matches!(
            unsupported(vec![ext("1.2.3.4", true, vec![0x05, 0x00])]),
            Finding::Unusable(RevocationNote::UnsupportedList)
        ));
        // Only certificates of authorities: not about an end entity.
        assert!(matches!(
            unsupported(vec![ext("2.5.29.28", true, vec![0x30, 0x03, 0x82, 0x01, 0xFF])]),
            Finding::NotAbout
        ));
        // An entry with the certificate issuer of an indirect list.
        let mut entry = RevokedEntry::of(&a.certificate, SIGN - 60, None);
        entry.extensions.push(ext("2.5.29.29", true, vec![0x30, 0x00]));
        let der = make(&ca, &[entry], SIGN, None, 1).unwrap();
        assert!(matches!(
            check(&der, &a.certificate, &ca),
            Finding::Unusable(RevocationNote::UnsupportedList)
        ));
        // An issuer whose key usage does not allow lists.
        let plain = authority(false);
        let b = plain
            .issue(
                &who("Subject Of A Plain Issuer"),
                KeyKind::EcP256,
                &CertProfile {
                    not_before: SIGN - DAY,
                    not_after: SIGN + DAY,
                    ..Default::default()
                },
            )
            .unwrap();
        let der = make(&plain, &[], SIGN, None, 1).unwrap();
        assert!(matches!(
            check(&der, &b.certificate, &plain),
            Finding::Unusable(RevocationNote::NotAuthorized)
        ));
    }
}
