//! Copying, pasting, moving and resizing annotations.

use std::collections::{BTreeMap, HashSet};

use lopdf::{Dictionary, Object, ObjectId};

use crate::document::Document;
use crate::error::{Error, Result};
use crate::util::{num, rect_of};

/// A self-contained copy of an annotation and every object it references
/// (appearance streams, fonts, graphics states), detached from any document.
#[derive(Debug, Clone, PartialEq)]
pub struct AnnotSnapshot {
    root: Dictionary,
    objects: BTreeMap<ObjectId, Object>,
}

/// Keys that point back into the source page tree or to other annotations.
const SKIP_KEYS: &[&[u8]] = &[b"P", b"Popup", b"Parent", b"IRT", b"StructParent"];

fn collect_refs(obj: &Object, out: &mut Vec<ObjectId>) {
    match obj {
        Object::Reference(r) => out.push(*r),
        Object::Array(a) => a.iter().for_each(|o| collect_refs(o, out)),
        Object::Dictionary(d) => d.iter().for_each(|(_, v)| collect_refs(v, out)),
        Object::Stream(s) => s.dict.iter().for_each(|(_, v)| collect_refs(v, out)),
        _ => {}
    }
}

fn remap(obj: &mut Object, map: &BTreeMap<ObjectId, ObjectId>) {
    match obj {
        Object::Reference(r) => {
            if let Some(n) = map.get(r) {
                *r = *n;
            }
        }
        Object::Array(a) => a.iter_mut().for_each(|o| remap(o, map)),
        Object::Dictionary(d) => d.iter_mut().for_each(|(_, v)| remap(v, map)),
        Object::Stream(s) => s.dict.iter_mut().for_each(|(_, v)| remap(v, map)),
        _ => {}
    }
}

/// Apply `f` to every (x, y) pair of an annotation's geometry entries.
fn map_geometry(dict: &mut Dictionary, f: &dyn Fn(f32, f32) -> (f32, f32)) {
    let map_flat = |arr: &mut Vec<Object>| {
        let mut i = 0;
        while i + 1 < arr.len() {
            if let (Some(x), Some(y)) = (num(&arr[i]), num(&arr[i + 1])) {
                let (nx, ny) = f(x, y);
                arr[i] = nx.into();
                arr[i + 1] = ny.into();
            }
            i += 2;
        }
    };
    for key in [&b"QuadPoints"[..], b"L", b"Vertices", b"CL"] {
        if let Ok(Object::Array(arr)) = dict.get_mut(key) {
            map_flat(arr);
        }
    }
    if let Ok(Object::Array(paths)) = dict.get_mut(b"InkList") {
        for p in paths.iter_mut() {
            if let Object::Array(arr) = p {
                map_flat(arr);
            }
        }
    }
}

impl Document {
    pub(crate) fn annotation_ref(&self, page: usize, index: usize) -> Result<(ObjectId, Option<ObjectId>)> {
        let page_id = self.page_id(page)?;
        let annots = self.inner.get_dictionary(page_id)?.get(b"Annots").ok().cloned();
        let arr = match annots {
            Some(Object::Array(a)) => a,
            Some(Object::Reference(r)) => self.inner.get_object(r)?.as_array()?.clone(),
            _ => Vec::new(),
        };
        match arr.get(index) {
            Some(Object::Reference(r)) => Ok((page_id, Some(*r))),
            Some(_) => Ok((page_id, None)),
            None => Err(Error::Invalid(format!("annotation {index} does not exist"))),
        }
    }

    /// Detached copy of an annotation.
    pub fn annotation_snapshot(&self, page: usize, index: usize) -> Result<AnnotSnapshot> {
        let (_, aid) = self.annotation_ref(page, index)?;
        let aid = aid.ok_or_else(|| Error::Invalid("inline annotations cannot be copied".into()))?;
        let mut root = self.inner.get_dictionary(aid)?.clone();
        for key in SKIP_KEYS {
            root.remove(key);
        }
        let mut objects = BTreeMap::new();
        let mut queue = Vec::new();
        collect_refs(&Object::Dictionary(root.clone()), &mut queue);
        let mut seen = HashSet::new();
        while let Some(id) = queue.pop() {
            if !seen.insert(id) || objects.len() > 2000 {
                continue;
            }
            if let Ok(obj) = self.inner.get_object(id) {
                // Never pull in pages (e.g. through destinations of link actions).
                if obj
                    .as_dict()
                    .map(|d| d.has_type(b"Page") || d.has_type(b"Pages"))
                    .unwrap_or(false)
                {
                    continue;
                }
                collect_refs(obj, &mut queue);
                objects.insert(id, obj.clone());
            }
        }
        Ok(AnnotSnapshot { root, objects })
    }

