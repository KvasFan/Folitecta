//! Digital IDs: a certificate with its private key. They are read from and
//! written to PKCS#12 files (`.pfx`, `.p12`) or created as self-signed IDs.

use cms::content_info::ContentInfo;
use cms::encrypted_data::EncryptedData;
use cms::enveloped_data::EncryptedContentInfo;
use const_oid::ObjectIdentifier;
use der::asn1::{Any, BitString, BmpString, OctetString, SetOfVec};
use der::{Decode, Encode};
use hmac::{Hmac, Mac};
use pkcs12::cert_type::CertBag;
use pkcs12::digest_info::DigestInfo;
use pkcs12::kdf::{Pkcs12KeyType, derive_key_bmp};
use pkcs12::mac_data::MacData;
use pkcs12::pbe_params::Pkcs12PbeParams;
use pkcs12::pfx::{Pfx, Version as PfxVersion};
use pkcs12::safe_bag::SafeBag;
use rand_core::{OsRng, RngCore};
use sha1::Sha1;
use sha2::{Digest, Sha256};
use spki::{AlgorithmIdentifierOwned, SubjectPublicKeyInfoOwned};
use x509_cert::attr::{Attribute, Attributes};
use x509_cert::certificate::{Certificate, TbsCertificate, Version};
use x509_cert::ext::{Extension, Extensions};
use x509_cert::name::Name;
use x509_cert::serial_number::SerialNumber;
use x509_cert::time::Validity;

use crate::error::{Error, Result};

pub(crate) const ID_DATA: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.7.1");
pub(crate) const ID_SIGNED_DATA: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.7.2");
const ID_ENCRYPTED_DATA: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.7.6");
const PBES2: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.5.13");
const ATTR_FRIENDLY_NAME: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.9.20");
const ATTR_LOCAL_KEY_ID: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.9.21");
pub(crate) const RSA_ENCRYPTION: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.1.1");
pub(crate) const SHA256_WITH_RSA: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.1.11");
pub(crate) const EC_PUBLIC_KEY: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.10045.2.1");
pub(crate) const ECDSA_SHA256: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.10045.4.3.2");
pub(crate) const ECDSA_SHA384: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.10045.4.3.3");
pub(crate) const CURVE_P256: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.10045.3.1.7");
pub(crate) const CURVE_P384: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.3.132.0.34");
pub(crate) const SHA1_OID: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.3.14.3.2.26");
pub(crate) const SHA256_OID: ObjectIdentifier = ObjectIdentifier::new_unwrap("2.16.840.1.101.3.4.2.1");
pub(crate) const SHA384_OID: ObjectIdentifier = ObjectIdentifier::new_unwrap("2.16.840.1.101.3.4.2.2");
pub(crate) const SHA512_OID: ObjectIdentifier = ObjectIdentifier::new_unwrap("2.16.840.1.101.3.4.2.3");
const KEY_USAGE: ObjectIdentifier = ObjectIdentifier::new_unwrap("2.5.29.15");
const OID_CN: ObjectIdentifier = ObjectIdentifier::new_unwrap("2.5.4.3");
const OID_O: ObjectIdentifier = ObjectIdentifier::new_unwrap("2.5.4.10");
const OID_OU: ObjectIdentifier = ObjectIdentifier::new_unwrap("2.5.4.11");
const OID_C: ObjectIdentifier = ObjectIdentifier::new_unwrap("2.5.4.6");
const OID_EMAIL: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.9.1");

fn bad(e: impl std::fmt::Display) -> Error {
    Error::Invalid(format!("invalid certificate data: {e}"))
}

/// Algorithm identifier with NULL parameters (RSA and digests).
pub(crate) fn alg_null(oid: ObjectIdentifier) -> AlgorithmIdentifierOwned {
    AlgorithmIdentifierOwned {
        oid,
        parameters: Some(Any::null()),
    }
}

pub(crate) fn alg(oid: ObjectIdentifier) -> AlgorithmIdentifierOwned {
    AlgorithmIdentifierOwned {
        oid,
        parameters: None,
    }
}

