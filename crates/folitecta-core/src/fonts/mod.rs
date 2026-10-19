//! Fonts of generated text.
//!
//! Text whose characters all have a WinAnsi code is drawn with the standard
//! Helvetica font, as it always was. Any other text is drawn with a subset of
//! GNU FreeFont FreeSans embedded in the appearance that shows it: a Type 0
//! font with the Identity-H encoding over a CIDFontType2 font, whose codes are
//! the characters in the order they are first drawn, a `/CIDToGIDMap` from
//! codes to the glyphs of FreeSans and a `/ToUnicode` map from codes back to
//! the characters (ISO 32000-1, 9.7 and 9.10.3), so the text can be copied and
//! searched. Each appearance owns its subset: removing the comment removes its
//! glyphs, and copies carry them.
//!
//! [`TextWriter`] is the one way generated text chooses its font, measures,
//! wraps and writes its lines; text needing right-to-left order, joining or
//! combining marks, and characters FreeSans has no glyph for, are refused with
//! a [`GlyphProblem`] before anything is changed (never drawn as `?`).
//!
//! The font is a data file (`fonts/FreeSans.ttf`) looked for in the folders of
//! [`set_font_dirs`]; it is never built into the program.

use std::fmt;
use std::path::PathBuf;

use lopdf::Dictionary;

use crate::document::Document;
use crate::error::Result;
use crate::util::deref;

mod embed;
mod source;
mod subset;
mod writer;

pub use source::{UNICODE_FONT_FILE, font_dirs, preload, set_font_dirs};
pub(crate) use writer::TextWriter;

/// Why generated text cannot be drawn. Every operation that meets one changes
/// nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GlyphProblem {
    /// The font for text beyond WinAnsi is in none of the font folders.
    FontMissing { file: String, searched: Vec<PathBuf> },
    /// The font file cannot be read or is not a TrueType font.
    FontUnreadable { path: PathBuf, reason: String },
    /// The licence of the font (OS/2 `fsType`) forbids embedding or subsetting it.
    NotEmbeddable { font: String },
    /// Characters that need right-to-left order, joining or marks combined
    /// with their letter, which generated text cannot lay out yet.
    NeedsLayout(Vec<char>),
    /// Characters the font has no glyph for.
    Missing { font: String, chars: Vec<char> },
    /// Characters beyond WinAnsi in the text of a text stamp, which is
    /// drawn with the standard Helvetica only for now.
    StampText(Vec<char>),
}

impl fmt::Display for GlyphProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GlyphProblem::FontMissing { file, searched } => {
                let folders: Vec<String> = searched.iter().map(|p| p.display().to_string()).collect();
                write!(
                    f,
                    "the font for text beyond the Western European characters ({file}) was not found in the fonts folders ({})",
                    folders.join("; ")
                )
            }
            GlyphProblem::FontUnreadable { path, reason } => {
                write!(f, "the font file {} cannot be used: {reason}", path.display())
            }
            GlyphProblem::NotEmbeddable { font } => {
                write!(
                    f,
                    "the licence of the font {font} does not allow embedding it in documents"
                )
            }
            GlyphProblem::NeedsLayout(chars) => write!(
                f,
                "{}: right-to-left text and letters that join or combine cannot be drawn in generated text yet",
                describe_chars(chars)
            ),
            GlyphProblem::Missing { font, chars } => {
                write!(f, "the font {font} has no glyphs for {}", describe_chars(chars))
            }
            GlyphProblem::StampText(chars) => write!(
                f,
                "{}: text stamps can draw only Western European characters for now",
                describe_chars(chars)
            ),
        }
    }
}

impl std::error::Error for GlyphProblem {}

/// Whether generated text can draw `text`; the problem otherwise. Operations
/// call it before changing anything.
pub fn check_text(text: &str) -> std::result::Result<(), GlyphProblem> {
    TextWriter::new(text).map(|_| ())
}

/// Width in points of one line of `text` drawn by generated text at `size`
/// points (Helvetica when the Unicode font cannot be used).
pub fn text_width(text: &str, size: f32) -> f32 {
    match TextWriter::new(text) {
        Ok(w) => w.width(w.text(), size),
        Err(_) => crate::util::helvetica_width(text, size),
    }
}

/// Whether a character is not drawn at all (format characters, selectors).
fn invisible(c: char) -> bool {
    c.is_control()
        || matches!(
            c,
            '\u{00AD}'
                | '\u{034F}'
                | '\u{061C}'
                | '\u{115F}'..='\u{1160}'
                | '\u{17B4}'..='\u{17B5}'
                | '\u{180B}'..='\u{180F}'
                | '\u{200B}'..='\u{200F}'
                | '\u{202A}'..='\u{202E}'
                | '\u{2060}'..='\u{206F}'
                | '\u{3164}'
                | '\u{FE00}'..='\u{FE0F}'
                | '\u{FEFF}'
                | '\u{FFA0}'
                | '\u{1BCA0}'..='\u{1BCA3}'
                | '\u{1D173}'..='\u{1D17A}'
                | '\u{E0000}'..='\u{E0FFF}'
        )
}

