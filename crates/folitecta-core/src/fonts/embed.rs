//! The objects of an embedded font subset: a Type 0 font with the Identity-H
//! encoding over a CIDFontType2 font, its descriptor, the font program, the
//! map from codes to glyphs and the map from codes to characters (ISO
//! 32000-1, 9.7.4, 9.7.6, 9.8, 9.9 and 9.10.3).

use std::collections::BTreeSet;

use lopdf::{Dictionary, Object, ObjectId, Stream, StringFormat, dictionary};
use sha2::{Digest, Sha256};

use super::GlyphProblem;
use super::source::SourceFont;
use crate::document::Document;

/// Private key of the Type 0 fonts Folitecta embeds: the hexadecimal SHA-256 of
/// the font file, its name and the characters with their glyphs, so a drawing
/// made again with the same characters can use the same font objects.
pub(crate) const FONT_KEY: &str = "FolitectaFont";

/// What identifies a subset: the font file, its name and the characters
/// (codes 1, 2, ... in this order) with their glyphs.
pub(crate) fn subset_key(font: &SourceFont, chars: &[(char, u16)]) -> String {
    let mut h = Sha256::new();
    h.update(font.digest);
    h.update(font.name.as_bytes());
    h.update([0]);
    for (c, g) in chars {
        h.update((*c as u32).to_be_bytes());
        h.update(g.to_be_bytes());
    }
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

/// The six capital letters before the name of a subset (ISO 32000-1, 9.6.4),
/// taken from its key: the same for the same subset, different otherwise.
pub(crate) fn subset_tag(key: &str) -> String {
    key.as_bytes()
        .chunks(2)
        .take(6)
        .map(|pair| {
            let byte = u8::from_str_radix(std::str::from_utf8(pair).unwrap_or("0"), 16).unwrap_or(0);
            (b'A' + byte % 26) as char
        })
        .collect()
}

/// The ToUnicode CMap of codes 1..=n (two bytes each) for the characters
/// `chars` (ISO 32000-1, 9.10.3); at most 100 mappings per block.
pub(crate) fn to_unicode_cmap(chars: &[char]) -> Vec<u8> {
    let mut out = String::from(
        "/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n\
         /CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n\
         /CMapName /Adobe-Identity-UCS def\n/CMapType 2 def\n\
         1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n",
    );
    let numbered: Vec<(usize, char)> = chars.iter().enumerate().map(|(i, c)| (i + 1, *c)).collect();
    for block in numbered.chunks(100) {
        out.push_str(&format!("{} beginbfchar\n", block.len()));
        for (code, c) in block {
            let mut units = [0u16; 2];
            let utf16: String = c
                .encode_utf16(&mut units)
                .iter()
                .map(|u| format!("{u:04X}"))
                .collect();
            out.push_str(&format!("<{code:04X}> <{utf16}>\n"));
        }
        out.push_str("endbfchar\n");
    }
    out.push_str("endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n");
    out.into_bytes()
}

fn compressed(dict: Dictionary, content: Vec<u8>) -> Stream {
    let mut stream = Stream::new(dict, content);
    // Without compression the stream is still correct, only larger.
    let _ = stream.compress();
    stream
}

/// The parts of a subset, made before anything is added to a document.
pub(crate) struct SubsetParts {
    key: String,
    base: String,
    /// The font program (`/FontFile2`).
    program: Vec<u8>,
    /// `/CIDToGIDMap`: two bytes per code, from code 0.
    map: Vec<u8>,
    to_unicode: Vec<u8>,
    widths: Vec<i64>,
}

/// The subset of `font` drawing `chars` (codes 1, 2, ... in this order, with
/// their glyphs).
pub(crate) fn build_subset(
    font: &SourceFont,
    chars: &[(char, u16)],
) -> std::result::Result<SubsetParts, GlyphProblem> {
    let key = subset_key(font, chars);
    let glyphs: BTreeSet<u16> = chars.iter().map(|(_, g)| *g).collect();
    let program = super::subset::subset(font.data(), &glyphs, chars).map_err(|reason| {
        GlyphProblem::FontUnreadable {
            path: font.path.clone(),
            reason,
        }
    })?;
    let mut map = vec![0u8, 0];
    for (_, g) in chars {
        map.extend(g.to_be_bytes());
    }
    let letters: Vec<char> = chars.iter().map(|(c, _)| *c).collect();
    Ok(SubsetParts {
        base: format!("{}+{}", subset_tag(&key), font.name),
        key,
        program,
        map,
        to_unicode: to_unicode_cmap(&letters),
        widths: chars.iter().map(|(_, g)| font.advance(*g) as i64).collect(),
    })
}

fn decoded(doc: &lopdf::Document, obj: &Object) -> Option<Vec<u8>> {
    let stream = doc.get_object(obj.as_reference().ok()?).ok()?.as_stream().ok()?;
    Some(
        stream
            .decompressed_content()
            .unwrap_or_else(|_| stream.content.clone()),
    )
}

impl Document {
    /// Add the objects of a subset of `font` (made by [`build_subset`]):
    /// the Type 0 font, its CIDFont, descriptor, program, glyph map and
    /// ToUnicode map.
    pub(crate) fn add_unicode_font(&mut self, font: &SourceFont, parts: SubsetParts) -> ObjectId {
        let SubsetParts {
            key,
            base,
            program,
            map,
            to_unicode,
            widths,
        } = parts;
        let program_len = program.len() as i64;
        let file = compressed(dictionary! { "Length1" => program_len }, program);
        let map = compressed(Dictionary::new(), map);
        let to_unicode = compressed(Dictionary::new(), to_unicode);
        let widths: Vec<Object> = widths.into_iter().map(Object::Integer).collect();
        let d = &font.descriptor;

        let file_id = self.inner.add_object(file);
        let map_id = self.inner.add_object(map);
        let to_unicode_id = self.inner.add_object(to_unicode);
        let descriptor_id = self.inner.add_object(dictionary! {
            "Type" => "FontDescriptor",
            "FontName" => Object::Name(base.clone().into_bytes()),
            "Flags" => d.flags,
            "FontBBox" => d.bbox.iter().map(|v| Object::Integer(*v)).collect::<Vec<_>>(),
            "ItalicAngle" => d.italic_angle,
            "Ascent" => d.ascent,
            "Descent" => d.descent,
            "CapHeight" => d.cap_height,
            "StemV" => d.stem_v,
            "FontFile2" => file_id,
        });
        let cid_font = self.inner.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "CIDFontType2",
            "BaseFont" => Object::Name(base.clone().into_bytes()),
            "CIDSystemInfo" => dictionary! {
                "Registry" => Object::string_literal("Adobe"),
                "Ordering" => Object::string_literal("Identity"),
                "Supplement" => 0,
            },
            "FontDescriptor" => descriptor_id,
            "DW" => 1000,
            "W" => vec![Object::Integer(1), Object::Array(widths)],
            "CIDToGIDMap" => map_id,
        });
        self.inner.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "Type0",
            "BaseFont" => Object::Name(base.into_bytes()),
            "Encoding" => "Identity-H",
            "DescendantFonts" => vec![Object::Reference(cid_font)],
            "ToUnicode" => to_unicode_id,
            FONT_KEY => Object::String(key.into_bytes(), StringFormat::Literal),
        })
    }

    /// Whether `id` is the subset `parts` as this module writes it: the key
    /// alone is not trusted (a file can change the objects behind it), the
    /// glyph map, the ToUnicode map, the widths and the font program are
    /// compared with the ones just made.
    pub(crate) fn same_subset(&self, id: ObjectId, parts: &SubsetParts) -> bool {
        let doc = &self.inner;
        let check = || -> Option<bool> {
            let font = doc.get_dictionary(id).ok()?;
            let name =
                |d: &Dictionary, k: &[u8]| d.get(k).ok().and_then(|n| n.as_name().ok()).map(<[u8]>::to_vec);
            let same_key = matches!(font.get(FONT_KEY.as_bytes()), Ok(Object::String(k, _)) if *k == parts.key.as_bytes());
            let type0 = name(font, b"Subtype")? == b"Type0"
                && name(font, b"Encoding")? == b"Identity-H"
                && name(font, b"BaseFont")? == parts.base.as_bytes();
            let cid_id = font
                .get(b"DescendantFonts")
                .ok()?
                .as_array()
                .ok()?
                .first()?
                .as_reference()
                .ok()?;
            let cid = doc.get_dictionary(cid_id).ok()?;
            let widths: Vec<Object> = parts.widths.iter().map(|w| Object::Integer(*w)).collect();
            let same_widths =
                cid.get(b"W").ok()?.as_array().ok()? == &vec![Object::Integer(1), Object::Array(widths)];
            let descriptor = doc
                .get_dictionary(cid.get(b"FontDescriptor").ok()?.as_reference().ok()?)
                .ok()?;
            let same_program = decoded(doc, descriptor.get(b"FontFile2").ok()?)? == parts.program;
            let same_map = decoded(doc, cid.get(b"CIDToGIDMap").ok()?)? == parts.map;
            let same_to_unicode = decoded(doc, font.get(b"ToUnicode").ok()?)? == parts.to_unicode;
            Some(
                same_key
                    && type0
                    && name(cid, b"Subtype")? == b"CIDFontType2"
                    && same_widths
                    && same_program
                    && same_map
                    && same_to_unicode,
            )
        };
        check().unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fonts::source::tests::bundled;

    #[test]
    fn to_unicode_maps_codes_in_blocks_of_100() {
        let chars: Vec<char> = (0x0400u32..0x0400 + 250).filter_map(char::from_u32).collect();
        assert_eq!(chars.len(), 250);
        let cmap = String::from_utf8(to_unicode_cmap(&chars)).unwrap();
        assert_eq!(cmap.matches("100 beginbfchar").count(), 2);
        assert_eq!(cmap.matches("50 beginbfchar").count(), 1);
        let map = crate::textedit::to_unicode(cmap.as_bytes());
        assert_eq!(map.len(), 250);
        for (k, (code, text)) in map.iter().enumerate() {
            assert_eq!(code, &(k as u16 + 1).to_be_bytes().to_vec());
            assert_eq!(text, &chars[k].to_string());
        }
    }

    #[test]
    fn supplementary_characters_become_surrogate_pairs() {
        let cmap = String::from_utf8(to_unicode_cmap(&['A', '𝐀'])).unwrap();
        assert!(cmap.contains("<0001> <0041>"), "{cmap}");
        assert!(cmap.contains("<0002> <D835DC00>"), "{cmap}");
    }

    #[test]
    fn subset_tags_are_six_capitals_and_stable() {
        let font = bundled();
        let chars =
            |s: &str| -> Vec<(char, u16)> { s.chars().map(|c| (c, font.glyph(c).unwrap())).collect() };
        let a = subset_key(&font, &chars("Łódź"));
        let tag = subset_tag(&a);
        assert_eq!(tag.len(), 6);
        assert!(tag.chars().all(|c| c.is_ascii_uppercase()));
        assert_eq!(subset_key(&font, &chars("Łódź")), a);
        assert_ne!(subset_key(&font, &chars("Łódż")), a);
        assert_ne!(
            subset_key(&font, &chars("ódźŁ")),
            a,
            "the order of the codes counts"
        );
    }
}
