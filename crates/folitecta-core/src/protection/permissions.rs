//! The permissions of a protected file, kept by Folitecta. A file opened
//! with its user password (or with none) whose security settings forbid
//! some kinds of change: every change of the document is sorted, object by
//! object, into the kinds the settings name — comments, filling in forms,
//! assembling pages (pages and bookmarks), changing the document — and a
//! change of a forbidden kind is taken back before it is kept.
//!
//! The sorting leans to allowing: what only follows from a change (a
//! document information entry, an object nothing refers to any more) is
//! no change of its own.

use std::collections::{HashMap, HashSet};

use lopdf::{Dictionary, Object, ObjectId, Permissions};

use super::security::Protection;

/// A kind of change the security settings of a file may forbid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Restricted {
    /// Changing the document: its pages' content, form design, anything else.
    Modify,
    /// Adding, changing or removing comments.
    Annotate,
    /// Filling in form fields.
    FillForms,
    /// Inserting, deleting, rotating and moving pages; bookmarks.
    Assemble,
    /// Copying text and pictures.
    Copy,
}

impl Restricted {
    /// The id of the setting, for the interface's words.
    pub fn id(self) -> &'static str {
        match self {
            Restricted::Modify => "modify",
            Restricted::Annotate => "annotate",
            Restricted::FillForms => "fill_forms",
            Restricted::Assemble => "assemble",
            Restricted::Copy => "copy",
        }
    }
}

/// Whether `protection` allows `kind`: always when there is none or it was
/// opened with the owner password. Revision 2 has no bits of their own for
/// filling forms and assembling: commenting and changing allow them.
pub(crate) fn allows(protection: Option<&Protection>, kind: Restricted) -> bool {
    let Some(state) = protection.filter(|p| !p.owner_access()).map(|p| p.state()) else {
        return true;
    };
    let p = state.permissions();
    let old = state.revision() < 3;
    match kind {
        Restricted::Modify => p.contains(Permissions::MODIFIABLE),
        Restricted::Annotate => p.contains(Permissions::ANNOTABLE),
        Restricted::FillForms => {
            p.contains(Permissions::ANNOTABLE) || (!old && p.contains(Permissions::FILLABLE))
        }
        Restricted::Assemble => {
            if old {
                p.contains(Permissions::MODIFIABLE)
            } else {
                p.contains(Permissions::ASSEMBLABLE)
            }
        }
        Restricted::Copy => p.contains(Permissions::COPYABLE),
    }
}

fn name<'a>(d: &'a Dictionary, key: &[u8]) -> &'a [u8] {
    d.get(key).ok().and_then(|t| t.as_name().ok()).unwrap_or(b"")
}

fn dict(o: &Object) -> Option<&Dictionary> {
    match o {
        Object::Dictionary(d) => Some(d),
        Object::Stream(s) => Some(&s.dict),
        _ => None,
    }
}

/// A dictionary of a form field or of the widget of one.
fn is_field(d: &Dictionary) -> bool {
    name(d, b"Subtype") == b"Widget" || d.has(b"FT") || (d.has(b"T") && d.has(b"Kids"))
}

fn is_comment(d: &Dictionary) -> bool {
    (name(d, b"Type") == b"Annot" || d.has(b"Subtype") && d.has(b"Rect")) && name(d, b"Subtype") != b"Widget"
}

fn is_bookmark(d: &Dictionary) -> bool {
    name(d, b"Type") == b"Outlines" || (d.has(b"Title") && d.has(b"Parent") && !d.has(b"Type"))
}

/// `a` and `b` equal but for the entries `keys`.
fn same_but(a: &Dictionary, b: &Dictionary, keys: &[&[u8]]) -> bool {
    let rest = |d: &Dictionary| -> Vec<(Vec<u8>, Object)> {
        d.iter()
            .filter(|(k, _)| !keys.contains(&k.as_slice()))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    };
    let (mut x, mut y) = (rest(a), rest(b));
    x.sort_by(|p, q| p.0.cmp(&q.0));
    y.sort_by(|p, q| p.0.cmp(&q.0));
    x.len() == y.len() && x.iter().zip(&y).all(|(p, q)| p.0 == q.0 && same(&p.1, &q.1))
}

