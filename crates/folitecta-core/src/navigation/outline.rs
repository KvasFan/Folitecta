//! Bookmarks (document outline).
//!
//! The tree is read from the outline item dictionaries and written back with
//! every entry Folitecta does not edit preserved: destinations and actions,
//! views (position and zoom), colours, styles, the open state and private keys.

use std::collections::HashSet;

use lopdf::{Dictionary, Object, ObjectId, dictionary};

use crate::document::Document;
use crate::error::{Error, Result};
use crate::util::{color_of, deref, dict_text, num, text_of};

/// How a destination shows its page.
#[derive(Debug, Clone, PartialEq)]
pub enum DestView {
    /// The whole page (`/Fit`).
    Fit,
    /// The page width, with `top` at the top of the window (`/FitH`).
    FitWidth(Option<f32>),
    /// A position and zoom (`/XYZ`); `None` keeps the current value of the viewer.
    Xyz {
        left: Option<f32>,
        top: Option<f32>,
        zoom: Option<f32>,
    },
    /// A rectangle (`/FitR`).
    Rect([f32; 4]),
}

impl DestView {
    /// Destination array entries after the page reference.
    fn to_objects(&self) -> Vec<Object> {
        let opt = |v: Option<f32>| v.map(Object::Real).unwrap_or(Object::Null);
        match self {
            DestView::Fit => vec![Object::Name(b"Fit".to_vec())],
            DestView::FitWidth(top) => vec![Object::Name(b"FitH".to_vec()), opt(*top)],
            DestView::Xyz { left, top, zoom } => {
                vec![Object::Name(b"XYZ".to_vec()), opt(*left), opt(*top), opt(*zoom)]
            }
            DestView::Rect(r) => vec![
                Object::Name(b"FitR".to_vec()),
                Object::Real(r[0]),
                Object::Real(r[1]),
                Object::Real(r[2]),
                Object::Real(r[3]),
            ],
        }
    }

    /// Short description: kind and parameters.
    pub fn describe(&self) -> String {
        let f = |v: &Option<f32>| v.map(crate::util::fmt_num).unwrap_or_else(|| "—".into());
        match self {
            DestView::Fit => "Fit".into(),
            DestView::FitWidth(top) => format!("FitH top {}", f(top)),
            DestView::Xyz { left, top, zoom } => format!(
                "XYZ left {} top {} zoom {}",
                f(left),
                f(top),
                zoom.map(|z| format!("{}%", (z * 100.0).round()))
                    .unwrap_or_else(|| "—".into())
            ),
            DestView::Rect(r) => format!(
                "FitR {} {} {} {}",
                crate::util::fmt_num(r[0]),
                crate::util::fmt_num(r[1]),
                crate::util::fmt_num(r[2]),
                crate::util::fmt_num(r[3])
            ),
        }
    }
}

/// Parse the view part of a destination array.
pub(crate) fn dest_view(doc: &lopdf::Document, arr: &[Object]) -> Option<DestView> {
    let kind = arr.get(1).and_then(|k| k.as_name().ok())?;
    let n = |i: usize| arr.get(i).and_then(|o| num(deref(doc, o)));
    Some(match kind {
        b"XYZ" => DestView::Xyz {
            left: n(2),
            top: n(3),
            zoom: n(4).filter(|z| *z > 0.0),
        },
        b"FitH" | b"FitBH" => DestView::FitWidth(n(2)),
        b"FitR" => DestView::Rect([n(2)?, n(3)?, n(4)?, n(5)?]),
        _ => DestView::Fit,
    })
}

/// New target of a bookmark.
#[derive(Debug, Clone, PartialEq)]
pub enum BookmarkTarget {
    /// Leave the target as it is.
    Keep,
    /// A page, with a view (the top of the page when `None`).
    Page(usize, Option<DestView>),
    /// A web address.
    Uri(String),
}

/// One bookmark with its children.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct OutlineItem {
    pub title: String,
    /// Zero-based target page, when it could be resolved.
    pub page: Option<usize>,
    pub children: Vec<OutlineItem>,
    /// Text colour (`/C`).
    pub color: Option<[f32; 3]>,
    pub bold: bool,
    pub italic: bool,
    /// Shown expanded when the document opens.
    pub open: bool,
    /// The bookmark has a working target (a page of the document, or another action).
    pub valid: bool,
    /// Description of a target that is not a page (web address, file, script).
    pub action: String,
    /// View of a page target.
    pub view: Option<DestView>,
    /// Destination (`/Dest`) or action (`/A`) as stored in the file.
    pub(crate) target: Option<(Vec<u8>, Object)>,
    /// Other entries of the outline item, kept when the tree is rewritten.
    pub(crate) extra: Dictionary,
}

