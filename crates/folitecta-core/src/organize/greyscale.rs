//! Conversion of the colours of a document to grey (FOL-L044): the colours
//! of the page content, of drawings and of comment appearances (RGB, CMYK
//! and ICC-based colours of three or four components, with their colour
//! spaces), the pictures in those colours (palettes included) and the
//! colours of comments and form fields. What is not converted is counted
//! and left as it was: patterns, spot colours (Separation, DeviceN), Lab
//! colours, shadings, pictures inline in the content and pictures that
//! cannot be read. One undo step; the view's grey preview only simulates.

use std::collections::{BTreeMap, HashMap};

use lopdf::content::{Content, Operation};
use lopdf::{Dictionary, Object, ObjectId, Stream};

use crate::doctools::stream_bytes;
use crate::document::Document;
use crate::error::Result;
use crate::util::deref;

/// What the conversion did, and what it left.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GreyReport {
    /// Colour operators converted.
    pub colours: usize,
    /// Pictures converted.
    pub pictures: usize,
    /// Colours of comments and fields converted.
    pub comments: usize,
    /// What was left as it was: kind (`pattern`, `spot`, `lab`, `shading`,
    /// `inline-picture`, `picture`, `content`), how many.
    pub kept: BTreeMap<&'static str, usize>,
}

impl GreyReport {
    fn keep(&mut self, what: &'static str) {
        *self.kept.entry(what).or_default() += 1;
    }

    /// Nothing changed.
    pub fn is_empty(&self) -> bool {
        self.colours + self.pictures + self.comments == 0
    }
}

/// Grey of an RGB colour (components 0 to 1).
pub fn grey_of_rgb(r: f32, g: f32, b: f32) -> f32 {
    (0.299 * r + 0.587 * g + 0.114 * b).clamp(0.0, 1.0)
}

/// Grey of a CMYK colour.
pub fn grey_of_cmyk(c: f32, m: f32, y: f32, k: f32) -> f32 {
    let rgb = |x: f32| (1.0 - x) * (1.0 - k);
    grey_of_rgb(rgb(c), rgb(m), rgb(y))
}

/// What a colour space is, for the conversion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Space {
    Grey,
    Rgb,
    Cmyk,
    /// Patterns, spot colours, Lab, palettes in the content: left alone.
    Other(&'static str),
}

fn number(o: &Object) -> Option<f32> {
    match o {
        Object::Integer(i) => Some(*i as f32),
        Object::Real(r) => Some(*r),
        _ => None,
    }
}

/// The kind of a colour space object (a name or an array).
fn space_of(doc: &lopdf::Document, cs: &Object) -> Space {
    match deref(doc, cs) {
        Object::Name(n) => match n.as_slice() {
            b"DeviceGray" | b"CalGray" | b"G" => Space::Grey,
            b"DeviceRGB" | b"CalRGB" | b"RGB" => Space::Rgb,
            b"DeviceCMYK" | b"CMYK" => Space::Cmyk,
            b"Pattern" => Space::Other("pattern"),
            _ => Space::Other("spot"),
        },
        Object::Array(a) => match a.first().and_then(|x| x.as_name().ok()) {
            Some(b"CalGray") => Space::Grey,
            Some(b"CalRGB") => Space::Rgb,
            Some(b"ICCBased") => {
                let n = a
                    .get(1)
                    .map(|s| deref(doc, s))
                    .and_then(|s| s.as_stream().ok())
                    .and_then(|s| s.dict.get(b"N").ok().and_then(|n| n.as_i64().ok()))
                    .unwrap_or(3);
                match n {
                    1 => Space::Grey,
                    4 => Space::Cmyk,
                    _ => Space::Rgb,
                }
            }
            Some(b"Pattern") => Space::Other("pattern"),
            Some(b"Lab") => Space::Other("lab"),
            Some(b"Indexed") => Space::Other("palette"),
            _ => Space::Other("spot"),
        },
        _ => Space::Other("spot"),
    }
}

