//! Optional content groups (layers): the layer tree, default visibility and
//! locks, new layers and groups, merging, flattening, extraction and
//! importing pages as a layer.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use lopdf::{Dictionary, Object, ObjectId, dictionary};

use crate::content_lex::{Decide, Rewriter, Treat};
use crate::document::Document;
use crate::error::{Error, Result};
use crate::util::{decode_pdf_text, deref, dict_text};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layer {
    pub id: ObjectId,
    pub name: String,
    /// Visible by default (when the document opens).
    pub visible: bool,
    pub locked: bool,
}

/// An entry of the layer tree: a layer with its child layers, or a labelled
/// group that is not a layer itself.
#[derive(Debug, Clone, PartialEq)]
pub enum LayerNode {
    Layer { id: ObjectId, children: Vec<LayerNode> },
    Group { label: String, children: Vec<LayerNode> },
}

impl LayerNode {
    pub fn children(&self) -> &[LayerNode] {
        match self {
            LayerNode::Layer { children, .. } | LayerNode::Group { children, .. } => children,
        }
    }

    fn children_mut(&mut self) -> &mut Vec<LayerNode> {
        match self {
            LayerNode::Layer { children, .. } | LayerNode::Group { children, .. } => children,
        }
    }

    pub fn layer_id(&self) -> Option<ObjectId> {
        match self {
            LayerNode::Layer { id, .. } => Some(*id),
            LayerNode::Group { .. } => None,
        }
    }

    /// Layers of this entry and of everything below it.
    pub fn layer_ids(&self, out: &mut Vec<ObjectId>) {
        if let Some(id) = self.layer_id() {
            out.push(id);
        }
        for c in self.children() {
            c.layer_ids(out);
        }
    }
}

/// The entry at `path` (child indices from the top level).
pub fn node_at<'a>(nodes: &'a [LayerNode], path: &[usize]) -> Option<&'a LayerNode> {
    let (first, rest) = path.split_first()?;
    let n = nodes.get(*first)?;
    if rest.is_empty() {
        Some(n)
    } else {
        node_at(n.children(), rest)
    }
}

fn children_at_mut<'a>(nodes: &'a mut Vec<LayerNode>, path: &[usize]) -> Option<&'a mut Vec<LayerNode>> {
    match path.split_first() {
        None => Some(nodes),
        Some((first, rest)) => children_at_mut(nodes.get_mut(*first)?.children_mut(), rest),
    }
}

/// Paths of every entry, depth first.
pub fn node_paths(nodes: &[LayerNode]) -> Vec<Vec<usize>> {
    fn walk(nodes: &[LayerNode], prefix: &mut Vec<usize>, out: &mut Vec<Vec<usize>>) {
        for (i, n) in nodes.iter().enumerate() {
            prefix.push(i);
            out.push(prefix.clone());
            walk(n.children(), prefix, out);
            prefix.pop();
        }
    }
    let mut out = Vec::new();
    walk(nodes, &mut Vec::new(), &mut out);
    out
}

/// Hide the layers below hidden layers (the parent overrides its children).
pub fn inherit_visibility(nodes: &[LayerNode], states: &mut HashMap<ObjectId, bool>) {
    fn walk(nodes: &[LayerNode], parent_visible: bool, states: &mut HashMap<ObjectId, bool>) {
        for n in nodes {
            let visible = match n.layer_id() {
                Some(id) => {
                    let v = parent_visible && states.get(&id).copied().unwrap_or(true);
                    states.insert(id, v);
                    v
                }
                None => parent_visible,
            };
            walk(n.children(), visible, states);
        }
    }
    walk(nodes, true, states);
}

fn refs_of(doc: &lopdf::Document, dict: &Dictionary, key: &[u8]) -> Vec<ObjectId> {
    dict.get(key)
        .map(|o| deref(doc, o))
        .ok()
        .and_then(|o| o.as_array().ok())
        .map(|arr| arr.iter().filter_map(|o| o.as_reference().ok()).collect())
        .unwrap_or_default()
}

fn is_ocg(doc: &lopdf::Document, id: ObjectId) -> bool {
    doc.get_dictionary(id)
        .map(|d| d.has_type(b"OCG"))
        .unwrap_or(false)
}

fn label_of(obj: &Object) -> Option<String> {
    match obj {
        Object::String(s, _) => Some(decode_pdf_text(s)),
        _ => None,
    }
}

fn parse_order(
    doc: &lopdf::Document,
    arr: &[Object],
    seen: &mut HashSet<ObjectId>,
    depth: usize,
) -> Vec<LayerNode> {
    let mut out: Vec<LayerNode> = Vec::new();
    if depth > 32 {
        return out;
    }
    for item in arr {
        let sub = match item {
            Object::Reference(id) => match doc.get_object(*id) {
                Ok(Object::Array(a)) => Some(a.as_slice()),
                _ => {
                    if is_ocg(doc, *id) && seen.insert(*id) {
                        out.push(LayerNode::Layer {
                            id: *id,
                            children: Vec::new(),
                        });
                    }
                    None
                }
            },
            Object::Array(a) => Some(a.as_slice()),
            _ => None,
        };
        let Some(sub) = sub else { continue };
        if let Some(label) = sub.first().and_then(label_of) {
            let children = parse_order(doc, &sub[1..], seen, depth + 1);
            out.push(LayerNode::Group { label, children });
        } else {
            let children = parse_order(doc, sub, seen, depth + 1);
            match out.last_mut() {
                Some(LayerNode::Layer { children: c, .. }) if c.is_empty() => *c = children,
                _ => out.extend(children),
            }
        }
    }
    out
}

