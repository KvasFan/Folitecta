//! Text recognition (OCR): the pages of a document read (all, some, or only
//! their pictures) and their text written invisibly over them, so they can
//! be searched, selected and copied; an area read and its text shown to
//! copy; pictures made into a searchable document. Pages are read away from
//! the interface thread, one at a time, with the progress shown and a way
//! to stop.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use dioxus::prelude::*;
use folitecta_core::marks::pdf_rect_to_display;
use folitecta_core::ocr::find_models;
use folitecta_core::{Document, OcrEngine, OcrModels, OcrOptions, PageOcr};

use crate::commands::{handlers, no_doc, select_tool};
use crate::files;
use crate::i18n::{t, tf};
use crate::state::{AppState, Dialog, DocTab, ToastKind, Tool};

/// How carefully pages are read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Quality {
    Fast,
    Normal,
    Accurate,
}

impl Quality {
    pub const ALL: [Quality; 3] = [Quality::Fast, Quality::Normal, Quality::Accurate];

    pub fn id(self) -> &'static str {
        match self {
            Quality::Fast => "fast",
            Quality::Normal => "normal",
            Quality::Accurate => "accurate",
        }
    }

    pub fn from_id(id: &str) -> Quality {
        Quality::ALL
            .into_iter()
            .find(|q| q.id() == id)
            .unwrap_or(Quality::Normal)
    }

    fn options(self) -> OcrOptions {
        match self {
            Quality::Fast => OcrOptions {
                dpi: 150.0,
                search_side: 960,
            },
            Quality::Normal => OcrOptions::default(),
            Quality::Accurate => OcrOptions {
                dpi: 300.0,
                search_side: 1792,
            },
        }
    }
}

/// A reading under way.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct OcrJob {
    pub running: bool,
    /// Pages read, and pages to read.
    pub done: usize,
    pub total: usize,
}

pub static JOB: GlobalSignal<OcrJob> = Signal::global(OcrJob::default);

/// The text read in an area, to copy or to write on the page.
pub static AREA: GlobalSignal<Option<(u64, PageOcr)>> = Signal::global(|| None);

static STOP: AtomicBool = AtomicBool::new(false);
static ENGINE: Mutex<Option<Arc<OcrEngine>>> = Mutex::new(None);

/// The folders searched for the models, the preferred first: the user's
/// own, the program's, the source tree (when run from there).
pub fn model_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(d) = crate::state::app_data_dir() {
        dirs.push(d.join("ocr"));
    }
    if let Some(exe_dir) = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(PathBuf::from))
    {
        dirs.push(exe_dir.join("ocr"));
        // The resources of a macOS application bundle, a Linux installation.
        dirs.push(exe_dir.join("../Resources/ocr"));
        dirs.push(exe_dir.join("../share/folitecta/ocr"));
    }
    dirs.push(PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../ocr")));
    dirs
}

/// The models, when they are there.
pub fn models() -> Option<OcrModels> {
    find_models(&model_dirs())
}

/// The engine, loaded the first time (call away from the interface thread).
fn engine() -> Result<Arc<OcrEngine>, String> {
    let mut slot = ENGINE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(e) = slot.as_ref() {
        return Ok(e.clone());
    }
    let models =
        models().ok_or_else(|| "the text recognition models are missing (ocr folder)".to_string())?;
    let e = Arc::new(OcrEngine::load(&models).map_err(|e| e.to_string())?);
    *slot = Some(e.clone());
    Ok(e)
}

fn with_tab<T>(st: AppState, id: u64, f: impl FnOnce(&mut DocTab) -> T) -> Option<T> {
    let mut docs = st.docs_mut();
    let tab = docs.iter_mut().find(|t| t.id == id)?;
    let out = f(tab);
    tab.refresh();
    Some(out)
}

