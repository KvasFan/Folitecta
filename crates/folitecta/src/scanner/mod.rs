//! Scanners (FOL-L011): the pictures of a scanner through the system's
//! imaging interface — SANE's `scanimage` on Linux (its devices, sources,
//! resolutions and colour modes chosen in Folitecta's dialog), the Windows
//! Image Acquisition dialog on Windows — and a stand-in for the self-test
//! that draws its pages itself. Every call blocks: they run away from the
//! interface thread.

use std::sync::Mutex;

use folitecta_core::ScannedPicture;
use serde::{Deserialize, Serialize};

#[cfg(all(unix, not(target_os = "macos")))]
mod sane;
#[cfg(windows)]
mod wia;

/// A scanner.
#[derive(Debug, Clone, PartialEq)]
pub struct Device {
    pub id: String,
    pub name: String,
}

/// What a scanner offers, by the names it gives them.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Capabilities {
    pub sources: Vec<String>,
    pub modes: Vec<String>,
    pub resolutions: Vec<u32>,
}

/// How to scan; empty names leave the scanner's own choice.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ScanSettings {
    pub device: String,
    pub source: String,
    pub mode: String,
    pub dpi: u32,
}

/// How pictures are acquired here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    /// Devices and settings chosen in Folitecta's dialog.
    Devices,
    /// The system's own dialog chooses the scanner and its settings.
    SystemDialog,
    /// No scanning interface on this system.
    None,
}

static STAND_IN: Mutex<bool> = Mutex::new(false);

/// The self-test's scanner instead of the system's: nothing is started.
pub fn set_stand_in(on: bool) {
    *STAND_IN.lock().unwrap_or_else(|e| e.into_inner()) = on;
}

fn stand_in() -> bool {
    *STAND_IN.lock().unwrap_or_else(|e| e.into_inner())
}

pub fn backend() -> Backend {
    if stand_in() {
        return Backend::Devices;
    }
    if cfg!(windows) {
        Backend::SystemDialog
    } else if cfg!(all(unix, not(target_os = "macos"))) {
        Backend::Devices
    } else {
        Backend::None
    }
}

/// The scanners found (a few seconds: the devices are asked).
pub fn devices() -> Result<Vec<Device>, String> {
    if stand_in() {
        return Ok(vec![Device {
            id: "test:0".into(),
            name: "Folitecta test scanner".into(),
        }]);
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    return sane::devices();
    #[allow(unreachable_code)]
    Ok(Vec::new())
}

/// What scanner `device` offers.
pub fn capabilities(device: &str) -> Result<Capabilities, String> {
    if stand_in() {
        return Ok(Capabilities {
            sources: vec!["Flatbed".into(), "ADF".into()],
            modes: vec!["Color".into(), "Gray".into(), "Lineart".into()],
            resolutions: vec![75, 100, 150, 300],
        });
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    return sane::capabilities(device);
    #[allow(unreachable_code)]
    {
        let _ = device;
        Ok(Capabilities::default())
    }
}

/// A source that feeds sheets one after the other (several pictures).
pub fn is_feeder(source: &str) -> bool {
    let s = source.to_lowercase();
    ["adf", "feeder", "duplex", "automatic"]
        .iter()
        .any(|w| s.contains(w))
}

/// Scan with `settings`; `window` is the window the system's dialog belongs to.
pub fn scan(settings: &ScanSettings, window: isize) -> Result<Vec<ScannedPicture>, String> {
    let _ = window;
    let pictures = if stand_in() {
        stand_in_scan(settings)
    } else {
        #[cfg(all(unix, not(target_os = "macos")))]
        let scanned = sane::scan(settings);
        #[cfg(windows)]
        let scanned = wia::acquire(window);
        #[cfg(not(any(windows, all(unix, not(target_os = "macos")))))]
        let scanned: Result<Vec<ScannedPicture>, String> = Err("no scanning interface".into());
        scanned?
    };
    Ok(pictures.into_iter().map(|p| compact(p, &settings.mode)).collect())
}

/// Colour and grey pictures kept as JPEG (a page of a scan is a photograph
/// of paper); black and white ones as they came.
fn compact(p: ScannedPicture, mode: &str) -> ScannedPicture {
    let m = mode.to_lowercase();
    if m.contains("lineart")
        || m.contains("black")
        || m.contains("binary")
        || p.data.starts_with(&[0xFF, 0xD8])
    {
        return p;
    }
    let Ok(img) = image::load_from_memory(&p.data) else {
        return p;
    };
    let gray = m.contains("gray") || m.contains("grey");
    let mut out = std::io::Cursor::new(Vec::new());
    let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 85);
    let done = if gray {
        img.to_luma8().write_with_encoder(encoder)
    } else {
        img.to_rgb8().write_with_encoder(encoder)
    };
    match done {
        Ok(()) => ScannedPicture {
            data: out.into_inner(),
            dpi: p.dpi,
        },
        Err(_) => p,
    }
}

/// The stand-in's pages: Letter paper at 75 dpi, page k with k dark bars;
/// the feeder gives three sheets, the flatbed one.
fn stand_in_scan(settings: &ScanSettings) -> Vec<ScannedPicture> {
    let dpi = 75u32;
    let (w, h) = (dpi * 17 / 2, dpi * 11);
    let n = if is_feeder(&settings.source) { 3 } else { 1 };
    let base = STAND_IN_COUNT.fetch_add(n, std::sync::atomic::Ordering::SeqCst);
    (0..n)
        .map(|i| {
            let k = base + i + 1;
            let img = image::RgbImage::from_fn(w, h, |x, y| {
                let bar = y > 60 && y < 90 && (x / 30) % 2 == 0 && x / 60 < k;
                if bar {
                    image::Rgb([30, 30, 40])
                } else {
                    image::Rgb([250, 249, 244])
                }
            });
            let mut out = std::io::Cursor::new(Vec::new());
            let _ = img.write_to(&mut out, image::ImageFormat::Png);
            ScannedPicture {
                data: out.into_inner(),
                dpi: dpi as f32,
            }
        })
        .collect()
}

static STAND_IN_COUNT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
