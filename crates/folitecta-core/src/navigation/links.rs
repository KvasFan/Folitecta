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
    /// A named action: `NextPage`, `PrevPage`, `FirstPage`, `LastPage` and
    /// the others readers know.
    NamedAction(String),
    /// Sending the form data to an address.
    Submit(String),
    /// Clearing the form.
    Reset,
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
            LinkTarget::NamedAction(n) => format!("Named action {n}"),
            LinkTarget::Submit(u) => format!("Submit form to {u}"),
            LinkTarget::Reset => "Reset form".into(),
            LinkTarget::Other(s) => s.clone(),
        }
    }
}

/// One thing pressing a push button does.
#[derive(Debug, Clone, PartialEq)]
pub enum ButtonAction {
    Target(LinkTarget),
    /// A script, for the sandbox of the script engine.
    Script(String),
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
    /// The index of every page object id.
    fn page_indices(&self) -> std::sync::Arc<HashMap<ObjectId, usize>> {
        self.page_index_map().unwrap_or_else(|| {
            std::sync::Arc::new(
                self.page_ids()
                    .into_iter()
                    .enumerate()
                    .map(|(i, id)| (id, i))
                    .collect(),
            )
        })
    }

    /// Raw named destination table (name -> destination object).
    pub(crate) fn named_dest_map(&self) -> Vec<(Vec<u8>, Object)> {
        self.kept("named_dest_map", || self.read_named_dest_map())
            .as_ref()
            .clone()
    }

    /// The named destinations by name (the first of a name when it repeats).
    fn named_dest_index(&self) -> std::sync::Arc<HashMap<Vec<u8>, Object>> {
        self.kept("named_dest_index", || {
            let mut map = HashMap::new();
            for (k, v) in self.kept("named_dest_map", || self.read_named_dest_map()).iter() {
                map.entry(k.clone()).or_insert_with(|| v.clone());
            }
            map
        })
    }

    fn read_named_dest_map(&self) -> Vec<(Vec<u8>, Object)> {
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
                    Object::Reference(r) => self.page_indices().get(r).copied(),
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
                let map = self.named_dest_index();
                let target = map.get(&key)?;
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
        self.named_dest_index().get(&key).cloned()
    }