/// The grey of `operands` read in `space`, if they are numbers of it.
fn grey_of(space: Space, operands: &[Object]) -> Option<f32> {
    let v: Vec<f32> = operands.iter().map(number).collect::<Option<Vec<_>>>()?;
    match (space, v.as_slice()) {
        (Space::Rgb, [r, g, b]) => Some(grey_of_rgb(*r, *g, *b)),
        (Space::Cmyk, [c, m, y, k]) => Some(grey_of_cmyk(*c, *m, *y, *k)),
        _ => None,
    }
}

/// Convert the colours of one content stream read with the colour spaces
/// of `resources`; `None` when nothing changes.
fn convert_content(
    doc: &lopdf::Document,
    data: &[u8],
    resources: Option<&Dictionary>,
    report: &mut GreyReport,
) -> Option<Vec<u8>> {
    let Ok(content) = Content::decode(data) else {
        report.keep("content");
        return None;
    };
    let spaces = resources
        .and_then(|r| r.get(b"ColorSpace").ok())
        .map(|c| deref(doc, c))
        .and_then(|c| c.as_dict().ok());
    let named = |n: &[u8]| -> Space {
        match n {
            b"DeviceGray" | b"DeviceRGB" | b"DeviceCMYK" | b"Pattern" => {
                space_of(doc, &Object::Name(n.to_vec()))
            }
            _ => spaces
                .and_then(|s| s.get(n).ok())
                .map(|cs| space_of(doc, cs))
                .unwrap_or(Space::Other("spot")),
        }
    };
    let mut ops: Vec<Operation> = Vec::with_capacity(content.operations.len());
    let mut changed = false;
    // Fill and stroke colour spaces, saved with the graphics state.
    let (mut fill, mut stroke) = (Space::Grey, Space::Grey);
    let mut saved: Vec<(Space, Space)> = Vec::new();
    for mut op in content.operations {
        match op.operator.as_str() {
            "q" => saved.push((fill, stroke)),
            "Q" => {
                if let Some((f, s)) = saved.pop() {
                    fill = f;
                    stroke = s;
                }
            }
            o @ ("rg" | "RG" | "k" | "K") => {
                let space = if o.eq_ignore_ascii_case("rg") {
                    Space::Rgb
                } else {
                    Space::Cmyk
                };
                if let Some(g) = grey_of(space, &op.operands) {
                    let stroking = o.chars().all(|c| c.is_ascii_uppercase());
                    op = Operation::new(if stroking { "G" } else { "g" }, vec![Object::Real(g)]);
                    if stroking {
                        stroke = Space::Grey;
                    } else {
                        fill = Space::Grey;
                    }
                    report.colours += 1;
                    changed = true;
                }
            }
            "g" => fill = Space::Grey,
            "G" => stroke = Space::Grey,
            o @ ("cs" | "CS") => {
                let space = op
                    .operands
                    .first()
                    .and_then(|n| n.as_name().ok())
                    .map(named)
                    .unwrap_or(Space::Other("spot"));
                let stroking = o == "CS";
                let now = match space {
                    Space::Rgb | Space::Cmyk => {
                        op.operands = vec![Object::Name(b"DeviceGray".to_vec())];
                        changed = true;
                        // The colours that follow are converted with it.
                        space
                    }
                    other => {
                        if let Space::Other(what) = other {
                            report.keep(what);
                        }
                        other
                    }
                };
                if stroking {
                    stroke = now;
                } else {
                    fill = now;
                }
            }
            o @ ("sc" | "scn" | "SC" | "SCN") => {
                let stroking = o.starts_with('S');
                let space = if stroking { stroke } else { fill };
                if let Some(g) = grey_of(space, &op.operands) {
                    op.operands = vec![Object::Real(g)];
                    report.colours += 1;
                    changed = true;
                }
            }
            "sh" => report.keep("shading"),
            "BI" => report.keep("inline-picture"),
            _ => {}
        }
        ops.push(op);
    }
    // Colour spaces set to grey hold grey colours from then on.
    if !changed {
        return None;
    }
    Content { operations: ops }.encode().ok()
}

