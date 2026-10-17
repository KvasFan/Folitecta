//! Handwritten signatures and initials placed on pages: drawn strokes, typed
//! names or pictures. They stay comments (stamps) until they are applied,
//! which merges them into the page content.

use lopdf::{Object, ObjectId, dictionary};

use crate::document::Document;
use crate::error::{Error, Result};
use crate::flatten::FlattenScope;
use crate::util::{fmt_num, pdf_literal};

/// Stamp name of placed signatures.
pub const SIGNATURE_STAMP: &str = "FolitectaSignature";

/// What a placed signature shows.
#[derive(Debug, Clone, PartialEq)]
pub enum SignatureMark {
    /// Pen strokes with coordinates from 0 to 1 (x to the right, y downwards).
    Strokes {
        strokes: Vec<Vec<(f32, f32)>>,
        color: [f32; 3],
        /// Pen width relative to the height of the mark.
        width: f32,
    },
    /// A typed name in an italic typeface.
    Typed { text: String, color: [f32; 3] },
    /// A picture (PNG or JPEG).
    Image(Vec<u8>),
}

impl SignatureMark {
    /// Natural width / height ratio of the mark.
    pub fn aspect(&self) -> f32 {
        match self {
            SignatureMark::Strokes { strokes, .. } => {
                let pts: Vec<&(f32, f32)> = strokes.iter().flatten().collect();
                if pts.is_empty() {
                    return 3.0;
                }
                let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
                for (x, y) in pts {
                    x0 = x0.min(*x);
                    y0 = y0.min(*y);
                    x1 = x1.max(*x);
                    y1 = y1.max(*y);
                }
                ((x1 - x0).max(0.01) / (y1 - y0).max(0.01)).clamp(0.3, 12.0)
            }
            SignatureMark::Typed { text, .. } => {
                (crate::util::helvetica_width(text, 1.0) * 0.95 / 1.2).clamp(0.5, 20.0)
            }
            SignatureMark::Image(data) => image::load_from_memory(data)
                .map(|i| i.width() as f32 / i.height().max(1) as f32)
                .unwrap_or(3.0),
        }
    }
}

impl Document {
    /// Place a signature mark inside `rect` (PDF space) as a stamp comment.
    pub fn add_signature_mark(
        &mut self,
        page: usize,
        rect: [f32; 4],
        mark: &SignatureMark,
        author: &str,
    ) -> Result<ObjectId> {
        let page_id = self.page_id(page)?;
        let r = [
            rect[0].min(rect[2]),
            rect[1].min(rect[3]),
            rect[0].max(rect[2]),
            rect[1].max(rect[3]),
        ];
        let (w, h) = (r[2] - r[0], r[3] - r[1]);
        if w < 2.0 || h < 2.0 {
            return Err(Error::Invalid("the signature area is too small".into()));
        }
        let image = match mark {
            SignatureMark::Image(data) => Some(crate::images::image_object(data.clone())?),
            _ => None,
        };
        self.begin_edit();
        let f = fmt_num;
        let mut resources = lopdf::Dictionary::new();
        let content = match mark {
            SignatureMark::Strokes {
                strokes,
                color,
                width,
            } => {
                // Fit the strokes into the rectangle, keeping their proportions.
                let pts: Vec<&(f32, f32)> = strokes.iter().flatten().collect();
                let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
                for (x, y) in &pts {
                    x0 = x0.min(*x);
                    y0 = y0.min(*y);
                    x1 = x1.max(*x);
                    y1 = y1.max(*y);
                }
                let (sw, sh) = ((x1 - x0).max(0.001), (y1 - y0).max(0.001));
                let pen = (width * h).clamp(0.4, 6.0);
                let margin = pen;
                let s = ((w - 2.0 * margin) / sw).min((h - 2.0 * margin) / sh);
                let ox = r[0] + (w - sw * s) / 2.0;
                let oy = r[3] - (h - sh * s) / 2.0;
                let mut c = format!(
                    "{} {} {} RG {} w 1 J 1 j ",
                    f(color[0]),
                    f(color[1]),
                    f(color[2]),
                    f(pen)
                );
                for stroke in strokes {
                    for (i, (x, y)) in stroke.iter().enumerate() {
                        let (px, py) = (ox + (x - x0) * s, oy - (y - y0) * s);
                        c.push_str(&format!(
                            "{} {} {} ",
                            f(px),
                            f(py),
                            if i == 0 { "m" } else { "l" }
                        ));
                    }
                    if stroke.len() == 1 {
                        let (x, y) = stroke[0];
                        c.push_str(&format!(
                            "{} {} l ",
                            f(ox + (x - x0) * s + 0.1),
                            f(oy - (y - y0) * s)
                        ));
                    }
                    c.push_str("S ");
                }
                c
            }
            SignatureMark::Typed { text, color } => {
                let font = self.inner.add_object(dictionary! {
                    "Type" => "Font",
                    "Subtype" => "Type1",
                    "BaseFont" => "Times-Italic",
                    "Encoding" => "WinAnsiEncoding",
                });
                resources.set("Font", dictionary! { "TI" => font });
                // Times italic is narrower than Helvetica: estimate with a factor.
                let unit = crate::util::helvetica_width(text, 1.0) * 0.88;
                let fs = (h * 0.75).min((w * 0.94) / unit.max(0.1)).max(3.0);
                let tw = unit * fs;
                format!(
                    "BT /TI {} Tf {} {} {} rg {} {} Td {} Tj ET",
                    f(fs),
                    f(color[0]),
                    f(color[1]),
                    f(color[2]),
                    f(r[0] + (w - tw) / 2.0),
                    f(r[1] + (h - fs * 0.7) / 2.0),
                    String::from_utf8_lossy(&pdf_literal(text))
                )
            }
            SignatureMark::Image(_) => {
                let img = image.expect("decoded above");
                let (iw, ih) = (img.width.max(1.0), img.height.max(1.0));
                let id = self.add_image_object(img);
                resources.set("XObject", dictionary! { "Img" => id });
                let s = (w / iw).min(h / ih);
                let (dw, dh) = (iw * s, ih * s);
                format!(
                    "q {} 0 0 {} {} {} cm /Img Do Q",
                    f(dw),
                    f(dh),
                    f(r[0] + (w - dw) / 2.0),
                    f(r[1] + (h - dh) / 2.0)
                )
            }
        };
        let mut annot = self.base_annotation(page_id, "Stamp", r, author, "", [0.0, 0.0, 0.0]);
        annot.set("Name", Object::Name(SIGNATURE_STAMP.as_bytes().to_vec()));
        annot.set("Subj", lopdf::text_string("Signature"));
        self.add_appearance(&mut annot, r, content, resources);
        let id = self.push_annotation(page_id, annot)?;
        self.commit()?;
        Ok(id)
    }

