//! How many times each picture, form, pattern and Type 3 glyph procedure is
//! drawn in the document, and removing what nothing draws any more.
//!
//! Every content stream is read once for each set of resources it is drawn
//! with (told apart by the object holding them), and the counts flow from the
//! pages and the appearances of annotations through the drawings: a form drawn
//! twice draws what it draws twice. Patterns, soft masks and glyph procedures
//! repeat, so what they draw counts as drawn more than once. The masks of a
//! picture are drawn with it. A Type 3 font draws the procedures of the codes
//! shown with it; when those cannot be told, all of them (and the font is
//! noted).

use std::collections::{HashMap, HashSet};

use lopdf::{Dictionary, Object, ObjectId};

use super::scan::resource;
use crate::content_edit::joined;
use crate::content_lex::{name_of, operations};
use crate::page_text::probe::{stream_data, stream_resources};
use crate::textedit::{TextState, shown_bytes, shows_glyphs, text_ops_from};
use crate::util::deref;

/// How many times each picture, form, pattern and glyph procedure is drawn.
pub(super) struct Counts {
    pub map: HashMap<ObjectId, usize>,
    /// Type 3 fonts some of whose shown glyphs could not be told: every
    /// procedure of theirs counts as drawn.
    pub type3_unknown: HashSet<ObjectId>,
}

impl Counts {
    pub fn get(&self, id: &ObjectId) -> Option<&usize> {
        self.map.get(id)
    }
}

/// The resources of an object that has some: the key that tells them apart
/// (the resources object when it is indirect, else the holder) and the
/// dictionary.
fn own_resources(doc: &lopdf::Document, holder: ObjectId, d: &Dictionary) -> Option<(ObjectId, Dictionary)> {
    let r = d.get(b"Resources").ok()?;
    let res = deref(doc, r).as_dict().ok()?.clone();
    Some((r.as_reference().unwrap_or(holder), res))
}

/// The resources a page draws with (inherited from the page tree when it has
/// none) and their key.
fn page_resources_keyed(doc: &lopdf::Document, page: ObjectId) -> (ObjectId, Dictionary) {
    let mut cur = Some(page);
    for _ in 0..64 {
        let Some(id) = cur else { break };
        let Ok(d) = doc.get_dictionary(id) else { break };
        if d.has(b"Resources") {
            return own_resources(doc, id, d).unwrap_or((id, Dictionary::new()));
        }
        cur = d.get(b"Parent").ok().and_then(|p| p.as_reference().ok());
    }
    (page, Dictionary::new())
}

/// The name a standard encoding gives a letter, a digit or the space (the
/// names every standard encoding shares).
fn standard_name(c: u8) -> Option<Vec<u8>> {
    const DIGITS: [&str; 10] = [
        "zero", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine",
    ];
    match c {
        b'A'..=b'Z' | b'a'..=b'z' => Some(vec![c]),
        b'0'..=b'9' => Some(DIGITS[usize::from(c - b'0')].as_bytes().to_vec()),
        b' ' => Some(b"space".to_vec()),
        _ => None,
    }
}

/// The glyph names a Type 3 font gives `codes`: from its differences, else
/// the names of letters, digits and space in a standard base encoding. None
/// when a code has no name that can be told.
pub(super) fn type3_glyph_names(
    doc: &lopdf::Document,
    font: &Dictionary,
    codes: &HashSet<u8>,
) -> Option<HashSet<Vec<u8>>> {
    let (base, diffs): (Option<&[u8]>, Option<&Vec<Object>>) =
        match font.get(b"Encoding").ok().map(|e| deref(doc, e)) {
            Some(Object::Dictionary(d)) => (
                d.get(b"BaseEncoding")
                    .ok()
                    .and_then(|b| deref(doc, b).as_name().ok()),
                d.get(b"Differences")
                    .ok()
                    .and_then(|x| deref(doc, x).as_array().ok()),
            ),
            Some(Object::Name(n)) => (Some(n.as_slice()), None),
            None => (None, None),
            Some(_) => return None,
        };
    let mut named: HashMap<u8, Vec<u8>> = HashMap::new();
    let mut code: i64 = 0;
    for d in diffs.map(Vec::as_slice).unwrap_or_default() {
        match deref(doc, d) {
            Object::Integer(i) => code = *i,
            Object::Name(n) => {
                if let Ok(c) = u8::try_from(code) {
                    named.insert(c, n.clone());
                }
                code = code.saturating_add(1);
            }
            _ => {}
        }
    }
    let standard = matches!(
        base,
        None | Some(b"WinAnsiEncoding" | b"MacRomanEncoding" | b"StandardEncoding")
    );
    let mut names = HashSet::new();
    for c in codes {
        match named.get(c) {
            Some(n) => {
                names.insert(n.clone());
            }
            None if standard => {
                names.insert(standard_name(*c)?);
            }
            None => return None,
        }
    }
    Some(names)
}

