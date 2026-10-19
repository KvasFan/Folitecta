//! Review workflow on comments: replies, review and check-mark states, comment
//! flags, pop-up notes and display filters.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use lopdf::{Dictionary, Object, ObjectId, dictionary};

use crate::document::Document;
use crate::error::{Error, Result};
use crate::layer_drawing::{oc_visible, single_layer};
use crate::util::{self, color_of, dict_text, rect_of};

/// Review states of the "Review" state model.
pub const REVIEW_STATES: &[&str] = &["Accepted", "Rejected", "Cancelled", "Completed", "None"];

/// Annotation flag bits.
pub const FLAG_HIDDEN: i64 = 2;
pub const FLAG_PRINT: i64 = 4;
pub const FLAG_LOCKED: i64 = 128;

/// A comment with its review information, for the comments pane.
#[derive(Debug, Clone, PartialEq)]
pub struct CommentInfo {
    pub page: usize,
    pub index: usize,
    pub id: ObjectId,
    pub subtype: String,
    pub author: String,
    pub subject: String,
    pub contents: String,
    pub created: String,
    pub modified: String,
    /// Raw modification date (`D:...`), for sorting.
    pub modified_raw: String,
    pub color: Option<[f32; 3]>,
    pub rect: [f32; 4],
    pub hidden: bool,
    pub locked: bool,
    pub printable: bool,
    /// The comment this one replies to.
    pub reply_to: Option<ObjectId>,
    /// A review or check-mark state change (listed in the review history only).
    pub is_state: bool,
    /// Current review state ("" when none was set).
    pub review: String,
    /// Checked with a check mark.
    pub marked: bool,
    /// Pop-up note: object, open, rectangle.
    pub popup: Option<(ObjectId, bool, [f32; 4])>,
    /// Belongs to a layer that is hidden by default.
    pub in_hidden_layer: bool,
    /// The layer it belongs to, when it names one directly.
    pub layer: Option<ObjectId>,
}

/// One state change of a comment.
#[derive(Debug, Clone, PartialEq)]
pub struct StateChange {
    pub date: String,
    pub author: String,
    pub model: String,
    pub state: String,
}

fn flags_of(d: &Dictionary) -> i64 {
    d.get(b"F").ok().and_then(|f| f.as_i64().ok()).unwrap_or(0)
}

impl Document {
    /// Every comment with review information, in page order.
    pub fn comment_infos(&self) -> Vec<CommentInfo> {
        self.infos_of(self.comments())
    }

    /// Comments of one page with review information (replies and states on
    /// other pages are not taken into account).
    pub fn page_comment_infos(&self, page: usize) -> Vec<CommentInfo> {
        self.infos_of(self.page_comments(page))
    }

    fn infos_of(&self, comments: Vec<crate::annots::Annotation>) -> Vec<CommentInfo> {
        let doc = &self.inner;
        let layer_states = self.layer_defaults();
        let mut out = Vec::new();
        let mut states: Vec<(ObjectId, String, String, String)> = Vec::new();
        for a in comments {
            let Some(id) = a.id else { continue };
            let Ok(d) = doc.get_dictionary(id) else {
                continue;
            };
            let flags = flags_of(d);
            let irt = d.get(b"IRT").ok().and_then(|r| r.as_reference().ok());
            let group = d.get(b"RT").ok().and_then(|r| r.as_name().ok()) == Some(b"Group".as_slice());
            let state = dict_text(doc, d, b"State");
            let model = dict_text(doc, d, b"StateModel").unwrap_or_else(|| {
                // Default model of a state (PDF 32000-1, 12.5.6.3).
                match state.as_deref() {
                    Some("Marked") | Some("Unmarked") => "Marked".into(),
                    _ => "Review".into(),
                }
            });
            let modified_raw = dict_text(doc, d, b"M").unwrap_or_default();
            let is_state = state.is_some() && irt.is_some();
            if let (true, Some(parent), Some(s)) = (is_state, irt, state.clone()) {
                states.push((parent, modified_raw.clone(), model.clone(), s));
            }
            let popup = d
                .get(b"Popup")
                .ok()
                .and_then(|p| p.as_reference().ok())
                .and_then(|p| {
                    let pd = doc.get_dictionary(p).ok()?;
                    let open = pd
                        .get(b"Open")
                        .ok()
                        .and_then(|o| o.as_bool().ok())
                        .unwrap_or(false);
                    let rect = pd.get(b"Rect").ok().and_then(|r| rect_of(doc, r))?;
                    Some((p, open, rect))
                });
            let oc = d.get(b"OC").ok();
            let in_hidden_layer = oc.is_some_and(|oc| !oc_visible(doc, oc, &layer_states));
            let layer = oc.and_then(|oc| single_layer(doc, oc));
            out.push(CommentInfo {
                page: a.page,
                index: a.index,
                id,
                subtype: a.subtype.clone(),
                author: a.author.clone(),
                subject: a.subject.clone(),
                contents: a.contents.clone(),
                created: dict_text(doc, d, b"CreationDate")
                    .map(|c| util::pdf_date_display(&c))
                    .unwrap_or_default(),
                modified: a.modified.clone(),
                modified_raw,
                color: a
                    .color
                    .or_else(|| d.get(b"IC").ok().and_then(|c| color_of(doc, c))),
                rect: a.rect,
                hidden: flags & FLAG_HIDDEN != 0,
                locked: flags & FLAG_LOCKED != 0,
                printable: flags & FLAG_PRINT != 0,
                reply_to: if group { None } else { irt },
                is_state,
                review: String::new(),
                marked: false,
                popup,
                in_hidden_layer,
                layer,
            });
        }
        // The latest state of each model wins.
        states.sort_by(|a, b| a.1.cmp(&b.1));
        let mut review: HashMap<ObjectId, String> = HashMap::new();
        let mut marked: HashMap<ObjectId, bool> = HashMap::new();
        for (parent, _, model, state) in states {
            if model == "Marked" {
                marked.insert(parent, state == "Marked");
            } else if state == "None" {
                review.remove(&parent);
            } else {
                review.insert(parent, state);
            }
        }
        for c in &mut out {
            if let Some(r) = review.get(&c.id) {
                c.review = r.clone();
            }
            c.marked = marked.get(&c.id).copied().unwrap_or(false);
        }
        out
    }

