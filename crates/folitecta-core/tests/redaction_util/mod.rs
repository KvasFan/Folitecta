//! Fixtures and checks shared by the redaction tests: documents built from
//! content streams with lopdf, text boxes, decoded streams of saved files.
#![allow(dead_code)]

use std::path::PathBuf;

use folitecta_core::marks::display_to_pdf;
use folitecta_core::{Document, PageText};
use lopdf::{Dictionary, Object, ObjectId, Stream, dictionary};

/// A one-page document drawing `content` with `resources`; `extra` may add
/// objects (forms, fonts) before the page is written. Pages share nothing.
pub fn doc_with(
    w: f32,
    h: f32,
    pages: &[&str],
    resources: impl Fn(&mut lopdf::Document) -> Dictionary,
) -> Document {
    let mut d = lopdf::Document::with_version("1.7");
    let pages_id = d.new_object_id();
    let mut kids = Vec::new();
    for content in pages {
        let res = resources(&mut d);
        let cid = d.add_object(Stream::new(dictionary! {}, content.as_bytes().to_vec()));
        let pid = d.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), w.into(), h.into()],
            "Contents" => cid,
            "Resources" => res,
        });
        kids.push(Object::Reference(pid));
    }
    let n = kids.len() as i64;
    d.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! { "Type" => "Pages", "Kids" => kids, "Count" => n }),
    );
    let cat = d.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    d.trailer.set("Root", cat);
    let mut out = Vec::new();
    d.save_to(&mut out).unwrap();
    Document::from_bytes(out, None, None).unwrap()
}

/// A Helvetica font resource (WinAnsi, no widths: a standard font).
pub fn helvetica(d: &mut lopdf::Document) -> ObjectId {
    d.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
        "Encoding" => "WinAnsiEncoding",
    })
}

pub fn temp(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("folitecta-redact-{name}-{}.pdf", std::process::id()))
}

/// Display bounds (y down) of the first occurrence of `word` in the text of a page.
pub fn word_box(layer: &PageText, word: &str) -> [f32; 4] {
    word_box_nth(layer, word, 0)
}

/// Display bounds of the `nth` occurrence of `word`.
pub fn word_box_nth(layer: &PageText, word: &str, nth: usize) -> [f32; 4] {
    let at = layer
        .text
        .match_indices(word)
        .nth(nth)
        .map(|(i, _)| i)
        .unwrap_or_else(|| panic!("{word} not in {:?}", layer.text));
    let start = layer.text[..at].chars().count();
    let boxes: Vec<[f32; 4]> = (start..start + word.chars().count())
        .filter_map(|i| layer.char_glyph[i])
        .map(|g| layer.glyphs[g].bbox)
        .collect();
    [
        boxes.iter().map(|b| b[0]).fold(f32::MAX, f32::min),
        boxes.iter().map(|b| b[1]).fold(f32::MAX, f32::min),
        boxes.iter().map(|b| b[2]).fold(f32::MIN, f32::max),
        boxes.iter().map(|b| b[3]).fold(f32::MIN, f32::max),
    ]
}

/// The PDF rectangle of a display rectangle of `page`.
pub fn pdf_rect(doc: &Document, page: usize, d: [f32; 4]) -> [f32; 4] {
    let size = doc.page_size(page).unwrap();
    let a = display_to_pdf(&size, d[0], size.height - d[3]);
    let b = display_to_pdf(&size, d[2], size.height - d[1]);
    [a.0.min(b.0), a.1.min(b.1), a.0.max(b.0), a.1.max(b.1)]
}

/// Every stream of saved bytes, decompressed when it can be.
pub fn streams_of(bytes: &[u8]) -> Vec<Vec<u8>> {
    let doc = lopdf::Document::load_mem(bytes).unwrap();
    doc.objects
        .values()
        .filter_map(|o| o.as_stream().ok())
        .map(|s| s.decompressed_content().unwrap_or_else(|_| s.content.clone()))
        .collect()
}

pub fn holds(hay: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty() && hay.windows(needle.len()).any(|w| w == needle)
}

pub fn hex(text: &str) -> Vec<u8> {
    text.bytes()
        .map(|b| format!("{b:02X}"))
        .collect::<String>()
        .into_bytes()
}

