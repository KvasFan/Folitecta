//! The undo-aware document model.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use lopdf::{Dictionary, Object, ObjectId, Stream, dictionary};

use crate::error::{Error, Result};
use crate::protection::digsig::dss::SourceAnalysis;
use crate::protection::encrypted::{self, DecryptedBase};
use crate::security::{EncryptionInfo, Protection};
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
    /// The document is protected (opened from an encrypted file, or made
    /// from a protected document): its files are encrypted.
    pub encrypted: bool,
    /// The encryption of its files ("AES-256", ...), empty when it has none.
    pub encryption: String,
    pub has_form: bool,
    pub tagged: bool,
    pub path: Option<PathBuf>,
}

/// A PDF document with file association, serialized snapshot and undo history.
pub struct Document {
    pub(crate) inner: lopdf::Document,
    path: Option<PathBuf>,
    /// Name shown when there is no file (an attachment opened on its own).
    name_hint: Option<String>,
    bytes: Arc<Vec<u8>>,
    revision: u64,
    dirty: bool,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    /// What the edit under way set aside, given back when it is abandoned.
    edit_backup: Option<EditBackup>,
    /// Files written so far (see [`Snapshot::saves`]).
    saves: u64,
    /// Set when the state carries signatures that only an update of the
    /// signed file it came from keeps valid and a later save replaced that
    /// file on the disk: the state cannot be saved with them. It holds that
    /// signed file when it is known.
    signed_base_lost: Option<Option<Source>>,
    /// Object id of the shared Helvetica font resource created by Folitecta, if any.
    pub(crate) helv_font: Option<ObjectId>,
    /// The file was encrypted when it was opened (it is kept decrypted in memory).
    source_encrypted: bool,
    /// The encryption of the opened file (or of the protected document this
    /// one was made from), applied again to every file written.
    protection: Option<Protection>,
    /// The file as it was opened or last saved: the base of incremental
    /// updates, which keep existing signatures valid, and the bytes
    /// signatures are validated against. For a protected document only when
    /// that file is signed.
    pub(crate) source: Option<Source>,
    /// Positioned text of the pages, kept per page for the revision of its text.
    pub(crate) text_cache: std::sync::Mutex<crate::text::TextCache>,
    /// Marked content of the pages already measured, keyed by revision.
    pub(crate) marked_cache: std::sync::Mutex<crate::structure::MarkedCache>,
    /// Structure tree read for a revision (for views, never while editing).
    pub(crate) struct_cache: std::sync::Mutex<crate::structure::StructCache>,
    /// Pictures of the pages already looked at, keyed by revision.
    pub(crate) image_cache: std::sync::Mutex<crate::pictures::ImageCache>,
    /// The layer new comments and page content go to (a choice of the view,
    /// not saved; see `layer_drawing`).
    pub(crate) drawing_layer: Option<ObjectId>,
    /// The layer something was last put on, until the interface asks.
    pub(crate) layer_drawn: Option<ObjectId>,
    /// What hidden layers hide and the bytes of views, for one revision.
    pub(crate) layer_cache: std::sync::Mutex<crate::layer_drawing::LayerCache>,
    /// The object ids of the pages of a revision, kept while no edit is under way.
    page_cache: std::sync::Mutex<PageCache>,
    /// What the views read from a revision (comments, bookmarks, links, form
    /// fields), kept while no edit is under way.
    derived: std::sync::Mutex<Derived>,
    /// An edit is under way (from `begin_edit`, `inner_mut` or a change of the
    /// page tree until `commit`, an undo or a redo): the pages are looked up
    /// in the page tree itself, never in `page_cache`.
    editing: bool,
    /// The objects and pages of the last revision, to find the pages the next
    /// change touches (made before the first change).
    baseline: Option<crate::changes::Baseline>,
    /// For every page, the revision at which its picture last changed, and
    /// its text (empty until the first change: every page at revision 0).
    page_revs: Vec<u64>,
    text_revs: Vec<u64>,
}

/// Values read from one revision, by name.
#[derive(Default)]
struct Derived {
    revision: u64,
    slots: std::collections::HashMap<&'static str, Arc<dyn std::any::Any + Send + Sync>>,
}

/// The pages of one revision: their object ids in order and, once asked
/// for, the index of each id.
#[derive(Default)]
struct PageCache {
    revision: u64,
    ids: Option<Arc<Vec<ObjectId>>>,
    index: Option<Arc<std::collections::HashMap<ObjectId, usize>>>,
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

/// The file a document was opened from or last saved to.
#[derive(Clone)] // no Debug: it holds decrypted objects
pub(crate) struct Source {
    /// The bytes of the file, as they are on the disk (encrypted when it is).
    pub(crate) bytes: Arc<Vec<u8>>,
    /// The file carries signatures.
    pub(crate) signed: bool,
    /// For an encrypted file: its decrypted objects and last revision, the
    /// base of encrypted incremental updates.
    pub(crate) decrypted: Option<Arc<DecryptedBase>>,
    /// The document security store of the file and the kinds of its
    /// revisions (what validation reads, worked out once per file).
    pub(crate) analysis: Arc<SourceAnalysis>,
}

/// The bytes of a file to write for a document.
pub(crate) struct FileBytes {
    pub(crate) bytes: Vec<u8>,
    /// Where what this write added starts (the length of the file it
    /// updates; 0 for a whole file).
    pub(crate) from: usize,
    /// For a protected signed document: the base of the next update, once
    /// the file is written.
    pub(crate) next: Option<DecryptedBase>,
}

pub(crate) fn serialize(doc: &mut lopdf::Document) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    doc.save_to(&mut out)?;
    Ok(out)
}

/// Write the bytes of a whole PDF file to `path`, replacing it only once the
/// new file is complete on the disk (the original survives any failure).
pub fn write_pdf_file(path: &Path, data: &[u8]) -> Result<()> {
    if !util::looks_like_whole_pdf(data) {
        return Err(Error::Invalid(format!(
            "{}: the file to write is not a complete PDF; nothing was written",
            path.display()
        )));
    }
    util::write_atomic(path, data).map_err(|e| Error::io(path, e))
}

/// What `begin_edit` set aside for `abandon_edit`: the state before the edit,
/// the redo history it cleared and the oldest undo steps it dropped at the
/// limit, so an abandoned edit leaves the whole history as it was.
struct EditBackup {
    snapshot: Snapshot,
    redo: Vec<Snapshot>,
    /// Oldest first.
    dropped: Vec<Snapshot>,
}

/// A state kept for undo and redo: the serialized document and the file it
/// is saved on (the base of incremental updates), so undoing an edit that
/// dropped that base (a redaction) brings it back.
#[derive(Clone)]
struct Snapshot {
    bytes: Arc<Vec<u8>>,
    source: Option<Source>,
    /// The saves made before the state was kept: the base comes back with
    /// it only while no file was written since (a save makes the file just
    /// written the base, whatever is undone).
    saves: u64,
    /// Whether the state lost the signed file it needs as its base, and that
    /// file (see `Document::signed_base_lost`).
    lost_base: Option<Option<Source>>,
}

/// The key of the encryption of a protected document, to read the files it
/// writes (for example to check them before they are written).
#[derive(Clone)]
pub struct ProtectionKey(pub(crate) Protection);

impl std::fmt::Debug for ProtectionKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ProtectionKey(..)")
    }
}

/// The bytes of a file ready to be written by a save (see
/// [`Document::prepare_save`]).
#[derive(Clone)] // no Debug: the base of a protected file holds decrypted objects
pub struct PreparedSave {
    pub bytes: Arc<Vec<u8>>,
    /// The key the bytes are encrypted with (None for plain files).
    pub key: Option<ProtectionKey>,
    revision: u64,
    /// For a protected signed document: the base of the next update.
    next: Option<Arc<DecryptedBase>>,
}

impl std::fmt::Debug for PreparedSave {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PreparedSave")
            .field("bytes", &self.bytes.len())
            .field("revision", &self.revision)
            .finish()
    }
}

impl PreparedSave {
    /// The revision of the document the bytes were made from.
    pub fn revision(&self) -> u64 {
        self.revision
    }
}