    fn comment_rect_color(&self, id: ObjectId) -> Result<([f32; 4], [f32; 3])> {
        let d = self.inner.get_dictionary(id)?;
        let rect = d
            .get(b"Rect")
            .ok()
            .and_then(|r| rect_of(&self.inner, r))
            .ok_or_else(|| Error::Invalid("the comment has no rectangle".into()))?;
        let color = d
            .get(b"C")
            .ok()
            .and_then(|c| color_of(&self.inner, c))
            .unwrap_or([1.0, 0.85, 0.2]);
        Ok((rect, color))
    }

    /// Put a reply, review state or pop-up note on the layer of the comment
    /// it belongs to.
    fn follow_layer_of(&self, parent: ObjectId, d: &mut Dictionary) {
        if let Ok(oc) = self.inner.get_dictionary(parent).and_then(|p| p.get(b"OC")) {
            d.set("OC", oc.clone());
        }
    }

    /// Reply to a comment (one undo step). Replies are shown with their comment.
    pub fn add_reply(&mut self, page: usize, index: usize, text: &str, author: &str) -> Result<ObjectId> {
        if text.trim().is_empty() {
            return Err(Error::Invalid("the reply is empty".into()));
        }
        let parent = self.annotation_object(page, index)?;
        let (rect, color) = self.comment_rect_color(parent)?;
        let page_id = self.page_id(page)?;
        self.begin_edit();
        let mut d = self.base_annotation(page_id, "Text", rect, author, text, color);
        d.set("IRT", parent);
        d.set("RT", "R");
        d.set("Name", "Comment");
        d.set("F", FLAG_PRINT | 8 | 16);
        d.set("Subj", lopdf::text_string("Reply"));
        self.follow_layer_of(parent, &mut d);
        let id = self.push_annotation(page_id, d)?;
        self.commit()?;
        Ok(id)
    }

    /// Record a review state ("Review" model: Accepted, Rejected, Cancelled,
    /// Completed, None) or a check mark ("Marked" model: Marked, Unmarked) on a
    /// comment (one undo step). `contents` describes the change.
    pub fn set_review_state(
        &mut self,
        page: usize,
        index: usize,
        model: &str,
        state: &str,
        author: &str,
        contents: &str,
    ) -> Result<()> {
        let valid = match model {
            "Review" => REVIEW_STATES.contains(&state),
            "Marked" => matches!(state, "Marked" | "Unmarked"),
            _ => false,
        };
        if !valid {
            return Err(Error::Invalid(format!("unknown state {model}/{state}")));
        }
        let parent = self.annotation_object(page, index)?;
        let (rect, color) = self.comment_rect_color(parent)?;
        let page_id = self.page_id(page)?;
        self.begin_edit();
        let mut d = self.base_annotation(page_id, "Text", rect, author, contents, color);
        d.set("IRT", parent);
        d.set("State", lopdf::text_string(state));
        d.set("StateModel", lopdf::text_string(model));
        // State changes are not drawn: only the review history lists them.
        d.set("F", FLAG_HIDDEN | FLAG_PRINT | 8 | 16);
        self.follow_layer_of(parent, &mut d);
        self.push_annotation(page_id, d)?;
        self.commit()
    }