/// The characters of a problem, readable in any language: each one with its
/// code point (`ł (U+0142)`), combining marks on a dotted circle, invisible
/// ones by code point only; at most eight, then how many more.
pub fn describe_chars(chars: &[char]) -> String {
    const SHOWN: usize = 8;
    let mut parts: Vec<String> = chars
        .iter()
        .take(SHOWN)
        .map(|&c| {
            let code = format!("U+{:04X}", c as u32);
            if invisible(c) || (c.is_whitespace() && c != ' ') {
                code
            } else if unicode_normalization::char::is_combining_mark(c) {
                format!("\u{25CC}{c} ({code})")
            } else {
                format!("{c} ({code})")
            }
        })
        .collect();
    if chars.len() > SHOWN {
        parts.push(format!("… (+{})", chars.len() - SHOWN));
    }
    parts.join(", ")
}

/// A font used by the normal appearance of a comment.
#[derive(Debug, Clone, PartialEq)]
pub struct AppearanceFont {
    /// Name of the font in the resources of the appearance (`Helv`, `FoliUni`).
    pub resource: String,
    /// `/BaseFont` (with the subset tag of an embedded subset).
    pub base_font: String,
    /// `/Subtype` (`Type1`, `Type0`, `TrueType`, ...).
    pub subtype: String,
    /// The font program is embedded in the document.
    pub embedded: bool,
    /// The font maps its codes back to characters (`/ToUnicode`).
    pub to_unicode: bool,
}

fn name_value(d: &Dictionary, key: &[u8]) -> String {
    d.get(key)
        .ok()
        .and_then(|n| n.as_name().ok())
        .map(|n| String::from_utf8_lossy(n).to_string())
        .unwrap_or_default()
}

fn has_font_file(doc: &lopdf::Document, font: &Dictionary) -> bool {
    font.get(b"FontDescriptor")
        .ok()
        .map(|d| deref(doc, d))
        .and_then(|d| d.as_dict().ok())
        .is_some_and(|d| d.has(b"FontFile") || d.has(b"FontFile2") || d.has(b"FontFile3"))
}

/// The font dictionary `name` of the resources of the normal appearance of `annot`.
pub(crate) fn appearance_font<'a>(
    doc: &'a lopdf::Document,
    annot: &'a Dictionary,
    name: &[u8],
) -> Option<(Option<lopdf::ObjectId>, &'a Dictionary)> {
    let fonts = appearance_fonts_dict(doc, annot)?;
    let obj = fonts.get(name).ok()?;
    let id = obj.as_reference().ok();
    deref(doc, obj).as_dict().ok().map(|d| (id, d))
}

fn appearance_fonts_dict<'a>(doc: &'a lopdf::Document, annot: &'a Dictionary) -> Option<&'a Dictionary> {
    let ap = annot.get(b"AP").ok().map(|a| deref(doc, a))?.as_dict().ok()?;
    let normal = deref(doc, ap.get(b"N").ok()?);
    let stream = normal.as_stream().ok()?;
    let resources = deref(doc, stream.dict.get(b"Resources").ok()?).as_dict().ok()?;
    deref(doc, resources.get(b"Font").ok()?).as_dict().ok()
}

impl Document {
    /// The fonts of the normal appearance of comment `index` of `page` (none
    /// when it has no appearance or draws no text).
    pub fn annotation_fonts(&self, page: usize, index: usize) -> Result<Vec<AppearanceFont>> {
        let id = self.annotation_object(page, index)?;
        let doc = &self.inner;
        let annot = doc.get_dictionary(id)?;
        let Some(fonts) = appearance_fonts_dict(doc, annot) else {
            return Ok(Vec::new());
        };
        let mut out = Vec::new();
        for (name, value) in fonts.iter() {
            let Ok(font) = deref(doc, value).as_dict() else {
                continue;
            };
            let subtype = name_value(font, b"Subtype");
            let embedded = if subtype == "Type0" {
                font.get(b"DescendantFonts")
                    .ok()
                    .map(|d| deref(doc, d))
                    .and_then(|d| d.as_array().ok())
                    .and_then(|a| a.first())
                    .map(|d| deref(doc, d))
                    .and_then(|d| d.as_dict().ok())
                    .is_some_and(|d| has_font_file(doc, d))
            } else {
                has_font_file(doc, font)
            };
            out.push(AppearanceFont {
                resource: String::from_utf8_lossy(name).to_string(),
                base_font: name_value(font, b"BaseFont"),
                subtype,
                embedded,
                to_unicode: font.has(b"ToUnicode"),
            });
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn describe_chars_names_code_points() {
        assert_eq!(describe_chars(&['ł']), "ł (U+0142)");
        let mark = describe_chars(&['\u{0301}']);
        assert!(mark.contains("U+0301") && mark.contains('\u{25CC}'), "{mark}");
        assert_eq!(describe_chars(&['\u{200D}']), "U+200D");
        assert_eq!(describe_chars(&['𝐀']), "𝐀 (U+1D400)");
        let many: Vec<char> = "abcdefghij".chars().collect();
        let text = describe_chars(&many);
        assert!(text.ends_with("… (+2)"), "{text}");
        assert_eq!(text.matches("(U+").count(), 8);
    }

    #[test]
    fn problems_name_the_characters() {
        let missing = GlyphProblem::Missing {
            font: "FreeSans".into(),
            chars: vec!['漢'],
        };
        assert!(missing.to_string().contains("U+6F22"), "{missing}");
        let layout = GlyphProblem::NeedsLayout(vec!['ש']);
        assert!(layout.to_string().contains("U+05E9"), "{layout}");
    }
}
