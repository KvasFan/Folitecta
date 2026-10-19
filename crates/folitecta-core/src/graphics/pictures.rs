//! Pictures drawn by the pages: where they are, their pixels, and an edited
//! picture put back in place of the old one.
//!
//! Only pictures drawn by the page content itself are found (not those
//! inside forms or written inline in the content).

use std::collections::HashMap;
use std::sync::Arc;

use hayro::kurbo::{Affine, Point, Rect};
use image::RgbaImage;
use image::imageops::FilterType;
use lopdf::{Dictionary, Object, ObjectId, Stream, dictionary};

use crate::content_edit::{Joined, Splice, count_refs, joined, name_token, rewrite, stream_span};
use crate::content_lex::{name_of, operations};
use crate::document::Document;
use crate::error::{Error, Result};
use crate::images::ImageObject;
use crate::render::{RenderOpts, Renderer};
use crate::util::deref;

/// Largest picture edge decoded through the renderer.
const MAX_EDGE: u32 = 12_000;
/// Largest picture decoded at all (pixels).
const MAX_PIXELS: u64 = 80_000_000;

/// How the colours of a picture are kept.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Colors {
    #[default]
    Color,
    Gray,
    /// One bit per pixel.
    BlackWhite,
    /// Up to 256 colours from a palette.
    Indexed,
}

/// A picture drawn by the content of a page.
#[derive(Debug, Clone, PartialEq)]
pub struct PageImage {
    /// Order among the pictures of the page.
    pub index: usize,
    /// Object of the picture.
    pub object: (u32, u16),
    /// Size in pixels.
    pub width: u32,
    pub height: u32,
    /// Where the unit square of the picture goes on the page (PDF user space).
    pub matrix: [f64; 6],
    /// Bounds on the page (PDF user space).
    pub rect: [f32; 4],
    /// Last filter of the data (`DCTDecode`, `FlateDecode`; empty when none).
    pub filter: String,
    pub colors: Colors,
    pub bits: u8,
}

impl PageImage {
    /// Lengths on the page of the bottom and left edges of the picture.
    pub fn sides(&self) -> (f64, f64) {
        let m = self.matrix;
        (m[0].hypot(m[1]), m[2].hypot(m[3]))
    }
}

/// Pictures of the pages already looked at, keyed by revision.
pub(crate) type ImageCache = Option<(u64, HashMap<usize, Arc<Vec<PageImage>>>)>;

/// One picture drawn with `Do` by the page content.
struct Draw {
    /// Byte range of the operation (name and operator) in the joined content.
    start: usize,
    end: usize,
    /// The name operand as written.
    name: Vec<u8>,
    id: ObjectId,
    ctm: Affine,
}

/// Resources of a page, inherited from its parents when needed.
fn page_resources(doc: &lopdf::Document, page_id: ObjectId) -> Dictionary {
    let mut cur = Some(page_id);
    for _ in 0..64 {
        let Some(id) = cur else { break };
        let Ok(d) = doc.get_dictionary(id) else { break };
        if let Ok(r) = d.get(b"Resources") {
            return deref(doc, r).as_dict().cloned().unwrap_or_default();
        }
        cur = d.get(b"Parent").ok().and_then(|p| p.as_reference().ok());
    }
    Dictionary::new()
}

fn page_xobjects(doc: &lopdf::Document, page_id: ObjectId) -> Dictionary {
    page_resources(doc, page_id)
        .get(b"XObject")
        .ok()
        .and_then(|o| deref(doc, o).as_dict().ok())
        .cloned()
        .unwrap_or_default()
}

fn number(data: &[u8], r: (usize, usize)) -> Option<f64> {
    std::str::from_utf8(data.get(r.0..r.1)?).ok()?.parse().ok()
}

fn is_image(doc: &lopdf::Document, id: ObjectId) -> bool {
    doc.get_object(id)
        .and_then(Object::as_stream)
        .ok()
        .and_then(|s| s.dict.get(b"Subtype").ok()?.as_name().ok())
        == Some(b"Image".as_slice())
}

/// The pictures drawn by the page content, in drawing order.
fn draws(doc: &lopdf::Document, page_id: ObjectId, data: &[u8]) -> Vec<Draw> {
    let xobjects = page_xobjects(doc, page_id);
    let mut out = Vec::new();
    let mut stack: Vec<Affine> = Vec::new();
    let mut ctm = Affine::IDENTITY;
    for op in operations(data) {
        match op.operator {
            b"q" => stack.push(ctm),
            b"Q" => {
                if let Some(m) = stack.pop() {
                    ctm = m;
                }
            }
            b"cm" if op.operands.len() == 6 => {
                let n: Vec<f64> = op.operands.iter().filter_map(|r| number(data, *r)).collect();
                if let [a, b, c, d, e, f] = n[..] {
                    ctm *= Affine::new([a, b, c, d, e, f]);
                }
            }
            b"Do" => {
                let Some(r) = op.operands.first() else { continue };
                let Some(name) = name_of(data, *r) else { continue };
                let Ok(Object::Reference(id)) = xobjects.get(&name) else {
                    continue;
                };
                if is_image(doc, *id) {
                    out.push(Draw {
                        start: op.start,
                        end: op.end,
                        name: data[r.0..r.1].to_vec(),
                        id: *id,
                        ctm,
                    });
                }
            }
            _ => {}
        }
    }
    out
}