fn write_order(nodes: &[LayerNode]) -> Vec<Object> {
    let mut out = Vec::new();
    for n in nodes {
        match n {
            LayerNode::Layer { id, children } => {
                out.push(Object::Reference(*id));
                if !children.is_empty() {
                    out.push(Object::Array(write_order(children)));
                }
            }
            LayerNode::Group { label, children } => {
                let mut arr = vec![lopdf::text_string(label)];
                arr.extend(write_order(children));
                out.push(Object::Array(arr));
            }
        }
    }
    out
}

/// Remove layers from an `/Order` array; the children of a removed layer take its place.
fn prune_order(arr: &[Object], gone: &HashSet<ObjectId>) -> Vec<Object> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < arr.len() {
        match &arr[i] {
            Object::Reference(id) if gone.contains(id) => {
                if let Some(Object::Array(sub)) = arr.get(i + 1) {
                    if sub.first().and_then(label_of).is_none() {
                        out.extend(prune_order(sub, gone));
                        i += 2;
                        continue;
                    }
                }
            }
            Object::Array(sub) => {
                let p = prune_order(sub, gone);
                let labelled = sub.first().and_then(label_of).is_some();
                if labelled || !p.is_empty() {
                    out.push(Object::Array(p));
                }
            }
            other => out.push(other.clone()),
        }
        i += 1;
    }
    out
}

fn retain_refs(d: &mut Dictionary, key: &[u8], gone: &HashSet<ObjectId>) {
    if let Ok(Object::Array(arr)) = d.get_mut(key) {
        arr.retain(|o| o.as_reference().map(|r| !gone.contains(&r)).unwrap_or(true));
    }
}

/// Remove layers from a configuration dictionary.
fn prune_config(d: &mut Dictionary, gone: &HashSet<ObjectId>) {
    for key in [b"ON".as_slice(), b"OFF", b"Locked"] {
        retain_refs(d, key, gone);
    }
    if let Ok(Object::Array(order)) = d.get(b"Order") {
        let pruned = prune_order(order, gone);
        d.set("Order", Object::Array(pruned));
    }
    if let Ok(Object::Array(groups)) = d.get_mut(b"RBGroups") {
        for g in groups.iter_mut() {
            if let Object::Array(arr) = g {
                arr.retain(|o| o.as_reference().map(|r| !gone.contains(&r)).unwrap_or(true));
            }
        }
        groups.retain(|g| g.as_array().map(|a| a.len() > 1).unwrap_or(true));
    }
    if let Ok(Object::Array(usages)) = d.get_mut(b"AS") {
        for u in usages.iter_mut() {
            if let Object::Dictionary(ud) = u {
                retain_refs(ud, b"OCGs", gone);
            }
        }
    }
}