/// The state of a document taken to work on elsewhere: the serialized
/// objects, how its files are protected and the base of incremental updates.
#[derive(Clone)]
pub struct DocSnapshot {
    bytes: Arc<Vec<u8>>,
    protection: Option<Protection>,
    source: Option<Source>,
    source_encrypted: bool,
    path: Option<PathBuf>,
    revision: u64,
    max_id: u32,
}

impl std::fmt::Debug for DocSnapshot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DocSnapshot")
            .field("revision", &self.revision)
            .field("bytes", &self.bytes.len())
            .finish()
    }
}

impl DocSnapshot {
    /// The revision of the document when the snapshot was taken.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// A document with this state, protected like the original and saved
    /// incrementally on the same base (the parse can be long: call it on a
    /// worker thread).
    pub fn open(&self) -> Result<Document> {
        let mut doc = Document::from_bytes(self.bytes.to_vec(), self.path.clone(), None)?;
        doc.protection = self.protection.clone();
        doc.source_encrypted = self.source_encrypted;
        doc.source = self.source.clone();
        doc.revision = self.revision;
        // Object numbers never go back (a saved file may use the higher ones).
        doc.inner.max_id = doc.inner.max_id.max(self.max_id);
        Ok(doc)
    }
}

/// A copy of a resource dictionary whose categories are direct dictionaries
/// (their entries still refer to the same fonts, pictures and drawings).
pub(crate) fn private_resources(doc: &lopdf::Document, res: &Dictionary) -> Dictionary {
    const CATEGORIES: [&[u8]; 7] = [
        b"XObject",
        b"Properties",
        b"ExtGState",
        b"Font",
        b"Pattern",
        b"Shading",
        b"ColorSpace",
    ];
    let mut own = Dictionary::new();
    for (k, v) in res.iter() {
        let value = match deref(doc, v) {
            Object::Dictionary(d) if CATEGORIES.contains(&k.as_slice()) => Object::Dictionary(d.clone()),
            _ => v.clone(),
        };
        own.set(k.clone(), value);
    }
    own
}

/// A file read for editing: its objects decrypted, and how it was protected.
struct Loaded {
    doc: lopdf::Document,
    encrypted: bool,
    /// The encryption of the file (key, passwords, permissions), to protect
    /// it the same way when it is saved; `None` for plain files.
    protection: Option<Protection>,
}

/// The name of a security handler that is not the standard (password) one.
fn other_security_handler(doc: &lopdf::Document) -> Option<Vec<u8>> {
    let filter = doc.get_encrypted().ok()?.get(b"Filter").ok()?.as_name().ok()?;
    (filter != b"Standard").then(|| filter.to_vec())
}

/// The file is protected by certificates (or another handler than passwords).
fn unsupported_handler(_name: &[u8]) -> Error {
    // The handler's name is not shown: it is a product name.
    Error::UnsupportedProtection(crate::security::CERTIFICATE_PROTECTION.into())
}

/// The dictionary of object `id` as the file `bytes` has it (an object of
/// its own, read with the content parser of the PDF library).
fn raw_dictionary(bytes: &[u8], xref: &lopdf::xref::Xref, id: ObjectId) -> Option<Dictionary> {
    let lopdf::xref::XrefEntry::Normal { offset, .. } = xref.get(id.0)? else {
        return None;
    };
    let rest = bytes.get(*offset as usize..)?;
    let start = encrypted::find_bytes(&rest[..rest.len().min(64)], b"obj")? + 3;
    let end = start + encrypted::find_bytes(&rest[start..], b"endobj")?;
    let mut text = rest[start..end].to_vec();
    text.extend_from_slice(b" n");
    let content = lopdf::content::Content::decode_strict(&text).ok()?;
    match content.operations.as_slice() {
        [op] if op.operator == "n" && op.operands.len() == 1 => match &op.operands[0] {
            Object::Dictionary(d) => Some(d.clone()),
            _ => None,
        },
        _ => None,
    }
}

/// Mark object streams so the library's reader of plain files keeps them
/// as they are (their content is still encrypted).
fn keep_object_streams(id: ObjectId, obj: &mut Object) -> Option<(ObjectId, Object)> {
    if let Object::Stream(s) = obj {
        if s.dict.has_type(b"ObjStm") {
            s.dict.set("Type", Object::Name(KEPT_OBJECT_STREAM.to_vec()));
        }
    }
    Some((id, Object::Null))
}

const KEPT_OBJECT_STREAM: &[u8] = b"FolitectaEncryptedObjStm";

/// An encrypted file read as a plain one (the `/Encrypt` entries of its
/// trailers hidden for the library's reader), its objects still encrypted;
/// with where its encryption dictionary is.
fn load_hidden(bytes: &[u8]) -> Result<(lopdf::Document, Object)> {
    let mut hidden = bytes.to_vec();
    let mut from = 0;
    while let Some(at) = encrypted::find_bytes(&hidden[from..], b"/Encrypt") {
        let p = from + at;
        // Not /EncryptMetadata: the key ends at a delimiter.
        let ends = hidden
            .get(p + 8)
            .is_none_or(|c| c.is_ascii_whitespace() || b"/<[(0123456789".contains(c));
        if ends {
            hidden[p + 7] = b'x';
        }
        from = p + 8;
    }
    let options = lopdf::LoadOptions {
        filter: Some(keep_object_streams),
        ..Default::default()
    };
    let mut doc = lopdf::Document::load_mem_with_options(&hidden, options)?;
    let encrypt = doc.trailer.remove(b"Encrypx").ok_or_else(|| {
        Error::UnsupportedProtection("an encryption whose dictionary could not be read".into())
    })?;
    Ok((doc, encrypt))
}

/// Decrypt every object of a document read by [`load_hidden`] (an object
/// that fails stays as the reader left it) and read its object streams.
fn decrypt_loaded(doc: &mut lopdf::Document, state: &lopdf::EncryptionState, encrypt_ref: ObjectId) {
    for (id, obj) in doc.objects.iter_mut() {
        if *id != encrypt_ref {
            let _ = lopdf::encryption::decrypt_object(state, *id, obj);
        }
    }
    // The objects of object streams, each from the stream the
    // cross-reference points to (an older stream may still hold an older
    // version of it, or an object freed since).
    let mut inside = Vec::new();
    for (&(container, _), obj) in doc.objects.iter_mut() {
        if let Object::Stream(s) = obj {
            if s.dict.has_type(KEPT_OBJECT_STREAM) {
                s.dict.set("Type", Object::Name(b"ObjStm".to_vec()));
                if let Ok(objects) = lopdf::ObjectStream::new(s) {
                    inside.extend(objects.objects.into_iter().filter(|((n, _), _)| {
                        matches!(
                            doc.reference_table.entries.get(n),
                            Some(lopdf::xref::XrefEntry::Compressed { container: c, .. }) if *c == container
                        )
                    }));
                }
            }
        }
    }
    for (id, obj) in inside {
        doc.objects.insert(id, obj);
    }
    if let Some(&(last, _)) = doc.objects.keys().next_back() {
        doc.max_id = doc.max_id.max(last);
    }
}

/// Read an encrypted file of revision 2 to 4 and decrypt it with `password`
/// (bytes that give its file key; see `security::r4_key_password`). The
/// library reads it as a plain file (the `/Encrypt` entries of its trailers
/// are hidden for that), then every object is decrypted here — an object
/// that fails stays as the library's reader leaves it — and object streams
/// are read once decrypted.
fn load_decrypted(bytes: &[u8], password: &[u8]) -> Result<lopdf::Document> {
    let (mut doc, encrypt) = load_hidden(bytes)?;
    let encrypt_ref = encrypt.as_reference()?;
    doc.trailer.set("Encrypt", encrypt);
    // The key is used only once the user password it is made from is
    // verified (Algorithms 6 and 7): another key would decrypt into noise.
    if doc.authenticate_raw_user_password(password).is_err() {
        return Err(Error::WrongPassword);
    }
    let state = lopdf::EncryptionState::decode(&doc, password)?;
    decrypt_loaded(&mut doc, &state, encrypt_ref);
    doc.encryption_state = Some(state);
    Ok(doc)
}

