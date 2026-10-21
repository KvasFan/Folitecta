//! Typesetting of the documents Folitecta makes from structured text
//! (Markdown, CSV): lines of styled text, rules, boxes and tables laid out
//! top to bottom on pages. Text whose characters are all in WinAnsi is set in
//! the standard Helvetica family and Courier; other text in the embedded
//! Unicode font for the whole document (bold stroked, italic slanted, code in
//! the same font). Links become link annotations and headings bookmarks.

use lopdf::{Object, ObjectId, dictionary};

use crate::document::Document;
use crate::error::Result;
use crate::fonts::{GlyphProblem, TextWriter};
use crate::outline::{DestView, OutlineItem};
use crate::util::{fmt_num, helvetica_bold_width, helvetica_width, pdf_literal};

/// How a piece of text is set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct Style {
    pub bold: bool,
    pub italic: bool,
    pub mono: bool,
    pub strike: bool,
}

/// A piece of text in one style, maybe a link.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Span {
    pub text: String,
    pub style: Style,
    pub link: Option<String>,
}

impl Span {
    pub fn plain(text: impl Into<String>) -> Span {
        Span {
            text: text.into(),
            style: Style::default(),
            link: None,
        }
    }

    pub fn styled(text: impl Into<String>, style: Style) -> Span {
        Span {
            text: text.into(),
            style,
            link: None,
        }
    }
}

/// Where the lines of a paragraph or a cell go across its width.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum Align {
    #[default]
    Left,
    Center,
    Right,
}

/// Resource names and base fonts of the standard faces: regular, bold,
/// italic, bold italic, monospaced.
const NAMES: [&str; 5] = ["FoliHelv", "FoliHelvB", "FoliHelvI", "FoliHelvBI", "FoliCour"];
const BASES: [&str; 5] = [
    "Helvetica",
    "Helvetica-Bold",
    "Helvetica-Oblique",
    "Helvetica-BoldOblique",
    "Courier",
];

/// Colour of links.
const LINK: [f32; 3] = [0.1, 0.32, 0.78];

fn face(style: Style) -> usize {
    match (style.mono, style.bold, style.italic) {
        (true, _, _) => 4,
        (false, false, false) => 0,
        (false, true, false) => 1,
        (false, false, true) => 2,
        (false, true, true) => 3,
    }
}

/// The fonts of a document, chosen for all of its text.
pub(crate) struct Fonts {
    writer: TextWriter,
}

impl Fonts {
    /// Fonts for `texts` (every text the document will show): text that
    /// cannot be drawn is refused here.
    pub fn new<'a>(texts: impl IntoIterator<Item = &'a str>) -> std::result::Result<Fonts, GlyphProblem> {
        Ok(Fonts {
            writer: TextWriter::for_pieces(texts)?,
        })
    }

    pub fn standard(&self) -> bool {
        self.writer.is_standard()
    }

    /// Text as these fonts draw it (prepared like the whole for the Unicode font).
    pub fn piece(&self, text: &str) -> String {
        self.writer.piece(text)
    }

    pub fn width(&self, text: &str, style: Style, size: f32) -> f32 {
        if !self.standard() {
            return self.writer.width(text, size);
        }
        match face(style) {
            4 => text.chars().count() as f32 * 0.6 * size,
            1 | 3 => helvetica_bold_width(text, size),
            _ => helvetica_width(text, size),
        }
    }

    /// The operations that draw `text` with its baseline starting at `(x, y)`.
    fn show(
        &mut self,
        text: &str,
        style: Style,
        size: f32,
        (x, y): (f32, f32),
        color: [f32; 3],
    ) -> std::result::Result<String, GlyphProblem> {
        let f = fmt_num;
        if self.standard() {
            return Ok(format!(
                "BT /{} {} Tf {} {} {} rg 1 0 0 1 {} {} Tm {} Tj ET\n",
                NAMES[face(style)],
                f(size),
                f(color[0]),
                f(color[1]),
                f(color[2]),
                f(x),
                f(y),
                String::from_utf8_lossy(&pdf_literal(text))
            ));
        }
        // The Unicode font has no italic: slanted by about 12 degrees.
        let at = if style.italic {
            format!("1 0 0.21 1 {} {} Tm", f(x), f(y))
        } else {
            format!("1 0 0 1 {} {} Tm", f(x), f(y))
        };
        let mut s =
            self.writer
                .text_object(text, size, Some(color), &at, style.bold, (NAMES[0], NAMES[1]))?;
        s.push('\n');
        Ok(s)
    }

    /// Put the fonts in the resources of `pages`.
    fn finish(self, doc: &mut Document, pages: &[ObjectId]) -> Result<()> {
        if self.standard() {
            let ids: Vec<ObjectId> = BASES
                .iter()
                .map(|base| {
                    doc.inner.add_object(dictionary! {
                        "Type" => "Font",
                        "Subtype" => "Type1",
                        "BaseFont" => *base,
                        "Encoding" => "WinAnsiEncoding",
                    })
                })
                .collect();
            for &page in pages {
                for (name, id) in NAMES.iter().zip(&ids) {
                    doc.add_page_resource(page, "Font", name, Object::Reference(*id))?;
                }
            }
        } else {
            let name = self.writer.resource_name();
            let font = self.writer.finish(doc, None)?;
            for &page in pages {
                doc.add_page_resource(page, "Font", &name, Object::Reference(font))?;
            }
        }
        Ok(())
    }
}

