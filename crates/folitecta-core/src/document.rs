//! The undo-aware document model.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use lopdf::{Dictionary, Object, ObjectId, Stream, dictionary};

use crate::error::{Error, Result};
use crate::util::{self, deref, dict_text, num, rect_of};

const UNDO_LIMIT: usize = 40;

/// Display size of a page in PDF points (after applying `/Rotate`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PageSize {
    pub width: f32,
    pub height: f32,
    /// Rotation in degrees (0, 90, 180, 270).
    pub rotation: i32,
    /// Media box in PDF user space.
    pub media_box: [f32; 4],
    /// Crop box in PDF user space (equals the media box when absent).
    pub crop_box: [f32; 4],
}

/// Document-level metadata.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DocInfo {
    pub title: String,
    pub author: String,
    pub subject: String,
    pub keywords: String,
    pub creator: String,
    pub producer: String,
    pub created: String,
    pub modified: String,
    pub version: String,
    pub page_count: usize,
    pub file_size: u64,
    pub encrypted: bool,
    pub has_form: bool,
    pub tagged: bool,
    pub path: Option<PathBuf>,
}

/// A PDF document with file association, serialized snapshot and undo history.
pub struct Document {
    pub(crate) inner: lopdf::Document,
    path: Option<PathBuf>,
    bytes: Arc<Vec<u8>>,
    revision: u64,
    dirty: bool,
    undo: Vec<Arc<Vec<u8>>>,
    redo: Vec<Arc<Vec<u8>>>,
    /// Object id of the shared Helvetica font resource created by Folitecta, if any.
    pub(crate) helv_font: Option<ObjectId>,
    /// The file was encrypted when it was opened (it is kept decrypted in memory).
    source_encrypted: bool,
    /// Positioned text of every page, keyed by revision.
    pub(crate) text_cache: std::sync::Mutex<Option<(u64, Arc<Vec<crate::textlayer::PageText>>)>>,
}

impl std::fmt::Debug for Document {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Document")
            .field("path", &self.path)
            .field("pages", &self.page_count())
            .field("revision", &self.revision)
            .field("dirty", &self.dirty)
            .finish()
    }
}

fn serialize(doc: &mut lopdf::Document) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    doc.save_to(&mut out)?;
    Ok(out)
}

fn load_inner(bytes: &[u8], password: Option<&str>) -> Result<(lopdf::Document, bool)> {
    let mut doc = match password {
        Some(pw) => lopdf::Document::load_mem_with_options(bytes, lopdf::LoadOptions::with_password(pw))?,
        None => lopdf::Document::load_mem(bytes)?,
    };
    if doc.is_encrypted() && !doc.was_encrypted() {
        doc.decrypt(password.unwrap_or(""))
            .map_err(|_| Error::PasswordRequired)?;
    }
    let encrypted = doc.was_encrypted() || doc.is_encrypted();
    if encrypted {
        // We keep the decrypted objects and write plain files from now on.
        doc.trailer.remove(b"Encrypt");
        doc.encryption_state = None;
    }
    Ok((doc, encrypted))
}

impl Document {
    // ------------------------------------------------------------------
    // Construction and persistence
    // ------------------------------------------------------------------

