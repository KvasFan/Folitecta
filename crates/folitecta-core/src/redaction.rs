//! Redaction: mark areas, then apply them: every page with marks is drawn as
//! a picture, the marked areas are covered (filled with a colour, blurred or
//! pixelated) and an overlay text (a redaction code, for example) is written
//! over them, so the content underneath cannot be recovered.

use std::collections::BTreeMap;

use image::RgbaImage;
use lopdf::{Dictionary, Object, ObjectId, Stream, dictionary};

use crate::document::Document;
use crate::error::{Error, Result};
use crate::marks::pdf_rect_to_display;
use crate::render::{RenderOpts, Renderer};
use crate::util::{deref, fmt_num, helvetica_width, rect_of, winansi_byte};

/// How a redacted area is covered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Overlay {
    /// Filled with a colour.
    #[default]
    Standard,
    Blur,
    BlurExtra,
    Pixelate,
    PixelateExtra,
}

impl Overlay {
    pub const ALL: [Overlay; 5] = [
        Overlay::Standard,
        Overlay::Blur,
        Overlay::BlurExtra,
        Overlay::Pixelate,
        Overlay::PixelateExtra,
    ];

    fn key(self) -> Option<&'static str> {
        match self {
            Overlay::Standard => None,
            Overlay::Blur => Some("Blur"),
            Overlay::BlurExtra => Some("BlurExtra"),
            Overlay::Pixelate => Some("Pixelate"),
            Overlay::PixelateExtra => Some("PixelateExtra"),
        }
    }

    fn from_key(k: &[u8]) -> Overlay {
        match k {
            b"Blur" => Overlay::Blur,
            b"BlurExtra" => Overlay::BlurExtra,
            b"Pixelate" => Overlay::Pixelate,
            b"PixelateExtra" => Overlay::PixelateExtra,
            _ => Overlay::Standard,
        }
    }
}

/// Standard fonts of overlay texts: name and the resource name used in /DA.
pub const OVERLAY_FONTS: &[(&str, &str)] = &[
    ("Helvetica", "Helv"),
    ("Helvetica-Bold", "HeBo"),
    ("Times-Roman", "TiRo"),
    ("Times-Bold", "TiBo"),
    ("Courier", "Cour"),
    ("Courier-Bold", "CoBo"),
];

/// How a redacted area looks once applied.
#[derive(Debug, Clone, PartialEq)]
pub struct RedactionLook {
    pub overlay: Overlay,
    /// Colour of a standard cover.
    pub fill: [f32; 3],
    /// Text written over the area (a redaction code, for example).
    pub text: String,
    /// One of [`OVERLAY_FONTS`].
    pub font: String,
    /// Font size; 0 makes the text fit the area.
    pub font_size: f32,
    pub text_color: [f32; 3],
    /// The text fills the area, repeated.
    pub repeat: bool,
}

impl Default for RedactionLook {
    fn default() -> Self {
        RedactionLook {
            overlay: Overlay::Standard,
            fill: [0.0, 0.0, 0.0],
            text: String::new(),
            font: "Helvetica".into(),
            font_size: 0.0,
            text_color: [1.0, 1.0, 1.0],
            repeat: false,
        }
    }
}

fn color_of(o: Option<&Object>) -> Option<[f32; 3]> {
    let a = o?.as_array().ok()?;
    let v: Vec<f32> = a
        .iter()
        .filter_map(|x| x.as_float().ok().or_else(|| x.as_i64().ok().map(|i| i as f32)))
        .collect();
    (v.len() == 3).then(|| [v[0], v[1], v[2]])
}

fn color_obj(c: [f32; 3]) -> Object {
    Object::Array(c.iter().map(|v| Object::Real(*v)).collect())
}

/// The look written on a redaction mark.
fn write_look(d: &mut Dictionary, look: &RedactionLook) {
    d.set("IC", color_obj(look.fill));
    match look.overlay.key() {
        Some(k) => d.set("FoliOverlay", Object::Name(k.as_bytes().to_vec())),
        None => {
            d.remove(b"FoliOverlay");
        }
    }
    if look.text.is_empty() {
        d.remove(b"OverlayText");
    } else {
        d.set("OverlayText", lopdf::text_string(&look.text));
    }
    d.set("Repeat", look.repeat);
    let res = OVERLAY_FONTS
        .iter()
        .find(|(n, _)| *n == look.font)
        .map(|(_, r)| *r)
        .unwrap_or("Helv");
    let c = look.text_color;
    d.set(
        "DA",
        Object::string_literal(format!(
            "/{res} {} Tf {} {} {} rg",
            fmt_num(look.font_size.max(0.0)),
            fmt_num(c[0]),
            fmt_num(c[1]),
            fmt_num(c[2])
        )),
    );
}

