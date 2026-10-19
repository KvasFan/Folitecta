//! Editing the text drawn by the page content: characters deleted, inserted
//! or replaced in the text-showing operations themselves, so the text of
//! the page changes (rather than a comment about it being added).
//!
//! The page is drawn once with every text-showing operation wrapped in a
//! marked-content probe, which tells which operation drew each glyph of the
//! text layer. The operations are then rewritten: deleted glyphs leave their
//! strings, inserted text uses the codes the page font already has for those
//! characters, and characters the font lacks are drawn with a standard font
//! of the same family, size and colour. Text after the change in the same
//! line flows with it.

use std::collections::{BTreeMap, HashMap};

use lopdf::{Dictionary, Object, ObjectId, Stream, dictionary};

use crate::content_edit::{Splice, check_splices, hex_string, joined, name_token, rewrite};
use crate::content_lex::{name_of, operations};
use crate::document::Document;
use crate::error::{Error, Result};
use crate::page_text::probe::{SHOW, probe_page_glyphs};
use crate::util::{deref, winansi_byte};

/// What a text edit did.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TextEditReport {
    /// Characters drawn with a standard font because the page font has no
    /// glyph for them (empty when the page font drew everything).
    pub substituted: String,
}

// ---- Strings of content streams ----------------------------------------------------

/// The bytes of a literal string token `( … )`.
fn literal_bytes(tok: &[u8]) -> Vec<u8> {
    let inner = &tok[1..tok.len().saturating_sub(1).max(1)];
    let mut out = Vec::with_capacity(inner.len());
    let mut i = 0;
    while i < inner.len() {
        let c = inner[i];
        if c != b'\\' {
            // An end of line inside a string is a line feed.
            if c == b'\r' {
                out.push(b'\n');
                if inner.get(i + 1) == Some(&b'\n') {
                    i += 1;
                }
            } else {
                out.push(c);
            }
            i += 1;
            continue;
        }
        i += 1;
        let Some(&e) = inner.get(i) else { break };
        match e {
            b'n' => out.push(b'\n'),
            b'r' => out.push(b'\r'),
            b't' => out.push(b'\t'),
            b'b' => out.push(8),
            b'f' => out.push(12),
            b'0'..=b'7' => {
                let mut v = 0u32;
                let mut n = 0;
                while n < 3 && i < inner.len() && (b'0'..=b'7').contains(&inner[i]) {
                    v = v * 8 + (inner[i] - b'0') as u32;
                    i += 1;
                    n += 1;
                }
                out.push((v & 0xff) as u8);
                continue;
            }
            // A backslash at the end of a line continues the string.
            b'\r' => {
                if inner.get(i + 1) == Some(&b'\n') {
                    i += 1;
                }
            }
            b'\n' => {}
            other => out.push(other),
        }
        i += 1;
    }
    out
}

/// The bytes of a hexadecimal string token `< … >`.
fn hex_bytes(tok: &[u8]) -> Vec<u8> {
    let digits: Vec<u8> = tok
        .iter()
        .filter_map(|c| (*c as char).to_digit(16).map(|d| d as u8))
        .collect();
    digits
        .chunks(2)
        .map(|p| (p[0] << 4) | p.get(1).copied().unwrap_or(0))
        .collect()
}

fn string_bytes(tok: &[u8]) -> Option<Vec<u8>> {
    match tok.first()? {
        b'(' => Some(literal_bytes(tok)),
        b'<' => Some(hex_bytes(tok)),
        _ => None,
    }
}

/// Items of a `TJ` array: strings and numbers.
#[derive(Debug, Clone, PartialEq)]
enum ArrayItem {
    Str(Vec<u8>),
    Num(f64),
}

fn array_items(tok: &[u8]) -> Option<Vec<ArrayItem>> {
    if tok.first() != Some(&b'[') {
        return None;
    }
    let body = &tok[1..tok.len().saturating_sub(1).max(1)];
    let mut out = Vec::new();
    let mut i = 0;
    while i < body.len() {
        match body[i] {
            b'(' => {
                let start = i;
                let mut level = 0;
                while i < body.len() {
                    match body[i] {
                        b'\\' => i += 1,
                        b'(' => level += 1,
                        b')' => {
                            level -= 1;
                            if level == 0 {
                                i += 1;
                                break;
                            }
                        }
                        _ => {}
                    }
                    i += 1;
                }
                out.push(ArrayItem::Str(literal_bytes(&body[start..i.min(body.len())])));
            }
            b'<' => {
                let start = i;
                while i < body.len() && body[i] != b'>' {
                    i += 1;
                }
                i = (i + 1).min(body.len());
                out.push(ArrayItem::Str(hex_bytes(&body[start..i])));
            }
            b'0'..=b'9' | b'-' | b'+' | b'.' => {
                let start = i;
                while i < body.len() && matches!(body[i], b'0'..=b'9' | b'-' | b'+' | b'.') {
                    i += 1;
                }
                let v = std::str::from_utf8(&body[start..i]).ok()?.parse::<f64>().ok()?;
                out.push(ArrayItem::Num(v));
            }
            _ => i += 1,
        }
    }
    Some(out)
}

