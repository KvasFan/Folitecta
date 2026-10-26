//! Document tools: splitting pages at cut lines, removing what lies outside
//! the crop box, auditing the space used, recompressing images, leaving PDF/A,
//! document actions, and plain text from rich text.

use std::collections::{HashMap, HashSet};

use lopdf::{Dictionary, Object, ObjectId, Stream, dictionary};

use crate::document::Document;
use crate::error::{Error, Result};
use crate::textops::display_rect_to_pdf;
use crate::util::{deref, fmt_num, text_of};

/// Space used by one kind of content.
#[derive(Debug, Clone, PartialEq)]
pub struct SpaceItem {
    /// `content`, `images`, `forms`, `fonts`, `annotations`, `fields`,
    /// `bookmarks`, `structure`, `attachments`, `metadata`, `colors`, `other`.
    pub category: &'static str,
    pub bytes: u64,
    pub objects: usize,
}

/// Document-level script events: `/AA` key and a readable name.
pub const DOCUMENT_EVENTS: &[(&str, &str)] = &[
    ("Open", "When the document opens"),
    ("WC", "Before the document closes"),
    ("WS", "Before the document is saved"),
    ("DS", "After the document is saved"),
    ("WP", "Before the document is printed"),
    ("DP", "After the document is printed"),
];

/// Approximate size of an object once written.
fn approx_size(o: &Object) -> u64 {
    match o {
        Object::Null => 4,
        Object::Boolean(_) => 5,
        Object::Integer(i) => i.to_string().len() as u64,
        Object::Real(_) => 6,
        Object::Name(n) => n.len() as u64 + 1,
        Object::String(s, _) => s.len() as u64 + 2,
        Object::Array(a) => 2 + a.iter().map(|x| approx_size(x) + 1).sum::<u64>(),
        Object::Dictionary(d) => {
            4 + d
                .iter()
                .map(|(k, v)| k.len() as u64 + 2 + approx_size(v))
                .sum::<u64>()
        }
        Object::Stream(s) => 20 + s.content.len() as u64 + approx_size(&Object::Dictionary(s.dict.clone())),
        Object::Reference(_) => 8,
    }
}

/// Plain text of an RTF document (formatting dropped, paragraphs kept).
pub fn rtf_to_text(rtf: &str) -> String {
    let chars: Vec<char> = rtf.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    // Groups whose text is not shown (font tables, colours, pictures...).
    let mut skip_depth: Vec<bool> = vec![false];
    let skipped = [
        "fonttbl",
        "colortbl",
        "stylesheet",
        "info",
        "pict",
        "object",
        "header",
        "footer",
        "*",
    ];
    let mut uc_skip = 1usize;
    let mut pending_skip = 0usize;
    while i < chars.len() {
        let c = chars[i];
        let skipping = *skip_depth.last().unwrap_or(&false);
        match c {
            '{' => {
                skip_depth.push(skipping);
                i += 1;
            }
            '}' => {
                if skip_depth.len() > 1 {
                    skip_depth.pop();
                }
                i += 1;
            }
            '\\' => {
                i += 1;
                let Some(&n) = chars.get(i) else { break };
                if n.is_ascii_alphabetic() {
                    let start = i;
                    while i < chars.len() && chars[i].is_ascii_alphabetic() {
                        i += 1;
                    }
                    let word: String = chars[start..i].iter().collect();
                    let num_start = i;
                    if i < chars.len() && (chars[i] == '-' || chars[i].is_ascii_digit()) {
                        i += 1;
                        while i < chars.len() && chars[i].is_ascii_digit() {
                            i += 1;
                        }
                    }
                    let num: Option<i64> = chars[num_start..i].iter().collect::<String>().parse().ok();
                    if i < chars.len() && chars[i] == ' ' {
                        i += 1;
                    }
                    if skipped.contains(&word.as_str()) {
                        if let Some(last) = skip_depth.last_mut() {
                            *last = true;
                        }
                        continue;
                    }
                    if skipping {
                        continue;
                    }
                    match word.as_str() {
                        "par" | "line" | "sect" | "page" => out.push('\n'),
                        "tab" => out.push('\t'),
                        "uc" => uc_skip = num.unwrap_or(1).max(0) as usize,
                        "u" => {
                            if let Some(v) = num {
                                let v = if v < 0 { v + 65536 } else { v };
                                if let Some(ch) = char::from_u32(v as u32) {
                                    out.push(ch);
                                }
                                pending_skip = uc_skip;
                            }
                        }
                        "emdash" => out.push('—'),
                        "endash" => out.push('–'),
                        "bullet" => out.push('•'),
                        "lquote" => out.push('‘'),
                        "rquote" => out.push('’'),
                        "ldblquote" => out.push('“'),
                        "rdblquote" => out.push('”'),
                        _ => {}
                    }
                } else if n == '\'' {
                    // \'hh: a byte in the document code page (taken as Windows-1252).
                    let hex: String = chars
                        .get(i + 1..i + 3)
                        .map(|h| h.iter().collect())
                        .unwrap_or_default();
                    i += 3;
                    if pending_skip > 0 {
                        pending_skip -= 1;
                        continue;
                    }
                    if !skipping {
                        if let Ok(b) = u8::from_str_radix(&hex, 16) {
                            out.push(
                                crate::util::decode_pdf_text(&[b])
                                    .chars()
                                    .next()
                                    .unwrap_or(b as char),
                            );
                        }
                    }
                } else {
                    if !skipping {
                        match n {
                            '~' => out.push('\u{a0}'),
                            '-' | '*' => {}
                            '_' => out.push('-'),
                            '\n' | '\r' => out.push('\n'),
                            other => out.push(other),
                        }
                    }
                    i += 1;
                }
            }
            '\r' | '\n' => i += 1,
            _ => {
                if pending_skip > 0 {
                    pending_skip -= 1;
                } else if !skipping {
                    out.push(c);
                }
                i += 1;
            }
        }
    }
    out.trim_end().to_string()
}