    /// Insert a snapshot on `page`, shifted by `(dx, dy)` points. Returns the new annotation id.
    pub fn paste_annotation_snapshot(
        &mut self,
        page: usize,
        snap: &AnnotSnapshot,
        dx: f32,
        dy: f32,
    ) -> Result<ObjectId> {
        let page_id = self.page_id(page)?;
        self.begin_edit();
        let mut map = BTreeMap::new();
        for old in snap.objects.keys() {
            map.insert(*old, self.inner.new_object_id());
        }
        for (old, obj) in &snap.objects {
            let mut obj = obj.clone();
            remap(&mut obj, &map);
            self.inner.objects.insert(map[old], obj);
        }
        let mut root = snap.root.clone();
        let mut root_obj = Object::Dictionary(root.clone());
        remap(&mut root_obj, &map);
        if let Object::Dictionary(d) = root_obj {
            root = d;
        }
        if let Some(r) = root.get(b"Rect").ok().and_then(|r| rect_of(&self.inner, r)) {
            root.set(
                "Rect",
                Object::Array(vec![
                    (r[0] + dx).into(),
                    (r[1] + dy).into(),
                    (r[2] + dx).into(),
                    (r[3] + dy).into(),
                ]),
            );
        }
        map_geometry(&mut root, &|x, y| (x + dx, y + dy));
        root.set("P", Object::Reference(page_id));
        root.set(
            "NM",
            Object::string_literal(format!("folitecta-{}-{}", self.revision(), self.inner.max_id + 1)),
        );
        root.set("M", Object::string_literal(crate::util::pdf_date_now()));
        let id = self.push_annotation(page_id, root)?;
        self.commit()?;
        Ok(id)
    }

    /// Duplicate an annotation (possibly onto another page).
    pub fn duplicate_annotation(
        &mut self,
        src_page: usize,
        index: usize,
        dst_page: usize,
        dx: f32,
        dy: f32,
    ) -> Result<ObjectId> {
        let snap = self.annotation_snapshot(src_page, index)?;
        self.paste_annotation_snapshot(dst_page, &snap, dx, dy)
    }

    /// Move an annotation by `(dx, dy)` points (PDF user space).
    pub fn move_annotation(&mut self, page: usize, index: usize, dx: f32, dy: f32) -> Result<()> {
        let (_, aid) = self.annotation_ref(page, index)?;
        let aid = aid.ok_or_else(|| Error::Invalid("inline annotations cannot be moved".into()))?;
        let rect = self
            .inner
            .get_dictionary(aid)?
            .get(b"Rect")
            .ok()
            .and_then(|r| rect_of(&self.inner, r))
            .ok_or_else(|| Error::Invalid("annotation has no rectangle".into()))?;
        self.set_annotation_rect(
            page,
            index,
            [rect[0] + dx, rect[1] + dy, rect[2] + dx, rect[3] + dy],
        )
    }

    /// Give an annotation a new rectangle, scaling its geometry to match.
    pub fn set_annotation_rect(&mut self, page: usize, index: usize, rect: [f32; 4]) -> Result<()> {
        let (_, aid) = self.annotation_ref(page, index)?;
        let aid = aid.ok_or_else(|| Error::Invalid("inline annotations cannot be changed".into()))?;
        let n = crate::geom::normalize(rect);
        if n[2] - n[0] < 1.0 || n[3] - n[1] < 1.0 {
            return Err(Error::Invalid("annotation rectangle is empty".into()));
        }
        self.begin_edit();
        self.reshape_annotation(aid, n)?;
        self.commit()
    }

    /// Give the annotation object `aid` the rectangle `n` (normalized),
    /// scaling its geometry and drawing it again, as part of the edit under
    /// way (no undo step of its own).
    pub(crate) fn reshape_annotation(&mut self, aid: ObjectId, n: [f32; 4]) -> Result<()> {
        let old = self
            .inner
            .get_dictionary(aid)?
            .get(b"Rect")
            .ok()
            .and_then(|r| rect_of(&self.inner, r))
            .unwrap_or(n);
        let sx = (n[2] - n[0]) / (old[2] - old[0]).max(0.001);
        let sy = (n[3] - n[1]) / (old[3] - old[1]).max(0.001);
        let dict = self.inner.get_dictionary_mut(aid)?;
        dict.set("Rect", Object::Array(n.iter().map(|v| (*v).into()).collect()));
        map_geometry(dict, &|x, y| (n[0] + (x - old[0]) * sx, n[1] + (y - old[1]) * sy));
        dict.set("M", Object::string_literal(crate::util::pdf_date_now()));
        // Redraw supported comments so strokes and text keep their size.
        if !self.refresh_appearance(aid)? {
            let widget = self
                .inner
                .get_dictionary(aid)
                .ok()
                .and_then(|d| d.get(b"Subtype").ok())
                .and_then(|s| s.as_name().ok())
                .map(|n| n == b"Widget")
                .unwrap_or(false);
            if widget {
                self.refresh_widget(aid)?;
            }
        }
        Ok(())
    }
}
