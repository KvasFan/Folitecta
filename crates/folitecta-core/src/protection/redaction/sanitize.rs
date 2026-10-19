//! Sanitizing a redacted document: what the person chose to remove besides
//! the content under the marks — document information and metadata,
//! attached files and media, hidden content, the places that quote the
//! removed text, and scripts or actions that run programs or send data.
//! Every part is applied to the whole document, best effort: a part that
//! fails is reported and the others still run.

use std::collections::HashSet;

use lopdf::{Dictionary, Object, ObjectId, Stream, dictionary};
use rand_core::{OsRng, RngCore};

use super::terms::{Term, count_terms, replace_terms};
use crate::document::Document;
use crate::error::Result;
use crate::util::{deref, text_of};

/// The parts of sanitizing applied with a redaction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SanitizeOptions {
    /// Document information, every metadata stream, private data of other
    /// programs (`/PieceInfo`); a new file identifier (unprotected files).
    pub metadata: bool,
    /// Attached files and embedded media.
    pub attachments: bool,
    /// Layers hidden by default and their content, hidden comments and form
    /// fields, page thumbnails, XFA forms.
    pub hidden: bool,
    /// Bookmarks, links, destination names, tags and document information
    /// that quote the removed text.
    pub quoting: bool,
    /// Scripts and actions that run programs or send data.
    pub scripts: bool,
    /// What replaces a quote of the removed text (when its mark has no
    /// overlay text).
    pub placeholder: String,
    /// Terms (lower case) the person chose to leave where they are quoted.
    pub skip_terms: Vec<String>,
}

/// Whether each sanitizing part is on by default: all of them.
pub const SANITIZE_BY_DEFAULT: bool = true;

impl Default for SanitizeOptions {
    fn default() -> Self {
        SanitizeOptions {
            metadata: SANITIZE_BY_DEFAULT,
            attachments: SANITIZE_BY_DEFAULT,
            hidden: SANITIZE_BY_DEFAULT,
            quoting: SANITIZE_BY_DEFAULT,
            scripts: SANITIZE_BY_DEFAULT,
            placeholder: "[redacted]".into(),
            skip_terms: Vec::new(),
        }
    }
}

impl SanitizeOptions {
    /// No sanitizing (only the parts every redaction does).
    pub fn none() -> Self {
        SanitizeOptions {
            metadata: false,
            attachments: false,
            hidden: false,
            quoting: false,
            scripts: false,
            ..SanitizeOptions::default()
        }
    }
}

/// A part of sanitizing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SanitizePart {
    Metadata,
    Attachments,
    Hidden,
    Quoting,
    Scripts,
}

/// What sanitizing removed (or, for a preview, would remove).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SanitizeReport {
    /// Document information or metadata were present (and removed).
    pub metadata: bool,
    /// Attached files and media comments.
    pub attachments: usize,
    /// Hidden comments and form fields.
    pub hidden: usize,
    /// Layers hidden by default.
    pub hidden_layers: usize,
    /// Quotes of the removed text.
    pub quotes: usize,
    /// Scripts and risky actions.
    pub scripts: usize,
    /// The document claimed PDF/A or PDF/UA conformance in its metadata; the
    /// claim is kept in a new minimal metadata stream.
    pub conformance_kept: bool,
    /// Parts that failed (the others were applied).
    pub failed: Vec<SanitizePart>,
}

/// Annotations that hold media or attached files.
const MEDIA: &[&[u8]] = &[
    b"FileAttachment",
    b"Screen",
    b"RichMedia",
    b"Movie",
    b"Sound",
    b"3D",
];

/// Which actions go: scripts and programs, and/or embedded-file targets.
#[derive(Debug, Clone, Copy)]
struct Risky {
    scripts: bool,
    attachments: bool,
}

/// An action that runs a program or a script, sends or reads data, or opens
/// an embedded file (each when chosen).
fn risky(doc: &lopdf::Document, a: &Dictionary, which: Risky) -> bool {
    let s = a.get(b"S").ok().and_then(|s| deref(doc, s).as_name().ok());
    match s {
        Some(b"JavaScript" | b"Launch" | b"SubmitForm" | b"ImportData" | b"RichMediaExecute") => {
            which.scripts
        }
        Some(b"Rendition") => which.scripts && a.has(b"JS"),
        Some(b"GoToE") => which.attachments,
        _ => false,
    }
}

