//! CMS (PKCS#7) detached signatures: creation and verification.

use cms::cert::{CertificateChoices, IssuerAndSerialNumber};
use cms::content_info::{CmsVersion, ContentInfo};
use cms::signed_data::{
    CertificateSet, EncapsulatedContentInfo, SignedData, SignerIdentifier, SignerInfo, SignerInfos,
};
use const_oid::ObjectIdentifier;
use der::asn1::{Any, OctetString, SetOfVec, UtcTime};
use der::{Decode, Encode};
use sha1::Sha1;
use sha2::{Digest, Sha256, Sha384, Sha512};
use spki::AlgorithmIdentifierOwned;
use x509_cert::Certificate;
use x509_cert::attr::Attribute;

use super::id::{
    CURVE_P256, CURVE_P384, DigitalId, EC_PUBLIC_KEY, ID_DATA, ID_SIGNED_DATA, RSA_ENCRYPTION, SHA1_OID,
    SHA256_OID, SHA384_OID, SHA512_OID, alg_null,
};
use crate::error::{Error, Result};

const ATTR_CONTENT_TYPE: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.9.3");
const ATTR_MESSAGE_DIGEST: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.9.4");
const ATTR_SIGNING_TIME: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.9.5");
const ATTR_SIGNING_CERT_V2: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.9.16.2.47");
const RSA_PSS: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.1.10");

fn bad(e: impl std::fmt::Display) -> Error {
    Error::Invalid(format!("invalid signature data: {e}"))
}

/// Digest of `data` with the algorithm `oid`.
pub(crate) fn digest_with(oid: ObjectIdentifier, parts: &[&[u8]]) -> Option<Vec<u8>> {
    fn run<D: Digest>(parts: &[&[u8]]) -> Vec<u8> {
        let mut d = D::new();
        for p in parts {
            d.update(p);
        }
        d.finalize().to_vec()
    }
    Some(if oid == SHA256_OID {
        run::<Sha256>(parts)
    } else if oid == SHA1_OID {
        run::<Sha1>(parts)
    } else if oid == SHA384_OID {
        run::<Sha384>(parts)
    } else if oid == SHA512_OID {
        run::<Sha512>(parts)
    } else {
        return None;
    })
}

fn attribute(oid: ObjectIdentifier, value: Any) -> Result<Attribute> {
    Ok(Attribute {
        oid,
        values: SetOfVec::try_from(vec![value]).map_err(bad)?,
    })
}

/// `SigningCertificateV2` with the SHA-256 hash of the certificate (the hash
/// algorithm is the default and is therefore omitted).
fn signing_certificate_v2(cert_der: &[u8]) -> Result<Any> {
    let hash = OctetString::new(Sha256::digest(cert_der).to_vec()).map_err(bad)?;
    let hash_der = hash.to_der().map_err(bad)?;
    // ESSCertIDv2 ::= SEQUENCE { certHash }
    let mut ess = vec![0x30];
    push_len(&mut ess, hash_der.len());
    ess.extend(hash_der);
    // SEQUENCE OF ESSCertIDv2
    let mut certs = vec![0x30];
    push_len(&mut certs, ess.len());
    certs.extend(ess);
    // SigningCertificateV2 ::= SEQUENCE { certs }
    let mut out = vec![0x30];
    push_len(&mut out, certs.len());
    out.extend(certs);
    Any::from_der(&out).map_err(bad)
}

fn push_len(out: &mut Vec<u8>, len: usize) {
    if len < 0x80 {
        out.push(len as u8);
    } else {
        let bytes: Vec<u8> = len
            .to_be_bytes()
            .iter()
            .copied()
            .skip_while(|b| *b == 0)
            .collect();
        out.push(0x80 | bytes.len() as u8);
        out.extend(bytes);
    }
}