fn hex(data: &[u8]) -> String {
    data.iter().map(|b| format!("{b:02X}")).collect()
}

/// Kind of key of a new digital ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyKind {
    Rsa2048,
    Rsa3072,
    EcP256,
}

/// Private key of a digital ID.
#[derive(Clone)]
pub(crate) enum IdKey {
    Rsa(Box<rsa::RsaPrivateKey>),
    P256(p256::ecdsa::SigningKey),
    P384(p384::ecdsa::SigningKey),
}

impl IdKey {
    fn from_pkcs8(der: &[u8]) -> Result<Self> {
        use pkcs8::DecodePrivateKey;
        if let Ok(k) = rsa::RsaPrivateKey::from_pkcs8_der(der) {
            return Ok(IdKey::Rsa(Box::new(k)));
        }
        if let Ok(k) = p256::SecretKey::from_pkcs8_der(der) {
            return Ok(IdKey::P256(k.into()));
        }
        if let Ok(k) = p384::SecretKey::from_pkcs8_der(der) {
            return Ok(IdKey::P384(k.into()));
        }
        Err(Error::Invalid(
            "the private key type is not supported (RSA, ECDSA P-256 or P-384)".into(),
        ))
    }

    fn to_pkcs8(&self) -> Result<Vec<u8>> {
        use pkcs8::EncodePrivateKey;
        let doc = match self {
            IdKey::Rsa(k) => k.to_pkcs8_der().map_err(bad)?,
            IdKey::P256(k) => p256::SecretKey::from(k).to_pkcs8_der().map_err(bad)?,
            IdKey::P384(k) => p384::SecretKey::from(k).to_pkcs8_der().map_err(bad)?,
        };
        Ok(doc.as_bytes().to_vec())
    }

    fn public_key_info(&self) -> Result<SubjectPublicKeyInfoOwned> {
        use pkcs8::EncodePublicKey;
        let der = match self {
            IdKey::Rsa(k) => k.to_public_key().to_public_key_der().map_err(bad)?,
            IdKey::P256(k) => k.verifying_key().to_public_key_der().map_err(bad)?,
            IdKey::P384(k) => k.verifying_key().to_public_key_der().map_err(bad)?,
        };
        SubjectPublicKeyInfoOwned::from_der(der.as_bytes()).map_err(bad)
    }

    /// Digest used with the key.
    pub(crate) fn digest_oid(&self) -> ObjectIdentifier {
        match self {
            IdKey::P384(_) => SHA384_OID,
            _ => SHA256_OID,
        }
    }

    /// Signature algorithm as written in certificates.
    fn cert_signature_alg(&self) -> AlgorithmIdentifierOwned {
        match self {
            IdKey::Rsa(_) => alg_null(SHA256_WITH_RSA),
            IdKey::P256(_) => alg(ECDSA_SHA256),
            IdKey::P384(_) => alg(ECDSA_SHA384),
        }
    }

    /// Signature algorithm as written in CMS signer infos.
    pub(crate) fn cms_signature_alg(&self) -> AlgorithmIdentifierOwned {
        match self {
            IdKey::Rsa(_) => alg_null(RSA_ENCRYPTION),
            IdKey::P256(_) => alg(ECDSA_SHA256),
            IdKey::P384(_) => alg(ECDSA_SHA384),
        }
    }

    /// Sign `message` (hashed with the key's digest).
    pub(crate) fn sign(&self, message: &[u8]) -> Result<Vec<u8>> {
        use signature::{SignatureEncoding, Signer};
        Ok(match self {
            IdKey::Rsa(k) => {
                let sk = rsa::pkcs1v15::SigningKey::<Sha256>::new(k.as_ref().clone());
                let sig: rsa::pkcs1v15::Signature = sk.try_sign(message).map_err(bad)?;
                sig.to_vec()
            }
            IdKey::P256(k) => {
                let sig: p256::ecdsa::Signature = k.try_sign(message).map_err(bad)?;
                sig.to_der().as_bytes().to_vec()
            }
            IdKey::P384(k) => {
                let sig: p384::ecdsa::Signature = k.try_sign(message).map_err(bad)?;
                sig.to_der().as_bytes().to_vec()
            }
        })
    }
}