pub(crate) fn stream_bytes(s: &Stream) -> Option<Vec<u8>> {
    if s.dict.has(b"Filter") {
        s.decompressed_content().ok()
    } else {
        Some(s.content.clone())
    }
}

impl Document {
    /// Split pages at vertical cuts `xs` and horizontal cuts `ys` (display
    /// points from the top-left corner); the parts follow in reading order.
    pub fn split_pages_at(&mut self, pages: &[usize], xs: &[f32], ys: &[f32]) -> Result<usize> {
        self.check(pages)?;
        let ids = self.page_ids();
        let mut made = 0;
        let mut plan: Vec<(ObjectId, Vec<[f32; 4]>)> = Vec::new();
        for (i, id) in ids.iter().enumerate() {
            if !pages.contains(&i) {
                continue;
            }
            let size = self.page_size_of(*id);
            let cuts = |list: &[f32], max: f32| {
                let mut v: Vec<f32> = list
                    .iter()
                    .copied()
                    .filter(|c| *c > 1.0 && *c < max - 1.0)
                    .collect();
                v.sort_by(|a, b| a.total_cmp(b));
                v.dedup_by(|a, b| (*a - *b).abs() < 1.0);
                let mut edges = vec![0.0];
                edges.extend(v);
                edges.push(max);
                edges
            };
            let (cx, cy) = (cuts(xs, size.width), cuts(ys, size.height));
            if cx.len() == 2 && cy.len() == 2 {
                continue;
            }
            let mut parts = Vec::new();
            for r in cy.windows(2) {
                for c in cx.windows(2) {
                    parts.push(display_rect_to_pdf(&size, [c[0], r[0], c[1], r[1]]));
                }
            }
            made += parts.len() - 1;
            plan.push((*id, parts));
        }
        if plan.is_empty() {
            return Err(Error::Invalid("no cut line crosses the pages".into()));
        }
        self.begin_edit();
        self.inherit_attributes(&ids);
        let mut order = Vec::new();
        for id in &ids {
            let Some((_, parts)) = plan.iter().find(|(p, _)| p == id) else {
                order.push(*id);
                continue;
            };
            for (k, rect) in parts.iter().enumerate() {
                let mut dict = self.inner.get_dictionary(*id)?.clone();
                dict.set(
                    "CropBox",
                    Object::Array(rect.iter().map(|v| (*v).into()).collect()),
                );
                if k > 0 {
                    dict.remove(b"Annots");
                }
                let nid = if k == 0 {
                    *self.inner.get_dictionary_mut(*id)? = dict;
                    *id
                } else {
                    self.inner.add_object(dict)
                };
                order.push(nid);
            }
        }
        self.set_page_order(order)?;
        self.commit()?;
        Ok(made)
    }

