//! Page rasterization with hayro, plus a background render service.
//!
//! Pages shown on the screen travel as PNG files; the operations that need
//! the pixels of a page (rasterizing, redacting, finding margins, exporting)
//! take them from [`Renderer::render_rgba`] without a PNG in between.

use std::collections::HashMap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;
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

    /// A renderer of an encrypted file, opened with `password` (an
    /// independent check of protected files: the renderer decrypts them by
    /// itself).
    pub fn new_with_password(bytes: Arc<Vec<u8>>, password: &str) -> Result<Self> {
        let pdf = Pdf::new_with_password(bytes, password)
            .map_err(|e| Error::Render(format!("cannot parse document for rendering: {e:?}")))?;
        Ok(Renderer { pdf })
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

/// Rows of pixels drawn for a printer that receives pixels, one band of a
/// page at a time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Band {
    pub width: u32,
    pub rows: u32,
    /// Bytes per row (a multiple of 4).
    pub stride: usize,
    /// 24 (blue, green, red) or 8 (grey) bits per pixel.
    pub bits: u16,
    /// The rows from the top.
    pub data: Vec<u8>,
}

/// How many rows of `width` pixels a band may have so that drawing it stays
/// within `budget` bytes (about 7 bytes a pixel), from 64 (or the whole
/// height when it is smaller) to `height`.
pub fn band_rows(width: u32, height: u32, budget: usize) -> u32 {
    let height = height.max(1);
    let per_row = (width.max(1) as usize).saturating_mul(7);
    let rows = (budget / per_row).clamp(1, u32::MAX as usize) as u32;
    rows.clamp(64.min(height), height)
}

