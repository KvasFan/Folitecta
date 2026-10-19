//! Flattening: merge annotation appearances into the page content as vector
//! form XObjects, then remove the annotations (comments, form widgets or both).

use std::collections::HashSet;

use lopdf::{Dictionary, Object, ObjectId};

use crate::document::Document;
use crate::error::{Error, Result};
use crate::util::{deref, fmt_num, num, rect_of};

/// Which annotations to flatten.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlattenScope {
    /// Comments and markups (everything except links, widgets and popups).
    Comments,
    /// Interactive form fields (widget annotations); the form is removed.
    FormFields,
    /// Comments and form fields.
    All,
}

const NEVER: &[&[u8]] = &[b"Link", b"Popup"];

fn matrix_of(doc: &lopdf::Document, dict: &Dictionary) -> [f32; 6] {
    dict.get(b"Matrix")
        .ok()
        .map(|m| deref(doc, m))
        .and_then(|m| m.as_array().ok())
        .and_then(|a| {
            let v: Vec<f32> = a.iter().filter_map(|o| num(deref(doc, o))).collect();
            (v.len() == 6).then(|| [v[0], v[1], v[2], v[3], v[4], v[5]])
        })
        .unwrap_or([1.0, 0.0, 0.0, 1.0, 0.0, 0.0])
}

impl Document {
    /// The normal appearance stream of an annotation (resolving `/AS` sub-states).
    fn normal_appearance(&self, annot: &Dictionary) -> Option<ObjectId> {
        let doc = &self.inner;
        let ap = deref(doc, annot.get(b"AP").ok()?).as_dict().ok()?;
        match ap.get(b"N").ok()? {
            Object::Reference(r) => match doc.get_object(*r).ok()? {
                Object::Stream(_) => Some(*r),
                Object::Dictionary(states) => {
                    let state = annot.get(b"AS").ok()?.as_name().ok()?;
                    states.get(state).ok()?.as_reference().ok()
                }
                _ => None,
            },
            Object::Dictionary(states) => {
                let state = annot.get(b"AS").ok()?.as_name().ok()?;
                states.get(state).ok()?.as_reference().ok()
            }
            _ => None,
        }
    }

    /// Flatten annotations of the given pages. Returns how many were merged.
    pub fn flatten(&mut self, pages: &[usize], scope: FlattenScope) -> Result<usize> {
        let n = self.page_count();
        for &p in pages {
            if p >= n {
                return Err(Error::InvalidPage(p));
            }
        }
        self.begin_edit();
        let count = self.flatten_internal(pages, scope, None)?;
        self.commit()?;
        Ok(count)
    }

    /// Flatten annotations; `only` restricts the work to one `(page, index)`.
    pub(crate) fn flatten_internal(
        &mut self,
        pages: &[usize],
        scope: FlattenScope,
        only: Option<(usize, usize)>,
    ) -> Result<usize> {
        let mut removed = HashSet::new();
        let count = self.flatten_where(pages, scope, only, &|_| true, &mut removed)?;
        self.forget_objects(&removed);
        Ok(count)
    }

    /// Remove `gone` objects and every reference to them in one pass, then
    /// the objects nothing reaches any more.
    pub(crate) fn forget_objects(&mut self, gone: &HashSet<ObjectId>) {
        if !gone.is_empty() {
            crate::util::drop_references(&mut self.inner, gone);
            for id in gone {
                self.inner.objects.remove(id);
            }
        }
        crate::util::prune_unreachable(&mut self.inner);
    }