fn fmt_kern(v: f64) -> String {
    if (v - v.round()).abs() < 1e-6 {
        format!("{}", v.round() as i64)
    } else {
        let s = format!("{v:.3}");
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

// ---- Text-showing operations ---------------------------------------------------------

/// A text-showing operation of a content stream with the text state in use.
#[derive(Debug, Clone)]
pub(crate) struct TextOp {
    pub start: usize,
    pub end: usize,
    pub operator: Vec<u8>,
    pub operands: Vec<(usize, usize)>,
    pub font: Vec<u8>,
    /// The font size operand of `Tf` as written.
    pub size: String,
    /// The font size.
    pub size_value: f64,
    /// Character and word spacing (`Tc`, `Tw`), in unscaled text space units.
    pub char_spacing: f64,
    pub word_spacing: f64,
    /// Text rendering mode (`Tr`).
    pub render_mode: i64,
    /// The text object (`BT` … `ET`) it belongs to, counted from 0.
    pub object: usize,
}

/// The text state carried by the graphics state (ISO 32000-1 §9.3): forms
/// start from the state of the operation that draws them.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TextState {
    pub font: Vec<u8>,
    pub size: String,
    pub size_value: f64,
    pub char_spacing: f64,
    pub word_spacing: f64,
    pub render_mode: i64,
}

impl Default for TextState {
    fn default() -> Self {
        TextState {
            font: Vec::new(),
            size: "1".into(),
            size_value: 1.0,
            char_spacing: 0.0,
            word_spacing: 0.0,
            render_mode: 0,
        }
    }
}

fn op_number(data: &[u8], r: (usize, usize)) -> Option<f64> {
    std::str::from_utf8(data.get(r.0..r.1)?).ok()?.trim().parse().ok()
}

impl TextState {
    /// Follow one operation that changes the text state.
    pub(crate) fn apply(&mut self, data: &[u8], operator: &[u8], operands: &[(usize, usize)]) {
        let num = |i: usize| operands.get(i).and_then(|r| op_number(data, *r));
        match operator {
            b"Tf" if operands.len() == 2 => {
                self.font = name_of(data, operands[0]).unwrap_or_default();
                self.size = String::from_utf8_lossy(&data[operands[1].0..operands[1].1]).into_owned();
                self.size_value = num(1).unwrap_or(1.0);
            }
            b"Tc" => self.char_spacing = num(0).unwrap_or(self.char_spacing),
            b"Tw" => self.word_spacing = num(0).unwrap_or(self.word_spacing),
            b"Tr" => self.render_mode = num(0).map(|v| v as i64).unwrap_or(self.render_mode),
            b"\"" if operands.len() == 3 => {
                self.word_spacing = num(0).unwrap_or(self.word_spacing);
                self.char_spacing = num(1).unwrap_or(self.char_spacing);
            }
            _ => {}
        }
    }
}

fn text_ops(data: &[u8]) -> Vec<TextOp> {
    text_ops_from(data, &TextState::default()).0
}

/// The text-showing operations of a content stream whose text state starts
/// as `init`, and where each text object ends (the end of its `ET`).
pub(crate) fn text_ops_from(data: &[u8], init: &TextState) -> (Vec<TextOp>, Vec<Option<usize>>) {
    let mut out = Vec::new();
    let mut ends: Vec<Option<usize>> = Vec::new();
    let mut state = init.clone();
    let mut stack: Vec<TextState> = Vec::new();
    let mut object = 0usize;
    for op in operations(data) {
        match op.operator {
            b"q" => stack.push(state.clone()),
            b"Q" => {
                if let Some(f) = stack.pop() {
                    state = f;
                }
            }
            b"BT" => {
                object = ends.len();
                ends.push(None);
            }
            b"ET" => {
                if let Some(e) = ends.get_mut(object) {
                    *e = Some(op.end);
                }
            }
            o => {
                state.apply(data, o, &op.operands);
                if SHOW.contains(&o) {
                    if ends.is_empty() {
                        ends.push(None);
                    }
                    out.push(TextOp {
                        start: op.start,
                        end: op.end,
                        operator: o.to_vec(),
                        operands: op.operands.clone(),
                        font: state.font.clone(),
                        size: state.size.clone(),
                        size_value: state.size_value,
                        char_spacing: state.char_spacing,
                        word_spacing: state.word_spacing,
                        render_mode: state.render_mode,
                        object,
                    });
                }
            }
        }
    }
    (out, ends)
}

/// The operation shows at least one byte of text.
pub(crate) fn shows_glyphs(data: &[u8], op: &TextOp) -> bool {
    let Some(last) = op.operands.last() else {
        return false;
    };
    let tok = &data[last.0..last.1];
    if op.operator == b"TJ" {
        array_items(tok).is_some_and(|items| {
            items
                .iter()
                .any(|i| matches!(i, ArrayItem::Str(b) if !b.is_empty()))
        })
    } else {
        string_bytes(tok).is_some_and(|b| !b.is_empty())
    }
}

/// A glyph or a position adjustment of a text-showing operation.
#[derive(Debug, Clone, PartialEq)]
enum Piece {
    Glyph(Vec<u8>),
    Kern(f64),
}

/// The pieces of an operation, with codes `width` bytes long.
fn pieces(data: &[u8], op: &TextOp, width: usize) -> Option<Vec<Piece>> {
    let last = *op.operands.last()?;
    let tok = &data[last.0..last.1];
    let items = if op.operator == b"TJ" {
        array_items(tok)?
    } else {
        vec![ArrayItem::Str(string_bytes(tok)?)]
    };
    let mut out = Vec::new();
    for it in items {
        match it {
            ArrayItem::Num(n) => out.push(Piece::Kern(n)),
            ArrayItem::Str(bytes) => {
                if bytes.len() % width != 0 {
                    return None;
                }
                out.extend(bytes.chunks(width).map(|c| Piece::Glyph(c.to_vec())));
            }
        }
    }
    Some(out)
}

// ---- Fonts ---------------------------------------------------------------------------

/// What an edit needs to know about a font of the page.
#[derive(Debug, Clone, Default)]
struct FontInfo {
    /// Codes are two bytes long (Identity encodings of composite fonts).
    two_byte: bool,
    base: String,
    flags: i64,
    /// A simple font without an embedded program and with a standard
    /// encoding: any WinAnsi character can be drawn with it.
    standard: bool,
    /// Codes of single characters, from the font's ToUnicode map.
    codes: HashMap<char, Vec<u8>>,
    /// Vertical writing (Identity-V).
    vertical: bool,
}

fn page_fonts(doc: &lopdf::Document, page_id: ObjectId) -> Dictionary {
    let mut cur = Some(page_id);
    for _ in 0..64 {
        let Some(id) = cur else { break };
        let Ok(d) = doc.get_dictionary(id) else { break };
        if let Ok(r) = d.get(b"Resources") {
            return deref(doc, r)
                .as_dict()
                .ok()
                .and_then(|res| res.get(b"Font").ok())
                .and_then(|f| deref(doc, f).as_dict().ok())
                .cloned()
                .unwrap_or_default();
        }
        cur = d.get(b"Parent").ok().and_then(|p| p.as_reference().ok());
    }
    Dictionary::new()
}

fn stream_data(s: &Stream) -> Vec<u8> {
    s.decompressed_content().unwrap_or_else(|_| s.content.clone())
}

/// The ToUnicode map of a font: code bytes and the text they stand for.
fn to_unicode(data: &[u8]) -> Vec<(Vec<u8>, String)> {
    // Tokens: hex strings, brackets and the operators that matter.
    #[derive(Debug, PartialEq)]
    enum Tok {
        Hex(Vec<u8>),
        Open,
        Close,
        Word(Vec<u8>),
    }
    let mut toks = Vec::new();
    let mut i = 0;
    while i < data.len() {
        match data[i] {
            b'<' if data.get(i + 1) != Some(&b'<') => {
                let start = i;
                while i < data.len() && data[i] != b'>' {
                    i += 1;
                }
                toks.push(Tok::Hex(hex_bytes(&data[start..(i + 1).min(data.len())])));
                i += 1;
            }
            b'[' => {
                toks.push(Tok::Open);
                i += 1;
            }
            b']' => {
                toks.push(Tok::Close);
                i += 1;
            }
            c if c.is_ascii_alphabetic() => {
                let start = i;
                while i < data.len() && data[i].is_ascii_alphanumeric() {
                    i += 1;
                }
                toks.push(Tok::Word(data[start..i].to_vec()));
            }
            _ => i += 1,
        }
    }
    let utf16 = |b: &[u8]| -> String {
        let units: Vec<u16> = b
            .chunks(2)
            .map(|p| u16::from_be_bytes([p[0], *p.get(1).unwrap_or(&0)]))
            .collect();
        String::from_utf16_lossy(&units)
    };
    let mut out = Vec::new();
    let mut k = 0;
    let mut mode = 0; // 1: bfchar, 2: bfrange
    while k < toks.len() {
        match &toks[k] {
            Tok::Word(w) if w == b"beginbfchar" => mode = 1,
            Tok::Word(w) if w == b"beginbfrange" => mode = 2,
            Tok::Word(w) if w == b"endbfchar" || w == b"endbfrange" => mode = 0,
            Tok::Hex(src) if mode == 1 => {
                if let Some(Tok::Hex(dst)) = toks.get(k + 1) {
                    out.push((src.clone(), utf16(dst)));
                    k += 1;
                }
            }
            Tok::Hex(lo) if mode == 2 => {
                let (Some(Tok::Hex(hi)), Some(next)) = (toks.get(k + 1), toks.get(k + 2)) else {
                    break;
                };
                let to_num = |b: &[u8]| b.iter().fold(0u64, |a, x| (a << 8) | *x as u64);
                let (a, b) = (to_num(lo), to_num(hi));
                let width = lo.len();
                let code =
                    |v: u64| -> Vec<u8> { (0..width).rev().map(|s| ((v >> (8 * s)) & 0xff) as u8).collect() };
                match next {
                    Tok::Hex(dst) if b >= a && b - a < 0x10000 => {
                        for (n, v) in (a..=b).enumerate() {
                            let mut d = dst.clone();
                            // The last code unit counts up.
                            if d.len() >= 2 {
                                let l = d.len();
                                let unit = u16::from_be_bytes([d[l - 2], d[l - 1]]).wrapping_add(n as u16);
                                d[l - 2..].copy_from_slice(&unit.to_be_bytes());
                            }
                            out.push((code(v), utf16(&d)));
                        }
                        k += 2;
                    }
                    Tok::Open => {
                        let mut j = k + 3;
                        let mut v = a;
                        while let Some(Tok::Hex(dst)) = toks.get(j) {
                            out.push((code(v), utf16(dst)));
                            v += 1;
                            j += 1;
                        }
                        k = j;
                    }
                    _ => k += 2,
                }
            }
            _ => {}
        }
        k += 1;
    }
    out
}

fn font_info(doc: &lopdf::Document, fonts: &Dictionary, name: &[u8]) -> Option<FontInfo> {
    let font = deref(doc, fonts.get(name).ok()?).as_dict().ok()?;
    let subtype = font.get(b"Subtype").ok()?.as_name().ok()?;
    let base = font
        .get(b"BaseFont")
        .ok()
        .and_then(|b| b.as_name().ok())
        .map(|b| String::from_utf8_lossy(b).into_owned())
        .unwrap_or_default();
    let mut info = FontInfo {
        base,
        ..Default::default()
    };
    let descriptor = |f: &Dictionary| -> Option<Dictionary> {
        f.get(b"FontDescriptor")
            .ok()
            .and_then(|d| deref(doc, d).as_dict().ok())
            .cloned()
    };
    let desc = if subtype == b"Type0" {
        let enc = font.get(b"Encoding").ok().and_then(|e| e.as_name().ok());
        if !matches!(enc, Some(b"Identity-H") | Some(b"Identity-V")) {
            return None;
        }
        info.two_byte = true;
        info.vertical = enc == Some(b"Identity-V".as_slice());
        font.get(b"DescendantFonts")
            .ok()
            .and_then(|d| deref(doc, d).as_array().ok())
            .and_then(|a| a.first())
            .and_then(|d| deref(doc, d).as_dict().ok())
            .and_then(descriptor)
    } else {
        descriptor(font)
    };
    let embedded = desc
        .as_ref()
        .is_some_and(|d| d.has(b"FontFile") || d.has(b"FontFile2") || d.has(b"FontFile3"));
    info.flags = desc
        .as_ref()
        .and_then(|d| d.get(b"Flags").ok())
        .and_then(|f| f.as_i64().ok())
        .unwrap_or(0);
    if !info.two_byte && !embedded && subtype != b"Type3" {
        info.standard = match font.get(b"Encoding").map(|e| deref(doc, e)) {
            Ok(Object::Name(n)) => n == b"WinAnsiEncoding",
            Err(_) => true,
            _ => false,
        };
    }
    if let Some(s) = font
        .get(b"ToUnicode")
        .ok()
        .and_then(|t| deref(doc, t).as_stream().ok())
    {
        for (code, text) in to_unicode(&stream_data(s)) {
            let mut chars = text.chars();
            if let (Some(c), None) = (chars.next(), chars.next()) {
                if code.len() == if info.two_byte { 2 } else { 1 } {
                    info.codes.entry(c).or_insert(code);
                }
            }
        }
    }
    Some(info)
}

fn num_of(doc: &lopdf::Document, o: &Object) -> Option<f64> {
    match deref(doc, o) {
        Object::Integer(i) => Some(*i as f64),
        Object::Real(r) => Some(*r as f64),
        _ => None,
    }
}

/// The width of the glyph of `code` in a font, in thousandths of the font
/// size (the units of `TJ` numbers), as the font dictionary gives it: the
/// `/Widths` of simple fonts (or the descriptor's `/MissingWidth`), the
/// `/Widths` of Type 3 fonts in their glyph space scaled by the font matrix,
/// the `/W` and `/DW` of the descendant of composite fonts. None when the
/// dictionary has no widths (the standard fonts).
pub(crate) fn pdf_glyph_width(doc: &lopdf::Document, font: &Dictionary, code: &[u8]) -> Option<f64> {
    let subtype = font.get(b"Subtype").ok()?.as_name().ok()?;
    if subtype == b"Type0" {
        let cid = code.iter().fold(0i64, |a, b| (a << 8) | *b as i64);
        let desc = font
            .get(b"DescendantFonts")
            .ok()
            .and_then(|d| deref(doc, d).as_array().ok())
            .and_then(|a| a.first())
            .and_then(|d| deref(doc, d).as_dict().ok())?;
        let dw = desc
            .get(b"DW")
            .ok()
            .and_then(|v| num_of(doc, v))
            .unwrap_or(1000.0);
        let Some(w) = desc.get(b"W").ok().and_then(|w| deref(doc, w).as_array().ok()) else {
            return Some(dw);
        };
        let mut i = 0;
        while i < w.len() {
            let Some(first) = num_of(doc, &w[i]).map(|v| v as i64) else {
                break;
            };
            match w.get(i + 1).map(|o| deref(doc, o)) {
                Some(Object::Array(list)) => {
                    if cid >= first && ((cid - first) as usize) < list.len() {
                        return num_of(doc, &list[(cid - first) as usize]);
                    }
                    i += 2;
                }
                Some(_) => {
                    let last = num_of(doc, &w[i + 1]).map(|v| v as i64)?;
                    let width = w.get(i + 2).and_then(|o| num_of(doc, o))?;
                    if cid >= first && cid <= last {
                        return Some(width);
                    }
                    i += 3;
                }
                None => break,
            }
        }
        return Some(dw);
    }
    let byte = *code.first()? as i64;
    let first = font
        .get(b"FirstChar")
        .ok()
        .and_then(|v| num_of(doc, v))
        .unwrap_or(0.0) as i64;
    let width = font
        .get(b"Widths")
        .ok()
        .and_then(|w| deref(doc, w).as_array().ok())
        .and_then(|w| w.get(usize::try_from(byte - first).ok()?))
        .and_then(|v| num_of(doc, v))
        .or_else(|| {
            font.get(b"FontDescriptor")
                .ok()
                .and_then(|d| deref(doc, d).as_dict().ok())
                .and_then(|d| d.get(b"MissingWidth").ok())
                .and_then(|v| num_of(doc, v))
        });
    if subtype == b"Type3" {
        // Type 3 widths are in glyph space: the font matrix makes them text space.
        let scale = font
            .get(b"FontMatrix")
            .ok()
            .and_then(|m| deref(doc, m).as_array().ok())
            .and_then(|m| m.first())
            .and_then(|v| num_of(doc, v))
            .unwrap_or(0.001);
        return width.map(|w| w * scale * 1000.0);
    }
    if font.has(b"Widths") { width } else { None }
}

/// A glyph to take out of a text-showing operation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct RemovedGlyph {
    /// Its number among the glyphs of the operation.
    pub index: usize,
    /// How far the next glyph of the operation starts (`TJ` units), as drawn.
    pub to_next: Option<f64>,
    /// The advance of its outline, when known.
    pub own_advance: Option<f64>,
}

