//! The print file: the planned sheets written as a vector PDF.
//!
//! Every page used is drawn once as two form XObjects: its content (the page
//! content streams with its resources, so an unbalanced graphics state stays
//! inside) and the page as printed (the content, then the appearance of each
//! printable annotation, clipped to the crop box). The sheets place these
//! forms with the matrices of the plan. The objects a page uses are copied
//! once, so fonts and pictures shared by pages stay shared. Optional content
//! is fixed to its print state in the default configuration of the file,
//! which every renderer reads the same way. The document itself never changes.

use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::ops::Range;
use std::sync::Arc;

use lopdf::{Dictionary, Object, ObjectId, Stream};

use super::plan::{Mark, PrintPlan};
use super::{Content, PageGeom, PrintPermission, permission_of};
use crate::document::Document;
use crate::error::{Error, Result};
use crate::util::{deref, num, rect_of};

/// What a print file is made from, taken from an open document in an
/// instant (the bytes are shared); the work is done by [`PrintSource::load`]
/// away from the interface.
#[derive(Clone)]
pub struct PrintSeed {
    /// The current state of the document, unencrypted.
    pub bytes: Arc<Vec<u8>>,
    /// The encryption of the file the document came from.
    pub protection: Option<lopdf::EncryptionState>,
    /// The `/ID` of the document (the keys of its encryption depend on it).
    pub id: Option<Object>,
    pub title: String,
    /// Annotations that do not print (hidden by the comment filter of the view).
    pub hidden: HashSet<ObjectId>,
    /// The layer visibility of the view (the document's own for layers left out).
    pub layers: HashMap<ObjectId, bool>,
}

impl std::fmt::Debug for PrintSeed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PrintSeed")
            .field("bytes", &self.bytes.len())
            .field("protected", &self.protection.is_some())
            .field("title", &self.title)
            .field("hidden", &self.hidden.len())
            .field("layers", &self.layers.len())
            .finish()
    }
}

impl Document {
    /// What a print file of the current state is made from.
    pub fn print_seed(&self) -> PrintSeed {
        let title = self.title();
        PrintSeed {
            bytes: self.bytes(),
            protection: self.protection().cloned(),
            id: self.inner.trailer.get(b"ID").ok().cloned(),
            title: if title.is_empty() {
                self.display_name()
            } else {
                title
            },
            hidden: HashSet::new(),
            layers: HashMap::new(),
        }
    }
}

/// How a print file is written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuildOptions {
    pub content: Content,
    /// Compress the streams made here (off for quick previews).
    pub compress: bool,
}

/// A document read for printing: a private copy, never saved.
pub struct PrintSource {
    doc: Document,
    page_ids: Vec<ObjectId>,
    geoms: Vec<PageGeom>,
    labels: Vec<String>,
    /// The print state of every layer.
    oc_print: HashMap<ObjectId, bool>,
    hidden: HashSet<ObjectId>,
    protection: Option<lopdf::EncryptionState>,
    id: Option<Object>,
    title: String,
}

impl std::fmt::Debug for PrintSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PrintSource")
            .field("pages", &self.page_ids.len())
            .field("title", &self.title)
            .finish()
    }
}

/// The widgets ask for new appearances (`/NeedAppearances true`).
fn need_appearances(doc: &lopdf::Document) -> bool {
    doc.catalog()
        .ok()
        .and_then(|c| c.get(b"AcroForm").ok())
        .and_then(|f| deref(doc, f).as_dict().ok())
        .and_then(|f| f.get(b"NeedAppearances").ok())
        .and_then(|v| v.as_bool().ok())
        .unwrap_or(false)
}

/// The annotations of a page: their ids (when indirect) and dictionaries.
fn page_annotations(doc: &lopdf::Document, page: ObjectId) -> Vec<(Option<ObjectId>, Dictionary)> {
    let Some(list) = doc
        .get_dictionary(page)
        .ok()
        .and_then(|d| d.get(b"Annots").ok())
        .and_then(|a| deref(doc, a).as_array().ok())
    else {
        return Vec::new();
    };
    list.iter()
        .filter_map(|item| match item {
            Object::Reference(r) => doc.get_dictionary(*r).ok().map(|d| (Some(*r), d.clone())),
            Object::Dictionary(d) => Some((None, d.clone())),
            _ => None,
        })
        .collect()
}

