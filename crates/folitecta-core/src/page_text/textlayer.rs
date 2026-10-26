//! Positioned text extraction.
//!
//! Pages are interpreted with hayro using a device that records every glyph with
//! its Unicode text and its quadrilateral in *display space*: points, origin at the
//! top-left corner of the page as shown (after `/Rotate` and the crop box), y down.
//! The glyphs are assembled into lines and words; the resulting [`PageText`] maps
//! every character of its plain text back to the glyph geometry, which powers text
//! selection, search highlighting, text markup, redaction of text and link creation.

use std::sync::Arc;

use hayro::hayro_interpret::font::Glyph;
use hayro::hayro_interpret::font::GlyphRun;
use hayro::hayro_interpret::{
    BlendMode, ClipPath, Context, Device, DrawMode, DrawProps, Image, ImageDrawProps, InterpreterCache,
    InterpreterSettings, SoftMask, TransformExt, interpret_page,
};
use hayro::hayro_syntax::Pdf;
use hayro::hayro_syntax::page::Page;
use hayro::kurbo::{Affine, BezPath, Point, Rect, Shape, Vec2};

use crate::document::PageSize;
use crate::error::{Error, Result};
use crate::marks::display_to_pdf;
use crate::text::SearchOptions;

/// One glyph with its text and geometry (display space).
#[derive(Debug, Clone, PartialEq)]
pub struct TextGlyph {
    pub text: String,
    /// Corners: upper-left, upper-right, lower-left, lower-right.
    pub quad: [f32; 8],
    /// Axis-aligned bounds `[x0, y0, x1, y1]` (y down).
    pub bbox: [f32; 4],
    /// Font size in points (em height in display space).
    pub size: f32,
}

/// A line of text: a range of characters of [`PageText::text`].
#[derive(Debug, Clone, PartialEq)]
pub struct TextLine {
    pub start: usize,
    pub end: usize,
    pub bbox: [f32; 4],
}

/// The text of one page with geometry.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PageText {
    pub width: f32,
    pub height: f32,
    pub glyphs: Vec<TextGlyph>,
    /// Plain text; lines separated by `\n`.
    pub text: String,
    /// For each character of `text`, the glyph it came from (None for inserted spaces/newlines).
    pub char_glyph: Vec<Option<usize>>,
    pub lines: Vec<TextLine>,
    /// Bounds of placed images (display space).
    pub images: Vec<[f32; 4]>,
    /// Union of text, image and path bounds, when anything is drawn.
    pub ink_bbox: Option<[f32; 4]>,
}

struct RawGlyph {
    text: String,
    corners: [Point; 4],
    origin: Point,
    advance: Vec2,
    size: f64,
}

#[derive(Default)]
struct Collector {
    raw: Vec<RawGlyph>,
    images: Vec<Rect>,
    ink: Option<Rect>,
    last_fill: Vec<(i64, i64)>,
}

fn union(a: Option<Rect>, b: Rect) -> Option<Rect> {
    if !b.width().is_finite() || !b.height().is_finite() {
        return a;
    }
    Some(match a {
        Some(a) => a.union(b),
        None => b,
    })
}

