//! Fonts a document names without carrying them, embedded for PDF/A
//! (ISO 19005-2, 6.2.11.4): a simple font (Type 1 or TrueType, the standard
//! Helvetica, Times and Courier included) is drawn with the GNU FreeFont face
//! of the same family and style — FreeSans, FreeSerif or FreeMono, made from
//! the URW fonts that have the metrics of those three — and a subset of that
//! face goes into the file as a TrueType font. Its glyphs take the widths the
//! document already lays its text out with, so no line moves, and the widths
//! of the font dictionary and of the program agree (6.2.11.5).
//!
//! Fonts left as they are: composite and Type 3 fonts, Symbol and
//! ZapfDingbats, symbolic fonts with codes of their own, encodings other than
//! the standard ones, and fonts whose substitute file is missing.

use std::collections::{BTreeMap, BTreeSet};

use lopdf::{Dictionary, Object, ObjectId, Stream, dictionary};

use super::embed::{subset_key, subset_tag};
use super::encodings::{GLYPHS, MAC_ROMAN, STANDARD, WIN_ANSI};
use super::source::font_file;
use super::subset::subset_with_advances;
use crate::document::Document;
use crate::error::{Error, Result};
use crate::util::deref;

/// The fourteen fonts readers bring.
const STANDARD_FONTS: [&str; 14] = [
    "Courier",
    "Courier-Bold",
    "Courier-BoldOblique",
    "Courier-Oblique",
    "Helvetica",
    "Helvetica-Bold",
    "Helvetica-BoldOblique",
    "Helvetica-Oblique",
    "Times-Roman",
    "Times-Bold",
    "Times-BoldItalic",
    "Times-Italic",
    "Symbol",
    "ZapfDingbats",
];

/// The width a reader gives a glyph of a standard font that is not
/// embedded (1/1000 em), when the font is Helvetica, Times or Courier.
fn standard_width(font: &str, glyph: &str) -> Option<f32> {
    use super::standard_metrics as m;
    let table = match font {
        "Courier" | "Courier-Bold" | "Courier-Oblique" | "Courier-BoldOblique" => return Some(600.0),
        "Helvetica" | "Helvetica-Oblique" => m::HELVETICA,
        "Helvetica-Bold" | "Helvetica-BoldOblique" => m::HELVETICA_BOLD,
        "Times-Roman" => m::TIMES_ROMAN,
        "Times-Bold" => m::TIMES_BOLD,
        "Times-Italic" => m::TIMES_ITALIC,
        "Times-BoldItalic" => m::TIMES_BOLD_ITALIC,
        _ => return None,
    };
    table
        .binary_search_by(|(n, _)| n.cmp(&glyph))
        .ok()
        .map(|i| f32::from(table[i].1))
}

/// The character of a glyph name: from the Adobe Glyph List, or `uniXXXX`
/// and `uXXXX` to `uXXXXXX`.
pub(crate) fn glyph_char(name: &str) -> Option<char> {
    if let Ok(i) = GLYPHS.binary_search_by(|(n, _)| n.cmp(&name)) {
        return Some(GLYPHS[i].1);
    }
    let hex = name
        .strip_prefix("uni")
        .filter(|h| h.len() == 4)
        .or_else(|| name.strip_prefix('u').filter(|h| (4..=6).contains(&h.len())))?;
    u32::from_str_radix(hex, 16).ok().and_then(char::from_u32)
}

/// The family of a substitute face.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Family {
    Sans,
    Serif,
    Mono,
}

/// The GNU FreeFont file of a family and style.
pub(crate) fn substitute_file(family: Family, bold: bool, italic: bool) -> String {
    let base = match family {
        Family::Sans => "FreeSans",
        Family::Serif => "FreeSerif",
        Family::Mono => "FreeMono",
    };
    let slant = if family == Family::Serif {
        "Italic"
    } else {
        "Oblique"
    };
    let style = match (bold, italic) {
        (false, false) => String::new(),
        (true, false) => "Bold".into(),
        (false, true) => slant.into(),
        (true, true) => format!("Bold{slant}"),
    };
    format!("{base}{style}.ttf")
}

