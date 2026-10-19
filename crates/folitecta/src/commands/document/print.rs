//! Print jobs: the print files built, checked and sent to a printer, saved
//! as PDF or laid out for the system print dialog, one document at a time,
//! away from the interface, with the progress in the status bar and a way to
//! stop. The window that sets them up is `ui::features::print`.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use dioxus::prelude::*;
use folitecta_core::print::{BuildOptions, Content, PrintPermission, PrintPlan, PrintSource};
use folitecta_core::{RenderService, print};

use crate::i18n::{t, tf};
use crate::printing::{self, JobProgress, JobTicket, PrintError};
use crate::state::{AppState, ToastKind};

/// A job under way.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PrintRun {
    pub running: bool,
    /// The document being printed (from 0) and how many there are.
    pub doc: usize,
    pub docs: usize,
    pub progress: Option<JobProgress>,
}

pub static JOB: GlobalSignal<PrintRun> = Signal::global(PrintRun::default);
static STOP: AtomicBool = AtomicBool::new(false);

/// Where the sheets go.
#[derive(Debug, Clone, PartialEq)]
pub enum Destination {
    Printer(String),
    /// A file (one document) or a folder (one file for each document).
    SavePdf(PathBuf),
    System,
}

/// One document to print: its source and the plan of its sheets.
#[derive(Clone)]
pub struct PrintItem {
    pub source: Arc<PrintSource>,
    pub plan: PrintPlan,
    pub name: String,
}

pub struct PrintRequest {
    pub items: Vec<PrintItem>,
    pub content: Content,
    pub ticket: JobTicket,
    pub dest: Destination,
    /// Print the sheets as pictures at this resolution.
    pub as_image: Option<u32>,
}

/// The pictures of the previews of the print window, newest last.
static PREVIEWS: Mutex<VecDeque<(u64, Arc<Vec<u8>>)>> = Mutex::new(VecDeque::new());
/// The sheets laid out for the system dialog: their generation and a
/// renderer for each document.
static SHEETS: Mutex<Option<(u64, Vec<Arc<RenderService>>)>> = Mutex::new(None);

/// Keep the preview picture of generation `generation`.
pub fn store_preview(generation: u64, png: Vec<u8>) {
    let mut p = PREVIEWS.lock().unwrap_or_else(|e| e.into_inner());
    p.push_back((generation, Arc::new(png)));
    while p.len() > 4 {
        p.pop_front();
    }
}

fn preview(generation: u64) -> Option<Arc<Vec<u8>>> {
    PREVIEWS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .iter()
        .find(|(g, _)| *g == generation)
        .map(|(_, png)| png.clone())
}

/// Answer `/sheet/p/{generation}.png` (a preview) and
/// `/sheet/s/{generation}/{doc}/{sheet}/{scale_milli}.png` (a sheet for the
/// system dialog); `reply` gets the status and the picture.
pub fn serve(path: &str, reply: impl FnOnce(u16, Vec<u8>) + Send + 'static) {
    let parts: Vec<&str> = path.trim_start_matches('/').split('/').collect();
    let number = |s: &str| s.trim_end_matches(".png").parse::<u64>().ok();
    match parts.as_slice() {
        ["sheet", "p", generation] => match number(generation).and_then(preview) {
            Some(png) => reply(200, png.to_vec()),
            None => reply(404, Vec::new()),
        },
        ["sheet", "s", generation, doc, sheet, scale] => {
            let (Some(generation), Some(doc), Some(sheet), Some(scale)) =
                (number(generation), number(doc), number(sheet), number(scale))
            else {
                return reply(404, Vec::new());
            };
            let service = SHEETS
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .as_ref()
                .filter(|(g, _)| *g == generation)
                .and_then(|(_, list)| list.get(doc as usize).cloned());
            match service {
                Some(svc) => svc.render(
                    sheet as usize,
                    (scale as f32 / 1000.0).clamp(0.5, 8.0),
                    move |r| match r {
                        Ok(page) => reply(200, page.png),
                        Err(e) => reply(500, e.to_string().into_bytes()),
                    },
                ),
                None => reply(404, Vec::new()),
            }
        }
        _ => reply(404, Vec::new()),
    }
}