    /// Placed signature marks: `(page, index)`.
    pub fn signature_marks(&self) -> Vec<(usize, usize)> {
        let doc = &self.inner;
        let mut out = Vec::new();
        for p in 0..self.page_count() {
            for a in self.annotations(p).unwrap_or_default() {
                if a.subtype != "Stamp" {
                    continue;
                }
                let is_mark =
                    a.id.and_then(|id| doc.get_dictionary(id).ok())
                        .and_then(|d| d.get(b"Name").ok())
                        .and_then(|n| n.as_name().ok())
                        == Some(SIGNATURE_STAMP.as_bytes());
                if is_mark {
                    out.push((p, a.index));
                }
            }
        }
        out
    }

    /// Place copies of the signature mark `(page, index)` at the same spot on
    /// other pages (one undo step). Returns the number placed.
    pub fn repeat_signature_mark(&mut self, page: usize, index: usize, pages: &[usize]) -> Result<usize> {
        if !self.signature_marks().contains(&(page, index)) {
            return Err(Error::Invalid("select a placed signature first".into()));
        }
        let n = self.page_count();
        if let Some(p) = pages.iter().find(|p| **p >= n) {
            return Err(Error::InvalidPage(*p));
        }
        let snap = self.annotation_snapshot(page, index)?;
        let targets: Vec<usize> = pages.iter().copied().filter(|p| *p != page).collect();
        let mark = self.undo_mark();
        for p in &targets {
            self.paste_annotation_snapshot(*p, &snap, 0.0, 0.0)?;
        }
        self.squash_undo(&mark);
        Ok(targets.len())
    }

    /// Merge every placed signature mark into its page (one undo step).
    pub fn apply_signature_marks(&mut self) -> Result<usize> {
        let marks = self.signature_marks();
        if marks.is_empty() {
            return Ok(0);
        }
        self.begin_edit();
        // Later indices first so earlier ones stay valid.
        let mut sorted = marks.clone();
        sorted.sort_by(|a, b| b.cmp(a));
        for (p, i) in sorted {
            self.flatten_internal(&[p], FlattenScope::Comments, Some((p, i)))?;
        }
        self.commit()?;
        Ok(marks.len())
    }

    /// Remove every placed signature mark that was not applied (one undo step).
    pub fn remove_signature_marks(&mut self) -> Result<usize> {
        let marks = self.signature_marks();
        if marks.is_empty() {
            return Ok(0);
        }
        self.begin_edit();
        let ids = self.page_ids();
        let mut by_page: std::collections::BTreeMap<usize, std::collections::HashSet<ObjectId>> =
            Default::default();
        for (p, i) in &marks {
            if let Ok(id) = self.annotation_object(*p, *i) {
                by_page.entry(*p).or_default().insert(id);
            }
        }
        for (p, set) in by_page {
            self.remove_annotations(ids[p], &set);
        }
        self.commit()?;
        Ok(marks.len())
    }
}
