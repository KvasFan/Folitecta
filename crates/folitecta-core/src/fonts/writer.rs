//! How one piece of generated text is drawn: which font, how wide, how it
//! wraps and what its lines become in a content stream.

use std::collections::HashMap;
use std::sync::Arc;

use lopdf::ObjectId;
use unicode_bidi::{BidiClass, BidiInfo};
use unicode_normalization::UnicodeNormalization;

use super::GlyphProblem;
use super::embed::build_subset;
use super::source::{SourceFont, unicode_font};
use crate::document::Document;
use crate::error::Result;
use crate::textdoc::{wrap_text, wrap_text_by};
use crate::util::{helvetica_width, pdf_literal, winansi_code};

/// Name of the Unicode font in the resources of a drawing.
pub(crate) const UNICODE_RESOURCE: &str = "FoliUni";

/// A standard font of the reader, with WinAnsi codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StdFont {
    Helvetica,
    Times,
    Courier,
}

impl StdFont {
    pub(crate) fn of(family: &str) -> StdFont {
        match family {
            "Times" => StdFont::Times,
            "Courier" => StdFont::Courier,
            _ => StdFont::Helvetica,
        }
    }

    fn resource(self) -> &'static str {
        match self {
            StdFont::Helvetica => "Helv",
            StdFont::Times => "TiRo",
            StdFont::Courier => "Cour",
        }
    }

    pub(crate) fn base_font(self) -> &'static str {
        match self {
            StdFont::Helvetica => "Helvetica",
            StdFont::Times => "Times-Roman",
            StdFont::Courier => "Courier",
        }
    }

    /// The width of a WinAnsi line at `size` points.
    fn width(self, line: &str, size: f32) -> f32 {
        match self {
            StdFont::Helvetica => helvetica_width(line, size),
            StdFont::Courier => line.chars().count() as f32 * 600.0 * size / 1000.0,
            StdFont::Times => {
                let table = super::standard_metrics::TIMES_ROMAN;
                let units: f32 = line
                    .chars()
                    .map(|c| {
                        crate::util::winansi_code(c)
                            .map(|code| super::encodings::WIN_ANSI[code as usize])
                            .and_then(|name| table.binary_search_by(|(n, _)| n.cmp(&name)).ok())
                            .map(|i| f32::from(table[i].1))
                            .unwrap_or(500.0)
                    })
                    .sum();
                units * size / 1000.0
            }
        }
    }
}

enum Face {
    /// A standard font with WinAnsi codes (Helvetica exactly as before).
    Standard(StdFont),
    /// A subset of the Unicode font; codes are given in order of first use.
    Unicode {
        font: Arc<SourceFont>,
        chars: Vec<(char, u16)>,
        codes: HashMap<char, u16>,
    },
}

/// The font, measures and codes of one piece of generated text.
pub(crate) struct TextWriter {
    text: String,
    face: Face,
    /// The text was prepared (line breaks, invisible characters, NFC): the
    /// pieces drawn with this writer are prepared the same way.
    prepared: bool,
    /// The name of the Unicode font in the resources of the drawing.
    unicode_resource: String,
}

/// Format characters that change the order or the joining of letters.
fn layout_control(c: char) -> bool {
    matches!(
        c,
        '\u{034F}'
            | '\u{061C}'
            | '\u{180B}'..='\u{180F}'
            | '\u{200C}'..='\u{200F}'
            | '\u{202A}'..='\u{202E}'
            | '\u{2066}'..='\u{2069}'
    )
}

/// Hebrew letters and punctuation: drawn right to left, with no joining and
/// no shaping. Their points are combining marks, which stay refused.
fn hebrew(c: char) -> bool {
    matches!(c, '\u{05D0}'..='\u{05EA}' | '\u{05EF}'..='\u{05F4}' | '\u{05BE}' | '\u{05C0}' | '\u{05C3}' | '\u{05C6}')
}