/// The objects of a file protected with `key` (any revision, whole or with
/// incremental updates), decrypted and without the encryption, read the way
/// a reader with the key does. Refused when the file was not encrypted with
/// that key.
pub(crate) fn load_with_key(bytes: &[u8], key: &ProtectionKey) -> Result<lopdf::Document> {
    let (mut doc, encrypt) = load_hidden(bytes)?;
    let encrypt_ref = encrypt.as_reference()?;
    let written_u = doc
        .get_dictionary(encrypt_ref)
        .ok()
        .and_then(|d| d.get(b"U").ok())
        .and_then(|u| u.as_str().ok())
        .map(<[u8]>::to_vec);
    let expected_u = key
        .0
        .dictionary()
        .ok()
        .and_then(|d| d.get(b"U").ok().and_then(|u| u.as_str().ok()).map(<[u8]>::to_vec))
        .unwrap_or_else(|| key.0.state().user_value().to_vec());
    if written_u.as_deref() != Some(expected_u.as_slice()) {
        return Err(Error::Invalid(
            "the file is not protected with the document's key".into(),
        ));
    }
    decrypt_loaded(&mut doc, key.0.state(), encrypt_ref);
    doc.objects.remove(&encrypt_ref);
    Ok(doc)
}

/// [`load_decrypted`] for the tests of the password algorithms.
#[cfg(test)]
pub(crate) fn load_decrypted_for_test(bytes: &[u8], password: &[u8]) -> Result<lopdf::Document> {
    load_decrypted(bytes, password)
}

fn load_inner(bytes: &[u8], password: Option<&str>) -> Result<Loaded> {
    let mut loaded = load_with(bytes, password)?;
    // Opened with the owner password (an empty one included), the permissions
    // of the file do not restrict the document.
    if let Some(p) = &loaded.protection {
        if p.is_owner_password(password.unwrap_or("")) {
            loaded.protection = Some(p.opened_by_owner());
        }
    }
    Ok(loaded)
}

fn load_with(bytes: &[u8], password: Option<&str>) -> Result<Loaded> {
    // Revisions 2 to 4: a password the library would make a wrong key of
    // (characters outside ASCII, an owner password) is decrypted here.
    // Files of revision 2 to 4 never reach the preparation for AES-256 below
    // (a composed form of an owner password would open them into noise).
    let mut r4 = false;
    if let Some(pw) = password {
        match crate::security::r4_key_password(bytes, pw) {
            crate::security::R4Key::Key(key) => return finish_load(load_decrypted(bytes, &key)?, bytes),
            crate::security::R4Key::Wrong => return Err(Error::WrongPassword),
            crate::security::R4Key::Library => r4 = true,
            crate::security::R4Key::NotR4 => {}
        }
    }
    let with =
        |pw: &str| lopdf::Document::load_mem_with_options(bytes, lopdf::LoadOptions::with_password(pw));
    let loaded = match password {
        Some(pw) => {
            // AES-256 keys come from the password prepared with SASLprep,
            // which the library checks but does not use for the key: the
            // prepared password first, then the password as typed (older
            // revisions encode it differently).
            let prepared = stringprep::saslprep(pw)
                .ok()
                .filter(|p| !r4 && p.as_ref() != pw)
                .map(|p| p.into_owned());
            match prepared.as_deref().map(with) {
                Some(Ok(doc)) => Ok(doc),
                _ => with(pw),
            }
        }
        None => lopdf::Document::load_mem(bytes),
    };
    let mut doc = match loaded {
        Ok(doc) => doc,
        Err(lopdf::Error::InvalidPassword) => {
            // A file protected by certificates refuses every password.
            if let Some(name) = lopdf::Document::load_mem(bytes)
                .ok()
                .and_then(|d| other_security_handler(&d))
            {
                return Err(unsupported_handler(&name));
            }
            return Err(Error::WrongPassword);
        }
        Err(lopdf::Error::UnsupportedSecurityHandler(name)) => return Err(unsupported_handler(&name)),
        Err(e) => return Err(e.into()),
    };
    if doc.is_encrypted() && !doc.was_encrypted() {
        if let Some(name) = other_security_handler(&doc) {
            return Err(unsupported_handler(&name));
        }
        doc.decrypt(password.unwrap_or("")).map_err(|_| match password {
            Some(_) => Error::WrongPassword,
            None => Error::PasswordRequired,
        })?;
    }
    finish_load(doc, bytes)
}