/// An action (with its `/Next` chain) without the risky ones; None when
/// nothing is left. `removed` counts what went.
fn filter_action(
    doc: &lopdf::Document,
    o: &Object,
    which: Risky,
    removed: &mut usize,
    depth: usize,
) -> Option<Object> {
    if depth > 32 {
        return Some(o.clone());
    }
    match deref(doc, o) {
        Object::Dictionary(a) => {
            let next = a
                .get(b"Next")
                .ok()
                .map(|n| filter_action(doc, n, which, removed, depth + 1));
            if risky(doc, a, which) {
                *removed += 1;
                return next.flatten();
            }
            match next {
                None => Some(o.clone()),
                Some(n) => {
                    let mut a = a.clone();
                    match n {
                        Some(n) => a.set("Next", n),
                        None => {
                            a.remove(b"Next");
                        }
                    }
                    Some(Object::Dictionary(a))
                }
            }
        }
        Object::Array(list) => {
            let kept: Vec<Object> = list
                .iter()
                .filter_map(|x| filter_action(doc, x, which, removed, depth + 1))
                .collect();
            (!kept.is_empty()).then_some(Object::Array(kept))
        }
        _ => Some(o.clone()),
    }
}

/// The text strings of a dictionary entry, replaced.
fn replace_entry(doc: &lopdf::Document, d: &mut Dictionary, key: &[u8], terms: &[Term]) -> bool {
    let Some(text) = d.get(key).ok().and_then(|v| text_of(doc, v)) else {
        return false;
    };
    match replace_terms(&text, terms) {
        Some(new) => {
            d.set(key.to_vec(), lopdf::text_string(&new));
            true
        }
        None => false,
    }
}

/// Text of a dictionary entry holds a term.
fn entry_quotes(doc: &lopdf::Document, d: &Dictionary, key: &[u8], terms: &[Term]) -> bool {
    d.get(key)
        .ok()
        .and_then(|v| text_of(doc, v))
        .is_some_and(|t| count_terms(&t, terms) > 0)
}

/// The URI of an action dictionary (or of a link's `/A`).
fn uri_of(doc: &lopdf::Document, d: &Dictionary) -> Option<String> {
    let a = deref(doc, d.get(b"A").ok()?).as_dict().ok()?;
    if a.get(b"S").ok()?.as_name().ok()? != b"URI" {
        return None;
    }
    a.get(b"URI").ok().map(|u| match deref(doc, u) {
        Object::String(b, _) => String::from_utf8_lossy(b).into_owned(),
        _ => String::new(),
    })
}