fn same(a: &Object, b: &Object) -> bool {
    match (a, b) {
        (Object::Integer(x), Object::Real(y)) | (Object::Real(y), Object::Integer(x)) => {
            *x as f64 == f64::from(*y)
        }
        (Object::Dictionary(x), Object::Dictionary(y)) => same_but(x, y, &[]),
        (Object::Stream(x), Object::Stream(y)) => {
            same_but(&x.dict, &y.dict, &[b"Length"]) && x.content == y.content
        }
        (Object::Array(x), Object::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(p, q)| same(p, q))
        }
        (Object::String(x, _), Object::String(y, _)) => x == y,
        _ => a == b,
    }
}

fn references(o: &Object, out: &mut Vec<ObjectId>) {
    match o {
        Object::Reference(id) => out.push(*id),
        Object::Array(a) => a.iter().for_each(|v| references(v, out)),
        Object::Dictionary(d) => d.iter().for_each(|(_, v)| references(v, out)),
        Object::Stream(s) => s.dict.iter().for_each(|(_, v)| references(v, out)),
        _ => {}
    }
}

/// The kind of the objects that only exist for others: the appearances of
/// comments and fields, the content and resources of pages, the items of
/// bookmarks — each object reached from one of a known kind, through the
/// entries `from` names, takes that kind.
fn owners(doc: &lopdf::Document, kinds: &HashMap<ObjectId, Restricted>) -> HashMap<ObjectId, Restricted> {
    let mut out: HashMap<ObjectId, Restricted> = HashMap::new();
    let mut queue: Vec<(ObjectId, Restricted)> = Vec::new();
    for (&id, &kind) in kinds {
        let Some(d) = doc.objects.get(&id).and_then(dict) else {
            continue;
        };
        let keys: &[&[u8]] = match kind {
            Restricted::Annotate | Restricted::FillForms => &[b"AP", b"MK", b"Popup"],
            Restricted::Assemble => &[
                b"Contents",
                b"Resources",
                b"Kids",
                b"First",
                b"Next",
                b"Thumb",
                b"Annots",
            ],
            _ => &[],
        };
        let mut refs = Vec::new();
        for k in keys {
            if let Ok(v) = d.get(k) {
                references(v, &mut refs);
            }
        }
        queue.extend(refs.into_iter().map(|r| (r, kind)));
    }
    while let Some((id, kind)) = queue.pop() {
        if kinds.contains_key(&id) || out.contains_key(&id) {
            continue;
        }
        out.insert(id, kind);
        if let Some(o) = doc.objects.get(&id) {
            let mut refs = Vec::new();
            references(o, &mut refs);
            queue.extend(refs.into_iter().map(|r| (r, kind)));
        }
    }
    out
}

/// The kind of every object that has one of its own: comments, fields,
/// pages and the page tree, bookmarks.
fn own_kinds(doc: &lopdf::Document) -> HashMap<ObjectId, Restricted> {
    doc.objects
        .iter()
        .filter_map(|(&id, o)| {
            let d = dict(o)?;
            let kind = if is_comment(d) {
                Restricted::Annotate
            } else if is_field(d) {
                Restricted::FillForms
            } else if matches!(name(d, b"Type"), b"Page" | b"Pages") || is_bookmark(d) {
                Restricted::Assemble
            } else {
                return None;
            };
            Some((id, kind))
        })
        .collect()
}

/// How one object changed from `old` to `new` (either missing: added or
/// removed), given the kinds objects take in their documents.
/// The form dictionary `b` is `a` with fewer fields (those of deleted
/// pages), or the same.
fn fewer_fields(a: &Dictionary, b: &Dictionary) -> bool {
    let fields = |d: &Dictionary| -> Vec<Object> {
        d.get(b"Fields")
            .ok()
            .and_then(|f| f.as_array().ok())
            .cloned()
            .unwrap_or_default()
    };
    let (before, after) = (fields(a), fields(b));
    same_but(a, b, &[b"Fields", b"NeedAppearances"]) && after.iter().all(|f| before.contains(f))
}

