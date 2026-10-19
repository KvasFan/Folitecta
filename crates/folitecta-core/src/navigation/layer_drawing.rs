//! Drawing on a layer: the layer new comments and page content go to, the
//! layer of existing comments, and what hidden layers hide.
//!
//! A comment belongs to a layer through its `/OC` entry (ISO 32000-1,
//! 12.5.2, table 164); page content through a marked-content sequence
//! `/OC /name BDC … EMC` whose name is in the `/Properties` of the page
//! resources (8.11.3.2). The renderer follows the optional content of the
//! page content and of forms, not the `/OC` of annotations: the bytes it is
//! given hide the comments of hidden layers (see [`Document::view_bytes`]).

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use lopdf::{Dictionary, Object, ObjectId};

use crate::annots::annot_refs;
use crate::document::Document;
use crate::error::{Error, Result};
use crate::layers::{is_ocg, ocmd_members};
use crate::util::{self, deref};

/// Kinds of annotation never put on the drawing layer: marks for redaction
/// (what they will remove must stay in sight), pop-up notes (they follow
/// their comment), form fields and links.
const NOT_DRAWN: &[&[u8]] = &[b"Redact", b"Popup", b"Widget", b"Link"];

fn subtype_of(d: &Dictionary) -> &[u8] {
    d.get(b"Subtype")
        .ok()
        .and_then(|s| s.as_name().ok())
        .unwrap_or(b"")
}

/// Whether an optional content entry (an `/OC` value: a layer or a
/// membership dictionary) is visible with the given layer states (layers not
/// listed are visible). Visibility expressions (`/VE`) are not evaluated:
/// the `/OCGs` and `/P` of the dictionary decide.
pub(crate) fn oc_visible(doc: &lopdf::Document, oc: &Object, states: &HashMap<ObjectId, bool>) -> bool {
    let on = |id: &ObjectId| states.get(id).copied().unwrap_or(true);
    if let Ok(id) = oc.as_reference() {
        if is_ocg(doc, id) {
            return on(&id);
        }
    }
    let Ok(d) = deref(doc, oc).as_dict() else {
        return true;
    };
    if !d.has_type(b"OCMD") {
        return true;
    }
    let members = ocmd_members(doc, d);
    if members.is_empty() {
        return true;
    }
    match d.get(b"P").ok().and_then(|p| p.as_name().ok()) {
        Some(b"AllOn") => members.iter().all(on),
        Some(b"AnyOff") => members.iter().any(|m| !on(m)),
        Some(b"AllOff") => members.iter().all(|m| !on(m)),
        _ => members.iter().any(on),
    }
}

/// The layers of an optional content entry: the layer itself, or the
/// members of a membership dictionary.
fn oc_layers(doc: &lopdf::Document, oc: &Object) -> Vec<ObjectId> {
    if let Ok(id) = oc.as_reference() {
        if is_ocg(doc, id) {
            return vec![id];
        }
    }
    deref(doc, oc)
        .as_dict()
        .ok()
        .filter(|d| d.has_type(b"OCMD"))
        .map(|d| ocmd_members(doc, d))
        .unwrap_or_default()
}

/// The single layer of an optional content entry that names one layer
/// directly (not through a membership dictionary).
pub(crate) fn single_layer(doc: &lopdf::Document, oc: &Object) -> Option<ObjectId> {
    oc.as_reference().ok().filter(|id| is_ocg(doc, *id))
}

impl Document {
    // ---- The drawing layer -------------------------------------------------

    /// The layer new comments and page content go to, when one is chosen and
    /// it is still a layer of the document.
    pub fn drawing_layer(&self) -> Option<ObjectId> {
        self.drawing_layer.filter(|id| self.check_layer(*id).is_ok())
    }

    /// Choose the layer new comments and page content go to (`None`: no
    /// layer). This is a choice of the view: the document does not change
    /// and it is not saved. A locked layer can be chosen too (a lock only
    /// keeps readers from showing or hiding the layer).
    pub fn set_drawing_layer(&mut self, layer: Option<ObjectId>) -> Result<()> {
        if let Some(id) = layer {
            self.check_layer(id)?;
        }
        self.drawing_layer = layer;
        Ok(())
    }

    /// Forget the drawing layer when it is no longer a layer of the
    /// document (merged, flattened, its creation undone).
    pub(crate) fn forget_lost_drawing_layer(&mut self) {
        if self.drawing_layer.is_some() && self.drawing_layer().is_none() {
            self.drawing_layer = None;
        }
    }

    /// The layer something was put on since the last call, for the view to
    /// show that layer when it hides it.
    pub fn take_layer_drawn(&mut self) -> Option<ObjectId> {
        self.layer_drawn.take()
    }

    /// Put a new comment on the drawing layer, when there is one and the
    /// comment is of a kind that goes on layers.
    pub(crate) fn put_on_drawing_layer(&mut self, annot: &mut Dictionary) {
        let Some(layer) = self.drawing_layer() else {
            return;
        };
        if NOT_DRAWN.contains(&subtype_of(annot)) {
            return;
        }
        annot.set("OC", Object::Reference(layer));
        self.layer_drawn = Some(layer);
    }