/// The properties of an XMP packet that claim conformance (PDF/A, PDF/UA).
fn conformance_claims(packet: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for key in [
        "pdfaid:part",
        "pdfaid:conformance",
        "pdfaid:amd",
        "pdfaid:rev",
        "pdfuaid:part",
    ] {
        // As an attribute or as an element.
        let attr = format!("{key}=\"");
        if let Some(at) = packet.find(&attr) {
            let rest = &packet[at + attr.len()..];
            if let Some(end) = rest.find('"') {
                out.push((key.to_string(), rest[..end].to_string()));
                continue;
            }
        }
        let open = format!("<{key}>");
        if let Some(at) = packet.find(&open) {
            let rest = &packet[at + open.len()..];
            if let Some(end) = rest.find('<') {
                out.push((key.to_string(), rest[..end].trim().to_string()));
            }
        }
    }
    out
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// A minimal XMP packet keeping conformance claims (and a title when PDF/UA
/// asks for one).
fn minimal_xmp(claims: &[(String, String)], title: Option<&str>) -> String {
    let mut props = String::new();
    for (k, v) in claims {
        props.push_str(&format!("<{k}>{}</{k}>", xml_escape(v)));
    }
    if let Some(t) = title {
        props.push_str(&format!(
            "<dc:title><rdf:Alt><rdf:li xml:lang=\"x-default\">{}</rdf:li></rdf:Alt></dc:title>",
            xml_escape(t)
        ));
    }
    format!(
        "<?xpacket begin=\"\u{feff}\" id=\"W5M0MpCehiHzreSzNTczkc9d\"?>\n\
         <x:xmpmeta xmlns:x=\"adobe:ns:meta/\">\n\
         <rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\">\n\
         <rdf:Description rdf:about=\"\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\" xmlns:pdfaid=\"http://www.aiim.org/pdfa/ns/id/\" xmlns:pdfuaid=\"http://www.aiim.org/pdfua/ns/id/\">{props}</rdf:Description>\n\
         </rdf:RDF>\n\
         </x:xmpmeta>\n\
         <?xpacket end=\"w\"?>"
    )
}

fn flags(d: &Dictionary) -> i64 {
    d.get(b"F").ok().and_then(|f| f.as_i64().ok()).unwrap_or(0)
}

fn hidden_flag(d: &Dictionary) -> bool {
    flags(d) & (2 | 32) != 0
}

impl Document {
    /// Annotations of every page matching `keep`: page id and annotation ids.
    fn annotations_where(
        &self,
        mut pick: impl FnMut(&Dictionary) -> bool,
    ) -> Vec<(ObjectId, HashSet<ObjectId>)> {
        let mut out = Vec::new();
        for pid in self.page_ids() {
            let Ok(pd) = self.inner.get_dictionary(pid) else {
                continue;
            };
            let set: HashSet<ObjectId> = crate::annots::annot_refs(&self.inner, pd)
                .into_iter()
                .filter(|(_, d)| pick(d))
                .filter_map(|(id, _)| id)
                .collect();
            if !set.is_empty() {
                out.push((pid, set));
            }
        }
        out
    }

    fn subtype_of(d: &Dictionary) -> Vec<u8> {
        d.get(b"Subtype")
            .ok()
            .and_then(|s| s.as_name().ok())
            .map(<[u8]>::to_vec)
            .unwrap_or_default()
    }

    /// Hidden annotations (not pop-ups or widgets) of every page.
    fn hidden_annotations(&self) -> Vec<(ObjectId, HashSet<ObjectId>)> {
        self.annotations_where(|d| {
            let st = Self::subtype_of(d);
            hidden_flag(d) && st != b"Popup" && st != b"Widget"
        })
    }

    /// Form fields (not signatures) whose widgets are all hidden or absent.
    fn hidden_fields(&self) -> Vec<ObjectId> {
        self.form_fields()
            .into_iter()
            .filter(|f| f.kind != crate::forms::FieldKind::Signature)
            .filter(|f| {
                self.field_widgets(f.id)
                    .iter()
                    .all(|w| self.inner.get_dictionary(*w).map(hidden_flag).unwrap_or(true))
            })
            .map(|f| f.id)
            .collect()
    }

    fn media_annotations(&self) -> Vec<(ObjectId, HashSet<ObjectId>)> {
        self.annotations_where(|d| MEDIA.contains(&Self::subtype_of(d).as_slice()))
    }

    /// Risky actions (scripts, programs, data sent or read) in the document.
    fn risky_actions(&self, which: Risky) -> usize {
        let doc = &self.inner;
        let mut n = 0;
        let mut seen: HashSet<*const Dictionary> = HashSet::new();
        let mut count = |a: &Dictionary| {
            if seen.insert(a as *const Dictionary) && risky(doc, a, which) {
                n += 1;
            }
        };
        for o in doc.objects.values() {
            if let Object::Dictionary(d) = o {
                if d.has(b"S") {
                    count(d);
                }
                for key in [b"A".as_slice(), b"OpenAction"] {
                    if let Ok(Object::Dictionary(a)) = d.get(key) {
                        count(a);
                    }
                }
                if let Ok(Object::Dictionary(aa)) = d.get(b"AA") {
                    for (_, v) in aa.iter() {
                        if let Object::Dictionary(a) = v {
                            count(a);
                        }
                    }
                }
            }
        }
        n + if which.scripts { self.javascript_names() } else { 0 }
    }

    fn javascript_names(&self) -> usize {
        let doc = &self.inner;
        let Some(tree) = doc
            .catalog()
            .ok()
            .and_then(|c| c.get(b"Names").ok())
            .and_then(|n| deref(doc, n).as_dict().ok())
            .and_then(|n| n.get(b"JavaScript").ok())
            .and_then(|t| deref(doc, t).as_dict().ok())
        else {
            return 0;
        };
        let mut pairs = Vec::new();
        crate::util::walk_name_tree(doc, tree, &mut pairs, 0);
        pairs.len().max(1)
    }

    /// Whether the document has risky actions (scripts …) at all.
    pub fn has_scripts(&self) -> bool {
        self.risky_actions(Risky {
            scripts: true,
            attachments: false,
        }) > 0
    }

    /// Places quoting the terms: bookmarks, links, destination names, tags,
    /// document information and metadata.
    pub(crate) fn quotes_of(&self, terms: &[Term]) -> usize {
        if terms.is_empty() {
            return 0;
        }
        let doc = &self.inner;
        let mut n = 0;
        for id in self.outline_objects() {
            if let Ok(d) = doc.get_dictionary(id) {
                n += usize::from(entry_quotes(doc, d, b"Title", terms));
                n += usize::from(uri_of(doc, d).is_some_and(|u| count_terms(&u, terms) > 0));
            }
        }
        for d in self.named_destinations() {
            n += usize::from(count_terms(&d.name, terms) > 0);
        }
        for (_, set) in self.annotations_where(|d| Self::subtype_of(d) == b"Link") {
            for id in set {
                if let Ok(d) = doc.get_dictionary(id) {
                    n += usize::from(entry_quotes(doc, d, b"Contents", terms));
                    n += usize::from(uri_of(doc, d).is_some_and(|u| count_terms(&u, terms) > 0));
                }
            }
        }
        for o in doc.objects.values() {
            if let Object::Dictionary(d) = o {
                if is_struct_elem(d) {
                    for key in [b"Alt".as_slice(), b"ActualText", b"E", b"T"] {
                        n += usize::from(entry_quotes(doc, d, key, terms));
                    }
                }
            }
        }
        if let Some(info) = self
            .inner
            .trailer
            .get(b"Info")
            .ok()
            .and_then(|i| deref(doc, i).as_dict().ok())
        {
            for (k, _) in info.iter() {
                n += usize::from(entry_quotes(doc, info, k, terms));
            }
        }
        if self.xmp_packet().is_some_and(|p| count_terms(&p, terms) > 0) {
            n += 1;
        }
        n
    }

    /// What sanitizing with `opts` would remove, without changing anything.
    pub fn sanitize_preview(&self, opts: &SanitizeOptions, terms: &[Term]) -> SanitizeReport {
        let doc = &self.inner;
        let info_present = self
            .inner
            .trailer
            .get(b"Info")
            .ok()
            .and_then(|i| deref(doc, i).as_dict().ok())
            .is_some_and(|d| !d.is_empty());
        let metadata = info_present
            || doc.objects.values().any(|o| match o {
                Object::Dictionary(d) => d.has(b"Metadata") || d.has(b"PieceInfo"),
                Object::Stream(s) => s.dict.has(b"Metadata") || s.dict.has(b"PieceInfo"),
                _ => false,
            });
        let terms: Vec<Term> = terms
            .iter()
            .filter(|t| !opts.skip_terms.contains(&t.text.iter().collect::<String>()))
            .cloned()
            .collect();
        SanitizeReport {
            metadata: opts.metadata && metadata,
            attachments: if opts.attachments {
                self.attachments().len()
                    + self
                        .media_annotations()
                        .iter()
                        .map(|(_, s)| s.len())
                        .sum::<usize>()
            } else {
                0
            },
            hidden: if opts.hidden {
                self.hidden_annotations()
                    .iter()
                    .map(|(_, s)| s.len())
                    .sum::<usize>()
                    + self.hidden_fields().len()
            } else {
                0
            },
            hidden_layers: if opts.hidden {
                self.layer_defaults().values().filter(|v| !**v).count()
            } else {
                0
            },
            quotes: if opts.quoting { self.quotes_of(&terms) } else { 0 },
            scripts: if opts.scripts {
                self.risky_actions(Risky {
                    scripts: true,
                    attachments: false,
                })
            } else {
                0
            },
            conformance_kept: false,
            failed: Vec::new(),
        }
    }

    /// Apply the chosen sanitizing parts (no undo step: the redaction makes one).
    pub(crate) fn sanitize_in(&mut self, opts: &SanitizeOptions, terms: &[Term]) -> SanitizeReport {
        let mut r = SanitizeReport::default();
        let terms: Vec<Term> = terms
            .iter()
            .filter(|t| !opts.skip_terms.contains(&t.text.iter().collect::<String>()))
            .cloned()
            .collect();
        if opts.scripts || opts.attachments {
            match self.remove_risky_actions(opts.scripts, opts.attachments) {
                Ok(n) => r.scripts = n,
                Err(_) => r.failed.push(SanitizePart::Scripts),
            }
        }
        if opts.attachments {
            match self.remove_attachments() {
                Ok(n) => r.attachments = n,
                Err(_) => r.failed.push(SanitizePart::Attachments),
            }
        }
        if opts.hidden {
            match self.remove_hidden() {
                Ok((n, layers)) => {
                    r.hidden = n;
                    r.hidden_layers = layers;
                }
                Err(_) => r.failed.push(SanitizePart::Hidden),
            }
        }
        if opts.quoting {
            match self.replace_quotes(&terms) {
                Ok(n) => r.quotes = n,
                Err(_) => r.failed.push(SanitizePart::Quoting),
            }
        }
        if opts.metadata {
            match self.remove_metadata(&opts.placeholder) {
                Ok((present, kept)) => {
                    r.metadata = present;
                    r.conformance_kept = kept;
                }
                Err(_) => r.failed.push(SanitizePart::Metadata),
            }
        }
        r
    }

    fn remove_risky_actions(&mut self, scripts: bool, attachments: bool) -> Result<usize> {
        let which = Risky { scripts, attachments };
        let mut removed = 0;
        let ids: Vec<ObjectId> = self.inner.objects.keys().copied().collect();
        for id in ids {
            let Ok(Object::Dictionary(d)) = self.inner.get_object(id) else {
                continue;
            };
            let mut d2 = d.clone();
            let mut changed = false;
            for key in [b"A".as_slice(), b"OpenAction"] {
                let Ok(v) = d.get(key) else { continue };
                // A destination (an array naming a page) is no action.
                if key == b"OpenAction" && matches!(deref(&self.inner, v), Object::Array(_)) {
                    continue;
                }
                let before = removed;
                let kept = filter_action(&self.inner, v, which, &mut removed, 0);
                if removed != before {
                    changed = true;
                    match kept {
                        Some(k) => d2.set(key.to_vec(), k),
                        None => {
                            d2.remove(key);
                        }
                    }
                }
            }
            if let Some(aa) = d
                .get(b"AA")
                .ok()
                .and_then(|aa| deref(&self.inner, aa).as_dict().ok())
            {
                let mut new_aa = Dictionary::new();
                let before = removed;
                for (k, v) in aa.iter() {
                    if let Some(kept) = filter_action(&self.inner, v, which, &mut removed, 0) {
                        new_aa.set(k.clone(), kept);
                    }
                }
                if removed != before {
                    changed = true;
                    if new_aa.is_empty() {
                        d2.remove(b"AA");
                    } else {
                        d2.set("AA", Object::Dictionary(new_aa));
                    }
                }
            }
            if changed {
                self.inner.objects.insert(id, Object::Dictionary(d2));
            }
        }
        if scripts {
            removed += self.javascript_names();
            let names = self
                .inner
                .catalog()
                .ok()
                .and_then(|c| c.get(b"Names").ok())
                .cloned();
            match names {
                Some(Object::Reference(r)) => {
                    if let Ok(n) = self.inner.get_dictionary_mut(r) {
                        n.remove(b"JavaScript");
                    }
                }
                Some(Object::Dictionary(_)) => {
                    if let Ok(Object::Dictionary(n)) = self.inner.catalog_mut()?.get_mut(b"Names") {
                        n.remove(b"JavaScript");
                    }
                }
                _ => {}
            }
            // Calculations need scripts: their order goes when no field keeps one.
            let calc_left = self.inner.objects.values().any(|o| {
                o.as_dict()
                    .ok()
                    .and_then(|d| d.get(b"AA").ok())
                    .and_then(|aa| deref(&self.inner, aa).as_dict().ok())
                    .is_some_and(|aa| aa.has(b"C"))
            });
            if !calc_left {
                if let Some(af) = self.acroform_id() {
                    if let Ok(d) = self.inner.get_dictionary_mut(af) {
                        d.remove(b"CO");
                    }
                }
            }
        }
        Ok(removed)
    }

    /// The AcroForm dictionary as an object of its own (made one when inline).
    fn acroform_id(&mut self) -> Option<ObjectId> {
        let af = self.inner.catalog().ok()?.get(b"AcroForm").ok()?.clone();
        match af {
            Object::Reference(r) => Some(r),
            Object::Dictionary(d) => {
                let id = self.inner.add_object(d);
                self.inner
                    .catalog_mut()
                    .ok()?
                    .set("AcroForm", Object::Reference(id));
                Some(id)
            }
            _ => None,
        }
    }

    fn remove_attachments(&mut self) -> Result<usize> {
        let mut n = self.attachments().len();
        for (pid, set) in self.media_annotations() {
            n += set.len();
            self.remove_annotations(pid, &set);
        }
        // The attachment tree, file specifications' embedded files, associated files.
        let names = self
            .inner
            .catalog()
            .ok()
            .and_then(|c| c.get(b"Names").ok())
            .cloned();
        match names {
            Some(Object::Reference(r)) => {
                if let Ok(d) = self.inner.get_dictionary_mut(r) {
                    d.remove(b"EmbeddedFiles");
                }
            }
            Some(Object::Dictionary(_)) => {
                if let Ok(Object::Dictionary(d)) = self.inner.catalog_mut()?.get_mut(b"Names") {
                    d.remove(b"EmbeddedFiles");
                }
            }
            _ => {}
        }
        for o in self.inner.objects.values_mut() {
            let d = match o {
                Object::Dictionary(d) => d,
                Object::Stream(s) => &mut s.dict,
                _ => continue,
            };
            d.remove(b"AF");
            d.remove(b"EF");
            d.remove(b"RF");
        }
        let cat = self.inner.catalog_mut()?;
        cat.remove(b"Collection");
        cat.remove(b"AF");
        Ok(n)
    }

    fn remove_hidden(&mut self) -> Result<(usize, usize)> {
        let layers = self.remove_hidden_layers_in()?;
        let mut n = 0;
        for (pid, set) in self.hidden_annotations() {
            n += set.len();
            self.remove_annotations(pid, &set);
        }
        for f in self.hidden_fields() {
            self.remove_form_field(f)?;
            n += 1;
        }
        for pid in self.page_ids() {
            if let Ok(d) = self.inner.get_dictionary_mut(pid) {
                d.remove(b"Thumb");
            }
        }
        self.remove_xfa()?;
        Ok((n, layers))
    }

    fn replace_quotes(&mut self, terms: &[Term]) -> Result<usize> {
        if terms.is_empty() {
            return Ok(0);
        }
        let mut n = 0;
        // Destination names first: bookmarks and links refer to them.
        let mut k = 0;
        for d in self.named_destinations() {
            if count_terms(&d.name, terms) == 0 {
                continue;
            }
            let new = loop {
                k += 1;
                let name = format!("FoliDest{k}");
                if !self.named_destinations().iter().any(|x| x.name == name) {
                    break name;
                }
            };
            self.rename_named_destination_in(d.name.as_bytes(), new.as_bytes())?;
            n += 1;
        }
        // Bookmarks.
        for id in self.outline_objects() {
            let Ok(Object::Dictionary(mut d)) = self.inner.get_object(id).cloned() else {
                continue;
            };
            let mut changed = replace_entry(&self.inner, &mut d, b"Title", terms);
            n += usize::from(changed);
            if uri_of(&self.inner, &d).is_some_and(|u| count_terms(&u, terms) > 0) {
                d.remove(b"A");
                n += 1;
                changed = true;
            }
            if changed {
                self.inner.objects.insert(id, Object::Dictionary(d));
            }
        }
        // Links.
        for (pid, set) in self.annotations_where(|d| Self::subtype_of(d) == b"Link") {
            let mut gone = HashSet::new();
            for id in set {
                let Ok(Object::Dictionary(mut d)) = self.inner.get_object(id).cloned() else {
                    continue;
                };
                if uri_of(&self.inner, &d).is_some_and(|u| count_terms(&u, terms) > 0) {
                    gone.insert(id);
                    n += 1;
                    continue;
                }
                if replace_entry(&self.inner, &mut d, b"Contents", terms) {
                    n += 1;
                    self.inner.objects.insert(id, Object::Dictionary(d));
                }
            }
            if !gone.is_empty() {
                self.remove_annotations(pid, &gone);
            }
        }
        // Tags.
        let elems: Vec<ObjectId> = self
            .inner
            .objects
            .iter()
            .filter(|(_, o)| o.as_dict().is_ok_and(is_struct_elem))
            .map(|(id, _)| *id)
            .collect();
        for id in elems {
            let Ok(Object::Dictionary(mut d)) = self.inner.get_object(id).cloned() else {
                continue;
            };
            let mut changed = false;
            for key in [b"Alt".as_slice(), b"ActualText", b"E", b"T"] {
                if replace_entry(&self.inner, &mut d, key, terms) {
                    changed = true;
                    n += 1;
                }
            }
            if changed {
                self.inner.objects.insert(id, Object::Dictionary(d));
            }
        }
        // Document information and metadata.
        let info_id = self
            .inner
            .trailer
            .get(b"Info")
            .ok()
            .and_then(|i| i.as_reference().ok());
        if let Some(info_id) = info_id {
            if let Ok(Object::Dictionary(mut d)) = self.inner.get_object(info_id).cloned() {
                let keys: Vec<Vec<u8>> = d.iter().map(|(k, _)| k.clone()).collect();
                let mut changed = false;
                for k in keys {
                    if replace_entry(&self.inner, &mut d, &k, terms) {
                        changed = true;
                        n += 1;
                    }
                }
                if changed {
                    self.inner.objects.insert(info_id, Object::Dictionary(d));
                }
            }
        }
        if let Some(packet) = self.xmp_packet() {
            if count_terms(&packet, terms) > 0 {
                let claims = conformance_claims(&packet);
                let generated = self.generated_xmp();
                let packet = if claims.is_empty() {
                    generated
                } else {
                    // Keep the claims next to the generated properties.
                    let title = self.title();
                    minimal_xmp(&claims, (!title.is_empty()).then_some(title.as_str()))
                };
                self.set_catalog_xmp(packet)?;
                n += 1;
            }
        }
        Ok(n)
    }

    fn set_catalog_xmp(&mut self, packet: String) -> Result<()> {
        let id = self.inner.add_object(Stream::new(
            dictionary! { "Type" => "Metadata", "Subtype" => "XML" },
            packet.into_bytes(),
        ));
        self.inner.catalog_mut()?.set("Metadata", Object::Reference(id));
        Ok(())
    }

    /// Remove document information and metadata. Returns whether there was
    /// some, and whether a conformance claim was kept.
    fn remove_metadata(&mut self, placeholder: &str) -> Result<(bool, bool)> {
        let claims = self
            .xmp_packet()
            .map(|p| conformance_claims(&p))
            .unwrap_or_default();
        let mut present = false;
        let info_id = self
            .inner
            .trailer
            .get(b"Info")
            .ok()
            .and_then(|i| i.as_reference().ok());
        if let Some(info_id) = info_id {
            if let Ok(d) = self.inner.get_dictionary_mut(info_id) {
                present |= !d.is_empty();
                *d = Dictionary::new();
            }
        }
        for o in self.inner.objects.values_mut() {
            let d = match o {
                Object::Dictionary(d) => d,
                Object::Stream(s) => &mut s.dict,
                _ => continue,
            };
            for key in [b"Metadata".as_slice(), b"PieceInfo", b"LastModified"] {
                present |= d.remove(key).is_some();
            }
        }
        {
            let cat = self.inner.catalog_mut()?;
            cat.remove(b"SpiderInfo");
        }
        let names = self
            .inner
            .catalog()
            .ok()
            .and_then(|c| c.get(b"Names").ok())
            .and_then(|n| n.as_reference().ok());
        if let Some(r) = names {
            if let Ok(d) = self.inner.get_dictionary_mut(r) {
                d.remove(b"IDS");
                d.remove(b"URLS");
            }
        }
        let mut kept = false;
        if !claims.is_empty() {
            // PDF/UA needs a title; the claims stay so the file says what it conforms to.
            let ua = claims.iter().any(|(k, _)| k.starts_with("pdfuaid"));
            if ua {
                if let Some(info_id) = info_id {
                    if let Ok(d) = self.inner.get_dictionary_mut(info_id) {
                        d.set("Title", lopdf::text_string(placeholder));
                    }
                }
            }
            self.set_catalog_xmp(minimal_xmp(&claims, ua.then_some(placeholder)))?;
            kept = true;
        }
        // A new file identifier: the old one could tie the copy to the original.
        // A protected file keeps it: its key depends on it.
        if self.protection_key().is_none() {
            let mut a = [0u8; 16];
            let mut b = [0u8; 16];
            OsRng.fill_bytes(&mut a);
            OsRng.fill_bytes(&mut b);
            self.inner.trailer.set(
                "ID",
                Object::Array(vec![
                    Object::String(a.to_vec(), lopdf::StringFormat::Hexadecimal),
                    Object::String(b.to_vec(), lopdf::StringFormat::Hexadecimal),
                ]),
            );
        }
        Ok((present, kept))
    }
}

/// A structure element dictionary.
pub(crate) fn is_struct_elem(d: &Dictionary) -> bool {
    d.has_type(b"StructElem") || (d.has(b"S") && d.has(b"P") && !d.has_type(b"MCR") && !d.has_type(b"OBJR"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn risky_actions_go_and_their_chains_keep_the_rest() {
        let doc = lopdf::Document::with_version("1.7");
        let action = Object::Dictionary(dictionary! {
            "S" => "JavaScript",
            "JS" => Object::string_literal("app.alert(1)"),
            "Next" => dictionary! { "S" => "GoTo", "D" => Object::string_literal("x") },
        });
        let mut n = 0;
        let scripts = Risky {
            scripts: true,
            attachments: false,
        };
        let kept = filter_action(&doc, &action, scripts, &mut n, 0).unwrap();
        assert_eq!(n, 1);
        assert_eq!(
            kept.as_dict().unwrap().get(b"S").unwrap().as_name().unwrap(),
            b"GoTo"
        );
        let launch = Object::Dictionary(dictionary! { "S" => "Launch" });
        assert!(filter_action(&doc, &launch, scripts, &mut n, 0).is_none());
        let goto_e = Object::Dictionary(dictionary! { "S" => "GoToE" });
        assert!(filter_action(&doc, &goto_e, scripts, &mut n, 0).is_some());
        let files = Risky {
            scripts: false,
            attachments: true,
        };
        assert!(filter_action(&doc, &goto_e, files, &mut n, 0).is_none());
        assert!(filter_action(&doc, &launch, files, &mut n, 0).is_some());
    }

    #[test]
    fn conformance_claims_are_read() {
        let p = "<rdf:Description pdfaid:part=\"2\" pdfaid:conformance=\"B\"/><pdfuaid:part>1</pdfuaid:part>";
        let c = conformance_claims(p);
        assert!(c.contains(&("pdfaid:part".into(), "2".into())));
        assert!(c.contains(&("pdfuaid:part".into(), "1".into())));
        assert!(minimal_xmp(&c, Some("[redacted]")).contains("<dc:title>"));
    }
}
