//! Reading the structure tree, its role map and class map, and the marked content of the pages.

use super::*;

// ----------------------------------------------------------------------
// Reading the structure tree
// ----------------------------------------------------------------------

pub(super) struct Reader<'a> {
    pub(super) doc: &'a lopdf::Document,
    pub(super) pages: HashMap<ObjectId, usize>,
    pub(super) annots: HashMap<ObjectId, (usize, usize, String)>,
    pub(super) roles: HashMap<String, String>,
    pub(super) seen: HashSet<ObjectId>,
}

pub(super) fn name_value(d: &Dictionary, key: &[u8]) -> Option<String> {
    d.get(key)
        .ok()
        .and_then(|o| o.as_name().ok())
        .map(|n| String::from_utf8_lossy(n).to_string())
}

pub(super) fn role_of(roles: &HashMap<String, String>, tag: &str) -> String {
    let mut t = tag.to_string();
    for _ in 0..16 {
        if is_standard(&t) {
            return t;
        }
        match roles.get(&t) {
            Some(n) if *n != t => t = n.clone(),
            _ => break,
        }
    }
    if is_standard(&t) { t } else { tag.to_string() }
}

impl Reader<'_> {
    pub(super) fn page_of(&self, d: &Dictionary) -> Option<usize> {
        d.get(b"Pg")
            .ok()
            .and_then(|p| p.as_reference().ok())
            .and_then(|p| self.pages.get(&p).copied())
    }

    pub(super) fn elem(&mut self, id: ObjectId, page: Option<usize>, depth: usize) -> Option<StructElem> {
        if depth > 256 || self.seen.len() > MAX_ELEMENTS || !self.seen.insert(id) {
            return None;
        }
        let doc = self.doc;
        let d = doc.get_dictionary(id).ok()?;
        let tag = name_value(d, b"S").unwrap_or_default();
        let pg = self.page_of(d).or(page);
        let text = |key: &[u8]| dict_text(doc, d, key).unwrap_or_default();
        let mut attributes = Vec::new();
        if let Ok(a) = d.get(b"A") {
            let list: Vec<&Object> = match deref(doc, a) {
                Object::Array(arr) => arr.iter().collect(),
                o => vec![o],
            };
            for o in list {
                if let Ok(ad) = deref(doc, o).as_dict() {
                    let owner = name_value(ad, b"O").unwrap_or_default();
                    for (k, _) in ad.iter() {
                        if k != b"O" {
                            attributes.push(format!("{owner}/{}", String::from_utf8_lossy(k)));
                        }
                    }
                }
            }
        }
        let mut e = StructElem {
            id,
            role: role_of(&self.roles, &tag),
            tag,
            title: text(b"T"),
            alt: text(b"Alt"),
            actual_text: text(b"ActualText"),
            lang: text(b"Lang"),
            page: pg,
            attributes,
            kids: Vec::new(),
        };
        if let Ok(k) = d.get(b"K") {
            for o in kid_objects(doc, k) {
                let kid = self.kid(&o, pg, depth);
                e.kids.push(kid);
            }
        }
        Some(e)
    }

    /// One kid: always one entry for each object of the `/K` list.
    pub(super) fn kid(&mut self, o: &Object, pg: Option<usize>, depth: usize) -> StructKid {
        let doc = self.doc;
        let mcid = |n: i64| match pg {
            Some(p) => StructKid::Content { page: p, mcid: n },
            None => StructKid::Other(format!("MCID {n}")),
        };
        match o {
            Object::Integer(n) => mcid(*n),
            Object::Reference(r) => match doc.get_object(*r) {
                Ok(Object::Integer(n)) => mcid(*n),
                Ok(Object::Dictionary(d)) => {
                    if d.has(b"S") && !d.has_type(b"MCR") && !d.has_type(b"OBJR") {
                        match self.elem(*r, pg, depth + 1) {
                            Some(e) => StructKid::Elem(e),
                            None => StructKid::Other(name_value(d, b"S").unwrap_or_default()),
                        }
                    } else {
                        self.dict_kid(d, pg)
                    }
                }
                _ => StructKid::Other(String::new()),
            },
            Object::Dictionary(d) => self.dict_kid(d, pg),
            _ => StructKid::Other(String::new()),
        }
    }

    pub(super) fn dict_kid(&self, d: &Dictionary, pg: Option<usize>) -> StructKid {
        if d.has_type(b"MCR") {
            let page = self.page_of(d).or(pg);
            let mcid = d.get(b"MCID").ok().and_then(|m| m.as_i64().ok());
            return match (page, mcid, d.has(b"Stm")) {
                (Some(page), Some(mcid), false) => StructKid::Content { page, mcid },
                (_, Some(m), _) => StructKid::Other(format!("MCID {m}")),
                _ => StructKid::Other(String::new()),
            };
        }
        if d.has_type(b"OBJR") {
            if let Some((page, index, subtype)) = d
                .get(b"Obj")
                .ok()
                .and_then(|o| o.as_reference().ok())
                .and_then(|o| self.annots.get(&o))
            {
                return StructKid::Annot {
                    page: *page,
                    index: *index,
                    subtype: subtype.clone(),
                };
            }
            return StructKid::Other("OBJR".into());
        }
        StructKid::Other(name_value(d, b"S").unwrap_or_default())
    }
}