    /// Make the crop box the page: the media box shrinks to it, the content
    /// is clipped to it and comments wholly outside it are removed. Returns
    /// the number of pages changed.
    pub fn remove_cropped_content(&mut self, pages: &[usize]) -> Result<usize> {
        self.check(pages)?;
        let ids = self.page_ids();
        let mut todo = Vec::new();
        for &p in pages {
            let size = self.page_size_of(ids[p]);
            if size.crop_box != size.media_box {
                todo.push((p, ids[p], size.crop_box));
            }
        }
        if todo.is_empty() {
            return Ok(0);
        }
        self.begin_edit();
        self.inherit_attributes(&ids);
        for (p, id, c) in &todo {
            let f = fmt_num;
            let clip = format!(
                "q {} {} {} {} re W n\n",
                f(c[0]),
                f(c[1]),
                f(c[2] - c[0]),
                f(c[3] - c[1])
            );
            self.prepend_page_content(*id, clip.into_bytes(), None)?;
            self.append_page_content(*id, b"\nQ\n".to_vec(), None)?;
            let bx = Object::Array(c.iter().map(|v| (*v).into()).collect());
            let d = self.inner.get_dictionary_mut(*id)?;
            d.set("MediaBox", bx);
            d.remove(b"CropBox");
            d.remove(b"BleedBox");
            d.remove(b"TrimBox");
            d.remove(b"ArtBox");
            // Comments that cannot be seen any more.
            let outside: HashSet<ObjectId> = self
                .annotations(*p)
                .unwrap_or_default()
                .into_iter()
                .filter(|a| a.rect[2] < c[0] || a.rect[0] > c[2] || a.rect[3] < c[1] || a.rect[1] > c[3])
                .filter_map(|a| a.id)
                .collect();
            if !outside.is_empty() {
                self.remove_annotations(*id, &outside);
            }
        }
        self.commit()?;
        Ok(todo.len())
    }

    /// Space used by each kind of content (approximate sizes of the objects;
    /// streams are counted as stored).
    pub fn space_audit(&self) -> Vec<SpaceItem> {
        let doc = &self.inner;
        let mut cat: HashMap<ObjectId, &'static str> = HashMap::new();
        let mark = |id: ObjectId, c: &'static str, cat: &mut HashMap<ObjectId, &'static str>| {
            cat.entry(id).or_insert(c);
        };
        // Page content streams.
        for pid in self.page_ids() {
            for cid in doc.get_page_contents(pid) {
                mark(cid, "content", &mut cat);
            }
        }
        if let Ok(catalog) = doc.catalog() {
            for (key, c) in [
                (b"Outlines".as_slice(), "bookmarks"),
                (b"StructTreeRoot", "structure"),
                (b"AcroForm", "fields"),
                (b"Metadata", "metadata"),
            ] {
                if let Ok(Object::Reference(r)) = catalog.get(key) {
                    mark(*r, c, &mut cat);
                }
            }
        }
        for (id, obj) in &doc.objects {
            let c = match obj {
                Object::Stream(s) => {
                    let sub = s.dict.get(b"Subtype").ok().and_then(|x| x.as_name().ok());
                    let typ = s.dict.get(b"Type").ok().and_then(|x| x.as_name().ok());
                    match (typ, sub) {
                        (_, Some(b"Image")) => "images",
                        (_, Some(b"Form")) => "forms",
                        (Some(b"EmbeddedFile"), _) => "attachments",
                        (Some(b"Metadata"), _) => "metadata",
                        _ if s.dict.has(b"Length1")
                            || s.dict.has(b"Length2")
                            || matches!(
                                sub,
                                Some(b"Type1C") | Some(b"CIDFontType0C") | Some(b"OpenType")
                            ) =>
                        {
                            "fonts"
                        }
                        _ if s.dict.has(b"N") && s.dict.has(b"Alternate")
                            || s.dict.get(b"N").is_ok() && !s.dict.has(b"Type") && s.dict.has(b"Range") =>
                        {
                            "colors"
                        }
                        _ => "other",
                    }
                }
                Object::Dictionary(d) => match d.get(b"Type").ok().and_then(|x| x.as_name().ok()) {
                    Some(b"Font") | Some(b"FontDescriptor") => "fonts",
                    Some(b"Annot") => {
                        if d.get(b"Subtype").ok().and_then(|x| x.as_name().ok()) == Some(b"Widget") {
                            "fields"
                        } else {
                            "annotations"
                        }
                    }
                    Some(b"Outlines") => "bookmarks",
                    Some(b"StructElem") | Some(b"StructTreeRoot") | Some(b"MarkInfo") => "structure",
                    Some(b"Filespec") => "attachments",
                    _ if d.has(b"FT") || d.has(b"Kids") && d.has(b"T") => "fields",
                    _ if d.has(b"Title")
                        && (d.has(b"Parent") || d.has(b"Dest") || d.has(b"A"))
                        && d.has(b"Prev") | d.has(b"Next") | d.has(b"First") =>
                    {
                        "bookmarks"
                    }
                    _ => "other",
                },
                _ => "other",
            };
            mark(*id, c, &mut cat);
        }
        // Font files are found through font descriptors.
        for obj in doc.objects.values() {
            if let Object::Dictionary(d) = obj {
                if d.get(b"Type").ok().and_then(|x| x.as_name().ok()) == Some(b"FontDescriptor") {
                    for key in [b"FontFile".as_slice(), b"FontFile2", b"FontFile3"] {
                        if let Ok(Object::Reference(r)) = d.get(key) {
                            cat.insert(*r, "fonts");
                        }
                    }
                }
                if let Ok(Object::Array(cs)) = d.get(b"ColorSpace") {
                    if let Some(Object::Reference(r)) = cs.get(1) {
                        cat.insert(*r, "colors");
                    }
                }
            }
        }
        let order = [
            "content",
            "images",
            "forms",
            "fonts",
            "annotations",
            "fields",
            "bookmarks",
            "structure",
            "attachments",
            "metadata",
            "colors",
            "other",
        ];
        let mut totals: HashMap<&'static str, (u64, usize)> = HashMap::new();
        for (id, obj) in &doc.objects {
            let c = cat.get(id).copied().unwrap_or("other");
            let e = totals.entry(c).or_default();
            e.0 += approx_size(obj);
            e.1 += 1;
        }
        order
            .iter()
            .filter_map(|c| {
                totals.get(c).map(|(b, n)| SpaceItem {
                    category: c,
                    bytes: *b,
                    objects: *n,
                })
            })
            .collect()
    }