/// Readable information about a certificate.
#[derive(Debug, Clone, PartialEq)]
pub struct CertInfo {
    pub subject: String,
    pub common_name: String,
    pub organization: String,
    pub unit: String,
    pub country: String,
    pub email: String,
    pub issuer: String,
    pub issuer_common_name: String,
    pub serial: String,
    pub not_before: String,
    pub not_after: String,
    pub not_before_unix: i64,
    pub not_after_unix: i64,
    pub sha256: String,
    pub sha1: String,
    pub key: String,
    pub self_signed: bool,
}

fn attr_string(value: &Any) -> String {
    use der::asn1::{Ia5StringRef, PrintableStringRef, TeletexStringRef, Utf8StringRef};
    if let Ok(s) = Utf8StringRef::try_from(value) {
        return s.as_str().to_string();
    }
    if let Ok(s) = PrintableStringRef::try_from(value) {
        return s.as_str().to_string();
    }
    if let Ok(s) = Ia5StringRef::try_from(value) {
        return s.as_str().to_string();
    }
    if let Ok(s) = TeletexStringRef::try_from(value) {
        return s.as_str().to_string();
    }
    if let Ok(s) = value.decode_as::<BmpString>() {
        return s.to_string();
    }
    String::from_utf8_lossy(value.value()).to_string()
}

/// First value of an attribute of a name.
pub(crate) fn name_part(name: &Name, oid: ObjectIdentifier) -> String {
    name.0
        .iter()
        .flat_map(|rdn| rdn.0.iter())
        .find(|a| a.oid == oid)
        .map(|a| attr_string(&a.value))
        .unwrap_or_default()
}

fn format_time(secs: i64) -> String {
    time::OffsetDateTime::from_unix_timestamp(secs)
        .ok()
        .and_then(|t| {
            t.format(&time::macros::format_description!(
                "[year]-[month]-[day] [hour]:[minute] UTC"
            ))
            .ok()
        })
        .unwrap_or_default()
}

/// Information about `cert`.
pub fn cert_info(cert: &Certificate) -> CertInfo {
    let tbs = &cert.tbs_certificate;
    let der = cert.to_der().unwrap_or_default();
    let nb = tbs.validity.not_before.to_unix_duration().as_secs() as i64;
    let na = tbs.validity.not_after.to_unix_duration().as_secs() as i64;
    let spki = &tbs.subject_public_key_info;
    let key = if spki.algorithm.oid == RSA_ENCRYPTION {
        use rsa::pkcs1::DecodeRsaPublicKey;
        let bits = rsa::RsaPublicKey::from_pkcs1_der(spki.subject_public_key.raw_bytes())
            .map(|k| rsa::traits::PublicKeyParts::size(&k) * 8)
            .unwrap_or(0);
        format!("RSA {bits}")
    } else if spki.algorithm.oid == EC_PUBLIC_KEY {
        let curve = spki
            .algorithm
            .parameters
            .as_ref()
            .and_then(|p| p.decode_as::<ObjectIdentifier>().ok());
        match curve {
            Some(c) if c == CURVE_P256 => "ECDSA P-256".into(),
            Some(c) if c == CURVE_P384 => "ECDSA P-384".into(),
            _ => "ECDSA".into(),
        }
    } else {
        spki.algorithm.oid.to_string()
    };
    CertInfo {
        subject: tbs.subject.to_string(),
        common_name: name_part(&tbs.subject, OID_CN),
        organization: name_part(&tbs.subject, OID_O),
        unit: name_part(&tbs.subject, OID_OU),
        country: name_part(&tbs.subject, OID_C),
        email: name_part(&tbs.subject, OID_EMAIL),
        issuer: tbs.issuer.to_string(),
        issuer_common_name: name_part(&tbs.issuer, OID_CN),
        serial: hex(tbs.serial_number.as_bytes()),
        not_before: format_time(nb),
        not_after: format_time(na),
        not_before_unix: nb,
        not_after_unix: na,
        sha256: hex(&Sha256::digest(&der)),
        sha1: hex(&Sha1::digest(&der)),
        key,
        self_signed: tbs.subject == tbs.issuer,
    }
}