    /// Flatten the annotations of `scope` whose subtype `wanted` accepts. The
    /// annotations merged (and their pop-ups) leave the pages and are added
    /// to `removed`, for the caller to take out of the file in one pass
    /// ([`Document::forget_objects`]).
    pub(crate) fn flatten_where(
        &mut self,
        pages: &[usize],
        scope: FlattenScope,
        only: Option<(usize, usize)>,
        wanted: &dyn Fn(&[u8]) -> bool,
        removed: &mut HashSet<ObjectId>,
    ) -> Result<usize> {
        let ids = self.page_ids();
        let mut flattened = 0;
        let mut removed_widgets = false;
        for &p in pages {
            let page_id = ids[p];
            self.inherit_attributes(&[page_id]);
            let annots_obj = self.inner.get_dictionary(page_id)?.get(b"Annots").ok().cloned();
            let list: Vec<Object> = match annots_obj {
                Some(Object::Array(a)) => a,
                Some(Object::Reference(r)) => self
                    .inner
                    .get_object(r)
                    .ok()
                    .and_then(|o| o.as_array().ok())
                    .cloned()
                    .unwrap_or_default(),
                _ => continue,
            };
            let mut keep = Vec::with_capacity(list.len());
            let mut content = String::new();
            for (position, item) in list.into_iter().enumerate() {
                if only.is_some_and(|(op, oi)| op != p || oi != position) {
                    keep.push(item);
                    continue;
                }
                let (aid, dict) = match &item {
                    Object::Reference(r) => match self.inner.get_dictionary(*r) {
                        Ok(d) => (Some(*r), d.clone()),
                        Err(_) => continue,
                    },
                    Object::Dictionary(d) => (None, d.clone()),
                    _ => continue,
                };
                let subtype = dict
                    .get(b"Subtype")
                    .ok()
                    .and_then(|s| s.as_name().ok())
                    .unwrap_or(b"")
                    .to_vec();
                let is_widget = subtype == b"Widget";
                let in_scope = match scope {
                    FlattenScope::Comments => !is_widget,
                    FlattenScope::FormFields => is_widget,
                    FlattenScope::All => true,
                } && !NEVER.contains(&subtype.as_slice())
                    && wanted(&subtype);
                if !in_scope {
                    keep.push(item);
                    continue;
                }
                let flags = dict.get(b"F").ok().and_then(|f| f.as_i64().ok()).unwrap_or(0);
                let hidden = flags & 2 != 0 || flags & 32 != 0;
                let rect = dict.get(b"Rect").ok().and_then(|r| rect_of(&self.inner, r));
                if let (false, Some(rect), Some(ap_id)) = (hidden, rect, self.normal_appearance(&dict)) {
                    let form = self.inner.get_object(ap_id)?.as_stream()?.dict.clone();
                    let bbox = form
                        .get(b"BBox")
                        .ok()
                        .and_then(|b| rect_of(&self.inner, b))
                        .unwrap_or(rect);
                    // The transformed appearance box on the rectangle (PDF 32000-1, 12.5.5).
                    let [sx, _, _, sy, tx, ty] =
                        crate::print::build::appearance_matrix(bbox, matrix_of(&self.inner, &form), rect);
                    // Make sure the form is a proper XObject.
                    if let Ok(Object::Stream(s)) = self.inner.get_object_mut(ap_id) {
                        s.dict.set("Type", Object::Name(b"XObject".to_vec()));
                        s.dict.set("Subtype", Object::Name(b"Form".to_vec()));
                        if !s.dict.has(b"BBox") {
                            s.dict
                                .set("BBox", Object::Array(rect.iter().map(|v| (*v).into()).collect()));
                        }
                    }
                    let name = format!("FoliFlat{}", ap_id.0);
                    self.add_page_resource(page_id, "XObject", &name, Object::Reference(ap_id))?;
                    let f = fmt_num;
                    content.push_str(&format!(
                        "q {} 0 0 {} {} {} cm /{} Do Q\n",
                        f(sx),
                        f(sy),
                        f(tx),
                        f(ty),
                        name
                    ));
                }
                flattened += 1;
                if is_widget {
                    removed_widgets = true;
                }
                if let Some(id) = aid {
                    if let Some(popup) = dict.get(b"Popup").ok().and_then(|p| p.as_reference().ok()) {
                        removed.insert(popup);
                    }
                    removed.insert(id);
                }
            }
            // The pop-ups of the merged annotations go with them.
            keep.retain(|o| match o {
                Object::Reference(r) => !removed.contains(r),
                _ => true,
            });
            self.inner
                .get_dictionary_mut(page_id)?
                .set("Annots", Object::Array(keep));
            if !content.is_empty() {
                self.append_page_content(page_id, content.into_bytes(), Some("Flatten"))?;
            }
        }
        if removed_widgets && matches!(scope, FlattenScope::FormFields | FlattenScope::All) {
            self.inner.catalog_mut()?.remove(b"AcroForm");
        }
        Ok(flattened)
    }
}