/// Detached CMS signature of `content_parts` (concatenated) by `id`.
pub fn sign_detached(id: &DigitalId, content_parts: &[&[u8]], signing_time_unix: u64) -> Result<Vec<u8>> {
    let digest_oid = id.key.digest_oid();
    let digest = digest_with(digest_oid, content_parts).ok_or_else(|| bad("digest"))?;
    let cert_der = id.certificate.to_der().map_err(bad)?;
    let time = UtcTime::from_unix_duration(std::time::Duration::from_secs(signing_time_unix)).map_err(bad)?;
    let attrs = vec![
        attribute(ATTR_CONTENT_TYPE, Any::encode_from(&ID_DATA).map_err(bad)?)?,
        attribute(ATTR_SIGNING_TIME, Any::encode_from(&time).map_err(bad)?)?,
        attribute(
            ATTR_MESSAGE_DIGEST,
            Any::encode_from(&OctetString::new(digest).map_err(bad)?).map_err(bad)?,
        )?,
        attribute(ATTR_SIGNING_CERT_V2, signing_certificate_v2(&cert_der)?)?,
    ];
    let signed_attrs = SetOfVec::try_from(attrs).map_err(bad)?;
    // The signature covers the DER of the attributes as a SET OF.
    let to_sign = signed_attrs.to_der().map_err(bad)?;
    let signature = id.key.sign(&to_sign)?;
    let tbs = &id.certificate.tbs_certificate;
    let signer = SignerInfo {
        version: CmsVersion::V1,
        sid: SignerIdentifier::IssuerAndSerialNumber(IssuerAndSerialNumber {
            issuer: tbs.issuer.clone(),
            serial_number: tbs.serial_number.clone(),
        }),
        digest_alg: alg_null(digest_oid),
        signed_attrs: Some(signed_attrs),
        signature_algorithm: id.key.cms_signature_alg(),
        signature: OctetString::new(signature).map_err(bad)?,
        unsigned_attrs: None,
    };
    let mut certs = vec![CertificateChoices::Certificate(id.certificate.clone())];
    certs.extend(id.chain.iter().cloned().map(CertificateChoices::Certificate));
    let sd = SignedData {
        version: CmsVersion::V1,
        digest_algorithms: SetOfVec::try_from(vec![alg_null(digest_oid)]).map_err(bad)?,
        encap_content_info: EncapsulatedContentInfo {
            econtent_type: ID_DATA,
            econtent: None,
        },
        certificates: Some(CertificateSet(SetOfVec::try_from(certs).map_err(bad)?)),
        crls: None,
        signer_infos: SignerInfos(SetOfVec::try_from(vec![signer]).map_err(bad)?),
    };
    let ci = ContentInfo {
        content_type: ID_SIGNED_DATA,
        content: Any::encode_from(&sd).map_err(bad)?,
    };
    ci.to_der().map_err(bad)
}

/// Result of checking a CMS signature against its content.
#[derive(Debug, Clone)]
pub struct CmsCheck {
    /// The signer's certificate, when it was found.
    pub signer: Option<Certificate>,
    /// Every certificate carried by the signature.
    pub certificates: Vec<Certificate>,
    /// The content digest matches the signed digest.
    pub digest_ok: bool,
    /// The signature value verifies with the signer's public key.
    pub signature_ok: bool,
    /// Signing time from the signed attributes (Unix seconds).
    pub signing_time: Option<i64>,
    /// Problems found while checking.
    pub problems: Vec<String>,
    /// Digest algorithm of the signer.
    pub digest_algorithm: String,
}

fn find_signer(sd: &SignedData, info: &SignerInfo) -> Option<Certificate> {
    let certs = sd.certificates.as_ref()?;
    certs.0.iter().find_map(|c| match c {
        CertificateChoices::Certificate(cert) => {
            let tbs = &cert.tbs_certificate;
            let matches = match &info.sid {
                SignerIdentifier::IssuerAndSerialNumber(ias) => {
                    ias.issuer == tbs.issuer && ias.serial_number == tbs.serial_number
                }
                SignerIdentifier::SubjectKeyIdentifier(ski) => tbs
                    .extensions
                    .as_ref()
                    .and_then(|exts| exts.iter().find(|e| e.extn_id.to_string() == "2.5.29.14"))
                    .and_then(|e| OctetString::from_der(e.extn_value.as_bytes()).ok())
                    .map(|v| v.as_bytes() == ski.0.as_bytes())
                    .unwrap_or(false),
            };
            matches.then(|| cert.clone())
        }
        _ => None,
    })
}