/// The resolution of the pictures of `sides` sides of `area` square points
/// for the system dialog, so that the decoded pictures stay within about
/// 1.2 GB; `None` when even 72 dpi would not.
pub fn system_dpi(sides: usize, area: f32) -> Option<u32> {
    const BUDGET: f64 = 1.2e9;
    let inches = f64::from(area) / (72.0 * 72.0);
    let dpi = (BUDGET / (4.0 * sides.max(1) as f64 * inches.max(1.0))).sqrt();
    (dpi >= 72.0).then(|| dpi.min(300.0) as u32)
}

/// The name of the saved print file of a document.
pub fn save_name(name: &str) -> String {
    let stem = name
        .strip_suffix(".pdf")
        .or_else(|| name.strip_suffix(".PDF"))
        .unwrap_or(name);
    let stem: String = stem
        .chars()
        .map(|c| if "/\\:*?\"<>|".contains(c) { '_' } else { c })
        .collect();
    format!("{} - print.pdf", stem.trim())
}

/// Stop the job under way (between sheets, or while it is sent).
pub fn cancel() {
    STOP.store(true, Ordering::Relaxed);
}

/// How one document's job ended.
enum Done {
    Sent(printing::JobOutcome),
    Saved,
    Sheets(Arc<Vec<u8>>),
    Cancelled,
}

/// Why one document's job failed.
enum Failed {
    Check(String),
    Print(PrintError),
    Other(String),
}

/// Build, check and deliver one document (on a worker thread).
fn run_one(
    item: &PrintItem,
    req: &PrintRequest,
    dest: &Destination,
    shared: &Mutex<Option<JobProgress>>,
) -> Result<Done, Failed> {
    let set = |p: JobProgress| *shared.lock().unwrap_or_else(|e| e.into_inner()) = Some(p);
    let stopped = || STOP.load(Ordering::Relaxed);
    let range = 0..item.plan.sheets.len();
    let o = BuildOptions {
        content: req.content,
        compress: true,
    };
    let built = item
        .source
        .build(&item.plan, range.clone(), &o, &mut |done, total| {
            set(JobProgress::Building { done, total });
            !stopped()
        })
        .map_err(|e| Failed::Other(e.to_string()))?;
    let Some(bytes) = built else {
        return Ok(Done::Cancelled);
    };
    let mut pdf = Arc::new(bytes);
    print::validate(&pdf, &item.plan, range.clone()).map_err(|e| Failed::Check(e.to_string()))?;
    let backend = printing::backend();
    let low = item.source.permission() == PrintPermission::LowQuality;
    let raster_backend = matches!(dest, Destination::Printer(_)) && backend.raster();
    let image_dpi = if low { Some(150) } else { req.as_image };
    if let (Some(dpi), false) = (image_dpi, raster_backend) {
        let images = print::sheets_as_images(pdf.clone(), dpi as f32, req.ticket.gray, &mut |done, total| {
            set(JobProgress::Rendering { sheet: done, total });
            !stopped()
        })
        .map_err(|e| Failed::Other(e.to_string()))?;
        let Some(images) = images else {
            return Ok(Done::Cancelled);
        };
        pdf = Arc::new(images);
        print::validate(&pdf, &item.plan, range).map_err(|e| Failed::Check(e.to_string()))?;
    }
    match dest {
        Destination::Printer(name) => {
            let mut ticket = req.ticket.clone();
            ticket.printer = name.clone();
            ticket.job_name = item.name.clone();
            if low {
                ticket.dpi = 150;
            }
            match backend.submit(&ticket, pdf, &set, &STOP) {
                Ok(outcome) => Ok(Done::Sent(outcome)),
                Err(PrintError::Cancelled) => Ok(Done::Cancelled),
                Err(e) => Err(Failed::Print(e)),
            }
        }
        Destination::SavePdf(path) => {
            let file = if path.is_dir() {
                path.join(save_name(&item.name))
            } else {
                path.clone()
            };
            let out = item
                .source
                .protect(&pdf)
                .map_err(|e| Failed::Check(e.to_string()))?;
            folitecta_core::document::write_pdf_file(&file, &out)
                .map_err(|e| Failed::Other(e.to_string()))?;
            Ok(Done::Saved)
        }
        Destination::System => Ok(Done::Sheets(pdf)),
    }
}

