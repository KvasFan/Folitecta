//! Recompression of pictures (FOL-L045): of the whole document, some pages
//! or one picture; JPEG at a quality, pictures scaled down to a resolution
//! where they are drawn largest (or to a number of pixels); presets for the
//! screen, e-books, print and archives. A picture is replaced only when the
//! result is smaller; the estimate tells, picture by picture, what would
//! change before anything does.

use std::collections::{HashMap, HashSet};

use lopdf::{Object, ObjectId, Stream};

use crate::doctools::stream_bytes;
use crate::document::Document;
use crate::error::{Error, Result};
use crate::util::deref;

/// How pictures are recompressed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RecompressSettings {
    /// JPEG quality, 10 to 100.
    pub quality: u8,
    /// Pictures drawn at more dots per inch than this are scaled down to it
    /// (0: never).
    pub max_dpi: f32,
    /// Pictures with a longer side of more pixels than this are scaled down
    /// to it (0: never).
    pub max_px: u32,
}

/// Ready settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecompressPreset {
    /// Reading on a screen: 96 dpi, quality 60.
    Screen,
    /// E-book readers and tablets: 150 dpi, quality 72.
    Ebook,
    /// Printing: 300 dpi, quality 85.
    Print,
    /// Keeping: the resolution kept, quality 92.
    Archive,
}

impl RecompressPreset {
    pub const ALL: [RecompressPreset; 4] = [
        RecompressPreset::Screen,
        RecompressPreset::Ebook,
        RecompressPreset::Print,
        RecompressPreset::Archive,
    ];

    pub fn id(self) -> &'static str {
        match self {
            RecompressPreset::Screen => "screen",
            RecompressPreset::Ebook => "ebook",
            RecompressPreset::Print => "print",
            RecompressPreset::Archive => "archive",
        }
    }

    pub fn from_id(id: &str) -> Option<RecompressPreset> {
        RecompressPreset::ALL.into_iter().find(|p| p.id() == id)
    }

    pub fn settings(self) -> RecompressSettings {
        let (quality, max_dpi) = match self {
            RecompressPreset::Screen => (60, 96.0),
            RecompressPreset::Ebook => (72, 150.0),
            RecompressPreset::Print => (85, 300.0),
            RecompressPreset::Archive => (92, 0.0),
        };
        RecompressSettings {
            quality,
            max_dpi,
            max_px: 0,
        }
    }
}

/// Which pictures.
#[derive(Debug, Clone, PartialEq)]
pub enum RecompressScope {
    All,
    Pages(Vec<usize>),
    /// The `index`-th picture drawn by page `page` (see `page_images`).
    Picture {
        page: usize,
        index: usize,
    },
}

/// Why a picture stays as it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kept {
    /// It has a mask or transparency.
    Mask,
    /// Its colours are not plain RGB or grey of 8 bits.
    Colors,
    /// Its data could not be read.
    Unreadable,
    /// The result would not be smaller.
    NotSmaller,
}

/// One picture looked at.
#[derive(Debug, Clone, PartialEq)]
pub struct RecompressItem {
    pub object: (u32, u16),
    /// The first page of the scope that draws it.
    pub page: Option<usize>,
    /// Bytes of its data, before and after (after: what it would weigh).
    pub before: u64,
    pub after: u64,
    /// Pixels, before and after.
    pub size_before: (u32, u32),
    pub size_after: (u32, u32),
    /// Its resolution where it is drawn largest (dots per inch).
    pub dpi: Option<f32>,
    /// Why it stays as it is; `None`: it is replaced.
    pub kept: Option<Kept>,
}

/// What recompression did, or would do.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RecompressReport {
    /// Pictures replaced (or that would be).
    pub images: usize,
    /// Bytes of those pictures, before and after.
    pub before: u64,
    pub after: u64,
    /// Every picture looked at.
    pub items: Vec<RecompressItem>,
}

/// The lowest resolution of each picture drawn by `pages` (where it is
/// drawn largest) and the first page drawing it.
fn drawn(doc: &Document, pages: &[usize]) -> HashMap<ObjectId, (usize, f32)> {
    let mut out: HashMap<ObjectId, (usize, f32)> = HashMap::new();
    for &page in pages {
        let Ok(images) = doc.page_images(page) else {
            continue;
        };
        for im in images.iter() {
            let (bottom, left) = im.sides();
            let inches = (bottom / 72.0, left / 72.0);
            if inches.0 <= 0.0 || inches.1 <= 0.0 {
                continue;
            }
            let dpi = (im.width as f64 / inches.0).min(im.height as f64 / inches.1) as f32;
            out.entry(im.object)
                .and_modify(|(_, d)| *d = d.min(dpi))
                .or_insert((page, dpi));
        }
    }
    out
}

