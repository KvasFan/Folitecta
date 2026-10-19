//! Comment summaries: a new PDF document that lists the comments of a document,
//! alone or next to the pages they belong to.

use std::collections::{HashMap, HashSet};

use lopdf::{Dictionary, Object, ObjectId, Stream, dictionary};

use crate::document::Document;
use crate::error::{Error, Result};
use crate::flatten::FlattenScope;
use crate::textdoc::wrap_text;
use crate::util::{self, color_of, dict_text, fmt_num, helvetica_width, pdf_literal};

/// How the summary is laid out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SummaryLayout {
    /// Only the list of comments.
    CommentsOnly,
    /// Each page, with numbered markers, followed by a page listing its comments.
    SeparatePages,
    /// Each page next to the list of its comments, joined by connector lines.
    ConnectorLines,
}

/// Order of the comments in the summary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SummarySort {
    Page,
    Author,
    Date,
    Type,
}

/// Texts used in the summary (localized by the caller).
#[derive(Debug, Clone, PartialEq)]
pub struct SummaryLabels {
    /// Title of the summary; `{file}` is replaced by the document name.
    pub title: String,
    /// Word for "Page".
    pub page: String,
    /// Shown for a page without comments in the side-by-side layout.
    pub no_comments: String,
    /// Display names of comment subtypes.
    pub types: Vec<(String, String)>,
}

impl Default for SummaryLabels {
    fn default() -> Self {
        SummaryLabels {
            title: "Summary of comments: {file}".into(),
            page: "Page".into(),
            no_comments: "No comments".into(),
            types: Vec::new(),
        }
    }
}

/// Options of a comment summary.
#[derive(Debug, Clone, PartialEq)]
pub struct SummaryOptions {
    pub layout: SummaryLayout,
    pub sort: SummarySort,
    /// Text size of the comment entries (points).
    pub font_size: f32,
    /// Include hidden comments.
    pub include_hidden: bool,
    /// Include the comments of hidden layers.
    pub include_hidden_layers: bool,
    /// The visibility of the layers where it differs from the default one
    /// (the view the summary is made from).
    pub layer_view: HashMap<ObjectId, bool>,
    /// Paper size of the comment pages of the comments-only layout (points).
    pub paper: (f32, f32),
    pub labels: SummaryLabels,
}

impl Default for SummaryOptions {
    fn default() -> Self {
        SummaryOptions {
            layout: SummaryLayout::CommentsOnly,
            sort: SummarySort::Page,
            font_size: 10.0,
            include_hidden: false,
            include_hidden_layers: false,
            layer_view: HashMap::new(),
            paper: (595.28, 841.89),
            labels: SummaryLabels::default(),
        }
    }
}

/// One comment of the summary.
#[derive(Debug, Clone, PartialEq)]
pub struct SummaryEntry {
    pub page: usize,
    /// Sequence number shown in the summary and on the page markers.
    pub number: usize,
    pub subtype: String,
    pub author: String,
    /// Raw modification date (`D:...`), used for sorting.
    pub date_raw: String,
    pub date: String,
    pub contents: String,
    pub color: [f32; 3],
    pub rect: [f32; 4],
    /// Replies and review states: (author, date, text).
    pub replies: Vec<(String, String, String)>,
}

const MARGIN: f32 = 42.0;

fn name_str(d: &Dictionary, key: &[u8]) -> Option<String> {
    d.get(key)
        .ok()
        .and_then(|n| n.as_name().ok())
        .map(|n| String::from_utf8_lossy(n).to_string())
}

fn bold_width(text: &str, size: f32) -> f32 {
    helvetica_width(text, size) * 1.07
}

fn lit(text: &str) -> String {
    String::from_utf8_lossy(&pdf_literal(text)).to_string()
}

fn rgb(c: [f32; 3], op: &str) -> String {
    format!("{} {} {} {op} ", fmt_num(c[0]), fmt_num(c[1]), fmt_num(c[2]))
}

/// Shorten `text` with an ellipsis so it fits `width` at `size`.
fn fit(text: &str, size: f32, width: f32, bold: bool) -> String {
    let w = |s: &str| {
        if bold {
            bold_width(s, size)
        } else {
            helvetica_width(s, size)
        }
    };
    if w(text) <= width {
        return text.to_string();
    }
    let mut s: String = text.to_string();
    while !s.is_empty() && w(&format!("{s}…")) > width {
        s.pop();
    }
    format!("{s}…")
}

