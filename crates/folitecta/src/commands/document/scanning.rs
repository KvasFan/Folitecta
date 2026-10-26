//! File › From Scanner… (FOL-L011): the scanners found and what they offer,
//! the scan away from the interface, a document of the pages scanned (the
//! backs scanned as a second stack put between the fronts), clean-up of
//! the scans offered afterwards; scan settings kept as presets.

use dioxus::prelude::*;
use folitecta_core::{Document, ScannedPicture};

use crate::commands::handlers;
use crate::scanner::{self, Capabilities, Device, ScanSettings};
use crate::state::{AppState, Dialog};

/// The state of the scan dialog.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ScanState {
    /// The scanners (None while they are looked for).
    pub devices: Option<Vec<Device>>,
    /// What the chosen scanner offers, and which scanner that is.
    pub caps: Option<(String, Capabilities)>,
    pub busy: bool,
    pub problem: Option<String>,
    /// The fronts scanned, waiting for the backs.
    pub fronts: Vec<ScannedPicture>,
}

pub static SCAN: GlobalSignal<ScanState> = Signal::global(ScanState::default);

/// What is done with the pages scanned.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ScanOptions {
    /// The backs are scanned next, as a second stack.
    pub backs: bool,
    /// That stack was turned over (the last sheet first).
    pub reversed: bool,
    /// Clean Up Scans opens on the new document.
    pub clean_up: bool,
}

/// The settings used last.
pub fn last_settings(st: AppState) -> ScanSettings {
    serde_json::from_str(&st.value("scan.last")).unwrap_or_default()
}

/// The presets, by name.
pub fn presets(st: AppState) -> Vec<(String, ScanSettings)> {
    serde_json::from_str(&st.value("scan.presets")).unwrap_or_default()
}

pub fn save_preset(st: AppState, name: &str, settings: &ScanSettings) {
    let name = name.trim();
    if name.is_empty() {
        return;
    }
    let mut all = presets(st);
    all.retain(|(n, _)| n != name);
    all.push((name.to_string(), settings.clone()));
    all.sort_by_key(|a| a.0.to_lowercase());
    st.set_value("scan.presets", serde_json::to_string(&all).unwrap_or_default());
}

pub fn delete_preset(st: AppState, name: &str) {
    let mut all = presets(st);
    all.retain(|(n, _)| n != name);
    st.set_value("scan.presets", serde_json::to_string(&all).unwrap_or_default());
}

/// Ask for what scanner `device` offers.
pub fn look_at(device: String) {
    if device.is_empty() || SCAN.peek().caps.as_ref().is_some_and(|(d, _)| *d == device) {
        return;
    }
    SCAN.write().caps = None;
    spawn(async move {
        let d = device.clone();
        let caps = tokio::task::spawn_blocking(move || scanner::capabilities(&d)).await;
        let mut s = SCAN.write();
        match caps {
            Ok(Ok(c)) => s.caps = Some((device, c)),
            Ok(Err(e)) => {
                s.caps = Some((device, Capabilities::default()));
                s.problem = Some(e);
            }
            Err(_) => {}
        }
    });
}

fn open(st: AppState) {
    *SCAN.write() = ScanState::default();
    st.set_dialog(Dialog::Scan);
    if scanner::backend() != scanner::Backend::Devices {
        SCAN.write().devices = Some(Vec::new());
        return;
    }
    let last = last_settings(st).device;
    spawn(async move {
        let found = tokio::task::spawn_blocking(scanner::devices).await;
        let devices = match found {
            Ok(Ok(d)) => d,
            Ok(Err(e)) => {
                SCAN.write().problem = Some(e);
                Vec::new()
            }
            Err(_) => Vec::new(),
        };
        let chosen = devices
            .iter()
            .find(|d| d.id == last)
            .or(devices.first())
            .map(|d| d.id.clone());
        SCAN.write().devices = Some(devices);
        if let Some(d) = chosen {
            look_at(d);
        }
    });
}

/// The window the system's scanning dialog belongs to.
fn parent_window() -> isize {
    #[cfg(windows)]
    {
        use dioxus::desktop::tao::platform::windows::WindowExtWindows;
        dioxus::desktop::window().window.hwnd() as isize
    }
    #[cfg(not(windows))]
    0
}

/// Scan with `settings`; the backs are waited for when `options.backs` and
/// no fronts were scanned yet.
pub fn start(st: AppState, settings: ScanSettings, options: ScanOptions) {
    if SCAN.peek().busy {
        return;
    }
    st.set_value("scan.last", serde_json::to_string(&settings).unwrap_or_default());
    {
        let mut s = SCAN.write();
        s.busy = true;
        s.problem = None;
    }
    let window = parent_window();
    spawn(async move {
        let s = settings.clone();
        let scanned = tokio::task::spawn_blocking(move || scanner::scan(&s, window)).await;
        let pictures = match scanned {
            Ok(Ok(p)) => p,
            Ok(Err(e)) => {
                let mut s = SCAN.write();
                s.busy = false;
                s.problem = Some(if e.is_empty() {
                    "the scanner did not answer".into()
                } else {
                    e
                });
                return;
            }
            Err(_) => {
                SCAN.write().busy = false;
                return;
            }
        };
        if pictures.is_empty() {
            // The system's dialog was cancelled.
            SCAN.write().busy = false;
            return;
        }
        let waiting_backs = options.backs && SCAN.peek().fronts.is_empty();
        if waiting_backs {
            let mut s = SCAN.write();
            s.fronts = pictures;
            s.busy = false;
            return;
        }
        let fronts = std::mem::take(&mut SCAN.write().fronts);
        finish(st, fronts, pictures, options).await;
    });
}

/// The fronts were scanned and the backs are not: a document of the fronts.
pub fn without_backs(st: AppState, options: ScanOptions) {
    let fronts = std::mem::take(&mut SCAN.write().fronts);
    if fronts.is_empty() {
        return;
    }
    SCAN.write().busy = true;
    spawn(async move {
        finish(st, Vec::new(), fronts, options).await;
    });
}

/// The document of the pages scanned, in a new tab.
async fn finish(
    st: AppState,
    fronts: Vec<ScannedPicture>,
    pictures: Vec<ScannedPicture>,
    options: ScanOptions,
) {
    let n = fronts.len();
    let made = tokio::task::spawn_blocking(move || {
        let mut all = fronts;
        all.extend(pictures);
        let mut doc = Document::from_scans(all)?;
        if n > 0 {
            doc.interleave_backs(n, options.reversed)?;
            doc.clear_history();
        }
        Ok::<_, folitecta_core::Error>(doc)
    })
    .await;
    SCAN.write().busy = false;
    match made {
        Ok(Ok(doc)) => {
            st.set_dialog(Dialog::None);
            crate::files::open_document_tab(st, doc);
            if options.clean_up {
                crate::commands::dispatch(st, "document.cleanUpScans");
            }
        }
        Ok(Err(e)) => SCAN.write().problem = Some(st.core_error_text(&e)),
        Err(_) => {}
    }
}

handlers! {
    "newDocScanner" => open,
    "newDocCustomScan" => open,
}
