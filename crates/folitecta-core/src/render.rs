//! Page rasterization with hayro, plus a background render service.
//!
//! Pages shown on the screen travel as PNG files; the operations that need
//! the pixels of a page (rasterizing, redacting, finding margins, exporting)
//! take them from [`Renderer::render_rgba`] without a PNG in between.

use std::collections::HashMap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread::JoinHandle;

use hayro::hayro_interpret::InterpreterSettings;
use hayro::hayro_syntax::Pdf;
use hayro::vello_cpu::color::palette::css::{TRANSPARENT, WHITE};
use hayro::vello_cpu::peniko::ImageAlphaType;
use hayro::vello_cpu::{Pixels, Pixmap};
use hayro::{PixmapSettings, RenderCache, RenderSettings, render};
use image::RgbaImage;

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
    let pixmap = render_pixmap(pdf, cache, page, scale, opts)?;
    let (width, height) = (u32::from(pixmap.width()), u32::from(pixmap.height()));
    let png = encode_png(pixmap, png::Compression::Fast)?;
    Ok(RenderedPage { png, width, height })
}

/// PNG bytes of a rendered page, opaque pages as RGB. `Fast` is the DEFLATE
/// of fdeflate, made for PNG: several times faster than the default level
/// for files a little larger, which suits pictures that only go to the
/// window or the clipboard.
fn encode_png(pixmap: Pixmap, compression: png::Compression) -> Result<Vec<u8>> {
    let (width, height) = (u32::from(pixmap.width()), u32::from(pixmap.height()));
    let (color, pixels) = match pixmap.try_take_rgb8(ImageAlphaType::Alpha) {
        Pixels::Rgb8(p) => (png::ColorType::Rgb, p),
        Pixels::Rgba8(p) => (png::ColorType::Rgba, p),
    };
    let failed = |e: png::EncodingError| Error::Render(format!("PNG encoding failed: {e}"));
    let mut data = Vec::with_capacity(pixels.len() / 8);
    let mut encoder = png::Encoder::new(&mut data, width, height);
    encoder.set_color(color);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_compression(compression);
    let mut writer = encoder.write_header().map_err(failed)?;
    writer.write_image_data(&pixels).map_err(failed)?;
    writer.finish().map_err(failed)?;
    Ok(data)
}

fn render_pixmap<'a>(
    pdf: &'a Pdf,
    cache: &RenderCache<'a>,
    page: usize,
    scale: f32,
    opts: RenderOpts,
) -> Result<Pixmap> {
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
    result.map_err(|_| Error::Render(format!("the renderer failed on page {}", page + 1)))
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

    /// The pixels of a rendered page (RGBA, not premultiplied), for the
    /// operations that work on them: no PNG is made and read back.
    pub fn render_rgba(&self, page: usize, scale: f32, opts: RenderOpts) -> Result<RgbaImage> {
        let cache = RenderCache::new();
        let pixmap = render_pixmap(&self.pdf, &cache, page, scale, opts)?;
        let (w, h) = (u32::from(pixmap.width()), u32::from(pixmap.height()));
        RgbaImage::from_raw(w, h, pixmap.take_rgba8(ImageAlphaType::Alpha))
            .ok_or_else(|| Error::Render(format!("the picture of page {} has no pixels", page + 1)))
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

/// What the render threads of a document did since they started.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RenderStats {
    /// Pictures asked for.
    pub requests: u64,
    /// Pictures drawn.
    pub rendered: u64,
    /// Pictures answered from those kept, without drawing.
    pub reused: u64,
    /// Requests answered without a picture because it was no longer wanted.
    pub dropped: u64,
    /// Time spent drawing and encoding, in microseconds.
    pub render_us: u64,
}

#[derive(Default)]
struct StatCounters {
    requests: AtomicU64,
    rendered: AtomicU64,
    reused: AtomicU64,
    dropped: AtomicU64,
    render_us: AtomicU64,
}

impl StatCounters {
    fn bump(counter: &AtomicU64, n: u64) {
        counter.fetch_add(n, Ordering::Relaxed);
    }

    fn snapshot(&self) -> RenderStats {
        RenderStats {
            requests: self.requests.load(Ordering::Relaxed),
            rendered: self.rendered.load(Ordering::Relaxed),
            reused: self.reused.load(Ordering::Relaxed),
            dropped: self.dropped.load(Ordering::Relaxed),
            render_us: self.render_us.load(Ordering::Relaxed),
        }
    }
}

/// Render threads of one document. Each owns its own parse of the document,
/// render cache and memo of recent renders; a page always goes to the same
/// thread (page number modulo the threads), so the pages on the screen are
/// drawn side by side and each page still finds its cache.
pub struct RenderService {
    workers: Vec<(Sender<Msg>, Option<JoinHandle<()>>)>,
    stats: Arc<StatCounters>,
}

/// Render threads per document: half the processors, from one to three.
fn worker_count() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(2)
        .div_ceil(2)
        .clamp(1, 3)
}

fn scale_key(scale: f32) -> u32 {
    (scale * 1000.0).round() as u32
}

