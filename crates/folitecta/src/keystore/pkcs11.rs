//! Smart cards and tokens through their PKCS #11 module (cryptoki): the
//! certificates of every token present, and a key found by the id of its
//! certificate once the token is unlocked.

use std::path::Path;
use std::sync::Mutex;

use cryptoki::context::{CInitializeArgs, CInitializeFlags, Pkcs11};
use cryptoki::error::{Error as P11Error, RvError};
use cryptoki::mechanism::Mechanism;
use cryptoki::object::{Attribute, AttributeType, CertificateType, ObjectClass, ObjectHandle};
use cryptoki::session::{Session, UserType};
use cryptoki::types::AuthPin;
use folitecta_core::digsig::{ExternalKey, ExternalKind, ecdsa_der, parse_certificates, rsa_digest_info};

use super::{OutsideId, Source};

/// The modules loaded, kept for the life of the program: a module is
/// initialised once and shared by the listing and the signatures.
static LOADED: Mutex<Vec<(std::path::PathBuf, Pkcs11)>> = Mutex::new(Vec::new());

/// The module at `path`, loaded and initialised.
fn context(path: &Path) -> Result<Pkcs11, String> {
    let mut loaded = LOADED.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((_, ctx)) = loaded.iter().find(|(p, _)| p == path) {
        return Ok(ctx.clone());
    }
    let ctx = Pkcs11::new(path).map_err(|e| format!("the module does not load: {e}"))?;
    match ctx.initialize(CInitializeArgs::new(CInitializeFlags::OS_LOCKING_OK)) {
        Ok(()) | Err(P11Error::Pkcs11(RvError::CryptokiAlreadyInitialized, _)) => {}
        Err(e) => return Err(e.to_string()),
    }
    loaded.push((path.to_path_buf(), ctx.clone()));
    Ok(ctx)
}

fn text(s: &str) -> String {
    s.trim_end_matches([' ', '\0']).to_string()
}

/// The certificates of the tokens the module at `path` reaches.
pub fn list(path: &Path) -> Result<Vec<OutsideId>, String> {
    let ctx = context(path)?;
    let mut out = Vec::new();
    for slot in ctx.get_slots_with_token().map_err(|e| e.to_string())? {
        let Ok(info) = ctx.get_token_info(slot) else {
            continue;
        };
        let Ok(session) = ctx.open_ro_session(slot) else {
            continue;
        };
        let template = [
            Attribute::Class(ObjectClass::CERTIFICATE),
            Attribute::CertificateType(CertificateType::X_509),
        ];
        for cert in session.find_objects(&template).unwrap_or_default() {
            let Ok(attrs) = session.get_attributes(cert, &[AttributeType::Value, AttributeType::Id]) else {
                continue;
            };
            let (mut der, mut id) = (Vec::new(), Vec::new());
            for a in attrs {
                match a {
                    Attribute::Value(v) => der = v,
                    Attribute::Id(v) => id = v,
                    _ => {}
                }
            }
            // A certificate without an id has no key of its own on the token
            // (an issuer's certificate).
            if der.is_empty() || id.is_empty() {
                continue;
            }
            let source = Source::Device {
                module: path.to_path_buf(),
                token: text(info.label()),
                serial: text(info.serial_number()),
                object: id,
                needs_pin: info.login_required() && !info.protected_authentication_path(),
            };
            if let Some(found) = OutsideId::new(source, text(info.label()), der) {
                out.push(found);
            }
        }
    }
    Ok(out)
}

/// A key of a token, in a session unlocked for it.
pub struct TokenKey {
    session: Mutex<Session>,
    key: ObjectHandle,
    kind: ExternalKind,
}

impl ExternalKey for TokenKey {
    fn sign_digest(&self, digest: &[u8]) -> folitecta_core::Result<Vec<u8>> {
        let session = self.session.lock().unwrap_or_else(|e| e.into_inner());
        let failed = |e: P11Error| folitecta_core::Error::Invalid(format!("the token did not sign: {e}"));
        match self.kind {
            ExternalKind::Rsa => session
                .sign(&Mechanism::RsaPkcs, self.key, &rsa_digest_info(digest))
                .map_err(failed),
            _ => ecdsa_der(
                &session
                    .sign(&Mechanism::Ecdsa, self.key, digest)
                    .map_err(failed)?,
            ),
        }
    }
}

/// The key of the certificate with the id `object` on the token `serial`
/// of the module at `path`, the token unlocked with `pin` (on a reader with
/// its own keypad, `None`: the PIN is typed there).
pub fn open(path: &Path, serial: &str, object: &[u8], pin: Option<&str>) -> Result<TokenKey, String> {
    let ctx = context(path)?;
    let slot = ctx
        .get_slots_with_token()
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|s| {
            ctx.get_token_info(*s)
                .is_ok_and(|i| text(i.serial_number()) == serial)
        })
        .ok_or("the card or token is no longer there")?;
    let session = ctx.open_ro_session(slot).map_err(|e| e.to_string())?;
    let pin = pin.map(|p| AuthPin::new(p.into()));
    match session.login(UserType::User, pin.as_ref()) {
        Ok(()) | Err(P11Error::Pkcs11(RvError::UserAlreadyLoggedIn, _)) => {}
        Err(P11Error::Pkcs11(RvError::PinIncorrect, _)) => return Err("wrong PIN".into()),
        Err(P11Error::Pkcs11(RvError::PinLocked, _)) => return Err("the PIN is locked".into()),
        Err(e) => return Err(e.to_string()),
    }
    let cert = session
        .find_objects(&[
            Attribute::Class(ObjectClass::CERTIFICATE),
            Attribute::Id(object.to_vec()),
        ])
        .ok()
        .and_then(|c| c.into_iter().next())
        .and_then(|c| session.get_attributes(c, &[AttributeType::Value]).ok())
        .and_then(|a| {
            a.into_iter().find_map(|a| match a {
                Attribute::Value(v) => Some(v),
                _ => None,
            })
        })
        .and_then(|der| parse_certificates(&der).ok())
        .and_then(|c| c.into_iter().next())
        .ok_or("the certificate is no longer on the token")?;
    let kind = ExternalKind::of(&cert).ok_or("the key of the certificate is not supported")?;
    let key = session
        .find_objects(&[
            Attribute::Class(ObjectClass::PRIVATE_KEY),
            Attribute::Id(object.to_vec()),
        ])
        .map_err(|e| e.to_string())?
        .into_iter()
        .next()
        .ok_or("the token has no private key for this certificate")?;
    Ok(TokenKey {
        session: Mutex::new(session),
        key,
        kind,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_module_that_is_not_one_is_refused() {
        let err = list(Path::new("/nonexistent/module.so")).unwrap_err();
        assert!(err.contains("does not load"), "{err}");
    }

    /// The system's trust module, where there is one: it loads and lists
    /// its tokens, twice (loaded once).
    #[cfg(all(unix, not(target_os = "macos")))]
    #[test]
    fn a_module_of_the_system_lists_its_tokens() {
        let trust = Path::new("/usr/lib/pkcs11/p11-kit-trust.so");
        if trust.exists() {
            list(trust).unwrap();
            list(trust).unwrap();
        }
    }
}