/// The font a resource name stands for when it is a Type 3 font: its
/// object (when indirect) and dictionary.
pub(super) fn type3_font<'a>(
    doc: &'a lopdf::Document,
    res: &'a Dictionary,
    name: &[u8],
) -> Option<(Option<ObjectId>, &'a Dictionary)> {
    let fonts = deref(doc, res.get(b"Font").ok()?).as_dict().ok()?;
    let entry = fonts.get(name).ok()?;
    let font = deref(doc, entry).as_dict().ok()?;
    let type3 = font.get(b"Subtype").ok().and_then(|v| v.as_name().ok()) == Some(b"Type3".as_slice());
    type3.then_some((entry.as_reference().ok(), font))
}

/// A content stream in the context of the resources it draws with: the
/// stream and the key of those resources.
type Node = (ObjectId, ObjectId);

/// One drawing relation: an object drawn `mult` times by a stream, and its
/// own content when it has some.
struct Edge {
    id: ObjectId,
    node: Option<Node>,
    mult: u64,
}

struct Graph<'a> {
    doc: &'a lopdf::Document,
    /// Resources by key.
    owners: HashMap<ObjectId, Dictionary>,
    /// Forms that show text with the font of their caller.
    inherits: HashMap<ObjectId, bool>,
    type3_unknown: HashSet<ObjectId>,
}

type Agg = HashMap<(ObjectId, Option<Node>), u64>;

fn add(agg: &mut Agg, id: ObjectId, node: Option<Node>, factor: u64) {
    let e = agg.entry((id, node)).or_insert(0);
    *e = e.saturating_add(factor);
}