/// Matrix placing a page (crop box `c`, rotation `rot`) at `(ox, oy)` with scale `s`,
/// upright as it is displayed.
fn place_matrix(c: [f32; 4], rot: i32, s: f32, ox: f32, oy: f32) -> [f32; 6] {
    match rot {
        90 => [0.0, -s, s, 0.0, ox - s * c[1], oy + s * c[2]],
        180 => [-s, 0.0, 0.0, -s, ox + s * c[2], oy + s * c[3]],
        270 => [0.0, s, -s, 0.0, ox + s * c[3], oy - s * c[0]],
        _ => [s, 0.0, 0.0, s, ox - s * c[0], oy - s * c[1]],
    }
}

fn apply(m: [f32; 6], x: f32, y: f32) -> (f32, f32) {
    (m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5])
}

/// Content of a numbered round marker centred at `(x, y)`, upright on a page rotated by `rot`.
fn marker(x: f32, y: f32, n: usize, color: [f32; 3], rot: i32) -> String {
    let f = fmt_num;
    let r = 7.0;
    let k = 0.5523 * r;
    let label = n.to_string();
    let fs = if label.len() > 2 { 6.0 } else { 7.5 };
    let tw = bold_width(&label, fs);
    let (sin, cos) = match rot {
        90 => (1.0, 0.0),
        180 => (0.0, -1.0),
        270 => (-1.0, 0.0),
        _ => (0.0, 1.0),
    };
    // Text origin relative to the centre, rotated with the page.
    let (dx, dy) = (-tw / 2.0, -fs * 0.36);
    let (tx, ty) = (x + dx * cos - dy * sin, y + dx * sin + dy * cos);
    // Dark numbers on light colours, white numbers on dark ones.
    let luminance = 0.299 * color[0] + 0.587 * color[1] + 0.114 * color[2];
    let ink = if luminance > 0.6 {
        "0.1 0.1 0.12 rg"
    } else {
        "1 1 1 rg"
    };
    format!(
        "q {}0.6 w 1 1 1 RG {} {} m {} {} {} {} {} {} c {} {} {} {} {} {} c {} {} {} {} {} {} c {} {} {} {} {} {} c b \
         BT /FB {} Tf {ink} {} {} {} {} {} {} Tm {} Tj ET Q\n",
        rgb(color, "rg"),
        f(x + r),
        f(y),
        f(x + r),
        f(y + k),
        f(x + k),
        f(y + r),
        f(x),
        f(y + r),
        f(x - k),
        f(y + r),
        f(x - r),
        f(y + k),
        f(x - r),
        f(y),
        f(x - r),
        f(y - k),
        f(x - k),
        f(y - r),
        f(x),
        f(y - r),
        f(x + k),
        f(y - r),
        f(x + r),
        f(y - k),
        f(x + r),
        f(y),
        f(fs),
        f(cos),
        f(sin),
        f(-sin),
        f(cos),
        f(tx),
        f(ty),
        lit(&label)
    )
}

/// Text pages written top to bottom.
struct Flow {
    pages: Vec<String>,
    x0: f32,
    x1: f32,
    top: f32,
    bottom: f32,
    y: f32,
}

impl Flow {
    fn new(x0: f32, x1: f32, top: f32, bottom: f32) -> Self {
        Flow {
            pages: vec![String::new()],
            x0,
            x1,
            top,
            bottom,
            y: top,
        }
    }

    fn width(&self) -> f32 {
        self.x1 - self.x0
    }

    fn new_page(&mut self) {
        self.pages.push(String::new());
        self.y = self.top;
    }

    /// Start a new page unless `h` points fit below the cursor.
    fn need(&mut self, h: f32) {
        if self.y - h < self.bottom && self.y < self.top - 1.0 {
            self.new_page();
        }
    }

    fn out(&mut self) -> &mut String {
        self.pages.last_mut().expect("a page")
    }

    fn line(&mut self, x: f32, font: &str, size: f32, color: [f32; 3], text: &str) {
        let y = self.y - size;
        let s = format!(
            "BT /{font} {} Tf {}{} {} Td {} Tj ET\n",
            fmt_num(size),
            rgb(color, "rg"),
            fmt_num(x),
            fmt_num(y),
            lit(text)
        );
        self.out().push_str(&s);
        self.y -= size * 1.25;
    }

    fn rule(&mut self, color: [f32; 3]) {
        let s = format!(
            "{}0.5 w {} {} m {} {} l S\n",
            rgb(color, "RG"),
            fmt_num(self.x0),
            fmt_num(self.y),
            fmt_num(self.x1),
            fmt_num(self.y)
        );
        self.out().push_str(&s);
    }

