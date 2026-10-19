//! Comment exchange with other applications: comments are exported and
//! imported as XFDF (XML) or FDF, and the comments of another PDF document can
//! be imported page by page.
//!
//! Imported comments keep their name (`/NM`): a comment whose name matches a
//! comment of the document replaces it, so a review can be exported, edited
//! elsewhere and imported again without duplicates.

use std::collections::{BTreeMap, HashMap};
use std::fmt;

use lopdf::{Dictionary, Object, ObjectId, Stream, dictionary};

use crate::document::Document;
use crate::error::{Error, Result};
use crate::fonts::GlyphProblem;
use crate::util::dict_text;

mod fdf;
mod helpers;
mod xfdf;

use helpers::*;

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

/// A comment of an imported file whose text cannot be drawn (it has no
/// drawing of its own, and generated text cannot draw it).
#[derive(Debug, Clone, PartialEq)]
pub struct UndrawableComment {
    /// Page of the comment (zero-based).
    pub page: usize,
    pub author: String,
    /// Name of the comment (`/NM`).
    pub name: String,
    pub contents: String,
    pub problem: GlyphProblem,
    /// The replies to it (and the replies to them), left out with it
    /// together with their pop-ups.
    pub replies: Vec<LeftOutReply>,
}

/// A reply left out with the comment it answers.
#[derive(Debug, Clone, PartialEq)]
pub struct LeftOutReply {
    pub author: String,
    pub contents: String,
}

/// At most 40 characters of a text, for messages.
pub(crate) fn excerpt(text: &str) -> String {
    let mut out: String = text.chars().take(40).collect();
    if text.chars().count() > 40 {
        out.push('…');
    }
    out
}

impl fmt::Display for LeftOutReply {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if !self.author.is_empty() {
            write!(f, "{}: ", self.author)?;
        }
        write!(f, "“{}”", excerpt(&self.contents))
    }
}

impl fmt::Display for UndrawableComment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "page {}", self.page + 1)?;
        if !self.author.is_empty() {
            write!(f, ", {}", self.author)?;
        }
        write!(
            f,
            ", \"{}\" ({}): {}",
            excerpt(&self.contents),
            self.name,
            self.problem
        )?;
        if !self.replies.is_empty() {
            let replies: Vec<String> = self.replies.iter().map(|r| r.to_string()).collect();
            write!(f, " (replies left out with it: {})", replies.join(", "))?;
        }
        Ok(())
    }
}

/// The comments that cannot be drawn, for the error of an import.
pub(crate) fn describe_undrawable(list: &[UndrawableComment]) -> String {
    let items: Vec<String> = list.iter().map(|c| c.to_string()).collect();
    format!(
        "{} comment(s) of the file cannot be drawn, so nothing was imported: {}",
        list.len(),
        items.join("; ")
    )
}

/// The comment `d` of an import, when its text cannot be drawn and it has no
/// drawing of its own to keep.
fn undrawable(
    doc: &lopdf::Document,
    page: usize,
    d: &Dictionary,
    name: &str,
    keeps_drawing: bool,
) -> Option<UndrawableComment> {
    if keeps_drawing && d.has(b"AP") {
        return None;
    }
    let problem = crate::appearance::text_problem(doc, d)?;
    Some(UndrawableComment {
        page,
        author: dict_text(doc, d, b"T").unwrap_or_default(),
        name: name.to_string(),
        contents: dict_text(doc, d, b"Contents").unwrap_or_default(),
        problem,
        replies: Vec::new(),
    })
}

/// One comment of an import, as the check before the import sees it.
struct Candidate<'a> {
    page: usize,
    name: String,
    /// Name of the comment it answers or is grouped under.
    in_reply_to: Option<String>,
    dict: &'a Dictionary,
    /// Its own drawing is kept when it has one (PDF and FDF sources).
    keeps_drawing: bool,
}

