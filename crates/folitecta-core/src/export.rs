//! Export: page images, text and embedded images.

use std::collections::HashSet;
use std::io::Cursor;
use std::path::{Path, PathBuf};

use lopdf::Object;

use crate::document::Document;
use crate::error::{Error, Result};
use crate::render::{RenderedPage, Renderer};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageFormat {
    Png,
    Jpeg { quality: u8 },
}

impl ImageFormat {
    pub fn extension(&self) -> &'static str {
        match self {
            ImageFormat::Png => "png",
            ImageFormat::Jpeg { .. } => "jpg",
        }
    }
}

fn encode(rendered: &RenderedPage, format: ImageFormat) -> Result<Vec<u8>> {
    match format {
        ImageFormat::Png => Ok(rendered.png.clone()),
        ImageFormat::Jpeg { quality } => {
            let img = image::load_from_memory(&rendered.png)
                .map_err(|e| Error::Image(e.to_string()))?
                .to_rgb8();
            let mut out = Cursor::new(Vec::new());
            let mut enc =
                image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, quality.clamp(10, 100));
            enc.encode_image(&img).map_err(|e| Error::Image(e.to_string()))?;
            Ok(out.into_inner())
        }
    }
}

/// Re-encode PNG bytes as a baseline JPEG (RGB) of the given quality.
pub(crate) fn png_to_jpeg(png: &[u8], quality: u8) -> Result<Vec<u8>> {
    let img = image::load_from_memory(png)
        .map_err(|e| Error::Image(e.to_string()))?
        .to_rgb8();
    let mut out = Cursor::new(Vec::new());
    let mut enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, quality.clamp(10, 100));
    enc.encode_image(&img).map_err(|e| Error::Image(e.to_string()))?;
    Ok(out.into_inner())
}

impl Document {
    /// Render a page at `scale` (1.0 = 72 dpi) to PNG.
    pub fn render_page_png(&self, page: usize, scale: f32) -> Result<RenderedPage> {
        Renderer::new(self.bytes())?.render_page(page, scale)
    }

    /// Render a rectangular area of a page (display points, origin top-left,
    /// y down) at `scale` and return it as PNG.
    pub fn render_region_png(&self, page: usize, rect: [f32; 4], scale: f32) -> Result<Vec<u8>> {
        let rendered = self.render_page_png(page, scale)?;
        let img = image::load_from_memory(&rendered.png).map_err(|e| Error::Image(e.to_string()))?;
        let (w, h) = (img.width(), img.height());
        if w == 0 || h == 0 {
            return Err(Error::Image("empty page image".into()));
        }
        let x0 = ((rect[0].min(rect[2]) * scale).floor().max(0.0) as u32).min(w - 1);
        let y0 = ((rect[1].min(rect[3]) * scale).floor().max(0.0) as u32).min(h - 1);
        let x1 = ((rect[0].max(rect[2]) * scale).ceil().max(0.0) as u32).clamp(x0 + 1, w);
        let y1 = ((rect[1].max(rect[3]) * scale).ceil().max(0.0) as u32).clamp(y0 + 1, h);
        let crop = img.crop_imm(x0, y0, x1 - x0, y1 - y0);
        let mut out = Cursor::new(Vec::new());
        crop.write_to(&mut out, image::ImageFormat::Png)
            .map_err(|e| Error::Image(e.to_string()))?;
        Ok(out.into_inner())
    }

    /// Export one page as an image file.
    pub fn export_page_image(
        &self,
        page: usize,
        dpi: f32,
        format: ImageFormat,
        path: impl AsRef<Path>,
    ) -> Result<()> {
        let rendered = self.render_page_png(page, dpi.max(18.0) / 72.0)?;
        let data = encode(&rendered, format)?;
        std::fs::write(path.as_ref(), data).map_err(|e| Error::io(path.as_ref(), e))
    }

    /// Export several pages as `prefix-NNN.ext` files inside `dir`.
    pub fn export_pages_images(
        &self,
        pages: &[usize],
        dir: impl AsRef<Path>,
        dpi: f32,
        format: ImageFormat,
        prefix: &str,
    ) -> Result<Vec<PathBuf>> {
        let dir = dir.as_ref();
        std::fs::create_dir_all(dir).map_err(|e| Error::io(dir, e))?;
        let renderer = Renderer::new(self.bytes())?;
        let mut out = Vec::new();
        for &p in pages {
            let rendered = renderer.render_page(p, dpi.max(18.0) / 72.0)?;
            let data = encode(&rendered, format)?;
            let path = dir.join(format!("{prefix}-{:03}.{}", p + 1, format.extension()));
            std::fs::write(&path, data).map_err(|e| Error::io(&path, e))?;
            out.push(path);
        }
        Ok(out)
    }