/// Members of an optional content membership dictionary.
fn ocmd_members(doc: &lopdf::Document, d: &Dictionary) -> Vec<ObjectId> {
    match d.get(b"OCGs") {
        Ok(Object::Reference(r)) if is_ocg(doc, *r) => vec![*r],
        Ok(o) => deref(doc, o)
            .as_array()
            .map(|a| a.iter().filter_map(|x| x.as_reference().ok()).collect())
            .unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

fn stream_data(s: &lopdf::Stream) -> Option<Vec<u8>> {
    if s.dict.has(b"Filter") {
        s.decompressed_content().ok()
    } else {
        Some(s.content.clone())
    }
}

/// Resources of a page, inherited from its parents when needed.
fn page_resources(doc: &lopdf::Document, page_id: ObjectId) -> Dictionary {
    let mut cur = Some(page_id);
    let mut guard = 0;
    while let Some(id) = cur {
        guard += 1;
        let Ok(d) = doc.get_dictionary(id) else { break };
        if let Ok(r) = d.get(b"Resources") {
            return deref(doc, r).as_dict().cloned().unwrap_or_default();
        }
        if guard > 64 {
            break;
        }
        cur = d.get(b"Parent").ok().and_then(|p| p.as_reference().ok());
    }
    Dictionary::new()
}

fn sub_dict(doc: &lopdf::Document, res: &Dictionary, key: &[u8]) -> Dictionary {
    res.get(key)
        .ok()
        .and_then(|o| deref(doc, o).as_dict().ok())
        .cloned()
        .unwrap_or_default()
}

/// Edit the optional content properties (created when missing) and their
/// default configuration.
fn edit_oc_in<R>(
    doc: &mut lopdf::Document,
    f: impl FnOnce(&mut Dictionary, &mut Dictionary) -> R,
) -> Result<R> {
    let (oc_ref, d_ref, mut oc, mut d) = {
        let doc = &*doc;
        let oc_obj = doc.catalog()?.get(b"OCProperties").ok().cloned();
        let oc_ref = oc_obj.as_ref().and_then(|o| o.as_reference().ok());
        let mut oc = oc_obj
            .as_ref()
            .and_then(|o| deref(doc, o).as_dict().ok().cloned())
            .unwrap_or_default();
        let inline = |d: &mut Dictionary, keys: &[&[u8]]| {
            for key in keys {
                if let Ok(Object::Reference(r)) = d.get(key) {
                    if let Ok(o) = doc.get_object(*r) {
                        let o = o.clone();
                        d.set(*key, o);
                    }
                }
            }
        };
        inline(&mut oc, &[b"OCGs"]);
        if !oc.has(b"OCGs") {
            oc.set("OCGs", Object::Array(Vec::new()));
        }
        let d_obj = oc.get(b"D").ok().cloned();
        let d_ref = d_obj.as_ref().and_then(|o| o.as_reference().ok());
        let mut d = d_obj
            .as_ref()
            .and_then(|o| deref(doc, o).as_dict().ok().cloned())
            .unwrap_or_default();
        inline(&mut d, &[b"ON", b"OFF", b"Locked", b"Order", b"RBGroups"]);
        (oc_ref, d_ref, oc, d)
    };
    let r = f(&mut oc, &mut d);
    match d_ref {
        Some(id) => {
            doc.objects.insert(id, Object::Dictionary(d));
        }
        None => oc.set("D", Object::Dictionary(d)),
    }
    match oc_ref {
        Some(id) => {
            doc.objects.insert(id, Object::Dictionary(oc));
        }
        None => {
            doc.catalog_mut()?.set("OCProperties", Object::Dictionary(oc));
        }
    }
    Ok(r)
}

/// Set the default state of the layers in `order` from `states`.
fn write_states(d: &mut Dictionary, order: &[ObjectId], states: &HashMap<ObjectId, bool>) {
    d.remove(b"BaseState");
    let pick = |want: bool| {
        order
            .iter()
            .filter(|id| states.get(id).copied() == Some(want))
            .map(|id| Object::Reference(*id))
            .collect::<Vec<_>>()
    };
    d.set("ON", Object::Array(pick(true)));
    d.set("OFF", Object::Array(pick(false)));
}

/// Which layers an operation changes and which of them keep their content.
struct Plan {
    /// Layers the operation applies to (every layer when `None`).
    affected: Option<HashSet<ObjectId>>,
    /// Affected layers whose content stays (the others lose it).
    keep: HashSet<ObjectId>,
}

impl Plan {
    fn affects(&self, id: ObjectId) -> bool {
        self.affected.as_ref().map(|a| a.contains(&id)).unwrap_or(true)
    }

    fn treat_oc(&self, doc: &lopdf::Document, obj: &Object) -> Treat {
        let Ok(d) = deref(doc, obj).as_dict() else {
            return Treat::Leave;
        };
        if d.has_type(b"OCMD") {
            let members = ocmd_members(doc, d);
            if members.is_empty() || !members.iter().all(|m| self.affects(*m)) {
                return Treat::Leave;
            }
            let on = |m: &ObjectId| self.keep.contains(m);
            let visible = match d.get(b"P").ok().and_then(|p| p.as_name().ok()) {
                Some(b"AllOn") => members.iter().all(on),
                Some(b"AnyOff") => members.iter().any(|m| !on(m)),
                Some(b"AllOff") => members.iter().all(|m| !on(m)),
                _ => members.iter().any(on),
            };
            return if visible { Treat::Keep } else { Treat::Remove };
        }
        match obj.as_reference() {
            Ok(id) if self.affects(id) => {
                if self.keep.contains(&id) {
                    Treat::Keep
                } else {
                    Treat::Remove
                }
            }
            _ => Treat::Leave,
        }
    }
}

struct LayerDecide<'a> {
    doc: &'a lopdf::Document,
    plan: &'a Plan,
    properties: Dictionary,
    xobjects: Dictionary,
    drop_unlayered: bool,
    /// Forms drawn by the stream, with whether they are drawn inside kept layer content.
    forms: Vec<(ObjectId, bool)>,
    /// XObjects whose `/OC` entry goes away.
    strip_oc: Vec<ObjectId>,
}

impl<'a> LayerDecide<'a> {
    fn new(doc: &'a lopdf::Document, plan: &'a Plan, res: &Dictionary, drop_unlayered: bool) -> Self {
        Self {
            doc,
            plan,
            properties: sub_dict(doc, res, b"Properties"),
            xobjects: sub_dict(doc, res, b"XObject"),
            drop_unlayered,
            forms: Vec::new(),
            strip_oc: Vec::new(),
        }
    }
}

impl Decide for LayerDecide<'_> {
    fn marked(&mut self, name: &[u8]) -> Treat {
        self.properties
            .get(name)
            .ok()
            .map(|o| self.plan.treat_oc(self.doc, o))
            .unwrap_or(Treat::Leave)
    }

    fn xobject(&mut self, name: &[u8], visible_context: bool) -> Treat {
        let Some(id) = self.xobjects.get(name).ok().and_then(|o| o.as_reference().ok()) else {
            return Treat::Leave;
        };
        let Ok(stream) = self.doc.get_object(id).and_then(Object::as_stream) else {
            return Treat::Leave;
        };
        let treat = stream
            .dict
            .get(b"OC")
            .ok()
            .map(|oc| self.plan.treat_oc(self.doc, oc))
            .unwrap_or(Treat::Leave);
        if treat == Treat::Keep {
            self.strip_oc.push(id);
        }
        let is_form = stream.dict.get(b"Subtype").ok().and_then(|s| s.as_name().ok()) == Some(b"Form");
        if is_form && treat != Treat::Remove {
            self.forms.push((id, visible_context || treat == Treat::Keep));
        }
        treat
    }

    fn drop_unlayered(&self) -> bool {
        self.drop_unlayered
    }
}

