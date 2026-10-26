//! Digital IDs whose private key stays where it is kept — a smart card or
//! token (PKCS #11), the certificate store of the system — and is only
//! asked to sign. Folitecta never reads such a key and never stores the PIN
//! that unlocks it; every signature it makes is checked against the
//! certificate before it is used, so a device that signs with another key
//! is caught.

use std::sync::Arc;

use const_oid::ObjectIdentifier;
use der::Encode;
use x509_cert::Certificate;

use super::id::{
    CURVE_P256, CURVE_P384, DigitalId, EC_PUBLIC_KEY, IdKey, RSA_ENCRYPTION, SHA256_OID, SHA384_OID,
};
use crate::error::{Error, Result};

/// The kind of a key kept outside, as its certificate gives it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExternalKind {
    /// RSA, signed as PKCS #1 v1.5 with SHA-256.
    Rsa,
    /// ECDSA on P-256 with SHA-256.
    EcP256,
    /// ECDSA on P-384 with SHA-384.
    EcP384,
}

impl ExternalKind {
    /// The kind of the key of `cert`, when Folitecta can sign with it.
    pub fn of(cert: &Certificate) -> Option<ExternalKind> {
        let spki = &cert.tbs_certificate.subject_public_key_info;
        if spki.algorithm.oid == RSA_ENCRYPTION {
            return Some(ExternalKind::Rsa);
        }
        if spki.algorithm.oid != EC_PUBLIC_KEY {
            return None;
        }
        let curve = spki
            .algorithm
            .parameters
            .as_ref()
            .and_then(|p| p.decode_as::<ObjectIdentifier>().ok());
        match curve {
            Some(c) if c == CURVE_P256 => Some(ExternalKind::EcP256),
            Some(c) if c == CURVE_P384 => Some(ExternalKind::EcP384),
            _ => None,
        }
    }

    /// The digest signed with the key.
    pub(crate) fn digest_oid(self) -> ObjectIdentifier {
        match self {
            ExternalKind::EcP384 => SHA384_OID,
            _ => SHA256_OID,
        }
    }

    /// The length of that digest in bytes.
    pub fn digest_len(self) -> usize {
        match self {
            ExternalKind::EcP384 => 48,
            _ => 32,
        }
    }
}

/// A private key kept outside Folitecta.
pub trait ExternalKey: Send + Sync {
    /// Sign `digest`, the hash of the signed data (SHA-256, or SHA-384 for
    /// P-384 keys): for RSA the PKCS #1 v1.5 signature of that hash, for
    /// ECDSA the signature as DER (see [`ecdsa_der`] for devices that give
    /// the two numbers side by side).
    fn sign_digest(&self, digest: &[u8]) -> Result<Vec<u8>>;
}

/// The DER `DigestInfo` of `digest` (SHA-256, or SHA-384 when 48 bytes
/// long): what an RSA key signs for PKCS #1 v1.5 when the device takes the
/// encoded value itself (PKCS #11 `CKM_RSA_PKCS`).
pub fn rsa_digest_info(digest: &[u8]) -> Vec<u8> {
    let prefix: &[u8] = if digest.len() == 48 {
        &[
            0x30, 0x41, 0x30, 0x0d, 0x06, 0x09, 0x60, 0x86, 0x48, 0x01, 0x65, 0x03, 0x04, 0x02, 0x02, 0x05,
            0x00, 0x04, 0x30,
        ]
    } else {
        &[
            0x30, 0x31, 0x30, 0x0d, 0x06, 0x09, 0x60, 0x86, 0x48, 0x01, 0x65, 0x03, 0x04, 0x02, 0x01, 0x05,
            0x00, 0x04, 0x20,
        ]
    };
    [prefix, digest].concat()
}

/// An ECDSA signature given as its two numbers side by side (`r ‖ s`, as
/// PKCS #11 and some system stores give it), written as DER.
pub fn ecdsa_der(raw: &[u8]) -> Result<Vec<u8>> {
    let bad = || Error::Invalid("the device gave a signature of an unexpected size".into());
    match raw.len() {
        64 => Ok(p256::ecdsa::Signature::from_slice(raw)
            .map_err(|_| bad())?
            .to_der()
            .as_bytes()
            .to_vec()),
        96 => Ok(p384::ecdsa::Signature::from_slice(raw)
            .map_err(|_| bad())?
            .to_der()
            .as_bytes()
            .to_vec()),
        _ => Err(bad()),
    }
}

/// The external key of an ID with its kind.
#[derive(Clone)]
pub(crate) struct Held {
    pub kind: ExternalKind,
    pub key: Arc<dyn ExternalKey>,
}

