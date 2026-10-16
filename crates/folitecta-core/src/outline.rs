//! Bookmarks (document outline).

use lopdf::{Bookmark, Object};

use crate::document::Document;
use crate::error::{Error, Result};

/// One bookmark with its children.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct OutlineItem {
    pub title: String,
    /// Zero-based target page, when it could be resolved.
    pub page: Option<usize>,
    pub children: Vec<OutlineItem>,
}

impl OutlineItem {
    pub fn new(title: impl Into<String>, page: Option<usize>) -> Self {
        OutlineItem {
            title: title.into(),
            page,
            children: Vec::new(),
        }
    }

    fn count(&self) -> usize {
        1 + self.children.iter().map(|c| c.count()).sum::<usize>()
    }
}

fn get_path<'a>(items: &'a mut [OutlineItem], path: &[usize]) -> Option<&'a mut OutlineItem> {
    let (first, rest) = path.split_first()?;
    let item = items.get_mut(*first)?;
    if rest.is_empty() {
        Some(item)
    } else {
        get_path(&mut item.children, rest)
    }
}

impl Document {
    /// The bookmark tree.
    pub fn outline(&self) -> Vec<OutlineItem> {
        let toc = match self.inner.get_toc() {
            Ok(toc) => toc,
            Err(_) => return Vec::new(),
        };
        let mut roots: Vec<OutlineItem> = Vec::new();
        // stack of (level, path indices)
        let mut stack: Vec<(usize, Vec<usize>)> = Vec::new();
        for entry in toc.toc {
            let item = OutlineItem {
                title: entry.title,
                page: entry.page.checked_sub(1),
                children: Vec::new(),
            };
            while let Some((lvl, _)) = stack.last() {
                if *lvl >= entry.level {
                    stack.pop();
                } else {
                    break;
                }
            }
            let path = match stack.last() {
                None => {
                    roots.push(item);
                    vec![roots.len() - 1]
                }
                Some((_, parent_path)) => {
                    let parent = get_path(&mut roots, parent_path).expect("valid parent path");
                    parent.children.push(item);
                    let mut p = parent_path.clone();
                    p.push(parent.children.len() - 1);
                    p
                }
            };
            stack.push((entry.level, path));
        }
        roots
    }

    /// Flat view of the outline: `(depth, title, page)`.
    pub fn outline_flat(&self) -> Vec<(usize, String, Option<usize>)> {
        fn walk(items: &[OutlineItem], depth: usize, out: &mut Vec<(usize, String, Option<usize>)>) {
            for item in items {
                out.push((depth, item.title.clone(), item.page));
                walk(&item.children, depth + 1, out);
            }
        }
        let mut out = Vec::new();
        walk(&self.outline(), 0, &mut out);
        out
    }

    /// Total number of bookmarks.
    pub fn bookmark_count(&self) -> usize {
        self.outline().iter().map(|i| i.count()).sum()
    }

    /// Replace the whole outline.
    pub fn set_outline(&mut self, items: &[OutlineItem]) -> Result<()> {
        self.begin_edit();
        self.write_outline(items)?;
        self.commit()
    }

    fn write_outline(&mut self, items: &[OutlineItem]) -> Result<()> {
        let page_ids = self.page_ids();
        let _ = self.inner.delete_outlines();
        self.inner.bookmarks.clear();
        self.inner.bookmark_table.clear();
        self.inner.max_bookmark_id = 0;
        fn add(
            doc: &mut lopdf::Document,
            items: &[OutlineItem],
            parent: Option<u32>,
            pages: &[lopdf::ObjectId],
        ) {
            for item in items {
                let page = item.page.and_then(|p| pages.get(p)).copied().unwrap_or(pages[0]);
                let id = doc.add_bookmark(
                    Bookmark::new(item.title.clone(), [0.0, 0.0, 0.0], 0, page),
                    parent,
                );
                add(doc, &item.children, Some(id), pages);
            }
        }
        if items.is_empty() {
            self.inner.catalog_mut()?.remove(b"Outlines");
            return Ok(());
        }
        add(&mut self.inner, items, None, &page_ids);
        match self.inner.build_outline() {
            Some(id) => {
                self.inner.catalog_mut()?.set("Outlines", Object::Reference(id));
            }
            None => {
                self.inner.catalog_mut()?.remove(b"Outlines");
            }
        }
        Ok(())
    }

    /// Append a bookmark under `parent_path` (empty slice = top level).
    pub fn add_bookmark(&mut self, title: &str, page: usize, parent_path: &[usize]) -> Result<()> {
        if page >= self.page_count() {
            return Err(Error::InvalidPage(page));
        }
        let mut items = self.outline();
        let item = OutlineItem::new(title, Some(page));
        if parent_path.is_empty() {
            items.push(item);
        } else {
            let parent = get_path(&mut items, parent_path)
                .ok_or_else(|| Error::Invalid("bookmark path not found".into()))?;
            parent.children.push(item);
        }
        self.set_outline(&items)
    }

    /// Delete the bookmark at `path` (children are removed too).
    pub fn delete_bookmark(&mut self, path: &[usize]) -> Result<()> {
        let mut items = self.outline();
        let (last, parent_path) = path
            .split_last()
            .ok_or_else(|| Error::Invalid("empty bookmark path".into()))?;
        let list = if parent_path.is_empty() {
            &mut items
        } else {
            &mut get_path(&mut items, parent_path)
                .ok_or_else(|| Error::Invalid("bookmark path not found".into()))?
                .children
        };
        if *last >= list.len() {
            return Err(Error::Invalid("bookmark path not found".into()));
        }
        list.remove(*last);
        self.set_outline(&items)
    }

    /// Rename the bookmark at `path`.
    pub fn rename_bookmark(&mut self, path: &[usize], title: &str) -> Result<()> {
        let mut items = self.outline();
        let item =
            get_path(&mut items, path).ok_or_else(|| Error::Invalid("bookmark path not found".into()))?;
        item.title = title.to_string();
        self.set_outline(&items)
    }

    /// Point the bookmark at `path` to another page.
    pub fn set_bookmark_page(&mut self, path: &[usize], page: usize) -> Result<()> {
        if page >= self.page_count() {
            return Err(Error::InvalidPage(page));
        }
        let mut items = self.outline();
        let item =
            get_path(&mut items, path).ok_or_else(|| Error::Invalid("bookmark path not found".into()))?;
        item.page = Some(page);
        self.set_outline(&items)
    }

    /// Create one bookmark per page titled "Page N" (useful for scanned documents).
    pub fn bookmarks_from_pages(&mut self) -> Result<()> {
        let items: Vec<OutlineItem> = (0..self.page_count())
            .map(|p| OutlineItem::new(format!("Page {}", p + 1), Some(p)))
            .collect();
        self.set_outline(&items)
    }
}
