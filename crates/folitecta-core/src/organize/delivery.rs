//! Copies of a document prepared to be sent (e-mail attachments): the chosen
//! pages, the comments kept, flattened into the pages or removed, protected
//! like the document. The open document is never changed: a copy is made
//! from a snapshot ([`CopySource`]), away from the interface thread.
//!
//! A copy of some pages keeps nothing of the others: their content streams,
//! the pictures, fonts and drawings only they use (the resources of the kept
//! pages are cut down to what their content names), their annotations and
//! form fields, the bookmarks, links and named destinations that lead to
//! them, their tags and article threads. What had to go is counted in
//! [`LeftOut`] so it can be shown.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use lopdf::{Dictionary, Object, ObjectId};

use crate::document::{Document, Source, serialize};
use crate::error::{Error, Result};
use crate::flatten::FlattenScope;
use crate::links::LinkTarget;
use crate::outline::OutlineItem;
use crate::security::Protection;
use crate::util::{self, deref};

/// A snapshot of a document to prepare copies from on another thread: its
/// current bytes, the file it was opened from (for incremental updates of
/// signed files) and its protection. Cheap to take.
#[derive(Clone)]
pub struct CopySource {
    bytes: Arc<Vec<u8>>,
    source: Option<Source>,
    protection: Option<Protection>,
    /// The highest object number the document gave out (an update of its
    /// file must not reuse one).
    max_id: u32,
}

impl std::fmt::Debug for CopySource {
    // Without the decrypted objects of the file it was opened from.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CopySource")
            .field("bytes", &self.bytes.len())
            .field("source", &self.source.as_ref().map(|s| s.bytes.len()))
            .field("protection", &self.protection)
            .finish()
    }
}

/// What happens to the comments of a copy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CopyComments {
    #[default]
    Keep,
    /// Drawn into the pages; the comments themselves go.
    Flatten,
    /// Removed with their pop-ups and replies.
    Remove,
}

/// How a copy is made.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CopyOptions {
    /// Zero-based pages to keep, increasing, each once; `None` keeps every page.
    pub pages: Option<Vec<usize>>,
    pub comments: CopyComments,
}

/// What became of the signatures of the document in a copy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CopySignatures {
    /// The document has no signatures.
    #[default]
    None,
    /// The copy is the signed file, or an incremental update of it: `count`
    /// signatures, of which `intact` still verify (`None`: not checked, the
    /// file is protected).
    Kept { count: usize, intact: Option<usize> },
    /// A changed or protected copy is written whole, which would break the
    /// signatures: they were removed.
    Removed(usize),
}

/// What a copy of some pages, or a document that loses pages, could not keep
/// because it belonged to, or led to, the pages left out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LeftOut {
    pub bookmarks: usize,
    pub links: usize,
    pub destinations: usize,
    pub fields: usize,
    pub tags: usize,
    /// Pictures, fonts and drawings only the other pages used.
    pub resources: usize,
    /// Layers (optional content groups) only the other pages used.
    pub layers: usize,
    /// The view the document opened with showed a page left out.
    pub open_action: bool,
    /// Article threads (they ran through pages left out).
    pub threads: bool,
    /// The XFA form data (it held the values of fields that went too).
    pub xfa: bool,
}

impl LeftOut {
    /// Nothing had to go.
    pub fn is_empty(&self) -> bool {
        *self == LeftOut::default()
    }
}

/// Facts read back from a prepared copy.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CopyFacts {
    pub pages: usize,
    pub comments: usize,
    pub fields: usize,
    /// Files attached inside the document.
    pub attachments: usize,
    /// Protected (encrypted) like the document.
    pub protected: bool,
    pub signatures: CopySignatures,
    pub left_out: LeftOut,
}

/// A copy ready to be attached.
#[derive(Debug, Clone)]
pub struct PreparedCopy {
    /// The file to attach (encrypted like the opened file when it was).
    pub bytes: Vec<u8>,
    /// The same copy without encryption, to show it; never write it to a
    /// file (FOL-L036).
    pub plain: Arc<Vec<u8>>,
    pub facts: CopyFacts,
}

/// The message of the error a copy of a protected document gets when it
/// could not be protected the same way (nothing is attached then).
pub const PROTECTION_REFUSED: &str = "the copy could not be protected like the document; it is not attached";

/// Resource categories whose entries are named by content streams.
const NAMED: [&[u8]; 7] = [
    b"Font",
    b"XObject",
    b"ExtGState",
    b"ColorSpace",
    b"Pattern",
    b"Shading",
    b"Properties",
];

/// Largest content stream read to find the names it uses.
const CONTENT_LIMIT: usize = 256 << 20;

/// The error of a preparation stopped on purpose.
const STOPPED: &str = "the preparation was stopped";

/// Go on unless `stop` is set.
fn go_on(stop: &AtomicBool) -> Result<()> {
    if stop.load(Ordering::Relaxed) {
        Err(Error::Invalid(STOPPED.into()))
    } else {
        Ok(())
    }
}

/// The written file carries an encryption dictionary in its trailer.
/// (Read from its cross-reference section and trailer, wherever they are.)
fn carries_encryption(bytes: &[u8]) -> bool {
    lopdf::Document::load_metadata_mem(bytes).is_ok_and(|m| m.encrypted)
}