fn int(d: &Dictionary, key: &[u8]) -> Option<i64> {
    d.get(key).ok()?.as_i64().ok()
}

fn flag(d: &Dictionary, key: &[u8]) -> bool {
    d.get(key).ok().and_then(|v| v.as_bool().ok()) == Some(true)
}

fn dims(d: &Dictionary) -> (u32, u32) {
    let side = |k: &[u8]| int(d, k).unwrap_or(0).clamp(0, u32::MAX as i64) as u32;
    (side(b"Width"), side(b"Height"))
}

fn filters(d: &Dictionary) -> Vec<Vec<u8>> {
    match d.get(b"Filter") {
        Ok(Object::Name(n)) => vec![n.clone()],
        Ok(Object::Array(a)) => a
            .iter()
            .filter_map(|x| x.as_name().ok().map(<[u8]>::to_vec))
            .collect(),
        _ => Vec::new(),
    }
}

fn info(doc: &lopdf::Document, index: usize, d: &Draw) -> Option<PageImage> {
    let s = doc.get_object(d.id).ok()?.as_stream().ok()?;
    let (width, height) = dims(&s.dict);
    let mask = flag(&s.dict, b"ImageMask");
    // JPEG 2000 pictures may leave the bits out: they are not one-bit pictures.
    let bits = int(&s.dict, b"BitsPerComponent")
        .unwrap_or(if mask { 1 } else { 8 })
        .clamp(1, 16) as u8;
    let jpx = filters(&s.dict).last().is_some_and(|f| f == b"JPXDecode");
    let colors = if mask {
        Colors::BlackWhite
    } else {
        match colour_space(doc, s.dict.get(b"ColorSpace").ok()) {
            Some(Cs::Gray) if bits == 1 && !jpx => Colors::BlackWhite,
            Some(Cs::Gray) => Colors::Gray,
            Some(Cs::Indexed { .. }) => Colors::Indexed,
            _ => Colors::Color,
        }
    };
    let b = d.ctm.transform_rect_bbox(Rect::new(0.0, 0.0, 1.0, 1.0));
    Some(PageImage {
        index,
        object: d.id,
        width,
        height,
        matrix: d.ctm.as_coeffs(),
        rect: [b.x0 as f32, b.y0 as f32, b.x1 as f32, b.y1 as f32],
        filter: filters(&s.dict)
            .last()
            .map(|f| String::from_utf8_lossy(f).into_owned())
            .unwrap_or_default(),
        colors,
        bits,
    })
}

// ---- Decoding ---------------------------------------------------------------

/// Colour spaces decoded without the renderer.
#[derive(Debug, Clone)]
enum Cs {
    Gray,
    Rgb,
    Cmyk,
    Indexed {
        base: Box<Cs>,
        lookup: Vec<u8>,
        hival: usize,
    },
}

impl Cs {
    fn comps(&self) -> usize {
        match self {
            Cs::Gray | Cs::Indexed { .. } => 1,
            Cs::Rgb => 3,
            Cs::Cmyk => 4,
        }
    }

    /// RGB of components between 0 and 1.
    fn rgb(&self, v: &[f32]) -> [u8; 3] {
        let q = |x: f32| (x.clamp(0.0, 1.0) * 255.0).round() as u8;
        match self {
            Cs::Gray => [q(v[0]); 3],
            Cs::Rgb => [q(v[0]), q(v[1]), q(v[2])],
            Cs::Cmyk => {
                let k = 1.0 - v[3];
                [q((1.0 - v[0]) * k), q((1.0 - v[1]) * k), q((1.0 - v[2]) * k)]
            }
            Cs::Indexed { .. } => [0; 3],
        }
    }
}

fn colour_space(doc: &lopdf::Document, obj: Option<&Object>) -> Option<Cs> {
    match deref(doc, obj?) {
        Object::Name(n) => match n.as_slice() {
            b"DeviceGray" | b"G" | b"CalGray" => Some(Cs::Gray),
            b"DeviceRGB" | b"RGB" | b"CalRGB" => Some(Cs::Rgb),
            b"DeviceCMYK" | b"CMYK" => Some(Cs::Cmyk),
            _ => None,
        },
        Object::Array(a) => match deref(doc, a.first()?).as_name().ok()? {
            b"ICCBased" => {
                let s = deref(doc, a.get(1)?).as_stream().ok()?;
                match int(&s.dict, b"N")? {
                    1 => Some(Cs::Gray),
                    3 => Some(Cs::Rgb),
                    4 => Some(Cs::Cmyk),
                    _ => None,
                }
            }
            b"CalGray" => Some(Cs::Gray),
            b"CalRGB" => Some(Cs::Rgb),
            b"Indexed" | b"I" => {
                let base = colour_space(doc, a.get(1))?;
                if matches!(base, Cs::Indexed { .. }) {
                    return None;
                }
                let hival = deref(doc, a.get(2)?).as_i64().ok()?.clamp(0, 255) as usize;
                let lookup = match deref(doc, a.get(3)?) {
                    Object::String(b, _) => b.clone(),
                    Object::Stream(s) => s.decompressed_content().ok()?,
                    _ => return None,
                };
                (lookup.len() >= (hival + 1) * base.comps()).then(|| Cs::Indexed {
                    base: Box::new(base),
                    lookup,
                    hival,
                })
            }
            _ => None,
        },
        _ => None,
    }
}

