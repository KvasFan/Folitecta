//! Comment exchange with other applications: comments are exported and
//! imported as XFDF (XML) or FDF, and the comments of another PDF document can
//! be imported page by page.
//!
//! Imported comments keep their name (`/NM`): a comment whose name matches a
//! comment of the document replaces it, so a review can be exported, edited
//! elsewhere and imported again without duplicates.

use std::collections::{BTreeMap, HashMap, HashSet};

use base64::Engine;
use lopdf::{Dictionary, Object, ObjectId, Stream, dictionary};

use crate::document::Document;
use crate::error::{Error, Result};
use crate::util::{deref, dict_text, fmt_num, num, rect_of};
use crate::xml::{self, XmlElement};

/// File formats for comment exchange.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommentFormat {
    Xfdf,
    Fdf,
    Pdf,
}

impl CommentFormat {
    pub fn extension(&self) -> &'static str {
        match self {
            CommentFormat::Xfdf => "xfdf",
            CommentFormat::Fdf => "fdf",
            CommentFormat::Pdf => "pdf",
        }
    }

    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext.to_ascii_lowercase().as_str() {
            "xfdf" | "xml" => Some(CommentFormat::Xfdf),
            "fdf" => Some(CommentFormat::Fdf),
            "pdf" => Some(CommentFormat::Pdf),
            _ => None,
        }
    }

    /// Recognise the format from the content of a file.
    pub fn sniff(data: &[u8]) -> Option<Self> {
        let start = data
            .iter()
            .position(|b| !b.is_ascii_whitespace() && ![0xEF, 0xBB, 0xBF].contains(b))?;
        let head = &data[start..];
        if head.starts_with(b"%FDF") {
            Some(CommentFormat::Fdf)
        } else if head.starts_with(b"%PDF") {
            Some(CommentFormat::Pdf)
        } else if head.starts_with(b"<") {
            Some(CommentFormat::Xfdf)
        } else {
            let window = &data[..data.len().min(1024)];
            if window.windows(5).any(|w| w == b"%PDF-") {
                Some(CommentFormat::Pdf)
            } else if window.windows(5).any(|w| w == b"%FDF-") {
                Some(CommentFormat::Fdf)
            } else {
                None
            }
        }
    }
}

/// Comment subtypes that are exchanged, with their XFDF element names.
const SUBTYPES: &[(&str, &str)] = &[
    ("Text", "text"),
    ("FreeText", "freetext"),
    ("Line", "line"),
    ("Square", "square"),
    ("Circle", "circle"),
    ("Polygon", "polygon"),
    ("PolyLine", "polyline"),
    ("Highlight", "highlight"),
    ("Underline", "underline"),
    ("Squiggly", "squiggly"),
    ("StrikeOut", "strikeout"),
    ("Stamp", "stamp"),
    ("Caret", "caret"),
    ("Ink", "ink"),
    ("FileAttachment", "fileattachment"),
    ("Sound", "sound"),
    ("Redact", "redact"),
];

/// Annotation flag names in bit order (`/F`).
const FLAG_NAMES: &[&str] = &[
    "invisible",
    "hidden",
    "print",
    "nozoom",
    "norotate",
    "noview",
    "readonly",
    "locked",
    "togglenoview",
    "lockedcontents",
];

/// Border styles (`/BS /S`) and their XFDF names.
const BORDER_STYLES: &[(&str, &str)] = &[
    ("S", "solid"),
    ("D", "dash"),
    ("B", "bevelled"),
    ("I", "inset"),
    ("U", "underline"),
];

/// Measurement number formats and their XFDF element names.
const MEASURE_AXES: &[(&str, &str)] = &[("X", "axis"), ("D", "distance"), ("A", "area")];

/// Keys a comment keeps only inside its own document.
const LOCAL_KEYS: &[&[u8]] = &[b"P", b"Popup", b"IRT", b"Parent", b"Page", b"StructParent"];

// ---- Small helpers -------------------------------------------------------------------

fn name_str(d: &Dictionary, key: &[u8]) -> Option<String> {
    d.get(key)
        .ok()
        .and_then(|n| n.as_name().ok())
        .map(|n| String::from_utf8_lossy(n).to_string())
}

fn sub_dict<'a>(doc: &'a lopdf::Document, d: &'a Dictionary, key: &[u8]) -> Option<&'a Dictionary> {
    d.get(key)
        .ok()
        .map(|o| deref(doc, o))
        .and_then(|o| o.as_dict().ok())
}

fn numbers(doc: &lopdf::Document, d: &Dictionary, key: &[u8]) -> Vec<f32> {
    d.get(key)
        .ok()
        .map(|o| deref(doc, o))
        .and_then(|o| o.as_array().ok())
        .map(|a| a.iter().filter_map(|v| num(deref(doc, v))).collect())
        .unwrap_or_default()
}

fn number(doc: &lopdf::Document, d: &Dictionary, key: &[u8]) -> Option<f32> {
    d.get(key).ok().and_then(|o| num(deref(doc, o)))
}

fn join(v: &[f32], sep: &str) -> String {
    v.iter().map(|x| fmt_num(*x)).collect::<Vec<_>>().join(sep)
}

/// `x,y;x,y;...`
fn points_text(v: &[f32]) -> String {
    v.chunks(2)
        .filter(|c| c.len() == 2)
        .map(|c| format!("{},{}", fmt_num(c[0]), fmt_num(c[1])))
        .collect::<Vec<_>>()
        .join(";")
}

fn parse_numbers(s: &str) -> Vec<f32> {
    s.split(|c: char| c == ',' || c == ';' || c.is_whitespace())
        .filter(|t| !t.is_empty())
        .filter_map(|t| t.parse::<f32>().ok())
        .filter(|v| v.is_finite())
        .collect()
}

fn parse_number(s: Option<&str>) -> Option<f32> {
    s.and_then(|s| s.trim().parse::<f32>().ok())
        .filter(|v| v.is_finite())
}

fn yes(s: Option<&str>) -> bool {
    matches!(
        s.map(|s| s.trim().to_ascii_lowercase()).as_deref(),
        Some("yes" | "true" | "1")
    )
}

fn hex_color(c: [f32; 3]) -> String {
    let b = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!("#{:02X}{:02X}{:02X}", b(c[0]), b(c[1]), b(c[2]))
}