/// One word of a line (or a piece of a word too long for any line), with
/// whether a space comes before it.
#[derive(Debug, Clone, PartialEq)]
struct Word {
    text: String,
    style: Style,
    link: Option<String>,
    space: bool,
}

/// A line made of words, set left to right.
pub(crate) type Line = Vec<Span>;

/// Page size and margins.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Sheet {
    pub width: f32,
    pub height: f32,
    pub margin: f32,
}

struct PageOut {
    content: String,
    /// Links: rectangle (PDF user space) and address.
    links: Vec<([f32; 4], String)>,
}

/// A heading: its level, text, page and the top of its first line.
struct Mark {
    level: u8,
    title: String,
    page: usize,
    top: f32,
}

/// Pages being filled, top to bottom.
pub(crate) struct Typeset {
    pub sheet: Sheet,
    pub fonts: Fonts,
    pages: Vec<PageOut>,
    /// The top of the next line (PDF user space, from the bottom).
    y: f32,
    /// Bars drawn beside every line (block quotes): their x.
    pub bars: Vec<f32>,
    marks: Vec<Mark>,
}

impl Typeset {
    pub fn new(sheet: Sheet, fonts: Fonts) -> Typeset {
        Typeset {
            sheet,
            fonts,
            pages: vec![PageOut {
                content: String::new(),
                links: Vec::new(),
            }],
            y: sheet.height - sheet.margin,
            bars: Vec::new(),
            marks: Vec::new(),
        }
    }

    fn top(&self) -> f32 {
        self.sheet.height - self.sheet.margin
    }

    /// The left and right edges of the text.
    pub fn left(&self) -> f32 {
        self.sheet.margin
    }

    pub fn right(&self) -> f32 {
        self.sheet.width - self.sheet.margin
    }

    /// The page being filled is still empty.
    pub fn at_top(&self) -> bool {
        self.y >= self.top() - 0.01
    }

    fn out(&mut self) -> &mut String {
        &mut self.pages.last_mut().expect("a page").content
    }

    pub fn new_page(&mut self) {
        self.pages.push(PageOut {
            content: String::new(),
            links: Vec::new(),
        });
        self.y = self.top();
    }

    /// Start a new page unless `h` points fit below the cursor.
    pub fn need(&mut self, h: f32) {
        if self.y - h < self.sheet.margin && !self.at_top() {
            self.new_page();
        }
    }

    /// Leave `h` points of space (none at the top of a page), with the bars
    /// of the quotes beside it.
    pub fn space(&mut self, h: f32) {
        if self.at_top() {
            return;
        }
        let h = h.min(self.y - self.sheet.margin).max(0.0);
        self.draw_bars(self.y, self.y - h);
        self.y -= h;
    }

    fn draw_bars(&mut self, top: f32, bottom: f32) {
        if self.bars.is_empty() || top - bottom < 0.01 {
            return;
        }
        let f = fmt_num;
        let mut s = String::from("q 0.78 0.8 0.84 RG 2 w\n");
        for x in &self.bars {
            s.push_str(&format!("{} {} m {} {} l S\n", f(*x), f(top), f(*x), f(bottom)));
        }
        s.push_str("Q\n");
        self.out().push_str(&s);
    }

