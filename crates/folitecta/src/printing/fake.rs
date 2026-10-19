//! A fake printer for the tests and the self-test: two printers that keep
//! the jobs they receive in memory (and, when asked, in a folder). It never
//! opens a socket or a device.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use folitecta_core::print::{STANDARD_PAPERS, Sides};

use super::{
    JobOutcome, JobProgress, JobTicket, Media, PrintBackend, PrintError, PrinterCaps, PrinterInfo,
    PrinterState,
};

pub const DUPLEX: &str = "Folitecta Test Duplex";
pub const SIMPLEX: &str = "Folitecta Test Simplex";

/// A job the fake printer received.
#[derive(Debug, Clone)]
pub struct FakeJob {
    pub ticket: JobTicket,
    pub pdf: Arc<Vec<u8>>,
}

pub struct FakeBackend {
    dir: Option<PathBuf>,
    jobs: Mutex<Vec<FakeJob>>,
}

impl FakeBackend {
    pub fn new(dir: Option<PathBuf>) -> Self {
        FakeBackend {
            dir,
            jobs: Mutex::new(Vec::new()),
        }
    }

    /// The jobs received so far.
    pub fn jobs(&self) -> Vec<FakeJob> {
        self.jobs.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    pub fn clear(&self) {
        self.jobs.lock().unwrap_or_else(|e| e.into_inner()).clear();
    }
}

fn media(key: &str) -> Media {
    let p = STANDARD_PAPERS
        .iter()
        .find(|p| p.key == key)
        .copied()
        .unwrap_or(STANDARD_PAPERS[1]);
    Media {
        key: p.key.to_string(),
        name: p.name.to_string(),
        width: p.width,
        height: p.height,
        margins: [12.0; 4],
    }
}

impl PrintBackend for FakeBackend {
    fn printers(&self) -> Result<Vec<PrinterInfo>, PrintError> {
        Ok([DUPLEX, SIMPLEX]
            .iter()
            .map(|name| PrinterInfo {
                name: name.to_string(),
                label: name.to_string(),
                location: String::new(),
                model: "Folitecta".into(),
                is_default: *name == DUPLEX,
                state: PrinterState::Idle,
                message: String::new(),
                jobs: 0,
                accepting: true,
            })
            .collect())
    }

    fn capabilities(&self, printer: &str) -> Result<PrinterCaps, PrintError> {
        let duplex = match printer {
            DUPLEX => true,
            SIMPLEX => false,
            other => return Err(PrintError::NotFound(other.to_string())),
        };
        let mut media_list = vec![media("iso_a4_210x297mm"), media("na_letter_8.5x11in")];
        if duplex {
            media_list.push(media("iso_a3_297x420mm"));
        }
        Ok(PrinterCaps {
            media: media_list,
            default_media: Some("iso_a4_210x297mm".into()),
            sides: if duplex {
                vec![Sides::OneSided, Sides::LongEdge, Sides::ShortEdge]
            } else {
                vec![Sides::OneSided]
            },
            duplex,
            default_sides: Sides::OneSided,
            max_copies: 99,
            collate: true,
            color: true,
            default_color: true,
            color_modes: vec!["color".into(), "monochrome".into()],
            resolutions: vec![(600, 600)],
            accepts_pdf: true,
            scaling_none: true,
        })
    }

    fn submit(
        &self,
        ticket: &JobTicket,
        pdf: Arc<Vec<u8>>,
        progress: &dyn Fn(JobProgress),
        cancel: &AtomicBool,
    ) -> Result<JobOutcome, PrintError> {
        self.capabilities(&ticket.printer)?;
        let total = pdf.len() as u64;
        for step in 1..=4u64 {
            if cancel.load(Ordering::Relaxed) {
                return Err(PrintError::Cancelled);
            }
            std::thread::sleep(std::time::Duration::from_millis(15));
            progress(JobProgress::Sending {
                sent: total * step / 4,
                total,
            });
        }
        let n = {
            let mut jobs = self.jobs.lock().unwrap_or_else(|e| e.into_inner());
            jobs.push(FakeJob {
                ticket: ticket.clone(),
                pdf: pdf.clone(),
            });
            jobs.len()
        };
        if let Some(dir) = &self.dir {
            let _ = std::fs::create_dir_all(dir);
            let _ = std::fs::write(dir.join(format!("print-job-{n}.pdf")), pdf.as_slice());
            let json = serde_json::json!({
                "printer": ticket.printer,
                "job_name": ticket.job_name,
                "copies": ticket.copies,
                "collate": ticket.collate,
                "sides": format!("{:?}", ticket.sides),
                "media": ticket.media.as_ref().map(|m| m.key.clone()),
                "gray": ticket.gray,
                "bytes": pdf.len(),
            });
            let _ = std::fs::write(
                dir.join(format!("print-job-{n}.json")),
                serde_json::to_string_pretty(&json).unwrap_or_default(),
            );
        }
        Ok(JobOutcome {
            job_id: Some(n as u32),
            ignored: Vec::new(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_fake_printer_keeps_its_jobs() {
        let dir = std::env::temp_dir().join(format!("folitecta-fake-printer-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let b = FakeBackend::new(Some(dir.clone()));
        let printers = b.printers().unwrap();
        assert_eq!(printers.len(), 2);
        assert!(printers[0].is_default);
        assert!(b.capabilities(DUPLEX).unwrap().duplex);
        assert!(!b.capabilities(SIMPLEX).unwrap().duplex);
        assert!(b.capabilities("other").is_err());
        let ticket = JobTicket {
            printer: DUPLEX.into(),
            job_name: "test".into(),
            copies: 2,
            collate: true,
            sides: Sides::LongEdge,
            media: Some(media("iso_a4_210x297mm")),
            gray: false,
            dpi: 300,
            devmode: None,
        };
        let sent = std::sync::Mutex::new(Vec::new());
        let out = b
            .submit(
                &ticket,
                Arc::new(b"%PDF-1.7 fake %%EOF".to_vec()),
                &|p| sent.lock().unwrap().push(p),
                &AtomicBool::new(false),
            )
            .unwrap();
        assert_eq!(out.job_id, Some(1));
        assert_eq!(sent.lock().unwrap().len(), 4);
        assert_eq!(b.jobs().len(), 1);
        assert!(dir.join("print-job-1.pdf").exists() && dir.join("print-job-1.json").exists());
        let cancelled = b.submit(&ticket, Arc::new(Vec::new()), &|_| {}, &AtomicBool::new(true));
        assert_eq!(cancelled, Err(PrintError::Cancelled));
        assert_eq!(b.jobs().len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
