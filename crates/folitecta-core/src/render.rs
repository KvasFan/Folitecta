//! Page rasterization with hayro, plus a background render service.

use std::collections::HashMap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread::JoinHandle;

use hayro::hayro_interpret::InterpreterSettings;
use hayro::hayro_syntax::Pdf;
use hayro::vello_cpu::color::palette::css::{TRANSPARENT, WHITE};
use hayro::{PixmapSettings, RenderCache, RenderSettings, render};

use crate::error::{Error, Result};

/// Largest pixmap edge we are willing to allocate.
const MAX_EDGE: f32 = 12_000.0;

/// A rendered page as PNG bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderedPage {
    pub png: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

/// Synchronous renderer over one PDF snapshot.
pub struct Renderer {
    pdf: Pdf,
}

fn load_pdf(bytes: Arc<Vec<u8>>) -> Result<Pdf> {
    Pdf::new(bytes).map_err(|e| Error::Render(format!("cannot parse document for rendering: {e:?}")))
}

/// Rendering options.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RenderOpts {
    /// Draw annotation appearances.
    pub annotations: bool,
    /// Leave the page background transparent instead of white.
    pub transparent: bool,
}

impl Default for RenderOpts {
    fn default() -> Self {
        RenderOpts {
            annotations: true,
            transparent: false,
        }
    }
}

fn render_with<'a>(
    pdf: &'a Pdf,
    cache: &RenderCache<'a>,
    page: usize,
    scale: f32,
    annotations: bool,
) -> Result<RenderedPage> {
    render_with_opts(
        pdf,
        cache,
        page,
        scale,
        RenderOpts {
            annotations,
            transparent: false,
        },
    )
}

fn render_with_opts<'a>(
    pdf: &'a Pdf,
    cache: &RenderCache<'a>,
    page: usize,
    scale: f32,
    opts: RenderOpts,
) -> Result<RenderedPage> {
    let annotations = opts.annotations;
    let interpreter = InterpreterSettings {
        render_annotations: annotations,
        ..InterpreterSettings::default()
    };
    let pages = pdf.pages();
    let p = pages.get(page).ok_or(Error::InvalidPage(page))?;
    let (w, h) = p.render_dimensions();
    let scale = scale.max(0.05).min(MAX_EDGE / w.max(h).max(1.0));
    let settings = PixmapSettings {
        x_scale: scale,
        y_scale: scale,
        bg_color: if opts.transparent { TRANSPARENT } else { WHITE },
    };
    let result = catch_unwind(AssertUnwindSafe(|| {
        render(p, cache, &interpreter, &RenderSettings::default(), &settings)
    }));
    let pixmap = result.map_err(|_| Error::Render(format!("the renderer failed on page {}", page + 1)))?;
    let (width, height) = (pixmap.width() as u32, pixmap.height() as u32);
    let png = pixmap
        .into_png()
        .map_err(|e| Error::Render(format!("PNG encoding failed: {e}")))?;
    Ok(RenderedPage { png, width, height })
}

impl Renderer {
    pub fn new(bytes: Arc<Vec<u8>>) -> Result<Self> {
        Ok(Renderer {
            pdf: load_pdf(bytes)?,
        })
    }

    pub fn page_count(&self) -> usize {
        self.pdf.pages().len()
    }

    /// Display size (points, after rotation) of every page.
    pub fn page_sizes(&self) -> Vec<(f32, f32)> {
        self.pdf.pages().iter().map(|p| p.render_dimensions()).collect()
    }

    /// Render `page` (zero-based) at `scale` (1.0 = 72 dpi).
    pub fn render_page(&self, page: usize, scale: f32) -> Result<RenderedPage> {
        self.render_page_opts(page, scale, true)
    }

    /// Render with control over annotation appearance streams.
    pub fn render_page_opts(&self, page: usize, scale: f32, annotations: bool) -> Result<RenderedPage> {
        let cache = RenderCache::new();
        render_with(&self.pdf, &cache, page, scale, annotations)
    }

    /// Render with every option (a transparent page among them).
    pub fn render_page_with(&self, page: usize, scale: f32, opts: RenderOpts) -> Result<RenderedPage> {
        let cache = RenderCache::new();
        render_with_opts(&self.pdf, &cache, page, scale, opts)
    }
}