    fn heading(&mut self, text: &str, size: f32) {
        self.need(size * 2.2);
        let w = self.width();
        self.line(self.x0, "FB", size, [0.1, 0.1, 0.12], &fit(text, size, w, true));
        self.y += size * 0.1;
        self.rule([0.6, 0.6, 0.65]);
        self.y -= size * 0.5;
    }

    /// Write a comment entry; returns the page index and the y of its first line.
    fn entry(
        &mut self,
        e: &SummaryEntry,
        type_name: &str,
        page_word: &str,
        show_page: bool,
        fs: f32,
    ) -> (usize, f32) {
        let sub = fs * 0.92;
        let indent = fs * 1.6;
        let text_w = self.width() - indent;
        let mut lines = wrap_text(&e.contents, fs, text_w);
        while lines.last().map(|l| l.is_empty()).unwrap_or(false) {
            lines.pop();
        }
        let first = fs * 1.4 + lines.first().map(|_| fs * 1.25).unwrap_or(0.0);
        self.need(first + 4.0);
        let page_index = self.pages.len() - 1;
        let start = self.y;
        // Colour swatch and header.
        let sw = fs * 0.8;
        let s = format!(
            "{}{} {} {} {} re f\n",
            rgb(e.color, "rg"),
            fmt_num(self.x0),
            fmt_num(self.y - fs * 0.85),
            fmt_num(sw),
            fmt_num(sw)
        );
        self.out().push_str(&s);
        let mut header = format!("{}. {}", e.number, type_name);
        if show_page {
            header.push_str(&format!(" · {} {}", page_word, e.page + 1));
        }
        if !e.author.is_empty() {
            header.push_str(&format!(" · {}", e.author));
        }
        if !e.date.is_empty() {
            header.push_str(&format!(" · {}", e.date));
        }
        let header = fit(&header, fs, text_w, true);
        self.line(self.x0 + indent, "FB", fs, [0.1, 0.1, 0.12], &header);
        for l in &lines {
            self.need(fs * 1.25);
            self.line(self.x0 + indent, "F1", fs, [0.15, 0.15, 0.18], l);
        }
        for (author, date, text) in &e.replies {
            let head = if date.is_empty() {
                format!("» {author}: ")
            } else {
                format!("» {author} ({date}): ")
            };
            for (i, l) in wrap_text(&format!("{head}{text}"), sub, text_w - indent)
                .iter()
                .enumerate()
            {
                self.need(sub * 1.25);
                let x = self.x0 + indent * if i == 0 { 2.0 } else { 2.6 };
                self.line(x, "F1", sub, [0.35, 0.35, 0.4], l);
            }
        }
        self.y -= fs * 0.6;
        (page_index, start)
    }
}

impl Document {
    /// The comments of the document as summary entries, numbered in summary order.
    pub fn summary_entries(&self, include_hidden: bool, sort: SummarySort) -> Vec<SummaryEntry> {
        self.summary_entries_except(include_hidden, &HashSet::new(), sort)
    }