/// The normal appearance stream of an annotation: its own, or the one of its
/// state (`/AS`), the `Off` state when that is missing.
pub(crate) fn normal_appearance(doc: &lopdf::Document, annot: &Dictionary) -> Option<ObjectId> {
    let ap = deref(doc, annot.get(b"AP").ok()?).as_dict().ok()?;
    let pick = |states: &Dictionary| -> Option<ObjectId> {
        let state = annot.get(b"AS").ok().and_then(|s| s.as_name().ok());
        state
            .and_then(|s| states.get(s).ok())
            .or_else(|| states.get(b"Off").ok())
            .and_then(|o| o.as_reference().ok())
            .filter(|r| matches!(doc.get_object(*r), Ok(Object::Stream(_))))
    };
    match ap.get(b"N").ok()? {
        Object::Reference(r) => match doc.get_object(*r).ok()? {
            Object::Stream(_) => Some(*r),
            Object::Dictionary(states) => pick(states),
            _ => None,
        },
        Object::Dictionary(states) => pick(states),
        _ => None,
    }
}

fn matrix_of(doc: &lopdf::Document, dict: &Dictionary) -> [f32; 6] {
    dict.get(b"Matrix")
        .ok()
        .map(|m| deref(doc, m))
        .and_then(|m| m.as_array().ok())
        .and_then(|a| {
            let v: Vec<f32> = a.iter().filter_map(|o| num(deref(doc, o))).collect();
            (v.len() == 6).then(|| [v[0], v[1], v[2], v[3], v[4], v[5]])
        })
        .unwrap_or([1.0, 0.0, 0.0, 1.0, 0.0, 0.0])
}

/// The matrix that maps the box of an appearance (its `bbox` through its
/// `matrix`) onto the rectangle of the annotation (PDF 32000-1, 12.5.5). The
/// form's own matrix is applied by drawing it.
pub(crate) fn appearance_matrix(bbox: [f32; 4], matrix: [f32; 6], rect: [f32; 4]) -> [f32; 6] {
    let corners = [
        super::plan::apply(matrix, bbox[0], bbox[1]),
        super::plan::apply(matrix, bbox[2], bbox[1]),
        super::plan::apply(matrix, bbox[0], bbox[3]),
        super::plan::apply(matrix, bbox[2], bbox[3]),
    ];
    let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for (x, y) in corners {
        x0 = x0.min(x);
        y0 = y0.min(y);
        x1 = x1.max(x);
        y1 = y1.max(y);
    }
    let (bw, bh) = ((x1 - x0).max(0.001), (y1 - y0).max(0.001));
    let sx = (rect[2] - rect[0]) / bw;
    let sy = (rect[3] - rect[1]) / bh;
    [sx, 0.0, 0.0, sy, rect[0] - x0 * sx, rect[1] - y0 * sy]
}

/// The members of an optional content membership dictionary.
fn ocmd_members(doc: &lopdf::Document, d: &Dictionary) -> Vec<ObjectId> {
    match d.get(b"OCGs") {
        Ok(Object::Reference(r)) => match doc.get_object(*r) {
            Ok(Object::Dictionary(_)) => vec![*r],
            Ok(Object::Array(a)) => a.iter().filter_map(|x| x.as_reference().ok()).collect(),
            _ => Vec::new(),
        },
        Ok(Object::Array(a)) => a.iter().filter_map(|x| x.as_reference().ok()).collect(),
        _ => Vec::new(),
    }
}

/// Whether content marked with `oc` (a layer or a membership dictionary)
/// shows with the layer states `states`. Visibility expressions count as
/// visible.
fn oc_visible(doc: &lopdf::Document, oc: &Object, states: &HashMap<ObjectId, bool>) -> bool {
    let Ok(d) = deref(doc, oc).as_dict() else {
        return true;
    };
    if d.has_type(b"OCMD") {
        if d.has(b"VE") {
            return true;
        }
        let members = ocmd_members(doc, d);
        if members.is_empty() {
            return true;
        }
        let on = |m: &ObjectId| states.get(m).copied().unwrap_or(true);
        return match d.get(b"P").ok().and_then(|p| p.as_name().ok()) {
            Some(b"AllOn") => members.iter().all(on),
            Some(b"AnyOff") => members.iter().any(|m| !on(m)),
            Some(b"AllOff") => members.iter().all(|m| !on(m)),
            _ => members.iter().any(on),
        };
    }
    match oc.as_reference() {
        Ok(id) => states.get(&id).copied().unwrap_or(true),
        Err(_) => true,
    }
}

