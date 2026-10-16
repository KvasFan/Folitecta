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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NamedDestination {
    pub name: String,
    pub page: Option<usize>,
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
    fn named_dest_map(&self) -> Vec<(Vec<u8>, Object)> {
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
            })
            .collect();
        out.sort_by(|a, b| a.name.cmp(&b.name));
        out
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
        let page_id = self.page_id(page)?;
        self.begin_edit();
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
        self.commit()?;
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