/// The look read from a redaction mark.
fn read_look(doc: &lopdf::Document, d: &Dictionary) -> RedactionLook {
    let mut look = RedactionLook {
        fill: color_of(d.get(b"IC").ok().map(|o| deref(doc, o))).unwrap_or([0.0, 0.0, 0.0]),
        overlay: d
            .get(b"FoliOverlay")
            .ok()
            .and_then(|o| o.as_name().ok())
            .map(Overlay::from_key)
            .unwrap_or_default(),
        repeat: d
            .get(b"Repeat")
            .ok()
            .and_then(|o| o.as_bool().ok())
            .unwrap_or(false),
        text: crate::util::dict_text(doc, d, b"OverlayText").unwrap_or_default(),
        ..RedactionLook::default()
    };
    if let Some(da) = d.get(b"DA").ok().and_then(|o| o.as_str().ok()) {
        let da = String::from_utf8_lossy(da).into_owned();
        let toks: Vec<&str> = da.split_whitespace().collect();
        for (i, t) in toks.iter().enumerate() {
            if *t == "Tf" && i >= 2 {
                let res = toks[i - 2].trim_start_matches('/');
                if let Some((name, _)) = OVERLAY_FONTS.iter().find(|(_, r)| *r == res) {
                    look.font = name.to_string();
                }
                look.font_size = toks[i - 1].parse().unwrap_or(0.0);
            }
            if *t == "rg" && i >= 3 {
                let v: Vec<f32> = toks[i - 3..i].iter().filter_map(|x| x.parse().ok()).collect();
                if v.len() == 3 {
                    look.text_color = [v[0], v[1], v[2]];
                }
            }
        }
    }
    look
}

/// Width of a text at 1 point in a standard font.
fn text_width(font: &str, text: &str) -> f32 {
    let n = text.chars().count() as f32;
    let w = if font.starts_with("Courier") {
        0.6 * n
    } else if font.starts_with("Times") {
        helvetica_width(text, 1.0) * 0.92
    } else {
        helvetica_width(text, 1.0)
    };
    if font.ends_with("Bold") { w * 1.06 } else { w }
}

/// Content drawing an overlay text inside `r` (PDF space of the drawn page).
fn overlay_text(look: &RedactionLook, r: [f32; 4]) -> String {
    let text = look.text.trim();
    let (w, h) = (r[2] - r[0], r[3] - r[1]);
    if text.is_empty() || w < 1.0 || h < 1.0 {
        return String::new();
    }
    let unit = text_width(&look.font, text).max(0.1);
    let size = if look.font_size > 0.0 {
        look.font_size
    } else if look.repeat {
        (h * 0.7).clamp(4.0, 12.0)
    } else {
        (h * 0.7).min(w * 0.92 / unit).max(2.0)
    };
    let bytes: Vec<u8> = text.chars().map(winansi_byte).collect();
    let hex: String = bytes.iter().map(|b| format!("{b:02X}")).collect();
    let f = fmt_num;
    let c = look.text_color;
    let mut out = format!(
        "q {} {} {} {} re W n BT /FoliRed {} Tf {} {} {} rg ",
        f(r[0]),
        f(r[1]),
        f(w),
        f(h),
        f(size),
        f(c[0]),
        f(c[1]),
        f(c[2])
    );
    let line_w = unit * size;
    if look.repeat {
        // Rows of the text from the top, as many as fit.
        let gap = size * 0.6;
        let step_x = line_w + gap;
        let step_y = size * 1.25;
        let mut y = r[3] - size;
        let mut row = 0;
        while y > r[1] - size && row < 400 {
            // Rows shift by half a word, like a pattern.
            let mut x = r[0] - if row % 2 == 1 { step_x / 2.0 } else { 0.0 };
            while x < r[2] {
                out.push_str(&format!("1 0 0 1 {} {} Tm <{hex}> Tj ", f(x), f(y)));
                x += step_x;
            }
            y -= step_y;
            row += 1;
        }
    } else {
        let x = r[0] + (w - line_w) / 2.0;
        let y = r[1] + (h - size * 0.7) / 2.0;
        out.push_str(&format!("1 0 0 1 {} {} Tm <{hex}> Tj ", f(x), f(y)));
    }
    out.push_str("ET Q\n");
    out
}

