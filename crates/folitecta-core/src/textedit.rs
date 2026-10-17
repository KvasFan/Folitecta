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

use std::collections::HashMap;
use std::sync::Arc;

use hayro::hayro_interpret::font::{Glyph, GlyphRun};
use hayro::hayro_interpret::{
    BlendMode, ClipPath, Context, Device, DrawMode, DrawProps, Image, ImageDrawProps, InterpreterCache,
    InterpreterSettings, SoftMask, TransformExt, interpret_page,
};
use hayro::hayro_syntax::Pdf;
use hayro::kurbo::{Affine, BezPath, Point};
use lopdf::{Dictionary, Object, ObjectId, Stream, dictionary};

use crate::content_edit::{Splice, check_splices, hex_string, joined, name_token, rewrite};
use crate::content_lex::{name_of, operations};
use crate::document::Document;
use crate::error::{Error, Result};
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

/// A text-showing operation of the page content with the font in use.
#[derive(Debug, Clone)]
struct TextOp {
    start: usize,
    end: usize,
    operator: Vec<u8>,
    operands: Vec<(usize, usize)>,
    font: Vec<u8>,
    /// The font size operand of `Tf` as written.
    size: String,
}

const SHOW: &[&[u8]] = &[b"Tj", b"TJ", b"'", b"\""];