impl Document {
    fn ocproperties(&self) -> Option<&Dictionary> {
        let catalog = self.inner.catalog().ok()?;
        deref(&self.inner, catalog.get(b"OCProperties").ok()?)
            .as_dict()
            .ok()
    }

    fn default_config(&self) -> Option<&Dictionary> {
        let oc = self.ocproperties()?;
        oc.get(b"D")
            .map(|d| deref(&self.inner, d))
            .ok()
            .and_then(|d| d.as_dict().ok())
    }

    /// Layers of the document in declaration order.
    pub fn layers(&self) -> Vec<Layer> {
        let doc = &self.inner;
        let Some(oc) = self.ocproperties() else {
            return Vec::new();
        };
        let ocgs = refs_of(doc, oc, b"OCGs");
        let (on, off, locked, base_off) = match self.default_config() {
            Some(d) => (
                refs_of(doc, d, b"ON"),
                refs_of(doc, d, b"OFF"),
                refs_of(doc, d, b"Locked"),
                d.get(b"BaseState").ok().and_then(|b| b.as_name().ok()) == Some(b"OFF"),
            ),
            None => (Vec::new(), Vec::new(), Vec::new(), false),
        };
        let mut seen = HashSet::new();
        ocgs.into_iter()
            .filter(|id| seen.insert(*id))
            .filter_map(|id| {
                let dict = doc.get_dictionary(id).ok()?;
                let name = dict_text(doc, dict, b"Name").unwrap_or_else(|| format!("Layer {}", id.0));
                let visible = if base_off {
                    on.contains(&id)
                } else {
                    !off.contains(&id)
                };
                Some(Layer {
                    id,
                    name,
                    visible,
                    locked: locked.contains(&id),
                })
            })
            .collect()
    }

    /// Default visibility of every layer.
    pub fn layer_defaults(&self) -> HashMap<ObjectId, bool> {
        self.layers().into_iter().map(|l| (l.id, l.visible)).collect()
    }

    /// The layer tree as listed by the document; layers it does not list
    /// follow at the top level.
    pub fn layer_tree(&self) -> Vec<LayerNode> {
        let doc = &self.inner;
        let mut seen = HashSet::new();
        let mut nodes = self
            .default_config()
            .and_then(|d| d.get(b"Order").ok())
            .and_then(|o| deref(doc, o).as_array().ok())
            .map(|arr| parse_order(doc, arr, &mut seen, 0))
            .unwrap_or_default();
        for l in self.layers() {
            if !seen.contains(&l.id) {
                nodes.push(LayerNode::Layer {
                    id: l.id,
                    children: Vec::new(),
                });
            }
        }
        nodes
    }

    fn edit_oc<R>(&mut self, f: impl FnOnce(&mut Dictionary, &mut Dictionary) -> R) -> Result<R> {
        edit_oc_in(&mut self.inner, f)
    }

    fn check_layer(&self, id: ObjectId) -> Result<()> {
        if self.layers().iter().any(|l| l.id == id) {
            Ok(())
        } else {
            Err(Error::Invalid("layer not found".into()))
        }
    }

    /// Change the default visibility of a layer.
    pub fn set_layer_visible(&mut self, id: ObjectId, visible: bool) -> Result<()> {
        self.check_layer(id)?;
        self.set_layer_defaults(&HashMap::from([(id, visible)]))
    }

    /// Show or hide every layer by default.
    pub fn set_all_layers_visible(&mut self, visible: bool) -> Result<()> {
        let states: HashMap<ObjectId, bool> = self.layers().iter().map(|l| (l.id, visible)).collect();
        if states.is_empty() {
            return Ok(());
        }
        self.set_layer_defaults(&states)
    }

    /// Make the given visibility the default one (one undo step); layers not
    /// listed keep theirs.
    pub fn set_layer_defaults(&mut self, states: &HashMap<ObjectId, bool>) -> Result<()> {
        let mut all = self.layer_defaults();
        if all.is_empty() {
            return Err(Error::Invalid("the document has no layers".into()));
        }
        for (id, v) in states {
            if let Some(cur) = all.get_mut(id) {
                *cur = *v;
            }
        }
        let order: Vec<ObjectId> = self.layers().iter().map(|l| l.id).collect();
        self.begin_edit();
        self.edit_oc(|_, d| write_states(d, &order, &all))?;
        self.commit()
    }

    /// Lock or unlock layers (locked layers cannot be shown or hidden by readers).
    pub fn set_layers_locked(&mut self, ids: &[ObjectId], locked: bool) -> Result<()> {
        for id in ids {
            self.check_layer(*id)?;
        }
        self.begin_edit();
        self.edit_oc(|_, d| {
            let mut arr: Vec<Object> = d
                .get(b"Locked")
                .ok()
                .and_then(|o| o.as_array().ok())
                .cloned()
                .unwrap_or_default();
            arr.retain(|o| o.as_reference().map(|r| !ids.contains(&r)).unwrap_or(true));
            if locked {
                arr.extend(ids.iter().map(|id| Object::Reference(*id)));
            }
            d.set("Locked", Object::Array(arr));
        })?;
        self.commit()
    }