fn name_of<'a>(d: &'a Dictionary, key: &[u8]) -> Option<&'a [u8]> {
    d.get(key).ok().and_then(|o| o.as_name().ok())
}

fn number(o: &Object) -> Option<f32> {
    match o {
        Object::Integer(i) => Some(*i as f32),
        Object::Real(r) => Some(*r),
        _ => None,
    }
}

/// The name of a font without the tag of a subset.
fn bare_name(font: &Dictionary) -> String {
    let name = String::from_utf8_lossy(name_of(font, b"BaseFont").unwrap_or_default()).to_string();
    match name.split_once('+') {
        Some((tag, rest)) if tag.len() == 6 && tag.bytes().all(|b| b.is_ascii_uppercase()) => {
            rest.to_string()
        }
        _ => name,
    }
}

/// Family and style of a font, from its name and its descriptor.
fn face_of(name: &str, descriptor: Option<&Dictionary>) -> (Family, bool, bool) {
    let lower = name.to_ascii_lowercase();
    let has = |words: &[&str]| words.iter().any(|w| lower.contains(w));
    let get = |key: &[u8]| descriptor.and_then(|d| d.get(key).ok()).and_then(number);
    let flags = get(b"Flags").unwrap_or(0.0) as i64;
    let mono = has(&["courier", "mono", "consola", "typewriter", "menlo", "fixed"]) || flags & 1 != 0;
    let serif = !has(&["sans", "gothic", "grotesk"])
        && (has(&[
            "times", "serif", "roman", "georgia", "garamond", "cambria", "palatino", "book", "minion",
        ]) || flags & 2 != 0);
    let family = if mono {
        Family::Mono
    } else if serif {
        Family::Serif
    } else {
        Family::Sans
    };
    let bold = has(&["bold", "black", "heavy", "demi"])
        || flags & (1 << 18) != 0
        || get(b"FontWeight").is_some_and(|w| w >= 600.0);
    let italic =
        has(&["italic", "oblique"]) || flags & 64 != 0 || get(b"ItalicAngle").is_some_and(|a| a != 0.0);
    (family, bold, italic)
}

/// The glyph name of every code of a simple font, from its encoding.
fn code_names(doc: &lopdf::Document, font: &Dictionary) -> Option<Vec<String>> {
    let table = |name: &[u8]| -> Option<&'static [&'static str; 256]> {
        match name {
            b"StandardEncoding" => Some(&STANDARD),
            b"WinAnsiEncoding" => Some(&WIN_ANSI),
            b"MacRomanEncoding" => Some(&MAC_ROMAN),
            _ => None,
        }
    };
    let encoding = font.get(b"Encoding").ok().map(|e| deref(doc, e));
    let (base, differences) = match encoding {
        None => (&STANDARD, None),
        Some(Object::Name(n)) => (table(n)?, None),
        Some(Object::Dictionary(d)) => (
            match name_of(d, b"BaseEncoding") {
                Some(n) => table(n)?,
                None => &STANDARD,
            },
            d.get(b"Differences")
                .ok()
                .and_then(|a| deref(doc, a).as_array().ok()),
        ),
        Some(_) => return None,
    };
    let mut names: Vec<String> = base.iter().map(|n| n.to_string()).collect();
    let mut code = 0usize;
    for item in differences.into_iter().flatten() {
        match deref(doc, item) {
            Object::Integer(c) => code = (*c).clamp(0, 255) as usize,
            Object::Name(n) => {
                if let Some(slot) = names.get_mut(code) {
                    *slot = String::from_utf8_lossy(n).to_string();
                }
                code += 1;
            }
            _ => return None,
        }
    }
    Some(names)
}

