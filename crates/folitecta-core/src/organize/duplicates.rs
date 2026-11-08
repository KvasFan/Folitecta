//! Fonts and pictures kept once (FOL-L045): documents put together from
//! others often carry the same font program, font or picture many times.
//! The copies that are the same object for object (the same dictionary and
//! the same data) become one, the references to them pointing at it, and
//! what nothing uses any more leaves the file. Only what pages and fields
//! draw with is merged (pictures, fonts and their parts, colour profiles):
//! pages, comments, fields and content streams stay each their own, so a
//! later change to one does not reach the others.

use std::collections::{HashMap, HashSet};

use lopdf::{Object, ObjectId};
use sha2::{Digest, Sha256};

use crate::document::Document;
use crate::error::{Error, Result};

/// What merging the copies did.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Deduplicated {
    /// Copies merged into the object they repeat.
    pub merged: usize,
    /// Objects nothing used any more, removed.
    pub unused: usize,
    /// Bytes of stream data the copies took (before compression of the file).
    pub bytes: u64,
}

/// The object `o` written so two objects that are the same give the same
/// bytes (dictionary keys in order; the length of a stream left out, its
/// data by a digest).
fn canon(o: &Object, out: &mut Vec<u8>) {
    match o {
        Object::Null => out.push(b'n'),
        Object::Boolean(b) => out.push(if *b { b't' } else { b'f' }),
        Object::Integer(i) => out.extend(format!("i{i};").bytes()),
        Object::Real(r) => out.extend(format!("r{r};").bytes()),
        Object::Name(n) => {
            out.extend(format!("/{}:", n.len()).bytes());
            out.extend(n);
        }
        Object::String(s, _) => {
            out.extend(format!("s{}:", s.len()).bytes());
            out.extend(s);
        }
        Object::Array(a) => {
            out.push(b'[');
            for x in a {
                canon(x, out);
            }
            out.push(b']');
        }
        Object::Dictionary(d) => canon_dict(d, out, false),
        Object::Stream(s) => {
            canon_dict(&s.dict, out, true);
            out.extend(format!("d{}:", s.content.len()).bytes());
            out.extend(Sha256::digest(&s.content));
        }
        Object::Reference(r) => out.extend(format!("R{}.{};", r.0, r.1).bytes()),
    }
}

fn canon_dict(d: &lopdf::Dictionary, out: &mut Vec<u8>, stream: bool) {
    let mut entries: Vec<(&Vec<u8>, &Object)> = d
        .iter()
        .filter(|(k, _)| !(stream && k.as_slice() == b"Length"))
        .collect();
    entries.sort_by(|a, b| a.0.cmp(b.0));
    out.extend(b"<<");
    for (k, v) in entries {
        out.extend(format!("/{}:", k.len()).bytes());
        out.extend(k);
        canon(v, out);
    }
    out.extend(b">>");
}

fn name_is(d: &lopdf::Dictionary, key: &[u8], value: &[u8]) -> bool {
    d.get(key).ok().and_then(|n| n.as_name().ok()) == Some(value)
}

/// The objects that may be merged: pictures, fonts with the objects they
/// are made of, colour profiles.
fn candidates(doc: &lopdf::Document) -> HashSet<ObjectId> {
    let mut out = HashSet::new();
    let mut parts: Vec<ObjectId> = Vec::new();
    for (id, obj) in &doc.objects {
        match obj {
            Object::Stream(s) if name_is(&s.dict, b"Subtype", b"Image") => {
                out.insert(*id);
            }
            Object::Dictionary(d) if name_is(d, b"Type", b"Font") => {
                out.insert(*id);
                for key in [
                    b"FontDescriptor".as_slice(),
                    b"ToUnicode",
                    b"Widths",
                    b"W",
                    b"W2",
                    b"Encoding",
                    b"CIDSystemInfo",
                    b"CIDToGIDMap",
                    b"DescendantFonts",
                ] {
                    // Direct values are part of the font itself; the
                    // descendant fonts in an array are fonts of their own.
                    if let Ok(Object::Reference(r)) = d.get(key) {
                        parts.push(*r);
                    }
                }
            }
            Object::Dictionary(d) if name_is(d, b"Type", b"FontDescriptor") => {
                out.insert(*id);
                for key in [b"FontFile".as_slice(), b"FontFile2", b"FontFile3", b"CIDSet"] {
                    if let Ok(Object::Reference(r)) = d.get(key) {
                        parts.push(*r);
                    }
                }
            }
            _ => {}
        }
        // Colour profiles: [/ICCBased <ref>] anywhere.
        let mut look = |a: &Vec<Object>| {
            if let [Object::Name(n), Object::Reference(r)] = a.as_slice() {
                if n == b"ICCBased" {
                    parts.push(*r);
                }
            }
        };
        match obj {
            Object::Array(a) => look(a),
            Object::Dictionary(d) => d.iter().for_each(|(_, v)| {
                if let Object::Array(a) = v {
                    look(a);
                }
            }),
            Object::Stream(s) => s.dict.iter().for_each(|(_, v)| {
                if let Object::Array(a) = v {
                    look(a);
                }
            }),
            _ => {}
        }
    }
    // Only objects of those kinds: a page or a comment is never a part.
    for id in parts {
        let kind_ok = match doc.objects.get(&id) {
            Some(Object::Stream(s)) => !s.dict.has(b"Type") || name_is(&s.dict, b"Type", b"CMap"),
            Some(Object::Array(_)) => true,
            Some(Object::Dictionary(d)) => {
                !d.has(b"Type") || name_is(d, b"Type", b"Encoding") || name_is(d, b"Type", b"Font")
            }
            _ => false,
        };
        if kind_ok {
            out.insert(id);
        }
    }
    out
}