    /// Whether the document says it follows PDF/A.
    pub fn is_pdfa(&self) -> bool {
        let Some(meta) = self.xmp_metadata() else {
            return false;
        };
        meta.contains("pdfaid:part")
    }

    /// The part of PDF/A the document says it follows (1, 2, 3, ...).
    pub fn pdfa_part(&self) -> Option<u32> {
        let meta = self.xmp_metadata()?;
        let at = meta.find("pdfaid:part")? + "pdfaid:part".len();
        // pdfaid:part="1" or <pdfaid:part>1</pdfaid:part>
        let near: String = meta[at..].chars().take(8).collect();
        let digits: String = near
            .chars()
            .skip_while(|c| !c.is_ascii_digit())
            .take_while(|c| c.is_ascii_digit())
            .collect();
        digits.parse().ok()
    }

    fn xmp_metadata(&self) -> Option<String> {
        let doc = &self.inner;
        let r = doc.catalog().ok()?.get(b"Metadata").ok()?.as_reference().ok()?;
        let s = doc.get_object(r).ok()?.as_stream().ok()?;
        Some(String::from_utf8_lossy(&stream_bytes(s)?).to_string())
    }

    /// Stop claiming PDF/A conformance so the document can be edited freely:
    /// the PDF/A identification leaves the metadata. Returns whether it was there.
    pub fn discard_pdfa(&mut self) -> Result<bool> {
        let Some(meta) = self.xmp_metadata() else {
            return Ok(false);
        };
        if !meta.contains("pdfaid:") {
            return Ok(false);
        }
        // Attributes (pdfaid:part="1") and elements (<pdfaid:part>1</pdfaid:part>).
        let mut out = String::with_capacity(meta.len());
        let mut rest = meta.as_str();
        while let Some(i) = rest.find("pdfaid:") {
            let before = &rest[..i];
            let after = &rest[i..];
            if let Some(kept) = before.strip_suffix('<') {
                // Element: drop up to its end tag.
                let name_end = after.find(['>', ' ', '/']).unwrap_or(after.len());
                let name = &after[..name_end];
                let close = format!("</{name}>");
                out.push_str(kept);
                rest = match after.find(&close) {
                    Some(e) => &after[e + close.len()..],
                    None => match after.find("/>") {
                        Some(e) => &after[e + 2..],
                        None => "",
                    },
                };
            } else {
                // Attribute: drop name="value".
                out.push_str(before.trim_end_matches([' ', '\t', '\n', '\r']));
                out.push(' ');
                let q = after.find(['"', '\'']).unwrap_or(after.len());
                let quote = after[q..].chars().next().unwrap_or('"');
                let end = after[q + 1..]
                    .find(quote)
                    .map(|e| q + 1 + e + 1)
                    .unwrap_or(after.len());
                rest = &after[end..];
            }
        }
        out.push_str(rest);
        // The namespace declaration is not needed any more.
        let out = out.replace(" xmlns:pdfaid=\"http://www.aiim.org/pdfa/ns/id/\"", "");
        let r = self.inner.catalog()?.get(b"Metadata")?.as_reference()?;
        self.begin_edit();
        if let Ok(Object::Stream(s)) = self.inner.get_object_mut(r) {
            s.set_plain_content(out.into_bytes());
        }
        self.commit()?;
        Ok(true)
    }