    /// Open a PDF file.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let bytes = std::fs::read(path).map_err(|e| Error::io(path, e))?;
        Self::from_bytes(bytes, Some(path.to_path_buf()), None)
    }

    /// Open a password protected PDF file.
    pub fn open_with_password(path: impl AsRef<Path>, password: &str) -> Result<Self> {
        let path = path.as_ref();
        let bytes = std::fs::read(path).map_err(|e| Error::io(path, e))?;
        Self::from_bytes(bytes, Some(path.to_path_buf()), Some(password))
    }

    /// Build a document from in-memory bytes.
    pub fn from_bytes(bytes: Vec<u8>, path: Option<PathBuf>, password: Option<&str>) -> Result<Self> {
        let (inner, source_encrypted) = load_inner(&bytes, password)?;
        let mut doc = Document {
            inner,
            path,
            bytes: Arc::new(bytes),
            revision: 0,
            dirty: false,
            undo: Vec::new(),
            redo: Vec::new(),
            helv_font: None,
            source_encrypted,
            text_cache: Default::default(),
        };
        if doc.page_count() == 0 {
            return Err(Error::Invalid("the file contains no pages".into()));
        }
        // Encrypted sources are re-serialized so the renderer sees plain objects.
        if source_encrypted {
            doc.bytes = Arc::new(serialize(&mut doc.inner)?);
        }
        Ok(doc)
    }

    /// Create a new document with `pages` blank pages of the given size (points).
    pub fn new_blank(pages: usize, width: f32, height: f32) -> Result<Self> {
        let pages = pages.max(1);
        let mut inner = lopdf::Document::with_version("1.7");
        let pages_id = inner.new_object_id();
        let mut kids = Vec::with_capacity(pages);
        for _ in 0..pages {
            let content_id = inner.add_object(Stream::new(dictionary! {}, Vec::new()));
            let page_id = inner.add_object(dictionary! {
                "Type" => "Page",
                "Parent" => pages_id,
                "MediaBox" => vec![0.into(), 0.into(), width.into(), height.into()],
                "Contents" => content_id,
                "Resources" => dictionary! {},
            });
            kids.push(Object::Reference(page_id));
        }
        inner.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => kids,
                "Count" => pages as i64,
            }),
        );
        let catalog_id = inner.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
        let info_id = inner.add_object(dictionary! {
            "Producer" => lopdf::text_string(&format!("{} {}", crate::APP_NAME, crate::VERSION)),
            "Creator" => lopdf::text_string(crate::APP_NAME),
            "CreationDate" => Object::string_literal(util::pdf_date_now()),
        });
        inner.trailer.set("Root", catalog_id);
        inner.trailer.set("Info", info_id);
        let bytes = serialize(&mut inner)?;
        Ok(Document {
            inner,
            path: None,
            bytes: Arc::new(bytes),
            revision: 0,
            dirty: true,
            undo: Vec::new(),
            redo: Vec::new(),
            helv_font: None,
            source_encrypted: false,
            text_cache: Default::default(),
        })
    }

    /// Save to the current path.
    pub fn save(&mut self) -> Result<()> {
        let path = self.path.clone().ok_or(Error::NoPath)?;
        self.save_as(&path)
    }

    /// Save to `path` and make it the document path.
    pub fn save_as(&mut self, path: impl AsRef<Path>) -> Result<()> {
        let path = path.as_ref();
        self.touch_mod_date();
        self.bytes = Arc::new(serialize(&mut self.inner)?);
        std::fs::write(path, self.bytes.as_slice()).map_err(|e| Error::io(path, e))?;
        self.path = Some(path.to_path_buf());
        self.dirty = false;
        Ok(())
    }

    /// Write a copy to `path` without changing the document path or dirty state.
    pub fn save_copy(&mut self, path: impl AsRef<Path>) -> Result<()> {
        let path = path.as_ref();
        let bytes = serialize(&mut self.inner)?;
        std::fs::write(path, &bytes).map_err(|e| Error::io(path, e))
    }

    // ------------------------------------------------------------------
    // Identity and state
    // ------------------------------------------------------------------

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    pub fn set_path(&mut self, path: Option<PathBuf>) {
        self.path = path;
    }

    /// Display name: file name or "Untitled".
    pub fn display_name(&self) -> String {
        self.path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "Untitled".to_string())
    }

    /// Serialized bytes matching the current state (used by the renderer).
    pub fn bytes(&self) -> Arc<Vec<u8>> {
        self.bytes.clone()
    }

    /// Monotonic revision counter, incremented on every change.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    /// Forget the undo/redo history (for example right after opening a file).
    pub fn clear_history(&mut self) {
        self.undo.clear();
        self.redo.clear();
    }

    /// Mark the current state as saved without writing a file (used by callers
    /// that persist the bytes themselves).
    pub fn mark_clean(&mut self) {
        self.dirty = false;
    }

    /// Merge the last two edits into one undo step (for operations built from
    /// two primitive edits).
    pub(crate) fn merge_last_undo(&mut self) {
        if self.undo.len() >= 2 {
            self.undo.pop();
        }
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Access the underlying object model.
    pub fn inner(&self) -> &lopdf::Document {
        &self.inner
    }

    /// Mutable access to the underlying object model. Call [`Document::commit`]
    /// afterwards so the serialized snapshot and revision stay in sync.
    pub fn inner_mut(&mut self) -> &mut lopdf::Document {
        &mut self.inner
    }

    // ------------------------------------------------------------------
    // Undo / redo
    // ------------------------------------------------------------------

    /// Record the current state before a mutation.
    pub fn begin_edit(&mut self) {
        self.undo.push(self.bytes.clone());
        if self.undo.len() > UNDO_LIMIT {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    /// Re-serialize after a mutation and bump the revision.
    pub fn commit(&mut self) -> Result<()> {
        self.bytes = Arc::new(serialize(&mut self.inner)?);
        self.revision += 1;
        self.dirty = true;
        self.helv_font = None;
        Ok(())
    }

    fn restore(&mut self, bytes: Arc<Vec<u8>>) -> Result<()> {
        self.inner = load_inner(&bytes, None)?.0;
        self.bytes = bytes;
        self.revision += 1;
        self.dirty = true;
        self.helv_font = None;
        Ok(())
    }

    pub fn undo(&mut self) -> Result<()> {
        let prev = self.undo.pop().ok_or(Error::NothingToUndo)?;
        self.redo.push(self.bytes.clone());
        self.restore(prev)
    }

    pub fn redo(&mut self) -> Result<()> {
        let next = self.redo.pop().ok_or(Error::NothingToRedo)?;
        self.undo.push(self.bytes.clone());
        self.restore(next)
    }

    // ------------------------------------------------------------------
    // Pages
    // ------------------------------------------------------------------

    pub fn page_count(&self) -> usize {
        self.inner.get_pages().len()
    }

    /// Object ids of all pages in document order.
    pub fn page_ids(&self) -> Vec<ObjectId> {
        self.inner.get_pages().into_values().collect()
    }

    /// Object id of a zero-based page index.
    pub fn page_id(&self, index: usize) -> Result<ObjectId> {
        self.page_ids()
            .get(index)
            .copied()
            .ok_or(Error::InvalidPage(index))
    }

    /// Zero-based index of a page object id.
    pub fn page_index(&self, id: ObjectId) -> Option<usize> {
        self.page_ids().iter().position(|p| *p == id)
    }

    /// Look up a page attribute, following the `/Parent` chain for inheritable keys.
    pub fn inherited_attr(&self, page_id: ObjectId, key: &[u8]) -> Option<Object> {
        let mut current = Some(page_id);
        let mut guard = 0;
        while let Some(id) = current {
            guard += 1;
            if guard > 64 {
                break;
            }
            let dict = self.inner.get_dictionary(id).ok()?;
            if let Ok(v) = dict.get(key) {
                return Some(deref(&self.inner, v).clone());
            }
            current = dict.get(b"Parent").ok().and_then(|p| p.as_reference().ok());
        }
        None
    }

    pub fn page_size(&self, index: usize) -> Result<PageSize> {
        let id = self.page_id(index)?;
        Ok(self.page_size_of(id))
    }

    pub(crate) fn page_size_of(&self, id: ObjectId) -> PageSize {
        let media = self
            .inherited_attr(id, b"MediaBox")
            .and_then(|o| rect_of(&self.inner, &o))
            .filter(|r| r[2] - r[0] > 1.0 && r[3] - r[1] > 1.0)
            .unwrap_or([0.0, 0.0, 612.0, 792.0]);
        let crop = self
            .inherited_attr(id, b"CropBox")
            .and_then(|o| rect_of(&self.inner, &o))
            .map(|c| {
                [
                    c[0].max(media[0]),
                    c[1].max(media[1]),
                    c[2].min(media[2]),
                    c[3].min(media[3]),
                ]
            })
            .filter(|c| c[2] - c[0] > 1.0 && c[3] - c[1] > 1.0)
            .unwrap_or(media);
        let rotation = self
            .inherited_attr(id, b"Rotate")
            .and_then(|o| num(&o))
            .map(|r| ((r.round() as i32 % 360) + 360) % 360)
            .unwrap_or(0);
        let (w, h) = (crop[2] - crop[0], crop[3] - crop[1]);
        let (width, height) = if rotation == 90 || rotation == 270 {
            (h, w)
        } else {
            (w, h)
        };
        PageSize {
            width,
            height,
            rotation,
            media_box: media,
            crop_box: crop,
        }
    }

    pub fn page_sizes(&self) -> Vec<PageSize> {
        self.page_ids()
            .into_iter()
            .map(|id| self.page_size_of(id))
            .collect()
    }

    /// Copy inheritable attributes from the page tree onto each page so the tree
    /// can be restructured safely.
    pub(crate) fn inherit_attributes(&mut self, ids: &[ObjectId]) {
        for &id in ids {
            for key in [&b"Resources"[..], b"MediaBox", b"CropBox", b"Rotate"] {
                let has = self.inner.get_dictionary(id).map(|d| d.has(key)).unwrap_or(true);
                if has {
                    continue;
                }
                if let Some(value) = self.inherited_attr(id, key) {
                    if let Ok(dict) = self.inner.get_dictionary_mut(id) {
                        dict.set(key, value);
                    }
                }
            }
        }
    }

    /// Rewrite the page tree as a flat `/Pages` node with the given order.
    pub(crate) fn set_page_order(&mut self, ids: Vec<ObjectId>) -> Result<()> {
        if ids.is_empty() {
            return Err(Error::LastPage);
        }
        self.inherit_attributes(&ids);
        let pages_ref = {
            let catalog = self.inner.catalog()?;
            catalog.get(b"Pages")?.as_reference()?
        };
        let kids: Vec<Object> = ids.iter().map(|id| Object::Reference(*id)).collect();
        let mut root = Dictionary::new();
        root.set("Type", Object::Name(b"Pages".to_vec()));
        root.set("Kids", Object::Array(kids));
        root.set("Count", Object::Integer(ids.len() as i64));
        self.inner.objects.insert(pages_ref, Object::Dictionary(root));
        for id in &ids {
            let dict = self.inner.get_dictionary_mut(*id)?;
            dict.set("Parent", Object::Reference(pages_ref));
        }
        self.inner.prune_objects();
        Ok(())
    }

    // ------------------------------------------------------------------
    // Metadata
    // ------------------------------------------------------------------

    fn info_dict(&self) -> Option<&Dictionary> {
        let info = self.inner.trailer.get(b"Info").ok()?;
        deref(&self.inner, info).as_dict().ok()
    }

    pub fn info(&self) -> DocInfo {
        let d = self.info_dict();
        let get = |key: &[u8]| d.and_then(|d| dict_text(&self.inner, d, key)).unwrap_or_default();
        let catalog = self.inner.catalog().ok();
        let has_form = catalog.map(|c| c.has(b"AcroForm")).unwrap_or(false);
        let tagged = catalog
            .and_then(|c| c.get(b"MarkInfo").ok())
            .map(|m| deref(&self.inner, m))
            .and_then(|m| m.as_dict().ok())
            .and_then(|m| m.get(b"Marked").ok())
            .and_then(|b| b.as_bool().ok())
            .unwrap_or(false);
        DocInfo {
            title: get(b"Title"),
            author: get(b"Author"),
            subject: get(b"Subject"),
            keywords: get(b"Keywords"),
            creator: get(b"Creator"),
            producer: get(b"Producer"),
            created: util::pdf_date_display(&get(b"CreationDate")),
            modified: util::pdf_date_display(&get(b"ModDate")),
            version: self.inner.version.clone(),
            page_count: self.page_count(),
            file_size: self.bytes.len() as u64,
            encrypted: self.source_encrypted,
            has_form,
            tagged,
            path: self.path.clone(),
        }
    }

    fn info_dict_mut(&mut self) -> Result<&mut Dictionary> {
        let existing = self
            .inner
            .trailer
            .get(b"Info")
            .ok()
            .and_then(|o| o.as_reference().ok());
        let id = match existing {
            Some(id) if self.inner.get_dictionary(id).is_ok() => id,
            _ => {
                let id = self.inner.add_object(Dictionary::new());
                self.inner.trailer.set("Info", Object::Reference(id));
                id
            }
        };
        Ok(self.inner.get_dictionary_mut(id)?)
    }

    fn touch_mod_date(&mut self) {
        let date = util::pdf_date_now();
        let producer = lopdf::text_string(&format!("{} {}", crate::APP_NAME, crate::VERSION));
        if let Ok(info) = self.info_dict_mut() {
            info.set("ModDate", Object::string_literal(date));
            info.set("Producer", producer);
        }
    }

    /// Update the standard metadata fields. Empty strings remove the entry.
    pub fn set_info(&mut self, title: &str, author: &str, subject: &str, keywords: &str) -> Result<()> {
        self.begin_edit();
        {
            let info = self.info_dict_mut()?;
            for (key, value) in [
                ("Title", title),
                ("Author", author),
                ("Subject", subject),
                ("Keywords", keywords),
            ] {
                if value.trim().is_empty() {
                    info.remove(key.as_bytes());
                } else {
                    info.set(key, lopdf::text_string(value.trim()));
                }
            }
        }
        self.touch_mod_date();
        self.commit()
    }

    // ------------------------------------------------------------------
    // Resources helpers used by annotation and page-mark modules
    // ------------------------------------------------------------------

    /// Ensure the page has a direct, mutable `/Resources` dictionary and return its id holder.
    pub(crate) fn with_page_resources<T>(
        &mut self,
        page_id: ObjectId,
        f: impl FnOnce(&mut Dictionary) -> T,
    ) -> Result<T> {
        let res_obj = self
            .inner
            .get_dictionary(page_id)?
            .get(b"Resources")
            .ok()
            .cloned();
        match res_obj {
            Some(Object::Reference(rid)) => {
                let obj = self.inner.get_object_mut(rid)?;
                let dict = obj.as_dict_mut()?;
                Ok(f(dict))
            }
            Some(Object::Dictionary(_)) => {
                let page = self.inner.get_dictionary_mut(page_id)?;
                let dict = page.get_mut(b"Resources")?.as_dict_mut()?;
                Ok(f(dict))
            }
            _ => {
                let page = self.inner.get_dictionary_mut(page_id)?;
                let mut dict = Dictionary::new();
                let out = f(&mut dict);
                page.set("Resources", Object::Dictionary(dict));
                Ok(out)
            }
        }
    }

    /// Register a named resource (font, ExtGState, XObject) on a page.
    pub(crate) fn add_page_resource(
        &mut self,
        page_id: ObjectId,
        category: &str,
        name: &str,
        value: Object,
    ) -> Result<()> {
        // Resolve an indirect sub-dictionary first so we can mutate it in place.
        let sub_ref = self.with_page_resources(page_id, |res| {
            res.get(category.as_bytes())
                .ok()
                .and_then(|o| o.as_reference().ok())
        })?;
        if let Some(sid) = sub_ref {
            if let Ok(obj) = self.inner.get_object_mut(sid) {
                if let Ok(sub) = obj.as_dict_mut() {
                    sub.set(name, value);
                    return Ok(());
                }
            }
        }
        self.with_page_resources(page_id, |res| {
            let needs_new = !matches!(res.get(category.as_bytes()), Ok(Object::Dictionary(_)));
            if needs_new {
                res.set(category, Object::Dictionary(Dictionary::new()));
            }
            if let Ok(Object::Dictionary(sub)) = res.get_mut(category.as_bytes()) {
                sub.set(name, value);
            }
        })
    }

    /// The shared Helvetica (WinAnsi) font used for generated text.
    pub(crate) fn helvetica_font_id(&mut self) -> ObjectId {
        if let Some(id) = self.helv_font {
            if self.inner.get_dictionary(id).is_ok() {
                return id;
            }
        }
        let id = self.inner.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica",
            "Encoding" => "WinAnsiEncoding",
        });
        self.helv_font = Some(id);
        id
    }

    /// Append a content stream to a page (optionally tagged so it can be removed later).
    pub(crate) fn append_page_content(
        &mut self,
        page_id: ObjectId,
        content: Vec<u8>,
        tag: Option<&str>,
    ) -> Result<ObjectId> {
        let mut dict = Dictionary::new();
        if let Some(tag) = tag {
            dict.set("FolitectaMark", Object::Name(tag.as_bytes().to_vec()));
        }
        let stream_id = self.inner.add_object(Stream::new(dict, content));
        let current = self.inner.get_dictionary(page_id)?.get(b"Contents").ok().cloned();
        let mut arr: Vec<Object> = match current {
            None | Some(Object::Null) => Vec::new(),
            Some(Object::Array(arr)) => arr,
            Some(Object::Reference(rid)) => match self.inner.get_object(rid) {
                Ok(Object::Array(arr)) => arr.clone(),
                _ => vec![Object::Reference(rid)],
            },
            Some(other) => vec![other],
        };
        // The first time something is added, wrap the original content in q/Q so a
        // graphics state it leaves behind (e.g. a flipped matrix) cannot distort
        // what Folitecta draws after it.
        let guarded = arr.iter().filter_map(|o| o.as_reference().ok()).any(|id| {
            self.inner
                .get_object(id)
                .ok()
                .and_then(|o| o.as_stream().ok())
                .is_some_and(|s| s.dict.has(b"FolitectaGuard"))
        });
        if !guarded && !arr.is_empty() {
            let open = self.inner.add_object(Stream::new(
                dictionary! { "FolitectaGuard" => true },
                b"q\n".to_vec(),
            ));
            let close = self.inner.add_object(Stream::new(
                dictionary! { "FolitectaGuard" => true },
                b"\nQ\n".to_vec(),
            ));
            arr.insert(0, Object::Reference(open));
            arr.push(Object::Reference(close));
        }
        arr.push(Object::Reference(stream_id));
        let new_contents = if arr.len() == 1 {
            arr.remove(0)
        } else {
            Object::Array(arr)
        };
        self.inner
            .get_dictionary_mut(page_id)?
            .set("Contents", new_contents);
        Ok(stream_id)
    }

    /// Content stream object ids of a page in order.
    pub(crate) fn page_content_ids(&self, page_id: ObjectId) -> Vec<ObjectId> {
        self.inner.get_page_contents(page_id)
    }
}