    /// Replace the layer tree (one undo step).
    pub fn set_layer_tree(&mut self, nodes: &[LayerNode]) -> Result<()> {
        if self.ocproperties().is_none() {
            return Err(Error::Invalid("the document has no layers".into()));
        }
        self.begin_edit();
        let order = write_order(nodes);
        self.edit_oc(|_, d| d.set("Order", Object::Array(order)))?;
        self.commit()
    }

    fn insert_node(&mut self, node: LayerNode, parent: &[usize]) -> Result<Vec<usize>> {
        let mut tree = self.layer_tree();
        let siblings =
            children_at_mut(&mut tree, parent).ok_or_else(|| Error::Invalid("layer not found".into()))?;
        siblings.push(node);
        let mut path = parent.to_vec();
        path.push(siblings.len() - 1);
        let order = write_order(&tree);
        self.edit_oc(|_, d| d.set("Order", Object::Array(order)))?;
        Ok(path)
    }

    /// Add a visible layer at the end of the entries below `parent` (the top
    /// level when empty). Returns the new layer and its path in the tree.
    pub fn add_layer(&mut self, name: &str, parent: &[usize]) -> Result<(ObjectId, Vec<usize>)> {
        let name = name.trim();
        if name.is_empty() {
            return Err(Error::Invalid("the layer needs a name".into()));
        }
        if !parent.is_empty() && node_at(&self.layer_tree(), parent).is_none() {
            return Err(Error::Invalid("layer not found".into()));
        }
        self.begin_edit();
        let id = self.inner.add_object(dictionary! {
            "Type" => "OCG",
            "Name" => lopdf::text_string(name),
        });
        let mut tree = self.layer_tree();
        let siblings = children_at_mut(&mut tree, parent).expect("checked above");
        siblings.push(LayerNode::Layer {
            id,
            children: Vec::new(),
        });
        let mut path = parent.to_vec();
        path.push(siblings.len() - 1);
        let order = write_order(&tree);
        self.edit_oc(|oc, d| {
            if let Ok(Object::Array(arr)) = oc.get_mut(b"OCGs") {
                arr.push(Object::Reference(id));
            }
            d.set("Order", Object::Array(order));
            // Visible also when the default state of unlisted layers is off.
            if d.get(b"BaseState").ok().and_then(|b| b.as_name().ok()) == Some(b"OFF") {
                match d.get_mut(b"ON") {
                    Ok(Object::Array(on)) => on.push(Object::Reference(id)),
                    _ => d.set("ON", Object::Array(vec![Object::Reference(id)])),
                }
            }
        })?;
        self.commit()?;
        Ok((id, path))
    }

    /// Add a labelled group of layers below `parent`.
    pub fn add_layer_group(&mut self, label: &str, parent: &[usize]) -> Result<Vec<usize>> {
        let label = label.trim();
        if label.is_empty() {
            return Err(Error::Invalid("the group needs a name".into()));
        }
        if self.ocproperties().is_none() {
            // A group alone still needs the layer properties.
            self.begin_edit();
            self.edit_oc(|_, _| ())?;
        } else {
            self.begin_edit();
        }
        let path = self.insert_node(
            LayerNode::Group {
                label: label.to_string(),
                children: Vec::new(),
            },
            parent,
        )?;
        self.commit()?;
        Ok(path)
    }

    /// Rename the layer or group at `path`.
    pub fn rename_layer_node(&mut self, path: &[usize], name: &str) -> Result<()> {
        let name = name.trim();
        if name.is_empty() {
            return Err(Error::Invalid("the name is empty".into()));
        }
        let mut tree = self.layer_tree();
        let node = node_at(&tree, path)
            .cloned()
            .ok_or_else(|| Error::Invalid("layer not found".into()))?;
        self.begin_edit();
        match node {
            LayerNode::Layer { id, .. } => {
                self.inner
                    .get_dictionary_mut(id)?
                    .set("Name", lopdf::text_string(name));
            }
            LayerNode::Group { .. } => {
                let (last, parent) = path.split_last().expect("non-empty path");
                if let Some(LayerNode::Group { label, .. }) =
                    children_at_mut(&mut tree, parent).and_then(|s| s.get_mut(*last))
                {
                    *label = name.to_string();
                }
                let order = write_order(&tree);
                self.edit_oc(|_, d| d.set("Order", Object::Array(order)))?;
            }
        }
        self.commit()
    }

    /// Sort the layer tree alphabetically at every level (one undo step).
    pub fn sort_layers(&mut self) -> Result<()> {
        let names: HashMap<ObjectId, String> = self.layers().into_iter().map(|l| (l.id, l.name)).collect();
        fn sort(nodes: &mut [LayerNode], names: &HashMap<ObjectId, String>) {
            let key = |n: &LayerNode| match n {
                LayerNode::Layer { id, .. } => names.get(id).cloned().unwrap_or_default().to_lowercase(),
                LayerNode::Group { label, .. } => label.to_lowercase(),
            };
            nodes.sort_by_key(key);
            for n in nodes.iter_mut() {
                sort(n.children_mut(), names);
            }
        }
        let mut tree = self.layer_tree();
        sort(&mut tree, &names);
        self.set_layer_tree(&tree)
    }