/// Cover the pixels `[x0, y0, x1, y1)` of a drawn page as the look says.
fn cover(img: &mut RgbaImage, r: [u32; 4], look: &RedactionLook) {
    let (x0, y0, x1, y1) = (r[0], r[1], r[2], r[3]);
    if x1 <= x0 || y1 <= y0 {
        return;
    }
    let (w, h) = (x1 - x0, y1 - y0);
    match look.overlay {
        Overlay::Standard => {
            let q = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
            let px = image::Rgba([q(look.fill[0]), q(look.fill[1]), q(look.fill[2]), 255]);
            for y in y0..y1 {
                for x in x0..x1 {
                    img.put_pixel(x, y, px);
                }
            }
        }
        Overlay::Blur | Overlay::BlurExtra => {
            let sigma = if look.overlay == Overlay::Blur { 8.0 } else { 18.0 };
            let part = image::imageops::crop_imm(img, x0, y0, w, h).to_image();
            let blurred = image::imageops::blur(&part, sigma);
            image::imageops::replace(img, &blurred, x0 as i64, y0 as i64);
        }
        Overlay::Pixelate | Overlay::PixelateExtra => {
            let block = if look.overlay == Overlay::Pixelate { 12 } else { 26 };
            let mut by = y0;
            while by < y1 {
                let mut bx = x0;
                let bh = block.min(y1 - by);
                while bx < x1 {
                    let bw = block.min(x1 - bx);
                    let mut sum = [0u64; 3];
                    for y in by..by + bh {
                        for x in bx..bx + bw {
                            let p = img.get_pixel(x, y);
                            for c in 0..3 {
                                sum[c] += p[c] as u64;
                            }
                        }
                    }
                    let n = (bw * bh) as u64;
                    let avg = image::Rgba([(sum[0] / n) as u8, (sum[1] / n) as u8, (sum[2] / n) as u8, 255]);
                    for y in by..by + bh {
                        for x in bx..bx + bw {
                            img.put_pixel(x, y, avg);
                        }
                    }
                    bx += bw;
                }
                by += bh;
            }
        }
    }
}

impl Document {
    /// Mark a rectangle for redaction (a `Redact` annotation with a red outline).
    pub fn add_redaction(&mut self, page: usize, rect: [f32; 4], author: &str) -> Result<ObjectId> {
        self.add_redaction_with(page, rect, author, &RedactionLook::default())
    }

    /// Mark a rectangle for redaction with the look it takes once applied.
    pub fn add_redaction_with(
        &mut self,
        page: usize,
        rect: [f32; 4],
        author: &str,
        look: &RedactionLook,
    ) -> Result<ObjectId> {
        let page_id = self.page_id(page)?;
        self.begin_edit();
        let mut annot = self.base_annotation(page_id, "Redact", rect, author, "", [1.0, 0.0, 0.0]);
        write_look(&mut annot, look);
        self.finish_annotation(page_id, annot)
    }

    /// The look of the redaction mark `index` of a page (None when it is not one).
    pub fn redaction_look(&self, page: usize, index: usize) -> Option<RedactionLook> {
        let id = self.annotations(page).ok()?.get(index)?.id?;
        let d = self.inner.get_dictionary(id).ok()?;
        (d.get(b"Subtype").ok()?.as_name().ok()? == b"Redact").then(|| read_look(&self.inner, d))
    }

    /// Change the look of a redaction mark.
    pub fn set_redaction_look(&mut self, page: usize, index: usize, look: &RedactionLook) -> Result<()> {
        let id = self
            .annotations(page)?
            .get(index)
            .and_then(|a| a.id)
            .ok_or_else(|| Error::Invalid("the comment does not exist".into()))?;
        let is_redact = self
            .inner
            .get_dictionary(id)?
            .get(b"Subtype")
            .ok()
            .and_then(|s| s.as_name().ok())
            == Some(b"Redact".as_slice());
        if !is_redact {
            return Err(Error::Invalid("the comment is not a redaction mark".into()));
        }
        self.begin_edit();
        write_look(self.inner.get_dictionary_mut(id)?, look);
        self.commit()
    }