/// The objects of a `/K` entry: one object, an array, or a reference to an array.
pub(super) fn kid_objects(doc: &lopdf::Document, k: &Object) -> Vec<Object> {
    match k {
        Object::Array(a) => a.clone(),
        Object::Reference(r) => match doc.get_object(*r) {
            Ok(Object::Array(a)) => a.clone(),
            Ok(_) => vec![k.clone()],
            Err(_) => Vec::new(),
        },
        Object::Null => Vec::new(),
        o => vec![o.clone()],
    }
}

/// Attributes text: `Key=Value` pairs separated by spaces, values as numbers or names.
pub(super) fn attrs_to_text(doc: &lopdf::Document, o: &Object) -> String {
    pub(super) fn value(doc: &lopdf::Document, v: &Object) -> String {
        match deref(doc, v) {
            Object::Name(n) => String::from_utf8_lossy(n).to_string(),
            Object::Integer(i) => i.to_string(),
            Object::Real(r) => r.to_string(),
            Object::Boolean(b) => b.to_string(),
            Object::Array(a) => format!(
                "[{}]",
                a.iter().map(|x| value(doc, x)).collect::<Vec<_>>().join(" ")
            ),
            Object::String(..) => crate::util::text_of(doc, v).unwrap_or_default(),
            _ => String::new(),
        }
    }
    let dicts: Vec<&Object> = match deref(doc, o) {
        Object::Array(a) => a.iter().collect(),
        other => vec![other],
    };
    dicts
        .iter()
        .filter_map(|d| deref(doc, d).as_dict().ok())
        .map(|d| {
            d.iter()
                .map(|(k, v)| format!("{}={}", String::from_utf8_lossy(k), value(doc, v)))
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect::<Vec<_>>()
        .join("; ")
}

pub(super) fn parse_value(v: &str) -> Object {
    let v = v.trim();
    if let Some(inner) = v.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
        return Object::Array(inner.split_whitespace().map(parse_value).collect());
    }
    if let Ok(i) = v.parse::<i64>() {
        return Object::Integer(i);
    }
    if let Ok(f) = v.parse::<f32>() {
        return Object::Real(f);
    }
    match v {
        "true" => Object::Boolean(true),
        "false" => Object::Boolean(false),
        _ => Object::Name(v.trim_start_matches('/').as_bytes().to_vec()),
    }
}

/// Words separated by spaces, `[...]` kept together.
pub(super) fn attr_words(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut depth = 0usize;
    for c in text.chars() {
        match c {
            '[' => depth += 1,
            ']' => depth = depth.saturating_sub(1),
            _ => {}
        }
        if c.is_whitespace() && depth == 0 {
            if !cur.is_empty() {
                out.push(std::mem::take(&mut cur));
            }
        } else {
            cur.push(c);
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

pub(super) fn text_to_attrs(text: &str) -> Object {
    let dicts: Vec<Object> = text
        .split(';')
        .filter(|s| !s.trim().is_empty())
        .map(|part| {
            let mut d = Dictionary::new();
            for pair in attr_words(part) {
                if let Some((k, v)) = pair.split_once('=') {
                    d.set(k.trim_start_matches('/').as_bytes().to_vec(), parse_value(v));
                }
            }
            Object::Dictionary(d)
        })
        .collect();
    if dicts.len() == 1 {
        dicts.into_iter().next().unwrap_or(Object::Null)
    } else {
        Object::Array(dicts)
    }
}

impl Document {
    // ------------------------------------------------------------------
    // Reading
    // ------------------------------------------------------------------

    /// The document is marked as tagged (`/MarkInfo /Marked true`).
    pub fn is_tagged(&self) -> bool {
        self.info().tagged
    }

    /// The document has a structure tree.
    pub fn has_structure(&self) -> bool {
        self.inner
            .catalog()
            .map(|c| c.has(b"StructTreeRoot"))
            .unwrap_or(false)
    }

    pub(super) fn struct_root_object(&self) -> Option<(Option<ObjectId>, &Dictionary)> {
        let c = self.inner.catalog().ok()?;
        match c.get(b"StructTreeRoot").ok()? {
            Object::Reference(r) => self.inner.get_dictionary(*r).ok().map(|d| (Some(*r), d)),
            Object::Dictionary(d) => Some((None, d)),
            _ => None,
        }
    }

    pub(super) fn roles(&self) -> HashMap<String, String> {
        self.role_map().into_iter().collect()
    }

    /// The role map: tags and the types they stand for.
    pub fn role_map(&self) -> Vec<(String, String)> {
        let Some((_, root)) = self.struct_root_object() else {
            return Vec::new();
        };
        let Some(map) = root
            .get(b"RoleMap")
            .ok()
            .and_then(|m| deref(&self.inner, m).as_dict().ok())
        else {
            return Vec::new();
        };
        map.iter()
            .filter_map(|(k, v)| {
                deref(&self.inner, v).as_name().ok().map(|n| {
                    (
                        String::from_utf8_lossy(k).to_string(),
                        String::from_utf8_lossy(n).to_string(),
                    )
                })
            })
            .collect()
    }

    /// The class map: class names and their attributes as `Key=Value` text.
    pub fn class_map(&self) -> Vec<(String, String)> {
        let Some((_, root)) = self.struct_root_object() else {
            return Vec::new();
        };
        let Some(map) = root
            .get(b"ClassMap")
            .ok()
            .and_then(|m| deref(&self.inner, m).as_dict().ok())
        else {
            return Vec::new();
        };
        map.iter()
            .map(|(k, v)| {
                (
                    String::from_utf8_lossy(k).to_string(),
                    attrs_to_text(&self.inner, v),
                )
            })
            .collect()
    }

    /// The structure tree, its root with the tag `StructTreeRoot`.
    pub fn structure(&self) -> Option<StructElem> {
        let (root_id, root) = self.struct_root_object()?;
        let doc = &self.inner;
        let page_ids = self.page_ids();
        let mut annots = HashMap::new();
        for (p, pid) in page_ids.iter().enumerate() {
            if let Ok(pd) = doc.get_dictionary(*pid) {
                for (i, (aid, d)) in crate::annots::annot_refs(doc, pd).into_iter().enumerate() {
                    if let Some(aid) = aid {
                        annots.insert(aid, (p, i, name_value(&d, b"Subtype").unwrap_or_default()));
                    }
                }
            }
        }
        let mut reader = Reader {
            doc,
            pages: page_ids.iter().enumerate().map(|(i, id)| (*id, i)).collect(),
            annots,
            roles: self.roles(),
            seen: HashSet::new(),
        };
        let id = root_id.unwrap_or((0, 0));
        reader.seen.insert(id);
        let mut e = StructElem {
            id,
            tag: "StructTreeRoot".into(),
            role: String::new(),
            title: String::new(),
            alt: String::new(),
            actual_text: String::new(),
            lang: String::new(),
            page: None,
            attributes: Vec::new(),
            kids: Vec::new(),
        };
        if let Ok(k) = root.get(b"K") {
            for o in kid_objects(doc, k) {
                let kid = reader.kid(&o, None, 0);
                e.kids.push(kid);
            }
        }
        Some(e)
    }

    /// The structure tree for views, read once per revision. Editing code
    /// reads [Document::structure] instead: it sees changes not committed yet.
    pub fn structure_view(&self) -> Option<Arc<StructElem>> {
        let mut cache = self.struct_cache.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((rev, tree)) = cache.as_ref() {
            if *rev == self.revision() {
                return tree.clone();
            }
        }
        let tree = self.structure().map(Arc::new);
        *cache = Some((self.revision(), tree.clone()));
        tree
    }

    /// Marked content of a page and where it is drawn (cached per revision).
    pub fn marked_content(&self, page: usize) -> Arc<Vec<MarkedItem>> {
        self.marked_content_pages(&[page])
            .remove(&page)
            .unwrap_or_default()
    }

    /// Marked content of several pages.
    pub fn marked_content_pages(&self, pages: &[usize]) -> HashMap<usize, Arc<Vec<MarkedItem>>> {
        let mut cache = self.marked_cache.lock().unwrap_or_else(|e| e.into_inner());
        if cache.as_ref().map(|(rev, _)| *rev) != Some(self.revision()) {
            *cache = Some((self.revision(), HashMap::new()));
        }
        let Some((_, map)) = cache.as_mut() else {
            return HashMap::new();
        };
        let missing: Vec<usize> = pages.iter().copied().filter(|p| !map.contains_key(p)).collect();
        if !missing.is_empty() {
            if let Ok(found) = measure(self.bytes(), &missing) {
                for (p, items) in found {
                    map.insert(p, Arc::new(items));
                }
            }
        }
        pages
            .iter()
            .filter_map(|p| map.get(p).map(|items| (*p, items.clone())))
            .collect()
    }

    /// Text of an element: its actual text, or the text of its content.
    pub fn struct_text(&self, elem: &StructElem) -> String {
        if !elem.actual_text.trim().is_empty() {
            return elem.actual_text.trim().to_string();
        }
        let content = elem.content();
        let pages: Vec<usize> = content
            .iter()
            .map(|c| c.0)
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect();
        let marked = self.marked_content_pages(&pages);
        let mut parts = Vec::new();
        for (p, m) in content {
            if let Some(item) = marked
                .get(&p)
                .and_then(|items| items.iter().find(|i| i.mcid == Some(m)))
            {
                if !item.text.is_empty() {
                    parts.push(item.text.clone());
                }
            }
        }
        parts.join(" ")
    }

    /// Bounds of an element's content on a page (display space).
    pub fn struct_rect(&self, elem: &StructElem, page: usize) -> Option<[f32; 4]> {
        let marked = self.marked_content(page);
        let mut rect = None;
        for (p, m) in elem.content() {
            if p != page {
                continue;
            }
            if let Some(item) = marked.iter().find(|i| i.mcid == Some(m)) {
                rect = geom::union_opt(rect, item.rect);
            }
        }
        if let Ok(annots) = self.annotations(page) {
            let size = self.page_size(page).ok();
            for (p, i) in elem.annotations() {
                if p == page {
                    if let (Some(a), Some(size)) = (annots.get(i), size.as_ref()) {
                        rect = geom::union_opt(rect, crate::marks::pdf_rect_to_display(size, a.rect));
                    }
                }
            }
        }
        rect
    }

    /// The element that owns a marked content sequence.
    pub fn struct_for_content(&self, page: usize, mcid: i64) -> Option<ObjectId> {
        let tree = self.structure()?;
        let mut found = None;
        tree.walk(&mut |e, _| {
            if found.is_none()
                && e.kids
                    .iter()
                    .any(|k| matches!(k, StructKid::Content { page: p, mcid: m } if *p == page && *m == mcid))
            {
                found = Some(e.id);
            }
        });
        found
    }

    /// The element that owns an annotation.
    pub fn struct_for_annotation(&self, page: usize, index: usize) -> Option<ObjectId> {
        let tree = self.structure()?;
        let mut found = None;
        tree.walk(&mut |e, _| {
            if found.is_none()
                && e.kids.iter().any(
                    |k| matches!(k, StructKid::Annot { page: p, index: i, .. } if *p == page && *i == index),
                )
            {
                found = Some(e.id);
            }
        });
        found
    }

    /// The deepest element with content inside an area of a page.
    pub fn struct_at(&self, page: usize, rect: [f32; 4]) -> Option<ObjectId> {
        let marked = self.marked_content(page);
        let best = marked
            .iter()
            .filter(|i| i.mcid.is_some() && geom::intersects(i.rect, rect))
            .min_by(|a, b| {
                let area = |r: [f32; 4]| (r[2] - r[0]) * (r[3] - r[1]);
                area(a.rect).total_cmp(&area(b.rect))
            })?;
        self.struct_for_content(page, best.mcid?)
    }
}