/// Point every reference of `o` that `map` names at what it maps to.
fn remap(o: &mut Object, map: &HashMap<ObjectId, ObjectId>) {
    match o {
        Object::Reference(r) => {
            if let Some(to) = map.get(r) {
                *r = *to;
            }
        }
        Object::Array(a) => a.iter_mut().for_each(|x| remap(x, map)),
        Object::Dictionary(d) => d.iter_mut().for_each(|(_, x)| remap(x, map)),
        Object::Stream(s) => s.dict.iter_mut().for_each(|(_, x)| remap(x, map)),
        _ => {}
    }
}

/// Merge the copies of `doc` (see the module); how many, and their bytes.
fn merge(doc: &mut lopdf::Document) -> (usize, u64) {
    let (mut merged, mut bytes) = (0, 0u64);
    // A font is the same as another only once its parts are: a few rounds.
    for _ in 0..8 {
        let mut ids: Vec<ObjectId> = candidates(doc).into_iter().collect();
        ids.sort();
        let mut first: HashMap<Vec<u8>, ObjectId> = HashMap::new();
        let mut map: HashMap<ObjectId, ObjectId> = HashMap::new();
        for id in ids {
            let Some(obj) = doc.objects.get(&id) else { continue };
            let mut key = Vec::new();
            canon(obj, &mut key);
            match first.get(&key) {
                Some(keep) => {
                    map.insert(id, *keep);
                }
                None => {
                    first.insert(key, id);
                }
            }
        }
        if map.is_empty() {
            break;
        }
        for (id, obj) in doc.objects.iter_mut() {
            if !map.contains_key(id) {
                remap(obj, &map);
            }
        }
        for (_, v) in doc.trailer.iter_mut() {
            remap(v, &map);
        }
        for id in map.keys() {
            if let Some(Object::Stream(s)) = doc.objects.remove(id) {
                bytes += s.content.len() as u64;
            }
        }
        merged += map.len();
    }
    (merged, bytes)
}

impl Document {
    /// Keep once the fonts, font programs, pictures and colour profiles the
    /// document holds several times over, and remove what nothing uses any
    /// more. One undo step. Refused for signed documents, whose signed
    /// revisions would change.
    pub fn merge_duplicates(&mut self) -> Result<Deduplicated> {
        self.require(crate::security::Restricted::Modify)?;
        if self.signatures().iter().any(|s| s.signed) {
            return Err(Error::Invalid(
                "the document is signed: keeping its copies once would change what was signed".into(),
            ));
        }
        // On a copy: nothing to merge leaves no undo step.
        let mut copy = self.inner.clone();
        let (merged, bytes) = merge(&mut copy);
        let before = copy.objects.len();
        copy.prune_objects();
        let done = Deduplicated {
            merged,
            unused: before.saturating_sub(copy.objects.len()),
            bytes,
        };
        if done.merged == 0 && done.unused == 0 {
            return Ok(done);
        }
        self.edit_step(|doc| {
            doc.inner = copy;
            Ok(())
        })?;
        Ok(done)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::{Stream, dictionary};

    #[test]
    fn the_same_object_writes_the_same_whatever_the_order_of_keys() {
        let a = Object::Dictionary(dictionary! { "A" => 1, "B" => Object::Reference((4, 0)) });
        let b = Object::Dictionary(dictionary! { "B" => Object::Reference((4, 0)), "A" => 1 });
        let c = Object::Dictionary(dictionary! { "A" => 1, "B" => Object::Reference((5, 0)) });
        let key = |o: &Object| {
            let mut v = Vec::new();
            canon(o, &mut v);
            v
        };
        assert_eq!(key(&a), key(&b));
        assert_ne!(key(&a), key(&c));
        let mut s1 = Stream::new(dictionary! { "Length" => 3 }, b"abc".to_vec());
        let s2 = Stream::new(
            dictionary! { "Length" => Object::Reference((9, 0)) },
            b"abc".to_vec(),
        );
        assert_eq!(key(&Object::Stream(s1.clone())), key(&Object::Stream(s2)));
        s1.set_content(b"abd".to_vec());
        assert_ne!(key(&Object::Stream(s1)), key(&a));
    }
}
