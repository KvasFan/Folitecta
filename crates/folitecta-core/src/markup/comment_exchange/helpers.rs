//! Small helpers of comment exchange: numbers, colours, flags and names read and written,
//! objects copied between documents, hexadecimal data, rich text.

use std::collections::{BTreeMap, HashSet};

use lopdf::{Dictionary, Object, ObjectId};

use super::*;
use crate::util::{deref, dict_text, fmt_num, num};
use crate::xml::{self, XmlElement};

pub(super) fn name_str(d: &Dictionary, key: &[u8]) -> Option<String> {
    d.get(key)
        .ok()
        .and_then(|n| n.as_name().ok())
        .map(|n| String::from_utf8_lossy(n).to_string())
}

pub(super) fn sub_dict<'a>(
    doc: &'a lopdf::Document,
    d: &'a Dictionary,
    key: &[u8],
) -> Option<&'a Dictionary> {
    d.get(key)
        .ok()
        .map(|o| deref(doc, o))
        .and_then(|o| o.as_dict().ok())
}

pub(super) fn numbers(doc: &lopdf::Document, d: &Dictionary, key: &[u8]) -> Vec<f32> {
    d.get(key)
        .ok()
        .map(|o| deref(doc, o))
        .and_then(|o| o.as_array().ok())
        .map(|a| a.iter().filter_map(|v| num(deref(doc, v))).collect())
        .unwrap_or_default()
}

pub(super) fn number(doc: &lopdf::Document, d: &Dictionary, key: &[u8]) -> Option<f32> {
    d.get(key).ok().and_then(|o| num(deref(doc, o)))
}

pub(super) fn join(v: &[f32], sep: &str) -> String {
    v.iter().map(|x| fmt_num(*x)).collect::<Vec<_>>().join(sep)
}

/// `x,y;x,y;...`
pub(super) fn points_text(v: &[f32]) -> String {
    v.chunks(2)
        .filter(|c| c.len() == 2)
        .map(|c| format!("{},{}", fmt_num(c[0]), fmt_num(c[1])))
        .collect::<Vec<_>>()
        .join(";")
}

pub(super) fn parse_numbers(s: &str) -> Vec<f32> {
    s.split(|c: char| c == ',' || c == ';' || c.is_whitespace())
        .filter(|t| !t.is_empty())
        .filter_map(|t| t.parse::<f32>().ok())
        .filter(|v| v.is_finite())
        .collect()
}

pub(super) fn parse_number(s: Option<&str>) -> Option<f32> {
    s.and_then(|s| s.trim().parse::<f32>().ok())
        .filter(|v| v.is_finite())
}

pub(super) fn yes(s: Option<&str>) -> bool {
    matches!(
        s.map(|s| s.trim().to_ascii_lowercase()).as_deref(),
        Some("yes" | "true" | "1")
    )
}

pub(super) fn hex_color(c: [f32; 3]) -> String {
    let b = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!("#{:02X}{:02X}{:02X}", b(c[0]), b(c[1]), b(c[2]))
}

pub(super) fn parse_color(s: &str) -> Option<[f32; 3]> {
    let h = s.trim().trim_start_matches('#');
    if h.len() != 6 || !h.is_ascii() {
        return None;
    }
    let v = |i: usize| {
        u8::from_str_radix(&h[i..i + 2], 16)
            .ok()
            .map(|b| b as f32 / 255.0)
    };
    Some([v(0)?, v(2)?, v(4)?])
}

pub(super) fn color_from(doc: &lopdf::Document, d: &Dictionary, key: &[u8]) -> Option<[f32; 3]> {
    let v = numbers(doc, d, key);
    match v.len() {
        1 => Some([v[0], v[0], v[0]]),
        3 => Some([v[0], v[1], v[2]]),
        4 => {
            let k = 1.0 - v[3];
            Some([(1.0 - v[0]) * k, (1.0 - v[1]) * k, (1.0 - v[2]) * k])
        }
        _ => None,
    }
}

pub(super) fn number_list(v: &[f32]) -> Object {
    Object::Array(v.iter().map(|x| Object::Real(*x)).collect())
}

pub(super) fn name(s: &str) -> Object {
    Object::Name(s.trim().as_bytes().to_vec())
}

pub(super) fn flags_text(f: i64) -> String {
    FLAG_NAMES
        .iter()
        .enumerate()
        .filter(|(i, _)| f & (1 << i) != 0)
        .map(|(_, n)| *n)
        .collect::<Vec<_>>()
        .join(",")
}

pub(super) fn parse_flags(s: &str) -> i64 {
    s.split(|c: char| c == ',' || c.is_whitespace())
        .filter_map(|t| FLAG_NAMES.iter().position(|n| n.eq_ignore_ascii_case(t.trim())))
        .fold(0, |acc, i| acc | (1 << i))
}