    /// [`Document::summary_entries`] without the comments `left_out` (the
    /// comments of hidden layers, with their replies and states).
    pub fn summary_entries_except(
        &self,
        include_hidden: bool,
        left_out: &HashSet<ObjectId>,
        sort: SummarySort,
    ) -> Vec<SummaryEntry> {
        let doc = &self.inner;
        let mut entries: Vec<(ObjectId, SummaryEntry)> = Vec::new();
        let mut replies: Vec<(ObjectId, (String, String, String, String))> = Vec::new();
        for a in self.comments() {
            let Some(id) = a.id else { continue };
            if left_out.contains(&id) {
                continue;
            }
            let Ok(d) = doc.get_dictionary(id) else {
                continue;
            };
            let flags = d.get(b"F").ok().and_then(|f| f.as_i64().ok()).unwrap_or(0);
            let irt = d.get(b"IRT").ok().and_then(|r| r.as_reference().ok());
            let group = name_str(d, b"RT").as_deref() == Some("Group");
            // Review states are hidden annotations, but belong to their comment.
            if flags & 2 != 0 && !include_hidden && (irt.is_none() || group) {
                continue;
            }
            let date_raw = dict_text(doc, d, b"M")
                .or_else(|| dict_text(doc, d, b"CreationDate"))
                .unwrap_or_default();
            if let (Some(parent), false) = (irt, group) {
                // Replies and review states are listed under their comment.
                let state = dict_text(doc, d, b"State").unwrap_or_default();
                let text = if !state.is_empty() && a.contents.trim().is_empty() {
                    state
                } else if !state.is_empty() {
                    format!("{} [{}]", a.contents, state)
                } else {
                    a.contents.clone()
                };
                replies.push((
                    parent,
                    (
                        date_raw.clone(),
                        a.author.clone(),
                        util::pdf_date_display(&date_raw),
                        text,
                    ),
                ));
                continue;
            }
            entries.push((
                id,
                SummaryEntry {
                    page: a.page,
                    number: 0,
                    subtype: a.subtype.clone(),
                    author: a.author.clone(),
                    date: if date_raw.is_empty() {
                        String::new()
                    } else {
                        util::pdf_date_display(&date_raw)
                    },
                    date_raw,
                    contents: a.contents.clone(),
                    color: d
                        .get(b"C")
                        .ok()
                        .and_then(|c| color_of(doc, c))
                        .or_else(|| d.get(b"IC").ok().and_then(|c| color_of(doc, c)))
                        .unwrap_or([0.5, 0.5, 0.55]),
                    rect: a.rect,
                    replies: Vec::new(),
                },
            ));
        }
        replies.sort_by(|a, b| a.1.0.cmp(&b.1.0));
        let index: HashMap<ObjectId, usize> =
            entries.iter().enumerate().map(|(i, (id, _))| (*id, i)).collect();
        for (parent, (_, author, date, text)) in replies {
            if let Some(&i) = index.get(&parent) {
                entries[i].1.replies.push((author, date, text));
            }
        }
        let mut list: Vec<SummaryEntry> = entries.into_iter().map(|(_, e)| e).collect();
        // Reading order on the page: top to bottom, then left to right.
        let reading = |a: &SummaryEntry, b: &SummaryEntry| {
            a.page
                .cmp(&b.page)
                .then(b.rect[3].total_cmp(&a.rect[3]))
                .then(a.rect[0].total_cmp(&b.rect[0]))
        };
        list.sort_by(|a, b| match sort {
            SummarySort::Page => reading(a, b),
            SummarySort::Author => a
                .author
                .to_lowercase()
                .cmp(&b.author.to_lowercase())
                .then(reading(a, b)),
            SummarySort::Date => a.date_raw.cmp(&b.date_raw).then(reading(a, b)),
            SummarySort::Type => a.subtype.cmp(&b.subtype).then(reading(a, b)),
        });
        for (i, e) in list.iter_mut().enumerate() {
            e.number = i + 1;
        }
        list
    }