impl OutlineItem {
    pub fn new(title: impl Into<String>, page: Option<usize>) -> Self {
        OutlineItem {
            title: title.into(),
            page,
            valid: page.is_some(),
            ..Default::default()
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

/// The bookmark at `path`.
pub fn item_at<'a>(items: &'a [OutlineItem], path: &[usize]) -> Option<&'a OutlineItem> {
    let (first, rest) = path.split_first()?;
    let item = items.get(*first)?;
    if rest.is_empty() {
        Some(item)
    } else {
        item_at(&item.children, rest)
    }
}

/// The list holding the item at `path` and the item's index in it.
fn siblings_mut<'a>(
    items: &'a mut Vec<OutlineItem>,
    path: &[usize],
) -> Result<(&'a mut Vec<OutlineItem>, usize)> {
    let (last, parent) = path
        .split_last()
        .ok_or_else(|| Error::Invalid("empty bookmark path".into()))?;
    let list = if parent.is_empty() {
        items
    } else {
        &mut get_path(items, parent)
            .ok_or_else(|| Error::Invalid("bookmark path not found".into()))?
            .children
    };
    Ok((list, *last))
}

fn take_at(items: &mut Vec<OutlineItem>, path: &[usize]) -> Result<OutlineItem> {
    let (list, i) = siblings_mut(items, path)?;
    if i >= list.len() {
        return Err(Error::Invalid("bookmark path not found".into()));
    }
    Ok(list.remove(i))
}

fn insert_at(items: &mut Vec<OutlineItem>, path: &[usize], item: OutlineItem) -> Result<()> {
    let (list, i) = siblings_mut(items, path)?;
    list.insert(i.min(list.len()), item);
    Ok(())
}

/// A path after the item at `removed` was taken out of the tree; `None` when
/// the path pointed inside the removed item.
fn adjust_after_removal(path: &[usize], removed: &[usize]) -> Option<Vec<usize>> {
    if path.len() >= removed.len() && path[..removed.len()] == *removed {
        return None;
    }
    let mut p = path.to_vec();
    let d = removed.len() - 1;
    if p.len() > d && p[..d] == removed[..d] && p[d] > removed[d] {
        p[d] -= 1;
    }
    Some(p)
}

/// Every path of the tree in document order.
fn all_paths(items: &[OutlineItem], prefix: &mut Vec<usize>, out: &mut Vec<Vec<usize>>) {
    for (i, item) in items.iter().enumerate() {
        prefix.push(i);
        out.push(prefix.clone());
        all_paths(&item.children, prefix, out);
        prefix.pop();
    }
}

const KNOWN_KEYS: &[&[u8]] = &[
    b"Title", b"Parent", b"Prev", b"Next", b"First", b"Last", b"Count", b"Dest", b"A", b"C", b"F",
];

impl Document {
    /// Destination array `[page /View ...]` for a page of the document.
    pub(crate) fn dest_array(&self, page: usize, view: &DestView) -> Result<Object> {
        let page_id = self.page_id(page)?;
        let mut arr = vec![Object::Reference(page_id)];
        arr.extend(view.to_objects());
        Ok(Object::Array(arr))
    }

    /// Default view of a new destination: the top of the page, current zoom.
    pub fn top_of_page_view(&self, page: usize) -> DestView {
        let top = self
            .page_sizes()
            .get(page)
            .map(|s| s.crop_box[3])
            .unwrap_or(792.0);
        DestView::Xyz {
            left: None,
            top: Some(top),
            zoom: None,
        }
    }

    /// Page and view of a destination object, following named destinations.
    pub(crate) fn destination_view(&self, dest: &Object, depth: usize) -> Option<DestView> {
        if depth > 8 {
            return None;
        }
        let doc = &self.inner;
        match deref(doc, dest) {
            Object::Array(arr) => dest_view(doc, arr),
            Object::Dictionary(d) => d.get(b"D").ok().and_then(|d| self.destination_view(d, depth + 1)),
            Object::String(..) | Object::Name(_) => {
                let target = self.named_destination_object(dest)?;
                self.destination_view(&target, depth + 1)
            }
            _ => None,
        }
    }

