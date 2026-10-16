//! Redaction: mark areas, then apply by painting them black and rasterizing the page.

use lopdf::{Object, ObjectId, dictionary};

use crate::document::Document;
use crate::error::Result;
use crate::util::{deref, fmt_num, rect_of};

impl Document {
    /// Mark a rectangle for redaction (a `Redact` annotation with a red outline).
    pub fn add_redaction(&mut self, page: usize, rect: [f32; 4], author: &str) -> Result<ObjectId> {
        let page_id = self.page_id(page)?;
        self.begin_edit();
        let mut annot = self.base_annotation(page_id, "Redact", rect, author, "", [1.0, 0.0, 0.0]);
        annot.set("IC", vec![0.into(), 0.into(), 0.into()]);
        let f = fmt_num;
        let content = format!(
            "q /GS gs 1 0 0 rg {} {} {} {} re f Q q 1 0 0 RG 1.5 w {} {} {} {} re S Q",
            f(rect[0]),
            f(rect[1]),
            f(rect[2] - rect[0]),
            f(rect[3] - rect[1]),
            f(rect[0] + 0.75),
            f(rect[1] + 0.75),
            f(rect[2] - rect[0] - 1.5),
            f(rect[3] - rect[1] - 1.5),
        );
        let resources = dictionary! { "ExtGState" => dictionary! { "GS" => dictionary! { "CA" => 1.0f32, "ca" => 0.18f32 } } };
        self.add_appearance(&mut annot, rect, content, resources);
        let id = self.push_annotation(page_id, annot)?;
        self.commit()?;
        Ok(id)
    }

    /// Pending redaction marks as `(page, rect)`.
    pub fn pending_redactions(&self) -> Vec<(usize, [f32; 4])> {
        (0..self.page_count())
            .filter_map(|p| self.annotations(p).ok())
            .flatten()
            .filter(|a| a.subtype == "Redact")
            .map(|a| (a.page, a.rect))
            .collect()
    }

    /// Apply the pending redaction marks of the given pages (all pages when `None`):
    /// paints the areas black, removes the marks and rasterizes the affected pages at
    /// 200 dpi so the hidden content cannot be recovered. Returns the number of areas.
    pub fn apply_redactions(&mut self, pages: Option<&[usize]>) -> Result<usize> {
        let pending = self.pending_redactions();
        let mut by_page: Vec<(usize, Vec<[f32; 4]>)> = Vec::new();
        for (p, r) in pending {
            if let Some(sel) = pages {
                if !sel.contains(&p) {
                    continue;
                }
            }
            match by_page.iter_mut().find(|(q, _)| *q == p) {
                Some((_, v)) => v.push(r),
                None => by_page.push((p, vec![r])),
            }
        }
        if by_page.is_empty() {
            return Ok(0);
        }
        self.begin_edit();
        let mut count = 0;
        let f = fmt_num;
        for (p, rects) in &by_page {
            let page_id = self.page_id(*p)?;
            let mut content = String::from("q 0 g ");
            for r in rects {
                content.push_str(&format!(
                    "{} {} {} {} re f ",
                    f(r[0]),
                    f(r[1]),
                    f(r[2] - r[0]),
                    f(r[3] - r[1])
                ));
                count += 1;
            }
            content.push_str("Q\n");
            self.append_page_content(page_id, content.into_bytes(), Some("Redaction"))?;
            // Drop the Redact marks from the page.
            let existing = self.inner.get_dictionary(page_id)?.get(b"Annots").ok().cloned();
            let arr = match existing {
                Some(Object::Array(arr)) => arr,
                Some(Object::Reference(r)) => deref(&self.inner, &Object::Reference(r))
                    .as_array()
                    .cloned()
                    .unwrap_or_default(),
                _ => Vec::new(),
            };
            let mut keep = Vec::new();
            for a in arr {
                let is_redact = deref(&self.inner, &a)
                    .as_dict()
                    .ok()
                    .and_then(|d| d.get(b"Subtype").ok())
                    .and_then(|s| s.as_name().ok())
                    == Some(b"Redact");
                if is_redact {
                    if let Ok(r) = a.as_reference() {
                        self.inner.delete_object(r);
                    }
                } else {
                    keep.push(a);
                }
            }
            self.inner
                .get_dictionary_mut(page_id)?
                .set("Annots", Object::Array(keep));
        }
        // Serialize so the renderer sees the black boxes, then burn the pages.
        self.commit()?;
        let pages: Vec<usize> = by_page.iter().map(|(p, _)| *p).collect();
        self.rasterize_internal(&pages, 200.0, false)?;
        self.commit()?;
        Ok(count)
    }

    /// Rectangles of redaction marks on a page (for display).
    pub fn redaction_rects(&self, page: usize) -> Vec<[f32; 4]> {
        let Ok(page_id) = self.page_id(page) else {
            return Vec::new();
        };
        let Ok(dict) = self.inner.get_dictionary(page_id) else {
            return Vec::new();
        };
        dict.get(b"Annots")
            .ok()
            .map(|a| deref(&self.inner, a))
            .and_then(|a| a.as_array().ok())
            .map(|arr| {
                arr.iter()
                    .filter_map(|a| deref(&self.inner, a).as_dict().ok())
                    .filter(|d| d.get(b"Subtype").ok().and_then(|s| s.as_name().ok()) == Some(b"Redact"))
                    .filter_map(|d| d.get(b"Rect").ok().and_then(|r| rect_of(&self.inner, r)))
                    .collect()
            })
            .unwrap_or_default()
    }
}
