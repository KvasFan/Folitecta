//! The clean-up every redaction does after the content is taken out: the
//! structure tree follows the copies of forms and forgets content that is
//! gone, alternate descriptions of removed content go, and originals that
//! nothing draws any more are removed with every reference to them.

use std::collections::{BTreeSet, HashMap, HashSet};

use lopdf::{Dictionary, Object, ObjectId};

use super::holders::TouchKey;
use super::sanitize::is_struct_elem;
use super::terms::{Term, replace_terms};
use crate::document::Document;
use crate::util::{deref, text_of};

/// What becomes of a kid of a structure element.
enum Kid {
    Keep,
    /// Content of the element, removed or touched by the redaction.
    Touched,
    /// Content that stays as it was.
    Content,
    Drop,
}

impl Document {
    /// The page of a structure element (its own `/Pg` or its parents').
    fn elem_page(&self, d: &Dictionary) -> Option<ObjectId> {
        let mut cur = d.clone();
        for _ in 0..64 {
            if let Some(pg) = cur.get(b"Pg").ok().and_then(|p| p.as_reference().ok()) {
                return Some(pg);
            }
            let parent = cur.get(b"P").ok().and_then(|p| p.as_reference().ok())?;
            cur = self.inner.get_dictionary(parent).ok()?.clone();
        }
        None
    }

    /// Follow the redaction in the structure tree: marked-content
    /// references to forms copied for a page point to the copies, references
    /// to removed objects and to the content of pages drawn as pictures go,
    /// and elements whose content was touched lose their replacement text
    /// and expansion; their alternate description becomes `placeholder`
    /// when all their content was touched (otherwise the terms in it are
    /// replaced).
    pub(super) fn tidy_structure(
        &mut self,
        copies: &[(ObjectId, ObjectId, ObjectId)],
        touched: &BTreeSet<(TouchKey, i64)>,
        picture_pages: &HashSet<ObjectId>,
        gone: &HashSet<ObjectId>,
        terms: &[Term],
        placeholder: &str,
    ) {
        let copy_of: HashMap<(ObjectId, ObjectId), ObjectId> =
            copies.iter().map(|(o, c, p)| ((*o, *p), *c)).collect();
        let elems: Vec<ObjectId> = self
            .inner
            .objects
            .iter()
            .filter(|(_, o)| o.as_dict().is_ok_and(is_struct_elem))
            .map(|(id, _)| *id)
            .collect();
        for id in elems {
            let Ok(mut d) = self.inner.get_dictionary(id).cloned() else {
                continue;
            };
            let pg = self.elem_page(&d);
            let Ok(k) = d.get(b"K").cloned() else {
                continue;
            };
            // The kids, and where they live: in the element, or in an array object.
            let (list, array_id) = match &k {
                Object::Array(a) => (a.clone(), None),
                Object::Reference(r) => match self.inner.get_object(*r) {
                    Ok(Object::Array(a)) => (a.clone(), Some(*r)),
                    _ => (vec![k.clone()], None),
                },
                other => (vec![other.clone()], None),
            };
            let mut kept = Vec::with_capacity(list.len());
            let mut any_touched = false;
            let mut any_content = false;
            let mut changed = false;
            for kid in list {
                let (verdict, new) = self.tidy_kid(&kid, pg, &copy_of, touched, picture_pages, gone);
                match verdict {
                    Kid::Drop => {
                        any_touched = true;
                        changed = true;
                        continue;
                    }
                    Kid::Touched => any_touched = true,
                    Kid::Content => any_content = true,
                    Kid::Keep => {}
                }
                if let Some(new) = new {
                    changed = true;
                    kept.push(new);
                } else {
                    kept.push(kid);
                }
            }
            if changed {
                match array_id {
                    Some(a) => {
                        self.inner.objects.insert(a, Object::Array(kept));
                    }
                    None => {
                        if kept.len() == 1 && !matches!(k, Object::Array(_)) {
                            d.set("K", kept.remove(0));
                        } else {
                            d.set("K", Object::Array(kept));
                        }
                    }
                }
            }
            if any_touched {
                d.remove(b"ActualText");
                d.remove(b"E");
                if let Some(alt) = d.get(b"Alt").ok().and_then(|a| text_of(&self.inner, a)) {
                    if !any_content {
                        d.set("Alt", lopdf::text_string(placeholder));
                    } else if let Some(new) = replace_terms(&alt, terms) {
                        d.set("Alt", lopdf::text_string(&new));
                    }
                }
                changed = true;
            }
            if changed {
                self.inner.objects.insert(id, Object::Dictionary(d));
            }
        }
    }