impl DigitalId {
    /// A digital ID of `certificate` (with the certificates of its issuers
    /// in `chain`) whose private key is `key`, kept outside.
    pub fn from_external(
        certificate: Certificate,
        chain: Vec<Certificate>,
        key: Arc<dyn ExternalKey>,
    ) -> Result<DigitalId> {
        let kind = ExternalKind::of(&certificate).ok_or_else(|| {
            Error::Invalid("the key of the certificate is not supported (RSA, ECDSA P-256 or P-384)".into())
        })?;
        Ok(DigitalId {
            certificate,
            chain,
            key: IdKey::External(Held { kind, key }),
        })
    }

    /// Whether the private key stays outside (a device or the system).
    pub fn is_external(&self) -> bool {
        matches!(self.key, IdKey::External(_))
    }

    /// The DER of the certificate (for callers that keep the ID's
    /// certificate to find its key again).
    pub fn certificate_der(&self) -> Vec<u8> {
        self.certificate.to_der().unwrap_or_default()
    }
}

/// The key of an ID made here, used as if it were kept outside: for tests
/// and the stand-in card of the self-test.
pub struct SoftKey(DigitalId);

impl SoftKey {
    pub fn new(id: DigitalId) -> SoftKey {
        SoftKey(id)
    }
}

impl ExternalKey for SoftKey {
    fn sign_digest(&self, digest: &[u8]) -> Result<Vec<u8>> {
        use signature::hazmat::PrehashSigner;
        let failed = |e: signature::Error| Error::Invalid(e.to_string());
        match &self.0.key {
            IdKey::Rsa(k) => {
                let scheme = if digest.len() == 48 {
                    rsa::Pkcs1v15Sign::new::<sha2::Sha384>()
                } else {
                    rsa::Pkcs1v15Sign::new::<sha2::Sha256>()
                };
                k.sign(scheme, digest).map_err(|e| Error::Invalid(e.to_string()))
            }
            // Given side by side, as devices do.
            IdKey::P256(k) => {
                let sig: p256::ecdsa::Signature = k.sign_prehash(digest).map_err(failed)?;
                ecdsa_der(&sig.to_bytes())
            }
            IdKey::P384(k) => {
                let sig: p384::ecdsa::Signature = k.sign_prehash(digest).map_err(failed)?;
                ecdsa_der(&sig.to_bytes())
            }
            IdKey::External(h) => h.key.sign_digest(digest),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::digsig::{IdentityInfo, KeyKind, cms_sig};

    fn id(kind: KeyKind, name: &str) -> DigitalId {
        let info = IdentityInfo {
            name: name.into(),
            ..IdentityInfo::default()
        };
        DigitalId::create_self_signed(&info, kind, 1).unwrap()
    }

    #[test]
    fn a_key_outside_signs_and_is_checked() {
        for kind in [KeyKind::Rsa2048, KeyKind::EcP256] {
            let soft = id(kind, "Card");
            let outside = DigitalId::from_external(
                soft.certificate.clone(),
                vec![],
                Arc::new(SoftKey::new(soft.clone())),
            )
            .unwrap();
            assert!(outside.is_external() && !soft.is_external());
            let sig = cms_sig::sign_detached(&outside, &[b"the document"], 1_800_000_000).unwrap();
            let check = cms_sig::verify_detached(&sig, &[b"the document"]).unwrap();
            assert!(check.digest_ok && check.signature_ok, "{kind:?}");
            // The key never leaves: no PKCS #12 of it.
            assert!(outside.to_pkcs12("secret1").is_err());
        }
    }

    #[test]
    fn a_device_signing_with_another_key_is_caught() {
        let card = id(KeyKind::EcP256, "Card");
        let other = id(KeyKind::EcP256, "Other");
        let wrong = DigitalId::from_external(card.certificate.clone(), vec![], Arc::new(SoftKey::new(other)))
            .unwrap();
        let err = cms_sig::sign_detached(&wrong, &[b"x"], 1_800_000_000).unwrap_err();
        assert!(err.to_string().contains("does not belong"), "{err}");
    }

    #[test]
    fn digest_info_and_side_by_side_signatures() {
        let d = rsa_digest_info(&[7u8; 32]);
        assert_eq!(d.len(), 51);
        assert_eq!(d[1] as usize, d.len() - 2);
        assert_eq!(rsa_digest_info(&[7u8; 48]).len(), 67);
        assert!(ecdsa_der(&[1u8; 63]).is_err());
    }
}