impl Renderer {
    /// `rows` rows of pixels of `page` from row `top`, `width` pixels wide,
    /// drawn at `dpi` (horizontal, vertical) on white. Pixel (0, 0) of the
    /// whole page is the point at `origin` pixels from its top left corner
    /// (the edge a printer cannot print on). Only the band is allocated, so
    /// a page of any size can be drawn at a high resolution.
    #[allow(clippy::too_many_arguments)]
    pub fn render_band(
        &self,
        page: usize,
        dpi: (f32, f32),
        origin: (f32, f32),
        width: u32,
        top: u32,
        rows: u32,
        gray: bool,
    ) -> Result<Band> {
        use hayro::hayro_interpret::util::TransformExt;
        use hayro::kurbo::Affine;
        use hayro::vello_cpu::{RasterizerSettings, RenderContext, TargetInit};

        if width == 0 || rows == 0 || width > u32::from(u16::MAX) || rows > u32::from(u16::MAX) {
            return Err(Error::Render(format!(
                "a band of {width} x {rows} pixels cannot be drawn"
            )));
        }
        let pages = self.pdf.pages();
        let p = pages.get(page).ok_or(Error::InvalidPage(page))?;
        let cache = RenderCache::new();
        let interpreter = InterpreterSettings::default();
        let transform = Affine::translate((-f64::from(origin.0), -(f64::from(origin.1) + f64::from(top))))
            * Affine::scale_non_uniform(f64::from(dpi.0) / 72.0, f64::from(dpi.1) / 72.0)
            * p.initial_transform(true).to_kurbo();
        let drawn = catch_unwind(AssertUnwindSafe(|| {
            let mut ctx = RenderContext::new(width as u16, rows as u16);
            hayro::render_into(
                p,
                &cache,
                &interpreter,
                &RenderSettings::default(),
                &mut ctx,
                transform,
            );
            ctx.flush();
            let mut pixmap = Pixmap::new(width as u16, rows as u16);
            ctx.render_with(
                &mut pixmap,
                &mut hayro::vello_cpu::Resources::default(),
                RasterizerSettings {
                    target_init: TargetInit::Clear(WHITE),
                    ..Default::default()
                },
            );
            pixmap
        }))
        .map_err(|_| Error::Render(format!("the renderer failed on page {}", page + 1)))?;
        let rgba = drawn.take_rgba8(ImageAlphaType::Alpha);
        let bits: u16 = if gray { 8 } else { 24 };
        let row_bytes = width as usize * usize::from(bits / 8);
        let stride = row_bytes.div_ceil(4) * 4;
        let mut data = vec![0u8; stride * rows as usize];
        for (y, src) in rgba.chunks_exact(width as usize * 4).enumerate() {
            let dst = &mut data[y * stride..y * stride + row_bytes];
            if gray {
                for (d, px) in dst.iter_mut().zip(src.as_chunks::<4>().0) {
                    let l = (u32::from(px[0]) * 299 + u32::from(px[1]) * 587 + u32::from(px[2]) * 114 + 500)
                        / 1000;
                    *d = l as u8;
                }
            } else {
                for (d, px) in dst.as_chunks_mut::<3>().0.iter_mut().zip(src.as_chunks::<4>().0) {
                    d[0] = px[2];
                    d[1] = px[1];
                    d[2] = px[0];
                }
            }
        }
        Ok(Band {
            width,
            rows,
            stride,
            bits,
            data,
        })
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

/// Render threads of one document. Each owns its own parse of the document,
/// render cache and memo of recent renders; a page always goes to the same
/// thread (page number modulo the threads), so the pages on the screen are
/// drawn side by side and each page still finds its cache.
pub struct RenderService {
    workers: Vec<(Sender<Msg>, Option<JoinHandle<()>>)>,
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
    /// Start the render threads for the given document bytes.
    pub fn new(bytes: Arc<Vec<u8>>) -> Self {
        Self::with_workers(bytes, worker_count())
    }

    /// Start `n` render threads (at least one).
    pub fn with_workers(bytes: Arc<Vec<u8>>, n: usize) -> Self {
        let workers = (0..n.max(1))
            .map(|i| {
                let (tx, rx) = channel();
                let bytes = bytes.clone();
                let handle = std::thread::Builder::new()
                    .name(format!("folitecta-render-{i}"))
                    .spawn(move || worker(rx, bytes))
                    .expect("spawn render thread");
                (tx, Some(handle))
            })
            .collect();
        RenderService { workers }
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

    #[test]
    fn bands_put_together_are_the_whole_page() {
        let r = Renderer::new(text_pages(1)).unwrap();
        let dpi = 100.0;
        let whole = r.render_rgba(0, dpi / 72.0, RenderOpts::default()).unwrap();
        let (w, h) = whole.dimensions();
        let step = band_rows(w, h, w as usize * 7 * 97);
        assert_eq!(step, 97);
        let mut top = 0;
        let mut worst = 0u8;
        while top < h {
            let rows = step.min(h - top);
            let band = r
                .render_band(0, (dpi, dpi), (0.0, 0.0), w, top, rows, false)
                .unwrap();
            assert_eq!(band.stride % 4, 0);
            for y in 0..rows {
                for x in 0..w {
                    let i = y as usize * band.stride + x as usize * 3;
                    let px = whole.get_pixel(x, top + y).0;
                    for (c, v) in [(2, px[0]), (1, px[1]), (0, px[2])] {
                        worst = worst.max(band.data[i + c].abs_diff(v));
                    }
                }
            }
            top += rows;
        }
        assert!(worst <= 2, "largest difference {worst}");
        let gray = r
            .render_band(0, (dpi, dpi), (10.0, 20.0), 33, 0, 5, true)
            .unwrap();
        assert_eq!((gray.bits, gray.stride, gray.data.len()), (8, 36, 36 * 5));
        assert!(
            r.render_band(0, (dpi, dpi), (0.0, 0.0), 70_000, 0, 1, false)
                .is_err()
        );
        assert_eq!(band_rows(1000, 30, 1), 30);
        assert_eq!(band_rows(1000, 5000, 1), 64);
        assert_eq!(band_rows(1000, 5000, 7_000_000), 1000);
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