    /// One kid of an element: what it is and its new value (None: unchanged).
    fn tidy_kid(
        &mut self,
        kid: &Object,
        pg: Option<ObjectId>,
        copy_of: &HashMap<(ObjectId, ObjectId), ObjectId>,
        touched: &BTreeSet<(TouchKey, i64)>,
        picture_pages: &HashSet<ObjectId>,
        gone: &HashSet<ObjectId>,
    ) -> (Kid, Option<Object>) {
        let page_content = |page: Option<ObjectId>, mcid: i64| -> Kid {
            match page {
                Some(p) if picture_pages.contains(&p) => Kid::Drop,
                Some(p) if touched.contains(&(TouchKey::Page(p), mcid)) => Kid::Touched,
                _ => Kid::Content,
            }
        };
        match kid {
            Object::Integer(m) => (page_content(pg, *m), None),
            Object::Reference(r) => {
                let Ok(Object::Dictionary(d)) = self.inner.get_object(*r).cloned() else {
                    return (Kid::Keep, None);
                };
                if is_struct_elem(&d) {
                    return (Kid::Keep, None);
                }
                let (verdict, new) = self.tidy_ref_dict(&d, pg, copy_of, touched, picture_pages, gone);
                if let Some(new) = new {
                    self.inner.objects.insert(*r, Object::Dictionary(new));
                }
                (verdict, None)
            }
            Object::Dictionary(d) => {
                let (verdict, new) = self.tidy_ref_dict(d, pg, copy_of, touched, picture_pages, gone);
                (verdict, new.map(Object::Dictionary))
            }
            _ => (Kid::Keep, None),
        }
    }

    /// A marked-content or object reference.
    fn tidy_ref_dict(
        &self,
        d: &Dictionary,
        pg: Option<ObjectId>,
        copy_of: &HashMap<(ObjectId, ObjectId), ObjectId>,
        touched: &BTreeSet<(TouchKey, i64)>,
        picture_pages: &HashSet<ObjectId>,
        gone: &HashSet<ObjectId>,
    ) -> (Kid, Option<Dictionary>) {
        let page = d.get(b"Pg").ok().and_then(|p| p.as_reference().ok()).or(pg);
        if d.has_type(b"MCR") || d.has(b"MCID") {
            let mcid = d
                .get(b"MCID")
                .ok()
                .and_then(|m| deref(&self.inner, m).as_i64().ok())
                .unwrap_or(-1);
            let Some(stm) = d.get(b"Stm").ok().and_then(|s| s.as_reference().ok()) else {
                return (
                    match page {
                        Some(p) if picture_pages.contains(&p) => Kid::Drop,
                        Some(p) if touched.contains(&(TouchKey::Page(p), mcid)) => Kid::Touched,
                        _ => Kid::Content,
                    },
                    None,
                );
            };
            if gone.contains(&stm) || page.is_some_and(|p| picture_pages.contains(&p)) {
                return (Kid::Drop, None);
            }
            let (target, new) = match page.and_then(|p| copy_of.get(&(stm, p))) {
                Some(c) => {
                    let mut n = d.clone();
                    n.set("Stm", Object::Reference(*c));
                    (*c, Some(n))
                }
                None => (stm, None),
            };
            let verdict = if touched.contains(&(TouchKey::Stream(target), mcid)) {
                Kid::Touched
            } else {
                Kid::Content
            };
            return (verdict, new);
        }
        if d.has_type(b"OBJR") {
            let Some(obj) = d.get(b"Obj").ok().and_then(|o| o.as_reference().ok()) else {
                return (Kid::Keep, None);
            };
            if gone.contains(&obj) || !self.inner.objects.contains_key(&obj) {
                return (Kid::Drop, None);
            }
            if let Some(c) = page.and_then(|p| copy_of.get(&(obj, p))) {
                let mut n = d.clone();
                n.set("Obj", Object::Reference(*c));
                return (Kid::Keep, Some(n));
            }
        }
        (Kid::Keep, None)
    }
}