/// Sample `bpc` bits wide starting at bit `bit` of a row.
fn sample(line: &[u8], bit: usize, bpc: usize) -> u32 {
    match bpc {
        8 => line[bit / 8] as u32,
        16 => ((line[bit / 8] as u32) << 8) | line[bit / 8 + 1] as u32,
        _ => {
            let shift = 8 - bpc - bit % 8;
            (line[bit / 8] as u32 >> shift) & ((1 << bpc) - 1)
        }
    }
}

/// Pixels of uncompressed samples.
fn unpack(raw: &[u8], w: u32, h: u32, bpc: usize, cs: &Cs) -> Option<RgbaImage> {
    if !matches!(bpc, 1 | 2 | 4 | 8 | 16) {
        return None;
    }
    let comps = cs.comps();
    let (wu, hu) = (w as usize, h as usize);
    let row = (wu * comps * bpc).div_ceil(8);
    if raw.len() < row * hu {
        return None;
    }
    let mut out = RgbaImage::new(w, h);
    let px = out.as_mut();
    // The usual cases, copied byte by byte.
    if bpc == 8 && matches!(cs, Cs::Gray | Cs::Rgb) {
        for y in 0..hu {
            let line = &raw[y * row..y * row + wu * comps];
            for x in 0..wu {
                let o = (y * wu + x) * 4;
                let c = &line[x * comps..x * comps + comps];
                let (r, g, b) = if comps == 1 {
                    (c[0], c[0], c[0])
                } else {
                    (c[0], c[1], c[2])
                };
                px[o..o + 4].copy_from_slice(&[r, g, b, 255]);
            }
        }
        return Some(out);
    }
    let max = ((1u32 << bpc) - 1) as f32;
    let palette: Vec<[u8; 3]> = match cs {
        Cs::Indexed { base, lookup, hival } => {
            let n = base.comps();
            (0..=*hival)
                .map(|i| {
                    let v: Vec<f32> = lookup[i * n..i * n + n]
                        .iter()
                        .map(|b| *b as f32 / 255.0)
                        .collect();
                    base.rgb(&v)
                })
                .collect()
        }
        _ => Vec::new(),
    };
    let mut v = [0f32; 4];
    for y in 0..hu {
        let line = &raw[y * row..(y + 1) * row];
        for x in 0..wu {
            let rgb = if palette.is_empty() {
                for (k, slot) in v.iter_mut().enumerate().take(comps) {
                    *slot = sample(line, (x * comps + k) * bpc, bpc) as f32 / max;
                }
                cs.rgb(&v)
            } else {
                let i = sample(line, x * bpc, bpc) as usize;
                palette[i.min(palette.len() - 1)]
            };
            let o = (y * wu + x) * 4;
            px[o..o + 4].copy_from_slice(&[rgb[0], rgb[1], rgb[2], 255]);
        }
    }
    Some(out)
}

/// Pixels read straight from the data, for the usual encodings and colours.
fn decode_direct(doc: &lopdf::Document, s: &Stream, with_mask: bool) -> Option<RgbaImage> {
    let d = &s.dict;
    let (w, h) = dims(d);
    if w == 0
        || h == 0
        || (w as u64) * (h as u64) > MAX_PIXELS
        || flag(d, b"ImageMask")
        || d.has(b"Decode")
        || d.has(b"Mask")
        || matches!(d.get(b"DecodeParms"), Ok(Object::Array(_)))
    {
        return None;
    }
    let fs = filters(d);
    let cs = colour_space(doc, d.get(b"ColorSpace").ok())?;
    // Gray and RGB only: CMYK needs the colour management of the renderer
    // (and JPEG decoders guess at inverted CMYK).
    let plain = |c: &Cs| matches!(c, Cs::Gray | Cs::Rgb);
    let supported = match &cs {
        Cs::Indexed { base, .. } => plain(base),
        c => plain(c),
    };
    if !supported {
        return None;
    }
    let mut img = if fs.len() == 1 && fs[0] == b"DCTDecode" {
        if !plain(&cs) {
            return None;
        }
        image::load_from_memory_with_format(&s.content, image::ImageFormat::Jpeg)
            .ok()?
            .to_rgba8()
    } else {
        let known = [
            b"FlateDecode".as_slice(),
            b"LZWDecode",
            b"ASCII85Decode",
            b"ASCIIHexDecode",
            b"RunLengthDecode",
        ];
        if fs.iter().any(|f| !known.contains(&f.as_slice())) {
            return None;
        }
        let bpc = int(d, b"BitsPerComponent").unwrap_or(8);
        if !matches!(bpc, 1 | 2 | 4 | 8 | 16) {
            return None;
        }
        let bpc = bpc as usize;
        let need = (w as usize * cs.comps() * bpc).div_ceil(8) * h as usize;
        let raw = s
            .decompressed_content_with_limit(need.saturating_mul(2) + (1 << 20))
            .ok()?;
        unpack(&raw, w, h, bpc, &cs)?
    };
    if img.dimensions() != (w, h) {
        return None;
    }
    if with_mask {
        if let Ok(Object::Reference(mid)) = d.get(b"SMask") {
            let ms = doc.get_object(*mid).ok()?.as_stream().ok()?;
            if ms.dict.has(b"Matte") {
                return None;
            }
            let mut mask = decode_direct(doc, ms, false)?;
            if mask.dimensions() != (w, h) {
                mask = image::imageops::resize(&mask, w, h, FilterType::Triangle);
            }
            for (p, m) in img.pixels_mut().zip(mask.pixels()) {
                p[3] = m[0];
            }
        }
    }
    Some(img)
}

