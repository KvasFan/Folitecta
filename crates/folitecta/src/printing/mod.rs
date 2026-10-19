//! Sending print files to printers: the printers of this computer, what each
//! can do, and the jobs. Linux and macOS talk to the local print server
//! (CUPS) over IPP and send the vector print file; Windows draws the sheets
//! through GDI. A fake printer serves the tests and the self-test, so nothing
//! is ever printed by them.

// Each system builds the parts of its own backend only.
#![cfg_attr(not(unix), allow(dead_code))]

use std::sync::{Arc, Mutex};

#[cfg(unix)]
pub mod cups;
#[cfg(any(windows, test))]
pub mod devmode;
pub mod fake;
#[cfg(any(unix, test))]
pub mod ipp;
#[cfg(windows)]
pub mod windows;

mod types;
pub use types::*;

/// About how many bytes the spooler stores for `sides` sides of
/// `width` x `height` points sent as pictures at `dpi` (Windows: the bitmaps
/// are stored as they are).
pub fn spool_estimate(width: f32, height: f32, dpi: u32, gray: bool, sides: usize) -> u64 {
    let px = |pt: f32| (f64::from(pt) / 72.0 * f64::from(dpi)).ceil() as u64;
    px(width) * px(height) * if gray { 1 } else { 3 } * sides as u64
}

/// The fake printer replaces the real ones (the self-test).
static FAKE: Mutex<Option<Arc<fake::FakeBackend>>> = Mutex::new(None);

/// Use the fake printer from now on; its jobs are also written to `dir`.
/// The real printers are never reached afterwards.
pub fn use_fake(dir: Option<std::path::PathBuf>) {
    let mut slot = FAKE.lock().unwrap_or_else(|e| e.into_inner());
    if slot.is_none() {
        *slot = Some(Arc::new(fake::FakeBackend::new(dir)));
    }
}

/// The fake printer, when it is in use.
pub fn fake() -> Option<Arc<fake::FakeBackend>> {
    FAKE.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

/// The backend of this system (the fake one in tests and in the self-test).
pub fn backend() -> Arc<dyn PrintBackend> {
    if cfg!(test) {
        use_fake(None);
    }
    // A build for development may print to a folder instead of a printer.
    #[cfg(debug_assertions)]
    if let Some(dir) = std::env::var_os("FOLITECTA_FAKE_PRINTER") {
        use_fake(Some(dir.into()));
    }
    if let Some(f) = fake() {
        return f;
    }
    real_backend()
}

#[cfg(unix)]
fn real_backend() -> Arc<dyn PrintBackend> {
    Arc::new(cups::Cups::default())
}

#[cfg(windows)]
fn real_backend() -> Arc<dyn PrintBackend> {
    Arc::new(windows::Gdi)
}

#[cfg(not(any(unix, windows)))]
fn real_backend() -> Arc<dyn PrintBackend> {
    struct NoPrinters;
    impl PrintBackend for NoPrinters {
        fn printers(&self) -> Result<Vec<PrinterInfo>, PrintError> {
            Err(PrintError::NoServer)
        }
        fn capabilities(&self, _: &str) -> Result<PrinterCaps, PrintError> {
            Err(PrintError::NoServer)
        }
        fn submit(
            &self,
            _: &JobTicket,
            _: Arc<Vec<u8>>,
            _: &dyn Fn(JobProgress),
            _: &std::sync::atomic::AtomicBool,
        ) -> Result<JobOutcome, PrintError> {
            Err(PrintError::NoServer)
        }
    }
    Arc::new(NoPrinters)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spool_sizes_of_pictures() {
        // An A4 side at 600 dpi in colour is about 104 MB.
        let one = spool_estimate(595.28, 841.89, 600, false, 1);
        assert!((100_000_000..110_000_000).contains(&one), "{one}");
        assert_eq!(
            spool_estimate(595.28, 841.89, 300, true, 4),
            spool_estimate(595.28, 841.89, 300, true, 1) * 4
        );
    }

    #[test]
    fn tests_only_ever_reach_the_fake_printer() {
        let b = backend();
        let printers = b.printers().unwrap();
        assert!(printers.iter().all(|p| p.name.starts_with("Folitecta Test")));
        assert!(fake().is_some());
    }
}
