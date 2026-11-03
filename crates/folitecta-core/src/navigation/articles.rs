//! Article threads (FOL-L022): the stories a document runs across columns
//! and pages, each a chain of beads (an area of a page) read in order.

use lopdf::{Object, dictionary};

use crate::document::Document;
use crate::error::{Error, Result};
use crate::util::{deref, rect_of, text_of};

/// A thread as written: its object, title and beads (object, page, area).
struct Thread {
    id: Option<lopdf::ObjectId>,
    title: String,
    beads: Vec<(lopdf::ObjectId, usize, [f32; 4])>,
}

/// An article thread.
#[derive(Debug, Clone, PartialEq)]
pub struct Article {
    /// Its title (`/I /Title`), empty when it has none.
    pub title: String,
    /// Its beads in reading order: the page and the area (PDF user space).
    pub beads: Vec<(usize, [f32; 4])>,
}

impl Document {
    /// The threads of the catalog as they are written: the object of each
    /// (when it is one), its title, and its beads on pages of the document
    /// (object, page, area), in reading order.
    fn threads(&self) -> Vec<Thread> {
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
                let Some(id) = bead else { break };
                let Ok(b) = doc.get_dictionary(id) else {
                    break;
                };
                let page = b
                    .get(b"P")
                    .ok()
                    .and_then(|p| p.as_reference().ok())
                    .and_then(|p| pages.iter().position(|pid| *pid == p));
                let rect = b.get(b"R").ok().and_then(|r| rect_of(doc, r));
                if let (Some(page), Some(r)) = (page, rect) {
                    beads.push((id, page, r));
                }
                bead = b.get(b"N").ok().and_then(|n| match n {
                    Object::Reference(r) => Some(*r),
                    _ => None,
                });
                if bead == first {
                    break;
                }
            }
            out.push(Thread {
                id: t.as_reference().ok(),
                title,
                beads,
            });
        }
        out
    }

    /// The article threads of the document, in the order of the file. Beads
    /// on pages the document no longer has are left out.
    pub fn articles(&self) -> Vec<Article> {
        self.threads()
            .into_iter()
            .map(|t| Article {
                title: t.title,
                beads: t.beads.into_iter().map(|(_, p, r)| (p, r)).collect(),
            })
            .collect()
    }

    /// The object of thread `index`, to change it.
    fn thread_to_edit(&self, index: usize) -> Result<Thread> {
        let t = self
            .threads()
            .into_iter()
            .nth(index)
            .ok_or_else(|| Error::Invalid("there is no such article".into()))?;
        if t.id.is_none() {
            return Err(Error::Invalid("the article is written inside the catalog".into()));
        }
        Ok(t)
    }

    /// Give article `index` the title `title` (none when empty). One undo step.
    pub fn rename_article(&mut self, index: usize, title: &str) -> Result<()> {
        let t = self.thread_to_edit(index)?;
        let id = t.id.unwrap_or((0, 0));
        self.edit_step(|doc| {
            let d = doc.inner.get_dictionary_mut(id)?;
            if title.trim().is_empty() {
                d.remove(b"I");
            } else {
                d.set("I", dictionary! { "Title" => lopdf::text_string(title.trim()) });
            }
            Ok(())
        })
    }

    /// Take bead `id` off the list of beads of its page.
    fn unlist_bead(&mut self, id: lopdf::ObjectId) -> Result<()> {
        let page = self
            .inner
            .get_dictionary(id)?
            .get(b"P")
            .ok()
            .and_then(|p| p.as_reference().ok());
        if let Some(page) = page {
            if let Ok(d) = self.inner.get_dictionary_mut(page) {
                if let Ok(Object::Array(list)) = d.get_mut(b"B") {
                    list.retain(|b| b.as_reference().ok() != Some(id));
                }
            }
        }
        self.inner.objects.remove(&id);
        Ok(())
    }

    /// Delete article `index` with its parts. One undo step.
    pub fn delete_article(&mut self, index: usize) -> Result<()> {
        let t = self.thread_to_edit(index)?;
        let id = t.id.unwrap_or((0, 0));
        self.edit_step(|doc| {
            for (bead, _, _) in &t.beads {
                doc.unlist_bead(*bead)?;
            }
            let catalog = doc.inner.catalog_mut()?;
            let mut threads = catalog
                .get(b"Threads")
                .ok()
                .and_then(|t| t.as_array().ok())
                .cloned()
                .unwrap_or_default();
            threads.retain(|o| o.as_reference().ok() != Some(id));
            if threads.is_empty() {
                catalog.remove(b"Threads");
            } else {
                catalog.set("Threads", threads);
            }
            doc.inner.objects.remove(&id);
            Ok(())
        })
    }

    /// Take part `part` out of article `index`: the parts around it join;
    /// the last part takes the article with it. One undo step.
    pub fn delete_article_part(&mut self, index: usize, part: usize) -> Result<()> {
        let t = self.thread_to_edit(index)?;
        if part >= t.beads.len() {
            return Err(Error::Invalid("there is no such part".into()));
        }
        if t.beads.len() == 1 {
            return self.delete_article(index);
        }
        let thread = t.id.unwrap_or((0, 0));
        let ids: Vec<lopdf::ObjectId> = t.beads.iter().map(|b| b.0).collect();
        let n = ids.len();
        let gone = ids[part];
        let (prev, next) = (ids[(part + n - 1) % n], ids[(part + 1) % n]);
        self.edit_step(|doc| {
            doc.unlist_bead(gone)?;
            doc.inner
                .get_dictionary_mut(prev)?
                .set("N", Object::Reference(next));
            doc.inner
                .get_dictionary_mut(next)?
                .set("V", Object::Reference(prev));
            if part == 0 {
                // The next part begins the thread.
                doc.inner
                    .get_dictionary_mut(next)?
                    .set("T", Object::Reference(thread));
                doc.inner
                    .get_dictionary_mut(thread)?
                    .set("F", Object::Reference(next));
            }
            Ok(())
        })
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