/// Owner details of a new self-signed digital ID.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct IdentityInfo {
    pub name: String,
    pub organization: String,
    pub unit: String,
    pub email: String,
    /// Two-letter country code.
    pub country: String,
}

fn escape_rdn(v: &str) -> String {
    let mut out = String::new();
    for (i, c) in v.chars().enumerate() {
        if matches!(c, ',' | '+' | '"' | '\\' | '<' | '>' | ';' | '=') || (i == 0 && matches!(c, '#' | ' ')) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// A certificate with its private key.
#[derive(Clone)]
pub struct DigitalId {
    pub certificate: Certificate,
    /// Other certificates of the file (issuers of the certificate).
    pub chain: Vec<Certificate>,
    pub(crate) key: IdKey,
}

impl std::fmt::Debug for DigitalId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DigitalId")
            .field("subject", &self.certificate.tbs_certificate.subject.to_string())
            .finish()
    }
}

fn decrypt_pkcs12_pbe(
    alg_id: &AlgorithmIdentifierOwned,
    data: &[u8],
    password: &BmpString,
) -> Result<Vec<u8>> {
    use cbc::cipher::block_padding::Pkcs7;
    use cbc::cipher::{BlockDecryptMut, InnerIvInit, KeyIvInit};
    let params = alg_id
        .parameters
        .as_ref()
        .ok_or_else(|| bad("missing encryption parameters"))?
        .decode_as::<Pkcs12PbeParams>()
        .map_err(bad)?;
    let salt = params.salt.as_bytes();
    let it = params.iterations;
    let iv = derive_key_bmp::<Sha1>(password.clone(), salt, Pkcs12KeyType::Iv, it, 8);
    let oid = alg_id.oid.to_string();
    let wrong = || Error::Invalid("wrong password".into());
    match oid.as_str() {
        "1.2.840.113549.1.12.1.3" => {
            let key = derive_key_bmp::<Sha1>(password.clone(), salt, Pkcs12KeyType::EncryptionKey, it, 24);
            cbc::Decryptor::<des::TdesEde3>::new_from_slices(&key, &iv)
                .map_err(bad)?
                .decrypt_padded_vec_mut::<Pkcs7>(data)
                .map_err(|_| wrong())
        }
        "1.2.840.113549.1.12.1.4" => {
            let key = derive_key_bmp::<Sha1>(password.clone(), salt, Pkcs12KeyType::EncryptionKey, it, 16);
            cbc::Decryptor::<des::TdesEde2>::new_from_slices(&key, &iv)
                .map_err(bad)?
                .decrypt_padded_vec_mut::<Pkcs7>(data)
                .map_err(|_| wrong())
        }
        "1.2.840.113549.1.12.1.5" | "1.2.840.113549.1.12.1.6" => {
            let (len, bits) = if oid.ends_with(".5") { (16, 128) } else { (5, 40) };
            let key = derive_key_bmp::<Sha1>(password.clone(), salt, Pkcs12KeyType::EncryptionKey, it, len);
            let cipher = rc2::Rc2::new_with_eff_key_len(&key, bits);
            cbc::Decryptor::<rc2::Rc2>::inner_iv_slice_init(cipher, &iv)
                .map_err(bad)?
                .decrypt_padded_vec_mut::<Pkcs7>(data)
                .map_err(|_| wrong())
        }
        other => Err(Error::Invalid(format!(
            "unsupported encryption {other} in the certificate file"
        ))),
    }
}

fn decrypt(alg_id: &AlgorithmIdentifierOwned, data: &[u8], password: &str) -> Result<Vec<u8>> {
    if alg_id.oid == PBES2 {
        let der = alg_id.to_der().map_err(bad)?;
        let scheme = pkcs5::EncryptionScheme::try_from(der.as_slice()).map_err(bad)?;
        return scheme
            .decrypt(password, data)
            .map_err(|_| Error::Invalid("wrong password".into()));
    }
    let bmp = BmpString::from_utf8(password).map_err(bad)?;
    decrypt_pkcs12_pbe(alg_id, data, &bmp)
}