/// Why the text of an operation cannot be taken out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FontUnsupported;

/// Glyphs taken out of the text-showing operations of a stream.
#[derive(Debug, Default)]
pub(crate) struct TextRemoval {
    pub splices: Vec<Splice>,
    /// Glyphs taken out.
    pub removed: usize,
    /// Text objects (see [`TextOp::object`]) in a clipping mode that lost
    /// every clipping glyph: they no longer clip, so the caller adds a clip
    /// that hides what they hid.
    pub emptied_clips: Vec<usize>,
}

/// Take `removed` glyphs (by operation index) out of `ops` of `data`, each
/// replaced by a move of its width so the other glyphs stay where they were.
/// The move of a glyph followed by another is the distance to it as drawn,
/// which already holds the `TJ` numbers between them (ISO 32000-1 §9.4.4):
/// those go with the glyph. The move of the last glyph of an operation is its
/// width from the font with the character and word spacing. `counts` is the
/// number of glyphs drawn by each operation. Fails for fonts whose codes
/// cannot be matched to the glyphs drawn and for vertical writing.
pub(crate) fn glyph_removal_splices(
    doc: &lopdf::Document,
    data: &[u8],
    ops: &[TextOp],
    fonts: &Dictionary,
    removed: &BTreeMap<usize, Vec<RemovedGlyph>>,
    counts: &HashMap<usize, usize>,
) -> std::result::Result<TextRemoval, FontUnsupported> {
    let mut out = TextRemoval::default();
    // Glyphs drawn and taken out by text object, in clipping modes only.
    let mut clip_glyphs: HashMap<usize, (usize, usize)> = HashMap::new();
    let clipping = |op: &TextOp| (4..=7).contains(&op.render_mode);
    for (k, op) in ops.iter().enumerate() {
        if clipping(op) {
            clip_glyphs.entry(op.object).or_default().0 += counts.get(&k).copied().unwrap_or(0);
        }
    }
    for (&k, gone) in removed {
        if gone.is_empty() {
            continue;
        }
        let op = ops.get(k).ok_or(FontUnsupported)?;
        let font = fonts
            .get(&op.font)
            .ok()
            .and_then(|f| deref(doc, f).as_dict().ok())
            .ok_or(FontUnsupported)?;
        let info = font_info(doc, fonts, &op.font).ok_or(FontUnsupported)?;
        if info.vertical {
            return Err(FontUnsupported);
        }
        let p = pieces(data, op, if info.two_byte { 2 } else { 1 }).ok_or(FontUnsupported)?;
        let glyphs = p.iter().filter(|x| matches!(x, Piece::Glyph(_))).count();
        if glyphs != counts.get(&k).copied().unwrap_or(0) {
            return Err(FontUnsupported);
        }
        let by_index: HashMap<usize, &RemovedGlyph> = gone.iter().map(|g| (g.index, g)).collect();
        let mut kept: Vec<Piece> = Vec::with_capacity(p.len());
        let mut n = 0;
        // After a removed glyph followed by another, the numbers up to that
        // glyph are part of the move already.
        let mut skip_numbers = false;
        let mut taken = 0;
        for x in p {
            let piece = match x {
                Piece::Glyph(code) => {
                    let g = by_index.get(&n).copied();
                    n += 1;
                    match g {
                        Some(g) => {
                            taken += 1;
                            let advance = match g.to_next {
                                Some(d) => {
                                    skip_numbers = true;
                                    d
                                }
                                None => {
                                    skip_numbers = false;
                                    if op.size_value.abs() < 1e-9 {
                                        return Err(FontUnsupported);
                                    }
                                    let w = pdf_glyph_width(doc, font, &code)
                                        .or(g.own_advance)
                                        .unwrap_or(500.0);
                                    let space = if code == [32] { op.word_spacing } else { 0.0 };
                                    w + 1000.0 * (op.char_spacing + space) / op.size_value
                                }
                            };
                            // A move to the right is a negative number in a TJ array.
                            Piece::Kern(-advance)
                        }
                        None => {
                            skip_numbers = false;
                            Piece::Glyph(code)
                        }
                    }
                }
                Piece::Kern(v) => {
                    if skip_numbers {
                        continue;
                    }
                    Piece::Kern(v)
                }
            };
            // Moves next to each other become one.
            match (kept.last_mut(), &piece) {
                (Some(Piece::Kern(prev)), Piece::Kern(v)) => *prev += v,
                _ => kept.push(piece),
            }
        }
        out.removed += taken;
        if clipping(op) {
            clip_glyphs.entry(op.object).or_default().1 += taken;
        }
        let mut text = String::new();
        match op.operator.as_slice() {
            b"'" => text.push_str("T* "),
            b"\"" if op.operands.len() == 3 => {
                let raw = |r: (usize, usize)| String::from_utf8_lossy(&data[r.0..r.1]).into_owned();
                text.push_str(&format!(
                    "{} Tw {} Tc T* ",
                    raw(op.operands[0]),
                    raw(op.operands[1])
                ));
            }
            _ => {}
        }
        text.push('[');
        for x in &kept {
            match x {
                Piece::Glyph(c) => text.push_str(&hex_string(c)),
                Piece::Kern(v) => {
                    text.push(' ');
                    text.push_str(&fmt_kern(*v));
                    text.push(' ');
                }
            }
        }
        text.push_str("] TJ");
        out.splices.push(Splice {
            start: op.start,
            end: op.end,
            with: text.into_bytes(),
        });
    }
    let mut emptied: Vec<usize> = clip_glyphs
        .into_iter()
        .filter(|(_, (drawn, taken))| *drawn > 0 && taken >= drawn)
        .map(|(o, _)| o)
        .collect();
    emptied.sort_unstable();
    out.emptied_clips = emptied;
    Ok(out)
}