    /// Document-level scripts by event (see [`DOCUMENT_EVENTS`]).
    pub fn document_actions(&self) -> Vec<(String, String)> {
        let doc = &self.inner;
        let Ok(cat) = doc.catalog() else { return Vec::new() };
        let js = |a: &Object| -> Option<String> {
            let d = deref(doc, a).as_dict().ok()?;
            if d.get(b"S").ok()?.as_name().ok()? != b"JavaScript" {
                return None;
            }
            match deref(doc, d.get(b"JS").ok()?) {
                Object::Stream(s) => stream_bytes(s).map(|b| String::from_utf8_lossy(&b).to_string()),
                other => text_of(doc, other),
            }
        };
        let mut out = Vec::new();
        if let Some(code) = cat.get(b"OpenAction").ok().and_then(js) {
            out.push(("Open".to_string(), code));
        }
        if let Some(aa) = cat.get(b"AA").ok().and_then(|a| deref(doc, a).as_dict().ok()) {
            for (key, _) in DOCUMENT_EVENTS.iter().skip(1) {
                if let Some(code) = aa.get(key.as_bytes()).ok().and_then(js) {
                    out.push((key.to_string(), code));
                }
            }
        }
        out
    }

    /// Set (or, with empty `code`, remove) the script of a document event.
    pub fn set_document_action(&mut self, event: &str, code: &str) -> Result<()> {
        if !DOCUMENT_EVENTS.iter().any(|(k, _)| *k == event) {
            return Err(Error::Invalid(format!("unknown document event {event}")));
        }
        self.begin_edit();
        let action = (!code.trim().is_empty()).then(|| {
            Object::Dictionary(dictionary! { "S" => "JavaScript", "JS" => lopdf::text_string(code) })
        });
        if event == "Open" {
            let cat = self.inner.catalog_mut()?;
            match action {
                Some(a) => cat.set("OpenAction", a),
                None => {
                    let is_js = cat
                        .get(b"OpenAction")
                        .ok()
                        .and_then(|a| a.as_dict().ok())
                        .is_some_and(|d| {
                            d.get(b"S").ok().and_then(|s| s.as_name().ok()) == Some(b"JavaScript")
                        });
                    if is_js {
                        cat.remove(b"OpenAction");
                    }
                }
            }
        } else {
            let existing = self.inner.catalog()?.get(b"AA").ok().cloned();
            let mut aa: Dictionary = match &existing {
                Some(Object::Reference(r)) => self.inner.get_dictionary(*r)?.clone(),
                Some(Object::Dictionary(d)) => d.clone(),
                _ => Dictionary::new(),
            };
            match action {
                Some(a) => aa.set(event.as_bytes().to_vec(), a),
                None => {
                    aa.remove(event.as_bytes());
                }
            }
            match existing {
                Some(Object::Reference(r)) => {
                    self.inner.objects.insert(r, Object::Dictionary(aa));
                }
                _ => {
                    let cat = self.inner.catalog_mut()?;
                    if aa.is_empty() {
                        cat.remove(b"AA");
                    } else {
                        cat.set("AA", Object::Dictionary(aa));
                    }
                }
            }
        }
        self.commit()
    }
}

#[cfg(test)]
mod tests {
    use super::rtf_to_text;

    #[test]
    fn rich_text_becomes_plain_text() {
        let rtf = r"{\rtf1\ansi{\fonttbl{\f0 Arial;}}{\colortbl;\red255\green0\blue0;}\f0\fs24 Hello \b bold\b0  world.\par Caf\'e9 \u8364?\tab end\par}";
        assert_eq!(rtf_to_text(rtf), "Hello bold world.\nCafé €\tend");
    }
}