    fn read_items(
        &self,
        first: Option<ObjectId>,
        seen: &mut HashSet<ObjectId>,
        depth: usize,
    ) -> Vec<OutlineItem> {
        let doc = &self.inner;
        let mut out = Vec::new();
        let mut cur = first;
        while let Some(id) = cur {
            if !seen.insert(id) || depth > 64 || seen.len() > 100_000 {
                break;
            }
            let Ok(d) = doc.get_dictionary(id) else { break };
            let title = dict_text(doc, d, b"Title").unwrap_or_default();
            let target = d
                .get(b"Dest")
                .ok()
                .map(|o| (b"Dest".to_vec(), o.clone()))
                .or_else(|| d.get(b"A").ok().map(|o| (b"A".to_vec(), o.clone())));
            let (page, valid, action, view) = match &target {
                Some((key, obj)) if key == b"Dest" => {
                    let p = self.resolve_destination(obj);
                    (p, p.is_some(), String::new(), self.destination_view(obj, 0))
                }
                Some((_, obj)) => {
                    let a = deref(doc, obj).as_dict().ok();
                    let kind = a
                        .and_then(|a| a.get(b"S").ok())
                        .and_then(|s| s.as_name().ok())
                        .map(|s| String::from_utf8_lossy(s).to_string())
                        .unwrap_or_default();
                    match kind.as_str() {
                        "GoTo" => {
                            let dest = a.and_then(|a| a.get(b"D").ok());
                            let p = dest.and_then(|d| self.resolve_destination(d));
                            (
                                p,
                                p.is_some(),
                                String::new(),
                                dest.and_then(|d| self.destination_view(d, 0)),
                            )
                        }
                        "URI" => (
                            None,
                            true,
                            a.and_then(|a| a.get(b"URI").ok())
                                .and_then(|u| text_of(doc, u))
                                .unwrap_or_default(),
                            None,
                        ),
                        "" => (None, false, String::new(), None),
                        other => (None, true, other.to_string(), None),
                    }
                }
                None => (None, false, String::new(), None),
            };
            let flags = d.get(b"F").ok().and_then(|f| f.as_i64().ok()).unwrap_or(0);
            let count = d.get(b"Count").ok().and_then(|c| c.as_i64().ok()).unwrap_or(0);
            let first_child = d.get(b"First").ok().and_then(|f| f.as_reference().ok());
            let mut extra = Dictionary::new();
            for (k, v) in d.iter() {
                if !KNOWN_KEYS.contains(&k.as_slice()) {
                    extra.set(k.clone(), v.clone());
                }
            }
            let next = d.get(b"Next").ok().and_then(|n| n.as_reference().ok());
            let color = d
                .get(b"C")
                .ok()
                .and_then(|c| color_of(doc, c))
                .filter(|c| c.iter().any(|v| *v > 0.001));
            out.push(OutlineItem {
                title,
                page,
                children: self.read_items(first_child, seen, depth + 1),
                color,
                bold: flags & 2 != 0,
                italic: flags & 1 != 0,
                open: count > 0,
                valid,
                action,
                view,
                target,
                extra,
            });
            cur = next;
        }
        out
    }

    /// The bookmark tree.
    pub fn outline(&self) -> Vec<OutlineItem> {
        self.kept("outline", || self.read_outline()).as_ref().clone()
    }