impl Graph<'_> {
    /// The node of a stream drawn by a stream with resources `owner` (None
    /// for pictures and what is not a stream).
    fn node_of(&mut self, id: ObjectId, owner: ObjectId) -> Option<Node> {
        let s = self.doc.get_object(id).and_then(Object::as_stream).ok()?;
        if s.dict.get(b"Subtype").ok().and_then(|v| v.as_name().ok()) == Some(b"Image".as_slice()) {
            return None;
        }
        Some(match own_resources(self.doc, id, &s.dict) {
            Some((key, r)) => {
                self.owners.entry(key).or_insert(r);
                (id, key)
            }
            None => (id, owner),
        })
    }

    /// The masks a picture is drawn with.
    fn masks_of(&self, id: ObjectId) -> Vec<ObjectId> {
        let Ok(s) = self.doc.get_object(id).and_then(Object::as_stream) else {
            return Vec::new();
        };
        [b"SMask".as_slice(), b"Mask"]
            .iter()
            .filter_map(|k| s.dict.get(k).ok()?.as_reference().ok())
            .collect()
    }

    /// The glyph procedures a Type 3 font draws for `codes` (all of them when
    /// the codes are not known or cannot be told).
    fn type3_edges(
        &mut self,
        res: &Dictionary,
        owner: ObjectId,
        name: &[u8],
        codes: Option<&HashSet<u8>>,
        agg: &mut Agg,
    ) {
        let doc = self.doc;
        let Some((fid, font)) = type3_font(doc, res, name) else {
            return;
        };
        let Some(procs) = font
            .get(b"CharProcs")
            .ok()
            .and_then(|p| deref(doc, p).as_dict().ok())
        else {
            return;
        };
        let names = codes.and_then(|c| type3_glyph_names(doc, font, c));
        if names.is_none() {
            if let Some(f) = fid {
                self.type3_unknown.insert(f);
            }
        }
        let first = procs.iter().find_map(|(_, v)| v.as_reference().ok());
        let key = match fid.or(first).and_then(|h| own_resources(doc, h, font)) {
            Some((key, r)) => {
                self.owners.entry(key).or_insert(r);
                key
            }
            // Glyphs of a font without resources draw with those of the stream.
            None => owner,
        };
        for (k, v) in procs.iter() {
            if names.as_ref().is_some_and(|n| !n.contains(k)) {
                continue;
            }
            if let Ok(pid) = v.as_reference() {
                add(agg, pid, Some((pid, key)), 2);
            }
        }
    }

    /// Whether a form shows text with the font set by what draws it (before
    /// setting one of its own), itself or through the forms it draws.
    fn inherits_font(&mut self, id: ObjectId, parent: &Dictionary, depth: usize) -> bool {
        if let Some(v) = self.inherits.get(&id) {
            return *v;
        }
        self.inherits.insert(id, false);
        let doc = self.doc;
        let Ok(s) = doc.get_object(id).and_then(Object::as_stream) else {
            return false;
        };
        let res = stream_resources(doc, s, parent);
        let data = stream_data(s);
        let mut set = false;
        let mut stack: Vec<bool> = Vec::new();
        let mut out = false;
        for op in operations(&data) {
            match op.operator {
                b"q" => stack.push(set),
                b"Q" => set = stack.pop().unwrap_or(set),
                b"Tf" => set = true,
                b"Tj" | b"TJ" | b"'" | b"\"" if !set => {
                    out = true;
                    break;
                }
                b"Do" if !set && depth < 12 => {
                    let child = op
                        .operands
                        .first()
                        .and_then(|r| name_of(&data, *r))
                        .and_then(|n| crate::page_text::probe::xobject_id(doc, &res, &n));
                    if let Some(c) = child {
                        if crate::page_text::probe::is_form(doc, c) && self.inherits_font(c, &res, depth + 1)
                        {
                            out = true;
                            break;
                        }
                    }
                }
                _ => {}
            }
        }
        self.inherits.insert(id, out);
        out
    }

    /// What a content stream drawn with the resources `owner` draws (each
    /// kind of drawing once, with how many times).
    fn edges_of(&mut self, data: &[u8], owner: ObjectId) -> Vec<Edge> {
        let doc = self.doc;
        let res = self.owners.get(&owner).cloned().unwrap_or_default();
        let mut agg: Agg = HashMap::new();
        let lookup = |cat: &[u8], name: Option<Vec<u8>>| -> Option<ObjectId> {
            res.get(cat)
                .ok()
                .and_then(|x| deref(doc, x).as_dict().ok())
                .and_then(|x| x.get(&name?).ok())
                .and_then(|v| v.as_reference().ok())
        };
        // The font set (for forms showing text with it).
        let mut font: Option<Vec<u8>> = None;
        let mut fonts: Vec<Option<Vec<u8>>> = Vec::new();
        for op in operations(data) {
            match op.operator {
                b"q" => fonts.push(font.clone()),
                b"Q" => {
                    if let Some(f) = fonts.pop() {
                        font = f;
                    }
                }
                b"Tf" => font = op.operands.first().and_then(|r| name_of(data, *r)),
                b"Do" => {
                    let Some(id) = lookup(b"XObject", op.operands.first().and_then(|r| name_of(data, *r)))
                    else {
                        continue;
                    };
                    let node = self.node_of(id, owner);
                    add(&mut agg, id, node, 1);
                    if node.is_none() {
                        for m in self.masks_of(id) {
                            add(&mut agg, m, None, 1);
                        }
                        continue;
                    }
                    // A form showing text with this stream's Type 3 font:
                    // which glyphs is not known here.
                    if let Some(f) = &font {
                        if type3_font(doc, &res, f).is_some() && self.inherits_font(id, &res, 0) {
                            self.type3_edges(&res, owner, f, None, &mut agg);
                        }
                    }
                }
                b"scn" | b"SCN" => {
                    if let Some(id) = lookup(b"Pattern", op.operands.last().and_then(|r| name_of(data, *r))) {
                        let node = self.node_of(id, owner);
                        add(&mut agg, id, node, 2);
                    }
                }
                b"gs" => {
                    let g = op
                        .operands
                        .first()
                        .and_then(|r| name_of(data, *r))
                        .and_then(|n| resource(doc, &res, b"ExtGState", &n))
                        .and_then(|d| d.as_dict().ok())
                        .and_then(|d| d.get(b"SMask").ok())
                        .map(|m| deref(doc, m))
                        .and_then(|m| m.as_dict().ok())
                        .and_then(|m| m.get(b"G").ok())
                        .and_then(|g| g.as_reference().ok());
                    if let Some(g) = g {
                        let node = self.node_of(g, owner);
                        add(&mut agg, g, node, 2);
                    }
                }
                _ => {}
            }
        }
        // Type 3 fonts draw the procedures of the codes shown.
        let has_type3 = res
            .get(b"Font")
            .ok()
            .and_then(|f| deref(doc, f).as_dict().ok())
            .is_some_and(|f| f.iter().any(|(k, _)| type3_font(doc, &res, k).is_some()));
        if has_type3 {
            let mut codes: HashMap<Vec<u8>, HashSet<u8>> = HashMap::new();
            for op in text_ops_from(data, &TextState::default()).0 {
                if !op.font.is_empty() && shows_glyphs(data, &op) {
                    codes
                        .entry(op.font.clone())
                        .or_default()
                        .extend(shown_bytes(data, &op));
                }
            }
            for (name, cs) in codes {
                self.type3_edges(&res, owner, &name, Some(&cs), &mut agg);
            }
        }
        agg.into_iter()
            .map(|((id, node), mult)| Edge { id, node, mult })
            .collect()
    }
}