fn encrypt_pbes2(plain: &[u8], password: &str) -> Result<(AlgorithmIdentifierOwned, Vec<u8>)> {
    let mut salt = [0u8; 16];
    let mut iv = [0u8; 16];
    OsRng.fill_bytes(&mut salt);
    OsRng.fill_bytes(&mut iv);
    let params = pkcs5::pbes2::Parameters::pbkdf2_sha256_aes256cbc(2048, &salt, &iv).map_err(bad)?;
    let scheme = pkcs5::EncryptionScheme::from(params);
    let ct = scheme.encrypt(password, plain).map_err(bad)?;
    let alg_der = scheme.to_der().map_err(bad)?;
    let alg_id = AlgorithmIdentifierOwned::from_der(&alg_der).map_err(bad)?;
    Ok((alg_id, ct))
}

/// Content of a context-specific `[0] EXPLICIT` wrapper, as decoded by `SafeBag`.
fn unwrap_explicit(bytes: &[u8]) -> Vec<u8> {
    if bytes.first() == Some(&0xA0) {
        if let Ok(any) = Any::from_der(bytes) {
            return any.value().to_vec();
        }
    }
    bytes.to_vec()
}

fn mac_key(digest: ObjectIdentifier, password: &BmpString, salt: &[u8], it: i32) -> Option<Vec<u8>> {
    if digest == SHA1_OID {
        Some(derive_key_bmp::<Sha1>(
            password.clone(),
            salt,
            Pkcs12KeyType::Mac,
            it,
            20,
        ))
    } else if digest == SHA256_OID {
        Some(derive_key_bmp::<Sha256>(
            password.clone(),
            salt,
            Pkcs12KeyType::Mac,
            it,
            32,
        ))
    } else {
        None
    }
}

fn hmac_of(digest: ObjectIdentifier, key: &[u8], data: &[u8]) -> Vec<u8> {
    if digest == SHA1_OID {
        let mut m = <Hmac<Sha1> as Mac>::new_from_slice(key).expect("any key length");
        m.update(data);
        m.finalize().into_bytes().to_vec()
    } else {
        let mut m = <Hmac<Sha256> as Mac>::new_from_slice(key).expect("any key length");
        m.update(data);
        m.finalize().into_bytes().to_vec()
    }
}

fn attributes(local_key_id: &[u8], friendly: Option<&str>) -> Result<Attributes> {
    let mut attrs = Vec::new();
    let id = Any::encode_from(&OctetString::new(local_key_id.to_vec()).map_err(bad)?).map_err(bad)?;
    attrs.push(Attribute {
        oid: ATTR_LOCAL_KEY_ID,
        values: SetOfVec::try_from(vec![id]).map_err(bad)?,
    });
    if let Some(name) = friendly {
        let v = Any::encode_from(&BmpString::from_utf8(name).map_err(bad)?).map_err(bad)?;
        attrs.push(Attribute {
            oid: ATTR_FRIENDLY_NAME,
            values: SetOfVec::try_from(vec![v]).map_err(bad)?,
        });
    }
    SetOfVec::try_from(attrs).map_err(bad)
}