    /// State changes of a comment, oldest first.
    pub fn review_history(&self, page: usize, index: usize) -> Result<Vec<StateChange>> {
        let parent = self.annotation_object(page, index)?;
        let mut out: Vec<(String, StateChange)> = self
            .comment_infos()
            .into_iter()
            .filter(|c| c.is_state)
            .filter_map(|c| {
                let d = self.inner.get_dictionary(c.id).ok()?;
                let irt = d.get(b"IRT").ok().and_then(|r| r.as_reference().ok())?;
                if irt != parent {
                    return None;
                }
                let state = dict_text(&self.inner, d, b"State").unwrap_or_default();
                let model = dict_text(&self.inner, d, b"StateModel").unwrap_or_else(|| "Review".into());
                Some((
                    c.modified_raw.clone(),
                    StateChange {
                        date: c.modified.clone(),
                        author: c.author.clone(),
                        model,
                        state,
                    },
                ))
            })
            .collect();
        out.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(out.into_iter().map(|(_, s)| s).collect())
    }

    /// Set or clear annotation flag bits (one undo step).
    pub fn set_annotation_flag(&mut self, page: usize, index: usize, flag: i64, on: bool) -> Result<()> {
        let id = self.annotation_object(page, index)?;
        self.begin_edit();
        let d = self.inner.get_dictionary_mut(id)?;
        let flags = flags_of(d);
        d.set("F", if on { flags | flag } else { flags & !flag });
        d.set("M", Object::string_literal(util::pdf_date_now()));
        self.commit()
    }

    /// Default place of a pop-up note: right of the comment, inside the page.
    fn default_popup_rect(&self, page: usize, rect: [f32; 4]) -> [f32; 4] {
        let size = self.page_size_of(self.page_ids()[page]);
        let c = size.crop_box;
        let (w, h) = (200.0f32.min(c[2] - c[0]), 120.0f32.min(c[3] - c[1]));
        let mut x = rect[2] + 8.0;
        if x + w > c[2] {
            x = (rect[0] - 8.0 - w).max(c[0]);
        }
        let top = rect[3].clamp(c[1] + h, c[3]);
        [x, top - h, x + w, top]
    }

    /// Open or close the pop-up note of a comment, creating it when needed.
    pub fn set_popup_open(&mut self, page: usize, index: usize, open: bool) -> Result<()> {
        let id = self.annotation_object(page, index)?;
        let existing = self
            .inner
            .get_dictionary(id)?
            .get(b"Popup")
            .ok()
            .and_then(|p| p.as_reference().ok())
            .filter(|p| self.inner.get_dictionary(*p).is_ok());
        match existing {
            Some(p) => {
                self.begin_edit();
                self.inner.get_dictionary_mut(p)?.set("Open", open);
                self.commit()
            }
            None if open => {
                let (rect, _) = self.comment_rect_color(id)?;
                let popup_rect = self.default_popup_rect(page, rect);
                let page_id = self.page_id(page)?;
                self.begin_edit();
                let mut popup = dictionary! {
                    "Type" => "Annot",
                    "Subtype" => "Popup",
                    "Rect" => popup_rect.iter().map(|v| Object::Real(*v)).collect::<Vec<_>>(),
                    "Parent" => id,
                    "Open" => true,
                    "F" => FLAG_PRINT | 8 | 16,
                };
                self.follow_layer_of(id, &mut popup);
                let pid = self.push_annotation(page_id, popup)?;
                self.inner.get_dictionary_mut(id)?.set("Popup", pid);
                self.commit()
            }
            None => Ok(()),
        }
    }