/// The message of a failed job.
fn failure_text(st: AppState, f: &Failed, printer: &str) -> String {
    let lang = st.lang();
    match f {
        Failed::Check(e) => format!("{} ({e})", t(lang, "toast.print_check_failed")),
        Failed::Other(e) => tf(lang, "toast.print_failed", &[("reason", e)]),
        Failed::Print(e) => match e {
            PrintError::Auth => t(lang, "toast.print_auth"),
            PrintError::FormatNotSupported => t(lang, "toast.print_format"),
            PrintError::NotAccepting => t(lang, "toast.print_not_accepting"),
            PrintError::NotFound(_) => tf(lang, "toast.print_not_found", &[("printer", printer)]),
            PrintError::NoServer | PrintError::RemoteServer(_) => t(lang, "toast.print_no_server"),
            other => tf(lang, "toast.print_failed", &[("reason", &other.to_string())]),
        },
    }
}

/// Start a job (nothing happens while another one runs).
pub fn start(st: AppState, req: PrintRequest) {
    let lang = st.lang();
    if JOB.peek().running {
        st.toast(ToastKind::Warn, t(lang, "dialog.print.warn.busy"), "");
        return;
    }
    if req.items.is_empty() {
        return;
    }
    if req
        .items
        .iter()
        .any(|i| i.source.permission() == PrintPermission::Forbidden)
    {
        st.toast(ToastKind::Error, t(lang, "toast.print_forbidden"), "");
        return;
    }
    STOP.store(false, Ordering::Relaxed);
    let docs = req.items.len();
    *JOB.write() = PrintRun {
        running: true,
        doc: 0,
        docs,
        progress: None,
    };
    let req = Arc::new(req);
    dioxus::core::spawn_forever(async move {
        let printer = match &req.dest {
            Destination::Printer(p) => p.clone(),
            _ => String::new(),
        };
        let mut sheets: Vec<Arc<Vec<u8>>> = Vec::new();
        let mut sent = 0;
        let mut last_job = None;
        let mut ignored: Vec<String> = Vec::new();
        let mut failed = false;
        let mut cancelled = false;
        for i in 0..docs {
            JOB.write().doc = i;
            let shared = Arc::new(Mutex::new(None));
            let (r, s) = (req.clone(), shared.clone());
            let dest = req.dest.clone();
            let handle = tokio::task::spawn_blocking(move || run_one(&r.items[i], &r, &dest, &s));
            while !handle.is_finished() {
                tokio::time::sleep(Duration::from_millis(100)).await;
                let now = *shared.lock().unwrap_or_else(|e| e.into_inner());
                if JOB.peek().progress != now {
                    JOB.write().progress = now;
                }
            }
            match handle.await {
                Ok(Ok(Done::Sent(outcome))) => {
                    sent += 1;
                    last_job = outcome.job_id.or(last_job);
                    ignored.extend(outcome.ignored);
                }
                Ok(Ok(Done::Saved)) => sent += 1,
                Ok(Ok(Done::Sheets(pdf))) => sheets.push(pdf),
                Ok(Ok(Done::Cancelled)) => {
                    cancelled = true;
                    break;
                }
                Ok(Err(f)) => {
                    st.toast(
                        ToastKind::Error,
                        failure_text(st, &f, &printer),
                        req.items[i].name.clone(),
                    );
                    failed = true;
                    break;
                }
                Err(e) => {
                    st.error(e);
                    failed = true;
                    break;
                }
            }
        }
        *JOB.write() = PrintRun::default();
        if cancelled {
            st.toast(ToastKind::Info, t(lang, "toast.print_cancelled"), "");
            return;
        }
        if failed {
            return;
        }
        match &req.dest {
            Destination::Printer(p) => {
                let title = match last_job {
                    Some(job) => tf(
                        lang,
                        "toast.print_sent_job",
                        &[("printer", p), ("job", &job.to_string())],
                    ),
                    None => tf(lang, "toast.print_sent", &[("printer", p)]),
                };
                st.toast(ToastKind::Ok, title, "");
                ignored.sort();
                ignored.dedup();
                if !ignored.is_empty() {
                    st.toast(
                        ToastKind::Warn,
                        tf(lang, "toast.print_ignored", &[("names", &ignored.join(", "))]),
                        "",
                    );
                }
            }
            Destination::SavePdf(_) => {
                if sent > 0 {
                    st.toast(ToastKind::Ok, t(lang, "toast.print_saved"), "");
                }
            }
            Destination::System => system_dialog(st, &req, sheets).await,
        }
    });
}

