//! Annotations (comments): listing, creation with appearance streams, deletion.

use lopdf::{Dictionary, Object, ObjectId, Stream, dictionary};

use crate::document::Document;
use crate::error::{Error, Result};
use crate::util::{self, color_of, deref, dict_text, fmt_num, pdf_literal, rect_of};

/// A summary of one annotation.
#[derive(Debug, Clone, PartialEq)]
pub struct Annotation {
    pub page: usize,
    /// Position inside the page's `/Annots` array.
    pub index: usize,
    pub id: Option<ObjectId>,
    pub subtype: String,
    pub contents: String,
    pub author: String,
    pub subject: String,
    pub modified: String,
    pub rect: [f32; 4],
    pub color: Option<[f32; 3]>,
    pub hidden: bool,
}

/// Closed shapes that can be drawn as comments.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    Square,
    Circle,
}

/// Text markup kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Markup {
    Highlight,
    Underline,
    StrikeOut,
}

const NON_COMMENT_SUBTYPES: &[&str] = &[
    "Link",
    "Widget",
    "Popup",
    "Screen",
    "PrinterMark",
    "TrapNet",
    "Watermark",
];

fn annot_refs(doc: &lopdf::Document, page: &Dictionary) -> Vec<(Option<ObjectId>, Dictionary)> {
    let Some(annots) = page.get(b"Annots").ok() else {
        return Vec::new();
    };
    let Ok(arr) = deref(doc, annots).as_array() else {
        return Vec::new();
    };
    arr.iter()
        .filter_map(|a| match a {
            Object::Reference(r) => doc.get_dictionary(*r).ok().map(|d| (Some(*r), d.clone())),
            Object::Dictionary(d) => Some((None, d.clone())),
            _ => None,
        })
        .collect()
}