/// The standard font of the family of a page font, for the characters it lacks.
fn fallback_font(info: &FontInfo) -> &'static str {
    let name = info
        .base
        .split_once('+')
        .map(|(_, n)| n)
        .unwrap_or(&info.base)
        .to_lowercase();
    let bold = ["bold", "black", "heavy", "semibold", "demi"]
        .iter()
        .any(|w| name.contains(w))
        || info.flags & (1 << 18) != 0;
    let italic = name.contains("italic") || name.contains("oblique") || info.flags & (1 << 6) != 0;
    let mono = ["courier", "mono", "consol", "code"]
        .iter()
        .any(|w| name.contains(w))
        || info.flags & 1 != 0;
    let serif = !mono
        && ([
            "times", "roman", "georgia", "garamond", "cambria", "book", "palatino", "minion",
        ]
        .iter()
        .any(|w| name.contains(w))
            || (name.contains("serif") && !name.contains("sans"))
            || (info.flags & 2 != 0 && !name.contains("sans")));
    match (mono, serif, bold, italic) {
        (true, _, false, false) => "Courier",
        (true, _, true, false) => "Courier-Bold",
        (true, _, false, true) => "Courier-Oblique",
        (true, _, true, true) => "Courier-BoldOblique",
        (false, true, false, false) => "Times-Roman",
        (false, true, true, false) => "Times-Bold",
        (false, true, false, true) => "Times-Italic",
        (false, true, true, true) => "Times-BoldItalic",
        (false, false, false, false) => "Helvetica",
        (false, false, true, false) => "Helvetica-Bold",
        (false, false, false, true) => "Helvetica-Oblique",
        (false, false, true, true) => "Helvetica-BoldOblique",
    }
}