impl Document {
    /// The pictures `scope` names, with the page that draws each.
    fn recompress_candidates(&self, scope: &RecompressScope) -> Result<Vec<ObjectId>> {
        let n = self.page_count();
        match scope {
            RecompressScope::Picture { page, index } => {
                let images = self.page_images(*page)?;
                let im = images
                    .iter()
                    .find(|im| im.index == *index)
                    .ok_or_else(|| Error::Invalid("no such picture on the page".into()))?;
                Ok(vec![im.object])
            }
            RecompressScope::All | RecompressScope::Pages(_) => {
                let pages: Vec<usize> = match scope {
                    RecompressScope::Pages(p) => {
                        if let Some(bad) = p.iter().find(|p| **p >= n) {
                            return Err(Error::InvalidPage(*bad));
                        }
                        p.clone()
                    }
                    _ => (0..n).collect(),
                };
                let ids = self.page_ids();
                let mut out: Vec<ObjectId> = Vec::new();
                let mut seen: HashSet<ObjectId> = HashSet::new();
                for &page in &pages {
                    // The pictures the page content draws, and those of the
                    // drawings it uses.
                    if let Ok(images) = self.page_images(page) {
                        for im in images.iter() {
                            if seen.insert(im.object) {
                                out.push(im.object);
                            }
                        }
                    }
                    if let Ok(list) = self.inner.get_page_images(ids[page]) {
                        for img in list {
                            if seen.insert(img.id) {
                                out.push(img.id);
                            }
                        }
                    }
                }
                Ok(out)
            }
        }
    }

    /// What recompressing would do, and the new pictures.
    fn plan_recompress(
        &self,
        scope: &RecompressScope,
        settings: RecompressSettings,
    ) -> Result<(RecompressReport, Vec<(ObjectId, Stream)>)> {
        let pages: Vec<usize> = match scope {
            RecompressScope::All => (0..self.page_count()).collect(),
            RecompressScope::Pages(p) => p.clone(),
            RecompressScope::Picture { page, .. } => vec![*page],
        };
        let candidates = self.recompress_candidates(scope)?;
        let drawn = drawn(self, &pages);
        // Masks are pictures of their own, never recompressed as such.
        let mut masks: HashSet<ObjectId> = HashSet::new();
        for obj in self.inner.objects.values() {
            if let Object::Stream(s) = obj {
                for key in [b"SMask".as_slice(), b"Mask"] {
                    if let Ok(Object::Reference(m)) = s.dict.get(key) {
                        masks.insert(*m);
                    }
                }
            }
        }
        let mut report = RecompressReport::default();
        let mut replacements = Vec::new();
        for id in candidates {
            if masks.contains(&id) {
                continue;
            }
            let Ok(Object::Stream(s)) = self.inner.get_object(id) else {
                continue;
            };
            let (page, dpi) = match drawn.get(&id) {
                Some((p, d)) => (Some(*p), Some(*d)),
                None => (None, None),
            };
            let (item, stream) = recompress_one(&self.inner, s, settings, dpi);
            let item = RecompressItem {
                object: id,
                page,
                ..item
            };
            if let Some(stream) = stream {
                report.images += 1;
                report.before += item.before;
                report.after += item.after;
                replacements.push((id, stream));
            }
            report.items.push(item);
        }
        Ok((report, replacements))
    }

    /// What recompressing `scope` with `settings` would do, picture by
    /// picture; nothing changes.
    pub fn estimate_recompress(
        &self,
        scope: &RecompressScope,
        settings: RecompressSettings,
    ) -> Result<RecompressReport> {
        Ok(self.plan_recompress(scope, settings)?.0)
    }

    /// Recompress the pictures of `scope` with `settings`: each replaced only
    /// when the result is smaller; one undo step.
    pub fn recompress(
        &mut self,
        scope: &RecompressScope,
        settings: RecompressSettings,
    ) -> Result<RecompressReport> {
        let (report, replacements) = self.plan_recompress(scope, settings)?;
        if replacements.is_empty() {
            return Ok(report);
        }
        self.begin_edit();
        for (id, s) in replacements {
            self.inner.objects.insert(id, Object::Stream(s));
        }
        self.commit()?;
        Ok(report)
    }

    /// Recompress the pictures of some pages (all when `None`) as JPEG with
    /// `quality`, scaling down those larger than `max_px` pixels.
    pub fn recompress_images(
        &mut self,
        pages: Option<&[usize]>,
        quality: u8,
        max_px: Option<u32>,
    ) -> Result<RecompressReport> {
        let scope = match pages {
            Some(p) => RecompressScope::Pages(p.to_vec()),
            None => RecompressScope::All,
        };
        self.recompress(
            &scope,
            RecompressSettings {
                quality,
                max_dpi: 0.0,
                max_px: max_px.unwrap_or(0),
            },
        )
    }
}