/// Copy an object and everything it refers to into another document.
fn copy_object(
    src: &lopdf::Document,
    dst: &mut lopdf::Document,
    id: ObjectId,
    map: &mut HashMap<ObjectId, ObjectId>,
) -> ObjectId {
    if let Some(n) = map.get(&id) {
        return *n;
    }
    let new_id = dst.new_object_id();
    map.insert(id, new_id);
    let obj = src.get_object(id).cloned().unwrap_or(Object::Null);
    let obj = copy_value(src, dst, obj, map);
    dst.objects.insert(new_id, obj);
    new_id
}

fn copy_dict(
    src: &lopdf::Document,
    dst: &mut lopdf::Document,
    d: Dictionary,
    map: &mut HashMap<ObjectId, ObjectId>,
) -> Dictionary {
    let mut out = Dictionary::new();
    for (k, v) in d.into_iter() {
        out.set(k, copy_value(src, dst, v, map));
    }
    out
}

fn copy_value(
    src: &lopdf::Document,
    dst: &mut lopdf::Document,
    obj: Object,
    map: &mut HashMap<ObjectId, ObjectId>,
) -> Object {
    match obj {
        Object::Reference(r) => Object::Reference(copy_object(src, dst, r, map)),
        Object::Array(a) => Object::Array(a.into_iter().map(|v| copy_value(src, dst, v, map)).collect()),
        Object::Dictionary(d) => Object::Dictionary(copy_dict(src, dst, d, map)),
        Object::Stream(s) => {
            let mut copy = Stream::new(copy_dict(src, dst, s.dict, map), s.content);
            copy.allows_compression = false;
            Object::Stream(copy)
        }
        other => other,
    }
}

/// Pixels of any picture the renderer can draw: the picture alone on a page
/// of its own size, drawn at one pixel per point on a transparent page.
fn decode_by_rendering(doc: &lopdf::Document, id: ObjectId, w: u32, h: u32) -> Result<RgbaImage> {
    if w > MAX_EDGE || h > MAX_EDGE {
        return Err(Error::Invalid("the picture is too large to edit".into()));
    }
    let mut tmp = lopdf::Document::with_version("1.7");
    let mut map = HashMap::new();
    let img = copy_object(doc, &mut tmp, id, &mut map);
    let content = tmp.add_object(Stream::new(
        dictionary! {},
        format!("q {w} 0 0 {h} 0 0 cm /Im Do Q").into_bytes(),
    ));
    let pages = tmp.new_object_id();
    let page = tmp.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages,
        "MediaBox" => vec![0.into(), 0.into(), (w as i64).into(), (h as i64).into()],
        "Contents" => content,
        "Resources" => dictionary! { "XObject" => dictionary! { "Im" => img } },
    });
    tmp.objects.insert(
        pages,
        Object::Dictionary(dictionary! { "Type" => "Pages", "Kids" => vec![page.into()], "Count" => 1i64 }),
    );
    let catalog = tmp.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages });
    tmp.trailer.set("Root", catalog);
    let bytes = crate::document::serialize(&mut tmp)?;
    let mut img = Renderer::new(Arc::new(bytes))?.render_rgba(
        0,
        1.0,
        RenderOpts {
            annotations: false,
            transparent: true,
        },
    )?;
    if img.dimensions() != (w, h) {
        img = image::imageops::resize(&img, w, h, FilterType::Triangle);
    }
    Ok(img)
}

/// The pixels of a picture object.
pub(crate) fn decode_image(doc: &lopdf::Document, id: ObjectId) -> Result<RgbaImage> {
    let s = doc.get_object(id)?.as_stream()?;
    let (w, h) = dims(&s.dict);
    if w == 0 || h == 0 {
        return Err(Error::Invalid("the picture has no size".into()));
    }
    match decode_direct(doc, s, true) {
        Some(img) => Ok(img),
        None => decode_by_rendering(doc, id, w, h),
    }
}

// ---- Encoding ---------------------------------------------------------------

fn luma(p: &image::Rgba<u8>) -> u8 {
    ((p[0] as u32 * 299 + p[1] as u32 * 587 + p[2] as u32 * 114 + 500) / 1000) as u8
}