/// Verify `signature` (DER) made with the key of `cert` over `message`.
pub(crate) fn verify_with_cert(
    cert: &Certificate,
    sig_alg: &AlgorithmIdentifierOwned,
    digest_oid: ObjectIdentifier,
    message: &[u8],
    signature: &[u8],
) -> bool {
    use signature::Verifier;
    let spki = &cert.tbs_certificate.subject_public_key_info;
    let key_bytes = spki.subject_public_key.raw_bytes();
    if spki.algorithm.oid == RSA_ENCRYPTION {
        use rsa::pkcs1::DecodeRsaPublicKey;
        let Ok(public) = rsa::RsaPublicKey::from_pkcs1_der(key_bytes) else {
            return false;
        };
        if sig_alg.oid == RSA_PSS {
            let Some(digest) = digest_with(digest_oid, &[message]) else {
                return false;
            };
            return public
                .verify(rsa::Pss::new::<Sha256>(), &digest, signature)
                .is_ok()
                || public
                    .verify(rsa::Pss::new::<Sha384>(), &digest, signature)
                    .is_ok()
                || public
                    .verify(rsa::Pss::new::<Sha512>(), &digest, signature)
                    .is_ok();
        }
        let Ok(sig) = rsa::pkcs1v15::Signature::try_from(signature) else {
            return false;
        };
        return if digest_oid == SHA256_OID {
            rsa::pkcs1v15::VerifyingKey::<Sha256>::new(public)
                .verify(message, &sig)
                .is_ok()
        } else if digest_oid == SHA1_OID {
            rsa::pkcs1v15::VerifyingKey::<Sha1>::new(public)
                .verify(message, &sig)
                .is_ok()
        } else if digest_oid == SHA384_OID {
            rsa::pkcs1v15::VerifyingKey::<Sha384>::new(public)
                .verify(message, &sig)
                .is_ok()
        } else if digest_oid == SHA512_OID {
            rsa::pkcs1v15::VerifyingKey::<Sha512>::new(public)
                .verify(message, &sig)
                .is_ok()
        } else {
            false
        };
    }
    if spki.algorithm.oid == EC_PUBLIC_KEY {
        use signature::hazmat::PrehashVerifier;
        let curve = spki
            .algorithm
            .parameters
            .as_ref()
            .and_then(|p| p.decode_as::<ObjectIdentifier>().ok());
        let Some(digest) = digest_with(digest_oid, &[message]) else {
            return false;
        };
        if curve == Some(CURVE_P256) {
            let Ok(vk) = p256::ecdsa::VerifyingKey::from_sec1_bytes(key_bytes) else {
                return false;
            };
            let Ok(sig) = p256::ecdsa::Signature::from_der(signature) else {
                return false;
            };
            return vk.verify_prehash(&digest, &sig).is_ok();
        }
        if curve == Some(CURVE_P384) {
            let Ok(vk) = p384::ecdsa::VerifyingKey::from_sec1_bytes(key_bytes) else {
                return false;
            };
            let Ok(sig) = p384::ecdsa::Signature::from_der(signature) else {
                return false;
            };
            return vk.verify_prehash(&digest, &sig).is_ok();
        }
    }
    false
}

/// Digest algorithm implied by a certificate signature algorithm.
pub(crate) fn digest_of_sig_alg(oid: ObjectIdentifier) -> ObjectIdentifier {
    match oid.to_string().as_str() {
        "1.2.840.113549.1.1.5" | "1.2.840.10045.4.1" => SHA1_OID,
        "1.2.840.113549.1.1.12" | "1.2.840.10045.4.3.3" => SHA384_OID,
        "1.2.840.113549.1.1.13" | "1.2.840.10045.4.3.4" => SHA512_OID,
        _ => SHA256_OID,
    }
}

/// Whether `cert` was signed by the key of `issuer`.
pub fn issued_by(cert: &Certificate, issuer: &Certificate) -> bool {
    if cert.tbs_certificate.issuer != issuer.tbs_certificate.subject {
        return false;
    }
    let Ok(tbs) = cert.tbs_certificate.to_der() else {
        return false;
    };
    let digest = digest_of_sig_alg(cert.signature_algorithm.oid);
    verify_with_cert(
        issuer,
        &cert.signature_algorithm,
        digest,
        &tbs,
        cert.signature.raw_bytes(),
    )
}

fn name_of(oid: ObjectIdentifier) -> String {
    match oid.to_string().as_str() {
        "1.3.14.3.2.26" => "SHA-1".into(),
        "2.16.840.1.101.3.4.2.1" => "SHA-256".into(),
        "2.16.840.1.101.3.4.2.2" => "SHA-384".into(),
        "2.16.840.1.101.3.4.2.3" => "SHA-512".into(),
        other => other.to_string(),
    }
}