/// The pixels of a picture in grey (8 bits per pixel), and whether its data
/// was a JPEG; `None` when it cannot be read.
fn picture_grey(doc: &lopdf::Document, s: &Stream, space: Space) -> Option<(image::GrayImage, bool)> {
    let d = &s.dict;
    let int = |k: &[u8]| d.get(k).ok().and_then(|v| v.as_i64().ok());
    let (w, h) = (int(b"Width")? as u32, int(b"Height")? as u32);
    if int(b"BitsPerComponent").unwrap_or(8) != 8 || d.has(b"Decode") || w == 0 || h == 0 {
        return None;
    }
    let filters: Vec<Vec<u8>> = match d.get(b"Filter") {
        Ok(Object::Name(n)) => vec![n.clone()],
        Ok(Object::Array(a)) => a
            .iter()
            .filter_map(|x| x.as_name().ok().map(|n| n.to_vec()))
            .collect(),
        _ => Vec::new(),
    };
    let _ = doc;
    if filters.last().map(|f| f.as_slice()) == Some(b"DCTDecode") {
        if filters.len() != 1 {
            return None;
        }
        let img = image::load_from_memory_with_format(&s.content, image::ImageFormat::Jpeg).ok()?;
        return Some((img.to_luma8(), true));
    }
    let px = stream_bytes(s)?;
    let comps = match space {
        Space::Rgb => 3,
        Space::Cmyk => 4,
        _ => return None,
    } as usize;
    let n = w as usize * h as usize;
    if px.len() < n * comps {
        return None;
    }
    let grey = px[..n * comps]
        .chunks_exact(comps)
        .map(|c| {
            let f = |i: usize| c[i] as f32 / 255.0;
            let g = if comps == 3 {
                grey_of_rgb(f(0), f(1), f(2))
            } else {
                grey_of_cmyk(f(0), f(1), f(2), f(3))
            };
            (g * 255.0).round() as u8
        })
        .collect();
    Some((image::GrayImage::from_raw(w, h, grey)?, false))
}

/// A palette of `space` in grey.
fn palette_grey(doc: &lopdf::Document, space: Space, lookup: &Object) -> Option<Vec<u8>> {
    let bytes = match deref(doc, lookup) {
        Object::String(b, _) => b.clone(),
        Object::Stream(s) => stream_bytes(s)?,
        _ => return None,
    };
    let comps = match space {
        Space::Rgb => 3,
        Space::Cmyk => 4,
        _ => return None,
    };
    Some(
        bytes
            .chunks_exact(comps)
            .map(|c| {
                let f = |i: usize| c[i] as f32 / 255.0;
                let g = if comps == 3 {
                    grey_of_rgb(f(0), f(1), f(2))
                } else {
                    grey_of_cmyk(f(0), f(1), f(2), f(3))
                };
                (g * 255.0).round() as u8
            })
            .collect(),
    )
}

/// A colour array of a comment (`/C`, `/IC`, `/MK` colours) in grey.
fn colour_array_grey(a: &[Object]) -> Option<Vec<Object>> {
    let v: Vec<f32> = a.iter().map(number).collect::<Option<Vec<_>>>()?;
    let g = match v.as_slice() {
        [r, g, b] => grey_of_rgb(*r, *g, *b),
        [c, m, y, k] => grey_of_cmyk(*c, *m, *y, *k),
        _ => return None,
    };
    Some(vec![Object::Real(g)])
}

impl Document {
    /// What converting the colours to grey would do (nothing changes).
    pub fn estimate_grey(&self) -> Result<GreyReport> {
        let mut copy = self.duplicate()?;
        copy.convert_to_grey()
    }