impl<'a> Device<'a> for Collector {
    fn draw_path(&mut self, path: &BezPath, props: DrawProps<'a>, _mode: &DrawMode) {
        let r = (props.transform * path.clone()).bounding_box();
        self.ink = union(self.ink, r);
    }
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
        // Fill+stroke text arrives twice; keep the first copy only.
        if matches!(mode, DrawMode::Stroke(_)) && key == self.last_fill {
            return;
        }
        if !matches!(mode, DrawMode::Stroke(_)) {
            self.last_fill = key;
        }
        for (i, g) in glyphs.iter().enumerate() {
            let t: Affine = props.transform * g.transform();
            let (adv_units, text) = match &**g {
                Glyph::Outline(o) => (o.advance_width().map(|a| a as f64), g.as_unicode()),
                Glyph::Type3(_) => (None, g.as_unicode()),
            };
            let adv_units = adv_units.filter(|a| *a > 0.0).unwrap_or_else(|| {
                // Fall back to the distance to the next glyph of the run.
                glyphs
                    .get(i + 1)
                    .map(|n| {
                        let inv = g.transform().inverse();
                        (inv * n.transform() * Point::ZERO).x
                    })
                    .filter(|d| d.is_finite() && *d > 0.0 && *d < 3000.0)
                    .unwrap_or(500.0)
            });
            let text = match text {
                Some(hayro::hayro_interpret::hayro_cmap::BfString::Char(c)) => c.to_string(),
                Some(hayro::hayro_interpret::hayro_cmap::BfString::String(s)) => s,
                None => String::new(),
            };
            let origin = t * Point::ZERO;
            let advance = (t * Point::new(adv_units, 0.0)) - origin;
            let up = (t * Point::new(0.0, 1000.0)) - origin;
            let size = up.hypot();
            if !size.is_finite() || size < 0.01 {
                continue;
            }
            let corners = [
                t * Point::new(0.0, 800.0),
                t * Point::new(adv_units, 800.0),
                t * Point::new(0.0, -200.0),
                t * Point::new(adv_units, -200.0),
            ];
            let mut r = Rect::from_points(corners[0], corners[1]);
            r = r.union_pt(corners[2]).union_pt(corners[3]);
            self.ink = union(self.ink, r);
            self.raw.push(RawGlyph {
                text,
                corners,
                origin,
                advance,
                size,
            });
        }
    }
    fn draw_image(&mut self, _image: Image<'a, '_>, props: ImageDrawProps<'a>) {
        let unit = Rect::new(0.0, 0.0, 1.0, 1.0).to_path(0.1);
        let r = (props.transform * unit).bounding_box();
        self.images.push(r);
        self.ink = union(self.ink, r);
    }
    fn pop_clip(&mut self) {}
    fn pop_transparency_group(&mut self) {}
}

fn rect4(r: Rect) -> [f32; 4] {
    [r.x0 as f32, r.y0 as f32, r.x1 as f32, r.y1 as f32]
}

fn assemble(raw: Vec<RawGlyph>, width: f32, height: f32, images: Vec<Rect>, ink: Option<Rect>) -> PageText {
    let mut out = PageText {
        width,
        height,
        ..Default::default()
    };
    let mut prev: Option<&RawGlyph> = None;
    let mut line_start = 0usize;
    let mut line_bbox: Option<Rect> = None;
    let mut chars = 0usize;
    for g in raw.iter() {
        let gi = out.glyphs.len();
        let mut r = Rect::from_points(g.corners[0], g.corners[1]);
        r = r.union_pt(g.corners[2]).union_pt(g.corners[3]);
        if let Some(p) = prev {
            let dir = if p.advance.hypot() > 1e-6 {
                p.advance / p.advance.hypot()
            } else {
                Vec2::new(1.0, 0.0)
            };
            let delta = g.origin - p.origin;
            let perp = (dir.x * delta.y - dir.y * delta.x).abs();
            let p_end = p.origin + p.advance;
            let gap = dir.dot(g.origin - p_end);
            let gdir = if g.advance.hypot() > 1e-6 {
                g.advance / g.advance.hypot()
            } else {
                dir
            };
            let size = p.size.max(g.size);
            let new_line = perp > 0.5 * size || gap < -1.5 * size || dir.dot(gdir) < 0.95;
            if new_line {
                if let Some(b) = line_bbox.take() {
                    out.lines.push(TextLine {
                        start: line_start,
                        end: chars,
                        bbox: rect4(b),
                    });
                }
                out.text.push('\n');
                out.char_glyph.push(None);
                chars += 1;
                line_start = chars;
            } else if gap > 0.22 * size
                && !p.text.ends_with(char::is_whitespace)
                && !g.text.starts_with(char::is_whitespace)
            {
                out.text.push(' ');
                out.char_glyph.push(None);
                chars += 1;
            }
        }
        for c in g.text.chars() {
            // Normalise control characters and non-breaking spaces.
            let c = if c == '\u{a0}' || c.is_control() { ' ' } else { c };
            out.text.push(c);
            out.char_glyph.push(Some(gi));
            chars += 1;
        }
        line_bbox = union(line_bbox, r);
        let quad = [
            g.corners[0].x as f32,
            g.corners[0].y as f32,
            g.corners[1].x as f32,
            g.corners[1].y as f32,
            g.corners[2].x as f32,
            g.corners[2].y as f32,
            g.corners[3].x as f32,
            g.corners[3].y as f32,
        ];
        out.glyphs.push(TextGlyph {
            text: g.text.clone(),
            quad,
            bbox: rect4(r),
            size: g.size as f32,
        });
        prev = Some(g);
    }
    if let Some(b) = line_bbox {
        out.lines.push(TextLine {
            start: line_start,
            end: chars,
            bbox: rect4(b),
        });
    }
    out.images = images.into_iter().map(rect4).collect();
    out.ink_bbox = ink.map(rect4);
    out
}