/// Check a detached CMS signature over `content_parts` (concatenated). For
/// signatures that embed a SHA-1 digest of the content (`adbe.pkcs7.sha1`),
/// the embedded digest is compared too.
pub fn verify_detached(signature: &[u8], content_parts: &[&[u8]]) -> Result<CmsCheck> {
    // The /Contents value is padded with zeros after the DER structure.
    let ci = ContentInfo::from_der(trim_der(signature)).map_err(bad)?;
    if ci.content_type != ID_SIGNED_DATA {
        return Err(bad("not a signed-data structure"));
    }
    let sd = ci.content.decode_as::<SignedData>().map_err(bad)?;
    let info = sd
        .signer_infos
        .0
        .iter()
        .next()
        .ok_or_else(|| bad("no signer"))?
        .clone();
    let certificates: Vec<Certificate> = sd
        .certificates
        .iter()
        .flat_map(|s| s.0.iter())
        .filter_map(|c| match c {
            CertificateChoices::Certificate(c) => Some(c.clone()),
            _ => None,
        })
        .collect();
    let signer = find_signer(&sd, &info);
    let digest_oid = info.digest_alg.oid;
    let mut problems = Vec::new();
    let Some(content_digest) = digest_with(digest_oid, content_parts) else {
        return Err(Error::Invalid(format!(
            "unsupported digest algorithm {digest_oid}"
        )));
    };
    // Content embedded in the signature (adbe.pkcs7.sha1): its own digest is signed.
    let embedded = sd
        .encap_content_info
        .econtent
        .as_ref()
        .and_then(|e| e.decode_as::<OctetString>().ok());
    let (signed_content_digest, embedded_ok) = match &embedded {
        Some(e) => {
            let sha1 = digest_with(SHA1_OID, content_parts).unwrap_or_default();
            let ok = e.as_bytes() == sha1.as_slice();
            if !ok {
                problems.push("the embedded digest does not match the document".into());
            }
            (digest_with(digest_oid, &[e.as_bytes()]).unwrap_or_default(), ok)
        }
        None => (content_digest.clone(), true),
    };
    let mut signing_time = None;
    let (digest_ok, message) = match &info.signed_attrs {
        Some(attrs) => {
            let md = attrs
                .iter()
                .find(|a| a.oid == ATTR_MESSAGE_DIGEST)
                .and_then(|a| a.values.iter().next())
                .and_then(|v| v.decode_as::<OctetString>().ok());
            if let Some(t) = attrs
                .iter()
                .find(|a| a.oid == ATTR_SIGNING_TIME)
                .and_then(|a| a.values.iter().next())
            {
                if let Ok(u) = t.decode_as::<UtcTime>() {
                    signing_time = Some(u.to_unix_duration().as_secs() as i64);
                } else if let Ok(g) = t.decode_as::<der::asn1::GeneralizedTime>() {
                    signing_time = Some(g.to_unix_duration().as_secs() as i64);
                }
            }
            let ok = md
                .map(|m| m.as_bytes() == signed_content_digest.as_slice())
                .unwrap_or(false);
            (ok && embedded_ok, attrs.to_der().map_err(bad)?)
        }
        None => (embedded_ok, Vec::new()),
    };
    if !digest_ok {
        problems.push("the document was changed after it was signed".into());
    }
    let signature_ok = match &signer {
        Some(cert) => {
            if info.signed_attrs.is_some() {
                verify_with_cert(
                    cert,
                    &info.signature_algorithm,
                    digest_oid,
                    &message,
                    info.signature.as_bytes(),
                )
            } else {
                // Without signed attributes the signature covers the content itself.
                let content: Vec<u8> = content_parts.concat();
                verify_with_cert(
                    cert,
                    &info.signature_algorithm,
                    digest_oid,
                    &content,
                    info.signature.as_bytes(),
                )
            }
        }
        None => {
            problems.push("the signer's certificate is missing".into());
            false
        }
    };
    if signer.is_some() && !signature_ok {
        problems.push("the signature value does not verify".into());
    }
    Ok(CmsCheck {
        signer,
        certificates,
        digest_ok,
        signature_ok,
        signing_time,
        problems,
        digest_algorithm: name_of(digest_oid),
    })
}

/// The DER structure at the start of `data` (without trailing padding).
fn trim_der(data: &[u8]) -> &[u8] {
    if data.len() < 2 || data[0] != 0x30 {
        return data;
    }
    let (len, header) = if data[1] < 0x80 {
        (data[1] as usize, 2)
    } else {
        let n = (data[1] & 0x7F) as usize;
        if n == 0 || n > 4 || data.len() < 2 + n {
            return data;
        }
        let len = data[2..2 + n]
            .iter()
            .fold(0usize, |acc, b| (acc << 8) | *b as usize);
        (len, 2 + n)
    };
    &data[..(header + len).min(data.len())]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trailing_padding_is_ignored() {
        let mut v = vec![0x30, 0x03, 0x02, 0x01, 0x05];
        v.extend([0u8; 10]);
        assert_eq!(trim_der(&v), &[0x30, 0x03, 0x02, 0x01, 0x05]);
    }
}
