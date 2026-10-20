//! Which pages a change of the document touched, found by comparing the
//! objects of the document with those of the last revision.
//!
//! Every object is summarized by a hash of its contents (the bytes of a
//! stream by a checksum). A page changed when an object it depends on for
//! its picture changed: its own dictionary, its content streams, resources
//! (fonts, pictures, drawings), comments and their appearances, and the
//! inherited attributes of the page tree nodes above it. The references that
//! do not change what a page shows are not followed: the parent of a comment
//! or a field, links' and bookmarks' destinations and actions, the tree of
//! bookmarks, replies. A change of the page tree itself, of the catalog, of
//! the layers' default visibility or of the form's resources counts as a
//! change of every page. The text of a page depends on the same objects but
//! its comments.
//!
//! The comparison only ever widens: whatever it cannot tell (an object it
//! does not know, the first change after opening) counts as a change of
//! every page.

use std::collections::{HashMap, HashSet, VecDeque};
use std::hash::{Hash, Hasher};

use lopdf::{Dictionary, Object, ObjectId};

/// The hash of every object of a document.
pub(crate) type Hashes = HashMap<ObjectId, u64>;

/// What a revision looked like: its objects and its pages.
#[derive(Clone, Default)]
pub(crate) struct Baseline {
    pub hashes: Hashes,
    pub pages: Vec<ObjectId>,
    /// The hash of every page dictionary without its comments (`/Annots`).
    pub page_texts: Hashes,
}

impl Baseline {
    pub(crate) fn of(doc: &lopdf::Document, pages: Vec<ObjectId>) -> Baseline {
        let page_texts = pages
            .iter()
            .filter_map(|id| {
                let d = doc.get_dictionary(*id).ok()?;
                let mut h = std::collections::hash_map::DefaultHasher::new();
                for (k, v) in d.iter().filter(|(k, _)| k.as_slice() != b"Annots") {
                    k.hash(&mut h);
                    hash_object(v, &mut h);
                }
                Some((*id, h.finish()))
            })
            .collect();
        Baseline {
            hashes: hash_objects(doc),
            pages,
            page_texts,
        }
    }
}

/// The pages a change touched.
#[derive(Debug, Default, PartialEq)]
pub(crate) struct Touched {
    /// Every page (the page tree or something shared by all changed).
    pub all: bool,
    /// The pages whose picture may differ.
    pub pictures: HashSet<usize>,
    /// The pages whose text may differ.
    pub text: HashSet<usize>,
}

impl Touched {
    pub(crate) fn everything() -> Touched {
        Touched {
            all: true,
            ..Default::default()
        }
    }
}

fn hash_object(o: &Object, h: &mut impl Hasher) {
    // A number by its value: one made as a real in the program (300.0) is
    // read back from the saved file as an integer (300); the page is the
    // same.
    match o {
        Object::Integer(i) => {
            0xA5u8.hash(h);
            (*i as f64).to_bits().hash(h);
            return;
        }
        Object::Real(r) => {
            0xA5u8.hash(h);
            f64::from(*r).to_bits().hash(h);
            return;
        }
        _ => std::mem::discriminant(o).hash(h),
    }
    match o {
        Object::Null => {}
        Object::Boolean(b) => b.hash(h),
        Object::Integer(_) | Object::Real(_) => {}
        Object::Name(n) => n.hash(h),
        Object::String(s, f) => {
            s.hash(h);
            std::mem::discriminant(f).hash(h);
        }
        Object::Array(a) => {
            a.len().hash(h);
            for v in a {
                hash_object(v, h);
            }
        }
        Object::Dictionary(d) => hash_dict(d, h),
        Object::Stream(s) => {
            hash_dict(&s.dict, h);
            s.content.len().hash(h);
            crc32fast::hash(&s.content).hash(h);
        }
        Object::Reference(id) => id.hash(h),
    }
}

fn hash_dict(d: &Dictionary, h: &mut impl Hasher) {
    d.len().hash(h);
    for (k, v) in d.iter() {
        k.hash(h);
        hash_object(v, h);
    }
}

/// The hash of every object.
pub(crate) fn hash_objects(doc: &lopdf::Document) -> Hashes {
    doc.objects
        .iter()
        .map(|(id, o)| {
            let mut h = std::collections::hash_map::DefaultHasher::new();
            hash_object(o, &mut h);
            (*id, h.finish())
        })
        .collect()
}

/// The keys whose references do not change what a page shows.
const NOT_SHOWN: &[&[u8]] = &[
    b"P",
    b"Dest",
    b"A",
    b"AA",
    b"Next",
    b"Prev",
    b"First",
    b"Last",
    b"IRT",
    b"Popup",
    b"StructParent",
    b"StructParents",
    b"Threads",
    b"B",
    b"Outlines",
];

fn type_of(d: &Dictionary) -> &[u8] {
    d.get(b"Type").ok().and_then(|t| t.as_name().ok()).unwrap_or(b"")
}