/// Saved bytes hold `word` in no stream and no string, as text or as codes.
pub fn absent(bytes: &[u8], word: &str) -> bool {
    let streams = streams_of(bytes);
    let low = word.to_lowercase();
    let raw_low: Vec<u8> = bytes.to_ascii_lowercase();
    !holds(&raw_low, low.as_bytes())
        && streams.iter().all(|s| {
            let s = s.to_ascii_lowercase();
            !holds(&s, low.as_bytes())
                && !holds(&s, &hex(&low))
                && !holds(&s, &hex(word).to_ascii_lowercase())
        })
}

/// The bytes of the document as a save writes them.
pub fn saved(doc: &mut Document, name: &str) -> Vec<u8> {
    let path = temp(name);
    doc.save_as(&path).unwrap();
    let bytes = std::fs::read(&path).unwrap();
    let _ = std::fs::remove_file(&path);
    bytes
}

// ---- Geometry painted by a saved page ----------------------------------------------

pub type Pt = (f64, f64);

/// What a page paints (page space), read from its content streams except
/// the covers of a redaction.
#[derive(Debug, Default)]
pub struct Painted {
    /// Filled polygons with their colour.
    pub fills: Vec<(Vec<Vec<Pt>>, [f32; 3])>,
    /// Stroked polylines with their colour.
    pub strokes: Vec<(Vec<Vec<Pt>>, [f32; 3])>,
    /// Clip polygons.
    pub clips: Vec<Vec<Vec<Pt>>>,
    pub operators: Vec<String>,
}

fn mul(m: [f64; 6], n: [f64; 6]) -> [f64; 6] {
    // m applied after n (row vectors: p * n * m).
    [
        n[0] * m[0] + n[1] * m[2],
        n[0] * m[1] + n[1] * m[3],
        n[2] * m[0] + n[3] * m[2],
        n[2] * m[1] + n[3] * m[3],
        n[4] * m[0] + n[5] * m[2] + m[4],
        n[4] * m[1] + n[5] * m[3] + m[5],
    ]
}

fn apply(m: [f64; 6], p: Pt) -> Pt {
    (m[0] * p.0 + m[2] * p.1 + m[4], m[1] * p.0 + m[3] * p.1 + m[5])
}

fn nums(ops: &[lopdf::Object]) -> Vec<f64> {
    ops.iter()
        .filter_map(|o| match o {
            lopdf::Object::Integer(i) => Some(*i as f64),
            lopdf::Object::Real(r) => Some(*r as f64),
            _ => None,
        })
        .collect()
}

/// The covers of a redaction are streams tagged `/FolitectaMark /Redaction…`.
pub fn is_cover(s: &lopdf::Stream) -> bool {
    s.dict
        .get(b"FolitectaMark")
        .ok()
        .and_then(|v| v.as_name().ok())
        .is_some_and(|n| n.starts_with(b"Redaction"))
}

/// What page `page` of saved bytes paints (forms followed).
pub fn painted(bytes: &[u8], page: usize) -> Painted {
    let doc = lopdf::Document::load_mem(bytes).unwrap();
    let pid = *doc.get_pages().get(&(page as u32 + 1)).unwrap();
    let mut data = Vec::new();
    for sid in doc.get_page_contents(pid) {
        let s = doc.get_object(sid).unwrap().as_stream().unwrap();
        if is_cover(s) {
            continue;
        }
        data.extend(s.decompressed_content().unwrap_or_else(|_| s.content.clone()));
        data.push(b'\n');
    }
    let res = page_resources(&doc, pid);
    let mut out = Painted::default();
    interpret(&doc, &data, &res, [1.0, 0.0, 0.0, 1.0, 0.0, 0.0], &mut out, 0);
    out
}

fn page_resources(doc: &lopdf::Document, pid: ObjectId) -> Dictionary {
    let mut cur = Some(pid);
    while let Some(id) = cur {
        let d = doc.get_dictionary(id).unwrap();
        if let Ok(r) = d.get(b"Resources") {
            return match r {
                Object::Reference(r) => doc.get_dictionary(*r).unwrap().clone(),
                Object::Dictionary(d) => d.clone(),
                _ => Dictionary::new(),
            };
        }
        cur = d.get(b"Parent").ok().and_then(|p| p.as_reference().ok());
    }
    Dictionary::new()
}