/// The descendant font of a composite font that can be embedded, and its
/// character of every code: Identity-H (codes are two-byte CIDs), a ToUnicode
/// map to read the characters from, a descendant without a font program.
fn composite_parts(doc: &lopdf::Document, font: &Dictionary) -> Option<(Dictionary, BTreeMap<u16, char>)> {
    if name_of(font, b"Encoding") != Some(b"Identity-H") {
        return None;
    }
    let descendant = font
        .get(b"DescendantFonts")
        .ok()
        .and_then(|d| deref(doc, d).as_array().ok())
        .and_then(|a| a.first())
        .and_then(|d| deref(doc, d).as_dict().ok())?
        .clone();
    if !matches!(
        name_of(&descendant, b"Subtype"),
        Some(b"CIDFontType0" | b"CIDFontType2")
    ) {
        return None;
    }
    let stream = deref(doc, font.get(b"ToUnicode").ok()?).as_stream().ok()?;
    let data = stream
        .decompressed_content()
        .unwrap_or_else(|_| stream.content.clone());
    let map: BTreeMap<u16, char> = crate::page_text::textedit::to_unicode(&data)
        .into_iter()
        .filter_map(|(code, text)| {
            let mut chars = text.chars();
            let c = chars.next()?;
            (code.len() == 2 && chars.next().is_none()).then(|| (u16::from_be_bytes([code[0], code[1]]), c))
        })
        .collect();
    (!map.is_empty()).then_some((descendant, map))
}

/// The widths of a CIDFont by CID (`/W`, `/DW`, 1/1000 em).
fn cid_width(doc: &lopdf::Document, descendant: &Dictionary, cid: u16) -> f32 {
    let default = descendant.get(b"DW").ok().and_then(number).unwrap_or(1000.0);
    let Some(w) = descendant
        .get(b"W")
        .ok()
        .and_then(|w| deref(doc, w).as_array().ok())
    else {
        return default;
    };
    let cid = f32::from(cid);
    let mut i = 0;
    while i < w.len() {
        let Some(first) = number(deref(doc, &w[i])) else {
            break;
        };
        match w.get(i + 1).map(|o| deref(doc, o)) {
            Some(Object::Array(list)) => {
                let at = cid - first;
                if at >= 0.0 && (at as usize) < list.len() {
                    return number(deref(doc, &list[at as usize])).unwrap_or(default);
                }
                i += 2;
            }
            Some(last) => {
                let last = number(last).unwrap_or(-1.0);
                if (first..=last).contains(&cid) {
                    return w
                        .get(i + 2)
                        .and_then(|x| number(deref(doc, x)))
                        .unwrap_or(default);
                }
                i += 3;
            }
            None => break,
        }
    }
    default
}

/// The built-in encoding of Symbol or ZapfDingbats, when `font` is one of
/// them without an encoding of its own.
fn symbolic_table(font: &Dictionary) -> Option<&'static [(u8, &'static str, char, u16)]> {
    if font.has(b"Encoding") {
        return None;
    }
    match bare_name(font).as_str() {
        "Symbol" => Some(super::symbolic::SYMBOL),
        "ZapfDingbats" => Some(super::symbolic::ZAPF_DINGBATS),
        _ => None,
    }
}

/// Whether the font at `font` is one [`Document::embed_substitute`] can
/// embed (without reading any font file).
pub(crate) fn can_substitute(doc: &lopdf::Document, font: &Dictionary) -> bool {
    if name_of(font, b"Subtype") == Some(b"Type0") {
        return composite_parts(doc, font).is_some();
    }
    let simple = matches!(
        name_of(font, b"Subtype"),
        Some(b"Type1" | b"MMType1" | b"TrueType")
    );
    if !simple {
        return false;
    }
    let name = bare_name(font);
    if name == "Symbol" || name == "ZapfDingbats" {
        return symbolic_table(font).is_some();
    }
    let descriptor = font
        .get(b"FontDescriptor")
        .ok()
        .and_then(|d| deref(doc, d).as_dict().ok());
    let flags = descriptor
        .and_then(|d| d.get(b"Flags").ok())
        .and_then(number)
        .unwrap_or(32.0) as i64;
    // A symbolic font has codes of its own, unless it is a standard one.
    let symbolic = flags & 4 != 0 && flags & 32 == 0;
    (!symbolic || STANDARD_FONTS.contains(&name.as_str())) && code_names(doc, font).is_some()
}