fn extract_page(page: &Page<'_>) -> PageText {
    let (w, h) = page.render_dimensions();
    let cache = InterpreterCache::new();
    let settings = InterpreterSettings {
        render_annotations: false,
        ..InterpreterSettings::default()
    };
    let transform = page.initial_transform(true).to_kurbo();
    let mut ctx = Context::new(
        transform,
        Rect::new(0.0, 0.0, w as f64, h as f64),
        &cache,
        page.xref(),
        settings,
    );
    let mut collector = Collector::default();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        interpret_page(page, &mut ctx, &mut collector);
    }));
    if result.is_err() {
        return PageText {
            width: w,
            height: h,
            ..Default::default()
        };
    }
    assemble(collector.raw, w, h, collector.images, collector.ink)
}

/// Extract the text layer of every page of a serialized document.
pub fn extract_all(bytes: Arc<Vec<u8>>) -> Result<Vec<PageText>> {
    let pdf = Pdf::new(bytes).map_err(|e| Error::Render(format!("cannot parse document: {e:?}")))?;
    Ok(pdf.pages().iter().map(extract_page).collect())
}

/// Extract the text layers of some pages of a serialized document (pages
/// that do not exist are left out).
pub fn extract_pages(bytes: Arc<Vec<u8>>, pages: &[usize]) -> Result<Vec<(usize, PageText)>> {
    let pdf = Pdf::new(bytes).map_err(|e| Error::Render(format!("cannot parse document: {e:?}")))?;
    let all = pdf.pages();
    Ok(pages
        .iter()
        .filter_map(|&p| all.get(p).map(|page| (p, extract_page(page))))
        .collect())
}

/// Extract the text layer of one page of a serialized document.
pub fn extract_one(bytes: Arc<Vec<u8>>, page: usize) -> Result<PageText> {
    let pdf = Pdf::new(bytes).map_err(|e| Error::Render(format!("cannot parse document: {e:?}")))?;
    let p = pdf.pages().get(page).ok_or(Error::InvalidPage(page))?;
    Ok(extract_page(p))
}