    /// Add a comment the user drew or wrote to a page: on the drawing layer
    /// when there is one (part of the edit under way).
    pub(crate) fn push_drawn_annotation(
        &mut self,
        page_id: ObjectId,
        mut annot: Dictionary,
    ) -> Result<ObjectId> {
        self.put_on_drawing_layer(&mut annot);
        self.push_annotation(page_id, annot)
    }

    /// Page content drawn by an edit tool, on the drawing layer when there is
    /// one: wrapped in `/OC /name BDC … EMC`, the layer named in the
    /// `/Properties` of the page (part of the edit under way).
    pub(crate) fn drawn_content(&mut self, page_id: ObjectId, content: Vec<u8>) -> Result<Vec<u8>> {
        let Some(layer) = self.drawing_layer() else {
            return Ok(content);
        };
        let name = self.oc_property(page_id, Object::Reference(layer))?;
        self.layer_drawn = Some(layer);
        Ok(wrap_marked(&name, content))
    }

    /// The name of an optional content entry in the `/Properties` of the
    /// resources of a page, added when the page does not name it yet. A
    /// direct membership dictionary becomes an object of its own first, as
    /// renderers look marked-content properties up by reference.
    pub(crate) fn oc_property(&mut self, page_id: ObjectId, oc: Object) -> Result<String> {
        let oc = match oc {
            Object::Dictionary(d) => Object::Reference(self.inner.add_object(d)),
            other => other,
        };
        self.inherit_attributes(&[page_id]);
        let props: Dictionary = self
            .with_page_resources(page_id, |res| res.get(b"Properties").ok().cloned())?
            .map(|p| deref(&self.inner, &p).as_dict().cloned().unwrap_or_default())
            .unwrap_or_default();
        if let Some((name, _)) = props.iter().find(|(_, v)| **v == oc) {
            return Ok(String::from_utf8_lossy(name).to_string());
        }
        let base = match oc.as_reference() {
            Ok(id) => format!("FoliOC{}", id.0),
            Err(_) => "FoliOC".to_string(),
        };
        let mut name = base.clone();
        let mut n = 1;
        while props.has(name.as_bytes()) {
            n += 1;
            name = format!("{base}_{n}");
        }
        self.add_page_resource(page_id, "Properties", &name, oc)?;
        Ok(name)
    }

    // ---- The layer of comments ---------------------------------------------

    /// The layers a comment belongs to: its layer, or the layers of its
    /// membership dictionary (empty when it is on no layer).
    pub fn annotation_layers(&self, page: usize, index: usize) -> Vec<ObjectId> {
        let Ok((_, Some(aid))) = self.annotation_ref(page, index) else {
            return Vec::new();
        };
        self.inner
            .get_dictionary(aid)
            .ok()
            .and_then(|d| d.get(b"OC").ok())
            .map(|oc| oc_layers(&self.inner, oc))
            .unwrap_or_default()
    }

    /// The layer of a comment when it names one layer directly.
    pub fn annotation_layer(&self, page: usize, index: usize) -> Option<ObjectId> {
        let (_, aid) = self.annotation_ref(page, index).ok()?;
        let d = self.inner.get_dictionary(aid?).ok()?;
        single_layer(&self.inner, d.get(b"OC").ok()?)
    }

    /// The comments `indices` of a page with what follows them on a layer:
    /// their pop-up notes, replies and review states.
    fn layer_targets(&self, page: usize, indices: &[usize]) -> Result<(Vec<ObjectId>, Vec<ObjectId>)> {
        if indices.is_empty() {
            return Err(Error::Invalid("select a comment first".into()));
        }
        let mut main = Vec::new();
        for &i in indices {
            let (_, aid) = self.annotation_ref(page, i)?;
            let aid = aid.ok_or_else(|| Error::Invalid(format!("annotation {i} cannot be edited")))?;
            let d = self.inner.get_dictionary(aid)?;
            if NOT_DRAWN.contains(&subtype_of(d)) {
                return Err(Error::Invalid("only comments can be put on a layer".into()));
            }
            if !main.contains(&aid) {
                main.push(aid);
            }
        }
        // Pop-up notes, then replies and states (and theirs), on any page.
        let mut follow: Vec<ObjectId> = Vec::new();
        let mut parents: Vec<ObjectId> = main.clone();
        let mut seen: HashSet<ObjectId> = main.iter().copied().collect();
        let all: Vec<(ObjectId, Dictionary)> = self
            .page_ids()
            .into_iter()
            .filter_map(|pid| self.inner.get_dictionary(pid).ok())
            .flat_map(|pd| annot_refs(&self.inner, pd))
            .filter_map(|(id, d)| id.map(|id| (id, d)))
            .collect();
        while let Some(p) = parents.pop() {
            if let Some(popup) = self
                .inner
                .get_dictionary(p)
                .ok()
                .and_then(|d| d.get(b"Popup").ok())
                .and_then(|o| o.as_reference().ok())
            {
                if seen.insert(popup) {
                    follow.push(popup);
                }
            }
            for (id, d) in &all {
                let irt = d.get(b"IRT").ok().and_then(|o| o.as_reference().ok());
                if irt == Some(p) && seen.insert(*id) {
                    follow.push(*id);
                    parents.push(*id);
                }
            }
        }
        Ok((main, follow))
    }