fn jpeg(encode: impl FnOnce(&mut std::io::Cursor<Vec<u8>>) -> image::ImageResult<()>) -> Result<Vec<u8>> {
    let mut out = std::io::Cursor::new(Vec::new());
    encode(&mut out).map_err(|e| Error::Image(e.to_string()))?;
    Ok(out.into_inner())
}

/// A picture stream of `px` and its soft mask (when some pixels are not
/// opaque). JPEG keeps photographs small; black and white is one bit per pixel.
pub(crate) fn encode_pixels(
    px: &RgbaImage,
    colors: Colors,
    use_jpeg: bool,
) -> Result<(Stream, Option<Stream>)> {
    let (w, h) = px.dimensions();
    let mut dict = dictionary! {
        "Type" => "XObject",
        "Subtype" => "Image",
        "Width" => w as i64,
        "Height" => h as i64,
    };
    let quality = 92;
    let stream = match colors {
        Colors::BlackWhite => {
            let row = (w as usize).div_ceil(8);
            let mut bits = vec![0u8; row * h as usize];
            for (x, y, p) in px.enumerate_pixels() {
                if luma(p) >= 128 {
                    bits[y as usize * row + x as usize / 8] |= 0x80 >> (x % 8);
                }
            }
            dict.set("ColorSpace", "DeviceGray");
            dict.set("BitsPerComponent", 1i64);
            let mut s = Stream::new(dict, bits);
            s.compress()?;
            s
        }
        Colors::Indexed => {
            let mut fitted = px.clone();
            crate::imaging::quantize(&mut fitted, 256);
            let mut palette: Vec<[u8; 3]> = Vec::new();
            let mut index: HashMap<[u8; 3], u8> = HashMap::new();
            let mut data = Vec::with_capacity((w * h) as usize);
            for p in fitted.pixels() {
                let c = [p[0], p[1], p[2]];
                let i = match index.get(&c) {
                    Some(i) => *i,
                    None if palette.len() < 256 => {
                        palette.push(c);
                        let i = (palette.len() - 1) as u8;
                        index.insert(c, i);
                        i
                    }
                    // Never more than 256 colours after fitting; nearest otherwise.
                    None => 0,
                };
                data.push(i);
            }
            let lookup: Vec<u8> = palette.iter().flatten().copied().collect();
            dict.set(
                "ColorSpace",
                Object::Array(vec![
                    Object::Name(b"Indexed".to_vec()),
                    Object::Name(b"DeviceRGB".to_vec()),
                    Object::Integer(palette.len() as i64 - 1),
                    Object::String(lookup, lopdf::StringFormat::Hexadecimal),
                ]),
            );
            dict.set("BitsPerComponent", 8i64);
            let mut s = Stream::new(dict, data);
            s.compress()?;
            s
        }
        Colors::Gray => {
            let data: Vec<u8> = px.pixels().map(luma).collect();
            dict.set("ColorSpace", "DeviceGray");
            dict.set("BitsPerComponent", 8i64);
            if use_jpeg {
                let gray =
                    image::GrayImage::from_raw(w, h, data).ok_or_else(|| Error::Image("bad size".into()))?;
                let bytes = jpeg(|out| {
                    gray.write_with_encoder(image::codecs::jpeg::JpegEncoder::new_with_quality(out, quality))
                })?;
                dict.set("Filter", "DCTDecode");
                Stream::new(dict, bytes)
            } else {
                let mut s = Stream::new(dict, data);
                s.compress()?;
                s
            }
        }
        Colors::Color => {
            let rgb: Vec<u8> = px.pixels().flat_map(|p| [p[0], p[1], p[2]]).collect();
            dict.set("ColorSpace", "DeviceRGB");
            dict.set("BitsPerComponent", 8i64);
            if use_jpeg {
                let img =
                    image::RgbImage::from_raw(w, h, rgb).ok_or_else(|| Error::Image("bad size".into()))?;
                let bytes = jpeg(|out| {
                    img.write_with_encoder(image::codecs::jpeg::JpegEncoder::new_with_quality(out, quality))
                })?;
                dict.set("Filter", "DCTDecode");
                Stream::new(dict, bytes)
            } else {
                let mut s = Stream::new(dict, rgb);
                s.compress()?;
                s
            }
        }
    };
    let smask = if px.pixels().any(|p| p[3] < 255) {
        let alpha: Vec<u8> = px.pixels().map(|p| p[3]).collect();
        let mut m = Stream::new(
            dictionary! {
                "Type" => "XObject",
                "Subtype" => "Image",
                "Width" => w as i64,
                "Height" => h as i64,
                "ColorSpace" => "DeviceGray",
                "BitsPerComponent" => 8i64,
            },
            alpha,
        );
        m.compress()?;
        Some(m)
    } else {
        None
    };
    Ok((stream, smask))
}

// ---- Putting a picture back ---------------------------------------------------