/// The codes the document shows with the simple font at `font`.
fn used_codes(doc: &lopdf::Document, font: ObjectId) -> BTreeSet<u8> {
    used_strings(doc, font).into_iter().flatten().collect()
}

/// The strings the document shows with the font at `font`: in the content
/// of its pages and of the forms (appearances included) whose resources name it.
fn used_strings(doc: &lopdf::Document, font: ObjectId) -> Vec<Vec<u8>> {
    use lopdf::content::Content;
    let mut out = Vec::new();
    for (id, obj) in &doc.objects {
        let (resources, content) = match obj {
            Object::Dictionary(d) if name_of(d, b"Type") == Some(b"Page") => (
                crate::page_text::probe::page_resources(doc, *id),
                doc.get_page_content(*id),
            ),
            Object::Stream(s) if name_of(&s.dict, b"Subtype") == Some(b"Form") => (
                s.dict
                    .get(b"Resources")
                    .ok()
                    .and_then(|r| deref(doc, r).as_dict().ok())
                    .cloned()
                    .unwrap_or_default(),
                s.decompressed_content().unwrap_or_else(|_| s.content.clone()),
            ),
            _ => continue,
        };
        let names: Vec<Vec<u8>> = resources
            .get(b"Font")
            .ok()
            .and_then(|f| deref(doc, f).as_dict().ok())
            .map(|f| {
                f.iter()
                    .filter(|(_, v)| v.as_reference().ok() == Some(font))
                    .map(|(k, _)| k.clone())
                    .collect()
            })
            .unwrap_or_default();
        if names.is_empty() {
            continue;
        }
        let Ok(ops) = Content::decode(&content) else {
            continue;
        };
        let mut current: Option<Vec<u8>> = None;
        let take = |o: &Object, out: &mut Vec<Vec<u8>>| {
            if let Object::String(bytes, _) = o {
                out.push(bytes.clone());
            }
        };
        for op in ops.operations {
            match op.operator.as_str() {
                "Tf" => {
                    current = op
                        .operands
                        .first()
                        .and_then(|n| n.as_name().ok())
                        .map(<[u8]>::to_vec)
                }
                _ if !current.as_ref().is_some_and(|c| names.contains(c)) => {}
                "Tj" | "'" => op.operands.first().into_iter().for_each(|o| take(o, &mut out)),
                "\"" => op.operands.get(2).into_iter().for_each(|o| take(o, &mut out)),
                "TJ" => {
                    if let Some(Object::Array(a)) = op.operands.first() {
                        a.iter().for_each(|o| take(o, &mut out));
                    }
                }
                _ => {}
            }
        }
    }
    out
}

/// A ToUnicode CMap of one-byte codes (ISO 32000-1, 9.10.3).
fn byte_to_unicode(codes: &BTreeMap<u8, char>) -> Vec<u8> {
    let mut out = String::from(
        "/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n\
         /CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n\
         /CMapName /Adobe-Identity-UCS def\n/CMapType 2 def\n\
         1 begincodespacerange\n<00> <FF>\nendcodespacerange\n",
    );
    let list: Vec<(&u8, &char)> = codes.iter().collect();
    for block in list.chunks(100) {
        out.push_str(&format!("{} beginbfchar\n", block.len()));
        for (code, c) in block {
            let mut units = [0u16; 2];
            let hex: String = c
                .encode_utf16(&mut units)
                .iter()
                .map(|u| format!("{u:04X}"))
                .collect();
            out.push_str(&format!("<{code:02X}> <{hex}>\n"));
        }
        out.push_str("endbfchar\n");
    }
    out.push_str("endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n");
    out.into_bytes()
}

