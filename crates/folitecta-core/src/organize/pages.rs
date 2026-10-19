//! Page-level operations: rotate, delete, insert, reorder, extract, merge, crop, resize.

use lopdf::{Dictionary, Object, ObjectId, Stream, dictionary};

use crate::document::Document;
use crate::error::{Error, Result};
use crate::util::{fmt_num, rect_of};

impl Document {
    fn check_pages(&self, pages: &[usize]) -> Result<()> {
        let n = self.page_count();
        for &p in pages {
            if p >= n {
                return Err(Error::InvalidPage(p));
            }
        }
        Ok(())
    }

    /// Rotate pages by `degrees` (multiple of 90, positive = clockwise).
    pub fn rotate_pages(&mut self, pages: &[usize], degrees: i32) -> Result<()> {
        self.check_pages(pages)?;
        self.begin_edit();
        let ids = self.page_ids();
        for &p in pages {
            let id = ids[p];
            let current = self
                .inherited_attr(id, b"Rotate")
                .and_then(|o| o.as_i64().ok())
                .unwrap_or(0) as i32;
            let next = (((current + degrees) % 360) + 360) % 360;
            self.inner
                .get_dictionary_mut(id)?
                .set("Rotate", Object::Integer(next as i64));
        }
        self.commit()
    }

    /// Delete the given pages (zero-based). At least one page must remain.
    pub fn delete_pages(&mut self, pages: &[usize]) -> Result<()> {
        self.check_pages(pages)?;
        let ids = self.page_ids();
        let keep: Vec<ObjectId> = ids
            .iter()
            .enumerate()
            .filter(|(i, _)| !pages.contains(i))
            .map(|(_, id)| *id)
            .collect();
        if keep.is_empty() {
            return Err(Error::LastPage);
        }
        self.begin_edit();
        self.set_page_order(keep)?;
        self.commit()
    }

    /// Insert `count` blank pages before index `at` (`at == page_count` appends).
    pub fn insert_blank_pages(&mut self, at: usize, count: usize, width: f32, height: f32) -> Result<()> {
        let n = self.page_count();
        if at > n {
            return Err(Error::InvalidPage(at));
        }
        self.begin_edit();
        let mut ids = self.page_ids();
        let mut new_ids = Vec::with_capacity(count.max(1));
        for _ in 0..count.max(1) {
            let content_id = self.inner.add_object(Stream::new(dictionary! {}, Vec::new()));
            let page_id = self.inner.add_object(dictionary! {
                "Type" => "Page",
                "MediaBox" => vec![0.into(), 0.into(), width.into(), height.into()],
                "Contents" => content_id,
                "Resources" => dictionary! {},
            });
            new_ids.push(page_id);
        }
        let tail = ids.split_off(at);
        ids.extend(new_ids);
        ids.extend(tail);
        self.set_page_order(ids)?;
        self.commit()
    }

    /// Duplicate pages; each copy is inserted right after its source.
    pub fn duplicate_pages(&mut self, pages: &[usize]) -> Result<()> {
        self.check_pages(pages)?;
        self.begin_edit();
        let ids = self.page_ids();
        self.inherit_attributes(&ids);
        let mut order = Vec::with_capacity(ids.len() + pages.len());
        for (i, id) in ids.iter().enumerate() {
            order.push(*id);
            if pages.contains(&i) {
                let mut copy = self.inner.get_dictionary(*id)?.clone();
                // Give the copy its own annotation objects so edits do not leak between pages.
                if let Ok(annots) = copy.get(b"Annots").cloned() {
                    let resolved = match &annots {
                        Object::Reference(r) => {
                            self.inner.get_object(*r).ok().cloned().unwrap_or(annots.clone())
                        }
                        other => other.clone(),
                    };
                    if let Object::Array(items) = resolved {
                        let mut new_items = Vec::with_capacity(items.len());
                        for item in items {
                            match item {
                                Object::Reference(r) => {
                                    if let Ok(obj) = self.inner.get_object(r).cloned() {
                                        new_items.push(Object::Reference(self.inner.add_object(obj)));
                                    }
                                }
                                other => new_items.push(other),
                            }
                        }
                        copy.set("Annots", Object::Array(new_items));
                    }
                }
                let new_id = self.inner.add_object(copy);
                order.push(new_id);
            }
        }
        self.set_page_order(order)?;
        self.commit()
    }

