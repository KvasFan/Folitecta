//! Page-level operations: rotate, delete, insert, reorder, extract, merge, crop, resize.

use std::collections::HashMap;
use std::sync::atomic::AtomicBool;

use lopdf::{Dictionary, Object, ObjectId, Stream, dictionary};

use crate::delivery::LeftOut;
use crate::document::Document;
use crate::error::{Error, Result};
use crate::util::{deref, fmt_num, rect_of, text_of};

/// The entries of a form field (not of its widgets).
const FIELD_KEYS: [&[u8]; 13] = [
    b"T", b"TU", b"TM", b"FT", b"Ff", b"V", b"DV", b"Opt", b"TI", b"I", b"MaxLen", b"Lock", b"SV",
];

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

    /// What deleting `pages` takes with them, nothing changed: the bookmarks,
    /// links and named destinations leading to them, their form fields and
    /// tags, and the rest [`LeftOut`] counts.
    pub fn deleting_pages_takes(&self, pages: &[usize]) -> Result<LeftOut> {
        self.check_pages(pages)?;
        let kept: Vec<usize> = (0..self.page_count()).filter(|p| !pages.contains(p)).collect();
        if kept.is_empty() {
            return Err(Error::LastPage);
        }
        let mut copy = self.derived()?;
        copy.changing();
        copy.keep_only_pages(&kept, &AtomicBool::new(false))
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

    /// Duplicate pages; each copy is inserted right after its source. The
    /// comments of a copy are its own (their pop-ups and replies among
    /// them), and its form fields are more widgets of the same fields, so
    /// both pages show the same values. A copy is not tagged.
    pub fn duplicate_pages(&mut self, pages: &[usize]) -> Result<()> {
        self.check_pages(pages)?;
        self.begin_edit();
        let ids = self.page_ids();
        self.inherit_attributes(&ids);
        let mut order = Vec::with_capacity(ids.len() + pages.len());
        for (i, id) in ids.iter().enumerate() {
            order.push(*id);
            if pages.contains(&i) {
                match self.copy_page(*id) {
                    Ok(copy) => order.push(copy),
                    Err(e) => {
                        self.abandon_edit();
                        return Err(e);
                    }
                }
            }
        }
        self.set_page_order(order)?;
        self.commit()
    }

    /// A copy of the page `id` (not in the page tree yet) with copies of
    /// its annotations; see [`duplicate_pages`](Self::duplicate_pages).
    fn copy_page(&mut self, id: ObjectId) -> Result<ObjectId> {
        let mut page = self.inner.get_dictionary(id)?.clone();
        page.remove(b"StructParents");
        let new_page = self.inner.new_object_id();
        let annots: Vec<Object> = match page.get(b"Annots") {
            Ok(a) => deref(&self.inner, a).as_array().cloned().unwrap_or_default(),
            Err(_) => Vec::new(),
        };
        // Every annotation of the page gets its copy first, so they can
        // point to each other.
        let mut copies: HashMap<ObjectId, ObjectId> = HashMap::new();
        for a in &annots {
            if let Ok(r) = a.as_reference() {
                copies.insert(r, self.inner.new_object_id());
            }
        }
        let mut list = Vec::with_capacity(annots.len());
        for a in annots {
            let Ok(old) = a.as_reference() else {
                list.push(a);
                continue;
            };
            let Ok(mut d) = self.inner.get_dictionary(old).cloned() else {
                continue;
            };
            let new = copies[&old];
            d.set("P", Object::Reference(new_page));
            d.remove(b"StructParent");
            for key in [&b"Popup"[..], b"IRT", b"Parent"] {
                if let Some(c) = d
                    .get(key)
                    .ok()
                    .and_then(|o| o.as_reference().ok())
                    .and_then(|r| copies.get(&r))
                {
                    d.set(key.to_vec(), Object::Reference(*c));
                }
            }
            if let Some(nm) = d.get(b"NM").ok().and_then(|n| text_of(&self.inner, n)) {
                d.set("NM", lopdf::text_string(&format!("{nm}-{}", new.0)));
            }
            let widget = d.get(b"Subtype").ok().and_then(|s| s.as_name().ok()) == Some(b"Widget".as_slice());
            if widget {
                let field = match d.get(b"Parent").ok().and_then(|p| p.as_reference().ok()) {
                    Some(f) if !copies.values().any(|c| *c == f) => Some(f),
                    Some(_) => None,
                    None => self.split_field_widget(old)?,
                };
                if let Some(field) = field {
                    for key in FIELD_KEYS {
                        d.remove(key);
                    }
                    // The actions of the widget, without those of the field.
                    match self.inner.get_dictionary(old)?.get(b"AA").ok().cloned() {
                        Some(aa) => d.set("AA", aa),
                        None => {
                            d.remove(b"AA");
                        }
                    }
                    d.set("Parent", Object::Reference(field));
                    let f = self.inner.get_dictionary_mut(field)?;
                    let mut kids = f
                        .get(b"Kids")
                        .ok()
                        .and_then(|k| k.as_array().ok())
                        .cloned()
                        .unwrap_or_default();
                    kids.push(Object::Reference(new));
                    f.set("Kids", kids);
                }
            }
            self.inner.objects.insert(new, Object::Dictionary(d));
            list.push(Object::Reference(new));
        }
        if !list.is_empty() {
            page.set("Annots", Object::Array(list));
        }
        self.inner.objects.insert(new_page, Object::Dictionary(page));
        Ok(new_page)
    }

    /// A widget that is its own field (`/T` with no parent of its own)
    /// becomes the first kid of a new field that takes the field's entries,
    /// in its place in the form. Returns the field, or `None` for a widget
    /// that is in no field.
    fn split_field_widget(&mut self, widget: ObjectId) -> Result<Option<ObjectId>> {
        let d = self.inner.get_dictionary(widget)?.clone();
        if !d.has(b"T") && !d.has(b"FT") {
            return Ok(None);
        }
        let mut field = Dictionary::new();
        for key in FIELD_KEYS {
            if let Ok(v) = d.get(key) {
                field.set(key.to_vec(), v.clone());
            }
        }
        // Text drawing is the field's too, and stays with the widget.
        for key in [&b"DA"[..], b"Q"] {
            if let Ok(v) = d.get(key) {
                field.set(key.to_vec(), v.clone());
            }
        }
        if let Some(aa) = d
            .get(b"AA")
            .ok()
            .and_then(|a| deref(&self.inner, a).as_dict().ok())
            .cloned()
        {
            let (mut mine, mut theirs) = (Dictionary::new(), Dictionary::new());
            for (k, v) in aa.iter() {
                if [&b"K"[..], b"F", b"V", b"C"].contains(&k.as_slice()) {
                    theirs.set(k.clone(), v.clone());
                } else {
                    mine.set(k.clone(), v.clone());
                }
            }
            if !theirs.is_empty() {
                field.set("AA", theirs);
            }
            let w = self.inner.get_dictionary_mut(widget)?;
            if mine.is_empty() {
                w.remove(b"AA");
            } else {
                w.set("AA", mine);
            }
        }
        field.set("Kids", vec![Object::Reference(widget)]);
        let field_id = self.inner.add_object(field);
        let w = self.inner.get_dictionary_mut(widget)?;
        for key in FIELD_KEYS {
            w.remove(key);
        }
        w.set("Parent", Object::Reference(field_id));
        // The new field takes the widget's place in the form.
        let replace = |list: &mut Vec<Object>| {
            for o in list.iter_mut() {
                if o.as_reference().ok() == Some(widget) {
                    *o = Object::Reference(field_id);
                }
            }
        };
        let form = self.inner.catalog()?.get(b"AcroForm").ok().cloned();
        let form_dict = match form {
            Some(Object::Reference(r)) => self.inner.get_dictionary_mut(r).ok(),
            Some(Object::Dictionary(_)) => self
                .inner
                .catalog_mut()?
                .get_mut(b"AcroForm")
                .ok()
                .and_then(|f| f.as_dict_mut().ok()),
            _ => None,
        };
        if let Some(f) = form_dict {
            for key in [&b"Fields"[..], b"CO"] {
                match f.get(key).ok().cloned() {
                    Some(Object::Array(mut a)) => {
                        replace(&mut a);
                        f.set(key.to_vec(), a);
                    }
                    Some(Object::Reference(r)) => {
                        if let Ok(Object::Array(a)) = self.inner.get_object_mut(r) {
                            replace(a);
                        }
                        break;
                    }
                    _ => {}
                }
            }
        }
        Ok(Some(field_id))
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
        self.insert_pages_from(at, other, pages, None).map(|_| ())
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