impl DigitalId {
    /// Read a PKCS#12 file.
    pub fn from_pkcs12(data: &[u8], password: &str) -> Result<DigitalId> {
        let pfx =
            Pfx::from_der(data).map_err(|_| Error::Invalid("the file is not a PKCS#12 digital ID".into()))?;
        let auth = pfx.auth_safe.content.decode_as::<OctetString>().map_err(bad)?;
        let auth_bytes = auth.as_bytes();
        // Check the password with the integrity MAC (empty passwords come in two encodings).
        let bmp = BmpString::from_utf8(password).map_err(bad)?;
        if let Some(mac) = &pfx.mac_data {
            let digest = mac.mac.algorithm.oid;
            let salt = mac.mac_salt.as_bytes();
            let ok = |pw: &BmpString| {
                mac_key(digest, pw, salt, mac.iterations)
                    .map(|k| hmac_of(digest, &k, auth_bytes) == mac.mac.digest.as_bytes())
                    .unwrap_or(true)
            };
            if !ok(&bmp) {
                return Err(Error::Invalid("wrong password".into()));
            }
        }
        let safes = Vec::<ContentInfo>::from_der(auth_bytes).map_err(bad)?;
        let mut certs: Vec<(Vec<u8>, Certificate)> = Vec::new();
        let mut keys: Vec<(Vec<u8>, IdKey)> = Vec::new();
        for ci in safes {
            let contents = if ci.content_type == ID_DATA {
                ci.content
                    .decode_as::<OctetString>()
                    .map_err(bad)?
                    .as_bytes()
                    .to_vec()
            } else if ci.content_type == ID_ENCRYPTED_DATA {
                let ed = ci.content.decode_as::<EncryptedData>().map_err(bad)?;
                let enc = ed
                    .enc_content_info
                    .encrypted_content
                    .as_ref()
                    .ok_or_else(|| bad("empty encrypted content"))?;
                decrypt(&ed.enc_content_info.content_enc_alg, enc.as_bytes(), password)?
            } else {
                continue;
            };
            let bags = Vec::<SafeBag>::from_der(&contents).map_err(bad)?;
            for bag in bags {
                let value = unwrap_explicit(&bag.bag_value);
                let local_id = bag
                    .bag_attributes
                    .as_ref()
                    .and_then(|attrs| attrs.iter().find(|a| a.oid == ATTR_LOCAL_KEY_ID))
                    .and_then(|a| a.values.iter().next())
                    .and_then(|v| v.decode_as::<OctetString>().ok())
                    .map(|o| o.as_bytes().to_vec())
                    .unwrap_or_default();
                if bag.bag_id == pkcs12::PKCS_12_CERT_BAG_OID {
                    let cb = CertBag::from_der(&value).map_err(bad)?;
                    if cb.cert_id == pkcs12::PKCS_12_X509_CERT_OID {
                        let cert = Certificate::from_der(cb.cert_value.as_bytes()).map_err(bad)?;
                        certs.push((local_id, cert));
                    }
                } else if bag.bag_id == pkcs12::PKCS_12_PKCS8_KEY_BAG_OID {
                    // PBES2 or PKCS#12 password-based encryption of the key.
                    let epki = pkcs12::pbe_params::EncryptedPrivateKeyInfo::from_der(&value).map_err(bad)?;
                    let plain = decrypt(
                        &epki.encryption_algorithm,
                        epki.encrypted_data.as_bytes(),
                        password,
                    )?;
                    keys.push((local_id, IdKey::from_pkcs8(&plain)?));
                } else if bag.bag_id == pkcs12::PKCS_12_KEY_BAG_OID {
                    keys.push((local_id, IdKey::from_pkcs8(&value)?));
                }
            }
        }
        let (key_id, key) = keys
            .into_iter()
            .next()
            .ok_or_else(|| Error::Invalid("the file contains no private key".into()))?;
        // The certificate of the key: same public key (or the same local key id).
        let public = key.public_key_info()?;
        let pos = certs
            .iter()
            .position(|(_, c)| c.tbs_certificate.subject_public_key_info == public)
            .or_else(|| {
                certs
                    .iter()
                    .position(|(id, _)| !key_id.is_empty() && *id == key_id)
            })
            .ok_or_else(|| Error::Invalid("the file contains no certificate for its private key".into()))?;
        let (_, certificate) = certs.remove(pos);
        Ok(DigitalId {
            certificate,
            chain: certs.into_iter().map(|(_, c)| c).collect(),
            key,
        })
    }

