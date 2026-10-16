//! Page marks: text watermarks, headers and footers, page numbering (including Bates style).

use lopdf::{Object, ObjectId, dictionary};

use crate::document::{Document, PageSize};
use crate::error::{Error, Result};
use crate::util::{self, approx_text_width, fmt_num, pdf_literal};

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

fn text_op(
    x: f32,
    y: f32,
    angle_deg: f32,
    font_size: f32,
    color: [f32; 3],
    gs: Option<&str>,
    text: &str,
) -> String {
    let f = fmt_num;
    let (sin, cos) = angle_deg.to_radians().sin_cos();
    let mut s = String::from("q ");
    if let Some(g) = gs {
        s.push_str(&format!("/{g} gs "));
    }
    s.push_str(&format!(
        "BT /FoliHelv {} Tf {} {} {} rg {} {} {} {} {} {} Tm ",
        f(font_size),
        f(color[0]),
        f(color[1]),
        f(color[2]),
        f(cos),
        f(sin),
        f(-sin),
        f(cos),
        f(x),
        f(y)
    ));
    s.push_str(&String::from_utf8_lossy(&pdf_literal(text)));
    s.push_str(" Tj ET Q\n");
    s
}

impl Document {
    /// Expand `{page}`, `{pages}`, `{date}`, `{title}`, `{file}`, `{author}` and
    /// `{bates:N}` placeholders for a page.
    pub fn expand_macros(&self, template: &str, page_index: usize, start: i64) -> String {
        let info = self.info();
        let total = self.page_count();
        let number = start + page_index as i64;
        let mut out = template.to_string();
        out = out
            .replace("{page}", &number.to_string())
            .replace("{n}", &number.to_string());
        out = out
            .replace("{pages}", &total.to_string())
            .replace("{total}", &total.to_string());
        out = out.replace("{date}", &util::today_display());
        let file_name = self.display_name();
        let title = if info.title.is_empty() {
            file_name.as_str()
        } else {
            info.title.as_str()
        };
        out = out.replace("{title}", title);
        out = out.replace("{file}", &file_name);
        out = out.replace("{author}", &info.author);
        while let Some(start_idx) = out.find("{bates:") {
            let Some(end_rel) = out[start_idx..].find('}') else {
                break;
            };
            let end_idx = start_idx + end_rel;
            let digits: usize = out[start_idx + 7..end_idx].trim().parse().unwrap_or(6);
            let value = format!("{:0width$}", number, width = digits);
            out.replace_range(start_idx..=end_idx, &value);
        }
        out
    }

    fn prepare_mark_resources(&mut self, page_id: ObjectId, opacity: f32) -> Result<Option<String>> {
        let font = self.helvetica_font_id();
        self.add_page_resource(page_id, "Font", "FoliHelv", Object::Reference(font))?;
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
        self.begin_edit();
        for &p in pages {
            let id = ids[p];
            let size = self.page_size_of(id);
            let gs = self.prepare_mark_resources(id, opts.opacity)?;
            let w = approx_text_width(&opts.text, opts.font_size);
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
            let content = text_op(x, y, angle, opts.font_size, opts.color, gs.as_deref(), &opts.text);
            self.append_page_content(id, content.into_bytes(), Some("Watermark"))?;
        }
        self.commit()
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
        self.begin_edit();
        let slots = [
            (header[0], MarkPosition::TopLeft),
            (header[1], MarkPosition::TopCenter),
            (header[2], MarkPosition::TopRight),
            (footer[0], MarkPosition::BottomLeft),
            (footer[1], MarkPosition::BottomCenter),
            (footer[2], MarkPosition::BottomRight),
        ];
        for &p in pages {
            let id = ids[p];
            let size = self.page_size_of(id);
            self.prepare_mark_resources(id, 1.0)?;
            let mut content = String::new();
            for (template, pos) in slots {
                if template.trim().is_empty() {
                    continue;
                }
                let text = self.expand_macros(template, p, 1);
                let w = approx_text_width(&text, font_size);
                let (u, v) = anchor(&size, pos, w, font_size, margin);
                let (x, y) = display_to_pdf(&size, u, v);
                content.push_str(&text_op(
                    x,
                    y,
                    size.rotation as f32,
                    font_size,
                    color,
                    None,
                    &text,
                ));
            }
            self.append_page_content(id, content.into_bytes(), Some("HeaderFooter"))?;
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
        self.begin_edit();
        for (ordinal, &p) in pages.iter().enumerate() {
            let id = ids[p];
            let size = self.page_size_of(id);
            self.prepare_mark_resources(id, 1.0)?;
            // Number relative to the selection so Bates numbering is contiguous.
            let text = self.expand_macros(template, ordinal, start);
            let w = approx_text_width(&text, font_size);
            let (u, v) = anchor(&size, position, w, font_size, margin);
            let (x, y) = display_to_pdf(&size, u, v);
            let content = text_op(x, y, size.rotation as f32, font_size, color, None, &text);
            self.append_page_content(id, content.into_bytes(), Some("PageNumber"))?;
        }
        self.commit()
    }

    /// Remove marks previously added by Folitecta. `tag` restricts the kind
    /// ("Watermark", "HeaderFooter", "PageNumber"); `None` removes all kinds.
    pub fn remove_marks(&mut self, pages: &[usize], tag: Option<&str>) -> Result<usize> {
        let ids = self.page_ids();
        for &p in pages {
            if p >= ids.len() {
                return Err(Error::InvalidPage(p));
            }
        }
        self.begin_edit();
        let mut removed = 0;
        for &p in pages {
            let id = ids[p];
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
                    .map(|m| tag.map(|t| t.as_bytes() == m).unwrap_or(true))
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
        self.commit()?;
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