/// How many times each picture, form, pattern and glyph procedure is drawn
/// in the document: by the pages, the forms they draw and the appearances of
/// annotations. What drawings that draw themselves reach counts as drawn
/// more than once.
pub(super) fn drawing_counts(doc: &lopdf::Document) -> Counts {
    let mut g = Graph {
        doc,
        owners: HashMap::new(),
        inherits: HashMap::new(),
        type3_unknown: HashSet::new(),
    };
    let mut roots: Vec<Edge> = Vec::new();
    // Resources of appearances that have none of their own.
    let none = (0, 0);
    g.owners.insert(none, Dictionary::new());
    for (_, pid) in doc.get_pages() {
        let (key, res) = page_resources_keyed(doc, pid);
        g.owners.entry(key).or_insert(res);
        let data = joined(doc, pid).data;
        roots.extend(g.edges_of(&data, key));
        for (_, ad) in doc
            .get_dictionary(pid)
            .map(|pd| crate::annots::annot_refs(doc, pd))
            .unwrap_or_default()
        {
            let Some(ap) = ad.get(b"AP").ok().and_then(|a| deref(doc, a).as_dict().ok()) else {
                continue;
            };
            for (_, v) in ap.iter() {
                let ids: Vec<ObjectId> = match v {
                    Object::Reference(r) => match doc.get_object(*r) {
                        Ok(Object::Dictionary(states)) => {
                            states.iter().filter_map(|(_, s)| s.as_reference().ok()).collect()
                        }
                        _ => vec![*r],
                    },
                    Object::Dictionary(states) => {
                        states.iter().filter_map(|(_, s)| s.as_reference().ok()).collect()
                    }
                    _ => Vec::new(),
                };
                for id in ids {
                    let node = g.node_of(id, none);
                    roots.push(Edge { id, node, mult: 1 });
                }
            }
        }
    }
    // Every stream drawn, read once per set of resources.
    let mut edges: HashMap<Node, Vec<Edge>> = HashMap::new();
    let mut todo: Vec<Node> = roots.iter().filter_map(|e| e.node).collect();
    while let Some(n) = todo.pop() {
        if edges.contains_key(&n) {
            continue;
        }
        let data = doc
            .get_object(n.0)
            .and_then(Object::as_stream)
            .map(stream_data)
            .unwrap_or_default();
        let out = g.edges_of(&data, n.1);
        todo.extend(out.iter().filter_map(|e| e.node));
        edges.insert(n, out);
    }
    // The counts flow from the pages through the drawings, in order.
    let mut indeg: HashMap<Node, usize> = edges.keys().map(|k| (*k, 0)).collect();
    for list in edges.values() {
        for e in list {
            if let Some(c) = e.node {
                *indeg.entry(c).or_default() += 1;
            }
        }
    }
    let mut counts: HashMap<ObjectId, u64> = HashMap::new();
    let mut node_count: HashMap<Node, u64> = HashMap::new();
    for e in &roots {
        let c = counts.entry(e.id).or_default();
        *c = c.saturating_add(e.mult);
        if let Some(n) = e.node {
            let c = node_count.entry(n).or_default();
            *c = c.saturating_add(e.mult);
        }
    }
    let mut queue: Vec<Node> = indeg.iter().filter(|(_, d)| **d == 0).map(|(k, _)| *k).collect();
    while let Some(n) = queue.pop() {
        let c = node_count.get(&n).copied().unwrap_or(0);
        for e in edges.get(&n).map(Vec::as_slice).unwrap_or_default() {
            let add = c.saturating_mul(e.mult);
            let v = counts.entry(e.id).or_default();
            *v = v.saturating_add(add);
            if let Some(ch) = e.node {
                let v = node_count.entry(ch).or_default();
                *v = v.saturating_add(add);
                if let Some(d) = indeg.get_mut(&ch) {
                    *d -= 1;
                    if *d == 0 {
                        queue.push(ch);
                    }
                }
            }
        }
    }
    // Drawings that draw themselves: what they reach is drawn, more than once.
    let mut stack: Vec<Node> = indeg.iter().filter(|(_, d)| **d > 0).map(|(k, _)| *k).collect();
    let mut seen: HashSet<Node> = HashSet::new();
    while let Some(n) = stack.pop() {
        if !seen.insert(n) {
            continue;
        }
        let v = counts.entry(n.0).or_default();
        *v = (*v).max(2);
        for e in edges.get(&n).map(Vec::as_slice).unwrap_or_default() {
            let v = counts.entry(e.id).or_default();
            *v = (*v).max(2);
            stack.extend(e.node);
        }
    }
    Counts {
        map: counts
            .into_iter()
            .map(|(k, v)| (k, usize::try_from(v).unwrap_or(usize::MAX)))
            .collect(),
        type3_unknown: g.type3_unknown,
    }
}