    /// Serialized document with the given layer visibility as the default
    /// state and the given annotations hidden, for displaying a view without
    /// changing the document.
    pub fn view_bytes(
        &self,
        hidden_annotations: &HashSet<ObjectId>,
        layers: &HashMap<ObjectId, bool>,
    ) -> Result<Arc<Vec<u8>>> {
        let defaults = self.layer_defaults();
        let layers_differ = layers
            .iter()
            .any(|(id, v)| defaults.get(id).is_some_and(|d| d != v));
        if !layers_differ {
            return self.bytes_hiding(hidden_annotations);
        }
        let mut all = defaults;
        for (id, v) in layers {
            if let Some(cur) = all.get_mut(id) {
                *cur = *v;
            }
        }
        let order: Vec<ObjectId> = self.layers().iter().map(|l| l.id).collect();
        let mut copy = self.inner.clone();
        edit_oc_in(&mut copy, |_, d| write_states(d, &order, &all))?;
        crate::comment_review::hide_annotations_in(&mut copy, hidden_annotations);
        let mut out = Vec::new();
        copy.save_to(&mut out)?;
        Ok(Arc::new(out))
    }

    /// Layers referenced by a page: its marked content properties, its
    /// XObjects (and the forms they draw) and its annotations.
    pub fn layers_on_page(&self, page: usize) -> HashSet<ObjectId> {
        let doc = &self.inner;
        let mut out = HashSet::new();
        let Ok(pid) = self.page_id(page) else {
            return out;
        };
        let add_oc = |o: &Object, out: &mut HashSet<ObjectId>| {
            if let Ok(id) = o.as_reference() {
                if is_ocg(doc, id) {
                    out.insert(id);
                    return;
                }
            }
            if let Ok(d) = deref(doc, o).as_dict() {
                if d.has_type(b"OCMD") {
                    out.extend(ocmd_members(doc, d));
                }
            }
        };
        let mut stack = vec![page_resources(doc, pid)];
        let mut seen_forms = HashSet::new();
        while let Some(res) = stack.pop() {
            for (_, v) in sub_dict(doc, &res, b"Properties").iter() {
                add_oc(v, &mut out);
            }
            for (_, v) in sub_dict(doc, &res, b"XObject").iter() {
                let Ok(id) = v.as_reference() else { continue };
                let Ok(s) = doc.get_object(id).and_then(Object::as_stream) else {
                    continue;
                };
                if let Ok(oc) = s.dict.get(b"OC") {
                    add_oc(oc, &mut out);
                }
                if seen_forms.insert(id) {
                    if let Ok(r) = s.dict.get(b"Resources") {
                        if let Ok(d) = deref(doc, r).as_dict() {
                            stack.push(d.clone());
                        }
                    }
                }
            }
        }
        if let Ok(pd) = doc.get_dictionary(pid) {
            for (_, d) in crate::annots::annot_refs(doc, pd) {
                if let Ok(oc) = d.get(b"OC") {
                    add_oc(oc, &mut out);
                }
            }
        }
        out
    }

    /// Pages that use each layer.
    pub fn layer_pages(&self) -> HashMap<ObjectId, Vec<usize>> {
        let mut out: HashMap<ObjectId, Vec<usize>> = HashMap::new();
        for p in 0..self.page_count() {
            for id in self.layers_on_page(p) {
                out.entry(id).or_default().push(p);
            }
        }
        out
    }

    /// Annotations that belong to the given layers: page, index and subtype.
    pub fn layer_annotations(&self, ids: &HashSet<ObjectId>) -> Vec<(usize, usize, String)> {
        let doc = &self.inner;
        let mut out = Vec::new();
        for p in 0..self.page_count() {
            let Ok(pid) = self.page_id(p) else { continue };
            let Ok(pd) = doc.get_dictionary(pid) else { continue };
            for (i, (_, d)) in crate::annots::annot_refs(doc, pd).into_iter().enumerate() {
                let Ok(oc) = d.get(b"OC") else { continue };
                let hit = match oc.as_reference() {
                    Ok(r) if ids.contains(&r) => true,
                    _ => deref(doc, oc)
                        .as_dict()
                        .map(|m| ocmd_members(doc, m).iter().any(|x| ids.contains(x)))
                        .unwrap_or(false),
                };
                if hit {
                    let subtype = d
                        .get(b"Subtype")
                        .ok()
                        .and_then(|s| s.as_name().ok())
                        .map(|n| String::from_utf8_lossy(n).to_string())
                        .unwrap_or_default();
                    out.push((p, i, subtype));
                }
            }
        }
        out
    }