    /// Build a new document summarizing the comments (protected like this
    /// document when it is).
    pub fn summarize_comments(&self, opts: &SummaryOptions) -> Result<Document> {
        // The comments of the layers the view hides are left out, unless asked for.
        let left_out = if opts.include_hidden_layers {
            HashSet::new()
        } else {
            self.annotations_in_hidden_layers(&opts.layer_view)
        };
        let mut entries = self.summary_entries_except(opts.include_hidden, &left_out, opts.sort);
        if entries.is_empty() {
            return Err(Error::Invalid("the document has no comments to summarize".into()));
        }
        let types: HashMap<&str, &str> = opts
            .labels
            .types
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        let type_name = |s: &str| {
            types
                .get(s)
                .map(|v| v.to_string())
                .unwrap_or_else(|| s.to_string())
        };
        let file = self.display_name();
        let title = opts.labels.title.replace("{file}", &file);
        let fs = opts.font_size.clamp(6.0, 18.0);
        let mut out = match opts.layout {
            SummaryLayout::CommentsOnly => {
                let (w, h) = (opts.paper.0.max(200.0), opts.paper.1.max(200.0));
                let mut flow = Flow::new(MARGIN, w - MARGIN, h - MARGIN, MARGIN);
                flow.heading(&title, fs * 1.5);
                flow.y -= fs * 0.4;
                let mut group = String::new();
                for e in &entries {
                    let key = match opts.sort {
                        SummarySort::Page => format!("{} {}", opts.labels.page, e.page + 1),
                        SummarySort::Author => e.author.clone(),
                        SummarySort::Type => type_name(&e.subtype),
                        SummarySort::Date => e.date.split(' ').next().unwrap_or_default().to_string(),
                    };
                    if key != group {
                        flow.heading(&key, fs * 1.15);
                        group = key;
                    }
                    flow.entry(
                        e,
                        &type_name(&e.subtype),
                        &opts.labels.page,
                        opts.sort != SummarySort::Page,
                        fs,
                    );
                }
                let mut doc = Document::new_blank(flow.pages.len(), w, h)?;
                let ids = doc.page_ids();
                for (id, content) in ids.iter().zip(flow.pages) {
                    doc.write_summary_page(*id, content)?;
                }
                doc
            }
            SummaryLayout::SeparatePages | SummaryLayout::ConnectorLines => {
                let mut doc = Document::from_bytes(self.bytes().to_vec(), None, None)?;
                let n = doc.page_count();
                // Comments left out are not drawn on the pages either.
                if !left_out.is_empty() {
                    for pid in doc.page_ids() {
                        doc.remove_annotations(pid, &left_out);
                    }
                }
                doc.flatten_internal(&(0..n).collect::<Vec<_>>(), FlattenScope::Comments, None)?;
                // Group entries by page, keeping the chosen order inside each page.
                entries.sort_by_key(|e| e.page);
                let mut by_page: Vec<Vec<SummaryEntry>> = vec![Vec::new(); n];
                for e in entries {
                    if e.page < n {
                        by_page[e.page].push(e);
                    }
                }
                let ids = doc.page_ids();
                let mut order = Vec::new();
                for (p, list) in by_page.iter().enumerate() {
                    let page_id = ids[p];
                    if opts.layout == SummaryLayout::SeparatePages {
                        order.push(page_id);
                        if list.is_empty() {
                            continue;
                        }
                        let size = doc.page_size_of(page_id);
                        let mut marks = String::new();
                        for e in list {
                            marks.push_str(&marker(e.rect[0], e.rect[3], e.number, e.color, size.rotation));
                        }
                        doc.add_summary_fonts(page_id)?;
                        doc.append_page_content(page_id, marks.into_bytes(), Some("Summary"))?;
                        let (w, h) = (size.width.max(300.0), size.height.max(300.0));
                        let mut flow = Flow::new(MARGIN, w - MARGIN, h - MARGIN, MARGIN);
                        flow.heading(&format!("{} {}", opts.labels.page, p + 1), fs * 1.3);
                        for e in list {
                            flow.entry(e, &type_name(&e.subtype), &opts.labels.page, false, fs);
                        }
                        for content in flow.pages {
                            let id = doc.new_summary_page(w, h)?;
                            doc.write_summary_page(id, content)?;
                            order.push(id);
                        }
                    } else {
                        order.extend(doc.side_by_side_page(page_id, list, &type_name, &opts.labels, fs)?);
                    }
                }
                doc.set_page_order(order)?;
                doc
            }
        };
        out.set_path(None);
        let _ = out.set_info(&title, "", "", "");
        out.commit()?;
        out.clear_history();
        // It quotes the comments: protected like the document.
        out.protect_like(self);
        Ok(out)
    }