/// The opposite of a bracket, drawn in a right-to-left run.
pub(crate) fn mirrored(c: char) -> char {
    match c {
        '(' => ')',
        ')' => '(',
        '[' => ']',
        ']' => '[',
        '{' => '}',
        '}' => '{',
        '<' => '>',
        '>' => '<',
        '«' => '»',
        '»' => '«',
        other => other,
    }
}

/// `line` in the order its glyphs are drawn from left to right.
pub(crate) fn visual_order(line: &str) -> String {
    if !line.chars().any(hebrew) {
        return line.to_string();
    }
    let info = BidiInfo::new(line, None);
    let Some(paragraph) = info.paragraphs.first() else {
        return line.to_string();
    };
    let (levels, runs) = info.visual_runs(paragraph, paragraph.range.clone());
    let mut out = String::with_capacity(line.len());
    for run in runs {
        let text = &line[run.clone()];
        if levels[run.start].is_rtl() {
            out.extend(text.chars().rev().map(mirrored));
        } else {
            out.push_str(text);
        }
    }
    out
}

/// Whether `line` reads from right to left.
pub(crate) fn right_to_left(line: &str) -> bool {
    line.chars()
        .find(|c| {
            matches!(
                unicode_bidi::bidi_class(*c),
                BidiClass::L | BidiClass::R | BidiClass::AL
            )
        })
        .is_some_and(hebrew)
}

/// Characters that need a layout generated text cannot make yet: right-to-left
/// order beyond Hebrew, joining, marks combined with their letter, vertical
/// scripts.
fn needs_layout(c: char) -> bool {
    use BidiClass::*;
    if hebrew(c) {
        return false;
    }
    matches!(
        unicode_bidi::bidi_class(c),
        R | AL | AN | RLE | LRE | RLO | LRO | PDF | RLI | LRI | FSI | PDI
    ) || unicode_normalization::char::is_combining_mark(c)
        || layout_control(c)
        || matches!(c, '\u{1800}'..='\u{18AF}' | '\u{A840}'..='\u{A87F}')
}

/// The text to draw: line breaks as `\n`, invisible characters left out (they
/// draw nothing), then composed (NFC), so `e` with a combining acute accent is
/// drawn as `é`.
fn prepare(text: &str) -> String {
    let lines = text
        .replace("\r\n", "\n")
        .replace(['\r', '\u{0B}', '\u{0C}', '\u{85}', '\u{2028}', '\u{2029}'], "\n");
    let kept: String = lines
        .chars()
        .filter(|&c| matches!(c, '\n' | '\t') || layout_control(c) || !super::invisible(c))
        .collect();
    kept.nfc().collect()
}

fn distinct(chars: impl Iterator<Item = char>) -> Vec<char> {
    let mut out: Vec<char> = Vec::new();
    for c in chars {
        if !out.contains(&c) {
            out.push(c);
        }
    }
    out
}

impl TextWriter {
    /// Choose how `text` is drawn, or say why it cannot be. Nothing is added
    /// to any document here.
    pub(crate) fn new(text: &str) -> std::result::Result<TextWriter, GlyphProblem> {
        TextWriter::with_family(text, "")
    }

