//! The personal certificates of the Windows store ("MY") that have a
//! private key, signed with through CNG: the key may be a file of the
//! system or a smart card, and Windows asks for its PIN itself.

use folitecta_core::digsig::{ExternalKey, ExternalKind, ecdsa_der, parse_certificates};
use windows::Win32::Security::Cryptography::{
    BCRYPT_PKCS1_PADDING_INFO, BCRYPT_SHA256_ALGORITHM, BCRYPT_SHA384_ALGORITHM, CERT_CONTEXT,
    CERT_FIND_SHA1_HASH, CERT_KEY_PROV_INFO_PROP_ID, CRYPT_ACQUIRE_ONLY_NCRYPT_KEY_FLAG, CRYPT_INTEGER_BLOB,
    CertCloseStore, CertEnumCertificatesInStore, CertFindCertificateInStore, CertFreeCertificateContext,
    CertGetCertificateContextProperty, CertOpenSystemStoreW, CryptAcquireCertificatePrivateKey, HCERTSTORE,
    HCRYPTPROV_OR_NCRYPT_KEY_HANDLE, NCRYPT_FLAGS, NCRYPT_HANDLE, NCRYPT_KEY_HANDLE, NCRYPT_PAD_PKCS1_FLAG,
    NCryptFreeObject, NCryptSignHash, PKCS_7_ASN_ENCODING, X509_ASN_ENCODING,
};
use windows::core::{BOOL, w};

use super::{OutsideId, Source};

/// The personal store, closed when dropped.
struct Store(HCERTSTORE);

impl Store {
    fn open() -> Result<Store, String> {
        // SAFETY: opens the current user's personal store by its name.
        unsafe { CertOpenSystemStoreW(None, w!("MY")) }
            .map(Store)
            .map_err(|e| format!("the certificate store does not open: {e}"))
    }
}

impl Drop for Store {
    fn drop(&mut self) {
        // SAFETY: the store was opened by `open` and is closed once.
        let _ = unsafe { CertCloseStore(Some(self.0), 0) };
    }
}

/// The DER of a certificate of the store.
///
/// # Safety
/// `cert` is a valid certificate context.
unsafe fn der_of(cert: *const CERT_CONTEXT) -> Vec<u8> {
    let c = unsafe { &*cert };
    unsafe { std::slice::from_raw_parts(c.pbCertEncoded, c.cbCertEncoded as usize) }.to_vec()
}

/// The certificates of the personal store that have a private key.
pub fn list() -> Result<Vec<OutsideId>, String> {
    let store = Store::open()?;
    let mut out = Vec::new();
    let mut prev: Option<*const CERT_CONTEXT> = None;
    loop {
        // SAFETY: enumerates the open store; the previous context is freed by the call.
        let cert = unsafe { CertEnumCertificatesInStore(store.0, prev) };
        if cert.is_null() {
            break;
        }
        prev = Some(cert);
        let mut len = 0u32;
        // SAFETY: asks only for the size of the key's property.
        let has_key =
            unsafe { CertGetCertificateContextProperty(cert, CERT_KEY_PROV_INFO_PROP_ID, None, &mut len) }
                .is_ok();
        if !has_key {
            continue;
        }
        // SAFETY: `cert` is the current context of the enumeration.
        let der = unsafe { der_of(cert) };
        if let Some(found) = OutsideId::new(Source::System, "Windows".into(), der) {
            out.push(found);
        }
    }
    Ok(out)
}

/// A key of the store, given by CNG.
pub struct StoreKey {
    key: NCRYPT_KEY_HANDLE,
    free: bool,
    kind: ExternalKind,
}

impl Drop for StoreKey {
    fn drop(&mut self) {
        if self.free {
            // SAFETY: the handle was acquired for this key and is freed once.
            let _ = unsafe { NCryptFreeObject(NCRYPT_HANDLE(self.key.0)) };
        }
    }
}

impl ExternalKey for StoreKey {
    fn sign_digest(&self, digest: &[u8]) -> folitecta_core::Result<Vec<u8>> {
        let rsa = BCRYPT_PKCS1_PADDING_INFO {
            pszAlgId: if digest.len() == 48 {
                BCRYPT_SHA384_ALGORITHM
            } else {
                BCRYPT_SHA256_ALGORITHM
            },
        };
        let (padding, flags) = match self.kind {
            ExternalKind::Rsa => (
                Some(&rsa as *const BCRYPT_PKCS1_PADDING_INFO as *const core::ffi::c_void),
                NCRYPT_PAD_PKCS1_FLAG,
            ),
            _ => (None, NCRYPT_FLAGS(0)),
        };
        let failed =
            |e: windows::core::Error| folitecta_core::Error::Invalid(format!("Windows did not sign: {e}"));
        let mut len = 0u32;
        // SAFETY: the first call gives the size of the signature, the second
        // writes it into a buffer of that size.
        unsafe { NCryptSignHash(self.key, padding, digest, None, &mut len, flags) }.map_err(failed)?;
        let mut sig = vec![0u8; len as usize];
        unsafe { NCryptSignHash(self.key, padding, digest, Some(&mut sig), &mut len, flags) }
            .map_err(failed)?;
        sig.truncate(len as usize);
        match self.kind {
            ExternalKind::Rsa => Ok(sig),
            _ => ecdsa_der(&sig),
        }
    }
}

/// The key of the certificate whose SHA-1 thumbprint is `sha1` (hex).
pub fn open(sha1: &str) -> Result<StoreKey, String> {
    let mut hash: Vec<u8> = (0..sha1.len() / 2)
        .filter_map(|i| u8::from_str_radix(&sha1[2 * i..2 * i + 2], 16).ok())
        .collect();
    let blob = CRYPT_INTEGER_BLOB {
        cbData: hash.len() as u32,
        pbData: hash.as_mut_ptr(),
    };
    let store = Store::open()?;
    // SAFETY: finds a certificate of the open store by its thumbprint.
    let cert = unsafe {
        CertFindCertificateInStore(
            store.0,
            X509_ASN_ENCODING | PKCS_7_ASN_ENCODING,
            0,
            CERT_FIND_SHA1_HASH,
            Some(&blob as *const CRYPT_INTEGER_BLOB as *const core::ffi::c_void),
            None,
        )
    };
    if cert.is_null() {
        return Err("the certificate is no longer in the store".into());
    }
    // SAFETY: `cert` was found above and is freed after use.
    let der = unsafe { der_of(cert) };
    let kind = parse_certificates(&der)
        .ok()
        .and_then(|c| c.into_iter().next())
        .and_then(|c| ExternalKind::of(&c));
    let Some(kind) = kind else {
        // SAFETY: the context found above, no longer needed.
        unsafe {
            let _ = CertFreeCertificateContext(Some(cert));
        }
        return Err("the key of the certificate is not supported".into());
    };
    let mut handle = HCRYPTPROV_OR_NCRYPT_KEY_HANDLE::default();
    let mut free = BOOL(0);
    // SAFETY: acquires the CNG key of the certificate; Windows may ask for a PIN.
    let acquired = unsafe {
        CryptAcquireCertificatePrivateKey(
            cert,
            CRYPT_ACQUIRE_ONLY_NCRYPT_KEY_FLAG,
            None,
            &mut handle,
            None,
            Some(&mut free),
        )
    };
    // SAFETY: the context found above, no longer needed.
    unsafe {
        let _ = CertFreeCertificateContext(Some(cert));
    }
    acquired.map_err(|e| format!("the private key is not available: {e}"))?;
    Ok(StoreKey {
        key: NCRYPT_KEY_HANDLE(handle.0),
        free: free.as_bool(),
        kind,
    })
}