    /// Remember a heading at the top of the next line (a bookmark).
    pub fn mark(&mut self, level: u8, title: &str) {
        let title = title.split_whitespace().collect::<Vec<_>>().join(" ");
        if title.is_empty() {
            return;
        }
        self.marks.push(Mark {
            level,
            title,
            page: self.pages.len() - 1,
            top: self.y,
        });
    }

    /// The words of `spans`, prepared for the fonts.
    fn words(&self, spans: &[Span]) -> Vec<Option<Word>> {
        // None: a line break.
        let mut out = Vec::new();
        let mut space = false;
        for s in spans {
            let text = self.fonts.piece(&s.text);
            let mut word = String::new();
            let flush = |word: &mut String, space: &mut bool, out: &mut Vec<Option<Word>>| {
                if !word.is_empty() {
                    out.push(Some(Word {
                        text: std::mem::take(word),
                        style: s.style,
                        link: s.link.clone(),
                        space: *space,
                    }));
                    *space = false;
                }
            };
            for c in text.chars() {
                if c == '\n' {
                    flush(&mut word, &mut space, &mut out);
                    out.push(None);
                    space = false;
                } else if c.is_whitespace() {
                    flush(&mut word, &mut space, &mut out);
                    space = true;
                } else {
                    word.push(c);
                }
            }
            flush(&mut word, &mut space, &mut out);
        }
        out
    }

    /// `spans` broken into lines no wider than `width` at `size`; a word
    /// longer than a line is broken between its letters.
    pub fn break_lines(&self, spans: &[Span], size: f32, width: f32) -> Vec<Line> {
        let fonts = &self.fonts;
        let mut lines: Vec<Vec<Word>> = vec![Vec::new()];
        let mut used = 0.0f32;
        for w in self.words(spans) {
            let Some(mut w) = w else {
                lines.push(Vec::new());
                used = 0.0;
                continue;
            };
            let line_empty = lines.last().is_none_or(|l| l.is_empty());
            // The space before a word is set in the style of the word before it.
            let gap = match lines.last().and_then(|l| l.last()) {
                Some(prev) if w.space => fonts.width(" ", prev.style, size),
                _ => 0.0,
            };
            let ww = fonts.width(&w.text, w.style, size);
            if used + gap + ww <= width + 0.01 {
                used += gap + ww;
                lines.last_mut().expect("a line").push(w);
                continue;
            }
            if !line_empty {
                lines.push(Vec::new());
                w.space = false;
            }
            // Too long for a line: as many letters as fit, line by line.
            let mut rest = w.text.clone();
            loop {
                if fonts.width(&rest, w.style, size) <= width + 0.01 {
                    used = fonts.width(&rest, w.style, size);
                    lines.last_mut().expect("a line").push(Word {
                        text: rest,
                        ..w.clone()
                    });
                    break;
                }
                let mut cut = String::new();
                for c in rest.chars() {
                    cut.push(c);
                    if fonts.width(&cut, w.style, size) > width && cut.chars().count() > 1 {
                        cut.pop();
                        break;
                    }
                }
                rest = rest[cut.len()..].to_string();
                lines.last_mut().expect("a line").push(Word {
                    text: cut,
                    space: false,
                    ..w.clone()
                });
                lines.push(Vec::new());
                used = 0.0;
                if rest.is_empty() {
                    break;
                }
            }
        }
        // Words in a row of the same style and link are drawn together.
        lines
            .into_iter()
            .map(|words| {
                let mut line: Line = Vec::new();
                for w in words {
                    if let Some(last) = line.last_mut() {
                        if w.space {
                            last.text.push(' ');
                        }
                    }
                    match line.last_mut() {
                        Some(last) if last.style == w.style && last.link == w.link => {
                            last.text.push_str(&w.text)
                        }
                        _ => line.push(Span {
                            text: w.text,
                            style: w.style,
                            link: w.link,
                        }),
                    }
                }
                line
            })
            .collect()
    }