/// The `Differences` of the codes whose glyph is not the WinAnsi one there.
fn differences(names: &BTreeMap<u8, String>) -> Option<Object> {
    let mut out: Vec<Object> = Vec::new();
    let mut next = None;
    for (code, name) in names {
        if WIN_ANSI[*code as usize] == name {
            continue;
        }
        if next != Some(*code) {
            out.push(Object::Integer(*code as i64));
        }
        out.push(Object::Name(name.clone().into_bytes()));
        next = code.checked_add(1);
    }
    (!out.is_empty()).then_some(Object::Array(out))
}

impl Document {
    /// Embed the font at `id` when it is not embedded: a subset of its GNU
    /// FreeFont substitute, with the widths the document lays its text out
    /// with. Whether it was embedded.
    pub(crate) fn embed_substitute(&mut self, id: ObjectId) -> Result<bool> {
        let doc = &self.inner;
        let font = doc.get_dictionary(id)?.clone();
        if name_of(&font, b"Subtype") == Some(b"Type0") {
            return self.embed_composite(id, &font);
        }
        let descriptor = font
            .get(b"FontDescriptor")
            .ok()
            .and_then(|d| deref(doc, d).as_dict().ok())
            .cloned();
        if descriptor
            .as_ref()
            .is_some_and(|d| d.has(b"FontFile") || d.has(b"FontFile2") || d.has(b"FontFile3"))
            || !can_substitute(doc, &font)
        {
            return Ok(false);
        }
        let name = bare_name(&font);
        let symbolic = symbolic_table(&font);
        // Every code: its glyph name, character and the width readers use.
        let entries: Vec<(u8, String, Option<char>, Option<f32>)> = match symbolic {
            Some(table) => table
                .iter()
                .map(|(code, glyph, c, w)| (*code, glyph.to_string(), Some(*c), Some(f32::from(*w))))
                .collect(),
            None => {
                let Some(names) = code_names(doc, &font) else {
                    return Ok(false);
                };
                names
                    .into_iter()
                    .enumerate()
                    .filter(|(_, glyph)| glyph != ".notdef")
                    .map(|(code, glyph)| {
                        let c = glyph_char(&glyph);
                        // A standard font without widths has those of its metrics.
                        let w = standard_width(&name, &glyph);
                        (code as u8, glyph, c, w)
                    })
                    .collect()
            }
        };
        let (family, bold, italic) = match symbolic {
            // FreeSerif has the Greek, mathematical and dingbat glyphs.
            Some(_) => (Family::Serif, false, false),
            None => face_of(&name, descriptor.as_ref()),
        };
        let Ok(face) = font_file(&substitute_file(family, bold, italic)) else {
            return Ok(false);
        };
        // The widths the text is laid out with: those of the dictionary.
        let first = font.get(b"FirstChar").ok().and_then(number).unwrap_or(0.0) as i64;
        let given: Vec<Option<f32>> = font
            .get(b"Widths")
            .ok()
            .and_then(|w| deref(doc, w).as_array().ok())
            .map(|a| a.iter().map(|w| number(deref(doc, w))).collect())
            .unwrap_or_default();
        let width_of = |code: usize| -> Option<f32> {
            let i = usize::try_from(code as i64 - first).ok()?;
            given.get(i).copied().flatten()
        };
        let upem = f32::from(face.upem);
        let mut chars: Vec<(char, u16)> = Vec::new();
        let mut advances: BTreeMap<u16, u16> = BTreeMap::new();
        let mut used: BTreeMap<u8, String> = BTreeMap::new();
        let mut shown: BTreeMap<u8, char> = BTreeMap::new();
        let mut widths: BTreeMap<u8, f32> = BTreeMap::new();
        for (code, glyph, c, standard) in &entries {
            let found = c.and_then(|c| face.glyph(c).map(|g| (c, g)));
            let width = width_of(*code as usize)
                .or(*standard)
                .or(found.map(|(_, g)| f32::from(face.advance(g))));
            if let Some(w) = width {
                widths.insert(*code, w);
            }
            let Some((c, g)) = found else { continue };
            // A symbolic font is reached by code through the (3, 0) table.
            let key = match symbolic {
                Some(_) => char::from_u32(0xF000 + u32::from(*code)).unwrap_or(c),
                None => c,
            };
            chars.push((key, g));
            shown.insert(*code, c);
            let w = width.unwrap_or(0.0);
            advances
                .entry(g)
                .or_insert((w * upem / 1000.0).round().clamp(0.0, f32::from(u16::MAX)) as u16);
            used.insert(*code, glyph.clone());
        }
        // A code the document shows that the substitute cannot draw would be
        // a missing glyph (ISO 19005-2, 6.2.11.8): the font stays as it is.
        if chars.is_empty() || used_codes(doc, id).iter().any(|c| !used.contains_key(c)) {
            return Ok(false);
        }
        let glyphs: BTreeSet<u16> = chars.iter().map(|(_, g)| *g).collect();
        let program = subset_with_advances(face.data(), &glyphs, &chars, &advances, symbolic.is_some())
            .map_err(Error::Invalid)?;
        let base = format!("{}+{}", subset_tag(&subset_key(&face, &chars)), face.name);
        let d = &face.descriptor;
        let mut flags = if symbolic.is_some() { 4 } else { 32 };
        match family {
            Family::Mono => flags |= 1,
            Family::Serif => flags |= 2,
            Family::Sans => {}
        }
        if italic {
            flags |= 64;
        }
        let length = program.len() as i64;
        let mut file = Stream::new(dictionary! { "Length1" => length }, program);
        let _ = file.compress();
        let file_id = self.inner.add_object(file);
        let descriptor_id = self.inner.add_object(dictionary! {
            "Type" => "FontDescriptor",
            "FontName" => Object::Name(base.clone().into_bytes()),
            "Flags" => flags,
            "FontBBox" => d.bbox.iter().map(|v| Object::Integer(*v)).collect::<Vec<_>>(),
            "ItalicAngle" => d.italic_angle,
            "Ascent" => d.ascent,
            "Descent" => d.descent,
            "CapHeight" => d.cap_height,
            "StemV" => d.stem_v,
            "FontFile2" => file_id,
        });
        let encoding = match differences(&used) {
            None => Object::Name(b"WinAnsiEncoding".to_vec()),
            Some(diff) => Object::Dictionary(dictionary! {
                "Type" => "Encoding",
                "BaseEncoding" => "WinAnsiEncoding",
                "Differences" => diff,
            }),
        };
        let (Some(&lo), Some(&hi)) = (widths.keys().next(), widths.keys().next_back()) else {
            return Ok(false);
        };
        let list: Vec<Object> = (lo..=hi)
            .map(|c| Object::Real(widths.get(&c).copied().unwrap_or(0.0)))
            .collect();
        let to_unicode = (symbolic.is_some() && !font.has(b"ToUnicode")).then(|| {
            let mut map = Stream::new(Dictionary::new(), byte_to_unicode(&shown));
            let _ = map.compress();
            self.inner.add_object(map)
        });
        let out = self.inner.get_dictionary_mut(id)?;
        out.set("Subtype", "TrueType");
        out.set("BaseFont", Object::Name(base.into_bytes()));
        out.set("FontDescriptor", descriptor_id);
        // A symbolic TrueType font has no encoding (6.2.11.6): a ToUnicode
        // map keeps its text readable when it has none.
        match symbolic {
            Some(_) => {
                out.remove(b"Encoding");
            }
            None => out.set("Encoding", encoding),
        }
        if let Some(map) = to_unicode {
            out.set("ToUnicode", map);
        }
        out.set("FirstChar", i64::from(lo));
        out.set("LastChar", i64::from(hi));
        out.set("Widths", list);
        Ok(true)
    }
}