    /// Move the given pages so that they start at index `to` (computed on the remaining pages).
    pub fn move_pages(&mut self, pages: &[usize], to: usize) -> Result<()> {
        self.check_pages(pages)?;
        self.begin_edit();
        let ids = self.page_ids();
        let mut moving: Vec<usize> = pages.to_vec();
        moving.sort_unstable();
        moving.dedup();
        let moved: Vec<ObjectId> = moving.iter().map(|&i| ids[i]).collect();
        let mut rest: Vec<ObjectId> = ids
            .iter()
            .enumerate()
            .filter(|(i, _)| !moving.contains(i))
            .map(|(_, id)| *id)
            .collect();
        let to = to.min(rest.len());
        let tail = rest.split_off(to);
        rest.extend(moved);
        rest.extend(tail);
        self.set_page_order(rest)?;
        self.commit()
    }

    /// Swap two pages.
    pub fn swap_pages(&mut self, a: usize, b: usize) -> Result<()> {
        self.check_pages(&[a, b])?;
        if a == b {
            return Ok(());
        }
        self.begin_edit();
        let mut ids = self.page_ids();
        ids.swap(a, b);
        self.set_page_order(ids)?;
        self.commit()
    }

    /// Reverse the page order.
    pub fn reverse_pages(&mut self) -> Result<()> {
        self.begin_edit();
        let mut ids = self.page_ids();
        ids.reverse();
        self.set_page_order(ids)?;
        self.commit()
    }

    /// Reorder pages explicitly (a permutation of `0..page_count`).
    pub fn reorder_pages(&mut self, order: &[usize]) -> Result<()> {
        let n = self.page_count();
        let mut seen = vec![false; n];
        for &p in order {
            if p >= n || seen[p] {
                return Err(Error::Invalid("invalid page order".into()));
            }
            seen[p] = true;
        }
        if order.len() != n {
            return Err(Error::Invalid("page order must list every page once".into()));
        }
        self.begin_edit();
        let ids = self.page_ids();
        self.set_page_order(order.iter().map(|&i| ids[i]).collect())?;
        self.commit()
    }

    /// Build a new document that contains only the given pages (in the given
    /// order), protected like this document when it is.
    pub fn extract_pages(&self, pages: &[usize]) -> Result<Document> {
        self.check_pages(pages)?;
        if pages.is_empty() {
            return Err(Error::LastPage);
        }
        let mut copy = self.derived()?;
        let ids = copy.page_ids();
        let order: Vec<ObjectId> = pages.iter().map(|&p| ids[p]).collect();
        copy.set_page_order(order)?;
        copy.set_dirty_new();
        copy.commit()?;
        Ok(copy)
    }

    /// Split the document into chunks of `pages_per_doc` pages.
    pub fn split_every(&self, pages_per_doc: usize) -> Result<Vec<Document>> {
        let n = self.page_count();
        let step = pages_per_doc.max(1);
        let mut out = Vec::new();
        let mut start = 0;
        while start < n {
            let end = (start + step).min(n);
            let pages: Vec<usize> = (start..end).collect();
            out.push(self.extract_pages(&pages)?);
            start = end;
        }
        Ok(out)
    }

    /// Insert pages from another document before index `at`. `pages` selects
    /// zero-based pages of `other`; `None` imports every page. Pages of a
    /// protected document are refused by a document that is not protected.
    pub fn insert_document(&mut self, at: usize, other: &Document, pages: Option<&[usize]>) -> Result<()> {
        self.check_import_from(other)?;
        let n = self.page_count();
        if at > n {
            return Err(Error::InvalidPage(at));
        }
        let mut src = lopdf::Document::load_mem(other.bytes().as_slice())?;
        src.renumber_objects_with(self.inner.max_id + 1);
        let src_pages: Vec<ObjectId> = src.get_pages().into_values().collect();
        let selected: Vec<ObjectId> = match pages {
            Some(sel) => {
                for &p in sel {
                    if p >= src_pages.len() {
                        return Err(Error::InvalidPage(p));
                    }
                }
                sel.iter().map(|&p| src_pages[p]).collect()
            }
            None => src_pages.clone(),
        };
        if selected.is_empty() {
            return Ok(());
        }
        self.begin_edit();
        let max_id = src.max_id;
        for (id, obj) in std::mem::take(&mut src.objects) {
            self.inner.objects.insert(id, obj);
        }
        self.inner.max_id = self.inner.max_id.max(max_id);
        // Resolve attributes inherited inside the foreign page tree before re-parenting.
        self.inherit_attributes(&selected);
        let mut ids = self.page_ids();
        let tail = ids.split_off(at);
        ids.extend(selected);
        ids.extend(tail);
        self.set_page_order(ids)?;
        self.commit()
    }

