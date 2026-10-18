//! Groups of comments: comments chosen, moved and deleted together. A group
//! is written the way PDF says: every comment of it but the first points to
//! the first with `/IRT` and says `/RT /Group`; the first one stands for the
//! group (its text, author and state are the group's).

use std::collections::HashSet;

use lopdf::{Object, ObjectId};

use crate::annots::{NON_COMMENT_SUBTYPES, annot_refs};
use crate::document::Document;
use crate::error::{Error, Result};
use crate::marks::pdf_rect_to_display;
use crate::textops::display_rect_to_pdf;
use crate::util::rect_of;

/// What a comment of a page is to the groups: its object, its subtype and,
/// for a member of a group, the object of the group's first comment.
struct Item {
    id: Option<ObjectId>,
    subtype: String,
    head: Option<ObjectId>,
}

impl Document {
    fn group_items(&self, page: usize) -> Result<Vec<Item>> {
        let page_id = self.page_id(page)?;
        let dict = self.inner.get_dictionary(page_id)?;
        Ok(annot_refs(&self.inner, dict)
            .into_iter()
            .map(|(id, d)| {
                let name = |k: &[u8]| {
                    d.get(k)
                        .ok()
                        .and_then(|o| o.as_name().ok())
                        .map(|n| String::from_utf8_lossy(n).to_string())
                };
                let grouped = name(b"RT").as_deref() == Some("Group");
                Item {
                    id,
                    subtype: name(b"Subtype").unwrap_or_default(),
                    head: d
                        .get(b"IRT")
                        .ok()
                        .and_then(|o| o.as_reference().ok())
                        .filter(|_| grouped),
                }
            })
            .collect())
    }

    /// The comments of the group of comment `index` (positions in the
    /// page's `/Annots`), `index` first; just `index` when it is in no group.
    pub fn annotation_group(&self, page: usize, index: usize) -> Vec<usize> {
        let Ok(items) = self.group_items(page) else {
            return vec![index];
        };
        let Some(item) = items.get(index) else {
            return vec![index];
        };
        let Some(head) = item.head.or(item.id) else {
            return vec![index];
        };
        let mut out = vec![index];
        for (i, it) in items.iter().enumerate() {
            if i != index && (it.id == Some(head) || it.head == Some(head)) {
                out.push(i);
            }
        }
        out
    }

    /// Whether comment `index` is in a group with others.
    pub fn is_grouped(&self, page: usize, index: usize) -> bool {
        self.annotation_group(page, index).len() > 1
    }

    /// Make one group of the comments `indices` of a page and of the groups
    /// they are in (one undo step). The group's first comment is the first
    /// of `indices` (or the first of its group). Returns the comments of
    /// the group, the first one first.
    pub fn group_annotations(&mut self, page: usize, indices: &[usize]) -> Result<Vec<usize>> {
        let items = self.group_items(page)?;
        let mut all: Vec<usize> = Vec::new();
        for &i in indices {
            for j in self.annotation_group(page, i) {
                if !all.contains(&j) {
                    all.push(j);
                }
            }
        }
        for &i in &all {
            let it = items
                .get(i)
                .ok_or_else(|| Error::Invalid(format!("annotation {i} does not exist")))?;
            if it.id.is_none() || NON_COMMENT_SUBTYPES.contains(&it.subtype.as_str()) {
                return Err(Error::Invalid("only comments can be grouped".into()));
            }
        }
        if all.len() < 2 {
            return Err(Error::Invalid("choose two comments or more to group".into()));
        }
        let first = &items[all[0]];
        let head_id = first.head.or(first.id).unwrap_or((0, 0));
        let head = items
            .iter()
            .position(|it| it.id == Some(head_id))
            .unwrap_or(all[0]);
        self.begin_edit();
        for &i in &all {
            if i == head {
                continue;
            }
            let Some(id) = items[i].id else { continue };
            let d = self.inner.get_dictionary_mut(id)?;
            d.set("IRT", Object::Reference(head_id));
            d.set("RT", Object::Name(b"Group".to_vec()));
        }
        // The first comment of a group is in no other group.
        let d = self.inner.get_dictionary_mut(head_id)?;
        if d.get(b"RT").ok().and_then(|o| o.as_name().ok()) == Some(b"Group".as_slice()) {
            d.remove(b"IRT");
            d.remove(b"RT");
        }
        self.commit()?;
        let mut out = vec![head];
        out.extend(all.into_iter().filter(|i| *i != head));
        Ok(out)
    }

    /// Break up the groups the comments `indices` of a page are in (one
    /// undo step). Returns how many comments left a group.
    pub fn ungroup_annotations(&mut self, page: usize, indices: &[usize]) -> Result<usize> {
        let items = self.group_items(page)?;
        let mut heads: HashSet<ObjectId> = HashSet::new();
        for &i in indices {
            if let Some(it) = items.get(i) {
                if let Some(h) = it.head.or(it.id) {
                    heads.insert(h);
                }
            }
        }
        let members: Vec<ObjectId> = items
            .iter()
            .filter(|it| it.head.is_some_and(|h| heads.contains(&h)))
            .filter_map(|it| it.id)
            .collect();
        if members.is_empty() {
            return Ok(0);
        }
        self.begin_edit();
        for id in &members {
            let d = self.inner.get_dictionary_mut(*id)?;
            d.remove(b"IRT");
            d.remove(b"RT");
        }
        self.commit()?;
        Ok(members.len())
    }

    /// Move the comments `indices` of a page by `(dx, dy)` points as the
    /// page is shown (y down), in one undo step.
    pub fn shift_annotations(&mut self, page: usize, indices: &[usize], dx: f32, dy: f32) -> Result<()> {
        let size = self.page_size(page)?;
        let mut moves: Vec<(ObjectId, [f32; 4])> = Vec::new();
        for &i in indices {
            let (_, aid) = self.annotation_ref(page, i)?;
            let Some(aid) = aid else { continue };
            let Some(rect) = self
                .inner
                .get_dictionary(aid)?
                .get(b"Rect")
                .ok()
                .and_then(|r| rect_of(&self.inner, r))
            else {
                continue;
            };
            let d = pdf_rect_to_display(&size, rect);
            let moved = [d[0] + dx, d[1] + dy, d[2] + dx, d[3] + dy];
            moves.push((aid, crate::geom::normalize(display_rect_to_pdf(&size, moved))));
        }
        if moves.is_empty() {
            return Ok(());
        }
        self.begin_edit();
        for (aid, r) in moves {
            self.reshape_annotation(aid, r)?;
        }
        self.commit()
    }

    /// Delete the comments `indices` of a page, with their popups, replies
    /// and the comments grouped under them, in one undo step.
    pub fn delete_annotations(&mut self, page: usize, indices: &[usize]) -> Result<usize> {
        let page_id = self.page_id(page)?;
        let items = self.group_items(page)?;
        let roots: HashSet<ObjectId> = indices
            .iter()
            .filter_map(|i| items.get(*i).and_then(|it| it.id))
            .collect();
        if roots.is_empty() {
            return Ok(0);
        }
        self.begin_edit();
        self.remove_annotations(page_id, &roots);
        self.commit()?;
        Ok(roots.len())
    }
}