fn kind_of_change(
    old: Option<&Object>,
    new: Option<&Object>,
    taken: Option<Restricted>,
    gone: &dyn Fn(&Dictionary) -> bool,
) -> Option<Restricted> {
    let (od, nd) = (old.and_then(dict), new.and_then(dict));
    let either = nd.or(od)?;
    // What goes with a deleted page (its comments, fields, appearances).
    if nd.is_none() && gone(either) {
        return Some(Restricted::Assemble);
    }
    if is_field(either) {
        // Filling in changes the value and the appearance, nothing else.
        return Some(match (od, nd) {
            (Some(a), Some(b)) if same_but(a, b, &[b"V", b"AS", b"AP"]) => Restricted::FillForms,
            (Some(a), Some(b)) if same_but(a, b, &[b"Kids"]) => Restricted::Assemble,
            _ => Restricted::Modify,
        });
    }
    if is_comment(either) {
        return Some(Restricted::Annotate);
    }
    match name(either, b"Type") {
        b"Page" => {
            return Some(match (od, nd) {
                (Some(a), Some(b)) if same_but(a, b, &[b"Annots"]) => Restricted::Annotate,
                (Some(a), Some(b)) if same_but(a, b, &[b"Rotate", b"Parent", b"Annots"]) => {
                    Restricted::Assemble
                }
                (Some(_), Some(_)) => Restricted::Modify,
                // Pages inserted or deleted.
                _ => Restricted::Assemble,
            });
        }
        b"Pages" => return Some(Restricted::Assemble),
        b"Catalog" => {
            let form = |d: &Dictionary| d.get(b"AcroForm").ok().and_then(|f| f.as_dict().ok()).cloned();
            return match (od, nd) {
                (Some(a), Some(b))
                    if same_but(
                        a,
                        b,
                        &[b"Outlines", b"PageLabels", b"Pages", b"Metadata", b"AcroForm"],
                    ) =>
                {
                    match (form(a), form(b)) {
                        // The form as it was: the change is in the others.
                        (Some(x), Some(y)) if same_but(&x, &y, &[]) => Some(Restricted::Assemble),
                        (Some(x), Some(y)) if same_but(&x, &y, &[b"NeedAppearances"]) => {
                            Some(Restricted::FillForms)
                        }
                        (Some(x), Some(y)) if fewer_fields(&x, &y) => Some(Restricted::Assemble),
                        (None, None) => Some(Restricted::Assemble),
                        _ => Some(Restricted::Modify),
                    }
                }
                _ => Some(Restricted::Modify),
            };
        }
        // What the writer makes again at every save.
        b"Metadata" | b"XRef" | b"ObjStm" => return None,
        _ => {}
    }
    if is_bookmark(either) {
        return Some(Restricted::Assemble);
    }
    // The encryption dictionary.
    if name(either, b"Filter") == b"Standard" {
        return None;
    }
    // The form's dictionary: the fields' list and the flag that asks
    // readers to redraw them follow filling in.
    if either.has(b"Fields") {
        return Some(match (od, nd) {
            (Some(a), Some(b)) if same_but(a, b, &[b"NeedAppearances"]) => Restricted::FillForms,
            (Some(a), Some(b)) if fewer_fields(a, b) => Restricted::Assemble,
            _ => Restricted::Modify,
        });
    }
    // The document information (dates and producer follow every change).
    if either.has(b"Producer")
        || either.has(b"ModDate")
        || either.has(b"CreationDate") && !either.has(b"Size")
    {
        return None;
    }
    match taken {
        // An object that only served another one which went (that one's
        // change is counted), or that nothing refers to any more.
        _ if new.is_none() => None,
        Some(kind) => Some(kind),
        None => Some(Restricted::Modify),
    }
}

/// The kinds of change from `old` to `new`.
pub(crate) fn kinds_of_change(old: &lopdf::Document, new: &lopdf::Document) -> HashSet<Restricted> {
    let (old_own, new_own) = (own_kinds(old), own_kinds(new));
    let (old_taken, new_taken) = (owners(old, &old_own), owners(new, &new_own));
    let ids: HashSet<ObjectId> = old.objects.keys().chain(new.objects.keys()).copied().collect();
    let mut out = HashSet::new();
    for id in ids {
        let (o, n) = (old.objects.get(&id), new.objects.get(&id));
        if let (Some(a), Some(b)) = (o, n) {
            if same(a, b) {
                continue;
            }
        }
        let taken = new_taken.get(&id).or_else(|| old_taken.get(&id)).copied();
        // An object of a page that is no longer there.
        let gone = |d: &Dictionary| {
            d.get(b"P")
                .ok()
                .and_then(|p| p.as_reference().ok())
                .is_some_and(|p| old.objects.contains_key(&p) && !new.objects.contains_key(&p))
        };
        if let Some(kind) = kind_of_change(o, n, taken, &gone) {
            out.insert(kind);
        }
    }
    out
}

/// The first kind of change from `old` to `new` that `protection` forbids.
pub(crate) fn forbidden_change(
    protection: Option<&Protection>,
    old: &lopdf::Document,
    new: &lopdf::Document,
) -> Option<Restricted> {
    let mut kinds: Vec<Restricted> = kinds_of_change(old, new).into_iter().collect();
    kinds.sort();
    kinds.into_iter().find(|k| !allows(protection, *k))
}