/// The WinAnsi code of a character, when it has one.
fn winansi(c: char) -> Option<u8> {
    let b = winansi_byte(c);
    (b != b'?' || c == '?').then_some(b)
}

fn same_quad(a: &[f32; 8], b: &[f32; 8]) -> bool {
    a.iter().zip(b).all(|(x, y)| (x - y).abs() < 0.05)
}

// ---- The edit ------------------------------------------------------------------------

/// An inserted run: codes of the page font, or text for the standard font.
enum Run {
    Same(Vec<Vec<u8>>),
    Fallback(String),
}

impl Document {
    /// Replace characters `start..end` of the text of a page (as in its text
    /// layer) with `text`: an empty range inserts, an empty text deletes.
    pub fn edit_page_text(
        &mut self,
        page: usize,
        start: usize,
        end: usize,
        text: &str,
    ) -> Result<TextEditReport> {
        self.edit_text_of(page, start, end, text, true)
    }

    /// [`Document::edit_page_text`], recording an undo step or (for the
    /// edits after the first of a bulk change) not.
    fn edit_text_of(
        &mut self,
        page: usize,
        start: usize,
        end: usize,
        text: &str,
        record_undo: bool,
    ) -> Result<TextEditReport> {
        let layer = self.text_layer(page).ok_or(Error::InvalidPage(page))?;
        let (start, end) = (start.min(end), start.max(end).min(layer.char_glyph.len()));
        if start == end && text.is_empty() {
            return Ok(TextEditReport::default());
        }
        let glyph_of = |i: usize| layer.char_glyph.get(i).copied().flatten();
        // Glyphs removed, and where the new text goes: before or after a glyph.
        let mut removed: Vec<usize> = (start..end).filter_map(glyph_of).collect();
        removed.dedup();
        let anchor = match removed.first() {
            Some(g) => (*g, false),
            None => match glyph_of(start) {
                Some(g) => (g, false),
                None => match start.checked_sub(1).and_then(glyph_of) {
                    Some(g) => (g, true),
                    None => return Err(Error::Invalid("there is no text at this position".into())),
                },
            },
        };
        // A space the text layer found between two pieces of text, chosen
        // with the text next to it: the gap goes too.
        let gap_after = end > start && glyph_of(end - 1).is_none();
        let gap_before = end > start && glyph_of(start).is_none();

        let page_id = self.page_id(page)?;
        let j = joined(&self.inner, page_id);
        let ops = text_ops(&j.data);
        if ops.is_empty() {
            return Err(Error::Invalid("this text is not drawn by the page itself".into()));
        }
        let probe = probe_page_glyphs(&self.inner, page)?;
        // Where every glyph of the text layer comes from: operation and index in it.
        let mut index_in_op: Vec<usize> = Vec::with_capacity(probe.len());
        let mut counts: HashMap<usize, usize> = HashMap::new();
        for g in &probe {
            let n = g.op.map(|o| {
                let c = counts.entry(o).or_insert(0);
                *c += 1;
                *c - 1
            });
            index_in_op.push(n.unwrap_or(usize::MAX));
        }
        let visible: Vec<usize> = (0..probe.len()).filter(|i| probe[*i].text != "\u{0}").collect();
        let locate = |gi: usize| -> Option<(usize, usize)> {
            let lg = layer.glyphs.get(gi)?;
            // The same order is expected; look around it otherwise.
            let guess = visible.get(gi).copied();
            let hit = guess
                .filter(|p| same_quad(&probe[*p].quad, &lg.quad) && probe[*p].text == lg.text)
                .or_else(|| {
                    visible
                        .iter()
                        .copied()
                        .find(|p| same_quad(&probe[*p].quad, &lg.quad) && probe[*p].text == lg.text)
                })?;
            Some((probe[hit].op?, index_in_op[hit]))
        };
        let outside =
            || Error::Invalid("this text is inside a drawing the page reuses and cannot be edited".into());
        let mut removal: HashMap<usize, Vec<usize>> = HashMap::new();
        for g in &removed {
            let (op, i) = locate(*g).ok_or_else(outside)?;
            removal.entry(op).or_default().push(i);
        }
        let (anchor_op, anchor_i) = locate(anchor.0).ok_or_else(outside)?;

        // Fonts of the operations involved and the codes their glyphs use.
        let fonts = page_fonts(&self.inner, page_id);
        let mut infos: HashMap<Vec<u8>, FontInfo> = HashMap::new();
        let unsupported =
            || Error::Invalid("this text uses a font encoding Folitecta cannot edit yet".into());
        let mut parsed: HashMap<usize, Vec<Piece>> = HashMap::new();
        for k in removal.keys().copied().chain(std::iter::once(anchor_op)) {
            if parsed.contains_key(&k) {
                continue;
            }
            let op = &ops[k];
            if !infos.contains_key(&op.font) {
                let info = font_info(&self.inner, &fonts, &op.font).ok_or_else(unsupported)?;
                infos.insert(op.font.clone(), info);
            }
            let width = if infos[&op.font].two_byte { 2 } else { 1 };
            let p = pieces(&j.data, op, width).ok_or_else(unsupported)?;
            let glyphs = p.iter().filter(|x| matches!(x, Piece::Glyph(_))).count();
            if glyphs != counts.get(&k).copied().unwrap_or(0) {
                return Err(Error::Invalid(
                    "the glyphs of this text could not be matched to its codes".into(),
                ));
            }
            parsed.insert(k, p);
        }
        // Codes the anchor font is seen to use on the page, by character.
        let anchor_font = ops[anchor_op].font.clone();
        let info = infos[&anchor_font].clone();
        let width = if info.two_byte { 2 } else { 1 };
        let mut learned: HashMap<char, Vec<u8>> = HashMap::new();
        for (k, op) in ops.iter().enumerate() {
            if op.font != anchor_font {
                continue;
            }
            let Some(p) = pieces(&j.data, op, width) else {
                continue;
            };
            let codes: Vec<&Vec<u8>> = p
                .iter()
                .filter_map(|x| match x {
                    Piece::Glyph(c) => Some(c),
                    _ => None,
                })
                .collect();
            let texts: Vec<&str> = probe
                .iter()
                .filter(|g| g.op == Some(k))
                .map(|g| g.text.as_str())
                .collect();
            if codes.len() != texts.len() {
                continue;
            }
            for (code, t) in codes.into_iter().zip(texts) {
                let mut cs = t.chars();
                if let (Some(c), None) = (cs.next(), cs.next()) {
                    learned.entry(c).or_insert_with(|| code.clone());
                }
            }
        }
        // The new text, as runs of the page font and of the standard font.
        let mut runs: Vec<Run> = Vec::new();
        let mut report = TextEditReport::default();
        for c in text.chars() {
            let code = learned
                .get(&c)
                .or_else(|| info.codes.get(&c))
                .cloned()
                .or_else(|| {
                    (info.standard && !info.two_byte)
                        .then(|| winansi(c).map(|b| vec![b]))
                        .flatten()
                });
            match (code, runs.last_mut()) {
                (Some(code), Some(Run::Same(v))) => v.push(code),
                (Some(code), _) => runs.push(Run::Same(vec![code])),
                (None, Some(Run::Fallback(s))) => {
                    s.push(c);
                    report.substituted.push(c);
                }
                (None, _) => {
                    runs.push(Run::Fallback(c.to_string()));
                    report.substituted.push(c);
                }
            }
        }
        let fallback = (!report.substituted.is_empty()).then(|| fallback_font(&info));
        let fallback_name = fallback.map(|f| format!("FoliTx-{f}").into_bytes());

        // The operations rewritten.
        let mut splices = Vec::new();
        let mut keys: Vec<usize> = parsed.keys().copied().collect();
        keys.sort_unstable();
        for k in keys {
            let op = &ops[k];
            let p = &parsed[&k];
            let gone: Vec<usize> = removal.get(&k).cloned().unwrap_or_default();
            // Glyph number of every piece (None for adjustments).
            let mut numbers = Vec::with_capacity(p.len());
            let mut n = 0;
            for x in p {
                match x {
                    Piece::Glyph(_) => {
                        numbers.push(Some(n));
                        n += 1;
                    }
                    Piece::Kern(_) => numbers.push(None),
                }
            }
            let prev_glyph = |at: usize| (0..at).rev().find_map(|q| numbers[q]);
            let next_glyph = |at: usize| (at + 1..p.len()).find_map(|q| numbers[q]);
            let last_gone = gone.iter().max().copied();
            let first_gone = gone.iter().min().copied();
            let mut before: Vec<Piece> = Vec::new();
            let mut after: Vec<Piece> = Vec::new();
            let mut past = false;
            for (q, x) in p.iter().enumerate() {
                // The new text starts where the anchor glyph was (or after it).
                if k == anchor_op && !past {
                    let here = match numbers[q] {
                        Some(g) if !anchor.1 => g == anchor_i,
                        _ => false,
                    };
                    if here {
                        past = true;
                    }
                }
                let keep = match (x, numbers[q]) {
                    (Piece::Glyph(_), Some(g)) => !gone.contains(&g),
                    (Piece::Kern(_), _) => {
                        let (a, b) = (prev_glyph(q), next_glyph(q));
                        let between =
                            a.is_some_and(|a| gone.contains(&a)) && b.is_some_and(|b| gone.contains(&b));
                        let trailing = gap_after && a.is_some() && a == last_gone;
                        let leading = gap_before
                            && b.is_some()
                            && b == first_gone
                            && a.is_none_or(|a| !gone.contains(&a));
                        !(between || trailing || leading)
                    }
                    _ => true,
                };
                if keep {
                    if past {
                        after.push(x.clone())
                    } else {
                        before.push(x.clone())
                    }
                }
                if k == anchor_op && anchor.1 && !past && numbers[q] == Some(anchor_i) {
                    past = true;
                }
            }
            if k != anchor_op {
                // Only deletions here: everything stays in one array.
                before.append(&mut after);
            }
            let mut out = String::new();
            match op.operator.as_slice() {
                b"'" => out.push_str("T* "),
                b"\"" if op.operands.len() == 3 => {
                    let raw = |r: (usize, usize)| String::from_utf8_lossy(&j.data[r.0..r.1]).into_owned();
                    out.push_str(&format!(
                        "{} Tw {} Tc T* ",
                        raw(op.operands[0]),
                        raw(op.operands[1])
                    ));
                }
                _ => {}
            }
            let array = |v: &[Piece]| -> String {
                let mut s = String::from("[");
                for x in v {
                    match x {
                        Piece::Glyph(c) => s.push_str(&hex_string(c)),
                        Piece::Kern(n) => {
                            s.push(' ');
                            s.push_str(&fmt_kern(*n));
                            s.push(' ');
                        }
                    }
                }
                s.push_str("] TJ ");
                s
            };
            if !before.is_empty() {
                out.push_str(&array(&before));
            }
            if k == anchor_op {
                for r in &runs {
                    match r {
                        Run::Same(codes) => {
                            let v: Vec<Piece> = codes.iter().cloned().map(Piece::Glyph).collect();
                            out.push_str(&array(&v));
                        }
                        Run::Fallback(s) => {
                            let bytes: Vec<u8> = s.chars().map(winansi_byte).collect();
                            out.push_str(&format!(
                                "{} {} Tf {} Tj {} {} Tf ",
                                name_token(fallback_name.as_deref().unwrap_or(b"F")),
                                op.size,
                                hex_string(&bytes),
                                name_token(&op.font),
                                op.size
                            ));
                        }
                    }
                }
                if !after.is_empty() {
                    out.push_str(&array(&after));
                }
            }
            splices.push(Splice {
                start: op.start,
                end: op.end,
                with: out.trim_end().as_bytes().to_vec(),
            });
        }
        check_splices(&j, &splices)?;
        if record_undo {
            self.begin_edit();
        }
        let before = self.revision();
        if let (Some(font), Some(name)) = (fallback, fallback_name) {
            self.inherit_attributes(&[page_id]);
            let fid = self.inner.add_object(dictionary! {
                "Type" => "Font",
                "Subtype" => "Type1",
                "BaseFont" => font,
                "Encoding" => "WinAnsiEncoding",
            });
            self.add_page_resource(
                page_id,
                "Font",
                &String::from_utf8_lossy(&name),
                Object::Reference(fid),
            )?;
        }
        rewrite(&mut self.inner, page_id, &j, &splices)?;
        self.commit()?;
        // Only this page changed: its text is extracted again, not every page's.
        self.refresh_text_layer(page, before);
        Ok(report)
    }

