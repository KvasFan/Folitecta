//! Link annotations and named destinations.

use std::collections::HashMap;

use lopdf::{Dictionary, Object, ObjectId, dictionary};

use crate::document::Document;
use crate::error::{Error, Result};
use crate::util::{deref, rect_of, text_of, walk_name_tree};

#[derive(Debug, Clone, PartialEq)]
pub enum LinkTarget {
    Uri(String),
    Page(usize),
    Named(String),
    Launch(String),
    Remote(String),
    Other(String),
}

impl LinkTarget {
    pub fn describe(&self) -> String {
        match self {
            LinkTarget::Uri(u) => u.clone(),
            LinkTarget::Page(p) => format!("Page {}", p + 1),
            LinkTarget::Named(n) => format!("Destination '{n}'"),
            LinkTarget::Launch(f) => format!("Open file {f}"),
            LinkTarget::Remote(f) => format!("Remote document {f}"),
            LinkTarget::Other(s) => s.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Link {
    pub page: usize,
    /// Index inside the page `/Annots` array.
    pub index: usize,
    pub rect: [f32; 4],
    pub target: LinkTarget,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NamedDestination {
    pub name: String,
    pub page: Option<usize>,
    /// How the page is shown.
    pub view: Option<crate::outline::DestView>,
}

impl Document {
    fn page_index_map(&self) -> HashMap<ObjectId, usize> {
        self.page_ids()
            .into_iter()
            .enumerate()
            .map(|(i, id)| (id, i))
            .collect()
    }

    /// Raw named destination table (name -> destination object).
    pub(crate) fn named_dest_map(&self) -> Vec<(Vec<u8>, Object)> {
        let doc = &self.inner;
        let mut out = Vec::new();
        let Ok(catalog) = doc.catalog() else { return out };
        if let Some(dests) = catalog
            .get(b"Dests")
            .ok()
            .map(|d| deref(doc, d))
            .and_then(|d| d.as_dict().ok())
        {
            for (k, v) in dests.iter() {
                out.push((k.clone(), v.clone()));
            }
        }
        if let Some(names) = catalog
            .get(b"Names")
            .ok()
            .map(|n| deref(doc, n))
            .and_then(|n| n.as_dict().ok())
        {
            if let Some(tree) = names
                .get(b"Dests")
                .ok()
                .map(|d| deref(doc, d))
                .and_then(|d| d.as_dict().ok())
            {
                walk_name_tree(doc, tree, &mut out, 0);
            }
        }
        out
    }

    /// Resolve a destination object (array, dictionary with `/D`, or name) to a page index.
    pub fn resolve_destination(&self, dest: &Object) -> Option<usize> {
        self.resolve_destination_inner(dest, 0)
    }

    fn resolve_destination_inner(&self, dest: &Object, depth: usize) -> Option<usize> {
        if depth > 8 {
            return None;
        }
        let doc = &self.inner;
        let dest = deref(doc, dest);
        match dest {
            Object::Array(arr) => {
                let first = arr.first()?;
                match first {
                    Object::Reference(r) => self.page_index_map().get(r).copied(),
                    Object::Integer(i) if *i >= 0 => Some(*i as usize),
                    _ => None,
                }
            }
            Object::Dictionary(d) => d
                .get(b"D")
                .ok()
                .and_then(|d| self.resolve_destination_inner(d, depth + 1)),
            Object::String(..) | Object::Name(_) => {
                let key: Vec<u8> = match dest {
                    Object::String(s, _) => s.clone(),
                    Object::Name(n) => n.clone(),
                    _ => return None,
                };
                let map = self.named_dest_map();
                let (_, target) = map.iter().find(|(k, _)| *k == key)?;
                self.resolve_destination_inner(target, depth + 1)
            }
            _ => None,
        }
    }

    /// Named destinations sorted by name.
    pub fn named_destinations(&self) -> Vec<NamedDestination> {
        let mut out: Vec<NamedDestination> = self
            .named_dest_map()
            .into_iter()
            .map(|(k, v)| NamedDestination {
                name: String::from_utf8_lossy(&k).to_string(),
                page: self.resolve_destination(&v),
                view: self.destination_view(&v, 0),
            })
            .collect();
        out.sort_by(|a, b| a.name.cmp(&b.name));
        out
    }

    /// Target of a destination given by name (string or name object).
    pub(crate) fn named_destination_object(&self, dest: &Object) -> Option<Object> {
        let key: Vec<u8> = match deref(&self.inner, dest) {
            Object::String(s, _) => s.clone(),
            Object::Name(n) => n.clone(),
            _ => return None,
        };
        self.named_dest_map()
            .into_iter()
            .find(|(k, _)| *k == key)
            .map(|(_, v)| v)
    }

    /// Entries of the `/Names /Dests` name tree.
    fn names_tree_entries(&self) -> Vec<(Vec<u8>, Object)> {
        let doc = &self.inner;
        let mut out = Vec::new();
        if let Some(tree) = doc
            .catalog()
            .ok()
            .and_then(|c| c.get(b"Names").ok())
            .map(|n| deref(doc, n))
            .and_then(|n| n.as_dict().ok())
            .and_then(|n| n.get(b"Dests").ok())
            .map(|d| deref(doc, d))
            .and_then(|d| d.as_dict().ok())
        {
            walk_name_tree(doc, tree, &mut out, 0);
        }
        out
    }

    /// The catalog `/Dests` dictionary (destinations named by name objects).
    fn dests_dict_mut(&mut self) -> Option<&mut Dictionary> {
        let r = self.inner.catalog().ok()?.get(b"Dests").ok()?.clone();
        match r {
            Object::Reference(id) => self.inner.get_dictionary_mut(id).ok(),
            Object::Dictionary(_) => match self.inner.catalog_mut().ok()?.get_mut(b"Dests").ok()? {
                Object::Dictionary(d) => Some(d),
                _ => None,
            },
            _ => None,
        }
    }

    /// Rewrite the `/Names /Dests` tree as one sorted leaf.
    fn write_names_tree(&mut self, mut entries: Vec<(Vec<u8>, Object)>) -> Result<()> {
        entries.sort_by(|a, b| a.0.cmp(&b.0));
        entries.dedup_by(|a, b| a.0 == b.0);
        let names_ref = self.inner.catalog()?.get(b"Names").ok().cloned();
        let mut names = match &names_ref {
            Some(Object::Reference(r)) => self.inner.get_dictionary(*r)?.clone(),
            Some(Object::Dictionary(d)) => d.clone(),
            _ => Dictionary::new(),
        };
        if entries.is_empty() {
            names.remove(b"Dests");
        } else {
            let arr: Vec<Object> = entries
                .into_iter()
                .flat_map(|(k, v)| [Object::String(k, lopdf::StringFormat::Literal), v])
                .collect();
            let tree = self.inner.add_object(dictionary! { "Names" => arr });
            names.set("Dests", Object::Reference(tree));
        }
        match names_ref {
            Some(Object::Reference(r)) => {
                self.inner.objects.insert(r, Object::Dictionary(names));
            }
            _ => {
                if names.is_empty() {
                    self.inner.catalog_mut()?.remove(b"Names");
                } else {
                    self.inner.catalog_mut()?.set("Names", Object::Dictionary(names));
                }
            }
        }
        Ok(())
    }

    fn named_destination_exists(&self, name: &str) -> bool {
        self.named_dest_map().iter().any(|(k, _)| k == name.as_bytes())
    }

    /// Add a named destination pointing at `page` (one undo step).
    pub fn add_named_destination(
        &mut self,
        name: &str,
        page: usize,
        view: Option<crate::outline::DestView>,
    ) -> Result<()> {
        let name = name.trim();
        if name.is_empty() {
            return Err(Error::Invalid("the destination name is empty".into()));
        }
        if self.named_destination_exists(name) {
            return Err(Error::Invalid(format!(
                "a destination named '{name}' already exists"
            )));
        }
        let view = view.unwrap_or_else(|| self.top_of_page_view(page));
        let dest = self.dest_array(page, &view)?;
        self.begin_edit();
        let mut entries = self.names_tree_entries();
        entries.push((name.as_bytes().to_vec(), dest));
        self.write_names_tree(entries)?;
        self.commit()
    }

    /// Point an existing named destination at `page` with `view` (one undo step).
    pub fn set_named_destination(
        &mut self,
        name: &str,
        page: usize,
        view: Option<crate::outline::DestView>,
    ) -> Result<()> {
        if !self.named_destination_exists(name) {
            return Err(Error::Invalid(format!("no destination named '{name}'")));
        }
        let view = view.unwrap_or_else(|| self.top_of_page_view(page));
        let dest = self.dest_array(page, &view)?;
        self.begin_edit();
        let key = name.as_bytes().to_vec();
        let in_dict = self.dests_dict_mut().map(|d| d.has(&key)).unwrap_or(false);
        if in_dict {
            if let Some(d) = self.dests_dict_mut() {
                d.set(key, dest);
            }
        } else {
            let mut entries = self.names_tree_entries();
            for e in entries.iter_mut().filter(|e| e.0 == key) {
                e.1 = dest.clone();
            }
            self.write_names_tree(entries)?;
        }
        self.commit()
    }

    /// Delete named destinations (one undo step). Returns how many were removed.
    pub fn delete_named_destinations(&mut self, names: &[String]) -> Result<usize> {
        let keys: Vec<Vec<u8>> = names.iter().map(|n| n.as_bytes().to_vec()).collect();
        self.begin_edit();
        match self.delete_named_destination_keys(&keys) {
            Ok(n) => {
                self.commit()?;
                Ok(n)
            }
            Err(e) => {
                self.abandon_edit();
                Err(e)
            }
        }
    }

    /// Delete the named destinations whose names are these bytes, without an
    /// undo step. Returns how many were removed.
    pub(crate) fn delete_named_destination_keys(&mut self, keys: &[Vec<u8>]) -> Result<usize> {
        let before = self.named_dest_map().len();
        if let Some(d) = self.dests_dict_mut() {
            for k in keys {
                d.remove(k);
            }
        }
        let entries: Vec<_> = self
            .names_tree_entries()
            .into_iter()
            .filter(|(k, _)| !keys.contains(k))
            .collect();
        self.write_names_tree(entries)?;
        Ok(before.saturating_sub(self.named_dest_map().len()))
    }

    /// Delete named destinations whose page does not exist (one undo step).
    pub fn remove_invalid_named_destinations(&mut self) -> Result<usize> {
        let invalid: Vec<String> = self
            .named_destinations()
            .into_iter()
            .filter(|d| d.page.is_none())
            .map(|d| d.name)
            .collect();
        if invalid.is_empty() {
            return Ok(0);
        }
        self.delete_named_destinations(&invalid)
    }

    /// Rename a named destination and update the links, bookmarks and actions
    /// that use it (one undo step).
    pub fn rename_named_destination(&mut self, old: &str, new: &str) -> Result<()> {
        let new = new.trim();
        if new.is_empty() {
            return Err(Error::Invalid("the destination name is empty".into()));
        }
        if old == new {
            return Ok(());
        }
        if self.named_destination_exists(new) {
            return Err(Error::Invalid(format!(
                "a destination named '{new}' already exists"
            )));
        }
        self.begin_edit();
        if let Err(e) = self.rename_named_destination_in(old.as_bytes(), new.as_bytes()) {
            self.abandon_edit();
            return Err(e);
        }
        self.commit()
    }

    /// Rename a named destination and the references to it, without an
    /// undo step of its own (the caller makes one).
    pub(crate) fn rename_named_destination_in(&mut self, old: &[u8], new: &[u8]) -> Result<()> {
        let (old_k, new_k) = (old.to_vec(), new.to_vec());
        let moved = self.dests_dict_mut().and_then(|d| d.remove(&old_k));
        if let Some(v) = moved {
            if let Some(d) = self.dests_dict_mut() {
                d.set(new_k.clone(), v);
            }
        } else {
            let entries: Vec<_> = self
                .names_tree_entries()
                .into_iter()
                .map(|(k, v)| if k == old_k { (new_k.clone(), v) } else { (k, v) })
                .collect();
            self.write_names_tree(entries)?;
        }
        // References by name: /Dest of links and bookmarks, /D of GoTo actions.
        for obj in self.inner.objects.values_mut() {
            let dict = match obj {
                Object::Dictionary(d) => d,
                Object::Stream(s) => &mut s.dict,
                _ => continue,
            };
            for key in [&b"Dest"[..], b"D"] {
                if let Ok(v) = dict.get_mut(key) {
                    match v {
                        Object::String(s, _) | Object::Name(s) if *s == old_k => *s = new_k.clone(),
                        _ => {}
                    }
                }
            }
            // Inline actions of links and bookmarks.
            if let Ok(Object::Dictionary(a)) = dict.get_mut(b"A") {
                if let Ok(v) = a.get_mut(b"D") {
                    match v {
                        Object::String(s, _) | Object::Name(s) if *s == old_k => *s = new_k.clone(),
                        _ => {}
                    }
                }
            }
        }
        Ok(())
    }

    fn link_target(&self, d: &Dictionary) -> LinkTarget {
        let doc = &self.inner;
        if let Some(action) = d
            .get(b"A")
            .ok()
            .map(|a| deref(doc, a))
            .and_then(|a| a.as_dict().ok())
        {
            let s = action
                .get(b"S")
                .ok()
                .and_then(|s| s.as_name().ok())
                .unwrap_or(b"");
            return match s {
                b"URI" => LinkTarget::Uri(
                    action
                        .get(b"URI")
                        .ok()
                        .and_then(|u| text_of(doc, u))
                        .unwrap_or_default(),
                ),
                b"GoTo" => match action.get(b"D").ok() {
                    Some(dest) => match self.resolve_destination(dest) {
                        Some(p) => LinkTarget::Page(p),
                        None => LinkTarget::Named(text_of(doc, dest).unwrap_or_default()),
                    },
                    None => LinkTarget::Other("GoTo".into()),
                },
                b"GoToR" => LinkTarget::Remote(
                    action
                        .get(b"F")
                        .ok()
                        .and_then(|f| self.file_spec_name(f))
                        .unwrap_or_default(),
                ),
                b"Launch" => LinkTarget::Launch(
                    action
                        .get(b"F")
                        .ok()
                        .and_then(|f| self.file_spec_name(f))
                        .unwrap_or_default(),
                ),
                b"Named" => LinkTarget::Other(format!(
                    "Named action {}",
                    action
                        .get(b"N")
                        .ok()
                        .and_then(|n| text_of(doc, n))
                        .unwrap_or_default()
                )),
                b"JavaScript" => LinkTarget::Other("JavaScript".into()),
                other => LinkTarget::Other(String::from_utf8_lossy(other).to_string()),
            };
        }
        if let Ok(dest) = d.get(b"Dest") {
            return match self.resolve_destination(dest) {
                Some(p) => LinkTarget::Page(p),
                None => LinkTarget::Named(text_of(doc, dest).unwrap_or_default()),
            };
        }
        LinkTarget::Other("No action".into())
    }

    fn file_spec_name(&self, obj: &Object) -> Option<String> {
        let doc = &self.inner;
        match deref(doc, obj) {
            Object::Dictionary(d) => d
                .get(b"UF")
                .or_else(|_| d.get(b"F"))
                .ok()
                .and_then(|f| text_of(doc, f)),
            other => text_of(doc, other),
        }
    }

    /// Links on a page.
    pub fn links(&self, page: usize) -> Result<Vec<Link>> {
        let id = self.page_id(page)?;
        let doc = &self.inner;
        let dict = doc.get_dictionary(id)?;
        let mut out = Vec::new();
        if let Ok(arr) = dict
            .get(b"Annots")
            .map(|a| deref(doc, a))
            .and_then(|a| a.as_array())
        {
            for (index, a) in arr.iter().enumerate() {
                let Ok(d) = deref(doc, a).as_dict() else { continue };
                if d.get(b"Subtype").ok().and_then(|s| s.as_name().ok()) != Some(b"Link") {
                    continue;
                }
                out.push(Link {
                    page,
                    index,
                    rect: d
                        .get(b"Rect")
                        .ok()
                        .and_then(|r| rect_of(doc, r))
                        .unwrap_or([0.0; 4]),
                    target: self.link_target(d),
                });
            }
        }
        Ok(out)
    }

    /// Links of the whole document.
    pub fn all_links(&self) -> Vec<Link> {
        (0..self.page_count())
            .filter_map(|p| self.links(p).ok())
            .flatten()
            .collect()
    }

    fn push_link(&mut self, page: usize, rect: [f32; 4], action: Dictionary) -> Result<ObjectId> {
        self.page_id(page)?;
        self.begin_edit();
        let id = self.push_link_raw(page, rect, action)?;
        self.commit()?;
        Ok(id)
    }

    /// Add web links (page, rectangle, address) in one undo step.
    pub(crate) fn push_uri_links(&mut self, links: &[(usize, [f32; 4], String)]) -> Result<usize> {
        if links.is_empty() {
            return Ok(0);
        }
        self.begin_edit();
        for (page, rect, uri) in links {
            let action =
                dictionary! { "S" => "URI", "URI" => Object::string_literal(uri.trim().to_string()) };
            self.push_link_raw(*page, *rect, action)?;
        }
        self.commit()?;
        Ok(links.len())
    }

    /// Add a link annotation, without an undo step of its own.
    fn push_link_raw(&mut self, page: usize, rect: [f32; 4], action: Dictionary) -> Result<ObjectId> {
        let page_id = self.page_id(page)?;
        let annot = dictionary! {
            "Type" => "Annot",
            "Subtype" => "Link",
            "Rect" => vec![rect[0].into(), rect[1].into(), rect[2].into(), rect[3].into()],
            "Border" => vec![0.into(), 0.into(), 0.into()],
            "P" => page_id,
            "A" => action,
        };
        let annot_id = self.inner.add_object(annot);
        let existing = self.inner.get_dictionary(page_id)?.get(b"Annots").ok().cloned();
        let value = match existing {
            Some(Object::Array(mut arr)) => {
                arr.push(Object::Reference(annot_id));
                Object::Array(arr)
            }
            Some(Object::Reference(r)) => {
                let mut arr = self
                    .inner
                    .get_object(r)
                    .ok()
                    .and_then(|o| o.as_array().ok())
                    .cloned()
                    .unwrap_or_default();
                arr.push(Object::Reference(annot_id));
                Object::Array(arr)
            }
            _ => Object::Array(vec![Object::Reference(annot_id)]),
        };
        self.inner.get_dictionary_mut(page_id)?.set("Annots", value);
        Ok(annot_id)
    }

    /// Add a web link.
    pub fn add_uri_link(&mut self, page: usize, rect: [f32; 4], uri: &str) -> Result<ObjectId> {
        if uri.trim().is_empty() {
            return Err(Error::Invalid("the link address is empty".into()));
        }
        self.push_link(
            page,
            rect,
            dictionary! { "S" => "URI", "URI" => Object::string_literal(uri.trim().to_string()) },
        )
    }

    /// Add a link that jumps to another page of the document.
    pub fn add_page_link(&mut self, page: usize, rect: [f32; 4], target_page: usize) -> Result<ObjectId> {
        let target = self.page_id(target_page)?;
        self.push_link(page, rect, dictionary! { "S" => "GoTo", "D" => vec![Object::Reference(target), Object::Name(b"Fit".to_vec())] })
    }

    /// Add a link that jumps to a named destination of the document.
    pub fn add_named_link(&mut self, page: usize, rect: [f32; 4], name: &str) -> Result<ObjectId> {
        if !self.named_destinations().iter().any(|d| d.name == name) {
            return Err(Error::Invalid(format!("there is no destination named '{name}'")));
        }
        self.push_link(
            page,
            rect,
            dictionary! { "S" => "GoTo", "D" => Object::string_literal(name.to_string()) },
        )
    }

    /// Add a link that opens a file (a path absolute or relative to the document).
    pub fn add_launch_link(&mut self, page: usize, rect: [f32; 4], file: &str) -> Result<ObjectId> {
        let file = file.trim();
        if file.is_empty() {
            return Err(Error::Invalid("no file to open".into()));
        }
        let spec = dictionary! {
            "Type" => "Filespec",
            "F" => Object::string_literal(file.replace('\\', "/")),
            "UF" => lopdf::text_string(file),
        };
        self.push_link(
            page,
            rect,
            dictionary! { "S" => "Launch", "F" => spec, "NewWindow" => true },
        )
    }

    /// Add a link that runs a script when clicked.
    pub fn add_script_link(&mut self, page: usize, rect: [f32; 4], script: &str) -> Result<ObjectId> {
        if script.trim().is_empty() {
            return Err(Error::Invalid("the script is empty".into()));
        }
        self.push_link(
            page,
            rect,
            dictionary! { "S" => "JavaScript", "JS" => lopdf::text_string(script) },
        )
    }

    /// Remove every link annotation of a page (or of the document).
    pub fn remove_links(&mut self, page: Option<usize>) -> Result<usize> {
        let pages: Vec<usize> = match page {
            Some(p) => vec![p],
            None => (0..self.page_count()).collect(),
        };
        self.begin_edit();
        let mut removed = 0;
        for p in pages {
            let page_id = self.page_id(p)?;
            let existing = self.inner.get_dictionary(page_id)?.get(b"Annots").ok().cloned();
            let arr = match existing {
                Some(Object::Array(arr)) => arr,
                Some(Object::Reference(r)) => self
                    .inner
                    .get_object(r)
                    .ok()
                    .and_then(|o| o.as_array().ok())
                    .cloned()
                    .unwrap_or_default(),
                _ => continue,
            };
            let mut keep = Vec::new();
            for a in arr {
                let is_link = deref(&self.inner, &a)
                    .as_dict()
                    .ok()
                    .and_then(|d| d.get(b"Subtype").ok())
                    .and_then(|s| s.as_name().ok())
                    == Some(b"Link");
                if is_link {
                    removed += 1;
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
        self.commit()?;
        Ok(removed)
    }

    /// Scan page text for `http(s)://` addresses and create link annotations is not
    /// possible without glyph positions; this reports the URLs found instead.
    pub fn find_urls_in_text(&self) -> Vec<(usize, String)> {
        let mut out = Vec::new();
        for p in 0..self.page_count() {
            let text = self.page_text(p);
            for word in text.split(|c: char| c.is_whitespace() || c == '<' || c == '>' || c == '"') {
                let w = word.trim_matches(|c: char| matches!(c, '(' | ')' | ',' | '.' | ';'));
                if w.starts_with("http://") || w.starts_with("https://") || w.starts_with("www.") {
                    out.push((p, w.to_string()));
                }
            }
        }
        out
    }
}