/// The document read from `bytes`, its encryption kept aside.
fn finish_load(mut doc: lopdf::Document, bytes: &[u8]) -> Result<Loaded> {
    let encrypted = doc.was_encrypted() || doc.is_encrypted();
    let mut protection = None;
    if encrypted {
        // The objects stay decrypted in memory; the encryption is kept aside and
        // applied again to the files written from the document.
        let in_trailer = doc.trailer.get(b"Encrypt").and_then(Object::as_reference).ok();
        if let Some(id) = in_trailer {
            doc.objects.remove(&id);
        }
        doc.trailer.remove(b"Encrypt");
        let state = doc.encryption_state.take().ok_or_else(|| {
            Error::UnsupportedProtection("an encryption whose key could not be read".into())
        })?;
        let encrypt_ref = in_trailer.or(state.encrypt_object_id());
        let dict = encrypt_ref.and_then(|id| raw_dictionary(bytes, &doc.reference_table, id));
        encrypted::restore_signature_values(&mut doc, &state, bytes);
        let id = doc.trailer.get(b"ID").ok().cloned();
        protection = Some(Protection::opened(state, id, dict, encrypt_ref));
    }
    Ok(Loaded {
        doc,
        encrypted,
        protection,
    })
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
        let Loaded {
            doc: inner,
            encrypted: source_encrypted,
            protection,
        } = load_inner(&bytes, password)?;
        let bytes = Arc::new(bytes);
        let mut doc = Document {
            inner,
            path,
            name_hint: None,
            source: None,
            bytes: bytes.clone(),
            revision: 0,
            dirty: false,
            undo: Vec::new(),
            redo: Vec::new(),
            edit_backup: None,
            saves: 0,
            signed_base_lost: None,
            helv_font: None,
            source_encrypted,
            protection,
            text_cache: Default::default(),
            marked_cache: Default::default(),
            struct_cache: Default::default(),
            image_cache: Default::default(),
            drawing_layer: None,
            layer_drawn: None,
            layer_cache: Default::default(),
            page_cache: Default::default(),
            derived: Default::default(),
            editing: false,
            baseline: None,
            page_revs: Vec::new(),
            text_revs: Vec::new(),
        };
        if doc.page_count() == 0 {
            return Err(Error::Invalid("the file contains no pages".into()));
        }
        let signed = doc.has_signatures();
        let encrypt_ref = doc.protection.as_ref().and_then(Protection::encrypt_ref);
        let analysis = Arc::new(SourceAnalysis::of(&doc.inner));
        doc.source = if !source_encrypted {
            Some(Source {
                bytes,
                signed,
                decrypted: None,
                analysis,
            })
        } else if let (true, Some(encrypt_ref)) = (signed, encrypt_ref) {
            // Taken before serializing, which changes the trailer.
            let base = encrypted::decrypted_base(&doc.inner, doc.inner.xref_start, encrypt_ref);
            Some(Source {
                bytes,
                signed,
                decrypted: Some(Arc::new(base)),
                analysis,
            })
        } else {
            None
        };
        // Encrypted sources are re-serialized so the renderer sees plain objects.
        if source_encrypted {
            doc.bytes = Arc::new(serialize(&mut doc.inner)?);
        }
        Ok(doc)
    }

    /// A private copy over objects already read (printing): never saved,
    /// without history or protection.
    pub(crate) fn detached(inner: lopdf::Document, bytes: Arc<Vec<u8>>) -> Self {
        Document {
            inner,
            path: None,
            name_hint: None,
            source: None,
            bytes,
            revision: 0,
            dirty: false,
            undo: Vec::new(),
            redo: Vec::new(),
            helv_font: None,
            source_encrypted: false,
            protection: None,
            edit_backup: None,
            saves: 0,
            signed_base_lost: None,
            text_cache: Default::default(),
            marked_cache: Default::default(),
            struct_cache: Default::default(),
            image_cache: Default::default(),
            drawing_layer: None,
            layer_drawn: None,
            layer_cache: Default::default(),
            page_cache: Default::default(),
            derived: Default::default(),
            // Changed in place and never committed: nothing is kept.
            editing: true,
            baseline: None,
            page_revs: Vec::new(),
            text_revs: Vec::new(),
        }
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
            name_hint: None,
            source: None,
            bytes: Arc::new(bytes),
            revision: 0,
            dirty: true,
            undo: Vec::new(),
            redo: Vec::new(),
            edit_backup: None,
            saves: 0,
            signed_base_lost: None,
            helv_font: None,
            source_encrypted: false,
            protection: None,
            text_cache: Default::default(),
            marked_cache: Default::default(),
            struct_cache: Default::default(),
            image_cache: Default::default(),
            drawing_layer: None,
            layer_drawn: None,
            layer_cache: Default::default(),
            page_cache: Default::default(),
            derived: Default::default(),
            editing: false,
            baseline: None,
            page_revs: Vec::new(),
            text_revs: Vec::new(),
        })
    }

    /// Save to the current path.
    pub fn save(&mut self) -> Result<()> {
        let path = self.path.clone().ok_or(Error::NoPath)?;
        self.save_as(&path)
    }

    /// Save to `path` and make it the document path. A protected document is
    /// written encrypted the same way (same passwords and permissions); a
    /// signed one as an incremental update that keeps its signatures valid.
    pub fn save_as(&mut self, path: impl AsRef<Path>) -> Result<()> {
        let path = path.as_ref();
        let prepared = self.prepare_save()?;
        write_pdf_file(path, prepared.bytes.as_slice())?;
        self.finish_save(path, &prepared);
        Ok(())
    }

    /// The bytes of the file a save writes now (modification date and
    /// producer updated, protected like the document, an incremental update
    /// of a signed file), to check or write them before
    /// [`Document::finish_save`].
    pub fn prepare_save(&mut self) -> Result<PreparedSave> {
        self.touch_mod_date();
        self.bytes = Arc::new(serialize(&mut self.inner)?);
        let file = self.file_bytes_from()?;
        Ok(PreparedSave {
            bytes: Arc::new(file.bytes),
            key: self.protection_key(),
            revision: self.revision,
            next: file.next.map(Arc::new),
        })
    }

    /// The bytes of `saved` were written to `path`: it becomes the document
    /// path and the base of the next updates, and the document is clean when
    /// it did not change meanwhile.
    pub fn finish_save(&mut self, path: &Path, saved: &PreparedSave) {
        self.path = Some(path.to_path_buf());
        self.written(saved.bytes.clone(), saved.next.clone());
        if saved.revision == self.revision {
            self.dirty = false;
        }
    }

    /// The file just written becomes the base of the next updates.
    fn written(&mut self, bytes: Arc<Vec<u8>>, next: Option<Arc<DecryptedBase>>) {
        let analysis = Arc::new(SourceAnalysis::of(&self.inner));
        self.saves += 1;
        self.source = match (&self.protection, next) {
            (None, _) => Some(Source {
                bytes,
                signed: self.has_signatures(),
                decrypted: None,
                analysis,
            }),
            (Some(_), Some(next)) => {
                // Numbers used by the file (its cross-reference streams) are not reused.
                self.inner.max_id = self.inner.max_id.max(next.skeleton.max_id);
                Some(Source {
                    bytes,
                    signed: true,
                    decrypted: Some(next),
                    analysis,
                })
            }
            (Some(_), None) => None,
        };
    }

    /// The state signatures are validated in: the revision of the document
    /// and how many times it was saved (the file it is checked against).
    pub fn validation_stamp(&self) -> (u64, u64) {
        (self.revision, self.saves)
    }

    /// The key of the encryption the saved files get (None for plain files).
    pub fn protection_key(&self) -> Option<ProtectionKey> {
        self.protection.clone().map(ProtectionKey)
    }

    /// The current state, cheap to take, to work on away from the document
    /// (on another thread): see [`DocSnapshot::open`].
    pub fn snapshot(&self) -> DocSnapshot {
        DocSnapshot {
            bytes: self.bytes.clone(),
            protection: self.protection.clone(),
            source: self.source.clone(),
            source_encrypted: self.source_encrypted,
            path: self.path.clone(),
            revision: self.revision,
            max_id: self.inner.max_id,
        }
    }

    /// Take the state of `worked` (a document opened from
    /// [`Document::snapshot`] and changed) as one undo step. The protection
    /// of this document stays, and so does its file identifier when it is
    /// protected (the key of older revisions depends on it).
    pub fn adopt_state(&mut self, mut worked: Document) -> Result<()> {
        self.begin_edit();
        if self.protection.is_some() {
            let id = self.inner.trailer.get(b"ID").ok().cloned();
            let changed = id.as_ref() != worked.inner.trailer.get(b"ID").ok();
            if let (Some(id), true) = (id, changed) {
                worked.inner.trailer.set("ID", id);
                worked.bytes = Arc::new(serialize(&mut worked.inner)?);
            }
        }
        let max_id = self.inner.max_id;
        self.inner = worked.inner;
        self.inner.max_id = self.inner.max_id.max(max_id);
        self.bytes = worked.bytes;
        self.source = worked.source;
        self.revision += 1;
        self.dirty = true;
        self.helv_font = None;
        self.edit_backup = None;
        Ok(())
    }

    /// Write a copy to `path` without changing the document path or dirty
    /// state; protected like the document.
    pub fn save_copy(&mut self, path: impl AsRef<Path>) -> Result<()> {
        let path = path.as_ref();
        let file = self.file_bytes_from()?;
        write_pdf_file(path, &file.bytes)
    }

    /// Write a copy to `path` without the protection of the document (an
    /// explicit choice of the user: Save Unrestricted Copy).
    pub fn save_unprotected_copy(&mut self, path: impl AsRef<Path>) -> Result<()> {
        let path = path.as_ref();
        let bytes = serialize(&mut self.inner)?;
        write_pdf_file(path, &bytes)
    }

    /// The document is protected (by the encryption of the file it was
    /// opened from, or of the protected document it was made from), which
    /// its saved files keep.
    pub fn is_protected(&self) -> bool {
        self.protection.is_some()
    }

    /// Give the document the owner's access when `password` is the owner
    /// password of its protection (the permissions no longer restrict it).
    /// Whether the document has the owner's access now.
    pub fn unlock_owner(&mut self, password: &str) -> bool {
        let Some(p) = &self.protection else {
            return false;
        };
        match p.unlocked(password) {
            Some(unlocked) => {
                self.protection = Some(unlocked);
                true
            }
            None => false,
        }
    }

    /// The protection of the document.
    pub fn protection(&self) -> Option<&Protection> {
        self.protection.as_ref()
    }

    /// The encryption of the document's files, when it is protected.
    pub fn encryption(&self) -> Option<EncryptionInfo> {
        self.protection.as_ref().map(Protection::info)
    }

    /// Protect this document like `from` (a document made from a protected
    /// one, or a report quoting it). Nothing changes when it is protected
    /// already.
    pub fn inherit_protection(&mut self, from: &Protection) {
        if self.protection.is_none() {
            self.protection = Some(from.clone());
            // A plain base would be updated without encryption.
            self.source = None;
        }
    }

    /// Protect this document like `other`, when `other` is protected.
    pub(crate) fn protect_like(&mut self, other: &Document) {
        if let Some(p) = &other.protection {
            self.inherit_protection(p);
        }
    }

    /// A new document made from the current state, protected like this one.
    pub(crate) fn derived(&self) -> Result<Document> {
        let mut doc = Document::from_bytes(self.bytes().as_ref().clone(), None, None)?;
        doc.protect_like(self);
        Ok(doc)
    }

    /// A copy of the document for another tab: the same state, protection
    /// and base of updates (signatures stay valid), no file and no history.
    pub fn duplicate(&self) -> Result<Document> {
        let mut doc = Document::from_bytes(self.bytes().as_ref().clone(), None, None)?;
        doc.protection = self.protection.clone();
        doc.source = self.source.clone();
        doc.inner.max_id = doc.inner.max_id.max(self.inner.max_id);
        doc.clear_history();
        Ok(doc)
    }

    /// The document a protected signed file opens as, made from the state
    /// that was signed (`work`, whose signature dictionary already holds what
    /// the file has) and the file itself (`source`).
    pub(crate) fn signed_state(
        mut work: Document,
        protection: Protection,
        source: Source,
    ) -> Result<Document> {
        work.protection = Some(protection);
        work.source = Some(source);
        work.source_encrypted = true;
        work.bytes = Arc::new(serialize(&mut work.inner)?);
        work.path = None;
        work.revision = 0;
        work.dirty = false;
        work.helv_font = None;
        work.clear_history();
        Ok(work)
    }

    /// Content of `other` may come into this document: refused when `other`
    /// is protected and this document is not (it would be written plain).
    pub(crate) fn check_import_from(&self, other: &Document) -> Result<()> {
        if other.is_protected() && !self.is_protected() {
            return Err(Error::ProtectedSource);
        }
        Ok(())
    }

    /// Give a work copy the protection of the document it was made from (its
    /// base of updates is set apart), so its files are protected the same way.
    pub(crate) fn set_protection(&mut self, protection: Option<Protection>) {
        self.protection = protection;
    }

    /// Whether [`Document::file_bytes_from`] writes the document as an
    /// incremental update of the signed file it was opened from (which keeps
    /// its signatures valid): encrypted for a protected document whose file
    /// is, plain for a plain one.
    pub(crate) fn updates_incrementally(&self) -> bool {
        self.source
            .as_ref()
            .is_some_and(|s| s.signed && s.decrypted.is_some() == self.protection.is_some())
            && self.has_signatures()
    }

    /// The bytes of the file to write: for a protected document, encrypted
    /// like it (an encrypted incremental update of a signed file, else the
    /// whole document); for a plain one, an incremental update of a signed
    /// file or the whole document. A protected file is checked to carry its
    /// encryption, else nothing is written.
    pub(crate) fn file_bytes_from(&mut self) -> Result<FileBytes> {
        if self.signed_base_lost.is_some() && self.has_signatures() {
            return Err(Error::SignaturesNotKept);
        }
        let Some(p) = self.protection.clone() else {
            if self.source.as_ref().is_some_and(|s| s.decrypted.is_some()) {
                return Err(Error::ProtectionNotKept(
                    "the base of the file is encrypted".into(),
                ));
            }
            // Signed documents are saved as an incremental update so the signatures stay valid.
            return Ok(match self.incremental_bytes()? {
                Some(bytes) => FileBytes {
                    from: self.source.as_ref().map_or(0, |s| s.bytes.len()),
                    bytes,
                    next: None,
                },
                None => FileBytes {
                    bytes: serialize(&mut self.inner)?,
                    from: 0,
                    next: None,
                },
            });
        };
        let signed = self.has_signatures();
        if let Some(Source {
            bytes,
            signed: true,
            decrypted: Some(base),
            ..
        }) = &self.source
        {
            if signed {
                let (out, next) = encrypted::protected_update(
                    &self.inner.objects,
                    &self.inner.trailer,
                    self.inner.max_id,
                    bytes,
                    base,
                    &p,
                )?;
                return Ok(FileBytes {
                    from: bytes.len(),
                    bytes: out,
                    next: Some(next),
                });
            }
        }
        let mut copy = self.inner.clone();
        let encrypt_ref = encrypted::encrypt_document(&mut copy, &p)?;
        let out = serialize(&mut copy)?;
        encrypted::check_written(&out)?;
        // A signed document needs the decrypted file as the base of its next update.
        let next = match signed {
            true => {
                let xref_start = encrypted::last_startxref(&out).ok_or_else(|| {
                    Error::ProtectionNotKept("the file has no cross-reference section".into())
                })?;
                let mut base = encrypted::decrypted_base(&self.inner, xref_start, encrypt_ref);
                base.skeleton.trailer = copy.trailer.clone();
                base.skeleton.trailer.remove(b"Encrypt");
                encrypted::clean_trailer(&mut base.skeleton.trailer);
                base.skeleton.max_id = copy.max_id;
                Some(base)
            }
            false => None,
        };
        Ok(FileBytes {
            bytes: out,
            from: 0,
            next,
        })
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

    /// Display name: file name, the name hint, or "Untitled".
    pub fn display_name(&self) -> String {
        self.path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().to_string())
            .or_else(|| self.name_hint.clone())
            .unwrap_or_else(|| "Untitled".to_string())
    }

    /// Name shown while the document has no file.
    pub fn set_name_hint(&mut self, name: &str) {
        self.name_hint = (!name.trim().is_empty()).then(|| name.trim().to_string());
    }

    /// Serialized bytes matching the current state (used by the renderer).
    pub fn bytes(&self) -> Arc<Vec<u8>> {
        self.bytes.clone()
    }

    /// Bytes of a plain copy of the current state that opens as this
    /// document (a recovery copy): an incremental update of the opened file
    /// when that file carries signatures (writing it whole would break them),
    /// else the current bytes. `None` for a protected document, which is
    /// never copied plain.
    pub fn copy_bytes(&self) -> Option<Arc<Vec<u8>>> {
        if self.is_protected() {
            return None;
        }
        if self.has_signatures() {
            if let Ok(Some(bytes)) = self.incremental_bytes() {
                return Some(Arc::new(bytes));
            }
        }
        Some(self.bytes.clone())
    }

    /// The document was opened from an encrypted file (it is kept decrypted
    /// in memory).
    pub fn from_encrypted_file(&self) -> bool {
        self.source_encrypted
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
        self.edit_backup = None;
    }

    /// Mark the current state as saved without writing a file (used by callers
    /// that persist the bytes themselves).
    pub fn mark_clean(&mut self) {
        self.dirty = false;
    }

    /// Mark the document as having changes to save (a copy recovered after
    /// the app ended without saving it).
    pub fn mark_dirty(&mut self) {
        self.dirty = true;
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
        self.start_changes();
        &mut self.inner
    }

    /// The object model is about to change: the pages are looked up in the
    /// page tree until the next commit, undo or redo.
    fn start_changes(&mut self) {
        if self.baseline.is_none() && !self.editing {
            // What the document is before its first change.
            let pages: Vec<ObjectId> = self.inner.get_pages().into_values().collect();
            self.page_revs = vec![self.revision; pages.len()];
            self.text_revs = vec![self.revision; pages.len()];
            self.baseline = Some(crate::changes::Baseline::of(&self.inner, pages));
        }
        self.editing = true;
        self.forget_kept();
    }

    /// The object model changes in place without `begin_edit` (a working
    /// copy that is never committed, or a part of an edit): nothing read
    /// before is kept, and nothing is kept until the next commit.
    pub(crate) fn changing(&mut self) {
        self.editing = true;
        self.forget_kept();
    }

    /// After a commit, an undo or a redo: the pages whose picture and text
    /// changed get the new revision.
    fn note_changes(&mut self) {
        let pages: Vec<ObjectId> = self.inner.get_pages().into_values().collect();
        let n = pages.len();
        let now = crate::changes::Baseline::of(&self.inner, pages);
        let touched = match &self.baseline {
            Some(old) => crate::changes::touched(&self.inner, old, &now),
            None => crate::changes::Touched::everything(),
        };
        let rev = self.revision;
        if touched.all || self.page_revs.len() != n {
            self.page_revs = vec![rev; n];
            self.text_revs = vec![rev; n];
        } else {
            for p in touched.pictures {
                self.page_revs[p] = rev;
            }
            for p in touched.text {
                self.text_revs[p] = rev;
            }
        }
        self.baseline = Some(now);
    }

    /// The revision at which the picture of `page` last changed (pages that
    /// do not exist: the current revision).
    pub fn page_revision(&self, page: usize) -> u64 {
        self.page_revs.get(page).copied().unwrap_or(self.revision)
    }

    /// The revision at which the text of `page` last changed.
    pub fn text_revision(&self, page: usize) -> u64 {
        self.text_revs.get(page).copied().unwrap_or(self.revision)
    }

    /// Drop the lists kept for the current revision.
    fn forget_kept(&self) {
        *self.page_cache.lock().unwrap_or_else(|e| e.into_inner()) = PageCache::default();
        *self.derived.lock().unwrap_or_else(|e| e.into_inner()) = Derived::default();
    }

    /// A value read from the document (`make`), kept for the revision while
    /// no edit is under way and made again otherwise.
    pub(crate) fn kept<T: std::any::Any + Send + Sync>(
        &self,
        name: &'static str,
        make: impl FnOnce() -> T,
    ) -> Arc<T> {
        if self.editing {
            return Arc::new(make());
        }
        {
            let cache = self.derived.lock().unwrap_or_else(|e| e.into_inner());
            if cache.revision == self.revision {
                if let Some(v) = cache
                    .slots
                    .get(name)
                    .cloned()
                    .and_then(|v| v.downcast::<T>().ok())
                {
                    return v;
                }
            }
        }
        // Made without the lock: making it may read other kept values.
        let value = Arc::new(make());
        let mut cache = self.derived.lock().unwrap_or_else(|e| e.into_inner());
        if cache.revision != self.revision {
            *cache = Derived {
                revision: self.revision,
                slots: Default::default(),
            };
        }
        cache.slots.insert(name, value.clone());
        value
    }

    /// The object model matches `bytes` and `revision` again.
    fn end_changes(&mut self) {
        self.editing = false;
        self.forget_kept();
    }

    // ------------------------------------------------------------------
    // Undo / redo
    // ------------------------------------------------------------------

    fn snapshot_now(&self) -> Snapshot {
        Snapshot {
            bytes: self.bytes.clone(),
            source: self.source.clone(),
            saves: self.saves,
            lost_base: self.signed_base_lost.clone(),
        }
    }

    /// Record the current state before a mutation.
    pub fn begin_edit(&mut self) {
        self.start_changes();
        let snapshot = self.snapshot_now();
        self.undo.push(snapshot.clone());
        let dropped = (self.undo.len() > UNDO_LIMIT).then(|| self.undo.remove(0));
        let redo = std::mem::take(&mut self.redo);
        match &mut self.edit_backup {
            // Another begin_edit of the same edit (nothing committed since).
            Some(backup) if Arc::ptr_eq(&backup.snapshot.bytes, &snapshot.bytes) => {
                backup.redo.extend(redo);
                backup.dropped.extend(dropped);
            }
            _ => {
                self.edit_backup = Some(EditBackup {
                    snapshot,
                    redo,
                    dropped: dropped.into_iter().collect(),
                })
            }
        }
    }

    /// An edit is under way: the objects differ from the bytes.
    pub(crate) fn editing(&self) -> bool {
        self.edit_backup.is_some()
    }

    /// Undo what an operation changed since its `begin_edit` when it failed
    /// half-way: the document, its undo steps and its redo history are as
    /// they were before the edit (revision and dirty flag never changed).
    pub(crate) fn abandon_edit(&mut self) {
        let Some(backup) = self.edit_backup.take() else {
            return;
        };
        let loaded = load_inner(&backup.snapshot.bytes, None);
        let Ok(loaded) = loaded else {
            // The state before the edit cannot be read again (it always can:
            // it was written from the document). Keep what the edit made as a
            // change of its own, which Undo takes back.
            if let Ok(bytes) = serialize(&mut self.inner) {
                self.bytes = Arc::new(bytes);
                self.revision += 1;
                self.dirty = true;
            }
            self.helv_font = None;
            self.end_changes();
            self.note_changes();
            return;
        };
        while self
            .undo
            .last()
            .is_some_and(|s| Arc::ptr_eq(&s.bytes, &backup.snapshot.bytes))
        {
            self.undo.pop();
        }
        for step in backup.dropped.into_iter().rev() {
            self.undo.insert(0, step);
        }
        self.redo = backup.redo;
        // Numbers given out by the edit are not given again: an update of
        // the file it was opened from must not reuse them.
        let max_id = self.inner.max_id;
        self.inner = loaded.doc;
        self.inner.max_id = self.inner.max_id.max(max_id);
        self.bytes = backup.snapshot.bytes;
        if backup.snapshot.saves == self.saves {
            self.source = backup.snapshot.source;
            self.signed_base_lost = backup.snapshot.lost_base;
        }
        self.helv_font = None;
        self.layer_drawn = None;
        // Back to the state of the last commit: the same pages as before.
        self.end_changes();
    }

    /// Run `f` as one undo step: committed when it succeeds, abandoned (the
    /// document and its history as they were) when it fails.
    pub(crate) fn edit_step<T>(&mut self, f: impl FnOnce(&mut Self) -> Result<T>) -> Result<T> {
        self.begin_edit();
        match f(self).and_then(|v| self.commit().map(|()| v)) {
            Ok(v) => Ok(v),
            Err(e) => {
                self.abandon_edit();
                Err(e)
            }
        }
    }

    /// The current state, to join the edits made after it with `squash_undo`.
    pub(crate) fn undo_mark(&self) -> Arc<Vec<u8>> {
        self.bytes.clone()
    }

    /// Join every edit made since `undo_mark` returned `mark` into one undo step.
    pub(crate) fn squash_undo(&mut self, mark: &Arc<Vec<u8>>) {
        if let Some(pos) = self.undo.iter().rposition(|s| Arc::ptr_eq(&s.bytes, mark)) {
            self.undo.truncate(pos + 1);
        }
    }

    /// Re-serialize after a mutation and bump the revision.
    pub fn commit(&mut self) -> Result<()> {
        self.bytes = Arc::new(serialize(&mut self.inner)?);
        self.revision += 1;
        self.dirty = true;
        self.helv_font = None;
        self.end_changes();
        self.note_changes();
        self.edit_backup = None;
        self.forget_lost_drawing_layer();
        Ok(())
    }

    fn restore(&mut self, snapshot: Snapshot) -> Result<()> {
        // Snapshots are plain; the protection of the document stays as it is.
        self.start_changes();
        // Object numbers never go back (a saved file may use the higher ones).
        let max_id = self.inner.max_id;
        self.inner = load_inner(&snapshot.bytes, None)?.doc;
        self.inner.max_id = self.inner.max_id.max(max_id);
        self.bytes = snapshot.bytes;
        // The base of incremental updates goes back with the state (a signed
        // file whose redaction is undone is saved without breaking its
        // signatures), unless a file was written since: then the file just
        // written stays the base.
        if snapshot.saves == self.saves {
            self.source = snapshot.source;
            self.signed_base_lost = snapshot.lost_base;
        } else {
            self.signed_base_lost = None;
            if self.has_signatures() && !self.source.as_ref().is_some_and(|s| s.signed) {
                // The state needs the signed file it came from as its base,
                // and the file written since is not: the base comes back only
                // while the file on the disk is still that one.
                let on_disk = |b: &[u8]| {
                    self.path
                        .as_ref()
                        .and_then(|p| std::fs::read(p).ok())
                        .is_some_and(|d| d == b)
                };
                let base = snapshot.lost_base.unwrap_or(snapshot.source).filter(|s| s.signed);
                match base {
                    Some(src) if on_disk(&src.bytes) => self.source = Some(src),
                    lost => self.signed_base_lost = Some(lost),
                }
            }
        }
        self.revision += 1;
        self.dirty = true;
        self.helv_font = None;
        self.end_changes();
        self.note_changes();
        self.edit_backup = None;
        self.forget_lost_drawing_layer();
        Ok(())
    }

    pub fn undo(&mut self) -> Result<()> {
        let prev = self.undo.pop().ok_or(Error::NothingToUndo)?;
        self.redo.push(self.snapshot_now());
        self.restore(prev)
    }

    pub fn redo(&mut self) -> Result<()> {
        let next = self.redo.pop().ok_or(Error::NothingToRedo)?;
        self.undo.push(self.snapshot_now());
        self.restore(next)
    }

    // ------------------------------------------------------------------
    // Pages
    // ------------------------------------------------------------------

    pub fn page_count(&self) -> usize {
        match self.cached_pages() {
            Some(ids) => ids.len(),
            None => self.inner.get_pages().len(),
        }
    }

    /// The number of pages the root of the page tree declares.
    fn declared_page_count(&self) -> Option<usize> {
        let catalog = self.inner.catalog().ok()?;
        let pages = deref(&self.inner, catalog.get(b"Pages").ok()?).as_dict().ok()?;
        usize::try_from(pages.get(b"Count").ok()?.as_i64().ok()?).ok()
    }

    /// The object ids of the pages, walked once per revision while no edit is
    /// under way (`None` during an edit: the page tree may be changing). A
    /// list whose length no longer matches the count of the page tree is
    /// walked again.
    fn cached_pages(&self) -> Option<Arc<Vec<ObjectId>>> {
        if self.editing {
            return None;
        }
        let mut cache = self.page_cache.lock().unwrap_or_else(|e| e.into_inner());
        if cache.revision == self.revision {
            if let Some(ids) = &cache.ids {
                if self.declared_page_count().is_none_or(|n| n == ids.len()) {
                    return Some(ids.clone());
                }
            }
        }
        let ids = Arc::new(self.inner.get_pages().into_values().collect::<Vec<_>>());
        *cache = PageCache {
            revision: self.revision,
            ids: Some(ids.clone()),
            index: None,
        };
        Some(ids)
    }

    /// Object ids of all pages in document order.
    pub fn page_ids(&self) -> Vec<ObjectId> {
        match self.cached_pages() {
            Some(ids) => ids.as_ref().clone(),
            None => self.inner.get_pages().into_values().collect(),
        }
    }

    /// Object id of a zero-based page index.
    pub fn page_id(&self, index: usize) -> Result<ObjectId> {
        let id = match self.cached_pages() {
            Some(ids) => ids.get(index).copied(),
            None => self.inner.get_pages().into_values().nth(index),
        };
        id.ok_or(Error::InvalidPage(index))
    }

    /// Zero-based index of a page object id.
    pub fn page_index(&self, id: ObjectId) -> Option<usize> {
        match self.page_index_map() {
            Some(map) => map.get(&id).copied(),
            None => self.page_ids().iter().position(|p| *p == id),
        }
    }

    /// The index of every page object id, kept with the page ids (`None`
    /// during an edit).
    pub(crate) fn page_index_map(&self) -> Option<Arc<std::collections::HashMap<ObjectId, usize>>> {
        let ids = self.cached_pages()?;
        let mut cache = self.page_cache.lock().unwrap_or_else(|e| e.into_inner());
        if cache.revision == self.revision && cache.ids.as_ref().is_some_and(|c| Arc::ptr_eq(c, &ids)) {
            if let Some(map) = &cache.index {
                return Some(map.clone());
            }
            let map: Arc<std::collections::HashMap<ObjectId, usize>> =
                Arc::new(ids.iter().enumerate().map(|(i, id)| (*id, i)).collect());
            cache.index = Some(map.clone());
            return Some(map);
        }
        Some(Arc::new(ids.iter().enumerate().map(|(i, id)| (*id, i)).collect()))
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
        self.write_page_tree(&ids)?;
        self.inner.prune_objects();
        Ok(())
    }

    /// Rewrite the page tree as a flat `/Pages` node with the given order,
    /// leaving the objects nothing points at any more for the caller to remove.
    pub(crate) fn write_page_tree(&mut self, ids: &[ObjectId]) -> Result<()> {
        if ids.is_empty() {
            return Err(Error::LastPage);
        }
        // The page tree changes: no list of pages kept until the commit.
        self.start_changes();
        self.inherit_attributes(ids);
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
        for id in ids {
            let dict = self.inner.get_dictionary_mut(*id)?;
            dict.set("Parent", Object::Reference(pages_ref));
        }
        Ok(())
    }

    // ------------------------------------------------------------------
    // Metadata
    // ------------------------------------------------------------------

    fn info_dict(&self) -> Option<&Dictionary> {
        let info = self.inner.trailer.get(b"Info").ok()?;
        deref(&self.inner, info).as_dict().ok()
    }

    /// The document title from the document information (empty when unset).
    pub fn title(&self) -> String {
        self.info_dict()
            .and_then(|d| dict_text(&self.inner, d, b"Title"))
            .map(|t| t.trim().to_string())
            .unwrap_or_default()
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
            encrypted: self.source_encrypted || self.protection.is_some(),
            encryption: self.encryption().map(|e| e.method).unwrap_or_default(),
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

    /// Give a page a `/Resources` dictionary of its own (a copy of the one it
    /// inherits or shares), with its categories as direct dictionaries, so
    /// names can be changed or dropped for this page only.
    pub(crate) fn own_page_resources(&mut self, page_id: ObjectId) -> Result<()> {
        let res = self
            .inherited_attr(page_id, b"Resources")
            .and_then(|r| r.as_dict().ok().cloned())
            .unwrap_or_default();
        let own = private_resources(&self.inner, &res);
        self.inner
            .get_dictionary_mut(page_id)?
            .set("Resources", Object::Dictionary(own));
        Ok(())
    }

    /// The `/Resources` of a page as written (a reference or a dictionary),
    /// its own or inherited from the page tree.
    fn written_resources(&self, page_id: ObjectId) -> Option<Object> {
        let mut current = Some(page_id);
        for _ in 0..64 {
            let dict = self.inner.get_dictionary(current?).ok()?;
            if let Ok(v) = dict.get(b"Resources") {
                return Some(v.clone());
            }
            current = dict.get(b"Parent").ok().and_then(|p| p.as_reference().ok());
        }
        None
    }

    /// Give a page resources of its own before something is added to them:
    /// a `/Resources` object, or a `category` sub-dictionary, that other
    /// pages use as well is copied into the page, so what is added does not
    /// appear on them too (as a shared content stream is copied before it
    /// changes).
    fn unshare_page_resources(&mut self, page_id: ObjectId, category: &[u8]) -> Result<()> {
        self.inherit_attributes(&[page_id]);
        let others: Vec<ObjectId> = self.page_ids().into_iter().filter(|p| *p != page_id).collect();
        let own = self
            .inner
            .get_dictionary(page_id)?
            .get(b"Resources")
            .ok()
            .cloned();
        if let Some(Object::Reference(rid)) = own {
            let shared = others
                .iter()
                .any(|p| matches!(self.written_resources(*p), Some(Object::Reference(r)) if r == rid));
            if shared {
                let copy = self.inner.get_dictionary(rid)?.clone();
                self.inner
                    .get_dictionary_mut(page_id)?
                    .set("Resources", Object::Dictionary(copy));
            }
        }
        let sub = self.with_page_resources(page_id, |res| {
            res.get(category).ok().and_then(|o| o.as_reference().ok())
        })?;
        if let Some(sid) = sub {
            let shared = others.iter().any(|p| {
                self.written_resources(*p).and_then(|r| {
                    deref(&self.inner, &r)
                        .as_dict()
                        .ok()
                        .and_then(|d| d.get(category).ok())
                        .and_then(|o| o.as_reference().ok())
                }) == Some(sid)
            });
            if shared {
                if let Ok(copy) = self.inner.get_dictionary(sid).cloned() {
                    self.with_page_resources(page_id, |res| res.set(category, Object::Dictionary(copy)))?;
                }
            }
        }
        Ok(())
    }

    /// Register a named resource (font, ExtGState, XObject) on a page.
    pub(crate) fn add_page_resource(
        &mut self,
        page_id: ObjectId,
        category: &str,
        name: &str,
        value: Object,
    ) -> Result<()> {
        self.unshare_page_resources(page_id, category.as_bytes())?;
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

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use lopdf::encryption::crypt_filters::{Aes256CryptFilter, CryptFilter};
    use lopdf::{EncryptionState, EncryptionVersion, Permissions};

    use super::*;

    fn hex(data: &[u8]) -> String {
        data.iter().map(|b| format!("{b:02X}")).collect()
    }

    /// An AES-256 file whose catalog, page tree, page, font and information
    /// live in an object stream (encrypted as a whole), written here by hand:
    /// the PDF library does not write encrypted object streams.
    fn object_stream_file() -> Vec<u8> {
        let key = [11u8; 32];
        let mut filters: BTreeMap<Vec<u8>, Arc<dyn CryptFilter>> = BTreeMap::new();
        filters.insert(b"StdCF".to_vec(), Arc::new(Aes256CryptFilter));
        let state = EncryptionState::try_from(EncryptionVersion::V5 {
            encrypt_metadata: true,
            crypt_filters: filters,
            file_encryption_key: &key,
            stream_filter: b"StdCF".to_vec(),
            string_filter: b"StdCF".to_vec(),
            owner_password: "owner",
            user_password: "pw",
            permissions: Permissions::default(),
        })
        .unwrap();
        let inside: [(u32, &str); 5] = [
            (2, "<< /Type /Catalog /Pages 3 0 R >>"),
            (3, "<< /Type /Pages /Kids [4 0 R] /Count 1 >>"),
            (
                4,
                "<< /Type /Page /Parent 3 0 R /MediaBox [0 0 300 300] /Resources << /Font << /F1 5 0 R >> >> /Contents 7 0 R >>",
            ),
            (
                5,
                "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>",
            ),
            (6, "<< /Producer (MARKER-OBJSTM-INFO) >>"),
        ];
        let (mut head, mut body) = (String::new(), String::new());
        for (n, text) in inside {
            head.push_str(&format!("{n} {} ", body.len()));
            body.push_str(text);
            body.push('\n');
        }
        let first = head.len() + 1;
        let mut objstm = Object::Stream(Stream::new(
            dictionary! {},
            format!("{head}\n{body}").into_bytes(),
        ));
        lopdf::encryption::encrypt_object(&state, (1, 0), &mut objstm).unwrap();
        let mut content = Object::Stream(Stream::new(
            dictionary! {},
            b"BT /F1 18 Tf 30 150 Td (MARKER-OBJSTM-PAGE) Tj ET".to_vec(),
        ));
        lopdf::encryption::encrypt_object(&state, (7, 0), &mut content).unwrap();
        let mut out = b"%PDF-1.7\n%\xe2\xe3\xcf\xd3\n".to_vec();
        let mut offsets = BTreeMap::new();
        let mut stream = |out: &mut Vec<u8>, n: u32, dict: String, data: &[u8]| {
            offsets.insert(n, out.len());
            out.extend(format!("{n} 0 obj\n<< {dict} /Length {} >>\nstream\n", data.len()).bytes());
            out.extend_from_slice(data);
            out.extend(b"\nendstream\nendobj\n");
        };
        let data = |o: &Object| o.as_stream().unwrap().content.clone();
        stream(
            &mut out,
            1,
            format!("/Type /ObjStm /N 5 /First {first}"),
            &data(&objstm),
        );
        stream(&mut out, 7, String::new(), &data(&content));
        offsets.insert(8, out.len());
        out.extend(
            format!(
                "8 0 obj\n<< /Filter /Standard /V 5 /R 6 /Length 256 /P {} /EncryptMetadata true /O <{}> /U <{}> /OE <{}> /UE <{}> /Perms <{}> /CF << /StdCF << /Type /CryptFilter /CFM /AESV3 /AuthEvent /DocOpen /Length 32 >> >> /StmF /StdCF /StrF /StdCF >>\nendobj\n",
                state.permissions().bits() as i64,
                hex(state.owner_value()),
                hex(state.user_value()),
                hex(state.owner_encrypted()),
                hex(state.user_encrypted()),
                hex(state.permission_encrypted()),
            )
            .bytes(),
        );
        let xref_at = out.len();
        offsets.insert(9, xref_at);
        let mut entries = Vec::new();
        for n in 0u32..10 {
            let (kind, field, index): (u8, u32, u16) = match n {
                0 => (0, 0, 0xFFFF),
                2..=6 => (2, 1, (n - 2) as u16),
                _ => (1, offsets[&n] as u32, 0),
            };
            entries.push(kind);
            entries.extend(field.to_be_bytes());
            entries.extend(index.to_be_bytes());
        }
        out.extend(
            format!(
                "9 0 obj\n<< /Type /XRef /Size 10 /W [1 4 2] /Root 2 0 R /Info 6 0 R /Encrypt 8 0 R /ID [<{0}> <{0}>] /Length {1} >>\nstream\n",
                hex(&[5u8; 16]),
                entries.len()
            )
            .bytes(),
        );
        out.extend(entries);
        out.extend(format!("\nendstream\nendobj\nstartxref\n{xref_at}\n%%EOF\n").bytes());
        out
    }

    #[test]
    fn updates_follow_files_with_object_streams() {
        let raw = object_stream_file();
        let mut doc = Document::from_bytes(raw.clone(), None, Some("pw")).unwrap();
        assert!(doc.page_text(0).contains("MARKER-OBJSTM-PAGE"));
        assert!(doc.is_protected());
        // As if the file were signed: its next file is an encrypted update
        // over the objects of its object stream.
        let encrypt_ref = doc.protection().unwrap().encrypt_ref().unwrap();
        let loaded = load_inner(&raw, Some("pw")).unwrap();
        let base = encrypted::decrypted_base(&loaded.doc, loaded.doc.xref_start, encrypt_ref);
        doc.source = Some(Source {
            bytes: Arc::new(raw.clone()),
            signed: true,
            decrypted: Some(Arc::new(base)),
            analysis: Arc::new(SourceAnalysis::of(&loaded.doc)),
        });
        let id = crate::digsig::DigitalId::create_self_signed(
            &crate::digsig::IdentityInfo {
                name: "Object Streams".into(),
                ..Default::default()
            },
            crate::digsig::KeyKind::EcP256,
            1,
        )
        .unwrap();
        let signed = doc.sign(&id, &Default::default()).unwrap();
        assert!(signed.starts_with(&raw));
        assert!(!signed.windows(18).any(|w| w == b"MARKER-OBJSTM-PAGE"));
        let mut back = Document::from_bytes(signed.clone(), None, Some("owner")).unwrap();
        assert!(back.page_text(0).contains("MARKER-OBJSTM-PAGE"));
        let v = back.validate_signatures(&[]);
        assert!(v.len() == 1 && v[0].valid && v[0].covers_document, "{v:?}");
        // Saved again: another update, the signature still valid.
        back.add_note(0, 40.0, 40.0, "after", "Ann", [1.0, 1.0, 0.0])
            .unwrap();
        let dir = std::env::temp_dir().join(format!("folitecta-objstm-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("saved.pdf");
        back.save_as(&path).unwrap();
        let saved = std::fs::read(&path).unwrap();
        assert!(saved.starts_with(&signed));
        let again = Document::from_bytes(saved.clone(), None, Some("pw")).unwrap();
        assert_eq!(again.annotations(0).unwrap().len(), 2);
        let v = again.validate_signatures(&[]);
        assert!(v[0].valid && !v[0].covers_document, "{v:?}");
        let r = crate::render::Renderer::new_with_password(Arc::new(saved), "pw").unwrap();
        r.render_rgba(0, 1.0, Default::default()).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn undo_len(doc: &Document) -> usize {
        doc.undo.len()
    }

    #[test]
    fn an_abandoned_edit_keeps_the_redo_history_and_the_oldest_undo_step() {
        let mut doc = Document::new_blank(1, 200.0, 200.0).unwrap();
        for i in 0..UNDO_LIMIT + 2 {
            doc.set_info(&format!("title {i}"), "", "", "").unwrap();
        }
        doc.undo().unwrap();
        let (undo, oldest) = (undo_len(&doc), doc.undo[0].clone());
        let (revision, bytes, title) = (doc.revision(), doc.bytes(), doc.title());
        assert!(doc.can_redo());
        // A failing edit that changed the document, made of two begin_edit calls.
        let failed: Result<()> = doc.edit_step(|d| {
            d.begin_edit();
            d.info_dict_mut()?.set("Title", lopdf::text_string("half done"));
            Err(Error::Invalid("refused".into()))
        });
        assert!(failed.is_err());
        assert!(doc.can_redo(), "the redo history survives a refused edit");
        assert_eq!(undo_len(&doc), undo);
        assert!(
            Arc::ptr_eq(&doc.undo[0].bytes, &oldest.bytes),
            "the oldest undo step is back"
        );
        assert_eq!(doc.revision(), revision);
        assert!(Arc::ptr_eq(&doc.bytes(), &bytes));
        assert_eq!(doc.title(), title);
        doc.redo().unwrap();
        assert_eq!(doc.title(), format!("title {}", UNDO_LIMIT + 1));
    }

    #[test]
    fn a_successful_edit_step_is_one_undo_step() {
        let mut doc = Document::new_blank(1, 200.0, 200.0).unwrap();
        doc.edit_step(|d| {
            d.info_dict_mut()?.set("Title", lopdf::text_string("one"));
            Ok(())
        })
        .unwrap();
        assert_eq!(doc.title(), "one");
        assert_eq!(undo_len(&doc), 1);
        doc.undo().unwrap();
        assert_eq!(doc.title(), "");
    }
}