    /// `text` cut into lines no wider than `width` between any two letters,
    /// its spaces kept (code).
    pub fn cut_lines(&self, text: &str, style: Style, size: f32, width: f32) -> Vec<Line> {
        let text = self.fonts.piece(text);
        let mut lines = Vec::new();
        let mut cur = String::new();
        for c in text.chars() {
            cur.push(c);
            if self.fonts.width(&cur, style, size) > width + 0.01 && cur.chars().count() > 1 {
                cur.pop();
                lines.push(vec![Span::styled(std::mem::take(&mut cur), style)]);
                cur.push(c);
            }
        }
        lines.push(if cur.is_empty() {
            Vec::new()
        } else {
            vec![Span::styled(cur, style)]
        });
        lines
    }

    /// Width of a line.
    pub fn line_width(&self, line: &Line, size: f32) -> f32 {
        line.iter()
            .map(|s| self.fonts.width(&s.text, s.style, size))
            .sum()
    }

    /// Draw `line` with its baseline at `y`, starting at `x`.
    fn draw_line(&mut self, line: &Line, size: f32, x: f32, y: f32, color: [f32; 3]) -> Result<()> {
        let mut x = x;
        for s in line {
            let w = self.fonts.width(&s.text, s.style, size);
            let color = if s.link.is_some() { LINK } else { color };
            let ops = self.fonts.show(&s.text, s.style, size, (x, y), color)?;
            self.out().push_str(&ops);
            let f = fmt_num;
            // Links underlined, struck text crossed out.
            // The words only, without the spaces around them.
            let lead = s.text.len() - s.text.trim_start().len();
            let x0 = x + self.fonts.width(&s.text[..lead], s.style, size);
            let trail = s.text.trim_end().len().max(lead);
            let x1 = x + w - self.fonts.width(&s.text[trail..], s.style, size);
            if s.link.is_some() || s.style.strike {
                let ly = if s.style.strike {
                    y + size * 0.3
                } else {
                    y - size * 0.12
                };
                let line = format!(
                    "q {} {} {} RG {} w {} {} m {} {} l S Q\n",
                    f(color[0]),
                    f(color[1]),
                    f(color[2]),
                    f(size * 0.06),
                    f(x0),
                    f(ly),
                    f(x1),
                    f(ly)
                );
                self.out().push_str(&line);
            }
            if let Some(link) = &s.link {
                let rect = [x0, y - size * 0.25, x1, y + size * 0.85];
                self.pages
                    .last_mut()
                    .expect("a page")
                    .links
                    .push((rect, link.clone()));
            }
            x += w;
        }
        Ok(())
    }

    /// Set `lines` from the cursor down, `leading` apart, within `x0` and
    /// `x0 + width`, on as many pages as they need.
    pub fn lines(
        &mut self,
        lines: &[Line],
        size: f32,
        leading: f32,
        x0: f32,
        width: f32,
        color: [f32; 3],
        align: Align,
    ) -> Result<()> {
        for line in lines {
            self.need(leading);
            let w = self.line_width(line, size);
            let x = match align {
                Align::Left => x0,
                Align::Center => x0 + (width - w) / 2.0,
                Align::Right => x0 + width - w,
            };
            // The baseline, with the room of the line split above and below.
            let y = self.y - (leading + size * 0.7) / 2.0;
            self.draw_bars(self.y, self.y - leading);
            self.draw_line(line, size, x, y, color)?;
            self.y -= leading;
        }
        Ok(())
    }

    /// A paragraph: `spans` broken into lines and set.
    #[allow(clippy::too_many_arguments)]
    pub fn paragraph(
        &mut self,
        spans: &[Span],
        size: f32,
        leading: f32,
        x0: f32,
        width: f32,
        color: [f32; 3],
        align: Align,
    ) -> Result<()> {
        let lines = self.break_lines(spans, size, width);
        self.lines(&lines, size, leading, x0, width, color, align)
    }

    /// Text drawn at a place of the line about to be set (a list marker),
    /// once room for the line is made.
    pub fn marker(
        &mut self,
        text: &str,
        size: f32,
        leading: f32,
        x_right: f32,
        color: [f32; 3],
    ) -> Result<()> {
        self.need(leading);
        let text = self.fonts.piece(text);
        let w = self.fonts.width(&text, Style::default(), size);
        let y = self.y - (leading + size * 0.7) / 2.0;
        let ops = self
            .fonts
            .show(&text, Style::default(), size, (x_right - w, y), color)?;
        self.out().push_str(&ops);
        Ok(())
    }