    /// [`TextWriter::new`] with a standard family for WinAnsi text ("" for
    /// Helvetica, "Times", "Courier"); other text uses the Unicode font.
    pub(crate) fn with_family(text: &str, family: &str) -> std::result::Result<TextWriter, GlyphProblem> {
        let family = StdFont::of(family);
        if text.chars().all(|c| winansi_code(c).is_some()) {
            return Ok(TextWriter {
                text: text.to_string(),
                face: Face::Standard(family),
                prepared: false,
                unicode_resource: UNICODE_RESOURCE.to_string(),
            });
        }
        let text = prepare(text);
        if text.chars().all(|c| winansi_code(c).is_some()) {
            return Ok(TextWriter {
                text,
                face: Face::Standard(family),
                prepared: true,
                unicode_resource: UNICODE_RESOURCE.to_string(),
            });
        }
        let layout = distinct(text.chars().filter(|c| needs_layout(*c)));
        if !layout.is_empty() {
            return Err(GlyphProblem::NeedsLayout(layout));
        }
        let font = unicode_font()?;
        let mut missing = distinct(
            text.chars()
                .filter(|c| !matches!(c, '\n' | '\t') && font.glyph(*c).is_none()),
        );
        // Tabs are drawn as spaces.
        if text.contains('\t') && font.glyph(' ').is_none() && !missing.contains(&' ') {
            missing.push(' ');
        }
        if !missing.is_empty() {
            return Err(GlyphProblem::Missing {
                font: font.name.clone(),
                chars: missing,
            });
        }
        Ok(TextWriter {
            text,
            face: Face::Unicode {
                font,
                chars: Vec::new(),
                codes: HashMap::new(),
            },
            prepared: true,
            unicode_resource: UNICODE_RESOURCE.to_string(),
        })
    }