/// The pages to keep, checked: `None` when every page is kept.
fn checked_pages(pages: Option<&[usize]>, n: usize) -> Result<Option<Vec<usize>>> {
    let Some(p) = pages else {
        return Ok(None);
    };
    if p.is_empty() {
        return Err(Error::LastPage);
    }
    if let Some(&bad) = p.iter().find(|&&x| x >= n) {
        return Err(Error::InvalidPage(bad));
    }
    if p.windows(2).any(|w| w[0] >= w[1]) {
        return Err(Error::Invalid("pages must be listed once, in order".into()));
    }
    Ok((p.len() != n).then(|| p.to_vec()))
}

/// The names (`/Name` tokens, `#xx` escapes decoded) a content stream uses,
/// outside strings, comments and the data of inline images.
fn content_names(data: &[u8], out: &mut HashSet<Vec<u8>>) {
    let is_ws = |c: u8| matches!(c, 0 | 9 | 10 | 12 | 13 | 32);
    let is_delim = |c: u8| {
        matches!(
            c,
            b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b'/' | b'%'
        )
    };
    let n = data.len();
    let mut i = 0;
    while i < n {
        let c = data[i];
        if is_ws(c) {
            i += 1;
        } else if c == b'%' {
            while i < n && data[i] != b'\n' && data[i] != b'\r' {
                i += 1;
            }
        } else if c == b'(' {
            let mut level = 0usize;
            while i < n {
                match data[i] {
                    b'\\' => i += 1,
                    b'(' => level += 1,
                    b')' => {
                        level = level.saturating_sub(1);
                        if level == 0 {
                            i += 1;
                            break;
                        }
                    }
                    _ => {}
                }
                i += 1;
            }
        } else if c == b'<' && data.get(i + 1) != Some(&b'<') {
            while i < n && data[i] != b'>' {
                i += 1;
            }
            i += 1;
        } else if c == b'/' {
            i += 1;
            let start = i;
            while i < n && !is_ws(data[i]) && !is_delim(data[i]) {
                i += 1;
            }
            let raw = &data[start..i];
            let mut name = Vec::with_capacity(raw.len());
            let mut j = 0;
            while j < raw.len() {
                if raw[j] == b'#' {
                    if let Some(v) = raw
                        .get(j + 1..j + 3)
                        .and_then(|h| std::str::from_utf8(h).ok())
                        .and_then(|h| u8::from_str_radix(h, 16).ok())
                    {
                        name.push(v);
                        j += 3;
                        continue;
                    }
                }
                name.push(raw[j]);
                j += 1;
            }
            out.insert(name);
        } else if is_delim(c) {
            i += 1;
        } else {
            let start = i;
            while i < n && !is_ws(data[i]) && !is_delim(data[i]) {
                i += 1;
            }
            if &data[start..i] == b"ID" {
                // The data of an inline image, up to `EI` between white space.
                let mut j = i + 1;
                while j + 2 <= n {
                    if data[j] == b'E'
                        && data[j + 1] == b'I'
                        && is_ws(data[j - 1])
                        && (j + 2 == n || is_ws(data[j + 2]) || is_delim(data[j + 2]))
                    {
                        break;
                    }
                    j += 1;
                }
                i = (j + 2).min(n);
            }
        }
    }
}

/// The decoded content of a stream object.
fn stream_content(doc: &lopdf::Document, id: ObjectId) -> Option<Vec<u8>> {
    doc.get_object(id)
        .ok()?
        .as_stream()
        .ok()?
        .get_plain_content_with_limit(CONTENT_LIMIT)
        .ok()
}

/// The streams drawing the glyphs of a Type 3 font, one after the other.
fn type3_content(doc: &lopdf::Document, font: &Dictionary) -> Option<Vec<u8>> {
    let procs = deref(doc, font.get(b"CharProcs").ok()?).as_dict().ok()?;
    let mut out = Vec::new();
    for (_, p) in procs.iter() {
        out.extend(stream_content(doc, p.as_reference().ok()?)?);
        out.push(b'\n');
    }
    Some(out)
}

/// A resource dictionary with only the named entries `used` names.
fn cut_resources(
    doc: &lopdf::Document,
    res: &Dictionary,
    used: &HashSet<Vec<u8>>,
    dropped: &mut HashSet<ObjectId>,
    kept: &mut HashSet<ObjectId>,
) -> Dictionary {
    let mut out = Dictionary::new();
    for (k, v) in res.iter() {
        let sub = NAMED
            .contains(&k.as_slice())
            .then(|| deref(doc, v).as_dict().ok())
            .flatten();
        match sub {
            Some(sub) => {
                let mut cut = Dictionary::new();
                for (name, value) in sub.iter() {
                    if used.contains(name) {
                        if let Object::Reference(r) = value {
                            kept.insert(*r);
                        }
                        cut.set(name.clone(), value.clone());
                    } else if let Object::Reference(r) = value {
                        dropped.insert(*r);
                    }
                }
                if !cut.is_empty() {
                    out.set(k.clone(), Object::Dictionary(cut));
                }
            }
            None => out.set(k.clone(), v.clone()),
        }
    }
    out
}

impl Document {
    /// A snapshot to prepare copies from (cheap: the bytes are shared).
    pub fn copy_source(&self) -> CopySource {
        CopySource {
            bytes: self.bytes(),
            source: self.source.clone(),
            protection: self.protection().cloned(),
            max_id: self.inner.max_id,
        }
    }

