//! The preview of a page mark in its window (watermark, header and footer,
//! page and Bates numbers, background): the current page of a copy of the
//! document with the mark added, drawn on a worker thread a moment after the
//! last change, at the zoom of Page Marks › Zoom In and Zoom Out.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use dioxus::prelude::*;
use folitecta_core::marks::{MarkPosition, TextMarkOptions};
use folitecta_core::{Document, Result};

use crate::commands::dispatch;
use crate::i18n::t;
use crate::icons::Icon;
use crate::state::AppState;

/// The mark shown, with what the window sets.
#[derive(Debug, Clone, PartialEq)]
pub enum MarkJob {
    Watermark(TextMarkOptions),
    HeaderFooter {
        header: [String; 3],
        footer: [String; 3],
        size: f32,
        margin: f32,
    },
    /// `number` is the number the previewed page gets.
    Numbers {
        format: String,
        number: i64,
        position: MarkPosition,
        size: f32,
    },
    Background {
        color: [f32; 3],
        opacity: f32,
    },
    /// The page is not among the pages marked: shown as it is.
    Nothing,
}

impl MarkJob {
    fn apply(&self, doc: &mut Document, page: usize) -> Result<()> {
        match self {
            MarkJob::Watermark(o) => doc.add_text_watermark(&[page], o),
            MarkJob::HeaderFooter {
                header,
                footer,
                size,
                margin,
            } => doc.add_header_footer(
                &[page],
                [&header[0], &header[1], &header[2]],
                [&footer[0], &footer[1], &footer[2]],
                *size,
                *margin,
                [0.0, 0.0, 0.0],
            ),
            MarkJob::Numbers {
                format,
                number,
                position,
                size,
            } => doc.add_page_numbers(&[page], format, *number, *position, *size, 28.0, [0.0, 0.0, 0.0]),
            MarkJob::Background { color, opacity } => doc.add_background_color(&[page], *color, *opacity),
            MarkJob::Nothing => Ok(()),
        }
    }
}

/// The last preview drawn: its number and its picture.
static PICTURE: Mutex<Option<(u64, Arc<Vec<u8>>)>> = Mutex::new(None);
static LAST: AtomicU64 = AtomicU64::new(0);

/// The picture at `/markprev/{number}.png`, when it is the last one drawn.
pub fn serve(path: &str) -> Option<Arc<Vec<u8>>> {
    let n: u64 = path.rsplit('/').next()?.strip_suffix(".png")?.parse().ok()?;
    let p = PICTURE.lock().ok()?;
    p.as_ref().filter(|(m, _)| *m == n).map(|(_, png)| png.clone())
}

/// The zoom of the preview (Page Marks › Zoom In, Zoom Out): 0.25 to 2.
pub fn zoom(st: AppState) -> f32 {
    st.value("marks.preview_zoom")
        .parse()
        .unwrap_or(0.5f32)
        .clamp(0.25, 2.0)
}

pub fn zoom_by(st: AppState, by: f32) {
    st.set_value(
        "marks.preview_zoom",
        format!("{:.3}", (zoom(st) * by).clamp(0.25, 2.0)),
    );
}

#[component]
pub fn MarkPreview(job: MarkJob) -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let page = st.current_page();
    let z = zoom(st);
    let mut shown = use_signal(|| None::<u64>);
    let mut failed = use_signal(|| false);
    use_effect(use_reactive!(|job, page, z| {
        let n = LAST.fetch_add(1, Ordering::SeqCst) + 1;
        let bytes = st.read_active(|t| t.doc.bytes());
        spawn(async move {
            // A moment after the last change.
            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
            if LAST.load(Ordering::SeqCst) != n {
                return;
            }
            let Some(bytes) = bytes else { return };
            let drawn = tokio::task::spawn_blocking(move || -> Result<Vec<u8>> {
                let mut doc = Document::from_bytes(bytes.as_ref().clone(), None, None)?;
                job.apply(&mut doc, page)?;
                let r = folitecta_core::render::Renderer::new(doc.bytes())?;
                Ok(r.render_page(page, z)?.png)
            })
            .await;
            if LAST.load(Ordering::SeqCst) != n {
                return;
            }
            match drawn {
                Ok(Ok(png)) => {
                    if let Ok(mut p) = PICTURE.lock() {
                        *p = Some((n, Arc::new(png)));
                    }
                    failed.set(false);
                    shown.set(Some(n));
                }
                _ => failed.set(true),
            }
        });
    }));
    let pct = (z * 100.0).round();
    rsx! {
        div { class: "mark-preview",
            div { class: "mark-preview-bar",
                span { class: "muted", "{t(lang, \"dialog.mark_preview.title\")} · {t(lang, \"status.page\")} {page + 1}" }
                div { class: "spacer" }
                button { class: "ibtn", title: "{t(lang, \"cmd.pagemarks.zoomOut\")}", onclick: move |_| dispatch(st, "pagemarks.zoomOut"), Icon { name: "zoom-out" } }
                span { class: "muted mark-preview-zoom", "{pct}%" }
                button { class: "ibtn", title: "{t(lang, \"cmd.pagemarks.zoomIn\")}", onclick: move |_| dispatch(st, "pagemarks.zoomIn"), Icon { name: "zoom-in" } }
            }
            div { class: "mark-preview-page",
                if failed() {
                    p { class: "muted", "{t(lang, \"dialog.mark_preview.failed\")}" }
                } else if let Some(n) = shown() {
                    img { src: "/markprev/{n}.png", alt: "" }
                }
            }
        }
    }
}