    /// Replace every occurrence of `query` in the text of `pages` (all when
    /// None) with `with`, as one undo step. Occurrences whose text cannot be
    /// changed are left and counted, with the first reason; so are
    /// occurrences that begin or end inside a glyph drawing several letters
    /// (a ligature such as "ffi"), which an edit would cut.
    pub fn replace_page_text(
        &mut self,
        query: &str,
        with: &str,
        opts: crate::text::SearchOptions,
        pages: Option<&[usize]>,
    ) -> Result<ReplaceReport> {
        if query.is_empty() {
            return Err(Error::Invalid("nothing to find".into()));
        }
        let all: Vec<usize> = match pages {
            Some(p) => p.iter().copied().filter(|p| *p < self.page_count()).collect(),
            None => (0..self.page_count()).collect(),
        };
        let mut report = ReplaceReport::default();
        // One undo step: the state before the first edit that succeeds.
        let mut recorded = false;
        for page in all {
            let Some(layer) = self.text_layer(page) else {
                continue;
            };
            let glyph = |i: usize| layer.char_glyph.get(i).copied().flatten();
            let cuts_glyph = |at: usize| at > 0 && glyph(at).is_some() && glyph(at) == glyph(at - 1);
            // From the last occurrence back: an edit moves only the text after it.
            let hits = layer.find(query, opts);
            for (start, end) in hits.into_iter().rev() {
                if cuts_glyph(start) || cuts_glyph(end) {
                    report.refused += 1;
                    report.reason.get_or_insert_with(|| {
                        "the text begins or ends inside a glyph of several letters".into()
                    });
                    continue;
                }
                match self.edit_text_of(page, start, end, with, !recorded) {
                    Ok(r) => {
                        recorded = true;
                        report.replaced += 1;
                        for c in r.substituted.chars() {
                            if !report.substituted.contains(c) {
                                report.substituted.push(c);
                            }
                        }
                    }
                    Err(e) => {
                        report.refused += 1;
                        report.reason.get_or_insert_with(|| e.to_string());
                    }
                }
            }
        }
        Ok(report)
    }
}

