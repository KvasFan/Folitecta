//! Digital signatures: digital IDs (PKCS#12), CMS signatures, time stamps
//! (RFC 3161), signing and validating PDF documents, revocation of
//! certificates (OCSP, revocation lists) and validation material in the
//! document security store.

pub mod cms_sig;
pub mod crl;
mod der_walk;
pub(crate) mod dss;
pub mod external;
pub mod id;
pub mod ocsp;
pub mod pdf;
pub mod revocation;
pub mod timestamp;

pub use dss::{Dss, DssVri, MaterialPlan, MaterialReport};
pub use external::{ExternalKey, ExternalKind, SoftKey, ecdsa_der, rsa_digest_info};
pub use id::{
    CertInfo, CertProfile, DigitalId, ExtendedUsage, IdentityInfo, KeyKind, cert_info, parse_certificates,
};
pub use pdf::{
    Check, SigAppearance, SignOptions, SignPlacement, SignatureChecks, SignatureStatus, SignedFile,
    TimeStampStatus, ValidationJob,
};
pub use revocation::{
    CertRevocation, FetchFailure, FetchProblem, FetchSummary, RevocationArchive, RevocationKind,
    RevocationNote, RevocationPool, RevocationRequest, RevocationSource, RevocationState,
    RevocationTransport, TimeBasis,
};
pub use timestamp::Transport;
/// A certificate (re-exported for callers that store or compare certificates).
pub use x509_cert::Certificate;

/// DER encoding of a certificate.
pub fn cert_to_der(cert: &Certificate) -> Vec<u8> {
    use der::Encode;
    cert.to_der().unwrap_or_default()
}