/// Read `pages` of the active document and write their text over them.
/// Pages that have text of their own are left out with `skip_text`; with
/// `pictures_only` only the text inside the pictures of the pages is kept.
pub fn run(st: AppState, pages: Vec<usize>, quality: Quality, skip_text: bool, pictures_only: bool) {
    if JOB.peek().running {
        return;
    }
    let Some((tab, bytes, todo, pictures)) = st.read_active(|t| {
        // The pictures of a page, as shown (points, y down).
        let pictures_of = |p: usize| -> Vec<[f32; 4]> {
            let Some(size) = t.page_sizes.get(p) else {
                return Vec::new();
            };
            t.doc
                .page_images(p)
                .map(|l| l.iter().map(|im| pdf_rect_to_display(size, im.rect)).collect())
                .unwrap_or_default()
        };
        let mut todo = Vec::new();
        let mut pictures = Vec::new();
        for p in pages.iter().copied().filter(|p| *p < t.doc.page_count()) {
            if pictures_only {
                let boxes = pictures_of(p);
                if !boxes.is_empty() {
                    todo.push(p);
                    pictures.push(boxes);
                }
            } else if !skip_text || t.doc.has_ocr_text(p) || t.doc.page_text(p).trim().is_empty() {
                todo.push(p);
                pictures.push(Vec::new());
            }
        }
        (t.id, t.doc.bytes(), todo, pictures)
    }) else {
        return;
    };
    let lang = st.lang();
    if todo.is_empty() {
        st.toast(ToastKind::Info, t(lang, "toast.ocr_nothing"), "");
        return;
    }
    STOP.store(false, Ordering::Relaxed);
    *JOB.write() = OcrJob {
        running: true,
        done: 0,
        total: todo.len(),
    };
    let opts = quality.options();
    dioxus::core::spawn_forever(async move {
        let mut results: Vec<PageOcr> = Vec::new();
        let mut failure = None;
        for (i, page) in todo.iter().copied().enumerate() {
            if STOP.load(Ordering::Relaxed) {
                break;
            }
            let b = bytes.clone();
            let read = tokio::task::spawn_blocking(move || {
                engine().and_then(|e| e.read_page(b, page, opts).map_err(|e| e.to_string()))
            })
            .await;
            match read {
                Ok(Ok(mut r)) => {
                    if pictures_only {
                        let boxes = &pictures[i];
                        let scale = r.scale;
                        r.lines.retain(|l| {
                            let (cx, cy) = (
                                l.quad.iter().map(|p| p[0]).sum::<f32>() / 4.0 / scale,
                                l.quad.iter().map(|p| p[1]).sum::<f32>() / 4.0 / scale,
                            );
                            boxes
                                .iter()
                                .any(|b| cx >= b[0] && cx <= b[2] && cy >= b[1] && cy <= b[3])
                        });
                    }
                    results.push(r);
                }
                Ok(Err(e)) => {
                    failure = Some(e);
                    break;
                }
                Err(e) => {
                    failure = Some(e.to_string());
                    break;
                }
            }
            JOB.write().done = i + 1;
        }
        let stopped = STOP.load(Ordering::Relaxed);
        let lines: usize = results.iter().map(|r| r.lines.len()).sum();
        let written = if results.is_empty() {
            Some(Ok(0))
        } else {
            with_tab(st, tab, |t| t.doc.add_ocr_text(&results))
        };
        *JOB.write() = OcrJob::default();
        if matches!(*st.dialog.peek(), Dialog::Ocr { .. }) {
            st.set_dialog(Dialog::None);
        }
        if let Some(e) = failure {
            st.error(e);
        }
        match written {
            Some(Ok(pages)) => {
                files::set_window_title(st);
                let key = if stopped {
                    "toast.ocr_stopped"
                } else {
                    "toast.ocr_done"
                };
                st.toast(
                    ToastKind::Ok,
                    tf(
                        lang,
                        key,
                        &[("pages", &pages.to_string()), ("lines", &lines.to_string())],
                    ),
                    "",
                );
            }
            Some(Err(e)) => st.error(e),
            None => {}
        }
    });
}

/// Stop the reading after the page being read.
pub fn stop() {
    STOP.store(true, Ordering::Relaxed);
}

/// Read an area of a page (display points, y down) and show its text.
pub fn read_area(st: AppState, page: usize, rect: [f32; 4]) {
    let Some((tab, bytes)) = st.read_active(|t| (t.id, t.doc.bytes())) else {
        return;
    };
    st.set_busy(true);
    spawn(async move {
        let read = tokio::task::spawn_blocking(move || {
            engine().and_then(|e| {
                e.read_page_area(bytes, page, Some(rect), Quality::Accurate.options())
                    .map_err(|e| e.to_string())
            })
        })
        .await;
        st.set_busy(false);
        match read {
            Ok(Ok(r)) if r.lines.is_empty() => {
                st.toast(ToastKind::Info, t(st.lang(), "toast.ocr_no_text"), "")
            }
            Ok(Ok(r)) => {
                let text = r
                    .lines
                    .iter()
                    .map(|l| l.text.as_str())
                    .collect::<Vec<_>>()
                    .join("\n");
                *AREA.write() = Some((tab, r));
                st.set_dialog(Dialog::OcrText { text });
            }
            Ok(Err(e)) => st.error(e),
            Err(e) => st.error(e),
        }
    });
}

/// Write the text of the area read last on its page.
pub fn write_area(st: AppState) {
    let Some((tab, r)) = AREA.peek().clone() else {
        return;
    };
    match with_tab(st, tab, |t| t.doc.append_ocr_text(&r)) {
        Some(Ok(_)) => {
            files::set_window_title(st);
            st.set_dialog(Dialog::None);
            st.toast(ToastKind::Ok, t(st.lang(), "toast.ocr_area_written"), "");
        }
        Some(Err(e)) => st.error(e),
        None => {}
    }
}

/// Choose pictures and make a document of them with their text read.
fn pictures(st: AppState) {
    spawn(async move {
        let Some(chosen) = files::image_dialog().set_title("OCR").pick_files().await else {
            return;
        };
        let images = files::read_files(chosen).await;
        match Document::from_images(images) {
            Ok(doc) => {
                let n = doc.page_count();
                files::open_document_tab(st, doc);
                run(st, (0..n).collect(), Quality::Normal, false, false);
            }
            Err(e) => st.error(e),
        }
    });
}

fn open(st: AppState, pictures_only: bool) {
    if no_doc(st) {
        return;
    }
    st.set_dialog(Dialog::Ocr { pictures_only });
}

handlers! {
    "document.OCRPages" => |st| open(st, false),
    "document.ocrImages" => |st| open(st, true),
    "document.OCRSelRgn" => |st| select_tool(st, Tool::OcrRegion),
    "OCRImage" => pictures,
}