/// What a replacement in the text of the pages did.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ReplaceReport {
    pub replaced: usize,
    /// Occurrences left as they were.
    pub refused: usize,
    /// Why the first refused occurrence could not be changed.
    pub reason: Option<String>,
    /// Characters drawn with a standard font (see [`TextEditReport`]).
    pub substituted: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strings_and_arrays() {
        assert_eq!(literal_bytes(b"(a\\(b\\)\\101\\\\)"), b"a(b)A\\".to_vec());
        assert_eq!(hex_bytes(b"<41 42 4>"), vec![0x41, 0x42, 0x40]);
        assert_eq!(
            array_items(b"[(He) -120 <006C> 3.5]"),
            Some(vec![
                ArrayItem::Str(b"He".to_vec()),
                ArrayItem::Num(-120.0),
                ArrayItem::Str(vec![0, 0x6c]),
                ArrayItem::Num(3.5)
            ])
        );
        assert_eq!(fmt_kern(-120.0), "-120");
        assert_eq!(fmt_kern(2.25), "2.25");
    }

    #[test]
    fn unicode_maps() {
        let map = b"/CIDInit /ProcSet findresource begin 2 beginbfchar <0003> <0020> <0024> <0041> endbfchar 1 beginbfrange <0044> <0046> <0061> endbfrange 1 beginbfrange <0050> <0051> [<00E8> <20AC>] endbfrange end";
        let m = to_unicode(map);
        assert!(m.contains(&(vec![0, 3], " ".into())));
        assert!(m.contains(&(vec![0, 0x24], "A".into())));
        assert!(m.contains(&(vec![0, 0x45], "b".into())));
        assert!(m.contains(&(vec![0, 0x51], "€".into())));
    }

    #[test]
    fn standard_fonts_by_family() {
        let f = |base: &str, flags: i64| {
            fallback_font(&FontInfo {
                base: base.into(),
                flags,
                ..Default::default()
            })
        };
        assert_eq!(f("ABCDEF+Arial-BoldMT", 0), "Helvetica-Bold");
        assert_eq!(f("TimesNewRomanPS-ItalicMT", 0), "Times-Italic");
        assert_eq!(f("Consolas", 0), "Courier");
        assert_eq!(f("SourceSerifPro", 0), "Times-Roman");
        assert_eq!(f("Calibri", 0), "Helvetica");
    }
}