    /// Fonts used by summary content (`/F1` Helvetica, `/FB` Helvetica-Bold).
    fn add_summary_fonts(&mut self, page_id: ObjectId) -> Result<()> {
        let regular = self.helvetica_font_id();
        let bold = self.inner.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica-Bold",
            "Encoding" => "WinAnsiEncoding",
        });
        self.add_page_resource(page_id, "Font", "F1", Object::Reference(regular))?;
        self.add_page_resource(page_id, "Font", "FB", Object::Reference(bold))
    }

    fn new_summary_page(&mut self, w: f32, h: f32) -> Result<ObjectId> {
        let content = self.inner.add_object(Stream::new(dictionary! {}, Vec::new()));
        let pages = self.inner.catalog()?.get(b"Pages")?.as_reference()?;
        Ok(self.inner.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages,
            "MediaBox" => vec![0.into(), 0.into(), w.into(), h.into()],
            "Contents" => content,
            "Resources" => dictionary! {},
        }))
    }

    fn write_summary_page(&mut self, page_id: ObjectId, content: String) -> Result<()> {
        self.add_summary_fonts(page_id)?;
        self.append_page_content(page_id, content.into_bytes(), Some("Summary"))?;
        Ok(())
    }

    /// Replace a page by a wider page showing it on the left and its comments on
    /// the right, joined by connector lines. Returns the new page ids (more than
    /// one when the comments need continuation pages).
    fn side_by_side_page(
        &mut self,
        page_id: ObjectId,
        list: &[SummaryEntry],
        type_name: &dyn Fn(&str) -> String,
        labels: &SummaryLabels,
        fs: f32,
    ) -> Result<Vec<ObjectId>> {
        let size = self.page_size_of(page_id);
        let (pw, ph) = (size.width, size.height);
        let panel = (pw * 0.75).clamp(220.0, 480.0);
        let (w, h) = (pw + panel, ph);
        // The page as a form XObject.
        let content = self.inner.get_page_content(page_id);
        let resources = self
            .inherited_attr(page_id, b"Resources")
            .unwrap_or_else(|| Object::Dictionary(Dictionary::new()));
        let resources = match resources {
            Object::Reference(r) => self
                .inner
                .get_object(r)
                .cloned()
                .unwrap_or(Object::Dictionary(Dictionary::new())),
            other => other,
        };
        let c = size.crop_box;
        let form = self.inner.add_object(Stream::new(
            dictionary! {
                "Type" => "XObject",
                "Subtype" => "Form",
                "BBox" => c.iter().map(|v| Object::from(*v)).collect::<Vec<_>>(),
                "Resources" => resources,
            },
            content,
        ));
        let m = place_matrix(c, size.rotation, 1.0, 0.0, 0.0);
        let f = fmt_num;
        let mut body = format!(
            "q 0.97 0.97 0.98 rg {} 0 {} {} re f Q\nq {} {} {} {} {} {} cm /SumPage Do Q\nq 0.7 0.7 0.75 RG 0.5 w {} 0 m {} {} l S Q\n",
            f(pw),
            f(panel),
            f(h),
            f(m[0]),
            f(m[1]),
            f(m[2]),
            f(m[3]),
            f(m[4]),
            f(m[5]),
            f(pw),
            f(pw),
            f(h)
        );
        let mut flow = Flow::new(pw + 16.0, w - 16.0, h - 24.0, 24.0);
        flow.heading(
            &format!(
                "{} {}",
                labels.page,
                list.first().map(|e| e.page + 1).unwrap_or(0)
            ),
            fs * 1.2,
        );
        if list.is_empty() {
            let gray = [0.45, 0.45, 0.5];
            flow.line(flow.x0, "F1", fs, gray, &labels.no_comments);
        }
        let mut connectors = Vec::new();
        for e in list {
            let (page_index, y) = flow.entry(e, &type_name(&e.subtype), &labels.page, false, fs);
            if page_index == 0 {
                let (ax, ay) = apply(m, e.rect[0], e.rect[3]);
                connectors.push((ax, ay, pw + 16.0, y - fs * 0.45, e.color, e.number));
            }
        }
        for (ax, ay, bx, by, color, n) in &connectors {
            body.push_str(&format!(
                "q {}0.6 w [2 2] 0 d {} {} m {} {} l S Q\n",
                rgb(*color, "RG"),
                f(*ax),
                f(*ay),
                f(*bx - 2.0),
                f(*by)
            ));
            body.push_str(&marker(*ax, *ay, *n, *color, 0));
        }
        let mut pages_out = Vec::new();
        for (i, text) in flow.pages.into_iter().enumerate() {
            let id = self.new_summary_page(w, h)?;
            self.add_summary_fonts(id)?;
            self.add_page_resource(id, "XObject", "SumPage", Object::Reference(form))?;
            let mut content = String::new();
            if i == 0 {
                content.push_str(&body);
            }
            content.push_str(&text);
            self.append_page_content(id, content.into_bytes(), Some("Summary"))?;
            pages_out.push(id);
        }
        Ok(pages_out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placement_matrices_keep_pages_upright() {
        let c = [0.0, 0.0, 100.0, 200.0];
        // Top-left corner of the unrotated page.
        assert_eq!(apply(place_matrix(c, 0, 1.0, 0.0, 0.0), 0.0, 200.0), (0.0, 200.0));
        // Rotated 90° clockwise: the top-left corner is displayed top-right.
        assert_eq!(
            apply(place_matrix(c, 90, 1.0, 0.0, 0.0), 0.0, 200.0),
            (200.0, 100.0)
        );
        assert_eq!(
            apply(place_matrix(c, 180, 1.0, 0.0, 0.0), 0.0, 200.0),
            (100.0, 0.0)
        );
        assert_eq!(apply(place_matrix(c, 270, 1.0, 0.0, 0.0), 0.0, 200.0), (0.0, 0.0));
    }

    #[test]
    fn long_titles_are_shortened() {
        let s = fit("A very long heading that does not fit", 12.0, 80.0, true);
        assert!(s.ends_with('…') && bold_width(&s, 12.0) <= 80.0 + 12.0);
        assert_eq!(fit("short", 12.0, 200.0, false), "short");
    }
}