    /// Set the crop box of pages (PDF user-space rectangle).
    pub fn crop_pages(&mut self, pages: &[usize], rect: [f32; 4]) -> Result<()> {
        self.check_pages(pages)?;
        self.begin_edit();
        let ids = self.page_ids();
        for &p in pages {
            let size = self.page_size_of(ids[p]);
            let m = size.media_box;
            let x0 = rect[0].max(m[0]);
            let y0 = rect[1].max(m[1]);
            let x1 = rect[2].min(m[2]);
            let y1 = rect[3].min(m[3]);
            if x1 - x0 < 1.0 || y1 - y0 < 1.0 {
                return Err(Error::Invalid("crop rectangle is empty".into()));
            }
            self.inner.get_dictionary_mut(ids[p])?.set(
                "CropBox",
                Object::Array(vec![x0.into(), y0.into(), x1.into(), y1.into()]),
            );
        }
        self.commit()
    }

    /// Crop by margins expressed in points from each edge of the current page box.
    pub fn crop_pages_by_margins(
        &mut self,
        pages: &[usize],
        left: f32,
        top: f32,
        right: f32,
        bottom: f32,
    ) -> Result<()> {
        self.check_pages(pages)?;
        self.begin_edit();
        let ids = self.page_ids();
        for &p in pages {
            let size = self.page_size_of(ids[p]);
            let c = size.crop_box;
            let rect = [c[0] + left, c[1] + bottom, c[2] - right, c[3] - top];
            if rect[2] - rect[0] < 1.0 || rect[3] - rect[1] < 1.0 {
                return Err(Error::Invalid("crop margins leave no page area".into()));
            }
            self.inner.get_dictionary_mut(ids[p])?.set(
                "CropBox",
                Object::Array(vec![
                    rect[0].into(),
                    rect[1].into(),
                    rect[2].into(),
                    rect[3].into(),
                ]),
            );
        }
        self.commit()
    }

    /// Remove the crop box so the whole media box is shown again.
    pub fn reset_crop(&mut self, pages: &[usize]) -> Result<()> {
        self.check_pages(pages)?;
        self.begin_edit();
        let ids = self.page_ids();
        for &p in pages {
            self.inner.get_dictionary_mut(ids[p])?.remove(b"CropBox");
        }
        self.commit()
    }

