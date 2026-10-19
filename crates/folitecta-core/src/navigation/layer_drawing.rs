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
use crate::content_edit::name_token;
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
    pub(crate) fn oc_property(&mut self, page_id: ObjectId, oc: Object) -> Result<Vec<u8>> {
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
            return Ok(name.clone());
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
        Ok(name.into_bytes())
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

    /// The layer states of `view` that differ from the default ones, in order.
    pub(crate) fn view_changes(&self, view: &HashMap<ObjectId, bool>) -> Vec<(ObjectId, bool)> {
        if view.is_empty() {
            return Vec::new();
        }
        let defaults = self.layer_defaults();
        let mut out: Vec<(ObjectId, bool)> = view
            .iter()
            .filter(|(id, v)| defaults.get(id).is_some_and(|d| d != *v))
            .map(|(id, v)| (*id, *v))
            .collect();
        out.sort_unstable();
        out
    }

    /// Calls `f` with the index, object and dictionary of every annotation
    /// of a page that has an `/OC` entry (no copies).
    fn with_layered_annots(
        &self,
        page_id: ObjectId,
        mut f: impl FnMut(usize, Option<ObjectId>, &Dictionary, &Object),
    ) {
        let doc = &self.inner;
        let Some(arr) = doc
            .get_dictionary(page_id)
            .ok()
            .and_then(|pd| pd.get(b"Annots").ok())
            .and_then(|a| deref(doc, a).as_array().ok())
        else {
            return;
        };
        for (i, a) in arr.iter().enumerate() {
            let (id, d) = match a {
                Object::Reference(r) => match doc.get_dictionary(*r) {
                    Ok(d) => (Some(*r), d),
                    Err(_) => continue,
                },
                Object::Dictionary(d) => (None, d),
                _ => continue,
            };
            if let Ok(oc) = d.get(b"OC") {
                f(i, id, d, oc);
            }
        }
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
            if let Ok(pid) = self.page_id(page) {
                self.with_layered_annots(pid, |i, _, _, oc| {
                    if !oc_visible(&self.inner, oc, &states) {
                        out.insert(i);
                    }
                });
            }
        }
        out
    }

    /// Annotations of the document that hidden layers hide (with the default
    /// visibility changed by `view`), with the pop-up notes of those comments.
    pub fn annotations_in_hidden_layers(&self, view: &HashMap<ObjectId, bool>) -> HashSet<ObjectId> {
        if !self.ocproperties_present() {
            return HashSet::new();
        }
        self.hidden_by_layers(&self.view_changes(view)).as_ref().clone()
    }

    /// [`Document::annotations_in_hidden_layers`] for the layer states that
    /// differ from the default ones, kept for the revision.
    pub(crate) fn hidden_by_layers(&self, changes: &[(ObjectId, bool)]) -> Arc<HashSet<ObjectId>> {
        if !self.ocproperties_present() {
            return Arc::default();
        }
        let editing = self.editing();
        if !editing {
            let mut cache = self.layer_cache.lock().unwrap_or_else(|e| e.into_inner());
            cache.for_revision(self.revision());
            if let Some((_, hit)) = cache.hidden.iter().find(|(k, _)| k == changes) {
                return hit.clone();
            }
        }
        let mut states = self.layer_defaults();
        states.extend(changes.iter().copied());
        let mut out = HashSet::new();
        for pid in self.page_ids() {
            self.with_layered_annots(pid, |_, id, d, oc| {
                if let (Some(id), false) = (id, oc_visible(&self.inner, oc, &states)) {
                    out.insert(id);
                    if let Some(p) = d.get(b"Popup").ok().and_then(|p| p.as_reference().ok()) {
                        out.insert(p);
                    }
                }
            });
        }
        let out = Arc::new(out);
        if !editing {
            let mut cache = self.layer_cache.lock().unwrap_or_else(|e| e.into_inner());
            cache.for_revision(self.revision());
            cache.hidden.push((changes.to_vec(), out.clone()));
            if cache.hidden.len() > CACHE_SLOTS {
                cache.hidden.remove(0);
            }
        }
        out
    }

    /// The bytes of a view kept for this revision.
    pub(crate) fn cached_view(&self, key: &ViewKey) -> Option<Arc<Vec<u8>>> {
        let mut cache = self.layer_cache.lock().unwrap_or_else(|e| e.into_inner());
        cache.for_revision(self.revision());
        cache.bytes.iter().find(|(k, _)| k == key).map(|(_, b)| b.clone())
    }

    /// Keep the bytes of a view for this revision.
    pub(crate) fn keep_view(&self, key: ViewKey, bytes: Arc<Vec<u8>>) {
        let mut cache = self.layer_cache.lock().unwrap_or_else(|e| e.into_inner());
        cache.for_revision(self.revision());
        cache.bytes.push((key, bytes));
        if cache.bytes.len() > CACHE_SLOTS {
            cache.bytes.remove(0);
        }
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

/// A view: the layer states that differ from the default ones and the
/// annotations hidden.
pub(crate) type ViewKey = (Vec<(ObjectId, bool)>, Vec<ObjectId>);

/// What hidden layers hide for the layer states that differ from the
/// default ones.
type HiddenEntry = (Vec<(ObjectId, bool)>, Arc<HashSet<ObjectId>>);

/// Views kept per revision (a few at a time).
const CACHE_SLOTS: usize = 4;

/// What hidden layers hide and the bytes of views, for one revision of a
/// document (never during an edit, whose objects differ from its bytes).
#[derive(Default)]
pub(crate) struct LayerCache {
    revision: u64,
    hidden: Vec<HiddenEntry>,
    bytes: Vec<(ViewKey, Arc<Vec<u8>>)>,
}

impl LayerCache {
    /// Forget what was kept for another revision.
    fn for_revision(&mut self, revision: u64) {
        if self.revision != revision {
            self.revision = revision;
            self.hidden.clear();
            self.bytes.clear();
        }
    }
}

/// Where the last cross-reference section of PDF bytes starts (the number
/// after the last `startxref`).
pub(crate) fn last_startxref(bytes: &[u8]) -> Option<usize> {
    let tail = &bytes[bytes.len().saturating_sub(1024)..];
    let at = tail.windows(9).rposition(|w| w == b"startxref")?;
    let digits: String = tail[at + 9..]
        .iter()
        .skip_while(|b| b.is_ascii_whitespace())
        .take_while(|b| b.is_ascii_digit())
        .map(|b| *b as char)
        .collect();
    digits.parse().ok().filter(|n| *n > 0 && *n < bytes.len())
}

/// `content` inside the marked-content sequence of the property `name`.
pub(crate) fn wrap_marked(name: &[u8], content: Vec<u8>) -> Vec<u8> {
    let mut out = format!("/OC {} BDC\n", name_token(name)).into_bytes();
    let newline = content.last().is_some_and(|b| *b == b'\n');
    out.extend(content);
    if !newline {
        out.push(b'\n');
    }
    out.extend_from_slice(b"EMC\n");
    out
}