    /// Prepare a copy of the document as it is now (see [`CopySource::prepare`]).
    pub fn prepare_copy(&self, opts: &CopyOptions) -> Result<PreparedCopy> {
        self.copy_source().prepare(opts)
    }

    /// The document is protected with RC4 (the older security handlers),
    /// whose key stream is the same for every file written with its key: two
    /// versions of it side by side let a reader compare them.
    pub fn rc4_protection(&self) -> bool {
        self.protection().is_some_and(|p| {
            let s = p.state();
            s.get_stream_filter().method() == b"V2" || s.get_string_filter().method() == b"V2"
        })
    }

    /// Facts of the document as it is: its signatures counted as kept, and
    /// checked when the document is not protected.
    pub fn copy_facts(&self) -> CopyFacts {
        let count = self.signed_field_count();
        let signatures = if count == 0 {
            CopySignatures::None
        } else {
            let intact = (!self.is_protected())
                .then(|| self.validate_signatures(&[]).iter().filter(|s| s.valid).count());
            CopySignatures::Kept { count, intact }
        };
        CopyFacts {
            pages: self.page_count(),
            comments: self.comments().len(),
            fields: self.form_fields().len(),
            attachments: self.attachments().len(),
            protected: self.is_protected(),
            signatures,
            left_out: LeftOut::default(),
        }
    }

    /// Whether `needle` is anywhere in the objects of the document: in a
    /// string, a name or a decoded stream (to check what a copy left out).
    pub fn holds(&self, needle: &[u8]) -> bool {
        fn find(o: &Object, has: &dyn Fn(&[u8]) -> bool) -> bool {
            match o {
                Object::String(s, _) | Object::Name(s) => has(s),
                Object::Array(a) => a.iter().any(|i| find(i, has)),
                Object::Dictionary(d) => d.iter().any(|(k, v)| has(k) || find(v, has)),
                Object::Stream(s) => {
                    s.dict.iter().any(|(k, v)| has(k) || find(v, has))
                        || has(&s.get_plain_content().unwrap_or_else(|_| s.content.clone()))
                }
                _ => false,
            }
        }
        let has = |b: &[u8]| !needle.is_empty() && b.windows(needle.len()).any(|w| w == needle);
        self.inner.objects.values().any(|o| find(o, &has))
    }

    /// The annotations on the pages.
    fn page_annotation_ids(&self) -> HashSet<ObjectId> {
        let mut out = HashSet::new();
        for pid in self.page_ids() {
            if let Ok(page) = self.inner.get_dictionary(pid) {
                out.extend(
                    crate::annots::annot_refs(&self.inner, page)
                        .into_iter()
                        .filter_map(|(id, _)| id),
                );
            }
        }
        out
    }