    /// Resize pages to `width` x `height` points. When `scale_content` is set the
    /// existing content is scaled to fit and centred; otherwise it is kept at its
    /// original size, anchored at the bottom-left corner.
    pub fn resize_pages(
        &mut self,
        pages: &[usize],
        width: f32,
        height: f32,
        scale_content: bool,
    ) -> Result<()> {
        self.check_pages(pages)?;
        if width < 1.0 || height < 1.0 {
            return Err(Error::Invalid("page size must be positive".into()));
        }
        self.begin_edit();
        let ids = self.page_ids();
        for &p in pages {
            let id = ids[p];
            let size = self.page_size_of(id);
            let m = size.media_box;
            let (pw, ph) = (m[2] - m[0], m[3] - m[1]);
            let (sx, sy, tx, ty) = if scale_content {
                let s = (width / pw).min(height / ph);
                (
                    s,
                    s,
                    (width - pw * s) / 2.0 - m[0] * s,
                    (height - ph * s) / 2.0 - m[1] * s,
                )
            } else {
                (1.0, 1.0, -m[0], -m[1])
            };
            let old = self.inner.get_page_content(id);
            let mut content = format!(
                "q {} 0 0 {} {} {} cm\n",
                fmt_num(sx),
                fmt_num(sy),
                fmt_num(tx),
                fmt_num(ty)
            )
            .into_bytes();
            content.extend_from_slice(&old);
            content.extend_from_slice(b"\nQ\n");
            self.inner.change_page_content(id, content)?;
            let page = self.inner.get_dictionary_mut(id)?;
            page.set(
                "MediaBox",
                Object::Array(vec![0.into(), 0.into(), width.into(), height.into()]),
            );
            page.remove(b"CropBox");
            // Transform annotation rectangles too.
            if let Ok(annots) = page.get(b"Annots").cloned() {
                let annot_ids: Vec<ObjectId> = match &annots {
                    Object::Array(arr) => arr.iter().filter_map(|a| a.as_reference().ok()).collect(),
                    Object::Reference(r) => self
                        .inner
                        .get_object(*r)
                        .ok()
                        .and_then(|o| o.as_array().ok())
                        .map(|arr| arr.iter().filter_map(|a| a.as_reference().ok()).collect())
                        .unwrap_or_default(),
                    _ => Vec::new(),
                };
                for aid in annot_ids {
                    let rect = self
                        .inner
                        .get_dictionary(aid)
                        .ok()
                        .and_then(|d| d.get(b"Rect").ok().cloned());
                    if let Some(rect_obj) = rect {
                        if let Some(r) = rect_of(&self.inner, &rect_obj) {
                            let nr = [r[0] * sx + tx, r[1] * sy + ty, r[2] * sx + tx, r[3] * sy + ty];
                            if let Ok(d) = self.inner.get_dictionary_mut(aid) {
                                d.set("Rect", Object::Array(nr.iter().map(|v| (*v).into()).collect()));
                            }
                        }
                    }
                }
            }
        }
        self.commit()
    }

    /// Grow the media box by the given margins (points); content keeps its position.
    pub fn add_margins(
        &mut self,
        pages: &[usize],
        left: f32,
        top: f32,
        right: f32,
        bottom: f32,
    ) -> Result<()> {
        self.check_pages(pages)?;
        self.begin_edit();
        let ids = self.page_ids();
        for &p in pages {
            let id = ids[p];
            let m = self.page_size_of(id).media_box;
            let nm = [m[0] - left, m[1] - bottom, m[2] + right, m[3] + top];
            if nm[2] - nm[0] < 1.0 || nm[3] - nm[1] < 1.0 {
                return Err(Error::Invalid("margins leave no page area".into()));
            }
            let page = self.inner.get_dictionary_mut(id)?;
            page.set(
                "MediaBox",
                Object::Array(nm.iter().map(|v| (*v).into()).collect()),
            );
            page.remove(b"CropBox");
        }
        self.commit()
    }

    /// Replace the content of pages with nothing (keeps annotations and size).
    pub fn clear_page_content(&mut self, pages: &[usize]) -> Result<()> {
        self.check_pages(pages)?;
        self.begin_edit();
        let ids = self.page_ids();
        for &p in pages {
            self.inner.change_page_content(ids[p], Vec::new())?;
        }
        self.commit()
    }

    pub(crate) fn set_dirty_new(&mut self) {
        self.set_path(None);
    }

    /// Pages that draw nothing (no text, image or path) and carry no annotations.
    pub fn empty_pages(&self) -> Vec<usize> {
        let layers = self.text_layers();
        self.page_ids()
            .iter()
            .enumerate()
            .filter(|(i, id)| {
                let blank = layers.get(*i).map(|l| l.ink_bbox.is_none()).unwrap_or(false);
                let annots = self
                    .inner
                    .get_dictionary(**id)
                    .ok()
                    .and_then(|d| d.get(b"Annots").ok())
                    .map(|a| match crate::util::deref(&self.inner, a) {
                        Object::Array(arr) => !arr.is_empty(),
                        _ => false,
                    })
                    .unwrap_or(false);
                blank && !annots
            })
            .map(|(i, _)| i)
            .collect()
    }

    /// Page dictionary (read-only) for inspection.
    pub fn page_dict(&self, index: usize) -> Result<&Dictionary> {
        let id = self.page_id(index)?;
        Ok(self.inner.get_dictionary(id)?)
    }
}
