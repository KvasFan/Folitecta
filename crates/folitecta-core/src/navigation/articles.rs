//! Article threads (FOL-L022): the stories a document runs across columns
//! and pages, each a chain of beads (an area of a page) read in order.

use lopdf::{Object, dictionary};

use crate::document::Document;
use crate::error::{Error, Result};
use crate::util::{deref, rect_of, text_of};

/// An article thread.
#[derive(Debug, Clone, PartialEq)]
pub struct Article {
    /// Its title (`/I /Title`), empty when it has none.
    pub title: String,
    /// Its beads in reading order: the page and the area (PDF user space).
    pub beads: Vec<(usize, [f32; 4])>,
}

impl Document {
    /// The article threads of the document, in the order of the file. Beads
    /// on pages the document no longer has are left out.
    pub fn articles(&self) -> Vec<Article> {
        let doc = &self.inner;
        let Some(threads) = doc
            .catalog()
            .ok()
            .and_then(|c| c.get(b"Threads").ok())
            .and_then(|t| deref(doc, t).as_array().ok())
        else {
            return Vec::new();
        };
        let pages = self.page_ids();
        let mut out = Vec::new();
        for t in threads {
            let Ok(thread) = deref(doc, t).as_dict() else {
                continue;
            };
            let title = thread
                .get(b"I")
                .ok()
                .and_then(|i| deref(doc, i).as_dict().ok())
                .and_then(|i| i.get(b"Title").ok())
                .and_then(|t| text_of(doc, t))
                .unwrap_or_default();
            let first = thread.get(b"F").ok().and_then(|f| f.as_reference().ok());
            let mut beads = Vec::new();
            let mut bead = first;
            // The chain goes round to the first bead; a broken one ends.
            for _ in 0..10_000 {
                let Some(b) = bead.and_then(|b| doc.get_dictionary(b).ok()) else {
                    break;
                };
                let page = b
                    .get(b"P")
                    .ok()
                    .and_then(|p| p.as_reference().ok())
                    .and_then(|p| pages.iter().position(|id| *id == p));
                let rect = b.get(b"R").ok().and_then(|r| rect_of(doc, r));
                if let (Some(page), Some(r)) = (page, rect) {
                    beads.push((page, r));
                }
                bead = b.get(b"N").ok().and_then(|n| match n {
                    Object::Reference(r) => Some(*r),
                    _ => None,
                });
                if bead == first {
                    break;
                }
            }
            out.push(Article { title, beads });
        }
        out
    }

    /// Add an article thread titled `title` (none when empty) running
    /// through `beads` in order: the page and the area of each (PDF user
    /// space). One undo step.
    pub fn add_article(&mut self, title: &str, beads: &[(usize, [f32; 4])]) -> Result<()> {
        if beads.is_empty() {
            return Err(Error::Invalid("an article needs at least one area".into()));
        }
        let pages = self.page_ids();
        let page_ids = beads
            .iter()
            .map(|(p, _)| pages.get(*p).copied().ok_or(Error::InvalidPage(*p)))
            .collect::<Result<Vec<_>>>()?;
        self.edit_step(|doc| {
            let thread = doc.inner.new_object_id();
            let ids: Vec<lopdf::ObjectId> = beads.iter().map(|_| doc.inner.new_object_id()).collect();
            let n = ids.len();
            for (k, ((_, r), page)) in beads.iter().zip(&page_ids).enumerate() {
                let mut bead = dictionary! {
                    "Type" => "Bead",
                    "P" => Object::Reference(*page),
                    "R" => r.iter().map(|v| Object::Real(*v)).collect::<Vec<_>>(),
                    "N" => Object::Reference(ids[(k + 1) % n]),
                    "V" => Object::Reference(ids[(k + n - 1) % n]),
                };
                if k == 0 {
                    bead.set("T", Object::Reference(thread));
                }
                doc.inner.objects.insert(ids[k], Object::Dictionary(bead));
                // The page lists the beads on it.
                let page = doc.inner.get_dictionary_mut(*page)?;
                let mut list = page
                    .get(b"B")
                    .ok()
                    .and_then(|b| b.as_array().ok())
                    .cloned()
                    .unwrap_or_default();
                list.push(Object::Reference(ids[k]));
                page.set("B", list);
            }
            let mut info = dictionary! { "Type" => "Thread", "F" => Object::Reference(ids[0]) };
            if !title.trim().is_empty() {
                info.set("I", dictionary! { "Title" => lopdf::text_string(title.trim()) });
            }
            doc.inner.objects.insert(thread, Object::Dictionary(info));
            let catalog = doc.inner.catalog_mut()?;
            let mut threads = catalog
                .get(b"Threads")
                .ok()
                .and_then(|t| t.as_array().ok())
                .cloned()
                .unwrap_or_default();
            threads.push(Object::Reference(thread));
            catalog.set("Threads", threads);
            Ok(())
        })
    }
}