/// Base letter of an accented Latin letter (`é` gives `e`); other characters
/// are returned unchanged.
pub fn strip_diacritic(c: char) -> char {
    match c {
        'À'..='Å' | 'Ā' | 'Ă' | 'Ą' => 'A',
        'à'..='å' | 'ā' | 'ă' | 'ą' => 'a',
        'Ç' | 'Ć' | 'Ĉ' | 'Ċ' | 'Č' => 'C',
        'ç' | 'ć' | 'ĉ' | 'ċ' | 'č' => 'c',
        'Ď' | 'Đ' => 'D',
        'ď' | 'đ' => 'd',
        'È'..='Ë' | 'Ē' | 'Ĕ' | 'Ė' | 'Ę' | 'Ě' => 'E',
        'è'..='ë' | 'ē' | 'ĕ' | 'ė' | 'ę' | 'ě' => 'e',
        'Ĝ' | 'Ğ' | 'Ġ' | 'Ģ' => 'G',
        'ĝ' | 'ğ' | 'ġ' | 'ģ' => 'g',
        'Ĥ' | 'Ħ' => 'H',
        'ĥ' | 'ħ' => 'h',
        'Ì'..='Ï' | 'Ĩ' | 'Ī' | 'Ĭ' | 'Į' | 'İ' => 'I',
        'ì'..='ï' | 'ĩ' | 'ī' | 'ĭ' | 'į' | 'ı' => 'i',
        'Ĵ' => 'J',
        'ĵ' => 'j',
        'Ķ' => 'K',
        'ķ' => 'k',
        'Ĺ' | 'Ļ' | 'Ľ' | 'Ŀ' | 'Ł' => 'L',
        'ĺ' | 'ļ' | 'ľ' | 'ŀ' | 'ł' => 'l',
        'Ñ' | 'Ń' | 'Ņ' | 'Ň' => 'N',
        'ñ' | 'ń' | 'ņ' | 'ň' => 'n',
        'Ò'..='Ö' | 'Ø' | 'Ō' | 'Ŏ' | 'Ő' => 'O',
        'ò'..='ö' | 'ø' | 'ō' | 'ŏ' | 'ő' => 'o',
        'Ŕ' | 'Ŗ' | 'Ř' => 'R',
        'ŕ' | 'ŗ' | 'ř' => 'r',
        'Ś' | 'Ŝ' | 'Ş' | 'Š' | 'Ș' => 'S',
        'ś' | 'ŝ' | 'ş' | 'š' | 'ș' => 's',
        'Ţ' | 'Ť' | 'Ŧ' | 'Ț' => 'T',
        'ţ' | 'ť' | 'ŧ' | 'ț' => 't',
        'Ù'..='Ü' | 'Ũ' | 'Ū' | 'Ŭ' | 'Ů' | 'Ű' | 'Ų' => 'U',
        'ù'..='ü' | 'ũ' | 'ū' | 'ŭ' | 'ů' | 'ű' | 'ų' => 'u',
        'Ŵ' => 'W',
        'ŵ' => 'w',
        'Ý' | 'Ŷ' | 'Ÿ' => 'Y',
        'ý' | 'ÿ' | 'ŷ' => 'y',
        'Ź' | 'Ż' | 'Ž' => 'Z',
        'ź' | 'ż' | 'ž' => 'z',
        _ => c,
    }
}

/// A combining accent (written after its letter).
fn is_combining(c: char) -> bool {
    matches!(c as u32, 0x300..=0x36F)
}