fn text_ops(data: &[u8]) -> Vec<TextOp> {
    let mut out = Vec::new();
    let mut font: (Vec<u8>, String) = (Vec::new(), "1".into());
    let mut stack: Vec<(Vec<u8>, String)> = Vec::new();
    for op in operations(data) {
        match op.operator {
            b"q" => stack.push(font.clone()),
            b"Q" => {
                if let Some(f) = stack.pop() {
                    font = f;
                }
            }
            b"Tf" if op.operands.len() == 2 => {
                let size = &data[op.operands[1].0..op.operands[1].1];
                font = (
                    name_of(data, op.operands[0]).unwrap_or_default(),
                    String::from_utf8_lossy(size).into_owned(),
                );
            }
            o if SHOW.contains(&o) => out.push(TextOp {
                start: op.start,
                end: op.end,
                operator: o.to_vec(),
                operands: op.operands.clone(),
                font: font.0.clone(),
                size: font.1.clone(),
            }),
            _ => {}
        }
    }
    out
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

// ---- Finding which operation drew each glyph -----------------------------------------

/// Tag of the marked content wrapped around every text-showing operation.
const PROBE: &[u8] = b"FoliTx";

/// A glyph drawn while probing: the operation that drew it (when the page
/// content drew it itself), its text and its quadrilateral (as in the text layer).
#[derive(Debug, Clone)]
struct ProbeGlyph {
    op: Option<usize>,
    text: String,
    quad: [f32; 8],
}

#[derive(Default)]
struct ProbeDevice {
    stack: Vec<Option<usize>>,
    glyphs: Vec<ProbeGlyph>,
    last_fill: Vec<(i64, i64)>,
}

impl<'a> Device<'a> for ProbeDevice {
    fn draw_path(&mut self, _path: &BezPath, _props: DrawProps<'a>, _mode: &DrawMode) {}
    fn push_clip_path(&mut self, _clip_path: &ClipPath) {}
    fn push_transparency_group(
        &mut self,
        _opacity: f32,
        _mask: Option<SoftMask<'a>>,
        _blend_mode: BlendMode,
    ) {
    }
    fn draw_glyph_run(&mut self, run: &GlyphRun<'_, 'a>, props: DrawProps<'a>, mode: &DrawMode) {
        let glyphs = run.glyphs();
        let key: Vec<(i64, i64)> = glyphs
            .iter()
            .map(|g| {
                let o = props.transform * g.transform() * Point::ZERO;
                ((o.x * 10.0) as i64, (o.y * 10.0) as i64)
            })
            .collect();
        // Fill and stroke text arrives twice; keep the first copy only.
        if matches!(mode, DrawMode::Stroke(_)) && key == self.last_fill {
            return;
        }
        if !matches!(mode, DrawMode::Stroke(_)) {
            self.last_fill = key;
        }
        let op = self.stack.iter().rev().find_map(|x| *x);
        for (i, g) in glyphs.iter().enumerate() {
            // The same geometry as the text layer, so the glyphs can be matched.
            let t: Affine = props.transform * g.transform();
            let (adv, text) = match &**g {
                Glyph::Outline(o) => (o.advance_width().map(|a| a as f64), g.as_unicode()),
                Glyph::Type3(_) => (None, g.as_unicode()),
            };
            let adv = adv.filter(|a| *a > 0.0).unwrap_or_else(|| {
                glyphs
                    .get(i + 1)
                    .map(|n| (g.transform().inverse() * n.transform() * Point::ZERO).x)
                    .filter(|d| d.is_finite() && *d > 0.0 && *d < 3000.0)
                    .unwrap_or(500.0)
            });
            let text = match text {
                Some(hayro::hayro_interpret::hayro_cmap::BfString::Char(c)) => c.to_string(),
                Some(hayro::hayro_interpret::hayro_cmap::BfString::String(s)) => s,
                None => String::new(),
            };
            let corners = [
                t * Point::new(0.0, 800.0),
                t * Point::new(adv, 800.0),
                t * Point::new(0.0, -200.0),
                t * Point::new(adv, -200.0),
            ];
            let size = ((t * Point::new(0.0, 1000.0)) - (t * Point::ZERO)).hypot();
            // The text layer leaves out glyphs without size; so does the probe,
            // but they still count among the glyphs of their operation.
            let quad = [
                corners[0].x as f32,
                corners[0].y as f32,
                corners[1].x as f32,
                corners[1].y as f32,
                corners[2].x as f32,
                corners[2].y as f32,
                corners[3].x as f32,
                corners[3].y as f32,
            ];
            self.glyphs.push(ProbeGlyph {
                op,
                text: if size.is_finite() && size >= 0.01 {
                    text
                } else {
                    String::from('\u{0}')
                },
                quad,
            });
        }
    }
    fn draw_image(&mut self, _image: Image<'a, '_>, _props: ImageDrawProps<'a>) {}
    fn pop_clip(&mut self) {}
    fn pop_transparency_group(&mut self) {}
    fn begin_marked_content(&mut self, tag: &[u8], mcid: Option<i32>) {
        self.stack
            .push((tag == PROBE).then(|| mcid.map(|m| m as usize)).flatten());
    }
    fn end_marked_content(&mut self) {
        self.stack.pop();
    }
}

/// The page content with every text-showing operation wrapped in a probe.
fn probed(data: &[u8], ops: &[TextOp]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() + ops.len() * 32);
    let mut at = 0;
    for (k, op) in ops.iter().enumerate() {
        out.extend_from_slice(&data[at..op.start]);
        out.extend_from_slice(format!("/FoliTx <</MCID {k}>> BDC ").as_bytes());
        out.extend_from_slice(&data[op.start..op.end]);
        out.extend_from_slice(b" EMC");
        at = op.end;
    }
    out.extend_from_slice(&data[at..]);
    out
}

/// Draw a page with probes; returns the glyphs in drawing order.
fn probe_glyphs(
    doc: &lopdf::Document,
    page: usize,
    page_id: ObjectId,
    content: Vec<u8>,
) -> Result<Vec<ProbeGlyph>> {
    let mut copy = doc.clone();
    let sid = copy.add_object(Stream::new(dictionary! {}, content));
    copy.get_dictionary_mut(page_id)?
        .set("Contents", Object::Reference(sid));
    let bytes = Arc::new(crate::document::serialize(&mut copy)?);
    let pdf = Pdf::new(bytes).map_err(|e| Error::Render(format!("cannot parse document: {e:?}")))?;
    let pages = pdf.pages();
    let p = pages.get(page).ok_or(Error::InvalidPage(page))?;
    let (w, h) = p.render_dimensions();
    let cache = InterpreterCache::new();
    let settings = InterpreterSettings {
        render_annotations: false,
        ..InterpreterSettings::default()
    };
    let mut ctx = Context::new(
        p.initial_transform(true).to_kurbo(),
        hayro::kurbo::Rect::new(0.0, 0.0, w as f64, h as f64),
        &cache,
        p.xref(),
        settings,
    );
    let mut dev = ProbeDevice::default();
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        interpret_page(p, &mut ctx, &mut dev);
    }))
    .map_err(|_| Error::Render(format!("the renderer failed on page {}", page + 1)))?;
    Ok(dev.glyphs)
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
        let probe = probe_glyphs(&self.inner, page, page_id, probed(&j.data, &ops))?;
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
        self.begin_edit();
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
        Ok(report)
    }
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
