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