fn interpret(
    doc: &lopdf::Document,
    data: &[u8],
    res: &Dictionary,
    m0: [f64; 6],
    out: &mut Painted,
    depth: usize,
) {
    let content = lopdf::content::Content::decode(data).unwrap();
    let mut ctm = m0;
    let mut stack = Vec::new();
    let (mut fill, mut stroke) = ([0.0f32; 3], [0.0f32; 3]);
    let mut path: Vec<Vec<Pt>> = Vec::new();
    let mut cur: Pt = (0.0, 0.0);
    let mut clip_next = false;
    for op in content.operations {
        out.operators.push(op.operator.clone());
        let n = nums(&op.operands);
        match op.operator.as_str() {
            "q" => stack.push((ctm, fill, stroke)),
            "Q" => {
                if let Some(s) = stack.pop() {
                    (ctm, fill, stroke) = s;
                }
            }
            "cm" if n.len() == 6 => ctm = mul(ctm, [n[0], n[1], n[2], n[3], n[4], n[5]]),
            "rg" if n.len() == 3 => fill = [n[0] as f32, n[1] as f32, n[2] as f32],
            "g" if n.len() == 1 => fill = [n[0] as f32; 3],
            "RG" if n.len() == 3 => stroke = [n[0] as f32, n[1] as f32, n[2] as f32],
            "G" if n.len() == 1 => stroke = [n[0] as f32; 3],
            "m" => {
                cur = (n[0], n[1]);
                path.push(vec![apply(ctm, cur)]);
            }
            "l" => {
                cur = (n[0], n[1]);
                if let Some(sp) = path.last_mut() {
                    sp.push(apply(ctm, cur));
                }
            }
            "c" | "v" | "y" => {
                let (c1, c2, e) = match op.operator.as_str() {
                    "c" => ((n[0], n[1]), (n[2], n[3]), (n[4], n[5])),
                    "v" => (cur, (n[0], n[1]), (n[2], n[3])),
                    _ => ((n[0], n[1]), (n[2], n[3]), (n[2], n[3])),
                };
                let p0 = cur;
                if let Some(sp) = path.last_mut() {
                    for k in 1..=32 {
                        let t = k as f64 / 32.0;
                        let u = 1.0 - t;
                        let x = u * u * u * p0.0
                            + 3.0 * u * u * t * c1.0
                            + 3.0 * u * t * t * c2.0
                            + t * t * t * e.0;
                        let y = u * u * u * p0.1
                            + 3.0 * u * u * t * c1.1
                            + 3.0 * u * t * t * c2.1
                            + t * t * t * e.1;
                        sp.push(apply(ctm, (x, y)));
                    }
                }
                cur = e;
            }
            "re" => {
                let (x, y, w, h) = (n[0], n[1], n[2], n[3]);
                path.push(
                    [(x, y), (x + w, y), (x + w, y + h), (x, y + h)]
                        .into_iter()
                        .map(|p| apply(ctm, p))
                        .collect(),
                );
                cur = (x, y);
            }
            "h" => {
                if let Some(first) = path.last().and_then(|sp| sp.first()).copied() {
                    path.last_mut().unwrap().push(first);
                }
            }
            "W" | "W*" => clip_next = true,
            "f" | "F" | "f*" | "B" | "B*" | "b" | "b*" | "S" | "s" | "n" => {
                let o = op.operator.as_str();
                if matches!(o, "f" | "F" | "f*" | "B" | "B*" | "b" | "b*") {
                    out.fills.push((std::mem::take(&mut path.clone()), fill));
                }
                if matches!(o, "S" | "s" | "B" | "B*" | "b" | "b*") {
                    out.strokes.push((path.clone(), stroke));
                }
                if clip_next {
                    out.clips.push(path.clone());
                }
                clip_next = false;
                path.clear();
            }
            "Do" if depth < 8 => {
                let Some(name) = op.operands.first().and_then(|o| o.as_name().ok()) else {
                    continue;
                };
                let Some(xo) = res
                    .get(b"XObject")
                    .ok()
                    .and_then(|x| match x {
                        Object::Reference(r) => doc.get_dictionary(*r).ok(),
                        Object::Dictionary(d) => Some(d),
                        _ => None,
                    })
                    .and_then(|x| x.get(name).ok())
                    .and_then(|v| v.as_reference().ok())
                else {
                    continue;
                };
                let s = doc.get_object(xo).unwrap().as_stream().unwrap();
                if s.dict.get(b"Subtype").ok().and_then(|v| v.as_name().ok()) != Some(b"Form") {
                    continue;
                }
                let fm = s
                    .dict
                    .get(b"Matrix")
                    .ok()
                    .and_then(|m| m.as_array().ok())
                    .map(|a| nums(a))
                    .filter(|v| v.len() == 6)
                    .map(|v| [v[0], v[1], v[2], v[3], v[4], v[5]])
                    .unwrap_or([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
                let fres = match s.dict.get(b"Resources") {
                    Ok(Object::Reference(r)) => doc.get_dictionary(*r).unwrap().clone(),
                    Ok(Object::Dictionary(d)) => d.clone(),
                    _ => res.clone(),
                };
                let fdata = s.decompressed_content().unwrap_or_else(|_| s.content.clone());
                interpret(doc, &fdata, &fres, mul(ctm, fm), out, depth + 1);
            }
            _ => {}
        }
    }
}

/// Signed area of a polygon.
pub fn area(poly: &[Pt]) -> f64 {
    let n = poly.len();
    (0..n)
        .map(|i| {
            let (a, b) = (poly[i], poly[(i + 1) % n]);
            a.0 * b.1 - b.0 * a.1
        })
        .sum::<f64>()
        / 2.0
}

/// A polygon clipped to a rectangle (Sutherland–Hodgman).
pub fn clip(poly: &[Pt], r: [f64; 4]) -> Vec<Pt> {
    let mut out = poly.to_vec();
    for side in 0..4 {
        if out.is_empty() {
            break;
        }
        let inside = |p: Pt| match side {
            0 => p.0 >= r[0],
            1 => p.0 <= r[2],
            2 => p.1 >= r[1],
            _ => p.1 <= r[3],
        };
        let cut = |a: Pt, b: Pt| -> Pt {
            match side {
                0 | 1 => {
                    let x = if side == 0 { r[0] } else { r[2] };
                    let t = (x - a.0) / (b.0 - a.0);
                    (x, a.1 + t * (b.1 - a.1))
                }
                _ => {
                    let y = if side == 2 { r[1] } else { r[3] };
                    let t = (y - a.1) / (b.1 - a.1);
                    (a.0 + t * (b.0 - a.0), y)
                }
            }
        };
        let input = std::mem::take(&mut out);
        let n = input.len();
        for i in 0..n {
            let (c, p) = (input[i], input[(i + n - 1) % n]);
            match (inside(c), inside(p)) {
                (true, true) => out.push(c),
                (true, false) => {
                    out.push(cut(p, c));
                    out.push(c);
                }
                (false, true) => out.push(cut(p, c)),
                _ => {}
            }
        }
    }
    out
}

/// Total area of filled pieces of a colour (pieces never overlap here).
pub fn fill_area(p: &Painted, colour: [f32; 3], within: Option<[f64; 4]>) -> f64 {
    p.fills
        .iter()
        .filter(|(_, c)| (0..3).all(|k| (c[k] - colour[k]).abs() < 0.01))
        .flat_map(|(subs, _)| subs.iter())
        .map(|sp| match within {
            Some(r) => area(&clip(sp, r)).abs(),
            None => area(sp).abs(),
        })
        .sum()
}

/// The saved bytes without the covers of the redaction (to look under them).
pub fn without_covers(bytes: &[u8]) -> Vec<u8> {
    let mut doc = lopdf::Document::load_mem(bytes).unwrap();
    let pages: Vec<ObjectId> = doc.get_pages().into_values().collect();
    for pid in pages {
        let keep: Vec<Object> = doc
            .get_page_contents(pid)
            .into_iter()
            .filter(|id| {
                !doc.get_object(*id)
                    .ok()
                    .and_then(|o| o.as_stream().ok())
                    .is_some_and(is_cover)
            })
            .map(Object::Reference)
            .collect();
        doc.get_dictionary_mut(pid)
            .unwrap()
            .set("Contents", Object::Array(keep));
    }
    let mut out = Vec::new();
    doc.save_to(&mut out).unwrap();
    out
}

/// Pixels of page `page` at one pixel per point (display space).
pub fn render(bytes: &[u8], page: usize) -> image::RgbaImage {
    let doc = Document::from_bytes(bytes.to_vec(), None, None).unwrap();
    let png = doc.render_page_png(page, 1.0).unwrap();
    image::load_from_memory(&png.png).unwrap().to_rgba8()
}