/// Remove `ids` from the document and every reference to them (dictionary
/// entries and array elements), so nothing keeps them.
pub(super) fn purge(doc: &mut lopdf::Document, ids: &HashSet<ObjectId>) {
    fn strip(o: &mut Object, ids: &HashSet<ObjectId>) {
        match o {
            Object::Dictionary(d) => strip_dict(d, ids),
            Object::Stream(s) => strip_dict(&mut s.dict, ids),
            Object::Array(a) => {
                a.retain(|x| !matches!(x, Object::Reference(r) if ids.contains(r)));
                for x in a.iter_mut() {
                    strip(x, ids);
                }
            }
            _ => {}
        }
    }
    fn strip_dict(d: &mut Dictionary, ids: &HashSet<ObjectId>) {
        let gone: Vec<Vec<u8>> = d
            .iter()
            .filter(|(_, v)| matches!(v, Object::Reference(r) if ids.contains(r)))
            .map(|(k, _)| k.clone())
            .collect();
        for k in gone {
            d.remove(&k);
        }
        for (_, v) in d.iter_mut() {
            strip(v, ids);
        }
    }
    if ids.is_empty() {
        return;
    }
    for id in ids {
        doc.objects.remove(id);
    }
    for o in doc.objects.values_mut() {
        strip(o, ids);
    }
    strip_dict(&mut doc.trailer, ids);
}

/// A name of a category of resources (pictures and forms, patterns) as a
/// page found it: the object listed, the object holding the dictionary of
/// the category, the keys leading to it from there, and the name.
#[derive(Debug, Clone)]
pub(super) struct Listing {
    pub id: ObjectId,
    at: ObjectId,
    path: Vec<Vec<u8>>,
    name: Vec<u8>,
}