    /// Ids of the annotations listed by a page.
    fn annots_of(&self, page: ObjectId) -> Vec<ObjectId> {
        self.inner
            .get_dictionary(page)
            .map(|d| {
                crate::annots::annot_refs(&self.inner, d)
                    .into_iter()
                    .filter_map(|(id, _)| id)
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Cut the resources of the kept pages, and of everything they draw, down
    /// to the entries their content names, so pictures, fonts and drawings
    /// only the other pages used leave the file with them (shared resource
    /// dictionaries are copied, one for each page). Content that cannot be
    /// read keeps its resources as they are. Returns how many objects no kept
    /// resource names any more.
    fn cut_page_resources(&mut self, pages: &[ObjectId], stop: &AtomicBool) -> Result<usize> {
        let mut dropped = HashSet::new();
        let mut kept = HashSet::new();
        let mut queue: Vec<ObjectId> = Vec::new();
        for &pid in pages {
            go_on(stop)?;
            let contents: Option<Vec<u8>> = self
                .inner
                .get_page_contents(pid)
                .into_iter()
                .map(|c| stream_content(&self.inner, c))
                .try_fold(Vec::new(), |mut acc, c| {
                    acc.extend(c?);
                    acc.push(b'\n');
                    Some(acc)
                });
            let res = self
                .inner
                .get_dictionary(pid)
                .ok()
                .and_then(|d| d.get(b"Resources").ok())
                .and_then(|r| deref(&self.inner, r).as_dict().ok())
                .cloned();
            if let (Some(content), Some(res)) = (contents, res) {
                let used = self.used_names(&content, &res, &mut queue);
                let cut = cut_resources(&self.inner, &res, &used, &mut dropped, &mut kept);
                if let Ok(d) = self.inner.get_dictionary_mut(pid) {
                    d.set("Resources", Object::Dictionary(cut));
                }
            }
            // The appearances of the annotations of the page.
            for aid in self.annots_of(pid) {
                let Some(ap) = self
                    .inner
                    .get_dictionary(aid)
                    .ok()
                    .and_then(|a| a.get(b"AP").ok())
                    .and_then(|ap| deref(&self.inner, ap).as_dict().ok())
                else {
                    continue;
                };
                for (_, v) in ap.iter() {
                    match deref(&self.inner, v) {
                        Object::Stream(_) => queue.extend(v.as_reference().ok()),
                        Object::Dictionary(states) => {
                            queue.extend(states.iter().filter_map(|(_, s)| s.as_reference().ok()))
                        }
                        _ => {}
                    }
                }
            }
        }
        let mut done = HashSet::new();
        while let Some(id) = queue.pop() {
            if !done.insert(id) {
                continue;
            }
            if done.len() % 64 == 0 {
                go_on(stop)?;
            }
            let (content, res) = match self.inner.get_object(id) {
                Ok(Object::Stream(s)) => {
                    let res = s
                        .dict
                        .get(b"Resources")
                        .ok()
                        .and_then(|r| deref(&self.inner, r).as_dict().ok())
                        .cloned();
                    (stream_content(&self.inner, id), res)
                }
                Ok(Object::Dictionary(font)) => {
                    let res = font
                        .get(b"Resources")
                        .ok()
                        .and_then(|r| deref(&self.inner, r).as_dict().ok())
                        .cloned();
                    (type3_content(&self.inner, font), res)
                }
                _ => continue,
            };
            let (Some(content), Some(res)) = (content, res) else {
                continue;
            };
            let used = self.used_names(&content, &res, &mut queue);
            let cut = cut_resources(&self.inner, &res, &used, &mut dropped, &mut kept);
            match self.inner.get_object_mut(id) {
                Ok(Object::Stream(s)) => s.dict.set("Resources", Object::Dictionary(cut)),
                Ok(Object::Dictionary(d)) => d.set("Resources", Object::Dictionary(cut)),
                _ => {}
            }
        }
        Ok(dropped.difference(&kept).count())
    }

    /// The names a content uses with the resources `res`. Forms, patterns and
    /// Type 3 fonts it names that have resources of their own are queued, to
    /// be cut the same way; those without draw with `res`, so their names
    /// count here.
    fn used_names(&self, content: &[u8], res: &Dictionary, queue: &mut Vec<ObjectId>) -> HashSet<Vec<u8>> {
        let doc = &self.inner;
        let sub = |cat: &[u8]| res.get(cat).ok().and_then(|s| deref(doc, s).as_dict().ok());
        let mut used = HashSet::new();
        let mut inlined = HashSet::new();
        let mut pending = vec![content.to_vec()];
        while let Some(data) = pending.pop() {
            let mut names = HashSet::new();
            content_names(&data, &mut names);
            for name in names {
                if !used.insert(name.clone()) {
                    continue;
                }
                for cat in [&b"XObject"[..], b"Pattern", b"Font"] {
                    let Some(target) = sub(cat)
                        .and_then(|d| d.get(&name).ok())
                        .and_then(|o| o.as_reference().ok())
                    else {
                        continue;
                    };
                    match doc.get_object(target) {
                        Ok(Object::Stream(s)) => {
                            let form = s.dict.get(b"Subtype").ok().and_then(|t| t.as_name().ok())
                                == Some(b"Form".as_slice())
                                || s.dict.has(b"PatternType");
                            if s.dict.has(b"Resources") {
                                queue.push(target);
                            } else if form && inlined.insert(target) {
                                pending.extend(stream_content(doc, target));
                            }
                        }
                        Ok(Object::Dictionary(font))
                            if font.get(b"Subtype").ok().and_then(|t| t.as_name().ok())
                                == Some(b"Type3".as_slice()) =>
                        {
                            if font.has(b"Resources") {
                                queue.push(target);
                            } else if inlined.insert(target) {
                                pending.extend(type3_content(doc, font));
                            }
                        }
                        _ => {}
                    }
                }
                // The group of a soft mask draws with resources of its own.
                if let Some(group) = sub(b"ExtGState")
                    .and_then(|d| d.get(&name).ok())
                    .and_then(|g| deref(doc, g).as_dict().ok())
                    .and_then(|g| g.get(b"SMask").ok())
                    .and_then(|m| deref(doc, m).as_dict().ok())
                    .and_then(|m| m.get(b"G").ok())
                    .and_then(|g| g.as_reference().ok())
                {
                    queue.push(group);
                }
            }
        }
        used
    }

    /// Keep only `pages` (zero-based, increasing, each once) and everything
    /// that belongs to them; nothing of the other pages stays in the file.
    /// Bookmarks, links and named destinations leading to the other pages,
    /// their form fields, tags and article threads go, and are counted; page
    /// labels stay as they were shown; layers only they used and the XFA
    /// form data (when fields go) go too. No undo step: for work copies.
    /// `stop` ends the work early (with an error).
    pub(crate) fn keep_only_pages(&mut self, pages: &[usize], stop: &AtomicBool) -> Result<LeftOut> {
        let n = self.page_count();
        let Some(pages) = checked_pages(Some(pages), n)? else {
            return Ok(LeftOut::default());
        };
        let mut left = LeftOut::default();
        let ids = self.page_ids();
        let keep: HashSet<usize> = pages.iter().copied().collect();
        let kept_ids: Vec<ObjectId> = pages.iter().map(|&p| ids[p]).collect();
        let gone_pages: HashSet<ObjectId> = ids
            .iter()
            .enumerate()
            .filter(|(i, _)| !keep.contains(i))
            .map(|(_, id)| *id)
            .collect();
        let page_gone = |p: Option<usize>| p.is_some_and(|p| !keep.contains(&p));

        // Everything is read in the numbering of the document as it was.
        let outline = self.outline();
        let gone_dests: Vec<Vec<u8>> = self
            .named_dest_map()
            .into_iter()
            .filter(|(_, v)| page_gone(self.resolve_destination(v)))
            .map(|(k, _)| k)
            .collect();
        let mut gone_links: Vec<(ObjectId, usize)> = Vec::new();
        for &p in &pages {
            for l in self.links(p)? {
                if matches!(l.target, LinkTarget::Page(t) if !keep.contains(&t)) {
                    gone_links.push((ids[p], l.index));
                }
            }
        }
        let on_kept: HashSet<ObjectId> = kept_ids.iter().flat_map(|p| self.annots_of(*p)).collect();
        let mut gone_annots: HashSet<ObjectId> = gone_pages
            .iter()
            .flat_map(|p| self.annots_of(*p))
            .filter(|a| !on_kept.contains(a))
            .collect();
        let widget_page: HashMap<ObjectId, ObjectId> = ids
            .iter()
            .flat_map(|p| self.annots_of(*p).into_iter().map(move |a| (a, *p)))
            .collect();
        let fields = self.form_fields();
        let layers_used = self.used_layers();
        let catalog = self.inner.catalog()?.clone();
        let open_action_gone = catalog.get(b"OpenAction").ok().is_some_and(|a| {
            let target = match deref(&self.inner, a) {
                Object::Dictionary(d)
                    if d.get(b"S").ok().and_then(|s| s.as_name().ok()) == Some(b"GoTo".as_slice()) =>
                {
                    d.get(b"D").ok()
                }
                Object::Dictionary(_) => None,
                _ => Some(a),
            };
            page_gone(target.and_then(|t| self.resolve_destination(t)))
        });
        let threads_gone = catalog
            .get(b"Threads")
            .ok()
            .and_then(|t| deref(&self.inner, t).as_array().ok())
            .is_some_and(|threads| {
                threads.iter().any(|t| {
                    let mut bead = deref(&self.inner, t)
                        .as_dict()
                        .ok()
                        .and_then(|t| t.get(b"F").ok())
                        .and_then(|f| f.as_reference().ok());
                    let first = bead;
                    for _ in 0..10_000 {
                        let Some(b) = bead.and_then(|b| self.inner.get_dictionary(b).ok()) else {
                            break;
                        };
                        if b.get(b"P")
                            .ok()
                            .and_then(|p| p.as_reference().ok())
                            .is_some_and(|p| gone_pages.contains(&p))
                        {
                            return true;
                        }
                        bead = b.get(b"N").ok().and_then(|n| n.as_reference().ok());
                        if bead == first {
                            break;
                        }
                    }
                    false
                })
            });
        let labels = catalog.has(b"PageLabels").then(|| self.page_label_parts());

        // Links to the other pages leave the kept pages.
        let mut by_page: HashMap<ObjectId, Vec<usize>> = HashMap::new();
        for (pid, index) in &gone_links {
            by_page.entry(*pid).or_default().push(*index);
        }
        for (pid, indices) in by_page {
            // The positions are those of the `/Annots` array as it is written.
            let raw: Vec<Object> = self
                .inner
                .get_dictionary(pid)
                .ok()
                .and_then(|d| d.get(b"Annots").ok())
                .and_then(|a| deref(&self.inner, a).as_array().ok())
                .cloned()
                .unwrap_or_default();
            for &i in &indices {
                match raw.get(i) {
                    Some(Object::Reference(id)) => {
                        gone_annots.insert(*id);
                        left.links += 1;
                    }
                    Some(_) => left.links += 1,
                    None => {}
                }
            }
            let keep_list: Vec<Object> = raw
                .into_iter()
                .enumerate()
                .filter(|(i, _)| !indices.contains(i))
                .map(|(_, o)| o)
                .collect();
            self.inner
                .get_dictionary_mut(pid)?
                .set("Annots", Object::Array(keep_list));
        }

        self.write_page_tree(&kept_ids)?;

        // Bookmarks leading to the other pages go; their children move up.
        fn keep_items(
            items: Vec<OutlineItem>,
            gone: &dyn Fn(Option<usize>) -> bool,
            dropped: &mut usize,
        ) -> Vec<OutlineItem> {
            let mut out = Vec::new();
            for mut item in items {
                let children = keep_items(std::mem::take(&mut item.children), gone, dropped);
                if gone(item.page) {
                    *dropped += 1;
                    out.extend(children);
                } else {
                    item.children = children;
                    out.push(item);
                }
            }
            out
        }
        let items = keep_items(outline, &page_gone, &mut left.bookmarks);
        if left.bookmarks > 0 {
            self.write_outline(&items)?;
        }
        if !gone_dests.is_empty() {
            left.destinations = self.delete_named_destination_keys(&gone_dests)?;
        }

        // Form fields: a field whose widgets were all on other pages goes,
        // with the parents left without kids.
        // A widget no page lists is on the page its `/P` names.
        let all_pages: HashSet<ObjectId> = ids.iter().copied().collect();
        let page_of = |w: &ObjectId| -> Option<ObjectId> {
            widget_page.get(w).copied().or_else(|| {
                self.inner
                    .get_dictionary(*w)
                    .ok()?
                    .get(b"P")
                    .ok()?
                    .as_reference()
                    .ok()
                    .filter(|p| all_pages.contains(p))
            })
        };
        let mut gone_fields: HashSet<ObjectId> = HashSet::new();
        let mut fields_touched = false;
        for f in &fields {
            let pages_of: Vec<Option<ObjectId>> = f.widgets.iter().map(&page_of).collect();
            let on_kept_page = pages_of
                .iter()
                .any(|p| p.is_some_and(|p| !gone_pages.contains(&p)));
            let on_gone_page = pages_of
                .iter()
                .any(|p| p.is_some_and(|p| gone_pages.contains(&p)));
            fields_touched |= on_gone_page;
            if on_gone_page && !on_kept_page {
                gone_fields.insert(f.id);
                gone_fields.extend(f.widgets.iter().copied());
                left.fields += 1;
            }
        }
        let mut parents: Vec<ObjectId> = gone_fields
            .iter()
            .filter_map(|f| self.inner.get_dictionary(*f).ok())
            .filter_map(|d| d.get(b"Parent").ok().and_then(|p| p.as_reference().ok()))
            .collect();
        while let Some(p) = parents.pop() {
            if gone_fields.contains(&p) {
                continue;
            }
            let Ok(d) = self.inner.get_dictionary(p) else {
                continue;
            };
            let kids: Vec<ObjectId> = d
                .get(b"Kids")
                .ok()
                .and_then(|k| deref(&self.inner, k).as_array().ok())
                .map(|a| a.iter().filter_map(|k| k.as_reference().ok()).collect())
                .unwrap_or_default();
            if !kids.is_empty()
                && kids
                    .iter()
                    .all(|k| gone_fields.contains(k) || gone_annots.contains(k))
            {
                gone_fields.insert(p);
                parents.extend(d.get(b"Parent").ok().and_then(|q| q.as_reference().ok()));
            }
        }

        // Tags of the other pages and of the annotations that went.
        let mut gone_struct_annots = gone_annots.clone();
        gone_struct_annots.extend(gone_fields.iter().copied());
        let removed = self.prune_structure(&gone_pages, &gone_struct_annots)?;
        left.tags = removed.len();

        // The catalog: opening view, article threads, page labels.
        if open_action_gone {
            self.inner.catalog_mut()?.remove(b"OpenAction");
            left.open_action = true;
        }
        if threads_gone {
            self.inner.catalog_mut()?.remove(b"Threads");
            for pid in &kept_ids {
                if let Ok(d) = self.inner.get_dictionary_mut(*pid) {
                    d.remove(b"B");
                }
            }
            left.threads = true;
        }
        if let Some(parts) = labels {
            let mut ranges: Vec<(usize, String, String, i64)> = Vec::new();
            let mut last: Option<(Vec<u8>, String, i64)> = None;
            for (i, &p) in pages.iter().enumerate() {
                let Some((style, prefix, value)) = parts.get(p).cloned() else {
                    continue;
                };
                let goes_on = last.as_ref().is_some_and(|(s, x, v)| {
                    *s == style && *x == prefix && (style.is_empty() || *v + 1 == value)
                });
                if !goes_on {
                    let first = if style.is_empty() { 1 } else { value.max(1) };
                    ranges.push((
                        i,
                        String::from_utf8_lossy(&style).to_string(),
                        prefix.clone(),
                        first,
                    ));
                }
                last = Some((style, prefix, value));
            }
            let refs: Vec<(usize, &str, &str, i64)> = ranges
                .iter()
                .map(|(i, s, x, v)| (*i, s.as_str(), x.as_str(), *v))
                .collect();
            self.write_page_labels(&refs)?;
        }

        // The XFA form data holds the values of the fields that went.
        if fields_touched {
            left.xfa = self.remove_xfa()?;
        }

        // Resources only the other pages used, then every reference left.
        left.resources = self.cut_page_resources(&kept_ids, stop)?;
        let mut gone: HashSet<ObjectId> = gone_pages;
        gone.extend(gone_annots);
        gone.extend(gone_fields);
        gone.extend(removed);
        util::drop_references(&mut self.inner, &gone);
        for id in &gone {
            self.inner.objects.remove(id);
        }
        util::prune_unreachable(&mut self.inner);
        // Layers only the other pages used.
        let still = self.used_layers();
        let gone_layers: HashSet<ObjectId> = layers_used.difference(&still).copied().collect();
        if !gone_layers.is_empty() {
            left.layers = self.remove_layers(&gone_layers)?;
            util::prune_unreachable(&mut self.inner);
        }
        Ok(left)
    }

    /// The optional content groups of `/OCProperties /OCGs`.
    fn layer_ids(&self) -> HashSet<ObjectId> {
        let doc = &self.inner;
        doc.catalog()
            .ok()
            .and_then(|c| c.get(b"OCProperties").ok())
            .and_then(|o| deref(doc, o).as_dict().ok())
            .and_then(|o| o.get(b"OCGs").ok())
            .and_then(|a| deref(doc, a).as_array().ok())
            .map(|a| a.iter().filter_map(|o| o.as_reference().ok()).collect())
            .unwrap_or_default()
    }

    /// The objects written for `/OCProperties` itself (its configurations
    /// and lists), the layers excepted.
    fn layer_settings_objects(&self, layers: &HashSet<ObjectId>) -> HashSet<ObjectId> {
        let doc = &self.inner;
        let mut out = HashSet::new();
        let mut stack: Vec<Object> = doc
            .catalog()
            .ok()
            .and_then(|c| c.get(b"OCProperties").ok())
            .cloned()
            .into_iter()
            .collect();
        while let Some(o) = stack.pop() {
            match o {
                Object::Reference(r) if !layers.contains(&r) && out.insert(r) => {
                    stack.extend(doc.get_object(r).ok().cloned());
                }
                Object::Array(a) => stack.extend(a),
                Object::Dictionary(d) => stack.extend(d.iter().map(|(_, v)| v.clone())),
                _ => {}
            }
        }
        out
    }

    /// The layers something of the document uses (content, pictures,
    /// annotations, membership dictionaries): not only listed by
    /// `/OCProperties`.
    fn used_layers(&self) -> HashSet<ObjectId> {
        let layers = self.layer_ids();
        if layers.is_empty() {
            return layers;
        }
        let settings = self.layer_settings_objects(&layers);
        let catalog = self
            .inner
            .trailer
            .get(b"Root")
            .and_then(|r| r.as_reference())
            .ok();
        fn refs(o: &Object, layers: &HashSet<ObjectId>, out: &mut HashSet<ObjectId>) {
            match o {
                Object::Reference(r) if layers.contains(r) => {
                    out.insert(*r);
                }
                Object::Array(a) => a.iter().for_each(|i| refs(i, layers, out)),
                Object::Dictionary(d) => d.iter().for_each(|(_, v)| refs(v, layers, out)),
                Object::Stream(s) => s.dict.iter().for_each(|(_, v)| refs(v, layers, out)),
                _ => {}
            }
        }
        let mut used = HashSet::new();
        for (id, o) in &self.inner.objects {
            if layers.contains(id) || settings.contains(id) {
                continue;
            }
            if Some(*id) == catalog {
                if let Object::Dictionary(d) = o {
                    for (k, v) in d.iter() {
                        if k != b"OCProperties" {
                            refs(v, &layers, &mut used);
                        }
                    }
                }
                continue;
            }
            refs(o, &layers, &mut used);
        }
        used
    }

    /// Take the `gone` layers out of `/OCProperties` (the list of layers,
    /// the order, the layers on and off, radio-button groups and usage
    /// settings of every configuration); without layers left it goes.
    /// Returns how many layers went.
    fn remove_layers(&mut self, gone: &HashSet<ObjectId>) -> Result<usize> {
        fn strip(o: &mut Object, gone: &HashSet<ObjectId>) {
            match o {
                Object::Array(items) => {
                    items.retain(|i| !matches!(i, Object::Reference(r) if gone.contains(r)));
                    for i in items.iter_mut() {
                        strip(i, gone);
                    }
                    // Groups of the order left with a label only, empty
                    // radio-button groups, usage settings without layers.
                    items.retain(|i| match i {
                        Object::Array(a) => a.iter().any(|x| !matches!(x, Object::String(..))),
                        Object::Dictionary(d) => d
                            .get(b"OCGs")
                            .map(|g| !matches!(g, Object::Array(a) if a.is_empty()))
                            .unwrap_or(true),
                        _ => true,
                    });
                }
                Object::Dictionary(d) => {
                    for (_, v) in d.iter_mut() {
                        strip(v, gone);
                    }
                }
                _ => {}
            }
        }
        let layers = self.layer_ids();
        let count = layers.intersection(gone).count();
        for id in self.layer_settings_objects(&layers) {
            if let Some(o) = self.inner.objects.get_mut(&id) {
                strip(o, gone);
            }
        }
        let catalog = self.inner.catalog_mut()?;
        if let Ok(o) = catalog.get_mut(b"OCProperties") {
            strip(o, gone);
        }
        if self.layer_ids().is_empty() {
            self.inner.catalog_mut()?.remove(b"OCProperties");
        }
        Ok(count)
    }

    /// Remove the XFA form data (and the need to render it) from the form.
    /// Returns whether there was any.
    pub(crate) fn remove_xfa(&mut self) -> Result<bool> {
        let form = self.inner.catalog()?.get(b"AcroForm").ok().cloned();
        let had = match form {
            Some(Object::Reference(id)) => self
                .inner
                .get_dictionary_mut(id)
                .map(|d| d.remove(b"XFA").is_some())
                .unwrap_or(false),
            Some(Object::Dictionary(_)) => match self.inner.catalog_mut()?.get_mut(b"AcroForm") {
                Ok(Object::Dictionary(d)) => d.remove(b"XFA").is_some(),
                _ => false,
            },
            _ => false,
        };
        if had {
            self.inner.catalog_mut()?.remove(b"NeedsRendering");
        }
        Ok(had)
    }
}

impl CopySource {
    /// Prepare a copy (see [`CopySource::prepare_until`]).
    pub fn prepare(&self, opts: &CopyOptions) -> Result<PreparedCopy> {
        self.prepare_until(opts, &AtomicBool::new(false))
    }

    /// Prepare a copy with `opts`. A copy without changes of a signed file
    /// is the file itself or an incremental update of it (the signatures
    /// stay valid); any other copy is written whole, and the signatures it
    /// would break are removed. A copy of a protected document is protected
    /// the same way (same passwords and permissions), or refused. `stop`
    /// ends the work early (with an error) when it is set.
    pub fn prepare_until(&self, opts: &CopyOptions, stop: &AtomicBool) -> Result<PreparedCopy> {
        let check = || go_on(stop);
        let mut work = Document::from_bytes(self.bytes.to_vec(), None, None)?;
        // Changed in place and never committed: nothing read from it is kept.
        work.changing();
        work.source = self.source.clone();
        work.set_protection(self.protection.clone());
        work.inner.max_id = work.inner.max_id.max(self.max_id);
        let pages = checked_pages(opts.pages.as_deref(), work.page_count())?;
        let changes = pages.is_some() || opts.comments != CopyComments::Keep;
        let incremental = !changes && work.updates_incrementally();
        let signed = work.signed_field_count();
        check()?;
        let mut signatures = CopySignatures::None;
        if signed > 0 && !incremental {
            signatures = CopySignatures::Removed(work.clear_signatures_inner());
        }
        if changes {
            // Written whole: an earlier revision would keep what was taken out.
            work.source = None;
        }
        let left_out = match &pages {
            Some(p) => work.keep_only_pages(p, stop)?,
            None => LeftOut::default(),
        };
        check()?;
        if opts.comments != CopyComments::Keep {
            let before = work.page_annotation_ids();
            let comment = |s: &[u8]| {
                !crate::annots::NON_COMMENT_SUBTYPES
                    .iter()
                    .any(|n| n.as_bytes() == s)
            };
            // Page by page, so a stop is seen soon; the objects that go are
            // taken out of the file in one pass at the end.
            let mut gone = HashSet::new();
            for p in 0..work.page_count() {
                check()?;
                match opts.comments {
                    CopyComments::Flatten => {
                        work.flatten_where(&[p], FlattenScope::Comments, None, &comment, &mut gone)?;
                    }
                    CopyComments::Remove => {
                        work.delete_comments_inner(&[p])?;
                    }
                    CopyComments::Keep => {}
                }
            }
            let after = work.page_annotation_ids();
            gone.extend(before.difference(&after).copied());
            let removed = work.prune_structure(&HashSet::new(), &gone)?;
            gone.extend(removed);
            util::drop_references(&mut work.inner, &gone);
            for id in &gone {
                work.inner.objects.remove(id);
            }
        }
        if changes || matches!(signatures, CopySignatures::Removed(_)) {
            util::prune_unreachable(&mut work.inner);
        }
        check()?;
        // FOL-L036: protected with the kept encryption of the document (the
        // way it is saved), or refused; never a plain copy.
        let protected = self.protection.is_some();
        let bytes = match work.file_bytes_from().map(|f| f.bytes) {
            Ok(b) if !protected || carries_encryption(&b) => b,
            Ok(_) => return Err(Error::Invalid(PROTECTION_REFUSED.into())),
            Err(_) if protected => return Err(Error::Invalid(PROTECTION_REFUSED.into())),
            Err(e) => return Err(e),
        };
        let plain = if protected {
            Arc::new(serialize(&mut work.inner)?)
        } else {
            Arc::new(bytes.clone())
        };
        if incremental && signed > 0 {
            // Checked when the copy opens without a password.
            let intact = match protected {
                false => {
                    let copy = Document::from_bytes(bytes.clone(), None, None)?;
                    Some(copy.validate_signatures(&[]).iter().filter(|s| s.valid).count())
                }
                true => None,
            };
            signatures = CopySignatures::Kept {
                count: signed,
                intact,
            };
        }
        Ok(PreparedCopy {
            facts: CopyFacts {
                pages: work.page_count(),
                comments: work.comments().len(),
                fields: work.form_fields().len(),
                attachments: work.attachments().len(),
                protected,
                signatures,
                left_out,
            },
            bytes,
            plain,
        })
    }

    /// Facts and plain bytes of the snapshot as it is (for a saved file
    /// that only opens with its password, when the document is that file).
    pub fn inspect(&self) -> Result<(CopyFacts, Arc<Vec<u8>>)> {
        let mut d = Document::from_bytes(self.bytes.to_vec(), None, None)?;
        d.source = self.source.clone();
        d.set_protection(self.protection.clone());
        Ok((d.copy_facts(), self.bytes.clone()))
    }

    /// The document is protected.
    pub fn is_protected(&self) -> bool {
        self.protection.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_of_content_skip_strings_and_inline_images() {
        let mut names = HashSet::new();
        content_names(
            b"q /Im#31 Do (not /A name) Tj <2F42> Tj BI /W 1 /H 1 /CS /G ID \x2f\x58 EI /F2 12 Tf % /C comment\n/Sh sh",
            &mut names,
        );
        let mut got: Vec<String> = names
            .iter()
            .map(|n| String::from_utf8_lossy(n).to_string())
            .collect();
        got.sort();
        assert_eq!(got, ["CS", "F2", "G", "H", "Im1", "Sh", "W"]);
    }

    #[test]
    fn page_lists_are_checked() {
        assert!(matches!(checked_pages(Some(&[]), 3), Err(Error::LastPage)));
        assert!(matches!(checked_pages(Some(&[3]), 3), Err(Error::InvalidPage(3))));
        assert!(matches!(checked_pages(Some(&[1, 0]), 3), Err(Error::Invalid(_))));
        assert!(matches!(checked_pages(Some(&[1, 1]), 3), Err(Error::Invalid(_))));
        assert_eq!(checked_pages(Some(&[0, 1, 2]), 3).unwrap(), None);
        assert_eq!(checked_pages(Some(&[0, 2]), 3).unwrap(), Some(vec![0, 2]));
    }
}
