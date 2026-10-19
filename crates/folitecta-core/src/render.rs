//! Page rasterization with hayro, plus a background render service.
//!
//! Pages shown on the screen travel as PNG files; the operations that need
//! the pixels of a page (rasterizing, redacting, finding margins, exporting)
//! take them from [`Renderer::render_rgba`] without a PNG in between.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;

use hayro::hayro_interpret::InterpreterSettings;
use hayro::hayro_syntax::Pdf;
use hayro::vello_cpu::color::palette::css::{TRANSPARENT, WHITE};
use hayro::vello_cpu::peniko::ImageAlphaType;
use hayro::vello_cpu::{Pixels, Pixmap};
use hayro::{RenderCache, RenderSettings};
use image::RgbaImage;

use crate::error::{Error, Result};

pub use crate::render_service::{Lane, RenderAnswer, RenderRequest, RenderService, RenderStats};

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

pub(crate) fn load_pdf(bytes: Arc<Vec<u8>>) -> Result<Pdf> {
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
    let (width, height, color, pixels) = pixels_of(pixmap);
    encode_pixels(width, height, color, &pixels, compression)
}

/// The pixels of a pixmap, not premultiplied: RGB when the page is opaque,
/// RGBA otherwise.
pub(crate) fn pixels_of(pixmap: Pixmap) -> (u32, u32, png::ColorType, Vec<u8>) {
    let (width, height) = (u32::from(pixmap.width()), u32::from(pixmap.height()));
    match pixmap.try_take_rgb8(ImageAlphaType::Alpha) {
        Pixels::Rgb8(p) => (width, height, png::ColorType::Rgb, p),
        Pixels::Rgba8(p) => (width, height, png::ColorType::Rgba, p),
    }
}

/// PNG bytes of RGB or RGBA pixels.
pub(crate) fn encode_pixels(
    width: u32,
    height: u32,
    color: png::ColorType,
    pixels: &[u8],
    compression: png::Compression,
) -> Result<Vec<u8>> {
    let failed = |e: png::EncodingError| Error::Render(format!("PNG encoding failed: {e}"));
    let mut data = Vec::with_capacity(pixels.len() / 8);
    let mut encoder = png::Encoder::new(&mut data, width, height);
    encoder.set_color(color);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_compression(compression);
    let mut writer = encoder.write_header().map_err(failed)?;
    writer.write_image_data(pixels).map_err(failed)?;
    writer.finish().map_err(failed)?;
    Ok(data)
}

pub(crate) fn render_pixmap<'a>(
    pdf: &'a Pdf,
    cache: &RenderCache<'a>,
    page: usize,
    scale: f32,
    opts: RenderOpts,
) -> Result<Pixmap> {
    render_pixmap_with(pdf, cache, page, scale, opts, 0)
}

/// Rasterize a page with `threads` threads of the rasterizer besides the
/// calling one (0: on the calling thread only). Every picture is made here
/// with these explicit settings: the rasterizer's own default would use as
/// many threads as the processors.
pub(crate) fn render_pixmap_with<'a>(
    pdf: &'a Pdf,
    cache: &RenderCache<'a>,
    page: usize,
    scale: f32,
    opts: RenderOpts,
    threads: u16,
) -> Result<Pixmap> {
    use hayro::hayro_interpret::TransformExt;
    use hayro::vello_cpu::{RasterizerSettings, RenderContext, Resources, TargetInit};
    let annotations = opts.annotations;
    let interpreter = InterpreterSettings {
        render_annotations: annotations,
        ..InterpreterSettings::default()
    };
    let pages = pdf.pages();
    let p = pages.get(page).ok_or(Error::InvalidPage(page))?;
    let (w, h) = p.render_dimensions();
    let scale = scale.max(0.05).min(MAX_EDGE / w.max(h).max(1.0));
    let background = if opts.transparent { TRANSPARENT } else { WHITE };
    let result = catch_unwind(AssertUnwindSafe(|| {
        // As `hayro::render` does, with the settings of the rasterizer given.
        let settings = hayro::vello_cpu::RenderSettings {
            num_threads: threads,
            ..Default::default()
        };
        let mut ctx = RenderContext::new_with((w * scale) as u16, (h * scale) as u16, settings);
        let transform = hayro::kurbo::Affine::scale(scale as f64) * p.initial_transform(true).to_kurbo();
        hayro::render_into(
            p,
            cache,
            &interpreter,
            &RenderSettings::default(),
            &mut ctx,
            transform,
        );
        ctx.flush();
        let mut pixmap = Pixmap::new(ctx.width(), ctx.height());
        ctx.render_with(
            &mut pixmap,
            &mut Resources::default(),
            RasterizerSettings {
                target_init: TargetInit::Clear(background),
                ..Default::default()
            },
        );
        pixmap
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
