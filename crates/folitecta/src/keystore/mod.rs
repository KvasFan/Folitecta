//! Digital IDs kept outside Folitecta (FOL-L039): the certificates of smart
//! cards and tokens reached through their PKCS #11 module — on Linux the
//! modules the system has registered, through p11-kit's proxy — and the
//! personal certificates of the system store on Windows and of the keychain
//! on macOS. Their private keys are only asked to sign; a PIN is used for
//! the signature it unlocks and never kept. Every call may block (a device,
//! a prompt of the system): they run away from the interface thread.

use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

use folitecta_core::digsig::{
    CertInfo, Certificate, DigitalId, ExternalKind, IdentityInfo, KeyKind, SoftKey, cert_info,
    parse_certificates,
};

#[cfg(target_os = "macos")]
mod macos;
mod pkcs11;
#[cfg(windows)]
mod windows;

/// Where an ID outside is kept.
#[derive(Debug, Clone, PartialEq)]
pub enum Source {
    /// A token of a PKCS #11 module: the module, the token's label and
    /// serial number, the id of the certificate's object (its key has the
    /// same id).
    Device {
        module: PathBuf,
        token: String,
        serial: String,
        object: Vec<u8>,
        /// The token asks for its PIN (not on a reader with its own keypad).
        needs_pin: bool,
    },
    /// The store of the system (Windows) or the keychain (macOS): the
    /// system asks for what it needs itself.
    #[cfg_attr(not(any(windows, target_os = "macos")), allow(dead_code))]
    System,
    /// The card of the self-test, made in memory (PIN 1234).
    StandIn,
}

/// A certificate with a private key kept outside.
#[derive(Debug, Clone, PartialEq)]
pub struct OutsideId {
    pub source: Source,
    /// Where it is, as shown: the token's label or the system store.
    pub place: String,
    pub info: CertInfo,
    pub der: Vec<u8>,
}

impl OutsideId {
    fn new(source: Source, place: String, der: Vec<u8>) -> Option<OutsideId> {
        let cert = parse_certificates(&der).ok()?.into_iter().next()?;
        // Only keys Folitecta can sign with.
        ExternalKind::of(&cert)?;
        Some(OutsideId {
            source,
            place,
            info: cert_info(&cert),
            der,
        })
    }

    /// A stable key for lists and settings: where and which certificate.
    pub fn key(&self) -> String {
        match &self.source {
            Source::Device { token, serial, .. } => format!("p11:{token}:{serial}:{}", self.info.sha1),
            Source::System => format!("system:{}", self.info.sha1),
            Source::StandIn => format!("standin:{}", self.info.sha1),
        }
    }

    /// The PIN is asked by Folitecta (a token without its own keypad).
    pub fn needs_pin(&self) -> bool {
        matches!(
            self.source,
            Source::Device { needs_pin: true, .. } | Source::StandIn
        )
    }

    fn certificate(&self) -> Result<Certificate, String> {
        parse_certificates(&self.der)
            .map_err(|e| e.to_string())?
            .into_iter()
            .next()
            .ok_or_else(|| "no certificate".into())
    }
}

/// The self-test uses a card of its own instead of the devices and stores
/// of the system.
static STAND_IN: Mutex<bool> = Mutex::new(false);

pub fn set_stand_in(on: bool) {
    *STAND_IN.lock().unwrap_or_else(|e| e.into_inner()) = on;
}

fn stand_in() -> bool {
    *STAND_IN.lock().unwrap_or_else(|e| e.into_inner())
}

/// The ID of the stand-in card, made once.
fn stand_in_id() -> Option<DigitalId> {
    static CARD: OnceLock<Option<DigitalId>> = OnceLock::new();
    CARD.get_or_init(|| {
        let info = IdentityInfo {
            name: "Folitecta Test Card".into(),
            organization: "Folitecta".into(),
            ..IdentityInfo::default()
        };
        DigitalId::create_self_signed(&info, KeyKind::EcP256, 1).ok()
    })
    .clone()
}

/// The PKCS #11 modules looked at without being added: the proxy of the
/// modules the system has registered.
pub fn system_modules() -> Vec<PathBuf> {
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        [
            "/usr/lib/p11-kit-proxy.so",
            "/usr/lib64/p11-kit-proxy.so",
            "/usr/lib/x86_64-linux-gnu/p11-kit-proxy.so",
            "/usr/lib/aarch64-linux-gnu/p11-kit-proxy.so",
        ]
        .iter()
        .map(PathBuf::from)
        .filter(|p| p.exists())
        .take(1)
        .collect()
    }
    #[cfg(not(all(unix, not(target_os = "macos"))))]
    {
        Vec::new()
    }
}

/// What the stores hold: the IDs found, and what could not be read (a
/// module that does not load, a token that fails), as text.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Found {
    pub ids: Vec<OutsideId>,
    pub problems: Vec<String>,
}

/// The IDs of the system store and of the modules (the system's and
/// `modules`, added by the user).
pub fn find(modules: &[PathBuf]) -> Found {
    let mut found = Found::default();
    if stand_in() {
        found.ids.extend(
            stand_in_id()
                .and_then(|id| OutsideId::new(Source::StandIn, "Test card".into(), id.certificate_der())),
        );
        return found;
    }
    let mut all = system_modules();
    for m in modules {
        if !all.contains(m) {
            all.push(m.clone());
        }
    }
    for module in all {
        match pkcs11::list(&module) {
            Ok(ids) => found.ids.extend(ids),
            Err(e) => found.problems.push(format!("{}: {e}", module.display())),
        }
    }
    #[cfg(windows)]
    match windows::list() {
        Ok(ids) => found.ids.extend(ids),
        Err(e) => found.problems.push(e),
    }
    #[cfg(target_os = "macos")]
    match macos::list() {
        Ok(ids) => found.ids.extend(ids),
        Err(e) => found.problems.push(e),
    }
    // The same certificate through two ways (a card in the system store and
    // through its module): listed once.
    let mut seen = std::collections::HashSet::new();
    found.ids.retain(|i| seen.insert(i.key()));
    found
}

/// The digital ID of `id`, its key unlocked with `pin` when it asks for one.
pub fn unlock(id: &OutsideId, pin: Option<&str>) -> Result<DigitalId, String> {
    let cert = id.certificate()?;
    let key: Arc<dyn folitecta_core::digsig::ExternalKey> = match &id.source {
        Source::Device {
            module,
            serial,
            object,
            ..
        } => Arc::new(pkcs11::open(module, serial, object, pin)?),
        #[cfg(windows)]
        Source::System => Arc::new(windows::open(&id.info.sha1)?),
        #[cfg(target_os = "macos")]
        Source::System => Arc::new(macos::open(&id.der)?),
        #[cfg(not(any(windows, target_os = "macos")))]
        Source::System => return Err("no system store on this system".into()),
        Source::StandIn => {
            if pin != Some("1234") {
                return Err("wrong PIN".into());
            }
            Arc::new(SoftKey::new(stand_in_id().ok_or("no test card")?))
        }
    };
    DigitalId::from_external(cert, Vec::new(), key).map_err(|e| e.to_string())
}
