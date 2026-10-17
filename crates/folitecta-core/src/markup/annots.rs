//! Annotations (comments): listing, creation with appearance streams, deletion.

use std::collections::HashSet;

use lopdf::{Dictionary, Object, ObjectId, Stream, dictionary};

use crate::appearance::{AnnotStyle, apply_style};
use crate::document::Document;
use crate::error::{Error, Result};
use crate::util::{self, color_of, deref, dict_text, rect_of};

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
    /// A reply to another comment (`/IRT` without `/RT /Group`), including review states.
    pub is_reply: bool,
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
    Squiggly,
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

pub(crate) fn annot_refs(doc: &lopdf::Document, page: &Dictionary) -> Vec<(Option<ObjectId>, Dictionary)> {
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
                    is_reply: d.has(b"IRT")
                        && d.get(b"RT").ok().and_then(|r| r.as_name().ok()) != Some(b"Group".as_slice()),
                }
            })
            .collect())
    }

    /// Comment-like annotations of the whole document (links, widgets and popups excluded).
    pub fn comments(&self) -> Vec<Annotation> {
        (0..self.page_count())
            .flat_map(|p| self.page_comments(p))
            .collect()
    }

    /// Comment-like annotations of one page.
    pub fn page_comments(&self, page: usize) -> Vec<Annotation> {
        self.annotations(page)
            .unwrap_or_default()
            .into_iter()
            .filter(|a| !NON_COMMENT_SUBTYPES.contains(&a.subtype.as_str()))
            .collect()
    }

    pub(crate) fn base_annotation(
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

    pub(crate) fn add_appearance(
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

    pub(crate) fn push_annotation(&mut self, page_id: ObjectId, annot: Dictionary) -> Result<ObjectId> {
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

    /// Generate the appearance of a new comment and add it to the page (one undo step).
    pub(crate) fn finish_annotation(&mut self, page_id: ObjectId, mut annot: Dictionary) -> Result<ObjectId> {
        self.generate_appearance(&mut annot)?;
        let id = self.push_annotation(page_id, annot)?;
        self.commit()?;
        Ok(id)
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
        self.finish_annotation(page_id, annot)
    }

    /// Add a free text comment inside `rect`: a text box with a border, or
    /// typewriter text without one.
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
        let style = AnnotStyle {
            color: Some(if border { [0.4, 0.4, 0.4] } else { color }),
            fill: border.then_some([1.0, 1.0, 1.0]),
            width: if border { 1.0 } else { 0.0 },
            font_size,
            text_color: color,
            ..AnnotStyle::default()
        };
        self.add_free_text_styled(page, rect, text, author, &style)
    }

    /// Add a free text comment with a complete style.
    pub fn add_free_text_styled(
        &mut self,
        page: usize,
        rect: [f32; 4],
        text: &str,
        author: &str,
        style: &AnnotStyle,
    ) -> Result<ObjectId> {
        let page_id = self.page_id(page)?;
        self.begin_edit();
        let mut annot = self.base_annotation(
            page_id,
            "FreeText",
            rect,
            author,
            text,
            style.color.unwrap_or(style.text_color),
        );
        apply_style(&mut annot, "FreeText", style);
        self.finish_annotation(page_id, annot)
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
        let style = AnnotStyle {
            color: Some(stroke),
            fill,
            width,
            ..AnnotStyle::default()
        };
        self.add_shape_styled(page, shape, rect, &style, author, contents)
    }

    /// Add a rectangle or ellipse comment with a complete style.
    pub fn add_shape_styled(
        &mut self,
        page: usize,
        shape: Shape,
        rect: [f32; 4],
        style: &AnnotStyle,
        author: &str,
        contents: &str,
    ) -> Result<ObjectId> {
        let page_id = self.page_id(page)?;
        self.begin_edit();
        let subtype = match shape {
            Shape::Square => "Square",
            Shape::Circle => "Circle",
        };
        let mut annot = self.base_annotation(
            page_id,
            subtype,
            rect,
            author,
            contents,
            style.color.unwrap_or([0.0; 3]),
        );
        apply_style(&mut annot, subtype, style);
        self.finish_annotation(page_id, annot)
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
        let style = AnnotStyle {
            color: Some(color),
            width,
            line_end: if arrow { "OpenArrow".into() } else { "None".into() },
            ..AnnotStyle::default()
        };
        self.add_polyline(page, &[from, to], false, &style, author, contents)
    }

    /// Add a line (two points), polyline or polygon comment.
    pub fn add_polyline(
        &mut self,
        page: usize,
        points: &[(f32, f32)],
        closed: bool,
        style: &AnnotStyle,
        author: &str,
        contents: &str,
    ) -> Result<ObjectId> {
        if points.len() < 2 || (closed && points.len() < 3) {
            return Err(Error::Invalid("not enough points for the shape".into()));
        }
        let page_id = self.page_id(page)?;
        self.begin_edit();
        let subtype = match (closed, points.len()) {
            (true, _) => "Polygon",
            (false, 2) => "Line",
            (false, _) => "PolyLine",
        };
        let flat: Vec<Object> = points
            .iter()
            .flat_map(|(x, y)| [(*x).into(), (*y).into()])
            .collect();
        let rect = points
            .iter()
            .fold([f32::MAX, f32::MAX, f32::MIN, f32::MIN], |r, p| {
                [r[0].min(p.0), r[1].min(p.1), r[2].max(p.0), r[3].max(p.1)]
            });
        let mut annot = self.base_annotation(
            page_id,
            subtype,
            rect,
            author,
            contents,
            style.color.unwrap_or([0.0; 3]),
        );
        annot.set(
            if subtype == "Line" { "L" } else { "Vertices" },
            Object::Array(flat),
        );
        apply_style(&mut annot, subtype, style);
        self.finish_annotation(page_id, annot)
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
        let style = AnnotStyle {
            color: Some(color),
            width,
            ..AnnotStyle::default()
        };
        self.add_ink_styled(page, paths, &style, author)
    }

    /// Add a freehand ink comment with a complete style.
    pub fn add_ink_styled(
        &mut self,
        page: usize,
        paths: &[Vec<(f32, f32)>],
        style: &AnnotStyle,
        author: &str,
    ) -> Result<ObjectId> {
        let page_id = self.page_id(page)?;
        let pts: Vec<(f32, f32)> = paths.iter().flatten().copied().collect();
        if pts.len() < 2 {
            return Err(Error::Invalid("an ink comment needs at least two points".into()));
        }
        self.begin_edit();
        let rect = pts.iter().fold([f32::MAX, f32::MAX, f32::MIN, f32::MIN], |r, p| {
            [r[0].min(p.0), r[1].min(p.1), r[2].max(p.0), r[3].max(p.1)]
        });
        let mut annot =
            self.base_annotation(page_id, "Ink", rect, author, "", style.color.unwrap_or([0.0; 3]));
        let ink_list: Vec<Object> = paths
            .iter()
            .map(|p| Object::Array(p.iter().flat_map(|(x, y)| [(*x).into(), (*y).into()]).collect()))
            .collect();
        annot.set("InkList", Object::Array(ink_list));
        apply_style(&mut annot, "Ink", style);
        self.finish_annotation(page_id, annot)
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
            Markup::Squiggly => "Squiggly",
        };
        let mut annot = self.base_annotation(page_id, subtype, rect, author, contents, color);
        annot.set(
            "QuadPoints",
            Object::Array(quads.iter().flatten().map(|v| (*v).into()).collect()),
        );
        self.finish_annotation(page_id, annot)
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
        match arr[index].clone() {
            Object::Reference(r) => self.remove_annotations(page_id, &HashSet::from([r])),
            _ => {
                arr.remove(index);
                self.inner
                    .get_dictionary_mut(page_id)?
                    .set("Annots", Object::Array(arr));
            }
        }
        self.commit()
    }

    /// Remove annotations of a page together with their popups, replies and
    /// review states (recursively).
    pub(crate) fn remove_annotations(&mut self, page_id: ObjectId, roots: &HashSet<ObjectId>) {
        let Ok(page) = self.inner.get_dictionary(page_id) else {
            return;
        };
        let items = annot_refs(&self.inner, page);
        let mut gone: HashSet<ObjectId> = roots.clone();
        loop {
            let before = gone.len();
            for (aid, d) in &items {
                let Some(id) = aid else { continue };
                if gone.contains(id) {
                    continue;
                }
                let is_popup =
                    d.get(b"Subtype").ok().and_then(|s| s.as_name().ok()) == Some(b"Popup".as_slice());
                let key: &[u8] = if is_popup { b"Parent" } else { b"IRT" };
                let target = d.get(key).ok().and_then(|o| o.as_reference().ok());
                if target.map(|t| gone.contains(&t)).unwrap_or(false) {
                    gone.insert(*id);
                }
            }
            if gone.len() == before {
                break;
            }
        }
        let keep: Vec<Object> = items
            .into_iter()
            .filter(|(aid, _)| aid.map(|id| !gone.contains(&id)).unwrap_or(true))
            .map(|(aid, d)| match aid {
                Some(id) => Object::Reference(id),
                None => Object::Dictionary(d),
            })
            .collect();
        if let Ok(d) = self.inner.get_dictionary_mut(page_id) {
            d.set("Annots", Object::Array(keep));
        }
        for id in gone {
            self.inner.objects.remove(&id);
        }
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
            let mut roots = HashSet::new();
            let mut inline = false;
            for (aid, d) in &items {
                let subtype = d
                    .get(b"Subtype")
                    .ok()
                    .and_then(|s| s.as_name().ok())
                    .map(|n| String::from_utf8_lossy(n).to_string())
                    .unwrap_or_default();
                if !NON_COMMENT_SUBTYPES.contains(&subtype.as_str()) {
                    removed += 1;
                    match aid {
                        Some(id) => {
                            roots.insert(*id);
                        }
                        None => inline = true,
                    }
                }
            }
            self.remove_annotations(page_id, &roots);
            if inline {
                // Direct (inline) comment dictionaries.
                let items = annot_refs(&self.inner, self.inner.get_dictionary(page_id)?);
                let keep: Vec<Object> = items
                    .into_iter()
                    .filter_map(|(aid, d)| {
                        let subtype = d
                            .get(b"Subtype")
                            .ok()
                            .and_then(|s| s.as_name().ok())
                            .map(|n| String::from_utf8_lossy(n).to_string())
                            .unwrap_or_default();
                        match aid {
                            Some(id) => Some(Object::Reference(id)),
                            None if NON_COMMENT_SUBTYPES.contains(&subtype.as_str()) => {
                                Some(Object::Dictionary(d))
                            }
                            None => None,
                        }
                    })
                    .collect();
                self.inner
                    .get_dictionary_mut(page_id)?
                    .set("Annots", Object::Array(keep));
            }
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
        // Text boxes and text stamps show their text: redraw them.
        let shows_text = matches!(
            d.get(b"Subtype").ok().and_then(|s| s.as_name().ok()),
            Some(b"FreeText") | Some(b"Stamp")
        );
        if shows_text {
            self.refresh_appearance(id)?;
        }
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