    /// Write the ID as a PKCS#12 file protected by `password` (AES-256, PBKDF2-SHA-256).
    pub fn to_pkcs12(&self, password: &str) -> Result<Vec<u8>> {
        let cert_der = self.certificate.to_der().map_err(bad)?;
        let local_id = Sha1::digest(&cert_der).to_vec();
        let friendly = cert_info(&self.certificate).common_name;
        // Certificates, encrypted.
        let mut cert_bags = Vec::new();
        for (i, c) in std::iter::once(&self.certificate)
            .chain(self.chain.iter())
            .enumerate()
        {
            let bag = CertBag {
                cert_id: pkcs12::PKCS_12_X509_CERT_OID,
                cert_value: OctetString::new(c.to_der().map_err(bad)?).map_err(bad)?,
            };
            cert_bags.push(SafeBag {
                bag_id: pkcs12::PKCS_12_CERT_BAG_OID,
                bag_value: bag.to_der().map_err(bad)?,
                bag_attributes: if i == 0 {
                    Some(attributes(&local_id, Some(&friendly))?)
                } else {
                    None
                },
            });
        }
        let certs_plain = cert_bags.to_der().map_err(bad)?;
        let (alg_id, ct) = encrypt_pbes2(&certs_plain, password)?;
        let ed = EncryptedData {
            version: cms::content_info::CmsVersion::V0,
            enc_content_info: EncryptedContentInfo {
                content_type: ID_DATA,
                content_enc_alg: alg_id,
                encrypted_content: Some(OctetString::new(ct).map_err(bad)?),
            },
            unprotected_attrs: None,
        };
        let certs_ci = ContentInfo {
            content_type: ID_ENCRYPTED_DATA,
            content: Any::encode_from(&ed).map_err(bad)?,
        };
        // Private key, shrouded.
        let key_plain = self.key.to_pkcs8()?;
        let (key_alg, key_ct) = encrypt_pbes2(&key_plain, password)?;
        let epki = pkcs12::pbe_params::EncryptedPrivateKeyInfo {
            encryption_algorithm: key_alg,
            encrypted_data: OctetString::new(key_ct).map_err(bad)?,
        };
        let key_bag = SafeBag {
            bag_id: pkcs12::PKCS_12_PKCS8_KEY_BAG_OID,
            bag_value: epki.to_der().map_err(bad)?,
            bag_attributes: Some(attributes(&local_id, Some(&friendly))?),
        };
        let key_contents = vec![key_bag].to_der().map_err(bad)?;
        let key_ci = ContentInfo {
            content_type: ID_DATA,
            content: Any::encode_from(&OctetString::new(key_contents).map_err(bad)?).map_err(bad)?,
        };
        let auth = vec![certs_ci, key_ci].to_der().map_err(bad)?;
        // Integrity: HMAC-SHA-256 with the PKCS#12 key derivation.
        let mut salt = [0u8; 16];
        OsRng.fill_bytes(&mut salt);
        let bmp = BmpString::from_utf8(password).map_err(bad)?;
        let key = derive_key_bmp::<Sha256>(bmp, &salt, Pkcs12KeyType::Mac, 2048, 32);
        let mac = hmac_of(SHA256_OID, &key, &auth);
        let pfx = Pfx {
            version: PfxVersion::V3,
            auth_safe: ContentInfo {
                content_type: ID_DATA,
                content: Any::encode_from(&OctetString::new(auth).map_err(bad)?).map_err(bad)?,
            },
            mac_data: Some(MacData {
                mac: DigestInfo {
                    algorithm: alg_null(SHA256_OID),
                    digest: OctetString::new(mac).map_err(bad)?,
                },
                mac_salt: OctetString::new(salt.to_vec()).map_err(bad)?,
                iterations: 2048,
            }),
        };
        pfx.to_der().map_err(bad)
    }