impl Document {
    /// [`Document::embed_substitute`] for a composite font: a CIDFontType2
    /// subset of the substitute, reached through a `/CIDToGIDMap`, the widths
    /// of `/W` kept.
    fn embed_composite(&mut self, id: ObjectId, font: &Dictionary) -> Result<bool> {
        let doc = &self.inner;
        let Some((descendant, map)) = composite_parts(doc, font) else {
            return Ok(false);
        };
        let descriptor = descendant
            .get(b"FontDescriptor")
            .ok()
            .and_then(|d| deref(doc, d).as_dict().ok())
            .cloned();
        if descriptor
            .as_ref()
            .is_some_and(|d| d.has(b"FontFile") || d.has(b"FontFile2") || d.has(b"FontFile3"))
        {
            return Ok(false);
        }
        let name = bare_name(&descendant);
        let (family, bold, italic) = face_of(&name, descriptor.as_ref());
        let Ok(face) = font_file(&substitute_file(family, bold, italic)) else {
            return Ok(false);
        };
        let cids: BTreeSet<u16> = used_strings(doc, id)
            .iter()
            .flat_map(|s| s.as_chunks::<2>().0.iter().map(|c| u16::from_be_bytes(*c)))
            .collect();
        let upem = f32::from(face.upem);
        let mut chars: Vec<(char, u16)> = Vec::new();
        let mut advances: BTreeMap<u16, u16> = BTreeMap::new();
        let mut gids: BTreeMap<u16, u16> = BTreeMap::new();
        // Every CID the document shows, and those the map names.
        for cid in cids
            .iter()
            .copied()
            .chain(map.keys().copied())
            .collect::<BTreeSet<u16>>()
        {
            let glyph = map.get(&cid).and_then(|c| face.glyph(*c).map(|g| (*c, g)));
            let Some((c, g)) = glyph else {
                if cids.contains(&cid) {
                    // Shown but not drawable: the font stays.
                    return Ok(false);
                }
                continue;
            };
            chars.push((c, g));
            gids.insert(cid, g);
            let w = cid_width(doc, &descendant, cid);
            advances
                .entry(g)
                .or_insert((w * upem / 1000.0).round().clamp(0.0, f32::from(u16::MAX)) as u16);
        }
        if gids.is_empty() {
            return Ok(false);
        }
        let glyphs: BTreeSet<u16> = gids.values().copied().collect();
        let program =
            subset_with_advances(face.data(), &glyphs, &chars, &advances, false).map_err(Error::Invalid)?;
        let base = format!("{}+{}", subset_tag(&subset_key(&face, &chars)), face.name);
        let max = gids.keys().next_back().copied().unwrap_or(0);
        let mut to_gid = Vec::with_capacity(2 * (usize::from(max) + 1));
        for cid in 0..=max {
            to_gid.extend(gids.get(&cid).copied().unwrap_or(0).to_be_bytes());
        }
        let d = &face.descriptor;
        let mut flags = 4;
        match family {
            Family::Mono => flags |= 1,
            Family::Serif => flags |= 2,
            Family::Sans => {}
        }
        if italic {
            flags |= 64;
        }
        let length = program.len() as i64;
        let mut file = Stream::new(dictionary! { "Length1" => length }, program);
        let _ = file.compress();
        let file_id = self.inner.add_object(file);
        let mut gid_map = Stream::new(Dictionary::new(), to_gid);
        let _ = gid_map.compress();
        let gid_map_id = self.inner.add_object(gid_map);
        let descriptor_id = self.inner.add_object(dictionary! {
            "Type" => "FontDescriptor",
            "FontName" => Object::Name(base.clone().into_bytes()),
            "Flags" => flags,
            "FontBBox" => d.bbox.iter().map(|v| Object::Integer(*v)).collect::<Vec<_>>(),
            "ItalicAngle" => d.italic_angle,
            "Ascent" => d.ascent,
            "Descent" => d.descent,
            "CapHeight" => d.cap_height,
            "StemV" => d.stem_v,
            "FontFile2" => file_id,
        });
        let update = |cid_font: &mut Dictionary| {
            cid_font.set("Subtype", "CIDFontType2");
            cid_font.set("BaseFont", Object::Name(base.clone().into_bytes()));
            cid_font.set("FontDescriptor", descriptor_id);
            cid_font.set("CIDToGIDMap", gid_map_id);
        };
        let descendant_ref = font.get(b"DescendantFonts").ok().and_then(|d| match d {
            Object::Array(a) => a.first().cloned(),
            Object::Reference(r) => self
                .inner
                .get_object(*r)
                .ok()
                .and_then(|o| o.as_array().ok())
                .and_then(|a| a.first().cloned()),
            _ => None,
        });
        match descendant_ref {
            Some(Object::Reference(r)) => update(self.inner.get_dictionary_mut(r)?),
            _ => {
                let mut cid_font = descendant.clone();
                update(&mut cid_font);
                let r = self.inner.add_object(cid_font);
                self.inner
                    .get_dictionary_mut(id)?
                    .set("DescendantFonts", vec![Object::Reference(r)]);
            }
        }
        self.inner
            .get_dictionary_mut(id)?
            .set("BaseFont", Object::Name(base.into_bytes()));
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_substitute_is_a_recorded_file() {
        use sha2::{Digest, Sha256};
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fonts");
        let readme = std::fs::read_to_string(dir.join("README.md")).unwrap();
        for family in [Family::Sans, Family::Serif, Family::Mono] {
            for (bold, italic) in [(false, false), (true, false), (false, true), (true, true)] {
                let file = substitute_file(family, bold, italic);
                let data = std::fs::read(dir.join(&file)).unwrap_or_else(|e| panic!("{file}: {e}"));
                let digest: String = Sha256::digest(&data).iter().map(|b| format!("{b:02x}")).collect();
                assert!(readme.contains(&format!("| `{file}` |")), "{file} is listed");
                assert!(readme.contains(&digest), "{file}: {digest} is recorded");
            }
        }
    }

    #[test]
    fn glyph_names_read_as_characters() {
        assert_eq!(glyph_char("A"), Some('A'));
        assert_eq!(glyph_char("quoteright"), Some('\u{2019}'));
        assert_eq!(glyph_char("Euro"), Some('€'));
        assert_eq!(glyph_char("uni0141"), Some('Ł'));
        assert_eq!(glyph_char("u1F600"), Some('😀'));
        assert_eq!(glyph_char("nonsense"), None);
    }

    #[test]
    fn families_and_styles_are_told_from_names() {
        assert_eq!(face_of("Helvetica-BoldOblique", None), (Family::Sans, true, true));
        assert_eq!(face_of("Times-Roman", None), (Family::Serif, false, false));
        assert_eq!(
            face_of("TimesNewRomanPS-ItalicMT", None),
            (Family::Serif, false, true)
        );
        assert_eq!(face_of("CourierNewPS-BoldMT", None), (Family::Mono, true, false));
        assert_eq!(face_of("Arial,Bold", None), (Family::Sans, true, false));
        assert_eq!(face_of("LiberationSans", None), (Family::Sans, false, false));
        assert_eq!(
            substitute_file(Family::Serif, true, true),
            "FreeSerifBoldItalic.ttf"
        );
        assert_eq!(substitute_file(Family::Sans, false, true), "FreeSansOblique.ttf");
        assert_eq!(substitute_file(Family::Mono, false, false), "FreeMono.ttf");
    }

    #[test]
    fn only_codes_that_differ_from_winansi_are_listed() {
        let names: BTreeMap<u8, String> = [
            (0x41u8, "A"),
            (0x27, "quoteright"),
            (0x28, "parenleft"),
            (0x60, "quoteleft"),
        ]
        .into_iter()
        .map(|(c, n)| (c, n.to_string()))
        .collect();
        let Some(Object::Array(a)) = differences(&names) else {
            panic!("differences expected");
        };
        assert_eq!(
            a,
            [
                Object::Integer(0x27),
                Object::Name(b"quoteright".to_vec()),
                Object::Integer(0x60),
                Object::Name(b"quoteleft".to_vec()),
            ]
        );
    }
}