type Reply<T> = Box<dyn FnOnce(T) + Send + 'static>;

enum Msg {
    Replace(Arc<Vec<u8>>),
    Render {
        page: usize,
        scale: f32,
        opts: RenderOpts,
        reply: Reply<Result<RenderedPage>>,
    },
    Sizes(Reply<Result<Vec<(f32, f32)>>>),
    Stop,
}

/// A dedicated render thread that owns the parsed document and a render cache.
pub struct RenderService {
    tx: Sender<Msg>,
    handle: Option<JoinHandle<()>>,
}

fn scale_key(scale: f32) -> u32 {
    (scale * 1000.0).round() as u32
}

fn worker(rx: Receiver<Msg>, initial: Arc<Vec<u8>>) {
    let mut bytes = initial;
    'outer: loop {
        let pdf = load_pdf(bytes.clone());
        let cache = RenderCache::new();
        let mut memo: HashMap<(usize, u32, RenderOpts), Arc<RenderedPage>> = HashMap::new();
        loop {
            let Ok(msg) = rx.recv() else { return };
            match msg {
                Msg::Stop => return,
                Msg::Replace(new_bytes) => {
                    bytes = new_bytes;
                    continue 'outer;
                }
                Msg::Sizes(reply) => reply(
                    pdf.as_ref()
                        .map(|p| p.pages().iter().map(|pg| pg.render_dimensions()).collect())
                        .map_err(|e| Error::Render(e.to_string())),
                ),
                Msg::Render {
                    page,
                    scale,
                    opts,
                    reply,
                } => {
                    let key = (page, scale_key(scale), opts);
                    if let Some(hit) = memo.get(&key) {
                        reply(Ok(hit.as_ref().clone()));
                        continue;
                    }
                    let result = match &pdf {
                        Ok(pdf) => render_with_opts(pdf, &cache, page, scale, opts),
                        Err(e) => Err(Error::Render(e.to_string())),
                    };
                    if let Ok(rendered) = &result {
                        if memo.len() >= 48 {
                            memo.clear();
                        }
                        memo.insert(key, Arc::new(rendered.clone()));
                    }
                    reply(result);
                }
            }
        }
    }
}

impl RenderService {
    /// Start a render thread for the given document bytes.
    pub fn new(bytes: Arc<Vec<u8>>) -> Self {
        let (tx, rx) = channel();
        let handle = std::thread::Builder::new()
            .name("folitecta-render".into())
            .spawn(move || worker(rx, bytes))
            .expect("spawn render thread");
        RenderService {
            tx,
            handle: Some(handle),
        }
    }

    /// Replace the document bytes (after an edit). Cached renders are dropped.
    pub fn replace(&self, bytes: Arc<Vec<u8>>) {
        let _ = self.tx.send(Msg::Replace(bytes));
    }

    /// Render asynchronously; `reply` runs on the render thread.
    pub fn render(&self, page: usize, scale: f32, reply: impl FnOnce(Result<RenderedPage>) + Send + 'static) {
        self.render_ext(page, scale, RenderOpts::default(), reply);
    }

    /// Render asynchronously with explicit options.
    pub fn render_ext(
        &self,
        page: usize,
        scale: f32,
        opts: RenderOpts,
        reply: impl FnOnce(Result<RenderedPage>) + Send + 'static,
    ) {
        let _ = self.tx.send(Msg::Render {
            page,
            scale,
            opts,
            reply: Box::new(reply),
        });
    }

    /// Render and wait for the result.
    pub fn render_blocking(&self, page: usize, scale: f32) -> Result<RenderedPage> {
        let (tx, rx) = channel();
        self.render(page, scale, move |r| {
            let _ = tx.send(r);
        });
        rx.recv()
            .map_err(|_| Error::Render("render thread stopped".into()))?
    }

    /// Page display sizes as seen by the renderer.
    pub fn page_sizes_blocking(&self) -> Result<Vec<(f32, f32)>> {
        let (tx, rx) = channel();
        let _ = self.tx.send(Msg::Sizes(Box::new(move |r| {
            let _ = tx.send(r);
        })));
        rx.recv()
            .map_err(|_| Error::Render("render thread stopped".into()))?
    }
}

impl Drop for RenderService {
    fn drop(&mut self) {
        let _ = self.tx.send(Msg::Stop);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}