pub(super) fn bbox(pts: &[f32]) -> [f32; 4] {
    let mut r = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
    for c in pts.chunks(2).filter(|c| c.len() == 2) {
        r = [r[0].min(c[0]), r[1].min(c[1]), r[2].max(c[0]), r[3].max(c[1])];
    }
    // Leave room for strokes and line endings.
    [r[0] - 4.0, r[1] - 4.0, r[2] + 4.0, r[3] + 4.0]
}

/// Name of a comment for exchange (`/NM`, or one derived from its object number).
pub(super) fn comment_name(doc: &lopdf::Document, d: &Dictionary, id: ObjectId) -> String {
    dict_text(doc, d, b"NM")
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| format!("folitecta-{}-{}", id.0, id.1))
}

pub(super) fn line_endings(doc: &lopdf::Document, d: &Dictionary) -> (String, String) {
    let n = |o: Option<&Object>| {
        o.and_then(|o| o.as_name().ok())
            .map(|n| String::from_utf8_lossy(n).to_string())
            .unwrap_or_else(|| "None".into())
    };
    match d.get(b"LE").ok().map(|l| deref(doc, l)) {
        Some(Object::Array(a)) => (n(a.first()), n(a.get(1))),
        Some(Object::Name(v)) => (String::from_utf8_lossy(v).to_string(), "None".into()),
        _ => ("None".into(), "None".into()),
    }
}

pub(super) fn collect_refs(obj: &Object, out: &mut Vec<ObjectId>) {
    match obj {
        Object::Reference(r) => out.push(*r),
        Object::Array(a) => a.iter().for_each(|o| collect_refs(o, out)),
        Object::Dictionary(d) => d.iter().for_each(|(_, v)| collect_refs(v, out)),
        Object::Stream(s) => s.dict.iter().for_each(|(_, v)| collect_refs(v, out)),
        _ => {}
    }
}

/// Replace references through `map`; references that were not copied become null.
pub(super) fn remap_strict(obj: &mut Object, map: &BTreeMap<ObjectId, ObjectId>) {
    match obj {
        Object::Reference(r) => match map.get(r) {
            Some(n) => *r = *n,
            None => *obj = Object::Null,
        },
        Object::Array(a) => a.iter_mut().for_each(|o| remap_strict(o, map)),
        Object::Dictionary(d) => d.iter_mut().for_each(|(_, v)| remap_strict(v, map)),
        Object::Stream(s) => s.dict.iter_mut().for_each(|(_, v)| remap_strict(v, map)),
        _ => {}
    }
}

pub(super) fn is_structural(obj: &Object) -> bool {
    obj.as_dict()
        .map(|d| d.has_type(b"Page") || d.has_type(b"Pages") || d.has_type(b"Catalog"))
        .unwrap_or(false)
}

/// Copy the objects in `queue`, and everything they reference, from `src`
/// into `dst`, recording the new ids in `map`. Pages and catalogs are never copied.
pub(super) fn copy_objects(
    src: &lopdf::Document,
    dst: &mut lopdf::Document,
    mut queue: Vec<ObjectId>,
    map: &mut BTreeMap<ObjectId, ObjectId>,
) {
    let mut fresh = Vec::new();
    let mut seen = HashSet::new();
    while let Some(id) = queue.pop() {
        if map.contains_key(&id) || !seen.insert(id) || fresh.len() > 100_000 {
            continue;
        }
        let Ok(obj) = src.get_object(id) else {
            continue;
        };
        if is_structural(obj) {
            continue;
        }
        map.insert(id, dst.new_object_id());
        collect_refs(obj, &mut queue);
        fresh.push(id);
    }
    for id in fresh {
        if let Ok(obj) = src.get_object(id) {
            let mut o = obj.clone();
            remap_strict(&mut o, map);
            dst.objects.insert(map[&id], o);
        }
    }
}

/// References held by a comment dictionary, without the document-local ones.
pub(super) fn content_refs(d: &Dictionary) -> Vec<ObjectId> {
    let mut out = Vec::new();
    for (k, v) in d.iter() {
        if !LOCAL_KEYS.contains(&k.as_slice()) {
            collect_refs(v, &mut out);
        }
    }
    out
}

pub(super) fn hex_decode(s: &str) -> Vec<u8> {
    let digits: Vec<u8> = s
        .bytes()
        .filter(|b| b.is_ascii_hexdigit())
        .map(|b| (b as char).to_digit(16).unwrap_or(0) as u8)
        .collect();
    digits
        .chunks(2)
        .map(|c| (c[0] << 4) | c.get(1).copied().unwrap_or(0))
        .collect()
}

pub(super) fn hex_encode(data: &[u8]) -> String {
    let mut s = String::with_capacity(data.len() * 2);
    for b in data {
        s.push_str(&format!("{b:02X}"));
    }
    s
}

/// The XHTML body of a rich text string, when it is well-formed XML.
pub(super) fn rich_text_element(rc: &str) -> Option<XmlElement> {
    let root = xml::parse(rc).ok()?;
    root.find("body").cloned()
}