/// Every reference in `o`, with the innermost dictionary key it is under.
fn references<'a>(o: &'a Object, key: &'a [u8], out: &mut Vec<(&'a [u8], ObjectId)>) {
    match o {
        Object::Reference(id) => out.push((key, *id)),
        Object::Array(a) => a.iter().for_each(|v| references(v, key, out)),
        Object::Dictionary(d) => d.iter().for_each(|(k, v)| references(v, k, out)),
        Object::Stream(s) => s.dict.iter().for_each(|(k, v)| references(v, k, out)),
        _ => {}
    }
}

/// Whether a reference from `source` under `key` makes the source depend
/// on the target for what it shows (`text`: for its text, comments aside).
fn follows(source: &Object, key: &[u8], text: bool) -> bool {
    if NOT_SHOWN.contains(&key) {
        return false;
    }
    let dict = match source {
        Object::Dictionary(d) => Some(d),
        Object::Stream(s) => Some(&s.dict),
        _ => None,
    };
    let kind = dict.map(type_of).unwrap_or(b"");
    match key {
        // A page depends on the inherited attributes of the nodes above it,
        // the nodes on theirs; nothing else depends on its parent.
        b"Parent" => kind == b"Page" || kind == b"Pages",
        // A page tree node does not depend on its pages.
        b"Kids" => kind != b"Pages",
        b"Annots" if text => kind != b"Page",
        _ => true,
    }
}

/// The objects reachable from `start` (references followed everywhere).
fn reachable(doc: &lopdf::Document, start: &Object) -> HashSet<ObjectId> {
    let mut seen = HashSet::new();
    let mut todo = Vec::new();
    let mut refs = Vec::new();
    references(start, b"", &mut refs);
    todo.extend(refs.iter().map(|(_, id)| *id));
    while let Some(id) = todo.pop() {
        if !seen.insert(id) {
            continue;
        }
        if let Ok(o) = doc.get_object(id) {
            let mut refs = Vec::new();
            references(o, b"", &mut refs);
            todo.extend(refs.iter().map(|(_, id)| *id));
        }
    }
    seen
}

/// The pages of `doc` that changed between `old` and `now` (made of `doc`).
pub(crate) fn touched(doc: &lopdf::Document, old: &Baseline, new: &Baseline) -> Touched {
    let pages = new.pages.as_slice();
    let now = &new.hashes;
    if pages != old.pages.as_slice() {
        return Touched::everything();
    }
    let changed: HashSet<ObjectId> = now
        .iter()
        .filter(|(id, h)| old.hashes.get(id) != Some(h))
        .map(|(id, _)| *id)
        .chain(old.hashes.keys().filter(|id| !now.contains_key(id)).copied())
        .collect();
    if changed.is_empty() {
        return Touched::default();
    }
    // What every page depends on: the catalog, the default visibility of the
    // layers, the form's resources.
    if let Ok(root) = doc.trailer.get(b"Root").and_then(Object::as_reference) {
        if changed.contains(&root) {
            return Touched::everything();
        }
        if let Ok(catalog) = doc.get_dictionary(root) {
            for key in [&b"OCProperties"[..], b"AcroForm"] {
                let Ok(v) = catalog.get(key) else { continue };
                let mut shared = HashSet::new();
                if let Object::Reference(id) = v {
                    shared.insert(*id);
                }
                let value = match v {
                    Object::Reference(id) => doc.get_object(*id).ok().cloned(),
                    other => Some(other.clone()),
                };
                if let Some(value) = value {
                    let scope = if key == b"AcroForm" {
                        // The form's own dictionary and resources, not its fields.
                        match value.as_dict().ok().and_then(|d| d.get(b"DR").ok()) {
                            Some(dr) => reachable(doc, dr),
                            None => HashSet::new(),
                        }
                    } else {
                        reachable(doc, &value)
                    };
                    shared.extend(scope);
                }
                if shared.iter().any(|id| changed.contains(id)) {
                    return Touched::everything();
                }
            }
        }
    }
    // Who depends on whom, the other way round.
    let mut users: HashMap<ObjectId, Vec<(ObjectId, bool)>> = HashMap::new();
    for (id, o) in &doc.objects {
        let mut refs = Vec::new();
        references(o, b"", &mut refs);
        for (key, target) in refs {
            if follows(o, key, false) {
                users
                    .entry(target)
                    .or_default()
                    .push((*id, follows(o, key, true)));
            }
        }
    }
    let index: HashMap<ObjectId, usize> = pages.iter().enumerate().map(|(i, id)| (*id, i)).collect();
    let spread = |text: bool| -> HashSet<usize> {
        // For the text, a page whose dictionary changed in its comments only
        // is not changed itself.
        let seeds: HashSet<ObjectId> = changed
            .iter()
            .copied()
            .filter(|id| !text || !index.contains_key(id) || old.page_texts.get(id) != new.page_texts.get(id))
            .collect();
        let mut seen: HashSet<ObjectId> = seeds.clone();
        let mut todo: VecDeque<ObjectId> = seeds.iter().copied().collect();
        while let Some(id) = todo.pop_front() {
            for (user, for_text) in users.get(&id).map(Vec::as_slice).unwrap_or_default() {
                if (!text || *for_text) && seen.insert(*user) {
                    todo.push_back(*user);
                }
            }
        }
        seen.iter().filter_map(|id| index.get(id).copied()).collect()
    };
    Touched {
        all: false,
        pictures: spread(false),
        text: spread(true),
    }
}