    /// Pending redaction marks as `(page, rect)`.
    pub fn pending_redactions(&self) -> Vec<(usize, [f32; 4])> {
        (0..self.page_count())
            .filter_map(|p| self.annotations(p).ok())
            .flatten()
            .filter(|a| a.subtype == "Redact")
            .map(|a| (a.page, a.rect))
            .collect()
    }

    /// Apply the pending redaction marks of the given pages (all pages when
    /// `None`): every page with marks is drawn as a picture at 200 dpi, the
    /// marked areas are covered as their marks say and their overlay texts
    /// are written over them; the marks go. Returns the number of areas.
    pub fn apply_redactions(&mut self, pages: Option<&[usize]>) -> Result<usize> {
        let mut by_page: BTreeMap<usize, Vec<([f32; 4], RedactionLook)>> = BTreeMap::new();
        for p in 0..self.page_count() {
            if pages.is_some_and(|sel| !sel.contains(&p)) {
                continue;
            }
            for a in self.annotations(p).unwrap_or_default() {
                if a.subtype != "Redact" {
                    continue;
                }
                let look = self.redaction_look(p, a.index).unwrap_or_default();
                by_page.entry(p).or_default().push((a.rect, look));
            }
        }
        if by_page.is_empty() {
            return Ok(0);
        }
        let count = by_page.values().map(Vec::len).sum();
        self.begin_edit();
        // The marks go first, so the page is drawn without them.
        for p in by_page.keys() {
            let page_id = self.page_id(*p)?;
            let existing = self.inner.get_dictionary(page_id)?.get(b"Annots").ok().cloned();
            let arr = match existing {
                Some(Object::Array(arr)) => arr,
                Some(Object::Reference(r)) => deref(&self.inner, &Object::Reference(r))
                    .as_array()
                    .cloned()
                    .unwrap_or_default(),
                _ => Vec::new(),
            };
            let mut keep = Vec::new();
            for a in arr {
                let is_redact = deref(&self.inner, &a)
                    .as_dict()
                    .ok()
                    .and_then(|d| d.get(b"Subtype").ok())
                    .and_then(|s| s.as_name().ok())
                    == Some(b"Redact");
                if is_redact {
                    if let Ok(r) = a.as_reference() {
                        self.inner.delete_object(r);
                    }
                } else {
                    keep.push(a);
                }
            }
            self.inner
                .get_dictionary_mut(page_id)?
                .set("Annots", Object::Array(keep));
        }
        self.commit()?;
        let renderer = Renderer::new(self.bytes())?;
        let scale = 200.0 / 72.0;
        for (p, marks) in by_page {
            let id = self.page_id(p)?;
            let size = self.page_size_of(id);
            let opts = RenderOpts {
                annotations: false,
                transparent: false,
            };
            let mut img = renderer.render_rgba(p, scale, opts)?;
            let (iw, ih) = img.dimensions();
            let (sx, sy) = (iw as f32 / size.width, ih as f32 / size.height);
            let mut texts = String::new();
            for (rect, look) in &marks {
                // Display space (y down) is the space of the drawn page.
                let d = pdf_rect_to_display(&size, *rect);
                let px = [
                    ((d[0] * sx).floor().max(0.0) as u32).min(iw),
                    ((d[1] * sy).floor().max(0.0) as u32).min(ih),
                    ((d[2] * sx).ceil().max(0.0) as u32).min(iw),
                    ((d[3] * sy).ceil().max(0.0) as u32).min(ih),
                ];
                cover(&mut img, px, look);
                // The page becomes upright and starts at the origin.
                let r = [d[0], size.height - d[3], d[2], size.height - d[1]];
                texts.push_str(&overlay_text(look, r));
            }
            let rgb = image::DynamicImage::ImageRgba8(img).to_rgb8();
            let mut jpeg = std::io::Cursor::new(Vec::new());
            rgb.write_with_encoder(image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 88))
                .map_err(|e| Error::Image(e.to_string()))?;
            let (w, h) = rgb.dimensions();
            let xid = self.inner.add_object(Stream::new(
                dictionary! {
                    "Type" => "XObject",
                    "Subtype" => "Image",
                    "Width" => w as i64,
                    "Height" => h as i64,
                    "ColorSpace" => "DeviceRGB",
                    "BitsPerComponent" => 8i64,
                    "Filter" => "DCTDecode",
                },
                jpeg.into_inner(),
            ));
            let f = fmt_num;
            let content = format!(
                "q {} 0 0 {} 0 0 cm /FoliRaster Do Q\n{texts}",
                f(size.width),
                f(size.height)
            );
            let mut resources = dictionary! { "XObject" => dictionary! { "FoliRaster" => xid } };
            if !texts.is_empty() {
                let font = marks
                    .iter()
                    .find(|(_, l)| !l.text.trim().is_empty())
                    .map(|(_, l)| l.font.clone())
                    .unwrap_or_else(|| "Helvetica".into());
                let fid = self.inner.add_object(dictionary! {
                    "Type" => "Font",
                    "Subtype" => "Type1",
                    "BaseFont" => Object::Name(font.into_bytes()),
                    "Encoding" => "WinAnsiEncoding",
                });
                resources.set("Font", dictionary! { "FoliRed" => fid });
            }
            {
                let page = self.inner.get_dictionary_mut(id)?;
                page.set(
                    "MediaBox",
                    Object::Array(vec![0.into(), 0.into(), size.width.into(), size.height.into()]),
                );
                page.remove(b"CropBox");
                page.remove(b"Rotate");
                page.set("Resources", Object::Dictionary(resources));
                if size.rotation != 0 {
                    page.remove(b"Annots");
                }
            }
            self.inner.change_page_content(id, content.into_bytes())?;
        }
        self.inner.prune_objects();
        self.commit()?;
        Ok(count)
    }

    /// Rectangles of redaction marks on a page (for display).
    pub fn redaction_rects(&self, page: usize) -> Vec<[f32; 4]> {
        let Ok(page_id) = self.page_id(page) else {
            return Vec::new();
        };
        let Ok(dict) = self.inner.get_dictionary(page_id) else {
            return Vec::new();
        };
        dict.get(b"Annots")
            .ok()
            .map(|a| deref(&self.inner, a))
            .and_then(|a| a.as_array().ok())
            .map(|arr| {
                arr.iter()
                    .filter_map(|a| deref(&self.inner, a).as_dict().ok())
                    .filter(|d| d.get(b"Subtype").ok().and_then(|s| s.as_name().ok()) == Some(b"Redact"))
                    .filter_map(|d| d.get(b"Rect").ok().and_then(|r| rect_of(&self.inner, r)))
                    .collect()
            })
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn looks_are_written_and_read_back() {
        let look = RedactionLook {
            overlay: Overlay::PixelateExtra,
            fill: [0.2, 0.4, 0.6],
            text: "(b)(6)".into(),
            font: "Courier-Bold".into(),
            font_size: 9.0,
            text_color: [1.0, 0.0, 0.0],
            repeat: true,
        };
        let mut d = Dictionary::new();
        write_look(&mut d, &look);
        let doc = lopdf::Document::with_version("1.7");
        let back = read_look(&doc, &d);
        assert_eq!(back.overlay, look.overlay);
        assert_eq!(back.text, look.text);
        assert_eq!(back.font, look.font);
        assert_eq!(back.font_size, 9.0);
        assert!(back.repeat);
        assert!((back.fill[1] - 0.4).abs() < 1e-4 && (back.text_color[0] - 1.0).abs() < 1e-4);
    }

    #[test]
    fn covers() {
        let mut img = RgbaImage::from_fn(40, 40, |x, _| image::Rgba([(x * 6) as u8, 0, 0, 255]));
        let solid = RedactionLook {
            fill: [0.0, 0.0, 1.0],
            ..Default::default()
        };
        cover(&mut img, [0, 0, 10, 10], &solid);
        assert_eq!(img.get_pixel(5, 5).0, [0, 0, 255, 255]);
        assert_eq!(img.get_pixel(15, 5)[0], 90);
        let pix = RedactionLook {
            overlay: Overlay::Pixelate,
            ..Default::default()
        };
        cover(&mut img, [12, 0, 24, 12], &pix);
        // One block: every pixel the same.
        assert_eq!(img.get_pixel(12, 0), img.get_pixel(23, 11));
        let text = overlay_text(
            &RedactionLook {
                text: "CODE".into(),
                ..Default::default()
            },
            [10.0, 10.0, 110.0, 30.0],
        );
        assert!(
            text.contains("re W n") && text.contains("<434F4445> Tj"),
            "{text}"
        );
    }
}