    /// Rewrite page content, forms and annotations following a plan.
    fn apply_plan(&mut self, plan: &Plan, drop_unlayered: bool) -> Result<()> {
        let page_ids = self.page_ids();
        let mut writes: Vec<(ObjectId, Vec<u8>)> = Vec::new();
        let mut strip_oc: HashSet<ObjectId> = HashSet::new();
        let mut forms_done: HashSet<ObjectId> = HashSet::new();
        let mut removals: Vec<(ObjectId, HashSet<ObjectId>)> = Vec::new();
        {
            let doc = &self.inner;
            for &pid in &page_ids {
                let res = page_resources(doc, pid);
                let mut rw = Rewriter::new();
                let mut forms: Vec<(ObjectId, bool)> = Vec::new();
                for sid in doc.get_page_contents(pid) {
                    let Ok(s) = doc.get_object(sid).and_then(Object::as_stream) else {
                        continue;
                    };
                    let Some(data) = stream_data(s) else { continue };
                    let mut dec = LayerDecide::new(doc, plan, &res, drop_unlayered);
                    if let Some(out) = rw.rewrite(&data, &mut dec) {
                        writes.push((sid, out));
                    }
                    forms.append(&mut dec.forms);
                    strip_oc.extend(dec.strip_oc);
                }
                while let Some((fid, kept)) = forms.pop() {
                    if !forms_done.insert(fid) {
                        continue;
                    }
                    let Ok(s) = doc.get_object(fid).and_then(Object::as_stream) else {
                        continue;
                    };
                    let fres = s
                        .dict
                        .get(b"Resources")
                        .ok()
                        .and_then(|r| deref(doc, r).as_dict().ok().cloned())
                        .unwrap_or_else(|| res.clone());
                    let Some(data) = stream_data(s) else { continue };
                    let mut frw = Rewriter::inside_kept_layer(kept);
                    let mut dec = LayerDecide::new(doc, plan, &fres, drop_unlayered);
                    if let Some(out) = frw.rewrite(&data, &mut dec) {
                        writes.push((fid, out));
                    }
                    forms.append(&mut dec.forms);
                    strip_oc.extend(dec.strip_oc);
                }
                let mut remove = HashSet::new();
                if let Ok(pd) = doc.get_dictionary(pid) {
                    for (aid, d) in crate::annots::annot_refs(doc, pd) {
                        let (Some(aid), Ok(oc)) = (aid, d.get(b"OC")) else {
                            continue;
                        };
                        match plan.treat_oc(doc, oc) {
                            Treat::Remove => {
                                remove.insert(aid);
                            }
                            Treat::Keep => {
                                strip_oc.insert(aid);
                            }
                            Treat::Leave => {}
                        }
                    }
                }
                if !remove.is_empty() {
                    removals.push((pid, remove));
                }
            }
        }
        for (id, data) in writes {
            if let Ok(Object::Stream(s)) = self.inner.get_object_mut(id) {
                s.set_plain_content(data);
            }
        }
        for id in strip_oc {
            match self.inner.get_object_mut(id) {
                Ok(Object::Stream(s)) => {
                    s.dict.remove(b"OC");
                }
                Ok(Object::Dictionary(d)) => {
                    d.remove(b"OC");
                }
                _ => {}
            }
        }
        for (pid, set) in removals {
            self.remove_annotations(pid, &set);
        }
        Ok(())
    }

    /// Remove layers from the optional content properties (the content is not touched).
    fn drop_layers(&mut self, gone: &HashSet<ObjectId>) -> Result<()> {
        let remaining = self.layers().iter().filter(|l| !gone.contains(&l.id)).count();
        if remaining == 0 {
            self.inner.catalog_mut()?.remove(b"OCProperties");
            return Ok(());
        }
        let configs: Vec<ObjectId> = self
            .ocproperties()
            .and_then(|oc| oc.get(b"Configs").ok())
            .and_then(|c| deref(&self.inner, c).as_array().ok())
            .map(|a| a.iter().filter_map(|o| o.as_reference().ok()).collect())
            .unwrap_or_default();
        self.edit_oc(|oc, d| {
            retain_refs(oc, b"OCGs", gone);
            prune_config(d, gone);
            if let Ok(Object::Array(cfgs)) = oc.get_mut(b"Configs") {
                for c in cfgs.iter_mut() {
                    if let Object::Dictionary(cd) = c {
                        prune_config(cd, gone);
                    }
                }
            }
        })?;
        for id in configs {
            if let Ok(d) = self.inner.get_dictionary_mut(id) {
                prune_config(d, gone);
            }
        }
        Ok(())
    }

    /// Remove the layers that are hidden by default with their content
    /// (pages, forms, annotations), without an undo step of its own.
    /// Returns how many layers went.
    pub(crate) fn remove_hidden_layers_in(&mut self) -> Result<usize> {
        let hidden: HashSet<ObjectId> = self
            .layer_defaults()
            .into_iter()
            .filter(|(_, visible)| !visible)
            .map(|(id, _)| id)
            .collect();
        if hidden.is_empty() {
            return Ok(0);
        }
        let plan = Plan {
            affected: Some(hidden.clone()),
            keep: HashSet::new(),
        };
        self.apply_plan(&plan, false)?;
        self.drop_layers(&hidden)?;
        Ok(hidden.len())
    }