fn parse_color(s: &str) -> Option<[f32; 3]> {
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

fn color_from(doc: &lopdf::Document, d: &Dictionary, key: &[u8]) -> Option<[f32; 3]> {
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

fn number_list(v: &[f32]) -> Object {
    Object::Array(v.iter().map(|x| Object::Real(*x)).collect())
}

fn name(s: &str) -> Object {
    Object::Name(s.trim().as_bytes().to_vec())
}

fn flags_text(f: i64) -> String {
    FLAG_NAMES
        .iter()
        .enumerate()
        .filter(|(i, _)| f & (1 << i) != 0)
        .map(|(_, n)| *n)
        .collect::<Vec<_>>()
        .join(",")
}

fn parse_flags(s: &str) -> i64 {
    s.split(|c: char| c == ',' || c.is_whitespace())
        .filter_map(|t| FLAG_NAMES.iter().position(|n| n.eq_ignore_ascii_case(t.trim())))
        .fold(0, |acc, i| acc | (1 << i))
}

fn bbox(pts: &[f32]) -> [f32; 4] {
    let mut r = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
    for c in pts.chunks(2).filter(|c| c.len() == 2) {
        r = [r[0].min(c[0]), r[1].min(c[1]), r[2].max(c[0]), r[3].max(c[1])];
    }
    // Leave room for strokes and line endings.
    [r[0] - 4.0, r[1] - 4.0, r[2] + 4.0, r[3] + 4.0]
}

/// Name of a comment for exchange (`/NM`, or one derived from its object number).
fn comment_name(doc: &lopdf::Document, d: &Dictionary, id: ObjectId) -> String {
    dict_text(doc, d, b"NM")
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| format!("folitecta-{}-{}", id.0, id.1))
}

fn line_endings(doc: &lopdf::Document, d: &Dictionary) -> (String, String) {
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

fn collect_refs(obj: &Object, out: &mut Vec<ObjectId>) {
    match obj {
        Object::Reference(r) => out.push(*r),
        Object::Array(a) => a.iter().for_each(|o| collect_refs(o, out)),
        Object::Dictionary(d) => d.iter().for_each(|(_, v)| collect_refs(v, out)),
        Object::Stream(s) => s.dict.iter().for_each(|(_, v)| collect_refs(v, out)),
        _ => {}
    }
}

/// Replace references through `map`; references that were not copied become null.
fn remap_strict(obj: &mut Object, map: &BTreeMap<ObjectId, ObjectId>) {
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

fn is_structural(obj: &Object) -> bool {
    obj.as_dict()
        .map(|d| d.has_type(b"Page") || d.has_type(b"Pages") || d.has_type(b"Catalog"))
        .unwrap_or(false)
}

/// Copy the objects in `queue`, and everything they reference, from `src`
/// into `dst`, recording the new ids in `map`. Pages and catalogs are never copied.
fn copy_objects(
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
fn content_refs(d: &Dictionary) -> Vec<ObjectId> {
    let mut out = Vec::new();
    for (k, v) in d.iter() {
        if !LOCAL_KEYS.contains(&k.as_slice()) {
            collect_refs(v, &mut out);
        }
    }
    out
}

fn hex_decode(s: &str) -> Vec<u8> {
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

fn hex_encode(data: &[u8]) -> String {
    let mut s = String::with_capacity(data.len() * 2);
    for b in data {
        s.push_str(&format!("{b:02X}"));
    }
    s
}

/// The XHTML body of a rich text string, when it is well-formed XML.
fn rich_text_element(rc: &str) -> Option<XmlElement> {
    let root = xml::parse(rc).ok()?;
    root.find("body").cloned()
}

// ---- Imported comments ---------------------------------------------------------------

/// A comment ready to be added to the document.
struct Imported {
    page: usize,
    dict: Dictionary,
    popup: Option<Dictionary>,
    name: String,
    in_reply_to: Option<String>,
    /// Keep the appearance that came with the comment (FDF and PDF sources).
    keep_appearance: bool,
    /// File of a file attachment comment: name, data, data is Flate-compressed.
    file: Option<(String, Vec<u8>, bool)>,
}

// ---- XFDF ----------------------------------------------------------------------------

fn measure_to_xfdf(doc: &lopdf::Document, m: &Dictionary) -> XmlElement {
    let mut el = XmlElement::new("measure");
    if let Some(r) = dict_text(doc, m, b"R") {
        el.set("rateValue", r);
    }
    for (key, tag) in MEASURE_AXES {
        let formats = m
            .get(key.as_bytes())
            .ok()
            .map(|o| deref(doc, o))
            .and_then(|o| o.as_array().ok())
            .cloned()
            .unwrap_or_default();
        for nf in formats {
            let Ok(nf) = deref(doc, &nf).as_dict() else {
                continue;
            };
            let mut c = XmlElement::new(tag);
            for (k, v) in nf.iter() {
                if k.as_slice() == b"Type" {
                    continue;
                }
                let value = match deref(doc, v) {
                    Object::Name(n) => String::from_utf8_lossy(n).to_string(),
                    Object::Boolean(b) => b.to_string(),
                    other => crate::util::text_of(doc, other).unwrap_or_default(),
                };
                c.set(&String::from_utf8_lossy(k).to_ascii_lowercase(), value);
            }
            el.push(c);
        }
    }
    el
}

fn measure_from_xfdf(el: &XmlElement) -> Dictionary {
    let mut m = dictionary! { "Type" => "Measure", "Subtype" => "RL" };
    if let Some(r) = el.attr("rateValue") {
        m.set("R", lopdf::text_string(r));
    }
    for (key, tag) in MEASURE_AXES {
        let formats: Vec<Object> = el
            .children_named(tag)
            .map(|nf| {
                let mut d = dictionary! { "Type" => "NumberFormat" };
                for (k, v) in &nf.attrs {
                    let k = k.to_ascii_uppercase();
                    let value = match k.as_str() {
                        "F" | "O" => name(v),
                        "FD" => Object::Boolean(yes(Some(v))),
                        "C" => Object::Real(parse_number(Some(v)).unwrap_or(1.0)),
                        "D" => Object::Integer(parse_number(Some(v)).unwrap_or(100.0).round() as i64),
                        _ => lopdf::text_string(v),
                    };
                    d.set(k, value);
                }
                Object::Dictionary(d)
            })
            .collect();
        if !formats.is_empty() {
            m.set(*key, Object::Array(formats));
        }
    }
    m
}

/// Convert one XFDF comment element. `fallback_name` names comments without a name.
fn xfdf_comment(el: &XmlElement, subtype: &str, page: usize, fallback_name: String) -> Option<Imported> {
    let a = |k: &str| el.attr(k);
    let mut d = dictionary! { "Type" => "Annot", "Subtype" => subtype };
    let mut geometry: Vec<f32> = Vec::new();
    let endings = |d: &mut Dictionary| {
        d.set(
            "LE",
            Object::Array(vec![
                name(a("head").unwrap_or("None")),
                name(a("tail").unwrap_or("None")),
            ]),
        );
    };
    match subtype {
        "Line" => {
            let s = parse_numbers(a("start").unwrap_or_default());
            let e = parse_numbers(a("end").unwrap_or_default());
            if s.len() < 2 || e.len() < 2 {
                return None;
            }
            let l = [s[0], s[1], e[0], e[1]];
            geometry.extend(l);
            d.set("L", number_list(&l));
            endings(&mut d);
            for (attr, key) in [
                ("leaderLength", "LL"),
                ("leaderExtend", "LLE"),
                ("leaderOffset", "LLO"),
            ] {
                if let Some(v) = parse_number(a(attr)) {
                    d.set(key, Object::Real(v));
                }
            }
            if yes(a("caption")) {
                d.set("Cap", true);
            }
            if let Some(cp) = a("caption-style") {
                d.set("CP", name(cp));
            }
            if let (Some(h), Some(v)) = (
                parse_number(a("caption-offset-h")),
                parse_number(a("caption-offset-v")),
            ) {
                d.set("CO", number_list(&[h, v]));
            }
        }
        "Polygon" | "PolyLine" => {
            let v = el
                .child("vertices")
                .map(|v| parse_numbers(&v.text()))
                .unwrap_or_default();
            if v.len() < 4 {
                return None;
            }
            let v = v[..v.len() / 2 * 2].to_vec();
            geometry.extend(&v);
            d.set("Vertices", number_list(&v));
            if subtype == "PolyLine" {
                endings(&mut d);
            }
        }
        "Ink" => {
            let gestures: Vec<Vec<f32>> = el
                .child("inklist")
                .map(|l| {
                    l.children_named("gesture")
                        .map(|g| parse_numbers(&g.text()))
                        .filter(|g| g.len() >= 2)
                        .map(|g| g[..g.len() / 2 * 2].to_vec())
                        .collect()
                })
                .unwrap_or_default();
            if gestures.is_empty() {
                return None;
            }
            for g in &gestures {
                geometry.extend(g);
            }
            d.set(
                "InkList",
                Object::Array(gestures.iter().map(|g| number_list(g)).collect()),
            );
        }
        "Highlight" | "Underline" | "Squiggly" | "StrikeOut" | "Redact" => {
            let q = parse_numbers(a("coords").unwrap_or_default());
            if q.len() >= 8 {
                let q = q[..q.len() / 8 * 8].to_vec();
                geometry.extend(&q);
                d.set("QuadPoints", number_list(&q));
            } else if subtype != "Redact" {
                return None;
            }
        }
        _ => {}
    }
    let r = parse_numbers(a("rect").unwrap_or_default());
    let rect = if r.len() >= 4 {
        [r[0].min(r[2]), r[1].min(r[3]), r[0].max(r[2]), r[1].max(r[3])]
    } else if geometry.len() >= 2 {
        bbox(&geometry)
    } else {
        return None;
    };
    d.set("Rect", number_list(&rect));
    if let Some(c) = a("color").and_then(parse_color) {
        d.set("C", number_list(&c));
    }
    if let Some(c) = a("interior-color").and_then(parse_color) {
        d.set("IC", number_list(&c));
    }
    if let Some(v) = parse_number(a("opacity")) {
        d.set("CA", Object::Real(v.clamp(0.0, 1.0)));
    }
    if let Some(f) = a("flags") {
        d.set("F", parse_flags(f));
    }
    let nm = a("name")
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .unwrap_or(fallback_name);
    d.set("NM", lopdf::text_string(&nm));
    for (attr, key) in [("title", "T"), ("subject", "Subj")] {
        if let Some(v) = a(attr) {
            d.set(key, lopdf::text_string(v));
        }
    }
    for (attr, key) in [("date", "M"), ("creationdate", "CreationDate")] {
        if let Some(v) = a(attr) {
            d.set(key, Object::string_literal(v.trim()));
        }
    }
    if let Some(v) = a("intent") {
        d.set("IT", name(v));
    }
    if let Some(v) = parse_number(a("rotation")) {
        d.set("Rotate", v.round() as i64);
    }
    let fringe = parse_numbers(a("fringe").unwrap_or_default());
    if fringe.len() == 4 {
        d.set("RD", number_list(&fringe));
    }
    // Border.
    let style = a("style").map(|s| s.trim().to_ascii_lowercase());
    let mut bs = dictionary! { "Type" => "Border" };
    if let Some(w) = parse_number(a("width")) {
        bs.set("W", Object::Real(w.max(0.0)));
    }
    if let Some(s) = style
        .as_deref()
        .and_then(|s| BORDER_STYLES.iter().find(|(_, n)| *n == s))
    {
        bs.set("S", name(s.0));
    }
    let dashes = parse_numbers(a("dashes").unwrap_or_default());
    if !dashes.is_empty() {
        bs.set("D", number_list(&dashes));
        if !bs.has(b"S") {
            bs.set("S", name("D"));
        }
    }
    if bs.len() > 1 {
        d.set("BS", bs);
    }
    if style.as_deref() == Some("cloudy") {
        let intensity = parse_number(a("intensity")).unwrap_or(1.0);
        d.set("BE", dictionary! { "S" => "C", "I" => Object::Real(intensity) });
    }
    // Subtype entries.
    match subtype {
        "Text" => {
            d.set("Name", name(a("icon").unwrap_or("Note")));
            if let Some(s) = a("state") {
                d.set("State", lopdf::text_string(s));
            }
            if let Some(s) = a("statemodel") {
                d.set("StateModel", lopdf::text_string(s));
            }
        }
        "FreeText" => {
            if let Some(q) = parse_number(a("justification")) {
                d.set("Q", (q.round() as i64).clamp(0, 2));
            }
            let da = el
                .child("defaultappearance")
                .map(|e| e.text())
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| "/Helv 12 Tf 0 0 0 rg".to_string());
            d.set("DA", Object::string_literal(da));
            if let Some(ds) = el.child("defaultstyle").map(|e| e.text()) {
                d.set("DS", lopdf::text_string(&ds));
            }
            let cl = parse_numbers(a("callout").unwrap_or_default());
            if cl.len() >= 4 {
                d.set("CL", number_list(&cl[..cl.len() / 2 * 2]));
            }
            if let Some(h) = a("head") {
                d.set("LE", name(h));
            }
        }
        "Stamp" | "Sound" => {
            if let Some(icon) = a("icon") {
                d.set("Name", name(icon));
            }
        }
        "Caret" => {
            let sym = a("symbol").unwrap_or("none").trim().to_ascii_lowercase();
            d.set(
                "Sy",
                name(if sym == "paragraph" || sym == "p" {
                    "P"
                } else {
                    "None"
                }),
            );
        }
        "FileAttachment" => {
            d.set("Name", name(a("icon").unwrap_or("PushPin")));
        }
        "Redact" => {
            if let Some(t) = a("overlay-text") {
                d.set("OverlayText", lopdf::text_string(t));
            }
            if let Some(q) = parse_number(a("justification")) {
                d.set("Q", (q.round() as i64).clamp(0, 2));
            }
        }
        _ => {}
    }
    if let Some(m) = el.child("measure") {
        d.set("Measure", measure_from_xfdf(m));
    }
    let rich = el
        .child("contents-richtext")
        .and_then(|r| r.elements().next().cloned());
    let contents = el
        .child("contents")
        .map(|c| c.text())
        .or_else(|| rich.as_ref().map(|b| b.text()));
    if let Some(c) = contents.filter(|c| !c.is_empty()) {
        d.set("Contents", lopdf::text_string(&c));
    }
    if let Some(body) = rich {
        d.set(
            "RC",
            lopdf::text_string(&format!("<?xml version=\"1.0\"?>{}", body.to_xml())),
        );
    }
    let in_reply_to = a("inreplyto")
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    if in_reply_to.is_some() {
        let group = a("replyType")
            .map(|t| t.eq_ignore_ascii_case("group"))
            .unwrap_or(false);
        d.set("RT", name(if group { "Group" } else { "R" }));
    }
    let popup = el.child("popup").and_then(|p| {
        let r = parse_numbers(p.attr("rect").unwrap_or_default());
        if r.len() < 4 {
            return None;
        }
        let mut pd = dictionary! {
            "Type" => "Annot",
            "Subtype" => "Popup",
            "Rect" => number_list(&[r[0].min(r[2]), r[1].min(r[3]), r[0].max(r[2]), r[1].max(r[3])]),
            "Open" => yes(p.attr("open")),
        };
        if let Some(f) = p.attr("flags") {
            pd.set("F", parse_flags(f));
        }
        Some(pd)
    });
    let file = if subtype == "FileAttachment" {
        el.child("data").map(|data| {
            let raw = data.text();
            let bytes = match data
                .attr("encoding")
                .map(|e| e.trim().to_ascii_lowercase())
                .as_deref()
            {
                Some("base64") => base64::engine::general_purpose::STANDARD
                    .decode(raw.split_whitespace().collect::<String>())
                    .unwrap_or_default(),
                _ => hex_decode(&raw),
            };
            let flate = data.attr("filter").map(|f| f.contains("Flate")).unwrap_or(false);
            let file_name = a("file").unwrap_or("attachment").to_string();
            (file_name, bytes, flate)
        })
    } else {
        None
    };
    Some(Imported {
        page,
        dict: d,
        popup,
        name: nm,
        in_reply_to,
        keep_appearance: false,
        file,
    })
}

impl Document {
    /// Comments taking part in an exchange: `(page, object id, dictionary)` for
    /// every comment, or only for the listed `(page, index)` pairs.
    fn exchange_comments(&self, only: Option<&[(usize, usize)]>) -> Vec<(usize, ObjectId, Dictionary)> {
        let mut out = Vec::new();
        for page in 0..self.page_count() {
            for a in self.annotations(page).unwrap_or_default() {
                if !SUBTYPES.iter().any(|(s, _)| *s == a.subtype) {
                    continue;
                }
                if let Some(only) = only {
                    if !only.contains(&(page, a.index)) {
                        continue;
                    }
                }
                let Some(id) = a.id else { continue };
                if let Ok(d) = self.inner.get_dictionary(id) {
                    out.push((page, id, d.clone()));
                }
            }
        }
        out
    }

    /// Number of comments an export would contain.
    pub fn exchange_comment_count(&self, only: Option<&[(usize, usize)]>) -> usize {
        self.exchange_comments(only).len()
    }

    fn comment_to_xfdf(&self, page: usize, id: ObjectId, d: &Dictionary) -> Option<XmlElement> {
        let doc = &self.inner;
        let subtype = name_str(d, b"Subtype")?;
        let tag = SUBTYPES.iter().find(|(s, _)| *s == subtype)?.1;
        let rect = d.get(b"Rect").ok().and_then(|r| rect_of(doc, r))?;
        let mut el = XmlElement::new(tag);
        el.set("page", page.to_string());
        el.set("rect", join(&rect, ","));
        if let Some(c) = color_from(doc, d, b"C") {
            el.set("color", hex_color(c));
        }
        if let Some(v) = dict_text(doc, d, b"M") {
            el.set("date", v);
        }
        let flags = d.get(b"F").ok().and_then(|f| f.as_i64().ok()).unwrap_or(0);
        if flags != 0 {
            el.set("flags", flags_text(flags));
        }
        el.set("name", comment_name(doc, d, id));
        if let Some(v) = dict_text(doc, d, b"T") {
            el.set("title", v);
        }
        if let Some(v) = dict_text(doc, d, b"Subj") {
            el.set("subject", v);
        }
        if let Some(v) = dict_text(doc, d, b"CreationDate") {
            el.set("creationdate", v);
        }
        if let Some(v) = number(doc, d, b"CA").filter(|v| *v < 0.999) {
            el.set("opacity", fmt_num(v));
        }
        if let Some(c) = color_from(doc, d, b"IC") {
            el.set("interior-color", hex_color(c));
        }
        let bs = sub_dict(doc, d, b"BS");
        let width = bs.and_then(|b| number(doc, b, b"W")).or_else(|| {
            let border = numbers(doc, d, b"Border");
            border.get(2).copied()
        });
        if let Some(w) = width {
            el.set("width", fmt_num(w));
        }
        if let Some(s) = bs.and_then(|b| name_str(b, b"S")) {
            if let Some((_, n)) = BORDER_STYLES.iter().find(|(k, _)| *k == s) {
                el.set("style", *n);
            }
        }
        let dashes = bs.map(|b| numbers(doc, b, b"D")).unwrap_or_default();
        if !dashes.is_empty() {
            el.set("dashes", join(&dashes, ","));
        }
        if let Some(be) = sub_dict(doc, d, b"BE") {
            if name_str(be, b"S").as_deref() == Some("C") {
                el.set("style", "cloudy");
                el.set("intensity", fmt_num(number(doc, be, b"I").unwrap_or(1.0)));
            }
        }
        if let Some(v) = name_str(d, b"IT") {
            el.set("intent", v);
        }
        if let Ok(irt) = d.get(b"IRT").and_then(|o| o.as_reference()) {
            if let Ok(target) = doc.get_dictionary(irt) {
                el.set("inreplyto", comment_name(doc, target, irt));
                let group = name_str(d, b"RT").as_deref() == Some("Group");
                el.set("replyType", if group { "group" } else { "reply" });
            }
        }
        if let Some(r) = d
            .get(b"Rotate")
            .ok()
            .and_then(|r| r.as_i64().ok())
            .filter(|r| *r != 0)
        {
            el.set("rotation", r.to_string());
        }
        let rd = numbers(doc, d, b"RD");
        if rd.len() == 4 {
            el.set("fringe", join(&rd, ","));
        }
        match subtype.as_str() {
            "Text" => {
                el.set("icon", name_str(d, b"Name").unwrap_or_else(|| "Note".into()));
                if let Some(s) = dict_text(doc, d, b"State") {
                    el.set("state", s);
                }
                if let Some(s) = dict_text(doc, d, b"StateModel") {
                    el.set("statemodel", s);
                }
            }
            "Line" => {
                let l = numbers(doc, d, b"L");
                if l.len() >= 4 {
                    el.set("start", format!("{},{}", fmt_num(l[0]), fmt_num(l[1])));
                    el.set("end", format!("{},{}", fmt_num(l[2]), fmt_num(l[3])));
                }
                let (head, tail) = line_endings(doc, d);
                el.set("head", head);
                el.set("tail", tail);
                for (key, attr) in [
                    (&b"LL"[..], "leaderLength"),
                    (b"LLE", "leaderExtend"),
                    (b"LLO", "leaderOffset"),
                ] {
                    if let Some(v) = number(doc, d, key) {
                        el.set(attr, fmt_num(v));
                    }
                }
                if d.get(b"Cap").ok().and_then(|c| c.as_bool().ok()) == Some(true) {
                    el.set("caption", "yes");
                }
                if let Some(cp) = name_str(d, b"CP") {
                    el.set("caption-style", cp);
                }
                let co = numbers(doc, d, b"CO");
                if co.len() == 2 {
                    el.set("caption-offset-h", fmt_num(co[0]));
                    el.set("caption-offset-v", fmt_num(co[1]));
                }
            }
            "PolyLine" | "Polygon" => {
                if subtype == "PolyLine" {
                    let (head, tail) = line_endings(doc, d);
                    el.set("head", head);
                    el.set("tail", tail);
                }
                el.push(XmlElement::new("vertices").with_text(&points_text(&numbers(doc, d, b"Vertices"))));
            }
            "Ink" => {
                let mut list = XmlElement::new("inklist");
                let paths = d
                    .get(b"InkList")
                    .ok()
                    .map(|l| deref(doc, l))
                    .and_then(|l| l.as_array().ok())
                    .cloned()
                    .unwrap_or_default();
                for p in paths {
                    let v: Vec<f32> = deref(doc, &p)
                        .as_array()
                        .map(|a| a.iter().filter_map(|o| num(deref(doc, o))).collect())
                        .unwrap_or_default();
                    list.push(XmlElement::new("gesture").with_text(&points_text(&v)));
                }
                el.push(list);
            }
            "Highlight" | "Underline" | "Squiggly" | "StrikeOut" | "Redact" => {
                let q = numbers(doc, d, b"QuadPoints");
                if !q.is_empty() {
                    el.set("coords", join(&q, ","));
                }
                if subtype == "Redact" {
                    if let Some(t) = dict_text(doc, d, b"OverlayText") {
                        el.set("overlay-text", t);
                    }
                    if let Some(q) = d.get(b"Q").ok().and_then(|q| q.as_i64().ok()) {
                        el.set("justification", q.to_string());
                    }
                }
            }
            "FreeText" => {
                if let Some(q) = d.get(b"Q").ok().and_then(|q| q.as_i64().ok()) {
                    el.set("justification", q.to_string());
                }
                let cl = numbers(doc, d, b"CL");
                if !cl.is_empty() {
                    el.set("callout", join(&cl, ","));
                }
                if let Some(Object::Name(n)) = d.get(b"LE").ok().map(|l| deref(doc, l)) {
                    el.set("head", String::from_utf8_lossy(n).to_string());
                }
                if let Some(da) = dict_text(doc, d, b"DA") {
                    el.push(XmlElement::new("defaultappearance").with_text(&da));
                }
                if let Some(ds) = dict_text(doc, d, b"DS") {
                    el.push(XmlElement::new("defaultstyle").with_text(&ds));
                }
            }
            "Stamp" | "Sound" => {
                if let Some(icon) = name_str(d, b"Name") {
                    el.set("icon", icon);
                }
            }
            "Caret" => {
                let sym = name_str(d, b"Sy").unwrap_or_default();
                el.set("symbol", if sym == "P" { "paragraph" } else { "none" });
            }
            "FileAttachment" => {
                el.set("icon", name_str(d, b"Name").unwrap_or_else(|| "PushPin".into()));
                if let Some(fs) = sub_dict(doc, d, b"FS") {
                    let file_name = dict_text(doc, fs, b"UF")
                        .or_else(|| dict_text(doc, fs, b"F"))
                        .unwrap_or_default();
                    if !file_name.is_empty() {
                        el.set("file", file_name);
                    }
                    let stream = sub_dict(doc, fs, b"EF")
                        .and_then(|ef| ef.get(b"F").or_else(|_| ef.get(b"UF")).ok())
                        .map(|o| deref(doc, o))
                        .and_then(|o| o.as_stream().ok());
                    if let Some(data) = stream.and_then(|s| s.get_plain_content().ok()) {
                        el.push(
                            XmlElement::new("data")
                                .with("mode", "raw")
                                .with("encoding", "hex")
                                .with("length", data.len().to_string())
                                .with_text(&hex_encode(&data)),
                        );
                    }
                }
            }
            _ => {}
        }
        if let Some(m) = sub_dict(doc, d, b"Measure") {
            el.push(measure_to_xfdf(doc, m));
        }
        if let Some(c) = dict_text(doc, d, b"Contents").filter(|c| !c.is_empty()) {
            el.push(XmlElement::new("contents").with_text(&c));
        }
        if let Some(body) = dict_text(doc, d, b"RC").and_then(|rc| rich_text_element(&rc)) {
            let mut rich = XmlElement::new("contents-richtext");
            rich.push(body);
            el.push(rich);
        }
        if let Ok(pid) = d.get(b"Popup").and_then(|p| p.as_reference()) {
            if let Ok(p) = doc.get_dictionary(pid) {
                if let Some(pr) = p.get(b"Rect").ok().and_then(|r| rect_of(doc, r)) {
                    let open = p
                        .get(b"Open")
                        .ok()
                        .and_then(|o| o.as_bool().ok())
                        .unwrap_or(false);
                    let mut pe = XmlElement::new("popup")
                        .with("page", page.to_string())
                        .with("rect", join(&pr, ","))
                        .with("open", if open { "yes" } else { "no" });
                    let pf = p.get(b"F").ok().and_then(|f| f.as_i64().ok()).unwrap_or(0);
                    if pf != 0 {
                        pe.set("flags", flags_text(pf));
                    }
                    el.push(pe);
                }
            }
        }
        Some(el)
    }

    /// Export comments as XFDF: every comment, or only the listed `(page, index)` pairs.
    pub fn export_comments_xfdf(&self, only: Option<&[(usize, usize)]>) -> String {
        let mut root = XmlElement::new("xfdf")
            .with("xmlns", "http://ns.adobe.com/xfdf/")
            .with("xml:space", "preserve");
        let mut annots = XmlElement::new("annots");
        for (page, id, d) in self.exchange_comments(only) {
            if let Some(el) = self.comment_to_xfdf(page, id, &d) {
                annots.push(el);
            }
        }
        root.push(annots);
        if let Some(file) = self.path().and_then(|p| p.file_name()) {
            root.push(XmlElement::new("f").with("href", file.to_string_lossy().to_string()));
        }
        if let Ok(Object::Array(ids)) = self.inner.trailer.get(b"ID") {
            let hex = |o: Option<&Object>| match o {
                Some(Object::String(b, _)) => hex_encode(b),
                _ => String::new(),
            };
            if ids.len() == 2 {
                root.push(
                    XmlElement::new("ids")
                        .with("original", hex(ids.first()))
                        .with("modified", hex(ids.get(1))),
                );
            }
        }
        format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n{}\n",
            root.to_pretty_xml()
        )
    }

    /// Export comments as FDF, with their appearances: every comment, or only
    /// the listed `(page, index)` pairs.
    pub fn export_comments_fdf(&self, only: Option<&[(usize, usize)]>) -> Result<Vec<u8>> {
        let items = self.exchange_comments(only);
        let src = &self.inner;
        let mut fdf = lopdf::Document::with_version("1.2");
        let mut map: BTreeMap<ObjectId, ObjectId> = BTreeMap::new();
        let exported: HashSet<ObjectId> = items.iter().map(|(_, id, _)| *id).collect();
        let mut popups = HashMap::new();
        for (_, id, d) in &items {
            map.insert(*id, fdf.new_object_id());
            if let Ok(p) = d.get(b"Popup").and_then(|p| p.as_reference()) {
                if src.get_dictionary(p).is_ok() {
                    map.insert(p, fdf.new_object_id());
                    popups.insert(*id, p);
                }
            }
        }
        let queue: Vec<ObjectId> = items.iter().flat_map(|(_, _, d)| content_refs(d)).collect();
        copy_objects(src, &mut fdf, queue, &mut map);
        let mut annots = Vec::new();
        for (page, id, d) in &items {
            let mut a = d.clone();
            a.remove(b"P");
            a.remove(b"StructParent");
            a.set("Page", *page as i64);
            if let Ok(t) = a.get(b"IRT").and_then(|t| t.as_reference()) {
                if !exported.contains(&t) {
                    a.remove(b"IRT");
                    a.remove(b"RT");
                }
            }
            let mut obj = Object::Dictionary(a);
            remap_strict(&mut obj, &map);
            fdf.objects.insert(map[id], obj);
            annots.push(Object::Reference(map[id]));
            if let Some(p) = popups.get(id) {
                let mut pd = src.get_dictionary(*p)?.clone();
                pd.remove(b"P");
                pd.set("Page", *page as i64);
                pd.set("Parent", Object::Reference(*id));
                let mut pobj = Object::Dictionary(pd);
                remap_strict(&mut pobj, &map);
                fdf.objects.insert(map[p], pobj);
                annots.push(Object::Reference(map[p]));
            }
        }
        let mut fdf_dict = dictionary! { "Annots" => annots };
        if let Some(file) = self.path().and_then(|p| p.file_name()) {
            fdf_dict.set("F", Object::string_literal(file.to_string_lossy().to_string()));
        }
        if let Ok(ids @ Object::Array(_)) = src.trailer.get(b"ID") {
            fdf_dict.set("ID", ids.clone());
        }
        let root = fdf.add_object(dictionary! { "FDF" => fdf_dict });
        fdf.trailer.set("Root", root);
        let mut out = Vec::new();
        fdf.save_to(&mut out)?;
        // FDF shares the PDF file structure; only the header differs.
        if out.starts_with(b"%PDF-") {
            out[1] = b'F';
        }
        Ok(out)
    }

    /// Export comments in a format (XFDF or FDF).
    pub fn export_comments(&self, format: CommentFormat, only: Option<&[(usize, usize)]>) -> Result<Vec<u8>> {
        match format {
            CommentFormat::Xfdf => Ok(self.export_comments_xfdf(only).into_bytes()),
            CommentFormat::Fdf => self.export_comments_fdf(only),
            CommentFormat::Pdf => Err(Error::Invalid("comments are exported as XFDF or FDF".into())),
        }
    }

    /// Import comments from XFDF, FDF or PDF data (the format is recognised from
    /// the content) as one undo step. Returns the number of comments imported.
    pub fn import_comments(&mut self, data: &[u8]) -> Result<usize> {
        match CommentFormat::sniff(data) {
            Some(CommentFormat::Xfdf) => self.import_comments_xfdf(&String::from_utf8_lossy(data)),
            Some(CommentFormat::Fdf) => self.import_comments_fdf(data),
            Some(CommentFormat::Pdf) => self.import_comments_pdf(data),
            None => Err(Error::Invalid("the file is not an XFDF, FDF or PDF file".into())),
        }
    }

    /// Import the comments of an XFDF document (one undo step).
    pub fn import_comments_xfdf(&mut self, xml_text: &str) -> Result<usize> {
        let root = xml::parse(xml_text)?;
        if !root.is("xfdf") {
            return Err(Error::Invalid("the file is not an XFDF document".into()));
        }
        let pages = self.page_count();
        let prefix = format!("folitecta-xfdf-{}-{}", self.revision(), self.inner.max_id);
        let items: Vec<Imported> = root
            .child("annots")
            .map(|annots| {
                annots
                    .elements()
                    .enumerate()
                    .filter_map(|(i, el)| {
                        let subtype = SUBTYPES.iter().find(|(_, t)| el.is(t))?.0;
                        let page: usize = el.attr("page").and_then(|p| p.trim().parse().ok()).unwrap_or(0);
                        if page >= pages {
                            return None;
                        }
                        xfdf_comment(el, subtype, page, format!("{prefix}-{i}"))
                    })
                    .collect()
            })
            .unwrap_or_default();
        if items.is_empty() {
            return Err(Error::Invalid(
                "the file contains no comments for the pages of this document".into(),
            ));
        }
        self.begin_edit();
        let n = self.insert_imported(items)?;
        self.commit()?;
        Ok(n)
    }

    /// Import the comments of an FDF file (one undo step).
    pub fn import_comments_fdf(&mut self, data: &[u8]) -> Result<usize> {
        let pos = data
            .windows(5)
            .position(|w| w == b"%FDF-")
            .ok_or_else(|| Error::Invalid("the file is not an FDF file".into()))?;
        let mut bytes = data.to_vec();
        bytes[pos + 1] = b'P';
        let src = lopdf::Document::load_mem(&bytes)
            .map_err(|e| Error::Invalid(format!("cannot read the FDF file: {e}")))?;
        let annots: Vec<Object> = src
            .trailer
            .get(b"Root")
            .ok()
            .map(|r| deref(&src, r))
            .and_then(|r| r.as_dict().ok())
            .and_then(|r| sub_dict(&src, r, b"FDF"))
            .and_then(|f| f.get(b"Annots").ok())
            .map(|a| deref(&src, a))
            .and_then(|a| a.as_array().ok())
            .cloned()
            .unwrap_or_default();
        let pages = self.page_count();
        let entries: Vec<(usize, ObjectId, Dictionary)> = annots
            .iter()
            .enumerate()
            .filter_map(|(i, a)| {
                let (id, d) = match a {
                    Object::Reference(r) => (*r, src.get_dictionary(*r).ok()?.clone()),
                    Object::Dictionary(d) => ((u32::MAX - i as u32, 0), d.clone()),
                    _ => return None,
                };
                let page = d
                    .get(b"Page")
                    .ok()
                    .and_then(|p| p.as_i64().ok())
                    .unwrap_or(0)
                    .max(0) as usize;
                (page < pages).then_some((page, id, d))
            })
            .collect();
        self.import_from_source(&src, entries)
    }

    /// Import the comments of another PDF: the comments of each page go to the
    /// page with the same number (one undo step).
    pub fn import_comments_pdf(&mut self, data: &[u8]) -> Result<usize> {
        let other = Document::from_bytes(data.to_vec(), None, None)?;
        let pages = self.page_count().min(other.page_count());
        let mut entries = Vec::new();
        for page in 0..pages {
            for a in other.annotations(page).unwrap_or_default() {
                let Some(id) = a.id else { continue };
                if let Ok(d) = other.inner.get_dictionary(id) {
                    entries.push((page, id, d.clone()));
                }
            }
        }
        self.import_from_source(&other.inner, entries)
    }

    /// Copy comment dictionaries of another object model into the document.
    fn import_from_source(
        &mut self,
        src: &lopdf::Document,
        entries: Vec<(usize, ObjectId, Dictionary)>,
    ) -> Result<usize> {
        let entries: Vec<_> = entries
            .into_iter()
            .filter(|(_, _, d)| {
                name_str(d, b"Subtype")
                    .map(|s| SUBTYPES.iter().any(|(t, _)| *t == s))
                    .unwrap_or(false)
            })
            .collect();
        if entries.is_empty() {
            return Err(Error::Invalid(
                "the file contains no comments for the pages of this document".into(),
            ));
        }
        self.begin_edit();
        let mut map = BTreeMap::new();
        let queue: Vec<ObjectId> = entries.iter().flat_map(|(_, _, d)| content_refs(d)).collect();
        copy_objects(src, &mut self.inner, queue, &mut map);
        let mut items = Vec::new();
        for (page, id, d) in entries {
            let name = comment_name(src, &d, id);
            let in_reply_to = d
                .get(b"IRT")
                .and_then(|o| o.as_reference())
                .ok()
                .and_then(|t| src.get_dictionary(t).ok().map(|td| comment_name(src, td, t)));
            let popup = d
                .get(b"Popup")
                .and_then(|o| o.as_reference())
                .ok()
                .and_then(|p| src.get_dictionary(p).ok())
                .map(|p| {
                    let mut pd = p.clone();
                    for k in LOCAL_KEYS {
                        pd.remove(k);
                    }
                    let mut o = Object::Dictionary(pd);
                    remap_strict(&mut o, &map);
                    match o {
                        Object::Dictionary(pd) => pd,
                        _ => Dictionary::new(),
                    }
                });
            let mut dict = d.clone();
            for k in LOCAL_KEYS {
                dict.remove(k);
            }
            dict.set("NM", lopdf::text_string(&name));
            let mut o = Object::Dictionary(dict);
            remap_strict(&mut o, &map);
            let Object::Dictionary(dict) = o else { continue };
            items.push(Imported {
                page,
                dict,
                popup,
                name,
                in_reply_to,
                keep_appearance: true,
                file: None,
            });
        }
        let n = self.insert_imported(items)?;
        self.commit()?;
        Ok(n)
    }

    fn annots_array(&self, page_id: ObjectId) -> Vec<Object> {
        match self
            .inner
            .get_dictionary(page_id)
            .ok()
            .and_then(|p| p.get(b"Annots").ok())
        {
            Some(Object::Array(a)) => a.clone(),
            Some(Object::Reference(r)) => self
                .inner
                .get_object(*r)
                .ok()
                .and_then(|o| o.as_array().ok())
                .cloned()
                .unwrap_or_default(),
            _ => Vec::new(),
        }
    }

    fn set_annots_array(&mut self, page_id: ObjectId, arr: Vec<Object>) -> Result<()> {
        self.inner
            .get_dictionary_mut(page_id)?
            .set("Annots", Object::Array(arr));
        Ok(())
    }

    /// Add comments built by an import (inside an edit). Comments with the name
    /// of an existing comment replace it.
    fn insert_imported(&mut self, items: Vec<Imported>) -> Result<usize> {
        let page_ids = self.page_ids();
        let mut by_name: HashMap<String, (ObjectId, ObjectId)> = HashMap::new();
        for (p, page_id) in page_ids.iter().enumerate() {
            for a in self.annotations(p).unwrap_or_default() {
                let Some(id) = a.id else { continue };
                if let Ok(d) = self.inner.get_dictionary(id) {
                    if let Some(nm) = dict_text(&self.inner, d, b"NM").filter(|s| !s.trim().is_empty()) {
                        by_name.insert(nm, (*page_id, id));
                    }
                }
            }
        }
        let mut replies = Vec::new();
        let mut count = 0;
        for it in items {
            let Some(&page_id) = page_ids.get(it.page) else {
                continue;
            };
            let mut d = it.dict;
            d.set("P", page_id);
            d.set("NM", lopdf::text_string(&it.name));
            if let Some((file_name, data, flate)) = it.file {
                let mut sd = dictionary! { "Type" => "EmbeddedFile" };
                if flate {
                    sd.set("Filter", "FlateDecode");
                } else {
                    sd.set("Params", dictionary! { "Size" => data.len() as i64 });
                }
                let sid = self.inner.add_object(Stream::new(sd, data));
                let spec = self.inner.add_object(dictionary! {
                    "Type" => "Filespec",
                    "F" => Object::string_literal(file_name.clone()),
                    "UF" => lopdf::text_string(&file_name),
                    "EF" => dictionary! { "F" => sid, "UF" => sid },
                });
                d.set("FS", spec);
            }
            if !it.keep_appearance || !d.has(b"AP") {
                d.remove(b"AP");
                self.generate_appearance(&mut d)?;
            }
            let id = match by_name.get(&it.name).copied() {
                Some((old_page, old_id)) => {
                    // Update the comment in place; its popup is replaced as well.
                    let old_popup = self
                        .inner
                        .get_dictionary(old_id)
                        .ok()
                        .and_then(|o| o.get(b"Popup").ok())
                        .and_then(|p| p.as_reference().ok());
                    let mut arr = self.annots_array(old_page);
                    arr.retain(|o| {
                        let r = o.as_reference().ok();
                        let is_old_popup = old_popup.is_some() && r == old_popup;
                        let moved_away = old_page != page_id && r == Some(old_id);
                        !is_old_popup && !moved_away
                    });
                    self.set_annots_array(old_page, arr)?;
                    if let Some(p) = old_popup {
                        self.inner.objects.remove(&p);
                    }
                    self.inner.objects.insert(old_id, Object::Dictionary(d));
                    if old_page != page_id {
                        let mut arr = self.annots_array(page_id);
                        arr.push(Object::Reference(old_id));
                        self.set_annots_array(page_id, arr)?;
                    }
                    old_id
                }
                None => self.push_annotation(page_id, d)?,
            };
            if let Some(mut popup) = it.popup {
                popup.set("P", page_id);
                popup.set("Parent", id);
                let pid = self.push_annotation(page_id, popup)?;
                self.inner.get_dictionary_mut(id)?.set("Popup", pid);
            }
            by_name.insert(it.name, (page_id, id));
            if let Some(parent) = it.in_reply_to {
                replies.push((id, parent));
            }
            count += 1;
        }
        for (id, parent) in replies {
            let target = by_name.get(&parent).map(|(_, t)| *t).filter(|t| *t != id);
            let d = self.inner.get_dictionary_mut(id)?;
            match target {
                Some(t) => d.set("IRT", t),
                None => {
                    d.remove(b"IRT");
                    d.remove(b"RT");
                }
            }
        }
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sniffs_formats() {
        assert_eq!(CommentFormat::sniff(b"%FDF-1.2\n"), Some(CommentFormat::Fdf));
        assert_eq!(CommentFormat::sniff(b"  %PDF-1.7"), Some(CommentFormat::Pdf));
        assert_eq!(
            CommentFormat::sniff("\u{feff}<?xml version=\"1.0\"?><xfdf/>".as_bytes()),
            Some(CommentFormat::Xfdf)
        );
        assert_eq!(CommentFormat::sniff(b"hello"), None);
    }

    #[test]
    fn flags_and_colors_round_trip() {
        assert_eq!(flags_text(4 | 128), "print,locked");
        assert_eq!(parse_flags("print, locked"), 4 | 128);
        assert_eq!(hex_color([1.0, 0.5, 0.0]), "#FF8000");
        let c = parse_color("#ff8000").unwrap();
        assert!((c[1] - 128.0 / 255.0).abs() < 1e-6);
        assert_eq!(parse_numbers("1,2;3 4"), vec![1.0, 2.0, 3.0, 4.0]);
        assert_eq!(hex_decode(&hex_encode(b"\x00ab\xff")), b"\x00ab\xff".to_vec());
    }
}