fn fold(c: char, opts: SearchOptions) -> char {
    let c = if opts.ignore_diacritics {
        strip_diacritic(c)
    } else {
        c
    };
    if opts.case_sensitive {
        c
    } else {
        c.to_lowercase().next().unwrap_or(c)
    }
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Character ranges `[start, end)` of `text` matching `query`. Whitespace in
/// the query matches any run of whitespace (including line breaks).
pub fn find_in(text: &str, query: &str, opts: SearchOptions) -> Vec<(usize, usize)> {
    let q: Vec<char> = query
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .filter(|c| !(opts.ignore_diacritics && is_combining(*c)))
        .map(|c| fold(c, opts))
        .collect();
    if q.is_empty() {
        return Vec::new();
    }
    let hay: Vec<char> = text.chars().map(|c| fold(c, opts)).collect();
    let skip = |c: char| opts.ignore_diacritics && is_combining(c);
    let mut out = Vec::new();
    let mut i = 0;
    while i < hay.len() {
        if skip(hay[i]) {
            i += 1;
            continue;
        }
        let mut j = i;
        let mut k = 0;
        while k < q.len() && j < hay.len() {
            if skip(hay[j]) {
                j += 1;
            } else if q[k] == ' ' {
                if !hay[j].is_whitespace() {
                    break;
                }
                while j < hay.len() && hay[j].is_whitespace() {
                    j += 1;
                }
                k += 1;
            } else if hay[j] == q[k] {
                j += 1;
                k += 1;
            } else {
                break;
            }
        }
        // Accents of the last letter belong to the match.
        while k == q.len() && j < hay.len() && skip(hay[j]) {
            j += 1;
        }
        if k == q.len() {
            let ok = !opts.whole_words
                || ((i == 0 || !is_word_char(hay[i - 1])) && (j >= hay.len() || !is_word_char(hay[j])));
            if ok {
                out.push((i, j));
                i = j.max(i + 1);
                continue;
            }
        }
        i += 1;
    }
    out
}

impl PageText {
    /// Characters of the page text.
    pub fn chars(&self) -> Vec<char> {
        self.text.chars().collect()
    }

    /// Text of the character range `[start, end)`.
    pub fn slice(&self, start: usize, end: usize) -> String {
        self.text
            .chars()
            .skip(start)
            .take(end.saturating_sub(start))
            .collect()
    }

    /// Character ranges `[start, end)` matching `query`. Whitespace in the query
    /// matches any run of whitespace (including line breaks) in the page.
    pub fn find(&self, query: &str, opts: SearchOptions) -> Vec<(usize, usize)> {
        find_in(&self.text, query, opts)
    }

    /// Character ranges of the words (runs of letters, digits and `_`).
    pub fn words(&self) -> Vec<(usize, usize)> {
        let mut out = Vec::new();
        let mut start = None;
        let mut n = 0;
        for (i, c) in self.text.chars().enumerate() {
            n = i + 1;
            match (is_word_char(c) || is_combining(c), start) {
                (true, None) => start = Some(i),
                (false, Some(s)) => {
                    out.push((s, i));
                    start = None;
                }
                _ => {}
            }
        }
        if let Some(s) = start {
            out.push((s, n));
        }
        out
    }

    /// Character ranges of the paragraphs: lines separated by a vertical gap
    /// larger than most of a line, or starting a new column.
    pub fn paragraphs(&self) -> Vec<(usize, usize)> {
        let mut out: Vec<(usize, usize)> = Vec::new();
        let mut prev: Option<&TextLine> = None;
        for l in &self.lines {
            let new_para = match prev {
                None => true,
                Some(p) => {
                    let h = (p.bbox[3] - p.bbox[1]).max(1.0);
                    let gap = l.bbox[1] - p.bbox[3];
                    // Lines of a clearly different size (a heading and the
                    // text under it) are paragraphs of their own.
                    let lh = (l.bbox[3] - l.bbox[1]).max(1.0);
                    let resized = lh > h * 1.2 || h > lh * 1.2;
                    gap > h * 0.7 || l.bbox[1] < p.bbox[1] - h || resized
                }
            };
            match out.last_mut() {
                Some(last) if !new_para => last.1 = l.end,
                _ => out.push((l.start, l.end)),
            }
            prev = Some(l);
        }
        if out.is_empty() && !self.text.is_empty() {
            out.push((0, self.text.chars().count()));
        }
        out
    }
    fn glyph_range(&self, start: usize, end: usize) -> Vec<usize> {
        let mut gl: Vec<usize> = self
            .char_glyph
            .get(start..end.min(self.char_glyph.len()))
            .unwrap_or(&[])
            .iter()
            .flatten()
            .copied()
            .collect();
        gl.dedup();
        gl
    }

    /// Per-line rectangles covering the character range (display space).
    pub fn range_rects(&self, start: usize, end: usize) -> Vec<[f32; 4]> {
        let mut out: Vec<[f32; 4]> = Vec::new();
        for line in &self.lines {
            let (s, e) = (start.max(line.start), end.min(line.end));
            if s >= e {
                continue;
            }
            let mut r: Option<[f32; 4]> = None;
            for gi in self.glyph_range(s, e) {
                let b = self.glyphs[gi].bbox;
                r = Some(match r {
                    Some(a) => [a[0].min(b[0]), a[1].min(b[1]), a[2].max(b[2]), a[3].max(b[3])],
                    None => b,
                });
            }
            if let Some(r) = r {
                out.push(r);
            }
        }
        out
    }

    /// Per-line quadrilaterals (upper-left, upper-right, lower-left, lower-right)
    /// covering the range, following the text direction (display space).
    pub fn range_quads(&self, start: usize, end: usize) -> Vec<[f32; 8]> {
        let mut out = Vec::new();
        for line in &self.lines {
            let (s, e) = (start.max(line.start), end.min(line.end));
            if s >= e {
                continue;
            }
            let gl = self.glyph_range(s, e);
            let (Some(first), Some(last)) = (gl.first(), gl.last()) else {
                continue;
            };
            let (a, b) = (&self.glyphs[*first].quad, &self.glyphs[*last].quad);
            out.push([a[0], a[1], b[2], b[3], a[4], a[5], b[6], b[7]]);
        }
        out
    }

    /// The same quads converted to PDF user space for annotations.
    pub fn range_quads_pdf(&self, size: &PageSize, start: usize, end: usize) -> Vec<[f32; 8]> {
        self.range_quads(start, end)
            .into_iter()
            .map(|q| {
                let mut o = [0.0; 8];
                for i in 0..4 {
                    let (x, y) = display_to_pdf(size, q[2 * i], size.height - q[2 * i + 1]);
                    o[2 * i] = x;
                    o[2 * i + 1] = y;
                }
                o
            })
            .collect()
    }

    /// Index of the character at (or nearest to) a display-space point.
    /// `None` when the page has no text.
    pub fn char_at(&self, x: f32, y: f32) -> Option<usize> {
        if self.glyphs.is_empty() {
            return None;
        }
        // Prefer the line whose box contains y, else the nearest line vertically.
        let line = self.lines.iter().min_by(|a, b| {
            let da = if y < a.bbox[1] {
                a.bbox[1] - y
            } else if y > a.bbox[3] {
                y - a.bbox[3]
            } else {
                0.0
            };
            let db = if y < b.bbox[1] {
                b.bbox[1] - y
            } else if y > b.bbox[3] {
                y - b.bbox[3]
            } else {
                0.0
            };
            let ha = if x < a.bbox[0] {
                a.bbox[0] - x
            } else if x > a.bbox[2] {
                x - a.bbox[2]
            } else {
                0.0
            };
            let hb = if x < b.bbox[0] {
                b.bbox[0] - x
            } else if x > b.bbox[2] {
                x - b.bbox[2]
            } else {
                0.0
            };
            (da * 4.0 + ha)
                .partial_cmp(&(db * 4.0 + hb))
                .unwrap_or(std::cmp::Ordering::Equal)
        })?;
        let mut best = (f32::MAX, line.start);
        for ci in line.start..line.end {
            if let Some(Some(gi)) = self.char_glyph.get(ci) {
                let b = self.glyphs[*gi].bbox;
                let cx = (b[0] + b[2]) / 2.0;
                // Left half of a glyph selects before it, right half after it.
                let d = (x - cx).abs();
                if d < best.0 {
                    best = (d, if x > cx { ci + 1 } else { ci });
                }
            }
        }
        Some(best.1.min(line.end))
    }

    /// Word boundaries around a character index.
    pub fn word_at(&self, index: usize) -> (usize, usize) {
        let chars = self.chars();
        if chars.is_empty() {
            return (0, 0);
        }
        let i = index.min(chars.len() - 1);
        if !is_word_char(chars[i]) {
            return (i, i + 1);
        }
        let mut s = i;
        while s > 0 && is_word_char(chars[s - 1]) {
            s -= 1;
        }
        let mut e = i;
        while e < chars.len() && is_word_char(chars[e]) {
            e += 1;
        }
        (s, e)
    }

    /// Line boundaries around a character index.
    pub fn line_at(&self, index: usize) -> (usize, usize) {
        self.lines
            .iter()
            .find(|l| index >= l.start && index <= l.end)
            .map(|l| (l.start, l.end))
            .unwrap_or((index, index))
    }

    /// `http(s)://` and `www.` addresses with their character ranges.
    pub fn urls(&self) -> Vec<(usize, usize, String)> {
        let chars = self.chars();
        let mut out = Vec::new();
        let mut i = 0;
        while i < chars.len() {
            let rest: String = chars[i..chars.len().min(i + 8)]
                .iter()
                .collect::<String>()
                .to_lowercase();
            let starts =
                rest.starts_with("http://") || rest.starts_with("https://") || rest.starts_with("www.");
            let boundary = i == 0 || !is_word_char(chars[i - 1]);
            if starts && boundary {
                let mut j = i;
                while j < chars.len()
                    && !chars[j].is_whitespace()
                    && !matches!(chars[j], '<' | '>' | '"' | '\'' | ')' | ']')
                {
                    j += 1;
                }
                while j > i && matches!(chars[j - 1], '.' | ',' | ';' | ':' | '!' | '?') {
                    j -= 1;
                }
                let url: String = chars[i..j].iter().collect();
                if url.len() > 7 {
                    let full = if url.to_lowercase().starts_with("www.") {
                        format!("https://{url}")
                    } else {
                        url
                    };
                    out.push((i, j, full));
                }
                i = j.max(i + 1);
            } else {
                i += 1;
            }
        }
        out
    }
}