/// A matrix for `cm`, precise enough for the unit square of a large picture.
fn matrix_text(m: Affine) -> String {
    m.as_coeffs()
        .iter()
        .map(|v| {
            if (v - v.round()).abs() < 1e-9 {
                format!("{}", v.round() as i64)
            } else {
                let s = format!("{v:.6}");
                s.trim_end_matches('0').trim_end_matches('.').to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn is_identity(m: Affine) -> bool {
    m.as_coeffs()
        .iter()
        .zip(Affine::IDENTITY.as_coeffs())
        .all(|(a, b)| (a - b).abs() < 1e-6)
}

/// Width over height of a drawing on the page (and whether it is mirrored,
/// as the sign).
fn sides_ratio(m: Affine) -> f64 {
    let c = m.as_coeffs();
    let (a, b) = (c[0].hypot(c[1]), c[2].hypot(c[3]));
    let sign = if m.determinant() < 0.0 { -1.0 } else { 1.0 };
    if b > 1e-9 { sign * a / b } else { sign }
}

/// A stencil mask of the opaque pixels (sample 0 paints).
fn encode_stencil(px: &RgbaImage) -> Result<Stream> {
    let (w, h) = px.dimensions();
    let row = (w as usize).div_ceil(8);
    let mut bits = vec![0u8; row * h as usize];
    for (x, y, p) in px.enumerate_pixels() {
        if p[3] < 128 {
            bits[y as usize * row + x as usize / 8] |= 0x80 >> (x % 8);
        }
    }
    let mut s = Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Image",
            "Width" => w as i64,
            "Height" => h as i64,
            "ImageMask" => true,
            "BitsPerComponent" => 1i64,
        },
        bits,
    );
    s.compress()?;
    Ok(s)
}

/// A resource name for a new picture that the page does not use yet.
fn free_name(doc: &lopdf::Document, page_id: ObjectId, id: ObjectId) -> String {
    let taken = page_xobjects(doc, page_id);
    let mut name = format!("FoliImg{}", id.0);
    let mut n = 1;
    while taken.has(name.as_bytes()) {
        name = format!("FoliImg{}_{n}", id.0);
        n += 1;
    }
    name
}

/// The drawing operation of a picture moved by `place` within its old square.
fn drawing(name: &[u8], place: Affine) -> Vec<u8> {
    let mut op = Vec::new();
    let moved = !is_identity(place);
    if moved {
        op.extend_from_slice(format!("q {} cm ", matrix_text(place)).as_bytes());
    }
    op.extend_from_slice(name);
    op.extend_from_slice(b" Do");
    if moved {
        op.extend_from_slice(b" Q");
    }
    op
}

/// The range of the layer name of the `/OC /name BDC … EMC` sequence that
/// holds the drawing starting at `at` and nothing else that paints (a
/// picture already put on a layer that way).
fn layer_wrapping(data: &[u8], at: usize) -> Option<(usize, usize)> {
    // Operators that only set the graphics state.
    const QUIET: &[&[u8]] = &[
        b"q", b"Q", b"cm", b"gs", b"w", b"J", b"j", b"M", b"d", b"ri", b"i",
    ];
    let ops = operations(data);
    let k = ops.iter().position(|op| op.start == at)?;
    // The marked-content sequences open at the drawing: where they start and
    // the name of the layer of those that are layers.
    let mut open: Vec<(usize, Option<(usize, usize)>)> = Vec::new();
    for (i, op) in ops.iter().enumerate().take(k) {
        match op.operator {
            b"BDC" => {
                let oc = op.operands.first().and_then(|r| name_of(data, *r)).as_deref() == Some(b"OC");
                open.push((i, oc.then(|| op.operands.get(1).copied()).flatten()));
            }
            b"BMC" => open.push((i, None)),
            b"EMC" => {
                open.pop();
            }
            _ => {}
        }
    }
    let (first, name) = open.last().copied()?;
    let name = name?;
    // Nothing else painted inside the sequence, before or after the drawing.
    let mut depth = 0usize;
    for (i, op) in ops.iter().enumerate().skip(first + 1) {
        match op.operator {
            b"BDC" | b"BMC" => depth += 1,
            b"EMC" if depth == 0 => return Some(name),
            b"EMC" => depth -= 1,
            o if i != k && !QUIET.contains(&o) => return None,
            _ => {}
        }
    }
    None
}

impl Document {
    /// Put the `index`-th picture of a page on a layer, as part of the edit
    /// under way: its drawing goes inside `/OC /name BDC … EMC`, or changes
    /// layer when it is already alone inside such a sequence.
    pub(crate) fn put_picture_on_layer(&mut self, page: usize, index: usize, layer: ObjectId) -> Result<()> {
        let pid = self.page_id(page)?;
        let name = self.oc_property(pid, Object::Reference(layer))?;
        let j = joined(&self.inner, pid);
        let all = draws(&self.inner, pid, &j.data);
        let d = all
            .get(index)
            .ok_or_else(|| Error::Invalid("the picture is not on the page".into()))?;
        let splice = match layer_wrapping(&j.data, d.start) {
            Some((a, b)) => Splice {
                start: a,
                end: b,
                with: name_token(&name).into_bytes(),
            },
            None => {
                let mut with = format!("/OC {} BDC ", name_token(&name)).into_bytes();
                with.extend_from_slice(&j.data[d.start..d.end]);
                with.extend_from_slice(b" EMC");
                Splice {
                    start: d.start,
                    end: d.end,
                    with,
                }
            }
        };
        stream_span(&j, splice.start, splice.end)?;
        rewrite(&mut self.inner, pid, &j, &[splice])
    }

    /// Put the `index`-th picture of a page on a new layer named `name`
    /// (one undo step). Returns the layer.
    pub fn move_picture_to_new_layer(&mut self, page: usize, index: usize, name: &str) -> Result<ObjectId> {
        let name = name.trim();
        if name.is_empty() {
            return Err(Error::Invalid("the layer needs a name".into()));
        }
        if index >= self.page_images(page)?.len() {
            return Err(Error::Invalid("the picture is not on the page".into()));
        }
        self.edit_step(|doc| {
            let (id, _) = doc.add_layer_in_edit(name, &[])?;
            doc.put_picture_on_layer(page, index, id)?;
            Ok(id)
        })
    }

    /// The pictures drawn by the content of a page (cached per revision).
    pub fn page_images(&self, page: usize) -> Result<Arc<Vec<PageImage>>> {
        let pid = self.page_id(page)?;
        {
            let cache = self.image_cache.lock().unwrap_or_else(|e| e.into_inner());
            if let Some((rev, pages)) = cache.as_ref() {
                if *rev == self.revision() {
                    if let Some(list) = pages.get(&page) {
                        return Ok(list.clone());
                    }
                }
            }
        }
        let j = joined(&self.inner, pid);
        let list: Arc<Vec<PageImage>> = Arc::new(
            draws(&self.inner, pid, &j.data)
                .iter()
                .enumerate()
                .filter_map(|(i, d)| info(&self.inner, i, d))
                .collect(),
        );
        let mut cache = self.image_cache.lock().unwrap_or_else(|e| e.into_inner());
        match cache.as_mut() {
            Some((rev, pages)) if *rev == self.revision() => {
                pages.insert(page, list.clone());
            }
            _ => *cache = Some((self.revision(), HashMap::from([(page, list.clone())]))),
        }
        Ok(list)
    }

    /// The topmost picture of a page under a point (PDF user space).
    pub fn image_at(&self, page: usize, x: f32, y: f32) -> Option<usize> {
        let images = self.page_images(page).ok()?;
        images
            .iter()
            .rev()
            .find(|im| {
                let m = Affine::new(im.matrix);
                if m.determinant().abs() < 1e-12 {
                    return false;
                }
                let p = m.inverse() * Point::new(x as f64, y as f64);
                (0.0..=1.0).contains(&p.x) && (0.0..=1.0).contains(&p.y)
            })
            .map(|im| im.index)
    }

    /// The pixels of the `index`-th picture of a page (top row first, with
    /// the transparency of its soft mask).
    pub fn image_pixels(&self, page: usize, index: usize) -> Result<RgbaImage> {
        let pid = self.page_id(page)?;
        let j = joined(&self.inner, pid);
        let d = draws(&self.inner, pid, &j.data)
            .into_iter()
            .nth(index)
            .ok_or_else(|| Error::Invalid("the picture is not on the page".into()))?;
        decode_image(&self.inner, d.id)
    }

    /// The picture object is used by this drawing only.
    fn image_used_once(&self, page_id: ObjectId, draws: &[Draw], id: ObjectId) -> bool {
        draws.iter().filter(|d| d.id == id).count() == 1
            && count_refs(&self.inner, id) == 1
            && self.page_ids().into_iter().filter(|p| *p != page_id).all(|p| {
                !page_xobjects(&self.inner, p)
                    .iter()
                    .any(|(_, v)| matches!(v, Object::Reference(r) if *r == id))
            })
    }

    /// The new picture object made from `pixels` for the old one `id`.
    fn new_picture(
        &self,
        id: ObjectId,
        pixels: &RgbaImage,
        colors: Colors,
    ) -> Result<(Stream, Option<Stream>, Option<ObjectId>)> {
        if pixels.width() == 0 || pixels.height() == 0 {
            return Err(Error::Invalid("the picture has no size".into()));
        }
        let old = self.inner.get_object(id)?.as_stream()?;
        let was_jpeg = filters(&old.dict).last().is_some_and(|f| f == b"DCTDecode");
        let jpeg = was_jpeg && matches!(colors, Colors::Color | Colors::Gray);
        // A stencil mask stays one: painted where the pixels are opaque, with
        // the fill colour of the page.
        let (mut stream, smask) = if flag(&old.dict, b"ImageMask") {
            (encode_stencil(pixels)?, None)
        } else {
            encode_pixels(pixels, colors, jpeg)?
        };
        for key in [
            b"OC".as_slice(),
            b"Interpolate",
            b"Metadata",
            b"StructParent",
            b"Intent",
        ] {
            if let Ok(v) = old.dict.get(key) {
                stream.dict.set(key, v.clone());
            }
        }
        let old_mask = match old.dict.get(b"SMask") {
            Ok(Object::Reference(m)) => Some(*m),
            _ => None,
        };
        Ok((stream, smask, old_mask))
    }

    /// Replace the picture object `id` itself (its soft mask too).
    fn put_picture(
        &mut self,
        id: ObjectId,
        mut stream: Stream,
        smask: Option<Stream>,
        old_mask: Option<ObjectId>,
    ) {
        if let Some(m) = smask {
            let mid = self.inner.add_object(m);
            stream.dict.set("SMask", Object::Reference(mid));
        }
        self.inner.objects.insert(id, Object::Stream(stream));
        if let Some(m) = old_mask {
            if count_refs(&self.inner, m) == 0 {
                self.inner.objects.remove(&m);
            }
        }
    }

    /// Put `pixels` in place of the `index`-th picture of a page.
    /// `placement` maps the unit square of the new picture into the unit
    /// square of the old one (the identity keeps the same place). JPEG
    /// pictures stay JPEG, others are compressed without loss. A picture
    /// drawn elsewhere too is left there and this page gets a new one.
    pub fn replace_page_image(
        &mut self,
        page: usize,
        index: usize,
        pixels: &RgbaImage,
        placement: [f64; 6],
        colors: Colors,
    ) -> Result<()> {
        self.begin_edit();
        if let Err(e) = self.put_page_image(page, index, pixels, placement, colors) {
            self.abandon_edit();
            return Err(e);
        }
        self.commit()
    }

    /// [`Document::replace_page_image`] without an undo step of its own.
    pub(crate) fn put_page_image(
        &mut self,
        page: usize,
        index: usize,
        pixels: &RgbaImage,
        placement: [f64; 6],
        colors: Colors,
    ) -> Result<()> {
        let pid = self.page_id(page)?;
        let j = joined(&self.inner, pid);
        let all = draws(&self.inner, pid, &j.data);
        let d = all
            .get(index)
            .ok_or_else(|| Error::Invalid("the picture is not on the page".into()))?;
        let (stream, smask, old_mask) = self.new_picture(d.id, pixels, colors)?;
        let alone = self.image_used_once(pid, &all, d.id);
        let place = Affine::new(placement);
        if !alone || !is_identity(place) {
            stream_span(&j, d.start, d.end)?;
        }
        let name = if alone {
            self.put_picture(d.id, stream, smask, old_mask);
            d.name.clone()
        } else {
            let mut stream = stream;
            // The structure tree knows the old picture only.
            stream.dict.remove(b"StructParent");
            self.inherit_attributes(&[pid]);
            let xid = self.add_image_object(ImageObject {
                stream,
                smask,
                width: pixels.width() as f32,
                height: pixels.height() as f32,
            });
            let name = free_name(&self.inner, pid, xid);
            self.inner.add_xobject(pid, name.as_str(), xid)?;
            format!("/{name}").into_bytes()
        };
        if !alone || !is_identity(place) {
            let with = drawing(&name, place);
            rewrite(
                &mut self.inner,
                pid,
                &j,
                &[Splice {
                    start: d.start,
                    end: d.end,
                    with,
                }],
            )?;
        }
        Ok(())
    }

    /// Like [`Document::replace_page_image`], for every drawing of the
    /// picture: the picture object itself changes, and every page that draws
    /// it moves it the same way. Returns how many drawings there are.
    pub fn replace_image_everywhere(
        &mut self,
        page: usize,
        index: usize,
        pixels: &RgbaImage,
        placement: [f64; 6],
        colors: Colors,
    ) -> Result<usize> {
        let pid = self.page_id(page)?;
        let j = joined(&self.inner, pid);
        let (id, shape) = draws(&self.inner, pid, &j.data)
            .get(index)
            .map(|d| (d.id, sides_ratio(d.ctm)))
            .ok_or_else(|| Error::Invalid("the picture is not on the page".into()))?;
        let (stream, smask, old_mask) = self.new_picture(id, pixels, colors)?;
        let place = Affine::new(placement);
        // Every page that draws the picture, with its drawings.
        let mut plans: Vec<(ObjectId, Joined, Vec<Splice>)> = Vec::new();
        let mut count = 0;
        for p in self.page_ids() {
            let j = joined(&self.inner, p);
            let found: Vec<Draw> = draws(&self.inner, p, &j.data)
                .into_iter()
                .filter(|d| d.id == id)
                .collect();
            // A change of shape made for one drawing fits the others only
            // when they have the same proportions.
            if !is_identity(place)
                && found
                    .iter()
                    .any(|d| (sides_ratio(d.ctm) / shape - 1.0).abs() > 0.01)
            {
                return Err(Error::Invalid(
                    "the picture is drawn with other proportions elsewhere: change this drawing only".into(),
                ));
            }
            let mine: Vec<Splice> = found
                .into_iter()
                .map(|d| Splice {
                    start: d.start,
                    end: d.end,
                    with: drawing(&d.name, place),
                })
                .collect();
            count += mine.len();
            for s in &mine {
                stream_span(&j, s.start, s.end)?;
            }
            if !mine.is_empty() {
                plans.push((p, j, mine));
            }
        }
        self.begin_edit();
        self.put_picture(id, stream, smask, old_mask);
        if !is_identity(place) {
            for (p, j, splices) in plans {
                rewrite(&mut self.inner, p, &j, &splices)?;
            }
        }
        self.commit()?;
        Ok(count)
    }
}