    /// Open or close the pop-up notes of every comment (replies and states
    /// excluded); `non_empty` limits opening to comments with text. Returns how
    /// many changed (one undo step).
    pub fn set_all_popups_open(&mut self, open: bool, non_empty: bool) -> Result<usize> {
        let targets: Vec<CommentInfo> = self
            .comment_infos()
            .into_iter()
            .filter(|c| c.reply_to.is_none() && !c.is_state)
            .filter(|c| !open || !non_empty || !c.contents.trim().is_empty())
            .filter(|c| c.popup.map(|(_, o, _)| o != open).unwrap_or(open))
            .collect();
        if targets.is_empty() {
            return Ok(0);
        }
        self.begin_edit();
        let ids = self.page_ids();
        for c in &targets {
            match c.popup {
                Some((p, _, _)) => {
                    self.inner.get_dictionary_mut(p)?.set("Open", open);
                }
                None => {
                    let popup_rect = self.default_popup_rect(c.page, c.rect);
                    let mut popup = dictionary! {
                        "Type" => "Annot",
                        "Subtype" => "Popup",
                        "Rect" => popup_rect.iter().map(|v| Object::Real(*v)).collect::<Vec<_>>(),
                        "Parent" => c.id,
                        "Open" => true,
                        "F" => FLAG_PRINT | 8 | 16,
                    };
                    self.follow_layer_of(c.id, &mut popup);
                    let pid = self.push_annotation(ids[c.page], popup)?;
                    self.inner.get_dictionary_mut(c.id)?.set("Popup", pid);
                }
            }
        }
        self.commit()?;
        Ok(targets.len())
    }

    /// Move a comment's pop-up note back next to the comment.
    pub fn reset_popup_location(&mut self, page: usize, index: usize) -> Result<()> {
        let id = self.annotation_object(page, index)?;
        let (rect, _) = self.comment_rect_color(id)?;
        let r = self.default_popup_rect(page, rect);
        self.set_popup_rect(page, index, r)
    }

    /// Give a comment's pop-up note a new rectangle (one undo step).
    pub fn set_popup_rect(&mut self, page: usize, index: usize, rect: [f32; 4]) -> Result<()> {
        let id = self.annotation_object(page, index)?;
        let p = self
            .inner
            .get_dictionary(id)?
            .get(b"Popup")
            .ok()
            .and_then(|p| p.as_reference().ok())
            .ok_or_else(|| Error::Invalid("the comment has no pop-up note".into()))?;
        let n = crate::geom::normalize(rect);
        self.begin_edit();
        self.inner
            .get_dictionary_mut(p)?
            .set("Rect", n.iter().map(|v| Object::Real(*v)).collect::<Vec<_>>());
        self.commit()
    }

    /// Swap the two ends of a line comment (and its line endings).
    pub fn flip_line(&mut self, page: usize, index: usize) -> Result<()> {
        let id = self.annotation_object(page, index)?;
        let mut d = self.inner.get_dictionary(id)?.clone();
        let l: Vec<Object> = d
            .get(b"L")
            .ok()
            .and_then(|l| l.as_array().ok())
            .cloned()
            .ok_or_else(|| Error::Invalid("the comment is not a line".into()))?;
        if l.len() < 4 {
            return Err(Error::Invalid("the comment is not a line".into()));
        }
        d.set("L", vec![l[2].clone(), l[3].clone(), l[0].clone(), l[1].clone()]);
        if let Ok(Object::Array(le)) = d.get(b"LE") {
            if le.len() == 2 {
                let swapped = vec![le[1].clone(), le[0].clone()];
                d.set("LE", swapped);
            }
        }
        d.set("M", Object::string_literal(util::pdf_date_now()));
        if let Some(problem) = crate::appearance::text_problem(&self.inner, &d) {
            return Err(problem.into());
        }
        self.edit_step(|doc| {
            doc.generate_appearance(&mut d)?;
            *doc.inner.get_dictionary_mut(id)? = d;
            Ok(())
        })
    }

    /// Serialized document in which the given annotations are hidden, for
    /// displaying a filtered view without changing the document.
    pub fn bytes_hiding(&self, ids: &HashSet<ObjectId>) -> Result<Arc<Vec<u8>>> {
        if ids.is_empty() {
            return Ok(self.bytes());
        }
        let mut copy = self.inner.clone();
        hide_annotations_in(&mut copy, ids);
        let mut out = Vec::new();
        copy.save_to(&mut out)?;
        Ok(Arc::new(out))
    }
}

/// Set the hidden flag of annotations (and close their pop-up notes).
pub(crate) fn hide_annotations_in(copy: &mut lopdf::Document, ids: &HashSet<ObjectId>) {
    for id in ids {
        if let Ok(d) = copy.get_dictionary_mut(*id) {
            let flags = flags_of(d);
            d.set("F", flags | FLAG_HIDDEN);
            // Pop-up notes of hidden comments close as well.
            if let Ok(p) = d.get(b"Popup").and_then(|p| p.as_reference()) {
                if let Ok(pd) = copy.get_dictionary_mut(p) {
                    pd.set("Open", false);
                }
            }
        }
    }
}