    /// Make layer content permanent: content of visible layers stays, content
    /// of hidden layers is removed, and the layers go away. `ids` limits the
    /// layers (all when `None`); `visible` gives their visibility (the
    /// default one for layers it does not list). Returns the number of
    /// layers flattened.
    pub fn flatten_layers(
        &mut self,
        ids: Option<&[ObjectId]>,
        visible: &HashMap<ObjectId, bool>,
    ) -> Result<usize> {
        let defaults = self.layer_defaults();
        if defaults.is_empty() {
            return Err(Error::Invalid("the document has no layers".into()));
        }
        let affected: HashSet<ObjectId> = match ids {
            Some(ids) => ids
                .iter()
                .copied()
                .filter(|id| defaults.contains_key(id))
                .collect(),
            None => defaults.keys().copied().collect(),
        };
        if affected.is_empty() {
            return Ok(0);
        }
        let keep = affected
            .iter()
            .copied()
            .filter(|id| visible.get(id).or(defaults.get(id)).copied().unwrap_or(true))
            .collect();
        let plan = Plan {
            affected: ids.map(|_| affected.clone()),
            keep,
        };
        self.begin_edit();
        self.apply_plan(&plan, false)?;
        self.drop_layers(&affected)?;
        self.commit()?;
        Ok(affected.len())
    }

    /// A new document with only some layer content: the given layers (their
    /// content only, without the content that is not on a layer), or the
    /// visible content when `only` is `None`. Protected like this document
    /// when it is.
    pub fn extract_layers(
        &self,
        only: Option<&HashSet<ObjectId>>,
        visible: &HashMap<ObjectId, bool>,
    ) -> Result<Document> {
        let defaults = self.layer_defaults();
        if defaults.is_empty() {
            return Err(Error::Invalid("the document has no layers".into()));
        }
        let mut copy = Document::from_bytes(self.bytes().to_vec(), None, None)?;
        let keep: HashSet<ObjectId> = match only {
            Some(set) => set.clone(),
            None => defaults
                .iter()
                .filter(|(id, d)| visible.get(id).copied().unwrap_or(**d))
                .map(|(id, _)| *id)
                .collect(),
        };
        let plan = Plan { affected: None, keep };
        copy.apply_plan(&plan, only.is_some())?;
        let all: HashSet<ObjectId> = defaults.keys().copied().collect();
        copy.drop_layers(&all)?;
        copy.commit()?;
        let bytes = copy.bytes().to_vec();
        let mut out = Document::from_bytes(bytes, None, None)?;
        out.protect_like(self);
        Ok(out)
    }

    /// Merge layers into `target`: their content, annotations and XObjects
    /// belong to it afterwards and they disappear from the layer list. The
    /// children of merged layers move up in the tree.
    pub fn merge_layers(&mut self, sources: &[ObjectId], target: ObjectId) -> Result<usize> {
        self.check_layer(target)?;
        let gone: HashSet<ObjectId> = sources.iter().copied().filter(|s| *s != target).collect();
        if gone.is_empty() {
            return Ok(0);
        }
        for id in &gone {
            self.check_layer(*id)?;
        }
        // Objects of the optional content properties are fixed separately.
        let mut skip: HashSet<ObjectId> = HashSet::new();
        let root = self
            .inner
            .trailer
            .get(b"Root")
            .and_then(Object::as_reference)
            .ok();
        if let Some(r) = root {
            skip.insert(r);
        }
        if let Ok(Object::Reference(r)) = self.inner.catalog()?.get(b"OCProperties") {
            skip.insert(*r);
        }
        if let Some(oc) = self.ocproperties() {
            for key in [b"D".as_slice(), b"OCGs", b"Configs"] {
                if let Ok(Object::Reference(r)) = oc.get(key) {
                    skip.insert(*r);
                }
            }
            if let Ok(cfgs) = oc.get(b"Configs") {
                if let Ok(arr) = deref(&self.inner, cfgs).as_array() {
                    skip.extend(arr.iter().filter_map(|o| o.as_reference().ok()));
                }
            }
        }
        fn replace(o: &mut Object, gone: &HashSet<ObjectId>, target: ObjectId) {
            match o {
                Object::Reference(r) if gone.contains(r) => *r = target,
                Object::Array(a) => a.iter_mut().for_each(|x| replace(x, gone, target)),
                Object::Dictionary(d) => d.iter_mut().for_each(|(_, v)| replace(v, gone, target)),
                Object::Stream(s) => s.dict.iter_mut().for_each(|(_, v)| replace(v, gone, target)),
                _ => {}
            }
        }
        self.begin_edit();
        for (id, obj) in self.inner.objects.iter_mut() {
            if !skip.contains(id) {
                replace(obj, &gone, target);
            }
        }
        // The catalog itself, except its optional content properties.
        if let Some(r) = root {
            if let Ok(cat) = self.inner.get_dictionary_mut(r) {
                for (k, v) in cat.iter_mut() {
                    if k != b"OCProperties" {
                        replace(v, &gone, target);
                    }
                }
            }
        }
        self.drop_layers(&gone)?;
        self.commit()?;
        Ok(gone.len())
    }

    /// Draw the pages of `other` over (or behind) the given pages as a new
    /// layer named `name`. Returns the layer.
    pub fn import_as_layer(
        &mut self,
        other: &Document,
        pages: &[usize],
        behind: bool,
        name: &str,
    ) -> Result<ObjectId> {
        self.check_import_from(other)?;
        let name = name.trim();
        if name.is_empty() {
            return Err(Error::Invalid("the layer needs a name".into()));
        }
        let (id, _) = self.add_layer(name, &[])?;
        self.overlay_document_on_layer(other, pages, behind, Some(id))?;
        // One undo step for the layer and its content.
        self.merge_last_undo();
        Ok(id)
    }
}