    /// Create a self-signed digital ID valid for `years` years.
    pub fn create_self_signed(info: &IdentityInfo, kind: KeyKind, years: u32) -> Result<DigitalId> {
        if info.name.trim().is_empty() {
            return Err(Error::Invalid("the name of the digital ID is empty".into()));
        }
        let key = match kind {
            KeyKind::Rsa2048 | KeyKind::Rsa3072 => {
                let bits = if kind == KeyKind::Rsa2048 { 2048 } else { 3072 };
                IdKey::Rsa(Box::new(rsa::RsaPrivateKey::new(&mut OsRng, bits).map_err(bad)?))
            }
            KeyKind::EcP256 => IdKey::P256(p256::ecdsa::SigningKey::random(&mut OsRng)),
        };
        let mut parts = vec![format!("CN={}", escape_rdn(info.name.trim()))];
        for (prefix, v) in [
            ("OU", &info.unit),
            ("O", &info.organization),
            ("C", &info.country),
        ] {
            if !v.trim().is_empty() {
                parts.push(format!("{prefix}={}", escape_rdn(v.trim())));
            }
        }
        let mut name: Name = parts.join(",").parse().map_err(bad)?;
        if !info.email.trim().is_empty() {
            // The e-mail address is an IA5String attribute.
            let email = der::asn1::Ia5String::new(info.email.trim()).map_err(bad)?;
            let attr = x509_cert::attr::AttributeTypeAndValue {
                oid: OID_EMAIL,
                value: Any::encode_from(&email).map_err(bad)?,
            };
            let rdn =
                x509_cert::name::RelativeDistinguishedName(SetOfVec::try_from(vec![attr]).map_err(bad)?);
            name.0.push(rdn);
        }
        let mut serial = [0u8; 16];
        OsRng.fill_bytes(&mut serial);
        serial[0] &= 0x7F;
        serial[0] |= 0x01;
        let validity = Validity::from_now(std::time::Duration::from_secs(
            u64::from(years.clamp(1, 30)) * 365 * 86_400,
        ))
        .map_err(bad)?;
        // Key usage: digital signature and non-repudiation.
        let key_usage = Extension {
            extn_id: KEY_USAGE,
            critical: true,
            extn_value: OctetString::new(
                BitString::new(6, vec![0xC0])
                    .map_err(bad)?
                    .to_der()
                    .map_err(bad)?,
            )
            .map_err(bad)?,
        };
        let extensions: Extensions = vec![key_usage];
        let tbs = TbsCertificate {
            version: Version::V3,
            serial_number: SerialNumber::new(&serial).map_err(bad)?,
            signature: key.cert_signature_alg(),
            issuer: name.clone(),
            validity,
            subject: name,
            subject_public_key_info: key.public_key_info()?,
            issuer_unique_id: None,
            subject_unique_id: None,
            extensions: Some(extensions),
        };
        let signature = key.sign(&tbs.to_der().map_err(bad)?)?;
        let certificate = Certificate {
            tbs_certificate: tbs,
            signature_algorithm: key.cert_signature_alg(),
            signature: BitString::from_bytes(&signature).map_err(bad)?,
        };
        Ok(DigitalId {
            certificate,
            chain: Vec::new(),
            key,
        })
    }

    pub fn info(&self) -> CertInfo {
        cert_info(&self.certificate)
    }
}

/// Certificates read from a file: DER, PEM (one or more) or the certificates of
/// a PKCS#7 bundle.
pub fn parse_certificates(data: &[u8]) -> Result<Vec<Certificate>> {
    if let Ok(c) = Certificate::from_der(data) {
        return Ok(vec![c]);
    }
    let text = String::from_utf8_lossy(data);
    let mut out = Vec::new();
    let mut rest = text.as_ref();
    while let Some(start) = rest.find("-----BEGIN CERTIFICATE-----") {
        let after = &rest[start + 27..];
        let Some(end) = after.find("-----END CERTIFICATE-----") else {
            break;
        };
        let b64: String = after[..end].chars().filter(|c| !c.is_whitespace()).collect();
        use base64::Engine;
        if let Ok(der) = base64::engine::general_purpose::STANDARD.decode(b64) {
            if let Ok(c) = Certificate::from_der(&der) {
                out.push(c);
            }
        }
        rest = &after[end..];
    }
    if out.is_empty() {
        if let Ok(ci) = ContentInfo::from_der(data) {
            if ci.content_type == ID_SIGNED_DATA {
                if let Ok(sd) = ci.content.decode_as::<cms::signed_data::SignedData>() {
                    for c in sd.certificates.iter().flat_map(|s| s.0.iter()) {
                        if let cms::cert::CertificateChoices::Certificate(c) = c {
                            out.push(c.clone());
                        }
                    }
                }
            }
        }
    }
    if out.is_empty() {
        Err(Error::Invalid("the file contains no certificate".into()))
    } else {
        Ok(out)
    }
}