    /// A horizontal rule across `x0`..`x1`, `gap` points below the cursor.
    pub fn rule(&mut self, x0: f32, x1: f32, gap: f32, gray: f32) {
        self.need(gap * 2.0);
        let y = self.y - gap;
        let f = fmt_num;
        let s = format!(
            "q {} G 0.75 w {} {} m {} {} l S Q\n",
            f(gray),
            f(x0),
            f(y),
            f(x1),
            f(y)
        );
        self.out().push_str(&s);
        self.y -= gap * 2.0;
    }

    /// Lines set over a gray box (code), the box drawn page by page.
    #[allow(clippy::too_many_arguments)]
    pub fn boxed_lines(
        &mut self,
        lines: &[Line],
        size: f32,
        leading: f32,
        x0: f32,
        width: f32,
        pad: f32,
        color: [f32; 3],
    ) -> Result<()> {
        let mut rest = lines;
        let f = fmt_num;
        while !rest.is_empty() {
            self.need(leading + 2.0 * pad);
            let room = ((self.y - self.sheet.margin - 2.0 * pad) / leading)
                .floor()
                .max(1.0) as usize;
            let (now, later) = rest.split_at(room.min(rest.len()));
            let h = now.len() as f32 * leading + 2.0 * pad;
            let bg = format!(
                "q 0.95 0.955 0.965 rg {} {} {} {} re f Q\n",
                f(x0),
                f(self.y - h),
                f(width),
                f(h)
            );
            self.out().push_str(&bg);
            self.draw_bars(self.y, self.y - h);
            self.y -= pad;
            for line in now {
                let y = self.y - (leading + size * 0.7) / 2.0;
                self.draw_line(line, size, x0 + pad, y, color)?;
                self.y -= leading;
            }
            self.y -= pad;
            rest = later;
            if !rest.is_empty() {
                self.new_page();
            }
        }
        Ok(())
    }

    /// A table: `head` repeated at the top of every page it runs onto, the
    /// columns `widths` wide from `x0`, cells `pad` from their borders.
    #[allow(clippy::too_many_arguments)]
    pub fn table(
        &mut self,
        head: Option<&[Vec<Span>]>,
        rows: &[Vec<Vec<Span>>],
        widths: &[f32],
        aligns: &[Align],
        size: f32,
        leading: f32,
        x0: f32,
        pad: f32,
    ) -> Result<()> {
        let head_lines = head.map(|h| self.cell_lines(h, widths, size, pad, true));
        let head_h = head_lines
            .as_ref()
            .map(|l| self.row_height(l, leading, pad))
            .unwrap_or(0.0);
        let mut first = true;
        for row in rows {
            let lines = self.cell_lines(row, widths, size, pad, false);
            let h = self.row_height(&lines, leading, pad);
            let at_start = first || self.y - h < self.sheet.margin;
            if at_start {
                self.need(head_h + h);
                if let Some(hl) = &head_lines {
                    self.row(hl, widths, aligns, size, leading, x0, pad, head_h, true)?;
                }
                first = false;
            }
            self.row(&lines, widths, aligns, size, leading, x0, pad, h, false)?;
        }
        if first {
            if let Some(hl) = &head_lines {
                self.need(head_h);
                self.row(hl, widths, aligns, size, leading, x0, pad, head_h, true)?;
            }
        }
        Ok(())
    }

    fn cell_lines(
        &self,
        cells: &[Vec<Span>],
        widths: &[f32],
        size: f32,
        pad: f32,
        bold: bool,
    ) -> Vec<Vec<Line>> {
        widths
            .iter()
            .enumerate()
            .map(|(i, w)| {
                let mut spans = cells.get(i).cloned().unwrap_or_default();
                if bold {
                    spans.iter_mut().for_each(|s| s.style.bold = true);
                }
                self.break_lines(&spans, size, (w - 2.0 * pad).max(1.0))
            })
            .collect()
    }

    fn row_height(&self, cells: &[Vec<Line>], leading: f32, pad: f32) -> f32 {
        cells.iter().map(|l| l.len()).max().unwrap_or(1).max(1) as f32 * leading + 2.0 * pad
    }