    fn read_outline(&self) -> Vec<OutlineItem> {
        let doc = &self.inner;
        let first = doc
            .catalog()
            .ok()
            .and_then(|c| c.get(b"Outlines").ok())
            .map(|o| deref(doc, o))
            .and_then(|o| o.as_dict().ok())
            .and_then(|o| o.get(b"First").ok())
            .and_then(|f| f.as_reference().ok());
        let mut seen = HashSet::new();
        self.read_items(first, &mut seen, 0)
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

    /// Replace the whole outline (one undo step).
    pub fn set_outline(&mut self, items: &[OutlineItem]) -> Result<()> {
        self.begin_edit();
        self.write_outline(items)?;
        self.commit()
    }

    /// Ids of the outline root and every outline item reachable from it.
    fn outline_objects(&self) -> Vec<ObjectId> {
        let doc = &self.inner;
        let mut out = Vec::new();
        let Some(root) = doc
            .catalog()
            .ok()
            .and_then(|c| c.get(b"Outlines").ok())
            .and_then(|o| o.as_reference().ok())
        else {
            return out;
        };
        out.push(root);
        let mut stack: Vec<ObjectId> = doc
            .get_dictionary(root)
            .ok()
            .and_then(|d| d.get(b"First").ok())
            .and_then(|f| f.as_reference().ok())
            .into_iter()
            .collect();
        let mut seen = HashSet::new();
        while let Some(id) = stack.pop() {
            if !seen.insert(id) || seen.len() > 100_000 {
                continue;
            }
            out.push(id);
            if let Ok(d) = doc.get_dictionary(id) {
                for key in [&b"Next"[..], b"First"] {
                    if let Some(r) = d.get(key).ok().and_then(|o| o.as_reference().ok()) {
                        stack.push(r);
                    }
                }
            }
        }
        out
    }

    pub(crate) fn write_outline(&mut self, items: &[OutlineItem]) -> Result<()> {
        for id in self.outline_objects() {
            self.inner.objects.remove(&id);
        }
        if items.is_empty() {
            self.inner.catalog_mut()?.remove(b"Outlines");
            return Ok(());
        }
        let page_ids = self.page_ids();
        let root = self.inner.new_object_id();
        fn build(
            doc: &mut Document,
            items: &[OutlineItem],
            parent: ObjectId,
            pages: &[ObjectId],
        ) -> Result<(ObjectId, ObjectId, i64)> {
            let ids: Vec<ObjectId> = items.iter().map(|_| doc.inner.new_object_id()).collect();
            let mut visible = 0i64;
            for (i, item) in items.iter().enumerate() {
                let id = ids[i];
                let mut d = item.extra.clone();
                d.set("Title", lopdf::text_string(&item.title));
                d.set("Parent", Object::Reference(parent));
                if i > 0 {
                    d.set("Prev", Object::Reference(ids[i - 1]));
                }
                if i + 1 < ids.len() {
                    d.set("Next", Object::Reference(ids[i + 1]));
                }
                match &item.target {
                    Some((key, obj)) => d.set(key.clone(), obj.clone()),
                    None => {
                        if let Some(&page_id) = item.page.and_then(|p| pages.get(p)) {
                            let mut arr = vec![Object::Reference(page_id)];
                            arr.extend(item.view.clone().unwrap_or(DestView::Fit).to_objects());
                            d.set("Dest", Object::Array(arr));
                        }
                    }
                }
                if let Some(c) = item.color {
                    d.set("C", vec![c[0].into(), c[1].into(), c[2].into()]);
                }
                let flags = (item.italic as i64) | ((item.bold as i64) << 1);
                if flags != 0 {
                    d.set("F", flags);
                }
                let mut shown = 1;
                if !item.children.is_empty() {
                    let (first, last, child_visible) = build(doc, &item.children, id, pages)?;
                    d.set("First", Object::Reference(first));
                    d.set("Last", Object::Reference(last));
                    d.set("Count", if item.open { child_visible } else { -child_visible });
                    if item.open {
                        shown += child_visible;
                    }
                }
                doc.inner.objects.insert(id, Object::Dictionary(d));
                visible += shown;
            }
            Ok((ids[0], ids[ids.len() - 1], visible))
        }
        let (first, last, visible) = build(self, items, root, &page_ids)?;
        let mut root_dict = Dictionary::new();
        root_dict.set("Type", Object::Name(b"Outlines".to_vec()));
        root_dict.set("First", Object::Reference(first));
        root_dict.set("Last", Object::Reference(last));
        root_dict.set("Count", visible);
        self.inner.objects.insert(root, Object::Dictionary(root_dict));
        self.inner.catalog_mut()?.set("Outlines", Object::Reference(root));
        Ok(())
    }

    /// Change the outline with `f` (one undo step).
    fn edit_outline(&mut self, f: impl FnOnce(&mut Vec<OutlineItem>) -> Result<()>) -> Result<()> {
        let mut items = self.outline();
        f(&mut items)?;
        self.set_outline(&items)
    }

    /// A new bookmark pointing at `page` with `view`.
    pub fn new_bookmark_item(&self, title: &str, page: usize, view: Option<DestView>) -> Result<OutlineItem> {
        if page >= self.page_count() {
            return Err(Error::InvalidPage(page));
        }
        let view = view.unwrap_or_else(|| self.top_of_page_view(page));
        Ok(OutlineItem {
            title: title.to_string(),
            page: Some(page),
            valid: true,
            view: Some(view),
            ..Default::default()
        })
    }

    /// Append a bookmark under `parent_path` (empty slice = top level).
    pub fn add_bookmark(&mut self, title: &str, page: usize, parent_path: &[usize]) -> Result<()> {
        let item = self.new_bookmark_item(title, page, None)?;
        self.edit_outline(|items| {
            if parent_path.is_empty() {
                items.push(item);
            } else {
                let parent = get_path(items, parent_path)
                    .ok_or_else(|| Error::Invalid("bookmark path not found".into()))?;
                parent.children.push(item);
                parent.open = true;
            }
            Ok(())
        })
    }

    /// Insert a bookmark so that it ends up at `path` (the parent must exist).
    pub fn insert_bookmark(
        &mut self,
        path: &[usize],
        title: &str,
        page: usize,
        view: Option<DestView>,
    ) -> Result<Vec<usize>> {
        let item = self.new_bookmark_item(title, page, view)?;
        let mut final_path = path.to_vec();
        self.edit_outline(|items| {
            if let Some((_, parent)) = path.split_last() {
                if !parent.is_empty() {
                    let p = get_path(items, parent)
                        .ok_or_else(|| Error::Invalid("bookmark path not found".into()))?;
                    p.open = true;
                }
            }
            let (list, i) = siblings_mut(items, path)?;
            let i = i.min(list.len());
            list.insert(i, item);
            if let Some(last) = final_path.last_mut() {
                *last = i;
            }
            Ok(())
        })?;
        Ok(final_path)
    }

    /// Delete the bookmark at `path` (children are removed too).
    pub fn delete_bookmark(&mut self, path: &[usize]) -> Result<()> {
        self.edit_outline(|items| take_at(items, path).map(|_| ()))
    }

    /// Delete the bookmarks at `paths` and their children (one undo step).
    pub fn delete_bookmarks(&mut self, paths: &[Vec<usize>]) -> Result<()> {
        let mut sorted = paths.to_vec();
        // Deepest and last first, so earlier paths stay valid.
        sorted.sort();
        sorted.dedup();
        sorted.reverse();
        self.edit_outline(|items| {
            for p in &sorted {
                let _ = take_at(items, p);
            }
            Ok(())
        })
    }

    /// Delete the children of the bookmark at `path`, keeping the bookmark.
    pub fn delete_bookmark_children(&mut self, path: &[usize]) -> Result<()> {
        self.edit_outline(|items| {
            let item =
                get_path(items, path).ok_or_else(|| Error::Invalid("bookmark path not found".into()))?;
            item.children.clear();
            Ok(())
        })
    }

    /// Delete the bookmark at `path`; its children take its place.
    pub fn delete_bookmark_keep_children(&mut self, path: &[usize]) -> Result<()> {
        self.edit_outline(|items| {
            let item = take_at(items, path)?;
            let (list, i) = siblings_mut(items, path)?;
            for (k, child) in item.children.into_iter().enumerate() {
                list.insert(i + k, child);
            }
            Ok(())
        })
    }

    /// Rename the bookmark at `path`.
    pub fn rename_bookmark(&mut self, path: &[usize], title: &str) -> Result<()> {
        self.edit_outline(|items| {
            let item =
                get_path(items, path).ok_or_else(|| Error::Invalid("bookmark path not found".into()))?;
            item.title = title.to_string();
            Ok(())
        })
    }

    /// Point the bookmark at `path` to the top of another page.
    pub fn set_bookmark_page(&mut self, path: &[usize], page: usize) -> Result<()> {
        self.set_bookmark_target(path, page, None)
    }

    /// Point the bookmark at `path` to `page` with `view` (top of the page by default).
    pub fn set_bookmark_target(&mut self, path: &[usize], page: usize, view: Option<DestView>) -> Result<()> {
        if page >= self.page_count() {
            return Err(Error::InvalidPage(page));
        }
        let view = view.unwrap_or_else(|| self.top_of_page_view(page));
        self.edit_outline(|items| {
            let item =
                get_path(items, path).ok_or_else(|| Error::Invalid("bookmark path not found".into()))?;
            item.page = Some(page);
            item.view = Some(view);
            item.target = None;
            item.valid = true;
            item.action.clear();
            Ok(())
        })
    }

    /// Make the bookmark at `path` open a web address.
    pub fn set_bookmark_uri(&mut self, path: &[usize], uri: &str) -> Result<()> {
        if uri.trim().is_empty() {
            return Err(Error::Invalid("the web address is empty".into()));
        }
        let action = Object::Dictionary(dictionary! {
            "S" => "URI",
            "URI" => Object::string_literal(uri.trim().to_string()),
        });
        self.edit_outline(|items| {
            let item =
                get_path(items, path).ok_or_else(|| Error::Invalid("bookmark path not found".into()))?;
            item.target = Some((b"A".to_vec(), action));
            item.page = None;
            item.view = None;
            item.valid = true;
            item.action = uri.trim().to_string();
            Ok(())
        })
    }

    /// Change title, target, colour and style of a bookmark in one undo step.
    pub fn set_bookmark_properties(
        &mut self,
        path: &[usize],
        title: &str,
        target: BookmarkTarget,
        color: Option<[f32; 3]>,
        bold: bool,
        italic: bool,
    ) -> Result<()> {
        let (page_view, uri) = match target {
            BookmarkTarget::Keep => (None, None),
            BookmarkTarget::Page(page, view) => {
                if page >= self.page_count() {
                    return Err(Error::InvalidPage(page));
                }
                (
                    Some((page, view.unwrap_or_else(|| self.top_of_page_view(page)))),
                    None,
                )
            }
            BookmarkTarget::Uri(u) => {
                if u.trim().is_empty() {
                    return Err(Error::Invalid("the web address is empty".into()));
                }
                (None, Some(u.trim().to_string()))
            }
        };
        self.edit_outline(|items| {
            let item =
                get_path(items, path).ok_or_else(|| Error::Invalid("bookmark path not found".into()))?;
            item.title = title.to_string();
            item.color = color;
            item.bold = bold;
            item.italic = italic;
            if let Some((page, view)) = page_view {
                item.page = Some(page);
                item.view = Some(view);
                item.target = None;
                item.valid = true;
                item.action.clear();
            }
            if let Some(u) = uri {
                item.target = Some((
                    b"A".to_vec(),
                    Object::Dictionary(
                        dictionary! { "S" => "URI", "URI" => Object::string_literal(u.clone()) },
                    ),
                ));
                item.page = None;
                item.view = None;
                item.valid = true;
                item.action = u;
            }
            Ok(())
        })
    }

    /// Colour and style of the bookmark at `path`.
    pub fn set_bookmark_style(
        &mut self,
        path: &[usize],
        color: Option<[f32; 3]>,
        bold: bool,
        italic: bool,
    ) -> Result<()> {
        self.edit_outline(|items| {
            let item =
                get_path(items, path).ok_or_else(|| Error::Invalid("bookmark path not found".into()))?;
            item.color = color;
            item.bold = bold;
            item.italic = italic;
            Ok(())
        })
    }

    /// Whether the bookmarks at `paths` show expanded when the document opens.
    pub fn set_bookmarks_open(&mut self, paths: &[Vec<usize>], open: bool) -> Result<()> {
        self.edit_outline(|items| {
            for p in paths {
                if let Some(item) = get_path(items, p) {
                    item.open = open;
                }
            }
            Ok(())
        })
    }

    /// Move the bookmark at `from` so that it becomes child `index` of the
    /// bookmark at `to_parent` (paths refer to the tree before the move; an
    /// empty parent is the top level). Returns the new path.
    pub fn move_bookmark(&mut self, from: &[usize], to_parent: &[usize], index: usize) -> Result<Vec<usize>> {
        let mut new_path = Vec::new();
        self.edit_outline(|items| {
            let parent = adjust_after_removal(to_parent, from)
                .ok_or_else(|| Error::Invalid("a bookmark cannot move inside itself".into()))?;
            let mut index = index;
            if from.len() == to_parent.len() + 1
                && from[..to_parent.len()] == *to_parent
                && index > from[from.len() - 1]
            {
                index -= 1;
            }
            let item = take_at(items, from)?;
            let mut target = parent.clone();
            target.push(index);
            if !parent.is_empty() {
                let p = get_path(items, &parent)
                    .ok_or_else(|| Error::Invalid("bookmark path not found".into()))?;
                p.open = true;
                target[parent.len()] = index.min(p.children.len());
            } else {
                target[0] = index.min(items.len());
            }
            insert_at(items, &target, item)?;
            new_path = target;
            Ok(())
        })?;
        Ok(new_path)
    }

    /// Move a bookmark before its previous sibling.
    pub fn move_bookmark_up(&mut self, path: &[usize]) -> Result<Vec<usize>> {
        let (last, parent) = path
            .split_last()
            .ok_or_else(|| Error::Invalid("empty bookmark path".into()))?;
        if *last == 0 {
            return Err(Error::Invalid("the bookmark is already the first".into()));
        }
        self.move_bookmark(path, parent, last - 1)
    }

    /// Move a bookmark after its next sibling.
    pub fn move_bookmark_down(&mut self, path: &[usize]) -> Result<Vec<usize>> {
        let (last, parent) = path
            .split_last()
            .ok_or_else(|| Error::Invalid("empty bookmark path".into()))?;
        let siblings = if parent.is_empty() {
            self.outline().len()
        } else {
            item_at(&self.outline(), parent)
                .map(|p| p.children.len())
                .unwrap_or(0)
        };
        if last + 1 >= siblings {
            return Err(Error::Invalid("the bookmark is already the last".into()));
        }
        self.move_bookmark(path, parent, last + 2)
    }

    /// Move a bookmark one level up: it follows its parent.
    pub fn promote_bookmark(&mut self, path: &[usize]) -> Result<Vec<usize>> {
        if path.len() < 2 {
            return Err(Error::Invalid("the bookmark is already at the top level".into()));
        }
        let parent = &path[..path.len() - 1];
        let (pi, grand) = parent.split_last().expect("parent path");
        self.move_bookmark(path, grand, pi + 1)
    }

    /// Move a bookmark one level down: it becomes the last child of its previous sibling.
    pub fn demote_bookmark(&mut self, path: &[usize]) -> Result<Vec<usize>> {
        let (last, parent) = path
            .split_last()
            .ok_or_else(|| Error::Invalid("empty bookmark path".into()))?;
        if *last == 0 {
            return Err(Error::Invalid(
                "the bookmark has no previous bookmark to go under".into(),
            ));
        }
        let mut prev = parent.to_vec();
        prev.push(last - 1);
        let n = item_at(&self.outline(), &prev)
            .map(|p| p.children.len())
            .unwrap_or(0);
        self.move_bookmark(path, &prev, n)
    }

    /// Create one bookmark per page titled "Page N" (useful for scanned documents).
    pub fn bookmarks_from_pages(&mut self) -> Result<()> {
        let items = (0..self.page_count())
            .map(|p| self.new_bookmark_item(&format!("Page {}", p + 1), p, None))
            .collect::<Result<Vec<_>>>()?;
        self.set_outline(&items)
    }

    /// Paths of bookmarks whose destination does not resolve, in document order.
    pub fn invalid_bookmarks(&self) -> Vec<Vec<usize>> {
        let items = self.outline();
        let mut paths = Vec::new();
        all_paths(&items, &mut Vec::new(), &mut paths);
        paths
            .into_iter()
            .filter(|p| item_at(&items, p).map(|i| !i.valid).unwrap_or(false))
            .collect()
    }

    /// Every bookmark path in document order.
    pub fn bookmark_paths(&self) -> Vec<Vec<usize>> {
        let mut paths = Vec::new();
        all_paths(&self.outline(), &mut Vec::new(), &mut paths);
        paths
    }

    /// Pages of the section a bookmark starts: from its page up to the page
    /// before the next bookmark at the same or a higher level.
    pub fn bookmark_section_pages(&self, path: &[usize]) -> Vec<usize> {
        let items = self.outline();
        let Some(start) = item_at(&items, path).and_then(|i| i.page) else {
            return Vec::new();
        };
        let paths = self.bookmark_paths();
        let pos = paths.iter().position(|p| p == path).unwrap_or(0);
        let end = paths[pos + 1..]
            .iter()
            .filter(|p| p.len() <= path.len())
            .filter_map(|p| item_at(&items, p).and_then(|i| i.page))
            .find(|p| *p > start)
            .unwrap_or(self.page_count());
        (start..end.max(start + 1)).collect()
    }

    /// Export bookmarks as indented text: one line per bookmark, a tab per
    /// level, then the title and the page number separated by a tab. `only`
    /// limits the export to the given bookmarks and their children.
    pub fn export_bookmarks_text(&self, only: Option<&[Vec<usize>]>) -> String {
        fn walk(items: &[OutlineItem], depth: usize, out: &mut String) {
            for item in items {
                out.push_str(&"\t".repeat(depth));
                out.push_str(&item.title.replace(['\t', '\n', '\r'], " "));
                if let Some(p) = item.page {
                    out.push('\t');
                    out.push_str(&(p + 1).to_string());
                }
                out.push('\n');
                walk(&item.children, depth + 1, out);
            }
        }
        let items = self.outline();
        let mut out = String::new();
        match only {
            Some(paths) => {
                for p in paths {
                    if let Some(item) = item_at(&items, p) {
                        walk(std::slice::from_ref(item), 0, &mut out);
                    }
                }
            }
            None => walk(&items, 0, &mut out),
        }
        out
    }

    /// Bookmarks parsed from indented text (tabs or groups of two spaces per
    /// level; an optional page number after the last tab).
    pub fn parse_bookmarks_text(&self, text: &str) -> Vec<OutlineItem> {
        let mut roots: Vec<OutlineItem> = Vec::new();
        let mut stack: Vec<(usize, Vec<usize>)> = Vec::new();
        let n = self.page_count();
        for raw in text.lines() {
            if raw.trim().is_empty() {
                continue;
            }
            let mut level = 0;
            let mut rest = raw;
            loop {
                if let Some(r) = rest.strip_prefix('\t') {
                    level += 1;
                    rest = r;
                } else if let Some(r) = rest.strip_prefix("  ") {
                    level += 1;
                    rest = r;
                } else {
                    break;
                }
            }
            let rest = rest.trim_end();
            let (title, page) = match rest.rsplit_once('\t') {
                Some((t, p)) if p.trim().parse::<usize>().is_ok() => {
                    (t.trim().to_string(), p.trim().parse::<usize>().ok())
                }
                _ => (rest.trim().to_string(), None),
            };
            let page = page
                .map(|p| p.saturating_sub(1).min(n.saturating_sub(1)))
                .unwrap_or(0);
            let Ok(item) = self.new_bookmark_item(&title, page, None) else {
                continue;
            };
            while stack.last().map(|(l, _)| *l >= level).unwrap_or(false) {
                stack.pop();
            }
            let path = match stack.last() {
                None => {
                    roots.push(item);
                    vec![roots.len() - 1]
                }
                Some((_, parent)) => {
                    let p = get_path(&mut roots, parent).expect("parent path");
                    p.children.push(item);
                    p.open = true;
                    let mut path = parent.clone();
                    path.push(p.children.len() - 1);
                    path
                }
            };
            stack.push((level, path));
        }
        roots
    }

    /// Append bookmarks from indented text at the end of the outline (one undo
    /// step). Returns the number of bookmarks added.
    pub fn import_bookmarks_text(&mut self, text: &str) -> Result<usize> {
        let new_items = self.parse_bookmarks_text(text);
        let count: usize = new_items.iter().map(|i| i.count()).sum();
        if count == 0 {
            return Err(Error::Invalid("no bookmarks found in the text".into()));
        }
        self.edit_outline(|items| {
            items.extend(new_items);
            Ok(())
        })?;
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_adjust_after_removal() {
        assert_eq!(adjust_after_removal(&[3], &[1]), Some(vec![2]));
        assert_eq!(adjust_after_removal(&[0], &[1]), Some(vec![0]));
        assert_eq!(adjust_after_removal(&[2, 4], &[1]), Some(vec![1, 4]));
        assert_eq!(adjust_after_removal(&[1, 0], &[1]), None);
        assert_eq!(adjust_after_removal(&[1, 3], &[1, 2]), Some(vec![1, 2]));
        assert_eq!(adjust_after_removal(&[0, 3], &[1, 2]), Some(vec![0, 3]));
    }
}