/// Where the dictionaries of the categories of resources are: the object
/// holding the resources (`holder`, under `/Resources`) and its dictionary.
fn category_places(
    doc: &lopdf::Document,
    holder: ObjectId,
    d: &Dictionary,
    cat: &[u8],
) -> Option<(ObjectId, Vec<Vec<u8>>, Dictionary)> {
    let r = d.get(b"Resources").ok()?;
    let (at, mut path, res) = match r {
        Object::Reference(id) => (*id, Vec::new(), doc.get_dictionary(*id).ok()?),
        Object::Dictionary(x) => (holder, vec![b"Resources".to_vec()], x),
        _ => return None,
    };
    match res.get(cat).ok()? {
        Object::Reference(id) => Some((*id, Vec::new(), doc.get_dictionary(*id).ok()?.clone())),
        Object::Dictionary(x) => {
            path.push(cat.to_vec());
            Some((at, path, x.clone()))
        }
        _ => None,
    }
}

/// The pictures, forms and patterns listed in the resources a page uses
/// (its own or inherited from the page tree) and in those of the forms and
/// patterns they list, with where each name is.
pub(super) fn listed_by_page(doc: &lopdf::Document, page: ObjectId) -> Vec<Listing> {
    let mut out: Vec<Listing> = Vec::new();
    let mut seen: HashSet<ObjectId> = HashSet::new();
    // The object holding the page's resources.
    let mut holder = None;
    let mut cur = Some(page);
    for _ in 0..64 {
        let Some(id) = cur else { break };
        let Ok(d) = doc.get_dictionary(id) else { break };
        if d.has(b"Resources") {
            holder = Some(id);
            break;
        }
        cur = d.get(b"Parent").ok().and_then(|p| p.as_reference().ok());
    }
    let mut todo: Vec<ObjectId> = holder.into_iter().collect();
    while let Some(h) = todo.pop() {
        let dict = match doc.get_object(h) {
            Ok(Object::Dictionary(d)) => d,
            Ok(Object::Stream(s)) => &s.dict,
            _ => continue,
        };
        for cat in [b"XObject".as_slice(), b"Pattern"] {
            let Some((at, path, names)) = category_places(doc, h, dict, cat) else {
                continue;
            };
            for (name, v) in names.iter() {
                let Ok(id) = v.as_reference() else { continue };
                out.push(Listing {
                    id,
                    at,
                    path: path.clone(),
                    name: name.clone(),
                });
                if seen.insert(id) && doc.get_object(id).and_then(Object::as_stream).is_ok() {
                    todo.push(id);
                }
            }
        }
    }
    out
}

/// Remove the name of a listing from its dictionary (when it still names the
/// object listed).
pub(super) fn unlist(doc: &mut lopdf::Document, l: &Listing) {
    let mut d = match doc.get_object_mut(l.at) {
        Ok(Object::Dictionary(d)) => d,
        Ok(Object::Stream(s)) => &mut s.dict,
        _ => return,
    };
    for k in &l.path {
        d = match d.get_mut(k) {
            Ok(Object::Dictionary(x)) => x,
            _ => return,
        };
    }
    if d.get(&l.name).ok().and_then(|v| v.as_reference().ok()) == Some(l.id) {
        d.remove(&l.name);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::dictionary;

    #[test]
    fn type3_names_come_from_differences_and_standard_letters() {
        let doc = lopdf::Document::with_version("1.7");
        let font = |enc: Object| dictionary! { "Subtype" => "Type3", "Encoding" => enc };
        let diffs = vec![
            97.into(),
            Object::Name(b"g1".to_vec()),
            Object::Name(b"g2".to_vec()),
        ];
        let with_base = font(Object::Dictionary(dictionary! {
            "BaseEncoding" => "WinAnsiEncoding", "Differences" => diffs.clone(),
        }));
        let codes: HashSet<u8> = [98u8, b'z', b'7'].into_iter().collect();
        let names = type3_glyph_names(&doc, &with_base, &codes).unwrap();
        let want: HashSet<Vec<u8>> = [b"g2".to_vec(), b"z".to_vec(), b"seven".to_vec()]
            .into_iter()
            .collect();
        assert_eq!(names, want);
        // A code no standard name is known for cannot be told.
        assert!(type3_glyph_names(&doc, &with_base, &HashSet::from(*b"%")).is_none());
        let mac_expert = font(Object::Dictionary(dictionary! {
            "BaseEncoding" => "MacExpertEncoding", "Differences" => diffs,
        }));
        assert!(type3_glyph_names(&doc, &mac_expert, &HashSet::from(*b"z")).is_none());
        assert!(type3_glyph_names(&doc, &mac_expert, &[97u8].into_iter().collect()).is_some());
    }
}