/// Lay the sheets out as pictures and open the system print dialog (never
/// while the fake printer is in use: the tests and the self-test).
async fn system_dialog(st: AppState, req: &PrintRequest, sheets: Vec<Arc<Vec<u8>>>) {
    let lang = st.lang();
    let count: Vec<usize> = req.items.iter().map(|i| i.plan.sheets.len()).collect();
    let total: usize = count.iter().sum();
    let paper = req
        .items
        .first()
        .and_then(|i| i.plan.sheets.first())
        .map(|s| (s.paper.width, s.paper.height))
        .unwrap_or((595.28, 841.89));
    let Some(dpi) = system_dpi(total, paper.0 * paper.1) else {
        st.toast(ToastKind::Warn, t(lang, "toast.print_too_many"), "");
        return;
    };
    let job = lay_out_sheets(st, sheets, count, paper, dpi);
    let ready = crate::files::prepare_print(st, job).await;
    if ready > 0 && printing::fake().is_none() {
        // The system dialog is modal: the page waits until it is closed.
        dioxus::desktop::window().print();
    }
}

/// Make the sheets of the print files `pdfs` (`count` sheets each) the
/// pictures of the print view, drawn at `dpi`.
pub fn lay_out_sheets(
    st: AppState,
    pdfs: Vec<Arc<Vec<u8>>>,
    count: Vec<usize>,
    paper: (f32, f32),
    dpi: u32,
) -> crate::state::PrintJob {
    let generation = st.alloc_id();
    let services = pdfs
        .into_iter()
        .map(|pdf| Arc::new(RenderService::with_workers(pdf, 2)))
        .collect();
    *SHEETS.lock().unwrap_or_else(|e| e.into_inner()) = Some((generation, services));
    crate::state::PrintJob {
        generation,
        sheets: count,
        scale_milli: ((dpi as f32 / 72.0) * 1000.0).round() as u32,
        paper,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_system_dialog_keeps_its_pictures_within_memory() {
        let a4 = 595.28 * 841.89;
        assert_eq!(system_dpi(1, a4), Some(300));
        let fifty = system_dpi(50, a4).unwrap();
        assert!((200..=260).contains(&fifty), "{fifty}");
        let three_hundred = system_dpi(300, a4).unwrap();
        assert!((90..=110).contains(&three_hundred), "{three_hundred}");
        assert_eq!(system_dpi(1000, a4), None);
        for sides in [50usize, 300] {
            let dpi = system_dpi(sides, a4).unwrap() as f64;
            let bytes = sides as f64 * (595.28 / 72.0 * dpi) * (841.89 / 72.0 * dpi) * 4.0;
            assert!(bytes <= 1.25e9, "{sides}: {bytes}");
        }
    }

    #[test]
    fn saved_print_files_are_named_after_the_document() {
        assert_eq!(save_name("report.pdf"), "report - print.pdf");
        assert_eq!(save_name("a/b:c"), "a_b_c - print.pdf");
    }

    #[test]
    fn sheet_addresses_are_served() {
        store_preview(41, vec![1, 2, 3]);
        let got = Arc::new(Mutex::new(None));
        let g = got.clone();
        serve("/sheet/p/41.png", move |status, body| {
            *g.lock().unwrap() = Some((status, body))
        });
        assert_eq!(*got.lock().unwrap(), Some((200, vec![1, 2, 3])));
        let g = got.clone();
        serve("/sheet/p/40.png", move |status, body| {
            *g.lock().unwrap() = Some((status, body))
        });
        assert_eq!(got.lock().unwrap().as_ref().map(|r| r.0), Some(404));
        let g = got.clone();
        serve("/sheet/s/1/0/0/1000.png", move |status, body| {
            *g.lock().unwrap() = Some((status, body))
        });
        assert_eq!(got.lock().unwrap().as_ref().map(|r| r.0), Some(404));
    }
}