/// The print state of every layer: the state for printing a layer declares
/// (`/Usage /Print /PrintState`) when the default configuration applies it
/// to printing (`/AS` with the `Print` event), otherwise its state in the view.
fn print_states(doc: &Document, view: &HashMap<ObjectId, bool>) -> HashMap<ObjectId, bool> {
    let mut states = doc.layer_defaults();
    for (id, v) in view {
        if let Some(s) = states.get_mut(id) {
            *s = *v;
        }
    }
    let inner = &doc.inner;
    let auto = inner
        .catalog()
        .ok()
        .and_then(|c| c.get(b"OCProperties").ok())
        .and_then(|o| deref(inner, o).as_dict().ok())
        .and_then(|o| o.get(b"D").ok())
        .and_then(|d| deref(inner, d).as_dict().ok())
        .and_then(|d| d.get(b"AS").ok())
        .and_then(|a| deref(inner, a).as_array().ok());
    for entry in auto.into_iter().flatten() {
        let Ok(entry) = deref(inner, entry).as_dict() else {
            continue;
        };
        if entry.get(b"Event").ok().and_then(|e| e.as_name().ok()) != Some(b"Print") {
            continue;
        }
        let ocgs = entry
            .get(b"OCGs")
            .ok()
            .and_then(|a| deref(inner, a).as_array().ok())
            .map(|a| a.iter().filter_map(|x| x.as_reference().ok()).collect::<Vec<_>>())
            .unwrap_or_default();
        for id in ocgs {
            let state = inner
                .get_dictionary(id)
                .ok()
                .and_then(|g| g.get(b"Usage").ok())
                .and_then(|u| deref(inner, u).as_dict().ok())
                .and_then(|u| u.get(b"Print").ok())
                .and_then(|p| deref(inner, p).as_dict().ok())
                .and_then(|p| p.get(b"PrintState").ok())
                .and_then(|s| s.as_name().ok());
            match state {
                Some(b"ON") => {
                    states.insert(id, true);
                }
                Some(b"OFF") => {
                    states.insert(id, false);
                }
                _ => {}
            }
        }
    }
    states
}

/// Copies objects of the source into the print file, each once.
struct Copier<'a> {
    src: &'a lopdf::Document,
    out: &'a mut lopdf::Document,
    /// Source id to new id (`None`: not copied, such as pages).
    map: HashMap<ObjectId, Option<ObjectId>>,
    queue: Vec<(ObjectId, ObjectId)>,
}

impl<'a> Copier<'a> {
    fn new(src: &'a lopdf::Document, out: &'a mut lopdf::Document) -> Self {
        Copier {
            src,
            out,
            map: HashMap::new(),
            queue: Vec::new(),
        }
    }

    /// The new reference of a source object (copied by [`Copier::run`]).
    /// Pages, the page tree and the catalogue are never copied.
    fn reference(&mut self, id: ObjectId) -> Object {
        if let Some(m) = self.map.get(&id) {
            return m.map(Object::Reference).unwrap_or(Object::Null);
        }
        let skip = match self.src.get_object(id) {
            Ok(Object::Dictionary(d)) => {
                d.has_type(b"Page") || d.has_type(b"Pages") || d.has_type(b"Catalog")
            }
            Ok(_) => false,
            Err(_) => true,
        };
        if skip {
            self.map.insert(id, None);
            return Object::Null;
        }
        let new = self.out.new_object_id();
        self.map.insert(id, Some(new));
        self.queue.push((id, new));
        Object::Reference(new)
    }

    fn object(&mut self, o: &Object) -> Object {
        match o {
            Object::Reference(id) => self.reference(*id),
            Object::Array(a) => Object::Array(a.iter().map(|x| self.object(x)).collect()),
            Object::Dictionary(d) => Object::Dictionary(self.dict(d)),
            Object::Stream(s) => Object::Stream(Stream {
                dict: self.dict(&s.dict),
                content: s.content.clone(),
                allows_compression: s.allows_compression,
                start_position: None,
            }),
            other => other.clone(),
        }
    }

    /// A dictionary without its links to the structure of the document
    /// (parents, structure keys, metadata) and without entries that point to
    /// pages.
    fn dict(&mut self, d: &Dictionary) -> Dictionary {
        let mut out = Dictionary::new();
        for (k, v) in d.iter() {
            if matches!(
                k.as_slice(),
                b"Parent" | b"StructParent" | b"StructParents" | b"Metadata"
            ) {
                continue;
            }
            let nv = self.object(v);
            if matches!(nv, Object::Null) && !matches!(v, Object::Null) {
                continue;
            }
            out.set(k.clone(), nv);
        }
        out
    }

