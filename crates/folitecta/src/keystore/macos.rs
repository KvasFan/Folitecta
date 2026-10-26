//! The identities of the macOS keychain — certificates with their private
//! key, a smart card's included — signed with through the Security
//! framework, which asks for the keychain password or the card's PIN
//! itself.

use folitecta_core::digsig::{ExternalKey, ExternalKind, parse_certificates};
use security_framework::identity::SecIdentity;
use security_framework::item::{ItemClass, ItemSearchOptions, Limit, Reference, SearchResult};
use security_framework::key::{Algorithm, SecKey};

use super::{OutsideId, Source};

fn identities() -> Result<Vec<SecIdentity>, String> {
    let found = ItemSearchOptions::new()
        .class(ItemClass::identity())
        .load_refs(true)
        .limit(Limit::All)
        .search();
    match found {
        Ok(items) => Ok(items
            .into_iter()
            .filter_map(|r| match r {
                SearchResult::Ref(Reference::Identity(i)) => Some(i),
                _ => None,
            })
            .collect()),
        // No identity at all is not a failure.
        Err(e) if e.code() == -25300 => Ok(Vec::new()),
        Err(e) => Err(format!("the keychain cannot be read: {e}")),
    }
}

/// The identities of the keychains.
pub fn list() -> Result<Vec<OutsideId>, String> {
    Ok(identities()?
        .into_iter()
        .filter_map(|i| i.certificate().ok())
        .filter_map(|c| OutsideId::new(Source::System, "Keychain".into(), c.to_der()))
        .collect())
}

/// A key of the keychain.
pub struct KeychainKey {
    key: SecKey,
    kind: ExternalKind,
}

impl ExternalKey for KeychainKey {
    fn sign_digest(&self, digest: &[u8]) -> folitecta_core::Result<Vec<u8>> {
        let long = digest.len() == 48;
        let algorithm = match (self.kind, long) {
            (ExternalKind::Rsa, false) => Algorithm::RSASignatureDigestPKCS1v15SHA256,
            (ExternalKind::Rsa, true) => Algorithm::RSASignatureDigestPKCS1v15SHA384,
            (_, false) => Algorithm::ECDSASignatureDigestX962SHA256,
            (_, true) => Algorithm::ECDSASignatureDigestX962SHA384,
        };
        // ECDSA signatures come as DER already.
        self.key
            .create_signature(algorithm, digest)
            .map_err(|e| folitecta_core::Error::Invalid(format!("the keychain did not sign: {e}")))
    }
}

/// The key of the identity whose certificate is `der`.
pub fn open(der: &[u8]) -> Result<KeychainKey, String> {
    let kind = parse_certificates(der)
        .ok()
        .and_then(|c| c.into_iter().next())
        .and_then(|c| ExternalKind::of(&c))
        .ok_or("the key of the certificate is not supported")?;
    let identity = identities()?
        .into_iter()
        .find(|i| i.certificate().is_ok_and(|c| c.to_der() == der))
        .ok_or("the certificate is no longer in the keychain")?;
    let key = identity
        .private_key()
        .map_err(|e| format!("the private key is not available: {e}"))?;
    Ok(KeychainKey { key, kind })
}