    /// Write the extracted text of every page to a UTF-8 file.
    pub fn export_text(&self, path: impl AsRef<Path>) -> Result<()> {
        let mut text = String::new();
        for (i, page) in self.all_text().into_iter().enumerate() {
            if i > 0 {
                text.push_str("\n\n");
            }
            text.push_str(&page);
        }
        std::fs::write(path.as_ref(), text).map_err(|e| Error::io(path.as_ref(), e))
    }

    /// Extract embedded raster images of the given pages into `dir`.
    /// JPEG and JPEG 2000 streams are written as-is; uncompressed/Flate RGB and
    /// gray images are converted to PNG. Other encodings are skipped.
    pub fn extract_images(&self, pages: &[usize], dir: impl AsRef<Path>) -> Result<Vec<PathBuf>> {
        let dir = dir.as_ref();
        std::fs::create_dir_all(dir).map_err(|e| Error::io(dir, e))?;
        let mut seen = HashSet::new();
        let mut out = Vec::new();
        for &p in pages {
            let page_id = self.page_id(p)?;
            let Ok(images) = self.inner.get_page_images(page_id) else {
                continue;
            };
            for img in images {
                if !seen.insert(img.id) {
                    continue;
                }
                let filters: Vec<String> = img.filters.clone().unwrap_or_default();
                let base = dir.join(format!("page{:03}-img{}", p + 1, img.id.0));
                let path = if filters.iter().any(|f| f == "DCTDecode") {
                    let path = base.with_extension("jpg");
                    std::fs::write(&path, img.content).map_err(|e| Error::io(&path, e))?;
                    path
                } else if filters.iter().any(|f| f == "JPXDecode") {
                    let path = base.with_extension("jp2");
                    std::fs::write(&path, img.content).map_err(|e| Error::io(&path, e))?;
                    path
                } else {
                    let Ok(Object::Stream(stream)) = self.inner.get_object(img.id) else {
                        continue;
                    };
                    let Ok(data) = stream.decompressed_content() else {
                        continue;
                    };
                    let (w, h) = (img.width.max(0) as u32, img.height.max(0) as u32);
                    let bpc = img.bits_per_component.unwrap_or(8);
                    let cs = img.color_space.clone().unwrap_or_default();
                    let buffer = match (cs.as_str(), bpc) {
                        ("DeviceRGB", 8) if data.len() >= (w * h * 3) as usize => {
                            image::RgbImage::from_raw(w, h, data[..(w * h * 3) as usize].to_vec())
                                .map(image::DynamicImage::ImageRgb8)
                        }
                        ("DeviceGray", 8) if data.len() >= (w * h) as usize => {
                            image::GrayImage::from_raw(w, h, data[..(w * h) as usize].to_vec())
                                .map(image::DynamicImage::ImageLuma8)
                        }
                        ("DeviceGray", 1) if data.len() >= (w.div_ceil(8) * h) as usize => {
                            let row = w.div_ceil(8) as usize;
                            let mut pixels = Vec::with_capacity((w * h) as usize);
                            for y in 0..h as usize {
                                for x in 0..w as usize {
                                    let bit = (data[y * row + x / 8] >> (7 - (x % 8))) & 1;
                                    pixels.push(if bit == 1 { 255 } else { 0 });
                                }
                            }
                            image::GrayImage::from_raw(w, h, pixels).map(image::DynamicImage::ImageLuma8)
                        }
                        _ => None,
                    };
                    let Some(dyn_img) = buffer else { continue };
                    let path = base.with_extension("png");
                    dyn_img.save(&path).map_err(|e| Error::Image(e.to_string()))?;
                    path
                };
                out.push(path);
            }
        }
        Ok(out)
    }

    /// Count of embedded images per page.
    pub fn image_count(&self, page: usize) -> usize {
        self.page_id(page)
            .ok()
            .and_then(|id| self.inner.get_page_images(id).ok())
            .map(|v| v.len())
            .unwrap_or(0)
    }
}