    /// Copy everything referenced so far (and what it references).
    fn run(&mut self) {
        let src = self.src;
        while let Some((old, new)) = self.queue.pop() {
            let copied = match src.get_object(old) {
                Ok(o) => self.object(o),
                Err(_) => Object::Null,
            };
            self.out.objects.insert(new, copied);
        }
    }
}

/// Inflate the whole of `data` (zlib, or raw deflate), failing on damaged or
/// incomplete data and past `LIMIT` bytes.
fn inflate(data: &[u8], zlib: bool) -> Option<Vec<u8>> {
    const LIMIT: usize = 1 << 30;
    let mut d = flate2::Decompress::new(zlib);
    let mut out: Vec<u8> = Vec::with_capacity(data.len().saturating_mul(3).clamp(64, 64 << 20));
    loop {
        if out.len() == out.capacity() {
            if out.len() >= LIMIT {
                return None;
            }
            out.reserve(out.len().max(4096));
        }
        let (before_in, before_out) = (d.total_in(), d.total_out());
        let rest = data.get(d.total_in() as usize..)?;
        match d
            .decompress_vec(rest, &mut out, flate2::FlushDecompress::Finish)
            .ok()?
        {
            flate2::Status::StreamEnd => return Some(out),
            _ if d.total_in() == before_in && d.total_out() == before_out && out.len() < out.capacity() => {
                return None;
            }
            _ => {}
        }
    }
}

/// The decoded data of a stream, strictly: a compressed stream that cannot
/// be decoded completely is an error (never an empty or raw stream).
pub(crate) fn decode_strict(s: &Stream) -> Option<Vec<u8>> {
    let filters: Vec<Vec<u8>> = match s.filters() {
        Ok(f) => f.into_iter().map(|f| f.to_vec()).collect(),
        Err(_) => return Some(s.content.clone()),
    };
    match filters.as_slice() {
        [] => Some(s.content.clone()),
        [f] if f == b"FlateDecode" && !s.dict.has(b"DecodeParms") => {
            inflate(&s.content, true).or_else(|| inflate(s.content.get(2..)?, false))
        }
        _ => s.get_plain_content().ok(),
    }
}

/// A number for a content stream: up to five decimals, no exponent.
fn n(v: f32) -> String {
    if !v.is_finite() {
        return "0".into();
    }
    let s = format!("{v:.5}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" { "0".into() } else { s.to_string() }
}

fn nums(v: &[f32]) -> String {
    v.iter().map(|x| n(*x)).collect::<Vec<_>>().join(" ")
}

fn flate(data: &[u8]) -> Vec<u8> {
    let mut enc = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast());
    // Writing to memory does not fail.
    let _ = enc.write_all(data);
    enc.finish().unwrap_or_default()
}

/// A stream made here, compressed when asked.
fn own_stream(mut dict: Dictionary, data: Vec<u8>, compress: bool) -> Stream {
    if compress && data.len() > 64 {
        dict.set("Filter", Object::Name(b"FlateDecode".to_vec()));
        Stream::new(dict, flate(&data))
    } else {
        Stream::new(dict, data)
    }
}

fn rect_obj(r: [f32; 4]) -> Object {
    Object::Array(r.iter().map(|v| Object::Real(*v)).collect())
}

/// The forms of the pages drawn by one print file.
struct Forms {
    /// Page index to the form of the page as printed.
    pages: HashMap<usize, ObjectId>,
}

impl PrintSource {
    /// Read a document for printing (call away from the interface thread).
    /// Widgets get new appearances when the form asks for them.
    pub fn load(seed: PrintSeed) -> Result<PrintSource> {
        let inner = lopdf::Document::load_mem(&seed.bytes)?;
        let mut doc = Document::detached(inner, seed.bytes.clone());
        let page_ids = doc.page_ids();
        if need_appearances(&doc.inner) {
            for &page in &page_ids {
                let widgets: Vec<ObjectId> = page_annotations(&doc.inner, page)
                    .into_iter()
                    .filter(|(_, d)| d.get(b"Subtype").ok().and_then(|s| s.as_name().ok()) == Some(b"Widget"))
                    .filter_map(|(id, _)| id)
                    .collect();
                for w in widgets {
                    // A widget whose appearance cannot be made keeps the one it has.
                    let _ = doc.refresh_widget(w);
                }
            }
        }
        let geoms = page_ids
            .iter()
            .map(|&id| {
                let size = doc.page_size_of(id);
                let unit = doc
                    .inner
                    .get_dictionary(id)
                    .ok()
                    .and_then(|d| d.get(b"UserUnit").ok())
                    .and_then(num)
                    .unwrap_or(1.0);
                PageGeom::from_size(&size, unit)
            })
            .collect();
        let labels = doc.page_labels();
        let oc_print = print_states(&doc, &seed.layers);
        Ok(PrintSource {
            doc,
            page_ids,
            geoms,
            labels,
            oc_print,
            hidden: seed.hidden,
            protection: seed.protection,
            id: seed.id,
            title: seed.title,
        })
    }