    /// Convert every colour of the document to grey (one undo step).
    pub fn convert_to_grey(&mut self) -> Result<GreyReport> {
        let mut report = GreyReport::default();
        let doc = &self.inner;
        let mut replaced: HashMap<ObjectId, Object> = HashMap::new();
        // Page contents, with the resources of their pages.
        for (_, pid) in doc.get_pages() {
            let Ok(page) = doc.get_dictionary(pid) else {
                continue;
            };
            let resources = page
                .get(b"Resources")
                .ok()
                .map(|r| deref(doc, r))
                .and_then(|r| r.as_dict().ok())
                .cloned();
            let contents: Vec<ObjectId> = match page.get(b"Contents") {
                Ok(Object::Reference(r)) => vec![*r],
                Ok(Object::Array(a)) => a.iter().filter_map(|x| x.as_reference().ok()).collect(),
                _ => Vec::new(),
            };
            if contents.len() == 1 {
                let id = contents[0];
                if let Ok(Object::Stream(s)) = doc.get_object(id) {
                    let Some(data) = stream_bytes(s) else { continue };
                    if let Some(new) = convert_content(doc, &data, resources.as_ref(), &mut report) {
                        let mut s = Stream::new(s.dict.clone(), new);
                        s.dict.remove(b"Filter");
                        s.dict.remove(b"DecodeParms");
                        let _ = s.compress();
                        replaced.insert(id, Object::Stream(s));
                    }
                }
            } else if !contents.is_empty() {
                // Several streams: converted as one and written in the first.
                let data = doc.get_page_content(pid);
                if let Some(new) = convert_content(doc, &data, resources.as_ref(), &mut report) {
                    let mut s = Stream::new(Dictionary::new(), new);
                    let _ = s.compress();
                    replaced.insert(contents[0], Object::Stream(s));
                    for id in &contents[1..] {
                        replaced.insert(*id, Object::Stream(Stream::new(Dictionary::new(), Vec::new())));
                    }
                }
            }
        }
        // Drawings (form XObjects, comment appearances) and pictures.
        for (id, obj) in &doc.objects {
            let Object::Stream(s) = obj else { continue };
            let subtype = s.dict.get(b"Subtype").ok().and_then(|n| n.as_name().ok());
            match subtype {
                Some(b"Form") => {
                    let resources = s
                        .dict
                        .get(b"Resources")
                        .ok()
                        .map(|r| deref(doc, r))
                        .and_then(|r| r.as_dict().ok())
                        .cloned();
                    let Some(data) = stream_bytes(s) else {
                        report.keep("content");
                        continue;
                    };
                    if let Some(new) = convert_content(doc, &data, resources.as_ref(), &mut report) {
                        let mut ns = Stream::new(s.dict.clone(), new);
                        ns.dict.remove(b"Filter");
                        ns.dict.remove(b"DecodeParms");
                        let _ = ns.compress();
                        replaced.insert(*id, Object::Stream(ns));
                    }
                }
                Some(b"Image") => {
                    if s.dict.get(b"ImageMask").ok().and_then(|b| b.as_bool().ok()) == Some(true) {
                        continue;
                    }
                    let Ok(cs) = s.dict.get(b"ColorSpace") else {
                        continue;
                    };
                    let cs = deref(doc, cs).clone();
                    // A palette: its colours become grey.
                    if let Object::Array(a) = &cs {
                        if a.first().and_then(|x| x.as_name().ok()) == Some(b"Indexed") && a.len() == 4 {
                            let base = space_of(doc, &a[1]);
                            if matches!(base, Space::Grey) {
                                continue;
                            }
                            match palette_grey(doc, base, &a[3]) {
                                Some(lookup) => {
                                    let mut ns = s.clone();
                                    ns.dict.set(
                                        "ColorSpace",
                                        vec![
                                            Object::Name(b"Indexed".to_vec()),
                                            Object::Name(b"DeviceGray".to_vec()),
                                            a[2].clone(),
                                            Object::String(lookup, lopdf::StringFormat::Hexadecimal),
                                        ],
                                    );
                                    replaced.insert(*id, Object::Stream(ns));
                                    report.pictures += 1;
                                }
                                None => report.keep("picture"),
                            }
                            continue;
                        }
                    }
                    let space = space_of(doc, &cs);
                    match space {
                        Space::Grey => continue,
                        Space::Rgb | Space::Cmyk => {}
                        Space::Other(_) => {
                            report.keep("picture");
                            continue;
                        }
                    }
                    let Some((grey, jpeg)) = picture_grey(doc, s, space) else {
                        report.keep("picture");
                        continue;
                    };
                    let mut dict = s.dict.clone();
                    dict.set("ColorSpace", Object::Name(b"DeviceGray".to_vec()));
                    dict.set("BitsPerComponent", 8);
                    dict.remove(b"DecodeParms");
                    let ns = if jpeg {
                        let mut out = std::io::Cursor::new(Vec::new());
                        let enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 88);
                        if grey.write_with_encoder(enc).is_err() {
                            report.keep("picture");
                            continue;
                        }
                        dict.set("Filter", Object::Name(b"DCTDecode".to_vec()));
                        let data = out.into_inner();
                        dict.set("Length", data.len() as i64);
                        let mut st = Stream::new(dict, data);
                        st.allows_compression = false;
                        st
                    } else {
                        dict.remove(b"Filter");
                        let mut st = Stream::new(dict, grey.into_raw());
                        let _ = st.compress();
                        st
                    };
                    replaced.insert(*id, Object::Stream(ns));
                    report.pictures += 1;
                }
                _ => {}
            }
        }
        // Colours of comments and fields.
        let mut dicts: Vec<(ObjectId, Dictionary)> = Vec::new();
        for (id, obj) in &doc.objects {
            let Object::Dictionary(d) = obj else { continue };
            if !(d.has(b"Rect") && d.has(b"Subtype")) {
                continue;
            }
            let mut nd = d.clone();
            let mut touched = false;
            for key in [b"C".as_slice(), b"IC"] {
                if let Ok(Object::Array(a)) = nd.get(key) {
                    if let Some(g) = colour_array_grey(a) {
                        nd.set(key, g);
                        touched = true;
                    }
                }
            }
            if let Ok(Object::Dictionary(mk)) = nd.get_mut(b"MK") {
                for key in [b"BC".as_slice(), b"BG"] {
                    if let Ok(Object::Array(a)) = mk.get(key) {
                        if let Some(g) = colour_array_grey(a) {
                            mk.set(key, g);
                            touched = true;
                        }
                    }
                }
            }
            if let Ok(Object::String(da, f)) = nd.get(b"DA") {
                let f = *f;
                let mut r = GreyReport::default();
                if let Some(new) = convert_content(doc, da, None, &mut r) {
                    let text = String::from_utf8_lossy(&new).trim().to_string();
                    nd.set("DA", Object::String(text.into_bytes(), f));
                    touched = true;
                }
            }
            if touched {
                report.comments += 1;
                dicts.push((*id, nd));
            }
        }
        if report.is_empty() {
            return Ok(report);
        }
        self.begin_edit();
        for (id, obj) in replaced {
            self.inner.objects.insert(id, obj);
        }
        for (id, d) in dicts {
            self.inner.objects.insert(id, Object::Dictionary(d));
        }
        self.commit()?;
        Ok(report)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colours_become_greys() {
        assert!((grey_of_rgb(1.0, 1.0, 1.0) - 1.0).abs() < 1e-6);
        assert!((grey_of_rgb(1.0, 0.0, 0.0) - 0.299).abs() < 1e-6);
        assert!((grey_of_cmyk(0.0, 0.0, 0.0, 1.0)).abs() < 1e-6);
        assert!((grey_of_cmyk(0.0, 0.0, 0.0, 0.0) - 1.0).abs() < 1e-6);
        let doc = lopdf::Document::new();
        let mut report = GreyReport::default();
        let out = convert_content(
            &doc,
            b"1 0 0 rg 0 0 1 RG 0 0 0 1 k /DeviceRGB cs 0 1 0 sc /P0 cs 0 0 10 10 re f",
            None,
            &mut report,
        )
        .unwrap();
        let text = String::from_utf8_lossy(&out);
        assert!(
            !text.contains(" rg") && !text.contains(" RG") && !text.contains(" k\n"),
            "{text}"
        );
        assert!(text.contains("/DeviceGray cs"), "{text}");
        assert_eq!(report.colours, 4);
        assert_eq!(report.kept.get("spot"), Some(&1));
        // Nothing to convert: nothing written.
        assert!(convert_content(&doc, b"0.5 g 0 0 10 10 re f", None, &mut GreyReport::default()).is_none());
    }
}