/// One picture recompressed: what happens, and its new data when smaller.
fn recompress_one(
    doc: &lopdf::Document,
    s: &Stream,
    settings: RecompressSettings,
    dpi: Option<f32>,
) -> (RecompressItem, Option<Stream>) {
    let d = &s.dict;
    let int = |k: &[u8]| d.get(k).ok().and_then(|v| v.as_i64().ok());
    let (w, h) = (
        int(b"Width").unwrap_or(0) as u32,
        int(b"Height").unwrap_or(0) as u32,
    );
    let before = s.content.len() as u64;
    let mut item = RecompressItem {
        object: (0, 0),
        page: None,
        before,
        after: before,
        size_before: (w, h),
        size_after: (w, h),
        dpi,
        kept: None,
    };
    let keep = |mut item: RecompressItem, why: Kept| {
        item.kept = Some(why);
        (item, None)
    };
    if d.has(b"SMask")
        || d.has(b"Mask")
        || d.get(b"ImageMask").ok().and_then(|b| b.as_bool().ok()) == Some(true)
    {
        return keep(item, Kept::Mask);
    }
    if int(b"BitsPerComponent").unwrap_or(8) != 8 || d.has(b"Decode") {
        return keep(item, Kept::Colors);
    }
    let comps = match d
        .get(b"ColorSpace")
        .ok()
        .map(|c| deref(doc, c))
        .and_then(|c| c.as_name().ok())
    {
        Some(b"DeviceRGB") => 3,
        Some(b"DeviceGray") => 1,
        _ => return keep(item, Kept::Colors),
    };
    if w == 0 || h == 0 {
        return keep(item, Kept::Unreadable);
    }
    let filters: Vec<Vec<u8>> = match d.get(b"Filter") {
        Ok(Object::Name(n)) => vec![n.clone()],
        Ok(Object::Array(a)) => a
            .iter()
            .filter_map(|x| x.as_name().ok().map(|n| n.to_vec()))
            .collect(),
        _ => Vec::new(),
    };
    let decoded: Option<image::DynamicImage> = if filters.last().map(|f| f.as_slice()) == Some(b"DCTDecode") {
        (filters.len() == 1)
            .then(|| image::load_from_memory_with_format(&s.content, image::ImageFormat::Jpeg).ok())
            .flatten()
    } else {
        stream_bytes(s).and_then(|px| {
            let n = (w * h * comps) as usize;
            if px.len() < n {
                return None;
            }
            if comps == 3 {
                image::RgbImage::from_raw(w, h, px[..n].to_vec()).map(image::DynamicImage::ImageRgb8)
            } else {
                image::GrayImage::from_raw(w, h, px[..n].to_vec()).map(image::DynamicImage::ImageLuma8)
            }
        })
    };
    let Some(mut img) = decoded else {
        return keep(item, Kept::Unreadable);
    };
    // The smaller of the scales the resolution and the pixels ask for.
    let mut scale = 1.0f32;
    if let (Some(dpi), true) = (dpi, settings.max_dpi > 0.0) {
        // A little above the resolution asked for is left alone.
        if dpi > settings.max_dpi * 1.05 {
            scale = scale.min(settings.max_dpi / dpi);
        }
    }
    if settings.max_px > 0 && w.max(h) > settings.max_px {
        scale = scale.min(settings.max_px as f32 / w.max(h) as f32);
    }
    if scale < 1.0 {
        let nw = ((w as f32 * scale).round() as u32).max(1);
        let nh = ((h as f32 * scale).round() as u32).max(1);
        img = img.resize_exact(nw, nh, image::imageops::FilterType::Triangle);
    }
    let mut out = std::io::Cursor::new(Vec::new());
    let enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, settings.quality.clamp(10, 100));
    let ok = if comps == 3 {
        img.to_rgb8().write_with_encoder(enc).is_ok()
    } else {
        img.to_luma8().write_with_encoder(enc).is_ok()
    };
    if !ok {
        return keep(item, Kept::Unreadable);
    }
    let jpeg = out.into_inner();
    item.after = jpeg.len() as u64;
    item.size_after = (img.width(), img.height());
    if item.after >= before {
        item.after = before;
        item.size_after = (w, h);
        return keep(item, Kept::NotSmaller);
    }
    let mut dict = d.clone();
    dict.set("Filter", Object::Name(b"DCTDecode".to_vec()));
    dict.remove(b"DecodeParms");
    dict.set("Width", img.width() as i64);
    dict.set("Height", img.height() as i64);
    dict.set("Length", jpeg.len() as i64);
    let mut stream = Stream::new(dict, jpeg);
    stream.allows_compression = false;
    (item, Some(stream))
}