    fn apply_layer(&mut self, main: &[ObjectId], follow: &[ObjectId], layer: Option<ObjectId>) -> Result<()> {
        let now = util::pdf_date_now();
        for id in main.iter().chain(follow) {
            let d = self.inner.get_dictionary_mut(*id)?;
            match layer {
                Some(l) => d.set("OC", Object::Reference(l)),
                None => {
                    d.remove(b"OC");
                }
            }
            if main.contains(id) {
                d.set("M", Object::string_literal(now.clone()));
            }
        }
        Ok(())
    }

    /// Put comments of a page on a layer, or on none (one undo step). Their
    /// pop-up notes, replies and review states follow them. Returns how many
    /// comments changed.
    pub fn set_annotations_layer(
        &mut self,
        page: usize,
        indices: &[usize],
        layer: Option<ObjectId>,
    ) -> Result<usize> {
        if let Some(l) = layer {
            self.check_layer(l)?;
        }
        let (main, follow) = self.layer_targets(page, indices)?;
        self.edit_step(|doc| doc.apply_layer(&main, &follow, layer))?;
        Ok(main.len())
    }

    /// Put comments of a page on a new layer named `name`, added at the top
    /// of the layer tree (one undo step). Returns the new layer.
    pub fn move_annotations_to_new_layer(
        &mut self,
        page: usize,
        indices: &[usize],
        name: &str,
    ) -> Result<ObjectId> {
        let name = name.trim();
        if name.is_empty() {
            return Err(Error::Invalid("the layer needs a name".into()));
        }
        let (main, follow) = self.layer_targets(page, indices)?;
        self.edit_step(|doc| {
            let (id, _) = doc.add_layer_in_edit(name, &[])?;
            doc.apply_layer(&main, &follow, Some(id))?;
            Ok(id)
        })
    }

    // ---- What hidden layers hide -------------------------------------------

    /// The visibility of every layer: the default one, changed by `view`.
    pub fn layer_states(&self, view: &HashMap<ObjectId, bool>) -> HashMap<ObjectId, bool> {
        let mut states = self.layer_defaults();
        for (id, v) in view {
            if let Some(cur) = states.get_mut(id) {
                *cur = *v;
            }
        }
        states
    }

    /// Indices of the annotations of a page that hidden layers hide, with the
    /// default visibility changed by `view`.
    pub fn page_annotations_in_hidden_layers(
        &self,
        page: usize,
        view: &HashMap<ObjectId, bool>,
    ) -> HashSet<usize> {
        let mut out = HashSet::new();
        if self.ocproperties_present() {
            let states = self.layer_states(view);
            let pd = self
                .page_id(page)
                .ok()
                .and_then(|p| self.inner.get_dictionary(p).ok());
            if let Some(pd) = pd {
                for (i, (_, d)) in annot_refs(&self.inner, pd).into_iter().enumerate() {
                    if d.get(b"OC").is_ok_and(|oc| !oc_visible(&self.inner, oc, &states)) {
                        out.insert(i);
                    }
                }
            }
        }
        out
    }

    /// Annotations of the document that hidden layers hide (with the default
    /// visibility changed by `view`), with the pop-up notes of those comments.
    pub fn annotations_in_hidden_layers(&self, view: &HashMap<ObjectId, bool>) -> HashSet<ObjectId> {
        let mut out = HashSet::new();
        if !self.ocproperties_present() {
            return out;
        }
        let states = self.layer_states(view);
        for pid in self.page_ids() {
            let Ok(pd) = self.inner.get_dictionary(pid) else {
                continue;
            };
            for (id, d) in annot_refs(&self.inner, pd) {
                let (Some(id), Ok(oc)) = (id, d.get(b"OC")) else {
                    continue;
                };
                if !oc_visible(&self.inner, oc, &states) {
                    out.insert(id);
                    if let Some(p) = d.get(b"Popup").ok().and_then(|p| p.as_reference().ok()) {
                        out.insert(p);
                    }
                }
            }
        }
        out
    }

    fn ocproperties_present(&self) -> bool {
        self.inner.catalog().is_ok_and(|c| c.has(b"OCProperties"))
    }

    /// The bytes to draw the document from with its default layer
    /// visibility: the comments of hidden layers hidden (for pictures of the
    /// pages, exports and the operations that draw comments).
    pub fn render_bytes(&self) -> Result<Arc<Vec<u8>>> {
        self.view_bytes(&HashSet::new(), &HashMap::new())
    }
}

/// `content` inside the marked-content sequence of the property `name`.
pub(crate) fn wrap_marked(name: &str, content: Vec<u8>) -> Vec<u8> {
    let mut out = format!("/OC /{name} BDC\n").into_bytes();
    let newline = content.last().is_some_and(|b| *b == b'\n');
    out.extend(content);
    if !newline {
        out.push(b'\n');
    }
    out.extend_from_slice(b"EMC\n");
    out
}