    /// Entries of the `/Names /Dests` name tree.
    pub(crate) fn names_tree_entries(&self) -> Vec<(Vec<u8>, Object)> {
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
    pub(crate) fn dests_dict_mut(&mut self) -> Option<&mut Dictionary> {
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
    pub(crate) fn write_names_tree(&mut self, mut entries: Vec<(Vec<u8>, Object)>) -> Result<()> {
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

    /// The first action of the push button `id` (its widget or field), when
    /// released (`/AA /U`) or activated (`/A`).
    fn button_action(&self, id: ObjectId) -> Option<Object> {
        let doc = &self.inner;
        let d = doc.get_dictionary(id).ok()?;
        let mut candidates = vec![d.clone()];
        if let Some(kids) = d.get(b"Kids").ok().and_then(|k| k.as_array().ok()) {
            candidates.extend(kids.iter().filter_map(|k| deref(doc, k).as_dict().ok().cloned()));
        }
        candidates.into_iter().find_map(|c| {
            c.get(b"A").ok().cloned().or_else(|| {
                c.get(b"AA")
                    .ok()
                    .map(|a| deref(doc, a))
                    .and_then(|a| a.as_dict().ok())
                    .and_then(|aa| aa.get(b"U").ok().cloned())
            })
        })
    }

    /// What pressing the push button of field `id` does first.
    pub fn button_target(&self, id: ObjectId) -> Option<LinkTarget> {
        let action = self.button_action(id)?;
        Some(self.link_target(&lopdf::dictionary! { "A" => action }))
    }

    /// Everything pressing the push button of field `id` does, in order: its
    /// action and those its `/Next` chain goes on to.
    pub fn button_actions(&self, id: ObjectId) -> Vec<ButtonAction> {
        let mut out = Vec::new();
        if let Some(action) = self.button_action(id) {
            self.action_chain(&action, &mut out, 0);
        }
        out
    }

    /// Everything following the link `index` of `page` does, in order (see
    /// [`Link::index`]).
    pub fn link_actions(&self, page: usize, index: usize) -> Vec<ButtonAction> {
        let doc = &self.inner;
        let annot = self.page_id(page).ok().and_then(|id| {
            let page = doc.get_dictionary(id).ok()?;
            let list = deref(doc, page.get(b"Annots").ok()?).as_array().ok()?;
            deref(doc, list.get(index)?).as_dict().ok()
        });
        let Some(annot) = annot else {
            return Vec::new();
        };
        let mut out = Vec::new();
        match annot.get(b"A") {
            Ok(action) => self.action_chain(action, &mut out, 0),
            Err(_) => out.push(ButtonAction::Target(self.link_target(annot))),
        }
        out
    }

    /// The link object `index` of `page` (its place in the page's `/Annots`).
    fn link_object(&self, page: usize, index: usize) -> Result<ObjectId> {
        let doc = &self.inner;
        let page_dict = doc.get_dictionary(self.page_id(page)?)?;
        let list = deref(doc, page_dict.get(b"Annots")?).as_array()?;
        let id = list
            .get(index)
            .and_then(|a| a.as_reference().ok())
            .ok_or_else(|| Error::Invalid("the link cannot be edited".into()))?;
        let d = doc.get_dictionary(id)?;
        if d.get(b"Subtype").ok().and_then(|s| s.as_name().ok()) != Some(b"Link") {
            return Err(Error::Invalid("not a link".into()));
        }
        Ok(id)
    }

    /// The actions of a link as they are written, one dictionary each,
    /// without their `/Next` (a `/Dest` of its own is made a go-to).
    fn raw_chain(&self, annot: &Dictionary) -> Vec<Dictionary> {
        fn walk(doc: &lopdf::Document, o: &Object, out: &mut Vec<Dictionary>, depth: usize) {
            if depth > 32 || out.len() > 64 {
                return;
            }
            match deref(doc, o) {
                Object::Array(list) => list.iter().for_each(|a| walk(doc, a, out, depth + 1)),
                Object::Dictionary(d) => {
                    let mut own = d.clone();
                    own.remove(b"Next");
                    out.push(own);
                    if let Ok(next) = d.get(b"Next") {
                        walk(doc, next, out, depth + 1);
                    }
                }
                _ => {}
            }
        }
        let doc = &self.inner;
        let mut out = Vec::new();
        match annot.get(b"A") {
            Ok(a) => walk(doc, a, &mut out, 0),
            Err(_) => {
                if let Ok(dest) = annot.get(b"Dest") {
                    out.push(dictionary! { "S" => "GoTo", "D" => dest.clone() });
                }
            }
        }
        out
    }

    /// The action dictionary that does `a`.
    fn action_dict(&self, a: &ButtonAction) -> Result<Dictionary> {
        Ok(match a {
            ButtonAction::Script(code) => {
                if code.trim().is_empty() {
                    return Err(Error::Invalid("the script is empty".into()));
                }
                dictionary! { "S" => "JavaScript", "JS" => lopdf::text_string(code) }
            }
            ButtonAction::Target(LinkTarget::Uri(u)) if !u.trim().is_empty() => {
                dictionary! { "S" => "URI", "URI" => Object::string_literal(u.trim().to_string()) }
            }
            ButtonAction::Target(LinkTarget::Page(p)) => {
                let target = self.page_id(*p)?;
                dictionary! { "S" => "GoTo", "D" => vec![Object::Reference(target), Object::Name(b"Fit".to_vec())] }
            }
            ButtonAction::Target(LinkTarget::Named(n)) if !n.is_empty() => {
                dictionary! { "S" => "GoTo", "D" => Object::string_literal(n.clone()) }
            }
            ButtonAction::Target(LinkTarget::NamedAction(n)) if !n.is_empty() => {
                dictionary! { "S" => "Named", "N" => Object::Name(n.as_bytes().to_vec()) }
            }
            ButtonAction::Target(LinkTarget::Reset) => dictionary! { "S" => "ResetForm" },
            _ => return Err(Error::Invalid("this action cannot be written".into())),
        })
    }

    /// Change the list of actions of link `index` of `page` with `change`
    /// (the actions as written; at least one must stay). One undo step.
    fn change_link_actions(
        &mut self,
        page: usize,
        index: usize,
        change: impl FnOnce(&Self, &mut Vec<Dictionary>) -> Result<()>,
    ) -> Result<()> {
        let id = self.link_object(page, index)?;
        let annot = self.inner.get_dictionary(id)?.clone();
        let mut list = self.raw_chain(&annot);
        change(self, &mut list)?;
        if list.is_empty() {
            return Err(Error::Invalid(
                "a link keeps at least one action: delete the link instead".into(),
            ));
        }
        let mut first = list.remove(0);
        if !list.is_empty() {
            first.set(
                "Next",
                Object::Array(list.into_iter().map(Object::Dictionary).collect()),
            );
        }
        self.edit_step(|doc| {
            let d = doc.inner.get_dictionary_mut(id)?;
            d.remove(b"Dest");
            d.set("A", Object::Dictionary(first));
            Ok(())
        })
    }

    /// Add `action` to the actions of link `index` of `page`, last.
    pub fn add_link_action(&mut self, page: usize, index: usize, action: &ButtonAction) -> Result<()> {
        let new = self.action_dict(action)?;
        self.change_link_actions(page, index, |_, list| {
            list.push(new);
            Ok(())
        })
    }

    /// Put `action` in place of action `at` of link `index` of `page`.
    pub fn replace_link_action(
        &mut self,
        page: usize,
        index: usize,
        at: usize,
        action: &ButtonAction,
    ) -> Result<()> {
        let new = self.action_dict(action)?;
        self.change_link_actions(page, index, |_, list| {
            let slot = list
                .get_mut(at)
                .ok_or_else(|| Error::Invalid("no such action".into()))?;
            *slot = new;
            Ok(())
        })
    }

    /// Move action `at` of link `index` of `page` one place up (earlier) or
    /// down; at an end it stays.
    pub fn move_link_action(&mut self, page: usize, index: usize, at: usize, up: bool) -> Result<()> {
        self.change_link_actions(page, index, |_, list| {
            if at >= list.len() {
                return Err(Error::Invalid("no such action".into()));
            }
            let to = if up {
                at.checked_sub(1)
            } else {
                (at + 1 < list.len()).then_some(at + 1)
            };
            if let Some(to) = to {
                list.swap(at, to);
            }
            Ok(())
        })
    }

    /// Remove action `at` of link `index` of `page` (one stays).
    pub fn remove_link_action(&mut self, page: usize, index: usize, at: usize) -> Result<()> {
        self.change_link_actions(page, index, |_, list| {
            if at >= list.len() {
                return Err(Error::Invalid("no such action".into()));
            }
            list.remove(at);
            Ok(())
        })
    }

    fn action_chain(&self, action: &Object, out: &mut Vec<ButtonAction>, depth: usize) {
        // A chain that turns back on itself ends here.
        if depth > 32 || out.len() > 64 {
            return;
        }
        let doc = &self.inner;
        match deref(doc, action) {
            Object::Array(list) => list.iter().for_each(|a| self.action_chain(a, out, depth + 1)),
            Object::Dictionary(d) => {
                let script =
                    d.get(b"S").ok().and_then(|s| s.as_name().ok()) == Some(b"JavaScript".as_slice());
                out.push(if script {
                    ButtonAction::Script(crate::interactive::scripts::script_code(doc, d).unwrap_or_default())
                } else {
                    ButtonAction::Target(self.link_target(&lopdf::dictionary! { "A" => d.clone() }))
                });
                if let Ok(next) = d.get(b"Next") {
                    self.action_chain(next, out, depth + 1);
                }
            }
            _ => {}
        }
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
                b"Named" => LinkTarget::NamedAction(
                    action
                        .get(b"N")
                        .ok()
                        .and_then(|n| text_of(doc, n))
                        .unwrap_or_default(),
                ),
                b"SubmitForm" => LinkTarget::Submit(
                    action
                        .get(b"F")
                        .ok()
                        .and_then(|f| self.file_spec_name(f))
                        .unwrap_or_default(),
                ),
                b"ResetForm" => LinkTarget::Reset,
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

    /// How link `index` of page `page` (its position among the comments of
    /// the page) shows a click (`/H`; inverted when it says nothing).
    pub fn link_highlight(&self, page: usize, index: usize) -> Result<crate::ButtonBehaviour> {
        let id = self.annotation_object(page, index)?;
        let d = self.inner.get_dictionary(id)?;
        Ok(match d.get(b"H").ok().and_then(|h| h.as_name().ok()) {
            Some(b"N") => crate::ButtonBehaviour::None,
            Some(b"O") => crate::ButtonBehaviour::Outline,
            Some(b"P") => crate::ButtonBehaviour::Push,
            _ => crate::ButtonBehaviour::Invert,
        })
    }

    /// Set how link `index` of page `page` shows a click. One undo step.
    pub fn set_link_highlight(
        &mut self,
        page: usize,
        index: usize,
        mode: crate::ButtonBehaviour,
    ) -> Result<()> {
        let id = self.annotation_object(page, index)?;
        if self
            .inner
            .get_dictionary(id)?
            .get(b"Subtype")
            .ok()
            .and_then(|s| s.as_name().ok())
            != Some(b"Link")
        {
            return Err(Error::Invalid("only links have a highlight mode".into()));
        }
        self.edit_step(|doc| {
            doc.inner
                .get_dictionary_mut(id)?
                .set("H", Object::Name(mode.name().as_bytes().to_vec()));
            Ok(())
        })
    }

    /// Links of the whole document.
    pub fn all_links(&self) -> Vec<Link> {
        self.kept("all_links", || {
            (0..self.page_count())
                .filter_map(|p| self.links(p).ok())
                .flatten()
                .collect::<Vec<_>>()
        })
        .as_ref()
        .clone()
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
    pub(crate) fn push_link_raw(
        &mut self,
        page: usize,
        rect: [f32; 4],
        action: Dictionary,
    ) -> Result<ObjectId> {
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

    /// Links › Import: the links of the pages of `other` made again on the
    /// same pages here (as many pages as both have): web links, links to a
    /// page (the same page here, or the last one) and to named destinations
    /// (by name when this document has it, else to the page it reaches
    /// there). One undo step; the links made and those left out (other
    /// actions).
    pub fn import_links_from(&mut self, other: &Document) -> Result<(usize, usize)> {
        let n = self.page_count();
        let pages = n.min(other.page_count());
        let own: std::collections::HashSet<String> =
            self.named_destinations().into_iter().map(|d| d.name).collect();
        let theirs: HashMap<String, Option<usize>> = other
            .named_destinations()
            .into_iter()
            .map(|d| (d.name, d.page))
            .collect();
        let mark = self.undo_mark();
        let (mut made, mut left) = (0usize, 0usize);
        let result = (|| -> Result<()> {
            for p in 0..pages {
                for l in other.links(p)? {
                    let r = match &l.target {
                        LinkTarget::Uri(u) if !u.trim().is_empty() => Some(self.add_uri_link(p, l.rect, u)),
                        LinkTarget::Page(t) => Some(self.add_page_link(p, l.rect, (*t).min(n - 1))),
                        LinkTarget::Named(name) if own.contains(name) => {
                            Some(self.add_named_link(p, l.rect, name))
                        }
                        LinkTarget::Named(name) => match theirs.get(name).copied().flatten() {
                            Some(t) => Some(self.add_page_link(p, l.rect, t.min(n - 1))),
                            None => None,
                        },
                        _ => None,
                    };
                    match r {
                        Some(r) => {
                            r?;
                            made += 1;
                        }
                        None => left += 1,
                    }
                }
            }
            Ok(())
        })();
        self.squash_undo(&mark);
        if let Err(e) = result {
            if made > 0 {
                let _ = self.undo();
            }
            return Err(e);
        }
        Ok((made, left))
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

/// What an action of a link does, for its view and layer editors.
#[derive(Debug, Clone, PartialEq)]
pub enum ActionInfo {
    /// A go-to: the page, the view and the name of the destination, when it
    /// has one.
    GoTo {
        page: Option<usize>,
        view: Option<crate::outline::DestView>,
        named: Option<String>,
    },
    /// Layers turned on, off or toggled: the word of the action (`ON`,
    /// `OFF`, `Toggle`, or `Mixed` when it has several) and the layers.
    Layers {
        state: String,
        layers: Vec<ObjectId>,
    },
    Other,
}

/// What a layer action does to its layers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayerState {
    On,
    Off,
    Toggle,
}

impl LayerState {
    fn word(self) -> &'static [u8] {
        match self {
            LayerState::On => b"ON",
            LayerState::Off => b"OFF",
            LayerState::Toggle => b"Toggle",
        }
    }
}

impl Document {
    /// What action `at` of link `index` of `page` does.
    pub fn link_action_info(&self, page: usize, index: usize, at: usize) -> Option<ActionInfo> {
        let id = self.link_object(page, index).ok()?;
        let annot = self.inner.get_dictionary(id).ok()?.clone();
        let a = self.raw_chain(&annot).into_iter().nth(at)?;
        let kind = a
            .get(b"S")
            .ok()
            .and_then(|s| s.as_name().ok())
            .unwrap_or_default();
        Some(match kind {
            b"GoTo" => {
                let d = a.get(b"D").ok()?;
                let named = match deref(&self.inner, d) {
                    Object::String(b, _) => Some(
                        lopdf::decode_text_string(&Object::String(b.clone(), lopdf::StringFormat::Literal))
                            .unwrap_or_default(),
                    ),
                    Object::Name(n) => Some(String::from_utf8_lossy(n).to_string()),
                    _ => None,
                };
                let target = match &named {
                    Some(_) => self.named_destination_object(d),
                    None => Some(deref(&self.inner, d).clone()),
                };
                let page = target.as_ref().and_then(|t| match deref(&self.inner, t) {
                    Object::Array(arr) => arr
                        .first()
                        .and_then(|o| o.as_reference().ok())
                        .and_then(|r| self.page_ids().iter().position(|p| *p == r)),
                    Object::Dictionary(d) => d
                        .get(b"D")
                        .ok()
                        .and_then(|d| deref(&self.inner, d).as_array().ok())
                        .and_then(|arr| arr.first())
                        .and_then(|o| o.as_reference().ok())
                        .and_then(|r| self.page_ids().iter().position(|p| *p == r)),
                    _ => None,
                });
                ActionInfo::GoTo {
                    page,
                    view: self.destination_view(d, 0),
                    named,
                }
            }
            b"SetOCGState" => {
                let mut words = Vec::new();
                let mut layers = Vec::new();
                for o in a
                    .get(b"State")
                    .ok()
                    .and_then(|s| s.as_array().ok())
                    .into_iter()
                    .flatten()
                {
                    match o {
                        Object::Name(n) => words.push(String::from_utf8_lossy(n).to_string()),
                        Object::Reference(r) => layers.push(*r),
                        _ => {}
                    }
                }
                words.dedup();
                let state = if words.len() == 1 {
                    words.remove(0)
                } else {
                    "Mixed".into()
                };
                ActionInfo::Layers { state, layers }
            }
            _ => ActionInfo::Other,
        })
    }

    /// Make action `at` of link `index` of `page` a go-to of page
    /// `target` with `view` (with `at` past the end, add it). One undo step.
    pub fn set_link_goto(
        &mut self,
        page: usize,
        index: usize,
        at: usize,
        target: usize,
        view: &crate::outline::DestView,
    ) -> Result<()> {
        let d = self.dest_array(target, view)?;
        let new = dictionary! { "S" => "GoTo", "D" => d };
        self.change_link_actions(page, index, |_, list| {
            match list.get_mut(at) {
                Some(slot) => *slot = new,
                None => list.push(new),
            }
            Ok(())
        })
    }

    /// Give the go-to `at` of link `index` of `page` the named destination
    /// `name`: its destination renamed when it has a name, else a named
    /// destination made of its page and view and the go-to pointed at it.
    /// One undo step.
    pub fn name_link_goto(&mut self, page: usize, index: usize, at: usize, name: &str) -> Result<()> {
        let name = name.trim();
        if name.is_empty() {
            return Err(Error::Invalid("the destination name is empty".into()));
        }
        match self.link_action_info(page, index, at) {
            Some(ActionInfo::GoTo { named: Some(old), .. }) => self.rename_named_destination(&old, name),
            Some(ActionInfo::GoTo {
                page: Some(target),
                view,
                named: None,
            }) => {
                let mark = self.undo_mark();
                self.add_named_destination(name, target, view)?;
                let new = dictionary! { "S" => "GoTo", "D" => lopdf::text_string(name) };
                let r = self.change_link_actions(page, index, |_, list| {
                    let slot = list
                        .get_mut(at)
                        .ok_or_else(|| Error::Invalid("no such action".into()))?;
                    *slot = new;
                    Ok(())
                });
                self.squash_undo(&mark);
                if r.is_err() {
                    let _ = self.undo();
                }
                r
            }
            _ => Err(Error::Invalid(
                "the action is not a go-to of this document".into(),
            )),
        }
    }

    /// Make action `at` of link `index` of `page` turn `layers` on, off or
    /// toggle them (None: every layer of the document); with `at` past the
    /// end, add it. One undo step.
    pub fn set_link_layer_action(
        &mut self,
        page: usize,
        index: usize,
        at: usize,
        state: LayerState,
        layers: Option<&[ObjectId]>,
    ) -> Result<()> {
        let layers: Vec<ObjectId> = match layers {
            Some(l) => l.to_vec(),
            None => self.layers().into_iter().map(|l| l.id).collect(),
        };
        if layers.is_empty() {
            return Err(Error::Invalid("the document has no layers".into()));
        }
        let mut list = vec![Object::Name(state.word().to_vec())];
        list.extend(layers.into_iter().map(Object::Reference));
        let new = dictionary! { "S" => "SetOCGState", "State" => list, "PreserveRB" => true };
        self.change_link_actions(page, index, |_, list| {
            match list.get_mut(at) {
                Some(slot) => *slot = new,
                None => list.push(new),
            }
            Ok(())
        })
    }

    /// Simplify Actions: layer actions next to each other become one (in
    /// their order), and an action that repeats the one before it goes.
    /// One undo step; returns the actions taken away.
    pub fn simplify_link_actions(&mut self, page: usize, index: usize) -> Result<usize> {
        let mut removed = 0;
        let id = self.link_object(page, index)?;
        let annot = self.inner.get_dictionary(id)?.clone();
        let before = self.raw_chain(&annot);
        let mut out: Vec<Dictionary> = Vec::new();
        for a in before {
            let layers = a.get(b"S").ok().and_then(|s| s.as_name().ok()) == Some(b"SetOCGState");
            if let Some(last) = out.last_mut() {
                if *last == a {
                    removed += 1;
                    continue;
                }
                let last_layers = last.get(b"S").ok().and_then(|s| s.as_name().ok()) == Some(b"SetOCGState");
                if layers && last_layers {
                    let mut state = last
                        .get(b"State")
                        .ok()
                        .and_then(|s| s.as_array().ok())
                        .cloned()
                        .unwrap_or_default();
                    state.extend(
                        a.get(b"State")
                            .ok()
                            .and_then(|s| s.as_array().ok())
                            .cloned()
                            .unwrap_or_default(),
                    );
                    last.set("State", state);
                    removed += 1;
                    continue;
                }
            }
            out.push(a);
        }
        if removed > 0 {
            self.change_link_actions(page, index, move |_, list| {
                *list = out;
                Ok(())
            })?;
        }
        Ok(removed)
    }
}