    pub fn page_count(&self) -> usize {
        self.page_ids.len()
    }

    pub fn geoms(&self) -> &[PageGeom] {
        &self.geoms
    }

    /// The page labels (the page numbers when the document has none).
    pub fn labels(&self) -> &[String] {
        &self.labels
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    /// The source is protected; a saved print file is protected the same way.
    pub fn is_protected(&self) -> bool {
        self.protection.is_some()
    }

    /// What the security settings of the source allow for printing.
    pub fn permission(&self) -> PrintPermission {
        permission_of(self.protection.as_ref())
    }

    /// The printable annotations of a page for `content`: their appearance
    /// streams and the matrix that puts each on its rectangle.
    fn printable(&self, page: usize, content: Content) -> Vec<(ObjectId, [f32; 6])> {
        let doc = &self.doc.inner;
        let mut out = Vec::new();
        for (id, dict) in page_annotations(doc, self.page_ids[page]) {
            if id.is_some_and(|i| self.hidden.contains(&i)) {
                continue;
            }
            let subtype = dict
                .get(b"Subtype")
                .ok()
                .and_then(|s| s.as_name().ok())
                .unwrap_or(b"");
            if subtype == b"Popup" || subtype == b"Link" {
                continue;
            }
            let widget = subtype == b"Widget";
            if !widget && content != Content::DocumentAndComments {
                continue;
            }
            let flags = dict.get(b"F").ok().and_then(|f| f.as_i64().ok()).unwrap_or(0);
            if flags & 4 == 0 || flags & 2 != 0 {
                continue;
            }
            if let Ok(oc) = dict.get(b"OC") {
                if !oc_visible(doc, oc, &self.oc_print) {
                    continue;
                }
            }
            let Some(ap) = normal_appearance(doc, &dict) else {
                continue;
            };
            let Some(r) = dict.get(b"Rect").ok().and_then(|r| rect_of(doc, r)) else {
                continue;
            };
            let rect = [r[0].min(r[2]), r[1].min(r[3]), r[0].max(r[2]), r[1].max(r[3])];
            if rect[2] - rect[0] <= 0.0 || rect[3] - rect[1] <= 0.0 {
                continue;
            }
            let Ok(form) = doc.get_object(ap).and_then(Object::as_stream) else {
                continue;
            };
            let bbox = form
                .dict
                .get(b"BBox")
                .ok()
                .and_then(|b| rect_of(doc, b))
                .unwrap_or(rect);
            out.push((ap, appearance_matrix(bbox, matrix_of(doc, &form.dict), rect)));
        }
        out
    }

    /// The form of the content of a page.
    fn content_form(&self, cp: &mut Copier<'_>, page: usize, compress: bool) -> Result<ObjectId> {
        let doc = &self.doc.inner;
        let pid = self.page_ids[page];
        let media = self.doc.page_size_of(pid).media_box;
        let streams = doc.get_page_contents(pid);
        let mut dict = Dictionary::new();
        dict.set("Type", Object::Name(b"XObject".to_vec()));
        dict.set("Subtype", Object::Name(b"Form".to_vec()));
        dict.set("BBox", rect_obj(media));
        if let Some(res) = self.doc.inherited_attr(pid, b"Resources") {
            let copied = cp.object(&res);
            dict.set("Resources", copied);
        }
        if let Ok(group) = doc.get_dictionary(pid).and_then(|d| d.get(b"Group")) {
            let copied = cp.object(group);
            if !matches!(copied, Object::Null) {
                dict.set("Group", copied);
            }
        }
        let stream = if let [one] = streams.as_slice() {
            // One stream: its bytes as they are, with their filters.
            match doc.get_object(*one) {
                Ok(Object::Stream(s)) => {
                    for key in [&b"Filter"[..], b"DecodeParms"] {
                        if let Ok(v) = s.dict.get(key) {
                            let copied = cp.object(deref(doc, v));
                            dict.set(key.to_vec(), copied);
                        }
                    }
                    Stream::new(dict, s.content.clone())
                }
                _ => Stream::new(dict, Vec::new()),
            }
        } else {
            let mut data = Vec::new();
            for id in &streams {
                let Ok(Object::Stream(s)) = doc.get_object(*id) else {
                    continue;
                };
                let plain = decode_strict(s).ok_or_else(|| {
                    Error::Invalid(format!(
                        "page {}: a content stream uses a filter that cannot be read",
                        page + 1
                    ))
                })?;
                data.extend_from_slice(&plain);
                data.push(b'\n');
            }
            own_stream(dict, data, compress)
        };
        cp.run();
        Ok(cp.out.add_object(stream))
    }

    /// The form of a page as printed: its content (unless only the form
    /// fields print) and its printable annotations, clipped to the crop box.
    fn page_form(
        &self,
        cp: &mut Copier<'_>,
        forms: &mut Forms,
        page: usize,
        o: &BuildOptions,
    ) -> Result<ObjectId> {
        if let Some(id) = forms.pages.get(&page) {
            return Ok(*id);
        }
        let crop = self.geoms[page].crop;
        let mut xobjects = Dictionary::new();
        let mut content = Vec::new();
        if o.content != Content::FormFieldsOnly {
            let c = self.content_form(cp, page, o.compress)?;
            xobjects.set("C", Object::Reference(c));
            content.extend_from_slice(b"q /C Do Q\n");
        }
        for (i, (ap, m)) in self.printable(page, o.content).into_iter().enumerate() {
            let Object::Reference(new) = cp.reference(ap) else {
                continue;
            };
            cp.run();
            if let Ok(Object::Stream(s)) = cp.out.get_object_mut(new) {
                s.dict.set("Type", Object::Name(b"XObject".to_vec()));
                s.dict.set("Subtype", Object::Name(b"Form".to_vec()));
                if !s.dict.has(b"BBox") {
                    s.dict.set("BBox", rect_obj(crop));
                }
            }
            let name = format!("A{i}");
            xobjects.set(name.clone(), Object::Reference(new));
            let _ = writeln!(content, "q {} cm /{name} Do Q", nums(&m));
        }
        let mut dict = Dictionary::new();
        dict.set("Type", Object::Name(b"XObject".to_vec()));
        dict.set("Subtype", Object::Name(b"Form".to_vec()));
        dict.set("BBox", rect_obj(crop));
        let mut res = Dictionary::new();
        res.set("XObject", Object::Dictionary(xobjects));
        dict.set("Resources", Object::Dictionary(res));
        let id = cp.out.add_object(own_stream(dict, content, o.compress));
        forms.pages.insert(page, id);
        Ok(id)
    }

    /// The sheets `range` of `plan` as a PDF, unencrypted. `progress(done,
    /// total)` is called after each sheet; when it returns false the work
    /// stops and the result is `Ok(None)`.
    pub fn build(
        &self,
        plan: &PrintPlan,
        range: Range<usize>,
        o: &BuildOptions,
        progress: &mut dyn FnMut(usize, usize) -> bool,
    ) -> Result<Option<Vec<u8>>> {
        let range = range.start.min(plan.sheets.len())..range.end.min(plan.sheets.len());
        let total = range.len();
        if total == 0 {
            return Err(Error::Invalid("there are no sheets to print".into()));
        }
        if !progress(0, total) {
            return Ok(None);
        }
        let mut out = lopdf::Document::with_version("1.7");
        let pages_id = out.new_object_id();
        let mut kids = Vec::with_capacity(total);
        let mut label_font: Option<ObjectId> = None;
        let mut forms = Forms {
            pages: HashMap::new(),
        };
        let mut cp = Copier::new(&self.doc.inner, &mut out);
        for (done, sheet) in plan.sheets[range.clone()].iter().enumerate() {
            let mut xobjects = Dictionary::new();
            let mut s = Vec::new();
            s.extend_from_slice(b"q\n");
            if sheet.landscape {
                let _ = writeln!(s, "{} cm", nums(&sheet.layout_matrix()));
            }
            for (k, pl) in sheet.placements.iter().enumerate() {
                if pl.page >= self.page_ids.len() {
                    continue;
                }
                let form = self.page_form(&mut cp, &mut forms, pl.page, o)?;
                let name = format!("P{k}");
                xobjects.set(name.clone(), Object::Reference(form));
                let _ = writeln!(s, "q {} re W n {} cm /{name} Do Q", nums(&pl.clip), nums(&pl.m));
            }
            for pl in &sheet.placements {
                if let Some(b) = pl.border {
                    let _ = writeln!(s, "q 0 G 0.5 w {} re S Q", nums(&b));
                }
            }
            let mut has_label = false;
            for mark in &sheet.marks {
                match mark {
                    Mark::Line(l) => {
                        let _ = writeln!(
                            s,
                            "q 0.45 G 0.25 w {} {} m {} {} l S Q",
                            n(l[0]),
                            n(l[1]),
                            n(l[2]),
                            n(l[3])
                        );
                    }
                    Mark::Label { x, y, text } => {
                        has_label = true;
                        let ascii: String = text
                            .chars()
                            .filter(|c| c.is_ascii() && !c.is_ascii_control())
                            .collect();
                        let _ = write!(s, "BT /FL 6 Tf 0.45 g {} {} Td ", n(*x), n(*y));
                        s.extend_from_slice(&crate::util::pdf_literal(&ascii));
                        s.extend_from_slice(b" Tj ET\n");
                    }
                }
            }
            s.extend_from_slice(b"Q\n");
            let contents = cp.out.add_object(own_stream(Dictionary::new(), s, o.compress));
            let mut res = Dictionary::new();
            res.set("XObject", Object::Dictionary(xobjects));
            if has_label {
                let font = *label_font.get_or_insert_with(|| {
                    let mut f = Dictionary::new();
                    f.set("Type", Object::Name(b"Font".to_vec()));
                    f.set("Subtype", Object::Name(b"Type1".to_vec()));
                    f.set("BaseFont", Object::Name(b"Helvetica".to_vec()));
                    f.set("Encoding", Object::Name(b"WinAnsiEncoding".to_vec()));
                    cp.out.add_object(f)
                });
                let mut fonts = Dictionary::new();
                fonts.set("FL", Object::Reference(font));
                res.set("Font", Object::Dictionary(fonts));
            }
            let mut page = Dictionary::new();
            page.set("Type", Object::Name(b"Page".to_vec()));
            page.set("Parent", Object::Reference(pages_id));
            page.set(
                "MediaBox",
                rect_obj([0.0, 0.0, sheet.paper.width, sheet.paper.height]),
            );
            page.set("Resources", Object::Dictionary(res));
            page.set("Contents", Object::Reference(contents));
            kids.push(Object::Reference(cp.out.add_object(page)));
            if !progress(done + 1, total) {
                return Ok(None);
            }
        }
        // Layers: the print state of each, fixed in the default configuration.
        let mut oc_properties = None;
        if !self.oc_print.is_empty() {
            let order: Vec<ObjectId> = self.doc.layers().into_iter().map(|l| l.id).collect();
            let mut ocgs = Vec::new();
            let mut off = Vec::new();
            for id in order {
                let r = cp.reference(id);
                if matches!(r, Object::Null) {
                    continue;
                }
                if self.oc_print.get(&id) == Some(&false) {
                    off.push(r.clone());
                }
                ocgs.push(r);
            }
            cp.run();
            let mut d = Dictionary::new();
            d.set("BaseState", Object::Name(b"ON".to_vec()));
            d.set("OFF", Object::Array(off));
            let mut props = Dictionary::new();
            props.set("OCGs", Object::Array(ocgs));
            props.set("D", Object::Dictionary(d));
            oc_properties = Some(props);
        }
        cp.run();
        drop(cp);
        let mut tree = Dictionary::new();
        tree.set("Type", Object::Name(b"Pages".to_vec()));
        tree.set("Count", Object::Integer(kids.len() as i64));
        tree.set("Kids", Object::Array(kids));
        out.objects.insert(pages_id, Object::Dictionary(tree));
        let mut catalog = Dictionary::new();
        catalog.set("Type", Object::Name(b"Catalog".to_vec()));
        catalog.set("Pages", Object::Reference(pages_id));
        if let Some(props) = oc_properties {
            catalog.set("OCProperties", Object::Dictionary(props));
        }
        let catalog_id = out.add_object(catalog);
        let mut info = Dictionary::new();
        info.set(
            "Producer",
            lopdf::text_string(&format!("{} {}", crate::APP_NAME, crate::VERSION)),
        );
        if !self.title.is_empty() {
            info.set("Title", lopdf::text_string(&self.title));
        }
        info.set(
            "CreationDate",
            Object::string_literal(crate::util::pdf_date_now()),
        );
        let info_id = out.add_object(info);
        out.trailer.set("Root", Object::Reference(catalog_id));
        out.trailer.set("Info", Object::Reference(info_id));
        // The same /ID as the source: the keys of a protected copy depend on it.
        if let Some(id) = &self.id {
            out.trailer.set("ID", id.clone());
        }
        let mut bytes = Vec::new();
        out.save_to(&mut bytes)?;
        Ok(Some(bytes))
    }

    /// A checked print file protected like the source (the same passwords and
    /// permissions), for saving: compressed, then encrypted, then written.
    /// Unprotected sources give the bytes as they are.
    pub fn protect(&self, plain: &[u8]) -> Result<Vec<u8>> {
        let Some(state) = &self.protection else {
            return Ok(plain.to_vec());
        };
        let fail = |why: &str| Error::Invalid(format!("the protected print file failed its check: {why}"));
        let mut doc = lopdf::Document::load_mem(plain)?;
        if let Some(id) = &self.id {
            doc.trailer.set("ID", id.clone());
        }
        doc.encrypt(state)?;
        let mut out = Vec::new();
        doc.save_to(&mut out)?;
        if !crate::util::looks_like_whole_pdf(&out) {
            return Err(fail("it is not a complete PDF"));
        }
        let back = lopdf::Document::load_mem(&out).map_err(|e| fail(&e.to_string()))?;
        if !(back.is_encrypted() || back.was_encrypted()) {
            return Err(fail("it is not encrypted"));
        }
        if back.trailer.get(b"ID").ok() != self.id.as_ref() {
            return Err(fail("its identifier differs from the document's"));
        }
        Ok(out)
    }
}

/// Check a print file before it is sent or saved: a whole PDF that reads,
/// one page of the planned paper for each sheet of `range`, no annotations,
/// every stream made here decodes, and the renderer reads it too.
pub fn validate(bytes: &Arc<Vec<u8>>, plan: &PrintPlan, range: Range<usize>) -> Result<()> {
    let fail = |why: String| Error::Invalid(format!("the print file failed its check: {why}"));
    if !crate::util::looks_like_whole_pdf(bytes) {
        return Err(fail("it is not a complete PDF".into()));
    }
    let doc = lopdf::Document::load_mem(bytes).map_err(|e| fail(e.to_string()))?;
    let pages = doc.get_pages();
    if pages.len() != range.len() || range.end > plan.sheets.len() {
        return Err(fail(format!("{} pages instead of {}", pages.len(), range.len())));
    }
    for (i, (_, id)) in pages.iter().enumerate() {
        let sheet = &plan.sheets[range.start + i];
        let page = doc.get_dictionary(*id).map_err(|e| fail(e.to_string()))?;
        let media = page
            .get(b"MediaBox")
            .ok()
            .and_then(|m| rect_of(&doc, m))
            .ok_or_else(|| fail(format!("sheet {} has no paper size", i + 1)))?;
        let expected = [0.0, 0.0, sheet.paper.width, sheet.paper.height];
        if media.iter().zip(expected).any(|(a, b)| (a - b).abs() > 0.01) {
            return Err(fail(format!("sheet {} is not of the planned paper", i + 1)));
        }
        if page.has(b"Annots") {
            return Err(fail(format!("sheet {} has annotations", i + 1)));
        }
        let mut streams = doc.get_page_contents(*id);
        if let Some(xobjects) = page
            .get(b"Resources")
            .ok()
            .and_then(|r| deref(&doc, r).as_dict().ok())
            .and_then(|r| r.get(b"XObject").ok())
            .and_then(|x| deref(&doc, x).as_dict().ok())
        {
            streams.extend(xobjects.iter().filter_map(|(_, v)| v.as_reference().ok()));
        }
        for sid in streams {
            let s = doc
                .get_object(sid)
                .and_then(Object::as_stream)
                .map_err(|_| fail(format!("sheet {} misses a stream", i + 1)))?;
            decode_strict(s).ok_or_else(|| fail(format!("a stream of sheet {} cannot be decoded", i + 1)))?;
        }
    }
    let renderer = crate::render::Renderer::new(bytes.clone()).map_err(|e| fail(e.to_string()))?;
    if renderer.page_count() != range.len() {
        return Err(fail("the renderer reads another number of pages".into()));
    }
    Ok(())
}