    /// A writer for a drawing of several pieces of text (the cells of a
    /// table, the lines of a tape, the texts of a stamp): the font is chosen
    /// for all of them together, and each piece is then drawn from
    /// [`TextWriter::piece`].
    pub(crate) fn for_pieces<'a>(
        pieces: impl IntoIterator<Item = &'a str>,
    ) -> std::result::Result<TextWriter, GlyphProblem> {
        TextWriter::new(&pieces.into_iter().collect::<Vec<_>>().join("\n"))
    }

    /// One piece of the text, as this writer draws it: as given when the
    /// whole text is drawn as given, else prepared like the whole.
    pub(crate) fn piece(&self, text: &str) -> String {
        if self.prepared {
            prepare(text)
        } else {
            text.to_string()
        }
    }

    /// The lines of a piece of the text, no wider than `max_width` at `size`
    /// points, measured with this writer's font.
    pub(crate) fn wrap_piece(&self, text: &str, size: f32, max_width: f32) -> Vec<String> {
        match self.face {
            Face::Standard(StdFont::Helvetica) => wrap_text(text, size, max_width),
            Face::Standard(f) => wrap_text_by(text, max_width.max(size), &|s| f.width(s, size)),
            Face::Unicode { .. } => wrap_text_by(text, max_width.max(size), &|s| self.width(s, size)),
        }
    }

    /// A text object showing `line`: `BT`, the font in `size` points, the
    /// fill colour when given, `position` (the `Td` or `Tm` that places the
    /// text), the string and `ET`. With the standard face the font is
    /// `standard.0`, or `standard.1` for bold, exactly as drawings wrote it
    /// before. The Unicode font has no bold: its bold letters are filled and
    /// stroked (a line of 4 % of the size, in the fill colour when given,
    /// else in the stroke colour of the drawing), inside `q`/`Q` so that what
    /// follows is drawn as before.
    pub(crate) fn text_object(
        &mut self,
        line: &str,
        size: f32,
        color: Option<[f32; 3]>,
        position: &str,
        bold: bool,
        standard: (&str, &str),
    ) -> std::result::Result<String, GlyphProblem> {
        let f = crate::util::fmt_num;
        let shown = self.show(line)?;
        let fill = color
            .map(|c| format!("{} {} {} rg ", f(c[0]), f(c[1]), f(c[2])))
            .unwrap_or_default();
        let name = &self.unicode_resource;
        Ok(match self.face {
            Face::Standard(_) => {
                let name = if bold { standard.1 } else { standard.0 };
                format!("BT /{name} {} Tf {fill}{position} {shown} Tj ET", f(size))
            }
            Face::Unicode { .. } if bold => {
                let stroke = color
                    .map(|c| format!("{} {} {} RG ", f(c[0]), f(c[1]), f(c[2])))
                    .unwrap_or_default();
                format!(
                    "q {stroke}{} w BT /{name} {} Tf 2 Tr {fill}{position} {shown} Tj ET Q",
                    f(size * 0.04),
                    f(size)
                )
            }
            Face::Unicode { .. } => format!("BT /{name} {} Tf {fill}{position} {shown} Tj ET", f(size)),
        })
    }

    /// The text to lay out: as given for the standard font, prepared for the
    /// Unicode font.
    pub(crate) fn text(&self) -> &str {
        &self.text
    }

    pub(crate) fn is_standard(&self) -> bool {
        matches!(self.face, Face::Standard(_))
    }

    /// Name of the font in the resources of the drawing.
    pub(crate) fn resource_name(&self) -> String {
        match self.face {
            Face::Standard(f) => f.resource().to_string(),
            Face::Unicode { .. } => self.unicode_resource.clone(),
        }
    }

    /// Name the Unicode font `name` in the resources instead of
    /// [`UNICODE_RESOURCE`]: drawings into the content of a page, whose
    /// resources other drawings share, take a name no other font there has.
    pub(crate) fn use_resource_name(&mut self, name: &str) {
        self.unicode_resource = name.to_string();
    }

    /// Width of one line at `size` points, with the widths written in the font.
    pub(crate) fn width(&self, line: &str, size: f32) -> f32 {
        match &self.face {
            Face::Standard(f) => f.width(line, size),
            Face::Unicode { font, .. } => {
                let units: u32 = line
                    .chars()
                    .filter_map(|c| font.glyph(c))
                    .map(|g| font.advance(g) as u32)
                    .sum();
                units as f32 * size / 1000.0
            }
        }
    }

    /// The lines of the text, no wider than `max_width` at `size` points.
    pub(crate) fn wrap(&self, size: f32, max_width: f32) -> Vec<String> {
        match self.face {
            Face::Standard(StdFont::Helvetica) => wrap_text(&self.text, size, max_width),
            Face::Standard(f) => wrap_text_by(&self.text, max_width.max(size), &|s| f.width(s, size)),
            Face::Unicode { .. } => wrap_text_by(&self.text, max_width.max(size), &|s| self.width(s, size)),
        }
    }

    /// The string operand that shows `line`: a literal of WinAnsi codes, or
    /// the two-byte codes of the Unicode font (given on first use).
    pub(crate) fn show(&mut self, line: &str) -> std::result::Result<String, GlyphProblem> {
        match &mut self.face {
            Face::Standard(_) => Ok(String::from_utf8_lossy(&pdf_literal(line)).to_string()),
            Face::Unicode { font, chars, codes } => {
                let mut hex = String::with_capacity(2 + 4 * line.len());
                hex.push('<');
                for c in visual_order(line).chars() {
                    let code = match codes.get(&c) {
                        Some(code) => *code,
                        None => {
                            let glyph = font.glyph(c).ok_or_else(|| GlyphProblem::Missing {
                                font: font.name.clone(),
                                chars: vec![c],
                            })?;
                            if chars.len() >= u16::MAX as usize - 1 {
                                return Err(GlyphProblem::Missing {
                                    font: font.name.clone(),
                                    chars: vec![c],
                                });
                            }
                            chars.push((c, glyph));
                            let code = chars.len() as u16;
                            codes.insert(c, code);
                            code
                        }
                    };
                    hex.push_str(&format!("{code:04X}"));
                }
                hex.push('>');
                Ok(hex)
            }
        }
    }

    /// The font object for the resources of the drawing: the shared standard
    /// Helvetica, `reuse` when it is the same subset (a drawing made again
    /// with the same characters), else a new subset.
    pub(crate) fn finish(self, doc: &mut Document, reuse: Option<ObjectId>) -> Result<ObjectId> {
        self.font_object(doc, reuse)
    }

    /// [`TextWriter::finish`] for a drawing that needs the font before its
    /// strings: every character must have been shown once already, and the
    /// strings shown after this use the same codes.
    pub(crate) fn font_object(&self, doc: &mut Document, reuse: Option<ObjectId>) -> Result<ObjectId> {
        match &self.face {
            Face::Standard(StdFont::Helvetica) => Ok(doc.helvetica_font_id()),
            Face::Standard(f) => Ok(doc.standard_font_id(f.base_font())),
            Face::Unicode { font, chars, .. } => {
                let parts = build_subset(font, chars)?;
                if let Some(id) = reuse.filter(|id| doc.same_subset(*id, &parts)) {
                    return Ok(id);
                }
                Ok(doc.add_unicode_font(font, parts))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn problem(text: &str) -> GlyphProblem {
        TextWriter::new(text).err().unwrap()
    }

    #[test]
    fn winansi_text_uses_the_standard_font() {
        let w = TextWriter::new("Città € “ok” \t\r\n").unwrap();
        assert!(w.is_standard());
        assert_eq!(w.text(), "Città € “ok” \t\r\n");
        assert_eq!(w.resource_name(), "Helv");
    }

    #[test]
    fn nfc_composes_before_choosing() {
        let w = TextWriter::new("Cafe\u{301}").unwrap();
        assert!(w.is_standard());
        assert_eq!(w.text(), "Café");
    }

    #[test]
    fn layout_needing_text_is_refused() {
        assert_eq!(
            problem("سلام"),
            GlyphProblem::NeedsLayout(vec!['س', 'ل', 'ا', 'م'])
        );
        assert_eq!(problem("x\u{301}"), GlyphProblem::NeedsLayout(vec!['\u{301}']));
        assert_eq!(problem("a\u{200D}b"), GlyphProblem::NeedsLayout(vec!['\u{200D}']));
        assert_eq!(
            problem("ok \u{202E}ko"),
            GlyphProblem::NeedsLayout(vec!['\u{202E}'])
        );
        assert_eq!(problem("١٢"), GlyphProblem::NeedsLayout(vec!['١', '٢']));
    }

    #[test]
    fn hebrew_is_drawn_from_right_to_left() {
        assert_eq!(visual_order("plain"), "plain");
        assert_eq!(visual_order("שלום"), "םולש");
        // Latin and numbers keep their order inside, brackets turn round.
        assert_eq!(visual_order("שלום (abc)"), "(abc) םולש");
        assert_eq!(visual_order("שלום 12"), "12 םולש");
        assert_eq!(visual_order("abc שלום def"), "abc םולש def");
        assert!(right_to_left("12 שלום") && !right_to_left("abc שלום"));
        let mut writer = TextWriter::new("שלום").unwrap();
        assert!(!writer.is_standard());
        assert_eq!(writer.show("שלום").unwrap(), "<0001000200030004>");
        // Points over the letters need a layout.
        assert_eq!(
            problem("שָׁ"),
            GlyphProblem::NeedsLayout(vec!['\u{5B8}', '\u{5C1}'])
        );
    }

    #[test]
    fn invisible_controls_and_selectors_are_not_drawn() {
        let w = TextWriter::new("©\u{FE0F}").unwrap();
        assert!(w.is_standard());
        assert_eq!(w.text(), "©");
        let w = TextWriter::new("Łó\u{200B}dź\u{2028}x").unwrap();
        assert!(!w.is_standard());
        assert_eq!(w.text(), "Łódź\nx");
    }

    #[test]
    fn minus_sign_uses_the_unicode_font() {
        let w = TextWriter::new("5 \u{2212} 3").unwrap();
        assert!(!w.is_standard());
        assert_eq!(w.resource_name(), UNICODE_RESOURCE);
    }

    #[test]
    fn missing_glyphs_are_named() {
        assert_eq!(
            problem("漢字 ł 漢"),
            GlyphProblem::Missing {
                font: "FreeSans".into(),
                chars: vec!['漢', '字']
            }
        );
    }

    #[test]
    fn codes_follow_first_use_one_per_character() {
        let mut w = TextWriter::new("abcał").unwrap();
        assert_eq!(w.show("abca").unwrap(), "<0001000200030001>");
        assert_eq!(w.show("ł a").unwrap(), "<000400050001>");
        let Face::Unicode { chars, .. } = &w.face else {
            panic!()
        };
        let order: String = chars.iter().map(|(c, _)| *c).collect();
        assert_eq!(order, "abcł ");
    }

    #[test]
    fn widths_come_from_the_font() {
        let w = TextWriter::new("Ł").unwrap();
        let Face::Unicode { font, .. } = &w.face else {
            panic!()
        };
        let advance = font.advance(font.glyph('Ł').unwrap()) as f32;
        assert!((w.width("Ł", 10.0) - advance * 10.0 / 1000.0).abs() < 1e-4);
        // Latin letters are close to their Helvetica widths (about 2 % narrower).
        let ratio = w.width("Hello world", 12.0) / helvetica_width("Hello world", 12.0);
        assert!((0.96..1.0).contains(&ratio), "{ratio}");
    }

    #[test]
    fn standard_text_objects_are_written_as_before() {
        let mut w = TextWriter::for_pieces(["Total", "Città"]).unwrap();
        assert!(w.is_standard());
        assert_eq!(w.piece("Città"), "Città");
        assert_eq!(
            w.text_object(
                "Città",
                10.0,
                Some([0.0, 0.5, 1.0]),
                "12 34 Td",
                false,
                ("TR", "TB")
            )
            .unwrap(),
            "BT /TR 10 Tf 0 0.5 1 rg 12 34 Td (Citt\\340) Tj ET"
        );
        assert_eq!(
            w.text_object("x", 8.0, None, "1 0 0 -1 2 3 Tm", true, ("H", "HB"))
                .unwrap(),
            "BT /HB 8 Tf 1 0 0 -1 2 3 Tm (x) Tj ET"
        );
    }

    #[test]
    fn unicode_bold_is_stroked_and_kept_apart() {
        let mut w = TextWriter::for_pieces(["Łódź", "total"]).unwrap();
        assert!(!w.is_standard());
        let plain = w
            .text_object(
                "total",
                10.0,
                Some([1.0, 0.0, 0.0]),
                "5 6 Td",
                false,
                ("TR", "TB"),
            )
            .unwrap();
        assert_eq!(
            plain,
            "BT /FoliUni 10 Tf 1 0 0 rg 5 6 Td <00010002000100030004> Tj ET"
        );
        let bold = w
            .text_object("Łódź", 10.0, Some([1.0, 0.0, 0.0]), "5 6 Td", true, ("TR", "TB"))
            .unwrap();
        assert!(
            bold.starts_with("q 1 0 0 RG 0.4 w BT /FoliUni 10 Tf 2 Tr 1 0 0 rg 5 6 Td <"),
            "{bold}"
        );
        assert!(bold.ends_with("> Tj ET Q"), "{bold}");
    }

    #[test]
    fn pieces_are_prepared_like_the_whole() {
        // Invisible characters make the whole text prepared, though it then
        // fits WinAnsi: every piece leaves them out too (no '?').
        let w = TextWriter::for_pieces(["a\u{200B}b", "c"]).unwrap();
        assert!(w.is_standard());
        assert_eq!(w.piece("a\u{200B}b"), "ab");
        let w = TextWriter::for_pieces(["Łó\u{200B}dź"]).unwrap();
        assert_eq!(w.piece("Łó\u{200B}dź"), "Łódź");
        let lines = w.wrap_piece("Łódź Łódź Łódź", 12.0, 40.0);
        assert!(lines.len() > 1, "{lines:?}");
    }

    #[test]
    fn wrapping_measures_with_the_font() {
        let w = TextWriter::new("Łódź Łódź Łódź Łódź Łódź").unwrap();
        let lines = w.wrap(12.0, 80.0);
        assert!(lines.len() > 1, "{lines:?}");
        assert!(lines.iter().all(|l| w.width(l, 12.0) <= 80.0 + 0.01), "{lines:?}");
        assert_eq!(lines.join(" "), w.text());
    }
}
