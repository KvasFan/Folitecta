//! Page marks: text watermarks, headers and footers, page numbering (including Bates style).

use lopdf::{Object, ObjectId, dictionary};

use crate::document::{Document, PageSize};
use crate::error::{Error, Result};
use crate::fonts::{GlyphProblem, TextWriter};
use crate::util::{self, fmt_num};

/// Name of the standard Helvetica of page marks in the resources of the pages.
const MARK_FONT: &str = "FoliHelv";

/// Mark kinds removed by "remove all page marks". Flattened comments, applied
/// redactions, placed images and overlays are page content and stay.
pub const PAGE_MARKS: &[&str] = &["Watermark", "HeaderFooter", "PageNumber", "Background"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MarkPosition {
    #[default]
    Center,
    TopLeft,
    TopCenter,
    TopRight,
    BottomLeft,
    BottomCenter,
    BottomRight,
}

impl MarkPosition {
    pub const ALL: [MarkPosition; 7] = [
        MarkPosition::Center,
        MarkPosition::TopLeft,
        MarkPosition::TopCenter,
        MarkPosition::TopRight,
        MarkPosition::BottomLeft,
        MarkPosition::BottomCenter,
        MarkPosition::BottomRight,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            MarkPosition::Center => "Center",
            MarkPosition::TopLeft => "Top left",
            MarkPosition::TopCenter => "Top center",
            MarkPosition::TopRight => "Top right",
            MarkPosition::BottomLeft => "Bottom left",
            MarkPosition::BottomCenter => "Bottom center",
            MarkPosition::BottomRight => "Bottom right",
        }
    }

    pub fn from_label(label: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.label() == label)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TextMarkOptions {
    pub text: String,
    pub font_size: f32,
    pub color: [f32; 3],
    /// 0.0 (transparent) to 1.0 (opaque).
    pub opacity: f32,
    /// Counter-clockwise rotation in degrees as seen on screen.
    pub rotation: f32,
    pub position: MarkPosition,
    pub margin: f32,
}

impl Default for TextMarkOptions {
    fn default() -> Self {
        TextMarkOptions {
            text: "DRAFT".into(),
            font_size: 72.0,
            color: [0.75, 0.75, 0.75],
            opacity: 0.4,
            rotation: 45.0,
            position: MarkPosition::Center,
            margin: 24.0,
        }
    }
}

/// Convert a point in display coordinates (origin bottom-left of the page as shown,
/// `u` rightwards, `v` upwards) into PDF user space for a rotated page.
pub fn display_to_pdf(size: &PageSize, u: f32, v: f32) -> (f32, f32) {
    let c = size.crop_box;
    let (x0, y0, x1, y1) = (c[0], c[1], c[2], c[3]);
    match size.rotation {
        90 => (x0 + (size.height - v), y0 + u),
        180 => (x1 - u, y1 - v),
        270 => (x0 + v, y1 - u),
        _ => (x0 + u, y0 + v),
    }
}

/// Inverse of [`display_to_pdf`]: PDF user space to display coordinates with the
/// origin at the top-left corner and y pointing down.
pub fn pdf_to_display(size: &PageSize, x: f32, y: f32) -> (f32, f32) {
    let c = size.crop_box;
    let (x0, y0, x1, y1) = (c[0], c[1], c[2], c[3]);
    let (u, v) = match size.rotation {
        90 => (y - y0, size.height - (x - x0)),
        180 => (x1 - x, y1 - y),
        270 => (y1 - y, x - x0),
        _ => (x - x0, y - y0),
    };
    (u, size.height - v)
}

/// PDF rectangle to a display rectangle `[x0, y0, x1, y1]` (y down).
pub fn pdf_rect_to_display(size: &PageSize, r: [f32; 4]) -> [f32; 4] {
    let (ax, ay) = pdf_to_display(size, r[0], r[1]);
    let (bx, by) = pdf_to_display(size, r[2], r[3]);
    [ax.min(bx), ay.min(by), ax.max(bx), ay.max(by)]
}

/// Display anchor (baseline start) for text of width `w` and size `s`.
fn anchor(size: &PageSize, position: MarkPosition, w: f32, s: f32, margin: f32) -> (f32, f32) {
    let (pw, ph) = (size.width, size.height);
    match position {
        MarkPosition::Center => (pw / 2.0 - w / 2.0, ph / 2.0 - s * 0.35),
        MarkPosition::TopLeft => (margin, ph - margin - s),
        MarkPosition::TopCenter => (pw / 2.0 - w / 2.0, ph - margin - s),
        MarkPosition::TopRight => (pw - margin - w, ph - margin - s),
        MarkPosition::BottomLeft => (margin, margin),
        MarkPosition::BottomCenter => (pw / 2.0 - w / 2.0, margin),
        MarkPosition::BottomRight => (pw - margin - w, margin),
    }
}

/// The drawing of one mark text, from its baseline start `(x, y)` turned by
/// `angle_deg`: the standard Helvetica for WinAnsi text (as before), else the
/// Unicode font of `writer`.
fn text_op(
    writer: &mut TextWriter,
    x: f32,
    y: f32,
    angle_deg: f32,
    font_size: f32,
    color: [f32; 3],
    gs: Option<&str>,
    text: &str,
) -> std::result::Result<String, GlyphProblem> {
    let f = fmt_num;
    let (sin, cos) = angle_deg.to_radians().sin_cos();
    let mut s = String::from("q ");
    if let Some(g) = gs {
        s.push_str(&format!("/{g} gs "));
    }
    let at = format!(
        "{} {} {} {} {} {} Tm",
        f(cos),
        f(sin),
        f(-sin),
        f(cos),
        f(x),
        f(y)
    );
    s.push_str(&writer.text_object(text, font_size, Some(color), &at, false, (MARK_FONT, MARK_FONT))?);
    s.push_str(" Q\n");
    Ok(s)
}

/// Watermarks, running headers and page numbers are artifacts: screen
/// readers leave them out.
fn pagination(content: String, subtype: Option<&str>) -> Vec<u8> {
    let props = match subtype {
        Some(s) => format!("<</Type /Pagination /Subtype /{s}>>"),
        None => "<</Type /Pagination>>".to_string(),
    };
    format!("/Artifact {props} BDC\n{content}EMC\n").into_bytes()
}

/// Region of the system locale (`IT` for `it_IT.UTF-8`), from the locale
/// environment variables; empty when they are not set.
fn locale_region() -> String {
    ["LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .filter_map(|n| std::env::var(n).ok())
        .find(|v| !v.is_empty() && v != "C" && v != "POSIX")
        .and_then(|v| {
            let v = v.split(['.', '@']).next().unwrap_or("").to_string();
            v.split(['_', '-']).nth(1).map(|r| r.to_uppercase())
        })
        .unwrap_or_default()
}

/// Macros understood in page-mark text: token and description.
pub const MACROS: &[(&str, &str)] = &[
    ("{page}", "page number (also {page:4} with leading zeros)"),
    ("{pages}", "number of pages"),
    ("{label}", "page label"),
    ("{bates:6}", "Bates number with 6 digits"),
    ("{autonumber:4}", "running number with 4 digits"),
    ("{docnumber}", "number of the document when numbering several"),
    (
        "{imagenumber}",
        "number of the image (page) in a document made from images",
    ),
    ("{srcpages}", "number of pages of the source document"),
    ("{date}", "date (YYYY-MM-DD)"),
    ("{time}", "time (HH:MM)"),
    ("{year}", "year"),
    ("{month}", "month"),
    ("{day}", "day"),
    ("{hour}", "hour"),
    ("{minute}", "minute"),
    ("{second}", "second"),
    ("{title}", "document title (or file name)"),
    ("{author}", "author"),
    ("{subject}", "subject"),
    ("{keywords}", "keywords"),
    ("{docinfo:Key}", "any entry of the document information"),
    ("{file}", "file name"),
    ("{filename}", "file name without extension"),
    ("{ext}", "file extension"),
    ("{folder}", "folder path"),
    ("{foldername}", "folder name"),
    ("{bookmark}", "title of the page's bookmark"),
    ("{computer}", "computer name"),
    ("{user}", "user name"),
    ("{env:NAME}", "environment variable"),
    ("{region}", "region of the system locale"),
    ("{app}", "application name and version"),
];

impl Document {
    /// Expand the macros of page-mark text for a page (`page_index`, numbered
    /// from `start`). See [`MACROS`] for the list.
    pub fn expand_macros(&self, template: &str, page_index: usize, start: i64) -> String {
        self.expand_macros_numbered(template, page_index, start + page_index as i64)
    }

    /// Like [`Document::expand_macros`], with the number printed by `{page}`,
    /// `{bates}` and `{autonumber}` given explicitly.
    pub fn expand_macros_numbered(&self, template: &str, page_index: usize, number: i64) -> String {
        if !template.contains('{') {
            return template.to_string();
        }
        let info = self.info();
        // UTC, like the {date} macro.
        let now = time::OffsetDateTime::now_utc();
        let file_name = self.display_name();
        let path = self.path();
        let stem = path
            .and_then(|p| p.file_stem())
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| file_name.clone());
        let ext = path
            .and_then(|p| p.extension())
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        let folder = path
            .and_then(|p| p.parent())
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        let folder_name = path
            .and_then(|p| p.parent())
            .and_then(|p| p.file_name())
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        let title = if info.title.is_empty() {
            file_name.clone()
        } else {
            info.title.clone()
        };
        let env = |names: &[&str]| {
            names
                .iter()
                .find_map(|n| std::env::var(n).ok())
                .unwrap_or_default()
        };
        let mut out = String::with_capacity(template.len() + 16);
        let mut rest = template;
        while let Some(open) = rest.find('{') {
            out.push_str(&rest[..open]);
            let after = &rest[open + 1..];
            let Some(close) = after.find('}') else {
                out.push_str(&rest[open..]);
                rest = "";
                break;
            };
            let token = &after[..close];
            let (name, arg) = match token.split_once(':') {
                Some((n, a)) => (n, Some(a)),
                None => (token, None),
            };
            let padded = |v: i64| match arg.and_then(|a| a.trim().parse::<usize>().ok()) {
                Some(w) => format!("{v:0w$}"),
                None => v.to_string(),
            };
            let two = |v: u8| format!("{v:02}");
            let value: Option<String> = match name {
                "page" | "n" => Some(padded(number)),
                "pages" | "total" => Some(self.page_count().to_string()),
                "label" => Some(
                    self.page_labels()
                        .get(page_index)
                        .cloned()
                        .unwrap_or_else(|| (page_index + 1).to_string()),
                ),
                "bates" => Some(format!(
                    "{:0w$}",
                    number,
                    w = arg.and_then(|a| a.trim().parse().ok()).unwrap_or(6)
                )),
                "autonumber" | "imagenumber" => Some(padded(number)),
                "srcpages" => Some(self.page_count().to_string()),
                // Multi-document numbering replaces it before; one document is number 1.
                "docnumber" => Some(padded(1)),
                "region" => Some(locale_region()),
                "date" => Some(util::today_display()),
                "time" => Some(format!("{}:{}", two(now.hour()), two(now.minute()))),
                "year" => Some(now.year().to_string()),
                "month" => Some(two(now.month() as u8)),
                "day" => Some(two(now.day())),
                "hour" => Some(two(now.hour())),
                "minute" => Some(two(now.minute())),
                "second" => Some(two(now.second())),
                "title" => Some(title.clone()),
                "author" => Some(info.author.clone()),
                "subject" => Some(info.subject.clone()),
                "keywords" => Some(info.keywords.clone()),
                "creator" => Some(info.creator.clone()),
                "producer" => Some(info.producer.clone()),
                "file" => Some(file_name.clone()),
                "filename" => Some(stem.clone()),
                "ext" => Some(ext.clone()),
                "folder" => Some(folder.clone()),
                "foldername" => Some(folder_name.clone()),
                "computer" => Some(env(&["COMPUTERNAME", "HOSTNAME"])),
                "user" => Some(env(&["USERNAME", "USER", "LOGNAME"])),
                "env" => arg.map(|a| std::env::var(a.trim()).unwrap_or_default()),
                "docinfo" => arg.map(|k| {
                    let key = k.trim();
                    match key.to_ascii_lowercase().as_str() {
                        "title" => info.title.clone(),
                        "author" => info.author.clone(),
                        "subject" => info.subject.clone(),
                        "keywords" => info.keywords.clone(),
                        "creator" => info.creator.clone(),
                        "producer" => info.producer.clone(),
                        _ => self.info_entry(key),
                    }
                }),
                "bookmark" => Some(self.bookmark_title_for_page(page_index)),
                "app" => Some(format!("{} {}", crate::APP_NAME, crate::VERSION)),
                _ => None,
            };
            match value {
                Some(v) => out.push_str(&v),
                // Unknown braces stay as they are.
                None => {
                    out.push('{');
                    out.push_str(token);
                    out.push('}');
                }
            }
            rest = &after[close + 1..];
        }
        out.push_str(rest);
        out
    }

    /// A custom entry of the document information dictionary.
    fn info_entry(&self, key: &str) -> String {
        let doc = &self.inner;
        doc.trailer
            .get(b"Info")
            .ok()
            .map(|i| util::deref(doc, i))
            .and_then(|i| i.as_dict().ok())
            .and_then(|d| util::dict_text(doc, d, key.as_bytes()))
            .unwrap_or_default()
    }

    /// Title of the last bookmark (in document order) that points at or before a page.
    pub fn bookmark_title_for_page(&self, page: usize) -> String {
        let mut best: Option<(usize, String)> = None;
        for (_, title, p) in self.outline_flat() {
            if let Some(p) = p {
                if p <= page && best.as_ref().map(|(bp, _)| p >= *bp).unwrap_or(true) {
                    best = Some((p, title));
                }
            }
        }
        best.map(|(_, t)| t).unwrap_or_default()
    }

    /// A writer for the texts of a mark on the pages `page_ids`: text that
    /// cannot be drawn is refused here, before anything changes. Its Unicode
    /// font gets a name that no font of those pages has (other marks of the
    /// pages keep theirs).
    fn mark_writer(&self, texts: &[String], page_ids: &[ObjectId]) -> Result<TextWriter> {
        let mut writer = TextWriter::for_pieces(texts.iter().map(String::as_str))?;
        if !writer.is_standard() {
            let mut taken = std::collections::HashSet::new();
            for &id in page_ids {
                let res = crate::page_text::probe::page_resources(&self.inner, id);
                if let Some(fonts) = res.get(b"Font").ok().map(|f| util::deref(&self.inner, f)) {
                    if let Ok(fonts) = fonts.as_dict() {
                        taken.extend(fonts.iter().map(|(k, _)| k.clone()));
                    }
                }
            }
            let name = (1..)
                .map(|n| format!("FoliUni{n}"))
                .find(|n| !taken.contains(n.as_bytes()))
                .unwrap_or_else(|| "FoliUni".into());
            writer.use_resource_name(&name);
        }
        Ok(writer)
    }

    /// The Unicode font of a mark, once every text is drawn: added to the
    /// resources of the pages it was drawn on (the standard Helvetica is
    /// added page by page).
    fn finish_mark_font(&mut self, writer: TextWriter, page_ids: &[ObjectId]) -> Result<()> {
        if writer.is_standard() {
            return Ok(());
        }
        let name = writer.resource_name();
        let font = writer.finish(self, None)?;
        for &id in page_ids {
            self.add_page_resource(id, "Font", &name, Object::Reference(font))?;
        }
        Ok(())
    }

    fn prepare_mark_resources(
        &mut self,
        page_id: ObjectId,
        opacity: f32,
        standard: bool,
    ) -> Result<Option<String>> {
        if standard {
            let font = self.helvetica_font_id();
            self.add_page_resource(page_id, "Font", MARK_FONT, Object::Reference(font))?;
        }
        if opacity < 0.999 {
            let name = format!("FoliGS{}", (opacity.clamp(0.0, 1.0) * 100.0).round() as i32);
            let gs = dictionary! { "Type" => "ExtGState", "CA" => opacity, "ca" => opacity };
            self.add_page_resource(page_id, "ExtGState", &name, Object::Dictionary(gs))?;
            Ok(Some(name))
        } else {
            Ok(None)
        }
    }

    /// Stamp a text watermark on the given pages.
    pub fn add_text_watermark(&mut self, pages: &[usize], opts: &TextMarkOptions) -> Result<()> {
        if opts.text.trim().is_empty() {
            return Err(Error::Invalid("watermark text is empty".into()));
        }
        let ids = self.page_ids();
        for &p in pages {
            if p >= ids.len() {
                return Err(Error::InvalidPage(p));
            }
        }
        // The texts first: macros read the document before it changes.
        let texts: Vec<String> = pages
            .iter()
            .map(|&p| self.expand_macros(&opts.text, p, 1))
            .collect();
        let targets: Vec<ObjectId> = pages.iter().map(|&p| ids[p]).collect();
        let mut writer = self.mark_writer(&texts, &targets)?;
        self.begin_edit();
        let drawn = (|| -> Result<()> {
            for (&p, text) in pages.iter().zip(&texts) {
                self.add_watermark_to(&mut writer, ids[p], text, opts)?;
            }
            Ok(())
        })();
        let drawn = drawn.and_then(|()| self.finish_mark_font(writer, &targets));
        if let Err(e) = drawn {
            self.abandon_edit();
            return Err(e);
        }
        self.commit()
    }

    /// One text watermark on page `id`.
    fn add_watermark_to(
        &mut self,
        writer: &mut TextWriter,
        id: ObjectId,
        text: &str,
        opts: &TextMarkOptions,
    ) -> Result<()> {
        let size = self.page_size_of(id);
        let gs = self.prepare_mark_resources(id, opts.opacity, writer.is_standard())?;
        let text = writer.piece(text);
        let w = writer.width(&text, opts.font_size);
        let (u, v) = if opts.position == MarkPosition::Center && opts.rotation.abs() > 0.1 {
            // rotate around the page centre
            let (cx, cy) = (size.width / 2.0, size.height / 2.0);
            let (sin, cos) = opts.rotation.to_radians().sin_cos();
            let (ox, oy) = (-w / 2.0, -opts.font_size * 0.35);
            (cx + ox * cos - oy * sin, cy + ox * sin + oy * cos)
        } else {
            anchor(&size, opts.position, w, opts.font_size, opts.margin)
        };
        let (x, y) = display_to_pdf(&size, u, v);
        let angle = opts.rotation + size.rotation as f32;
        let content = text_op(
            writer,
            x,
            y,
            angle,
            opts.font_size,
            opts.color,
            gs.as_deref(),
            &text,
        )?;
        self.append_page_content(id, pagination(content, Some("Watermark")), Some("Watermark"))?;
        Ok(())
    }

    /// Add header (left/center/right) and footer (left/center/right) texts. Empty
    /// strings are skipped. Placeholders are expanded per page.
    pub fn add_header_footer(
        &mut self,
        pages: &[usize],
        header: [&str; 3],
        footer: [&str; 3],
        font_size: f32,
        margin: f32,
        color: [f32; 3],
    ) -> Result<()> {
        let ids = self.page_ids();
        for &p in pages {
            if p >= ids.len() {
                return Err(Error::InvalidPage(p));
            }
        }
        if header.iter().chain(footer.iter()).all(|s| s.trim().is_empty()) {
            return Err(Error::Invalid("header and footer are empty".into()));
        }
        let slots = [
            (header[0], MarkPosition::TopLeft),
            (header[1], MarkPosition::TopCenter),
            (header[2], MarkPosition::TopRight),
            (footer[0], MarkPosition::BottomLeft),
            (footer[1], MarkPosition::BottomCenter),
            (footer[2], MarkPosition::BottomRight),
        ];
        // The texts first: macros read the document before it changes.
        let texts: Vec<Vec<(String, MarkPosition)>> = pages
            .iter()
            .map(|&p| {
                slots
                    .iter()
                    .filter(|(template, _)| !template.trim().is_empty())
                    .map(|(template, pos)| (self.expand_macros(template, p, 1), *pos))
                    .collect()
            })
            .collect();
        let all: Vec<String> = texts.iter().flatten().map(|(t, _)| t.clone()).collect();
        let targets: Vec<ObjectId> = pages.iter().map(|&p| ids[p]).collect();
        let mut writer = self.mark_writer(&all, &targets)?;
        self.begin_edit();
        let drawn = (|| -> Result<()> {
            for (&id, page_texts) in targets.iter().zip(&texts) {
                let size = self.page_size_of(id);
                self.prepare_mark_resources(id, 1.0, writer.is_standard())?;
                let mut content = String::new();
                for (text, pos) in page_texts {
                    let text = writer.piece(text);
                    let w = writer.width(&text, font_size);
                    let (u, v) = anchor(&size, *pos, w, font_size, margin);
                    let (x, y) = display_to_pdf(&size, u, v);
                    content.push_str(&text_op(
                        &mut writer,
                        x,
                        y,
                        size.rotation as f32,
                        font_size,
                        color,
                        None,
                        &text,
                    )?);
                }
                self.append_page_content(id, pagination(content, None), Some("HeaderFooter"))?;
            }
            Ok(())
        })();
        let drawn = drawn.and_then(|()| self.finish_mark_font(writer, &targets));
        if let Err(e) = drawn {
            self.abandon_edit();
            return Err(e);
        }
        self.commit()
    }

    /// Add page numbers using `template` (for example `"Page {page} of {pages}"` or
    /// `"ABC-{bates:6}"`). `start` is the number printed on the first selected page.
    pub fn add_page_numbers(
        &mut self,
        pages: &[usize],
        template: &str,
        start: i64,
        position: MarkPosition,
        font_size: f32,
        margin: f32,
        color: [f32; 3],
    ) -> Result<()> {
        let ids = self.page_ids();
        for &p in pages {
            if p >= ids.len() {
                return Err(Error::InvalidPage(p));
            }
        }
        if template.trim().is_empty() {
            return Err(Error::Invalid("page number format is empty".into()));
        }
        // The texts first: macros read the document before it changes. Numbers
        // are relative to the selection so Bates numbering is contiguous.
        let texts: Vec<String> = pages
            .iter()
            .enumerate()
            .map(|(ordinal, &p)| self.expand_macros_numbered(template, p, start + ordinal as i64))
            .collect();
        let targets: Vec<ObjectId> = pages.iter().map(|&p| ids[p]).collect();
        let mut writer = self.mark_writer(&texts, &targets)?;
        self.begin_edit();
        let drawn = (|| -> Result<()> {
            for (&id, text) in targets.iter().zip(&texts) {
                let size = self.page_size_of(id);
                self.prepare_mark_resources(id, 1.0, writer.is_standard())?;
                let text = writer.piece(text);
                let w = writer.width(&text, font_size);
                let (u, v) = anchor(&size, position, w, font_size, margin);
                let (x, y) = display_to_pdf(&size, u, v);
                let content = text_op(
                    &mut writer,
                    x,
                    y,
                    size.rotation as f32,
                    font_size,
                    color,
                    None,
                    &text,
                )?;
                self.append_page_content(id, pagination(content, None), Some("PageNumber"))?;
            }
            Ok(())
        })();
        let drawn = drawn.and_then(|()| self.finish_mark_font(writer, &targets));
        if let Err(e) = drawn {
            self.abandon_edit();
            return Err(e);
        }
        self.commit()
    }

    /// Remove marks previously added by Folitecta. `tag` restricts the kind
    /// ("Watermark", "HeaderFooter", "PageNumber", "Background", "Image",
    /// "Overlay", "Underlay"); `None` removes the page marks listed in
    /// [`PAGE_MARKS`].
    pub fn remove_marks(&mut self, pages: &[usize], tag: Option<&str>) -> Result<usize> {
        match tag {
            Some(t) => self.remove_marks_of(pages, &[t]),
            None => self.remove_marks_of(pages, PAGE_MARKS),
        }
    }

    /// Remove the content streams Folitecta added with any of the given tags.
    pub fn remove_marks_of(&mut self, pages: &[usize], tags: &[&str]) -> Result<usize> {
        let ids = self.page_ids();
        for &p in pages {
            if p >= ids.len() {
                return Err(Error::InvalidPage(p));
            }
        }
        self.begin_edit();
        let removed = self.strip_marks(pages, tags)?;
        self.commit()?;
        Ok(removed)
    }

    /// [`Document::remove_marks_of`] as part of the edit under way (no undo
    /// step of its own).
    pub(crate) fn strip_marks(&mut self, pages: &[usize], tags: &[&str]) -> Result<usize> {
        let ids = self.page_ids();
        let mut removed = 0;
        for &p in pages {
            let id = *ids.get(p).ok_or(Error::InvalidPage(p))?;
            let contents = self.page_content_ids(id);
            let mut keep = Vec::new();
            for cid in contents {
                let is_mark = self
                    .inner
                    .get_object(cid)
                    .ok()
                    .and_then(|o| o.as_stream().ok())
                    .and_then(|s| s.dict.get(b"FolitectaMark").ok())
                    .and_then(|m| m.as_name().ok())
                    .map(|m| tags.iter().any(|t| t.as_bytes() == m))
                    .unwrap_or(false);
                if is_mark {
                    removed += 1;
                    self.inner.delete_object(cid);
                } else {
                    keep.push(Object::Reference(cid));
                }
            }
            let page = self.inner.get_dictionary_mut(id)?;
            match keep.len() {
                0 => {
                    page.remove(b"Contents");
                }
                1 => page.set("Contents", keep.remove(0)),
                _ => page.set("Contents", Object::Array(keep)),
            }
        }
        Ok(removed)
    }

    /// Number of mark streams present on a page, by tag.
    pub fn marks_on_page(&self, page: usize) -> Vec<String> {
        let Ok(id) = self.page_id(page) else {
            return Vec::new();
        };
        self.page_content_ids(id)
            .into_iter()
            .filter_map(|cid| {
                self.inner
                    .get_object(cid)
                    .ok()
                    .and_then(|o| o.as_stream().ok())
                    .and_then(|s| s.dict.get(b"FolitectaMark").ok())
                    .and_then(|m| m.as_name().ok())
                    .map(|m| String::from_utf8_lossy(m).to_string())
            })
            .collect()
    }
}