    #[allow(clippy::too_many_arguments)]
    fn row(
        &mut self,
        cells: &[Vec<Line>],
        widths: &[f32],
        aligns: &[Align],
        size: f32,
        leading: f32,
        x0: f32,
        pad: f32,
        h: f32,
        head: bool,
    ) -> Result<()> {
        let f = fmt_num;
        let total: f32 = widths.iter().sum();
        let (top, bottom) = (self.y, self.y - h);
        let mut s = String::new();
        if head {
            s.push_str(&format!(
                "q 0.92 0.93 0.95 rg {} {} {} {} re f Q\n",
                f(x0),
                f(bottom),
                f(total),
                f(h)
            ));
        }
        // The borders of the row.
        s.push_str("q 0.7 0.72 0.76 RG 0.5 w\n");
        s.push_str(&format!("{} {} {} {} re S\n", f(x0), f(bottom), f(total), f(h)));
        let mut x = x0;
        for w in &widths[..widths.len().saturating_sub(1)] {
            x += w;
            s.push_str(&format!("{} {} m {} {} l S\n", f(x), f(top), f(x), f(bottom)));
        }
        s.push_str("Q\n");
        self.out().push_str(&s);
        let mut x = x0;
        for (i, (lines, w)) in cells.iter().zip(widths).enumerate() {
            let align = aligns.get(i).copied().unwrap_or_default();
            let mut y = top - pad;
            for line in lines {
                let lw = self.line_width(line, size);
                let lx = match align {
                    Align::Left => x + pad,
                    Align::Center => x + (w - lw) / 2.0,
                    Align::Right => x + w - pad - lw,
                };
                let base = y - (leading + size * 0.7) / 2.0;
                self.draw_line(line, size, lx, base, [0.1, 0.1, 0.12])?;
                y -= leading;
            }
            x += w;
        }
        self.y = bottom;
        Ok(())
    }

    /// The document: its pages, fonts, links and bookmarks (headings nested
    /// by level).
    pub fn finish(self, title: &str) -> Result<Document> {
        let Typeset {
            sheet,
            fonts,
            pages,
            marks,
            ..
        } = self;
        let mut doc = Document::new_blank(pages.len(), sheet.width, sheet.height)?;
        let ids = doc.page_ids();
        for (page, (out, id)) in pages.iter().zip(&ids).enumerate() {
            if !out.content.is_empty() {
                doc.append_page_content(*id, out.content.clone().into_bytes(), None)?;
            }
            for (rect, uri) in &out.links {
                let action = dictionary! { "S" => "URI", "URI" => Object::string_literal(uri.clone()) };
                doc.push_link_raw(page, *rect, action)?;
            }
        }
        fonts.finish(&mut doc, &ids)?;
        if !marks.is_empty() {
            let mut items: Vec<(u8, OutlineItem)> = Vec::new();
            for m in &marks {
                let view = DestView::Xyz {
                    left: Some(0.0),
                    top: Some(m.top),
                    zoom: None,
                };
                items.push((m.level, doc.new_bookmark_item(&m.title, m.page, Some(view))?));
            }
            doc.set_outline(&nest(items))?;
        }
        if !title.is_empty() {
            let _ = doc.set_info(title, "", "", "");
        }
        doc.set_path(None);
        doc.commit()?;
        doc.clear_history();
        Ok(doc)
    }
}

/// Widths of the columns of a table: as wide as their text when it fits,
/// else shared out from the width of their longest words.
pub(crate) fn column_widths(
    ts: &Typeset,
    head: &[Vec<Span>],
    rows: &[Vec<Vec<Span>>],
    size: f32,
    width: f32,
    pad: f32,
) -> Vec<f32> {
    let n = head
        .len()
        .max(rows.iter().map(|r| r.len()).max().unwrap_or(0))
        .max(1);
    let mut natural = vec![0.0f32; n];
    let mut least = vec![0.0f32; n];
    let measure = |cells: &[Vec<Span>], bold: bool, natural: &mut [f32], least: &mut [f32]| {
        for (i, cell) in cells.iter().enumerate().take(n) {
            let mut spans = cell.clone();
            if bold {
                spans.iter_mut().for_each(|s| s.style.bold = true);
            }
            let one = ts.break_lines(&spans, size, f32::MAX);
            let w = one.iter().map(|l| ts.line_width(l, size)).fold(0.0, f32::max);
            natural[i] = natural[i].max(w + 2.0 * pad);
            for s in &spans {
                for word in s.text.split_whitespace() {
                    let ww = ts.fonts.width(&ts.fonts.piece(word), s.style, size);
                    least[i] = least[i].max(ww + 2.0 * pad);
                }
            }
        }
    };
    measure(head, true, &mut natural, &mut least);
    for r in rows {
        measure(r, false, &mut natural, &mut least);
    }
    let total: f32 = natural.iter().sum();
    if total <= width {
        return natural;
    }
    // Each column gets its longest word (narrower when even that is too
    // wide), and the rest of the width goes to the columns by what they lack.
    let cap = width / n as f32;
    let least: Vec<f32> = least
        .iter()
        .zip(&natural)
        .map(|(l, nat)| l.min(*nat).min(cap.max(width * 0.5 / n as f32)))
        .collect();
    let base: f32 = least.iter().sum();
    let lack: f32 = natural.iter().zip(&least).map(|(n, l)| n - l).sum();
    let spare = (width - base).max(0.0);
    natural
        .iter()
        .zip(&least)
        .map(|(n, l)| l + if lack > 0.0 { (n - l) / lack * spare } else { 0.0 })
        .collect()
}

