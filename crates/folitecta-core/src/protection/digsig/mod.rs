//! Digital signatures: digital IDs (PKCS#12), CMS signatures, time stamps
//! (RFC 3161), signing and validating PDF documents.

pub mod cms_sig;
pub mod id;
pub mod pdf;
pub mod timestamp;

pub use id::{CertInfo, DigitalId, IdentityInfo, KeyKind, cert_info, parse_certificates};
pub use pdf::{
    Check, SigAppearance, SignOptions, SignPlacement, SignatureChecks, SignatureStatus, TimeStampStatus,
};
pub use timestamp::Transport;
/// A certificate (re-exported for callers that store or compare certificates).
pub use x509_cert::Certificate;

/// DER encoding of a certificate.
pub fn cert_to_der(cert: &Certificate) -> Vec<u8> {
    use der::Encode;
    cert.to_der().unwrap_or_default()
}