/// Which comments of an import are kept: not those whose text cannot be
/// drawn, nor the replies to them (a reply alone would become a comment of
/// its own); comments grouped under a left-out comment stay, on their own.
/// `doc` resolves the references of the dictionaries.
fn plan_import(doc: &lopdf::Document, items: &[Candidate<'_>]) -> (Vec<bool>, Vec<UndrawableComment>) {
    let mut left_out: Vec<UndrawableComment> = Vec::new();
    let mut root_of: HashMap<String, usize> = HashMap::new();
    for it in items {
        if let Some(c) = undrawable(doc, it.page, it.dict, &it.name, it.keeps_drawing) {
            root_of.insert(it.name.clone(), left_out.len());
            left_out.push(c);
        }
    }
    loop {
        let mut more = false;
        for it in items {
            let group = name_str(it.dict, b"RT").as_deref() == Some("Group");
            if group || root_of.contains_key(&it.name) {
                continue;
            }
            if let Some(root) = it.in_reply_to.as_ref().and_then(|p| root_of.get(p)).copied() {
                left_out[root].replies.push(LeftOutReply {
                    author: dict_text(doc, it.dict, b"T").unwrap_or_default(),
                    contents: dict_text(doc, it.dict, b"Contents").unwrap_or_default(),
                });
                root_of.insert(it.name.clone(), root);
                more = true;
            }
        }
        if !more {
            break;
        }
    }
    let keep = items.iter().map(|it| !root_of.contains_key(&it.name)).collect();
    (keep, left_out)
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

/// Keys a comment keeps only inside its own document. Its layer (`/OC`)
/// travels by name ([`LAYER_KEY`]): the layer object is never copied, as a
/// layer not listed in the optional content properties of the document is
/// not a layer of it (ISO 32000-1, 8.11.4.2).
const LOCAL_KEYS: &[&[u8]] = &[b"P", b"Popup", b"IRT", b"Parent", b"Page", b"StructParent", b"OC"];

/// The name of the layer of an exported comment.
pub(super) const LAYER_KEY: &[u8] = b"FolitectaLayer";

/// The name of the layer of a comment of `src`: the name written by an
/// export, or the name of the layer its `/OC` names directly.
pub(super) fn layer_name_in(src: &lopdf::Document, d: &Dictionary) -> Option<String> {
    if let Some(name) = dict_text(src, d, LAYER_KEY).filter(|n| !n.trim().is_empty()) {
        return Some(name);
    }
    let oc = d.get(b"OC").ok()?;
    let id = crate::layer_drawing::single_layer(src, oc)?;
    dict_text(src, src.get_dictionary(id).ok()?, b"Name").filter(|n| !n.trim().is_empty())
}

/// A comment ready to be added to the document.
pub(super) struct Imported {
    pub(super) page: usize,
    pub(super) dict: Dictionary,
    pub(super) popup: Option<Dictionary>,
    pub(super) name: String,
    pub(super) in_reply_to: Option<String>,
    /// Keep the appearance that came with the comment (FDF and PDF sources).
    pub(super) keep_appearance: bool,
    /// File of a file attachment comment: name, data, data is Flate-compressed.
    pub(super) file: Option<(String, Vec<u8>, bool)>,
    /// The name of the layer of the comment (`None`: the file does not say;
    /// a comment updated in place keeps its layer).
    pub(super) layer: Option<String>,
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
    /// When comments of the file cannot be drawn ([`Error::Undrawable`] names
    /// them) nothing is imported; [`Document::import_comments_skipping`]
    /// imports the others.
    pub fn import_comments(&mut self, data: &[u8]) -> Result<usize> {
        self.import_comments_with(data, false).map(|(n, _)| n)
    }

    /// Import the comments of XFDF, FDF or PDF data that can be drawn and
    /// leave out the others: the number imported and the comments left out.
    pub fn import_comments_skipping(&mut self, data: &[u8]) -> Result<(usize, Vec<UndrawableComment>)> {
        self.import_comments_with(data, true)
    }

    fn import_comments_with(&mut self, data: &[u8], skip: bool) -> Result<(usize, Vec<UndrawableComment>)> {
        match CommentFormat::sniff(data) {
            Some(CommentFormat::Xfdf) => self.xfdf_import(&String::from_utf8_lossy(data), skip),
            Some(CommentFormat::Fdf) => self.fdf_import(data, skip),
            Some(CommentFormat::Pdf) => self.pdf_import(data, skip),
            None => Err(Error::Invalid("the file is not an XFDF, FDF or PDF file".into())),
        }
    }

    /// Import the comments of another PDF: the comments of each page go to the
    /// page with the same number (one undo step). The comments of a protected
    /// file are refused by a document that is not protected.
    pub fn import_comments_pdf(&mut self, data: &[u8]) -> Result<usize> {
        self.pdf_import(data, false).map(|(n, _)| n)
    }

    fn pdf_import(&mut self, data: &[u8], skip: bool) -> Result<(usize, Vec<UndrawableComment>)> {
        let other = Document::from_bytes(data.to_vec(), None, None)?;
        self.check_import_from(&other)?;
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
        self.import_from_source(&other.inner, entries, skip)
    }

    /// Copy comment dictionaries of another object model into the document.
    /// Comments without a drawing of their own are drawn here: the text of
    /// each is checked first, and those that cannot be drawn refuse the
    /// import (or are left out when `skip`).
    pub(super) fn import_from_source(
        &mut self,
        src: &lopdf::Document,
        entries: Vec<(usize, ObjectId, Dictionary)>,
        skip: bool,
    ) -> Result<(usize, Vec<UndrawableComment>)> {
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
        let (keep, skipped) = {
            let candidates: Vec<Candidate<'_>> = entries
                .iter()
                .map(|(page, id, d)| Candidate {
                    page: *page,
                    name: comment_name(src, d, *id),
                    in_reply_to: d
                        .get(b"IRT")
                        .and_then(|o| o.as_reference())
                        .ok()
                        .and_then(|t| src.get_dictionary(t).ok().map(|td| comment_name(src, td, t))),
                    dict: d,
                    keeps_drawing: true,
                })
                .collect();
            plan_import(src, &candidates)
        };
        let entries: Vec<_> = entries
            .into_iter()
            .zip(keep)
            .filter_map(|(e, kept)| kept.then_some(e))
            .collect();
        if !skipped.is_empty() && !skip {
            return Err(Error::Undrawable(skipped));
        }
        if entries.is_empty() {
            return Ok((0, skipped));
        }
        let n = self.edit_step(|doc| doc.copy_imported(src, entries))?;
        Ok((n, skipped))
    }

    /// The copy of [`Document::import_from_source`], inside its edit.
    fn copy_imported(
        &mut self,
        src: &lopdf::Document,
        entries: Vec<(usize, ObjectId, Dictionary)>,
    ) -> Result<usize> {
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
            let layer = layer_name_in(src, &d);
            let mut dict = d.clone();
            for k in LOCAL_KEYS {
                dict.remove(k);
            }
            dict.remove(LAYER_KEY);
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
                layer,
            });
        }
        self.insert_imported(items)
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
        // Layers by name; a layer the document does not have is added, so the
        // comment stays on a layer of that name.
        let mut layers: HashMap<String, ObjectId> = HashMap::new();
        for l in self.layers() {
            layers.entry(l.name).or_insert(l.id);
        }
        for it in items {
            let Some(&page_id) = page_ids.get(it.page) else {
                continue;
            };
            let layer = match &it.layer {
                Some(name) => Some(match layers.get(name) {
                    Some(id) => *id,
                    None => {
                        let (id, _) = self.add_layer_in_edit(name.trim(), &[])?;
                        layers.insert(name.clone(), id);
                        id
                    }
                }),
                None => None,
            };
            let mut d = it.dict;
            if let Some(l) = layer {
                d.set("OC", Object::Reference(l));
            }
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
                    // A file that does not name a layer leaves the comment on its own.
                    if it.layer.is_none() {
                        if let Ok(oc) = self.inner.get_dictionary(old_id).and_then(|o| o.get(b"OC")) {
                            d.set("OC", oc.clone());
                        }
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
                popup.remove(b"OC");
                if let Ok(oc) = self.inner.get_dictionary(id).and_then(|o| o.get(b"OC")) {
                    popup.set("OC", oc.clone());
                }
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