fn circle_path(x0: f32, y0: f32, x1: f32, y1: f32) -> String {
    // Four cubic Béziers approximating an ellipse inside the rectangle.
    let k = 0.5523;
    let (cx, cy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
    let (rx, ry) = ((x1 - x0) / 2.0, (y1 - y0) / 2.0);
    let (ox, oy) = (rx * k, ry * k);
    let f = fmt_num;
    format!(
        "{} {} m {} {} {} {} {} {} c {} {} {} {} {} {} c {} {} {} {} {} {} c {} {} {} {} {} {} c h",
        f(cx + rx),
        f(cy),
        f(cx + rx),
        f(cy + oy),
        f(cx + ox),
        f(cy + ry),
        f(cx),
        f(cy + ry),
        f(cx - ox),
        f(cy + ry),
        f(cx - rx),
        f(cy + oy),
        f(cx - rx),
        f(cy),
        f(cx - rx),
        f(cy - oy),
        f(cx - ox),
        f(cy - ry),
        f(cx),
        f(cy - ry),
        f(cx + ox),
        f(cy - ry),
        f(cx + rx),
        f(cy - oy),
        f(cx + rx),
        f(cy),
    )
}

impl Document {
    /// Annotations of a page (all subtypes).
    pub fn annotations(&self, page: usize) -> Result<Vec<Annotation>> {
        let id = self.page_id(page)?;
        let dict = self.inner.get_dictionary(id)?;
        let doc = &self.inner;
        Ok(annot_refs(doc, dict)
            .into_iter()
            .enumerate()
            .map(|(index, (aid, d))| {
                let flags = d.get(b"F").ok().and_then(|f| f.as_i64().ok()).unwrap_or(0);
                Annotation {
                    page,
                    index,
                    id: aid,
                    subtype: d
                        .get(b"Subtype")
                        .ok()
                        .and_then(|s| s.as_name().ok())
                        .map(|n| String::from_utf8_lossy(n).to_string())
                        .unwrap_or_default(),
                    contents: dict_text(doc, &d, b"Contents").unwrap_or_default(),
                    author: dict_text(doc, &d, b"T").unwrap_or_default(),
                    subject: dict_text(doc, &d, b"Subj").unwrap_or_default(),
                    modified: dict_text(doc, &d, b"M")
                        .map(|m| util::pdf_date_display(&m))
                        .unwrap_or_default(),
                    rect: d
                        .get(b"Rect")
                        .ok()
                        .and_then(|r| rect_of(doc, r))
                        .unwrap_or([0.0; 4]),
                    color: d.get(b"C").ok().and_then(|c| color_of(doc, c)),
                    hidden: flags & 2 != 0,
                }
            })
            .collect())
    }

    /// Comment-like annotations of the whole document (links, widgets and popups excluded).
    pub fn comments(&self) -> Vec<Annotation> {
        (0..self.page_count())
            .filter_map(|p| self.annotations(p).ok())
            .flatten()
            .filter(|a| !NON_COMMENT_SUBTYPES.contains(&a.subtype.as_str()))
            .collect()
    }

    fn base_annotation(
        &self,
        page_id: ObjectId,
        subtype: &str,
        rect: [f32; 4],
        author: &str,
        contents: &str,
        color: [f32; 3],
    ) -> Dictionary {
        let date = util::pdf_date_now();
        let mut d = dictionary! {
            "Type" => "Annot",
            "Subtype" => subtype,
            "Rect" => vec![rect[0].into(), rect[1].into(), rect[2].into(), rect[3].into()],
            "P" => page_id,
            "F" => 4i64,
            "CA" => 1.0f32,
            "M" => Object::string_literal(date.clone()),
            "CreationDate" => Object::string_literal(date),
            "NM" => Object::string_literal(format!("folitecta-{}-{}", self.revision(), self.inner.max_id + 1)),
            "C" => vec![color[0].into(), color[1].into(), color[2].into()],
        };
        if !author.is_empty() {
            d.set("T", lopdf::text_string(author));
        }
        if !contents.is_empty() {
            d.set("Contents", lopdf::text_string(contents));
        }
        d
    }

    fn add_appearance(
        &mut self,
        annot: &mut Dictionary,
        rect: [f32; 4],
        content: String,
        resources: Dictionary,
    ) {
        let stream = Stream::new(
            dictionary! {
                "Type" => "XObject",
                "Subtype" => "Form",
                "BBox" => vec![rect[0].into(), rect[1].into(), rect[2].into(), rect[3].into()],
                "Resources" => resources,
            },
            content.into_bytes(),
        );
        let ap_id = self.inner.add_object(stream);
        annot.set("AP", dictionary! { "N" => ap_id });
    }

    fn push_annotation(&mut self, page_id: ObjectId, annot: Dictionary) -> Result<ObjectId> {
        let annot_id = self.inner.add_object(annot);
        let existing = self.inner.get_dictionary(page_id)?.get(b"Annots").ok().cloned();
        let new_value = match existing {
            Some(Object::Array(mut arr)) => {
                arr.push(Object::Reference(annot_id));
                Object::Array(arr)
            }
            Some(Object::Reference(r)) => {
                let mut arr = self
                    .inner
                    .get_object(r)
                    .ok()
                    .and_then(|o| o.as_array().ok())
                    .cloned()
                    .unwrap_or_default();
                arr.push(Object::Reference(annot_id));
                Object::Array(arr)
            }
            _ => Object::Array(vec![Object::Reference(annot_id)]),
        };
        self.inner.get_dictionary_mut(page_id)?.set("Annots", new_value);
        Ok(annot_id)
    }

    /// Add a sticky note at `(x, y)` (PDF user space, lower-left origin).
    pub fn add_note(
        &mut self,
        page: usize,
        x: f32,
        y: f32,
        contents: &str,
        author: &str,
        color: [f32; 3],
    ) -> Result<ObjectId> {
        let page_id = self.page_id(page)?;
        self.begin_edit();
        let rect = [x, y - 20.0, x + 20.0, y];
        let mut annot = self.base_annotation(page_id, "Text", rect, author, contents, color);
        annot.set("Name", Object::Name(b"Comment".to_vec()));
        annot.set("Open", false);
        let [r, g, b] = color;
        let f = fmt_num;
        let content = format!(
            "q {} {} {} rg 0.2 0.2 0.2 RG 1 w {} {} 18 14 re B {} {} m {} {} l {} {} l h B Q",
            f(r),
            f(g),
            f(b),
            f(x + 1.0),
            f(y - 15.0),
            f(x + 4.0),
            f(y - 15.0),
            f(x + 4.0),
            f(y - 19.5),
            f(x + 9.0),
            f(y - 15.0)
        );
        self.add_appearance(&mut annot, rect, content, Dictionary::new());
        let id = self.push_annotation(page_id, annot)?;
        self.commit()?;
        Ok(id)
    }

    /// Add a free text (typewriter) comment inside `rect`.
    pub fn add_free_text(
        &mut self,
        page: usize,
        rect: [f32; 4],
        text: &str,
        author: &str,
        font_size: f32,
        color: [f32; 3],
        border: bool,
    ) -> Result<ObjectId> {
        let page_id = self.page_id(page)?;
        self.begin_edit();
        let font_id = self.helvetica_font_id();
        let mut annot = self.base_annotation(page_id, "FreeText", rect, author, text, color);
        let [r, g, b] = color;
        annot.set(
            "DA",
            Object::string_literal(format!(
                "/Helv {} Tf {} {} {} rg",
                fmt_num(font_size),
                fmt_num(r),
                fmt_num(g),
                fmt_num(b)
            )),
        );
        annot.set("Q", 0i64);
        let f = fmt_num;
        let mut content = String::from("q ");
        if border {
            content.push_str(&format!(
                "1 1 1 rg 0.4 0.4 0.4 RG 1 w {} {} {} {} re B ",
                f(rect[0] + 0.5),
                f(rect[1] + 0.5),
                f(rect[2] - rect[0] - 1.0),
                f(rect[3] - rect[1] - 1.0)
            ));
        }
        content.push_str(&format!(
            "BT /Helv {} Tf {} {} {} rg {} TL {} {} Td ",
            f(font_size),
            f(r),
            f(g),
            f(b),
            f(font_size * 1.2),
            f(rect[0] + 3.0),
            f(rect[3] - font_size - 2.0)
        ));
        for (i, line) in text.lines().enumerate() {
            if i > 0 {
                content.push_str("T* ");
            }
            content.push_str(&String::from_utf8_lossy(&pdf_literal(line)));
            content.push_str(" Tj ");
        }
        content.push_str("ET Q");
        let resources = dictionary! { "Font" => dictionary! { "Helv" => font_id } };
        self.add_appearance(&mut annot, rect, content, resources);
        let id = self.push_annotation(page_id, annot)?;
        self.commit()?;
        Ok(id)
    }

    /// Add a rectangle or ellipse comment.
    pub fn add_shape(
        &mut self,
        page: usize,
        shape: Shape,
        rect: [f32; 4],
        stroke: [f32; 3],
        fill: Option<[f32; 3]>,
        width: f32,
        author: &str,
        contents: &str,
    ) -> Result<ObjectId> {
        let page_id = self.page_id(page)?;
        self.begin_edit();
        let subtype = match shape {
            Shape::Square => "Square",
            Shape::Circle => "Circle",
        };
        let mut annot = self.base_annotation(page_id, subtype, rect, author, contents, stroke);
        annot.set("BS", dictionary! { "W" => width, "S" => "S" });
        if let Some(ic) = fill {
            annot.set("IC", vec![ic[0].into(), ic[1].into(), ic[2].into()]);
        }
        let f = fmt_num;
        let half = width / 2.0;
        let mut content = format!(
            "q {} w {} {} {} RG ",
            f(width),
            f(stroke[0]),
            f(stroke[1]),
            f(stroke[2])
        );
        if let Some(ic) = fill {
            content.push_str(&format!("{} {} {} rg ", f(ic[0]), f(ic[1]), f(ic[2])));
        }
        let path = match shape {
            Shape::Square => format!(
                "{} {} {} {} re",
                f(rect[0] + half),
                f(rect[1] + half),
                f(rect[2] - rect[0] - width),
                f(rect[3] - rect[1] - width)
            ),
            Shape::Circle => circle_path(rect[0] + half, rect[1] + half, rect[2] - half, rect[3] - half),
        };
        content.push_str(&path);
        content.push_str(if fill.is_some() { " B Q" } else { " S Q" });
        self.add_appearance(&mut annot, rect, content, Dictionary::new());
        let id = self.push_annotation(page_id, annot)?;
        self.commit()?;
        Ok(id)
    }

    /// Add a straight line (optionally with an arrow head at the end).
    pub fn add_line(
        &mut self,
        page: usize,
        from: (f32, f32),
        to: (f32, f32),
        color: [f32; 3],
        width: f32,
        arrow: bool,
        author: &str,
        contents: &str,
    ) -> Result<ObjectId> {
        let page_id = self.page_id(page)?;
        self.begin_edit();
        let pad = width.max(4.0) * 3.0;
        let rect = [
            from.0.min(to.0) - pad,
            from.1.min(to.1) - pad,
            from.0.max(to.0) + pad,
            from.1.max(to.1) + pad,
        ];
        let mut annot = self.base_annotation(page_id, "Line", rect, author, contents, color);
        annot.set("L", vec![from.0.into(), from.1.into(), to.0.into(), to.1.into()]);
        annot.set("BS", dictionary! { "W" => width });
        if arrow {
            annot.set(
                "LE",
                vec![
                    Object::Name(b"None".to_vec()),
                    Object::Name(b"OpenArrow".to_vec()),
                ],
            );
        }
        let f = fmt_num;
        let mut content = format!(
            "q {} w 1 J 1 j {} {} {} RG {} {} m {} {} l S ",
            f(width),
            f(color[0]),
            f(color[1]),
            f(color[2]),
            f(from.0),
            f(from.1),
            f(to.0),
            f(to.1)
        );
        if arrow {
            let (dx, dy) = (to.0 - from.0, to.1 - from.1);
            let len = (dx * dx + dy * dy).sqrt().max(0.001);
            let (ux, uy) = (dx / len, dy / len);
            let size = width.max(2.0) * 4.0;
            let (bx, by) = (to.0 - ux * size, to.1 - uy * size);
            let (px, py) = (-uy * size * 0.5, ux * size * 0.5);
            content.push_str(&format!(
                "{} {} m {} {} l {} {} l S ",
                f(bx + px),
                f(by + py),
                f(to.0),
                f(to.1),
                f(bx - px),
                f(by - py)
            ));
        }
        content.push('Q');
        self.add_appearance(&mut annot, rect, content, Dictionary::new());
        let id = self.push_annotation(page_id, annot)?;
        self.commit()?;
        Ok(id)
    }

    /// Add a freehand ink comment made of one or more polylines.
    pub fn add_ink(
        &mut self,
        page: usize,
        paths: &[Vec<(f32, f32)>],
        color: [f32; 3],
        width: f32,
        author: &str,
    ) -> Result<ObjectId> {
        let page_id = self.page_id(page)?;
        let pts: Vec<(f32, f32)> = paths.iter().flatten().copied().collect();
        if pts.len() < 2 {
            return Err(Error::Invalid("an ink comment needs at least two points".into()));
        }
        self.begin_edit();
        let pad = width + 2.0;
        let rect = pts.iter().fold([f32::MAX, f32::MAX, f32::MIN, f32::MIN], |r, p| {
            [
                r[0].min(p.0 - pad),
                r[1].min(p.1 - pad),
                r[2].max(p.0 + pad),
                r[3].max(p.1 + pad),
            ]
        });
        let mut annot = self.base_annotation(page_id, "Ink", rect, author, "", color);
        annot.set("BS", dictionary! { "W" => width });
        let ink_list: Vec<Object> = paths
            .iter()
            .map(|p| Object::Array(p.iter().flat_map(|(x, y)| [(*x).into(), (*y).into()]).collect()))
            .collect();
        annot.set("InkList", Object::Array(ink_list));
        let f = fmt_num;
        let mut content = format!(
            "q {} w 1 J 1 j {} {} {} RG ",
            f(width),
            f(color[0]),
            f(color[1]),
            f(color[2])
        );
        for p in paths {
            for (i, (x, y)) in p.iter().enumerate() {
                content.push_str(&format!(
                    "{} {} {} ",
                    f(*x),
                    f(*y),
                    if i == 0 { "m" } else { "l" }
                ));
            }
            content.push_str("S ");
        }
        content.push('Q');
        self.add_appearance(&mut annot, rect, content, Dictionary::new());
        let id = self.push_annotation(page_id, annot)?;
        self.commit()?;
        Ok(id)
    }

    /// Add a text markup (highlight/underline/strikeout) over the given quadrilaterals.
    /// Each quad is `[x1 y1 x2 y2 x3 y3 x4 y4]` in PDF order (upper-left, upper-right, lower-left, lower-right).
    pub fn add_text_markup(
        &mut self,
        page: usize,
        kind: Markup,
        quads: &[[f32; 8]],
        color: [f32; 3],
        author: &str,
        contents: &str,
    ) -> Result<ObjectId> {
        if quads.is_empty() {
            return Err(Error::Invalid("text markup needs at least one rectangle".into()));
        }
        let page_id = self.page_id(page)?;
        self.begin_edit();
        let rect = quads
            .iter()
            .fold([f32::MAX, f32::MAX, f32::MIN, f32::MIN], |r, q| {
                let xs = [q[0], q[2], q[4], q[6]];
                let ys = [q[1], q[3], q[5], q[7]];
                [
                    r[0].min(xs.iter().cloned().fold(f32::MAX, f32::min)),
                    r[1].min(ys.iter().cloned().fold(f32::MAX, f32::min)),
                    r[2].max(xs.iter().cloned().fold(f32::MIN, f32::max)),
                    r[3].max(ys.iter().cloned().fold(f32::MIN, f32::max)),
                ]
            });
        let subtype = match kind {
            Markup::Highlight => "Highlight",
            Markup::Underline => "Underline",
            Markup::StrikeOut => "StrikeOut",
        };
        let mut annot = self.base_annotation(page_id, subtype, rect, author, contents, color);
        annot.set(
            "QuadPoints",
            Object::Array(quads.iter().flatten().map(|v| (*v).into()).collect()),
        );
        let f = fmt_num;
        let mut content = format!(
            "q /GS gs {} {} {} rg {} {} {} RG ",
            f(color[0]),
            f(color[1]),
            f(color[2]),
            f(color[0]),
            f(color[1]),
            f(color[2])
        );
        for q in quads {
            let x0 = q[0].min(q[4]);
            let x1 = q[2].max(q[6]);
            let top = q[1].max(q[3]);
            let bottom = q[5].min(q[7]);
            let h = top - bottom;
            match kind {
                Markup::Highlight => {
                    content.push_str(&format!("{} {} {} {} re f ", f(x0), f(bottom), f(x1 - x0), f(h)))
                }
                Markup::Underline => content.push_str(&format!(
                    "{} {} {} {} re f ",
                    f(x0),
                    f(bottom),
                    f(x1 - x0),
                    f((h * 0.07).max(0.8))
                )),
                Markup::StrikeOut => content.push_str(&format!(
                    "{} {} {} {} re f ",
                    f(x0),
                    f(bottom + h * 0.45),
                    f(x1 - x0),
                    f((h * 0.07).max(0.8))
                )),
            }
        }
        content.push('Q');
        let blend = if kind == Markup::Highlight {
            "Multiply"
        } else {
            "Normal"
        };
        let resources = dictionary! { "ExtGState" => dictionary! { "GS" => dictionary! { "BM" => blend, "CA" => 1.0f32, "ca" => 1.0f32 } } };
        self.add_appearance(&mut annot, rect, content, resources);
        let id = self.push_annotation(page_id, annot)?;
        self.commit()?;
        Ok(id)
    }

    /// Delete one annotation by its position in the page's `/Annots` array.
    pub fn delete_annotation(&mut self, page: usize, index: usize) -> Result<()> {
        let page_id = self.page_id(page)?;
        let existing = self.inner.get_dictionary(page_id)?.get(b"Annots").ok().cloned();
        let mut arr = match existing {
            Some(Object::Array(arr)) => arr,
            Some(Object::Reference(r)) => self
                .inner
                .get_object(r)
                .ok()
                .and_then(|o| o.as_array().ok())
                .cloned()
                .unwrap_or_default(),
            _ => Vec::new(),
        };
        if index >= arr.len() {
            return Err(Error::Invalid(format!("annotation {index} does not exist")));
        }
        self.begin_edit();
        let removed = arr.remove(index);
        self.inner
            .get_dictionary_mut(page_id)?
            .set("Annots", Object::Array(arr));
        if let Object::Reference(r) = removed {
            // Remove popups attached to the annotation too.
            let popup = self
                .inner
                .get_dictionary(r)
                .ok()
                .and_then(|d| d.get(b"Popup").ok())
                .and_then(|p| p.as_reference().ok());
            self.inner.delete_object(r);
            if let Some(p) = popup {
                self.inner.delete_object(p);
                if let Ok(d) = self.inner.get_dictionary_mut(page_id) {
                    if let Ok(Object::Array(arr)) = d.get_mut(b"Annots") {
                        arr.retain(|a| a.as_reference().ok() != Some(p));
                    }
                }
            }
        }
        self.commit()
    }

    /// Delete every comment of a page (or of the whole document when `page` is `None`).
    pub fn delete_all_comments(&mut self, page: Option<usize>) -> Result<usize> {
        let pages: Vec<usize> = match page {
            Some(p) => vec![p],
            None => (0..self.page_count()).collect(),
        };
        self.begin_edit();
        let mut removed = 0;
        for p in pages {
            let page_id = self.page_id(p)?;
            let items = annot_refs(&self.inner, self.inner.get_dictionary(page_id)?);
            let mut keep = Vec::new();
            for (aid, d) in items {
                let subtype = d
                    .get(b"Subtype")
                    .ok()
                    .and_then(|s| s.as_name().ok())
                    .map(|n| String::from_utf8_lossy(n).to_string())
                    .unwrap_or_default();
                if NON_COMMENT_SUBTYPES.contains(&subtype.as_str()) {
                    if let Some(id) = aid {
                        keep.push(Object::Reference(id));
                    } else {
                        keep.push(Object::Dictionary(d));
                    }
                } else {
                    removed += 1;
                    if let Some(id) = aid {
                        self.inner.delete_object(id);
                    }
                }
            }
            self.inner
                .get_dictionary_mut(page_id)?
                .set("Annots", Object::Array(keep));
        }
        self.commit()?;
        Ok(removed)
    }

    /// Update the text of an annotation.
    pub fn set_annotation_contents(&mut self, page: usize, index: usize, contents: &str) -> Result<()> {
        let id = self
            .annotations(page)?
            .get(index)
            .and_then(|a| a.id)
            .ok_or_else(|| Error::Invalid("annotation is not an indirect object".into()))?;
        self.begin_edit();
        let d = self.inner.get_dictionary_mut(id)?;
        d.set("Contents", lopdf::text_string(contents));
        d.set("M", Object::string_literal(util::pdf_date_now()));
        self.commit()
    }

    /// Set the hidden flag of an annotation.
    pub fn set_annotation_hidden(&mut self, page: usize, index: usize, hidden: bool) -> Result<()> {
        let id = self
            .annotations(page)?
            .get(index)
            .and_then(|a| a.id)
            .ok_or_else(|| Error::Invalid("annotation is not an indirect object".into()))?;
        self.begin_edit();
        let d = self.inner.get_dictionary_mut(id)?;
        let flags = d.get(b"F").ok().and_then(|f| f.as_i64().ok()).unwrap_or(4);
        let flags = if hidden { flags | 2 } else { flags & !2 };
        d.set("F", Object::Integer(flags));
        self.commit()
    }
}