fn worker(rx: Receiver<Msg>, initial: Arc<Vec<u8>>, stats: Arc<StatCounters>) {
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
                        StatCounters::bump(&stats.reused, 1);
                        reply(Ok(hit.as_ref().clone()));
                        continue;
                    }
                    let started = std::time::Instant::now();
                    let result = match &pdf {
                        Ok(pdf) => render_with_opts(pdf, &cache, page, scale, opts),
                        Err(e) => Err(Error::Render(e.to_string())),
                    };
                    StatCounters::bump(&stats.rendered, 1);
                    StatCounters::bump(&stats.render_us, started.elapsed().as_micros() as u64);
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
    /// Start the render threads for the given document bytes.
    pub fn new(bytes: Arc<Vec<u8>>) -> Self {
        Self::with_workers(bytes, worker_count())
    }

    /// Start `n` render threads (at least one).
    pub fn with_workers(bytes: Arc<Vec<u8>>, n: usize) -> Self {
        let stats = Arc::new(StatCounters::default());
        let workers = (0..n.max(1))
            .map(|i| {
                let (tx, rx) = channel();
                let bytes = bytes.clone();
                let stats = stats.clone();
                let handle = std::thread::Builder::new()
                    .name(format!("folitecta-render-{i}"))
                    .spawn(move || worker(rx, bytes, stats))
                    .expect("spawn render thread");
                (tx, Some(handle))
            })
            .collect();
        RenderService { workers, stats }
    }

    /// What the render threads did since they started.
    pub fn stats(&self) -> RenderStats {
        self.stats.snapshot()
    }

    fn send(&self, page: usize, msg: Msg) {
        let _ = self.workers[page % self.workers.len()].0.send(msg);
    }

    /// Replace the document bytes (after an edit). Cached renders are dropped.
    pub fn replace(&self, bytes: Arc<Vec<u8>>) {
        for (tx, _) in &self.workers {
            let _ = tx.send(Msg::Replace(bytes.clone()));
        }
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
        StatCounters::bump(&self.stats.requests, 1);
        self.send(
            page,
            Msg::Render {
                page,
                scale,
                opts,
                reply: Box::new(reply),
            },
        );
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
        self.send(
            0,
            Msg::Sizes(Box::new(move |r| {
                let _ = tx.send(r);
            })),
        );
        rx.recv()
            .map_err(|_| Error::Render("render thread stopped".into()))?
    }
}

impl Drop for RenderService {
    fn drop(&mut self) {
        for (tx, _) in &self.workers {
            let _ = tx.send(Msg::Stop);
        }
        for (_, handle) in &mut self.workers {
            if let Some(h) = handle.take() {
                let _ = h.join();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::Document;
    use crate::textdoc::TextLayout;

    /// A document of pages full of text.
    fn text_pages(pages: usize) -> Arc<Vec<u8>> {
        let para = "Folitecta renders, annotates and organizes PDF documents. \
                    The quick brown fox jumps over the lazy dog. "
            .repeat(12);
        let text = vec![para; pages * 6].join("\n\n");
        Document::from_text(&text, TextLayout::default()).unwrap().bytes()
    }

    #[test]
    fn every_render_thread_draws_the_same_pages() {
        let bytes = text_pages(4);
        let pages = Renderer::new(bytes.clone()).unwrap().page_count();
        assert!(pages >= 4, "{pages}");
        let one = RenderService::with_workers(bytes.clone(), 1);
        let three = RenderService::with_workers(bytes.clone(), 3);
        for page in 0..pages {
            assert_eq!(
                one.render_blocking(page, 0.5).unwrap(),
                three.render_blocking(page, 0.5).unwrap(),
                "page {page}"
            );
        }
        assert_eq!(three.page_sizes_blocking().unwrap().len(), pages);
        // After an edit every thread draws the new document.
        let shorter = text_pages(1);
        let fewer = Renderer::new(shorter.clone()).unwrap().page_count();
        three.replace(shorter);
        assert_eq!(three.page_sizes_blocking().unwrap().len(), fewer);
        assert!(three.render_blocking(pages - 1, 0.5).is_err() || fewer >= pages);
        assert!(three.render_blocking(0, 0.5).is_ok());
    }

    #[test]
    fn the_pixels_are_those_of_the_png() {
        let r = Renderer::new(text_pages(1)).unwrap();
        for transparent in [false, true] {
            let opts = RenderOpts {
                annotations: true,
                transparent,
            };
            let png = r.render_page_with(0, 1.0, opts).unwrap();
            let decoded = image::load_from_memory(&png.png).unwrap().to_rgba8();
            let direct = r.render_rgba(0, 1.0, opts).unwrap();
            assert_eq!((png.width, png.height), direct.dimensions());
            assert!(decoded == direct, "transparent {transparent}");
        }
    }

    /// How long the PNG of a full page takes at each level:
    /// `cargo test --release -p folitecta-core png_levels -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn png_levels() {
        let bytes = text_pages(1);
        let pdf = load_pdf(bytes).unwrap();
        let cache = RenderCache::new();
        for (name, level) in [
            ("balanced", png::Compression::Balanced),
            ("fast", png::Compression::Fast),
        ] {
            let mut total = std::time::Duration::ZERO;
            let mut size = 0;
            for _ in 0..5 {
                let pixmap = render_pixmap(&pdf, &cache, 0, 2.0, RenderOpts::default()).unwrap();
                let start = std::time::Instant::now();
                size = encode_png(pixmap, level).unwrap().len();
                total += start.elapsed();
            }
            eprintln!(
                "{name}: {:.1} ms, {} KiB",
                total.as_secs_f64() * 200.0,
                size / 1024
            );
        }
    }
}