/// Bookmarks in order with their levels, nested: each under the last one of
/// a lower level before it.
fn nest(items: Vec<(u8, OutlineItem)>) -> Vec<OutlineItem> {
    fn take(
        items: &mut std::iter::Peekable<std::vec::IntoIter<(u8, OutlineItem)>>,
        above: u8,
    ) -> Vec<OutlineItem> {
        let mut out = Vec::new();
        while let Some((level, _)) = items.peek() {
            if *level <= above {
                break;
            }
            let (level, mut item) = items.next().expect("peeked");
            item.children = take(items, level);
            item.open = level <= 1;
            out.push(item);
        }
        out
    }
    take(&mut items.into_iter().peekable(), 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sheet() -> Sheet {
        Sheet {
            width: 300.0,
            height: 200.0,
            margin: 20.0,
        }
    }

    fn typeset(text: &str) -> Typeset {
        Typeset::new(sheet(), Fonts::new([text]).unwrap())
    }

    #[test]
    fn words_wrap_and_keep_their_styles() {
        let ts = typeset("Plain bold and a verylongwordthatcannotfitanywhereatall");
        let bold = Style {
            bold: true,
            ..Style::default()
        };
        let spans = [
            Span::plain("Plain "),
            Span::styled("bold", bold),
            Span::plain(" and a verylongwordthatcannotfitanywhereatall"),
        ];
        let lines = ts.break_lines(&spans, 10.0, 100.0);
        assert!(lines.len() >= 3, "{lines:?}");
        assert_eq!(lines[0][0].text, "Plain ");
        assert_eq!(lines[0][1], Span::styled("bold ", bold));
        for line in &lines {
            assert!(ts.line_width(line, 10.0) <= 100.01, "{line:?}");
        }
        // Breaks inside the long word lose no letter.
        let joined: String = lines.iter().flatten().map(|s| s.text.as_str()).collect();
        assert!(joined.contains("verylongword") && joined.ends_with("atall"));
        assert_eq!(
            joined.replace(' ', ""),
            "Plainboldandaverylongwordthatcannotfitanywhereatall"
        );
    }

    #[test]
    fn bold_is_wider_than_regular() {
        let ts = typeset("Wide");
        let bold = Style {
            bold: true,
            ..Style::default()
        };
        assert!(ts.fonts.width("Wide", bold, 10.0) > ts.fonts.width("Wide", Style::default(), 10.0));
        let mono = Style {
            mono: true,
            ..Style::default()
        };
        assert_eq!(ts.fonts.width("iiii", mono, 10.0), 24.0);
    }

    #[test]
    fn headings_nest_by_level() {
        let item = |t: &str| OutlineItem {
            title: t.into(),
            ..OutlineItem::default()
        };
        let tree = nest(vec![
            (1, item("A")),
            (2, item("A.1")),
            (3, item("A.1.a")),
            (2, item("A.2")),
            (1, item("B")),
            (3, item("B..a")),
        ]);
        assert_eq!(tree.len(), 2);
        assert_eq!(tree[0].children.len(), 2);
        assert_eq!(tree[0].children[0].children[0].title, "A.1.a");
        assert_eq!(tree[1].children[0].title, "B..a");
    }
}
