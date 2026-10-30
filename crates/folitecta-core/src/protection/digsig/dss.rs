//! The document security store (`/DSS` of the catalog, ISO 32000-2
//! 12.8.4.3, ETSI EN 319 142-1 5.4.2): certificates, OCSP answers and
//! revocation lists that validate the signatures later without a network,
//! with an entry (`/VRI`) per signature whose validation succeeded.
//!
//! Reading is bounded (items, decoded bytes). Adding material is one undo
//! step that only adds objects to the store; the save writes it as an
//! incremental update (encrypted for a protected document), so the signed
//! revisions stay as they are. The later revisions of a signed file are
//! classified structurally: a revision only adds validation material when
//! everything the document shows is unchanged (see [`classify`]).

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::sync::{Mutex, OnceLock, RwLock};

use der::Decode;
use lopdf::xref::XrefEntry;
use lopdf::{Dictionary, Object, ObjectId, Stream, StringFormat, dictionary};
use x509_cert::Certificate;

use super::revocation::{RevocationArchive, RevocationNote, hex, sha256};
use crate::document::Document;
use crate::error::{Error, Result};
use crate::protection::encrypted;
use crate::security::Protection;

/// At most this many items are read from each array of the store.
const MAX_ITEMS: usize = 1000;
/// At most this many bytes are decoded from the whole store.
const MAX_DECODED: usize = 64 * 1024 * 1024;
/// At most this many bytes are decoded for one revocation list.
const MAX_ITEM: usize = 32 * 1024 * 1024;
/// At most this many bytes are decoded for one certificate or OCSP answer.
const MAX_SMALL_ITEM: usize = 1024 * 1024;
/// At most this many objects are followed from the store.
const MAX_OBJECTS: usize = 20_000;
/// Largest stream decoded while a revision is read for its classification.
const MAX_STREAM: usize = 64 * 1024 * 1024;

/// The entry of one signature in the store.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DssVri {
    pub certs: Vec<Vec<u8>>,
    pub ocsps: Vec<Vec<u8>>,
    pub crls: Vec<Vec<u8>>,
    /// The time the entry was made (`/TU`), as written; never used as proof.
    pub tu: Option<String>,
    /// The entry has a time stamp (`/TS`).
    pub ts: bool,
}

/// The validation material of a document security store (DER items).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Dss {
    pub certs: Vec<Vec<u8>>,
    pub ocsps: Vec<Vec<u8>>,
    pub crls: Vec<Vec<u8>>,
    /// Entries per signature, by key (uppercase hexadecimal SHA-1 of the
    /// signature's `/Contents`).
    pub vri: BTreeMap<String, DssVri>,
    /// Items not read: over the limits, or that cannot be decoded.
    pub skipped: usize,
}

/// The key of the entry of a signature: the uppercase hexadecimal SHA-1 of
/// the whole value of its `/Contents` (the padding included, EN 319 142-1
/// 5.4.2.2).
pub fn vri_key(contents: &[u8]) -> String {
    use sha1::Digest;
    hex(&sha1::Sha1::digest(contents))
}

impl Dss {
    pub fn is_empty(&self) -> bool {
        self.certs.is_empty() && self.ocsps.is_empty() && self.crls.is_empty() && self.vri.is_empty()
    }

    /// The revocation data of the store.
    pub fn archive(&self) -> RevocationArchive {
        RevocationArchive {
            crls: self.crls.clone(),
            ocsps: self.ocsps.clone(),
        }
    }

    /// The entry of the signature whose `/Contents` is `contents` (keys made
    /// over the DER structure only are found too).
    pub fn vri_for(&self, contents: &[u8]) -> Option<&DssVri> {
        self.vri
            .get(&vri_key(contents))
            .or_else(|| self.vri.get(&vri_key(super::cms_sig::trim_der(contents))))
    }

    /// The store whose dictionary is `dss` among `objects`.
    fn read(objects: &BTreeMap<ObjectId, Object>, dss: &Object) -> Dss {
        let mut out = Dss::default();
        let Some(d) = resolve(objects, dss).and_then(|o| o.as_dict().ok()) else {
            return out;
        };
        let mut budget = MAX_DECODED;
        let mut cache: HashMap<ObjectId, Option<Vec<u8>>> = HashMap::new();
        let mut items = |key: &[u8], out: &mut Dss, budget: &mut usize, max: usize| -> Vec<Vec<u8>> {
            let mut list = Vec::new();
            let Some(arr) = d
                .get(key)
                .ok()
                .and_then(|a| resolve(objects, a))
                .and_then(|a| a.as_array().ok())
            else {
                return list;
            };
            for (i, item) in arr.iter().enumerate() {
                if i >= MAX_ITEMS {
                    out.skipped += arr.len() - MAX_ITEMS;
                    break;
                }
                match stream_bytes(objects, item, budget, max, &mut cache) {
                    Some(b) => list.push(b),
                    None => out.skipped += 1,
                }
            }
            list
        };
        out.certs = items(b"Certs", &mut out, &mut budget, MAX_SMALL_ITEM);
        out.ocsps = items(b"OCSPs", &mut out, &mut budget, MAX_SMALL_ITEM);
        out.crls = items(b"CRLs", &mut out, &mut budget, MAX_ITEM);
        let Some(vri) = d
            .get(b"VRI")
            .ok()
            .and_then(|v| resolve(objects, v))
            .and_then(|v| v.as_dict().ok())
        else {
            return out;
        };
        for (key, entry) in vri.iter().take(MAX_ITEMS) {
            let Some(e) = resolve(objects, entry).and_then(|e| e.as_dict().ok()) else {
                continue;
            };
            let mut list = |k: &[u8]| -> Vec<Vec<u8>> {
                e.get(k)
                    .ok()
                    .and_then(|a| resolve(objects, a))
                    .and_then(|a| a.as_array().ok())
                    .map(|a| {
                        a.iter()
                            .take(MAX_ITEMS)
                            .filter_map(|i| stream_bytes(objects, i, &mut budget, MAX_ITEM, &mut cache))
                            .collect()
                    })
                    .unwrap_or_default()
            };
            let entry = DssVri {
                certs: list(b"Cert"),
                ocsps: list(b"OCSP"),
                crls: list(b"CRL"),
                tu: e
                    .get(b"TU")
                    .ok()
                    .and_then(|t| t.as_str().ok())
                    .map(|t| String::from_utf8_lossy(t).to_string()),
                ts: e.has(b"TS"),
            };
            out.vri
                .insert(String::from_utf8_lossy(key).to_ascii_uppercase(), entry);
        }
        out
    }
}

fn resolve<'a>(objects: &'a BTreeMap<ObjectId, Object>, obj: &'a Object) -> Option<&'a Object> {
    match obj {
        Object::Reference(id) => objects.get(id),
        o => Some(o),
    }
}

/// The decoded content of the stream `item` refers to, within `budget` and
/// at most `max` bytes.
fn stream_bytes(
    objects: &BTreeMap<ObjectId, Object>,
    item: &Object,
    budget: &mut usize,
    max: usize,
    cache: &mut HashMap<ObjectId, Option<Vec<u8>>>,
) -> Option<Vec<u8>> {
    let id = item.as_reference().ok();
    if let Some(Some(v)) = id.and_then(|id| cache.get(&id)) {
        return Some(v.clone());
    }
    let stream = resolve(objects, item)?.as_stream().ok()?;
    let limit = (*budget).min(max);
    let bytes = stream.get_plain_content_with_limit(limit).ok();
    if let Some(b) = &bytes {
        *budget = budget.saturating_sub(b.len());
    }
    if let Some(id) = id {
        cache.insert(id, bytes.clone());
    }
    bytes
}

/// The references inside an object (dictionaries, arrays, stream dictionaries).
fn refs_in(obj: &Object, out: &mut Vec<ObjectId>) {
    match obj {
        Object::Reference(id) => out.push(*id),
        Object::Array(items) => items.iter().for_each(|i| refs_in(i, out)),
        Object::Dictionary(d) => d.iter().for_each(|(_, v)| refs_in(v, out)),
        Object::Stream(s) => s.dict.iter().for_each(|(_, v)| refs_in(v, out)),
        _ => {}
    }
}

/// Every object reachable from the references in `start`, at most `cap`
/// (`None` past it).
fn reach(
    objects: &BTreeMap<ObjectId, Object>,
    start: Vec<ObjectId>,
    cap: usize,
) -> Option<HashSet<ObjectId>> {
    let mut seen: HashSet<ObjectId> = HashSet::new();
    let mut queue: VecDeque<ObjectId> = start.into_iter().collect();
    while let Some(id) = queue.pop_front() {
        if !seen.insert(id) {
            continue;
        }
        if seen.len() > cap {
            return None;
        }
        if let Some(o) = objects.get(&id) {
            let mut next = Vec::new();
            refs_in(o, &mut next);
            queue.extend(next.into_iter().filter(|n| !seen.contains(n)));
        }
    }
    Some(seen)
}

/// The store of a file as it was opened or saved, kept with its objects so
/// validation reads the store the file has (not one added since).
#[derive(Default)]
pub(crate) struct SavedDss {
    objects: BTreeMap<ObjectId, Object>,
    root: Option<Object>,
    parsed: OnceLock<Dss>,
    certs: OnceLock<Vec<Certificate>>,
}

impl SavedDss {
    /// The store of `doc`.
    pub(crate) fn of(doc: &lopdf::Document) -> SavedDss {
        let Some(value) = doc.catalog().ok().and_then(|c| c.get(b"DSS").ok()).cloned() else {
            return SavedDss::default();
        };
        let mut start = Vec::new();
        refs_in(&value, &mut start);
        let ids = reach(&doc.objects, start, MAX_OBJECTS).unwrap_or_default();
        SavedDss {
            objects: ids
                .into_iter()
                .filter_map(|id| doc.objects.get(&id).map(|o| (id, o.clone())))
                .collect(),
            root: Some(value),
            ..SavedDss::default()
        }
    }

    pub(crate) fn dss(&self) -> &Dss {
        self.parsed.get_or_init(|| match &self.root {
            Some(root) => Dss::read(&self.objects, root),
            None => Dss::default(),
        })
    }

    /// The certificates of the store, decoded.
    pub(crate) fn certificates(&self) -> &[Certificate] {
        self.certs.get_or_init(|| {
            self.dss()
                .certs
                .iter()
                .filter_map(|d| Certificate::from_der(d).ok())
                .collect()
        })
    }
}

/// What a later revision of a signed file does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RevisionKind {
    /// Only adds or extends the document security store (and may update
    /// the modification date and producer of the document information).
    Material,
    /// Changes nothing the document shows (the modification date and the
    /// producer at most).
    Neutral,
    /// Ends with a valid document time stamp (its other changes are not
    /// analysed).
    TimeStamp,
    /// Anything else.
    Other,
}

/// What is known about a file: its store, and the kinds of its revisions.
#[derive(Default)]
pub(crate) struct SourceAnalysis {
    pub(crate) dss: SavedDss,
    kinds: Mutex<HashMap<(usize, usize), RevisionKind>>,
}

impl SourceAnalysis {
    pub(crate) fn of(doc: &lopdf::Document) -> SourceAnalysis {
        SourceAnalysis {
            dss: SavedDss::of(doc),
            kinds: Mutex::default(),
        }
    }

    /// The kinds of the revisions of `data` ending at `ends`, each after the
    /// one before (the first after `from`); revisions that end with a valid
    /// document time stamp (`stamped`) are not read.
    pub(crate) fn kinds(
        &self,
        data: &[u8],
        from: usize,
        ends: &[usize],
        stamped: &[usize],
        protection: Option<&Protection>,
    ) -> Vec<RevisionKind> {
        let mut out = Vec::new();
        let mut prev_end = from;
        let mut prev_doc: Option<Option<lopdf::Document>> = None;
        for &end in ends {
            if out.len() >= MAX_REVISIONS {
                // Too many revisions to read: a change that is not analysed.
                out.push(RevisionKind::Other);
                break;
            }
            let key = (prev_end, end);
            let known = self.kinds.lock().ok().and_then(|k| k.get(&key).copied());
            let kind = if stamped.contains(&end) {
                prev_doc = None;
                RevisionKind::TimeStamp
            } else if let Some(k) = known {
                prev_doc = None;
                k
            } else {
                let prev = match prev_doc.take() {
                    Some(d) => d,
                    None => load_prefix(data, prev_end, protection),
                };
                let next = load_prefix(data, end, protection);
                let kind = match (&prev, &next) {
                    (Some(p), Some(n)) => classify(data, prev_end, end, p, n, protection),
                    _ => RevisionKind::Other,
                };
                prev_doc = Some(next);
                kind
            };
            if let Ok(mut k) = self.kinds.lock() {
                k.insert(key, kind);
            }
            out.push(kind);
            prev_end = end;
            if kind == RevisionKind::Other {
                // What follows changes nothing of the answer.
                break;
            }
        }
        out
    }
}

/// At most this many later revisions are read after a signature.
const MAX_REVISIONS: usize = 32;

/// Where the revisions of `data` end: after each `%%EOF` whose
/// cross-reference section (its `startxref`) lies inside the bytes since the
/// end before. Markers inside the data of a revision (in a stream) are not
/// ends of revisions.
pub(crate) fn revision_ends(data: &[u8]) -> Vec<usize> {
    let base = encrypted::find_bytes(data, b"%PDF-").unwrap_or(0);
    let mut out = Vec::new();
    let mut from = 0;
    let mut last = 0;
    while let Some(i) = encrypted::find_bytes(&data[from..], b"%%EOF") {
        from += i + 5;
        let inside = encrypted::last_startxref(&data[..from])
            .and_then(|s| s.checked_add(base))
            .is_some_and(|s| s >= last && s < from);
        if inside {
            out.push(from);
            last = from;
        }
    }
    out
}

/// The encryption used while an encrypted revision is read, and the objects
/// of that revision that sit in object streams (not encrypted on their own).
type PrefixKey = (lopdf::EncryptionState, HashSet<u32>);
static PREFIX_KEY: RwLock<Option<PrefixKey>> = RwLock::new(None);
/// One encrypted revision is read at a time.
static PREFIX_LOAD: Mutex<()> = Mutex::new(());

/// Decrypt an object of an encrypted revision as it is read (the value of a
/// signature, never encrypted, is left as it is).
fn decrypting_filter(id: ObjectId, obj: &mut Object) -> Option<(ObjectId, Object)> {
    if let Ok(guard) = PREFIX_KEY.read() {
        if let Some((state, compressed)) = guard.as_ref() {
            if !compressed.contains(&id.0) {
                decrypt_tolerantly(state, id, obj);
            }
        }
    }
    Some((id, obj.clone()))
}

fn decrypt_tolerantly(state: &lopdf::EncryptionState, id: ObjectId, obj: &mut Object) {
    match obj {
        Object::Dictionary(d) => {
            let signature = encrypted::is_signature_dict(d);
            for (key, value) in d.iter_mut() {
                if !(signature && key == b"Contents") {
                    decrypt_tolerantly(state, id, value);
                }
            }
        }
        Object::Array(items) => items.iter_mut().for_each(|v| decrypt_tolerantly(state, id, v)),
        Object::String(..) | Object::Stream(_) => {
            if let Object::Stream(s) = obj {
                encrypted::decrypt_stream_dictionary(state, id, s);
            }
            let _ = lopdf::encryption::decrypt_object(state, id, obj);
        }
        _ => {}
    }
}

fn drop_all(_: ObjectId, _: &mut Object) -> Option<(ObjectId, Object)> {
    None
}

/// The first `end` bytes of `data` read as a document; for an encrypted file
/// its objects decrypted with the key of `protection`. `None` when the
/// revision cannot be read with its own cross-reference section.
pub(crate) fn load_prefix(
    data: &[u8],
    end: usize,
    protection: Option<&Protection>,
) -> Option<lopdf::Document> {
    let prefix = data.get(..end)?;
    let options = |filter| lopdf::LoadOptions {
        filter,
        max_decompressed_size: Some(MAX_STREAM),
        ..Default::default()
    };
    let doc = if encrypted::last_trailer_declares_encryption(prefix) {
        let state = protection?.state().clone();
        // The reader decrypts with a password only: the encryption entry of
        // the last trailer is renamed (same length), and the objects are
        // decrypted with the key as they are read.
        let mut plain = prefix.to_vec();
        hide_encryption(&mut plain)?;
        let _serial = PREFIX_LOAD.lock().unwrap_or_else(|e| e.into_inner());
        let xref = lopdf::Document::load_mem_with_options(&plain, options(Some(drop_all))).ok()?;
        let compressed: HashSet<u32> = xref
            .reference_table
            .entries
            .iter()
            .filter(|(_, e)| matches!(e, XrefEntry::Compressed { .. }))
            .map(|(n, _)| *n)
            .collect();
        *PREFIX_KEY.write().unwrap_or_else(|e| e.into_inner()) = Some((state, compressed));
        let doc = lopdf::Document::load_mem_with_options(&plain, options(Some(decrypting_filter)));
        *PREFIX_KEY.write().unwrap_or_else(|e| e.into_inner()) = None;
        doc.ok()?
    } else {
        lopdf::Document::load_mem_with_options(prefix, options(None)).ok()?
    };
    // A table rebuilt by scanning is not the revision's own.
    (doc.xref_start != 0).then_some(doc)
}

/// Rename `/Encrypt` in the last trailer of `data` (same length).
fn hide_encryption(data: &mut [u8]) -> Option<()> {
    let at = encrypted::last_startxref(data)?;
    let rest = data.get(at..)?;
    let (from, to) = if rest.starts_with(b"xref") {
        let t = encrypted::find_bytes(rest, b"trailer")?;
        let e = encrypted::find_bytes(&rest[t..], b"startxref").map_or(rest.len(), |e| t + e);
        (at + t, at + e)
    } else {
        (
            at,
            at + encrypted::find_bytes(rest, b"stream").unwrap_or(rest.len()),
        )
    };
    let i = from + encrypted::find_bytes(&data[from..to], b"/Encrypt")?;
    data[i + 1..i + 8].copy_from_slice(b"NoCrypt");
    Some(())
}

/// Object headers (`N G obj`) in `data`: (position, number, generation).
fn object_headers(data: &[u8]) -> Vec<(usize, u32, u16)> {
    let space = |c: u8| matches!(c, b' ' | b'\t' | b'\r' | b'\n' | b'\x0c' | 0);
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(k) = encrypted::find_bytes(&data[i..], b"obj") {
        let at = i + k;
        i = at + 3;
        if at >= 3 && &data[at - 3..at] == b"end" {
            continue;
        }
        if data.get(at + 3).is_some_and(|c| c.is_ascii_alphanumeric()) {
            continue;
        }
        // "N G obj": G then N backwards.
        let mut j = at;
        let mut nums = [0u64; 2];
        let mut ok = true;
        for n in nums.iter_mut().rev() {
            let gap = j;
            while j > 0 && space(data[j - 1]) {
                j -= 1;
            }
            let end = j;
            while j > 0 && data[j - 1].is_ascii_digit() {
                j -= 1;
            }
            if j == end || gap == end {
                ok = false;
                break;
            }
            *n = std::str::from_utf8(&data[j..end])
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(u64::MAX);
        }
        if !ok || (j > 0 && !space(data[j - 1])) {
            continue;
        }
        if let (Ok(n), Ok(g)) = (u32::try_from(nums[0]), u16::try_from(nums[1])) {
            out.push((j, n, g));
        }
    }
    out
}

/// The `/Prev` of the cross-reference section at `at` (a table's trailer or
/// a cross-reference stream's dictionary).
fn declared_prev(data: &[u8], at: usize) -> Option<usize> {
    let rest = data.get(at..)?;
    let dict = if rest.starts_with(b"xref") {
        let t = encrypted::find_bytes(rest, b"trailer")?;
        let e = encrypted::find_bytes(&rest[t..], b"startxref").map_or(rest.len(), |e| t + e);
        &rest[t..e]
    } else {
        &rest[..encrypted::find_bytes(rest, b"stream").unwrap_or(rest.len())]
    };
    let p = encrypted::find_bytes(dict, b"/Prev")? + 5;
    let digits: Vec<u8> = dict[p..]
        .iter()
        .copied()
        .skip_while(|c| c.is_ascii_whitespace())
        .take_while(u8::is_ascii_digit)
        .collect();
    std::str::from_utf8(&digits).ok()?.parse().ok()
}

fn trailer_ref(doc: &lopdf::Document, key: &[u8]) -> Option<ObjectId> {
    doc.trailer.get(key).ok().and_then(|o| o.as_reference().ok())
}

/// The ids reachable from the document of `doc` (its catalog, without the
/// store, and its document information, without the two dates it may
/// change), the catalog and information ids included.
fn document_reach(
    doc: &lopdf::Document,
    root: ObjectId,
    info: Option<ObjectId>,
) -> Option<HashSet<ObjectId>> {
    let mut start = vec![];
    if let Some(cat) = doc.objects.get(&root).and_then(|c| c.as_dict().ok()) {
        for (k, v) in cat.iter() {
            if k != b"DSS" {
                refs_in(v, &mut start);
            }
        }
    }
    if let Some(d) = info
        .and_then(|i| doc.objects.get(&i))
        .and_then(|d| d.as_dict().ok())
    {
        for (k, v) in d.iter() {
            if k != b"ModDate" && k != b"Producer" {
                refs_in(v, &mut start);
            }
        }
    }
    let mut set = reach(&doc.objects, start, 4_000_000)?;
    set.insert(root);
    set.extend(info);
    Some(set)
}

/// The ids reachable from the store of `doc`.
fn store_reach(doc: &lopdf::Document, root: ObjectId) -> Option<HashSet<ObjectId>> {
    let mut start = Vec::new();
    if let Some(v) = doc
        .objects
        .get(&root)
        .and_then(|c| c.as_dict().ok())
        .and_then(|c| c.get(b"DSS").ok())
    {
        refs_in(v, &mut start);
    }
    reach(&doc.objects, start, MAX_OBJECTS * 4)
}

/// Two objects hold the same values: strings compared by their bytes
/// whatever their notation (an encrypted update writes them in
/// hexadecimal), streams by their dictionary and content, wherever they
/// were read from.
fn same(a: &Object, b: &Object) -> bool {
    match (a, b) {
        (Object::String(x, _), Object::String(y, _)) => x == y,
        (Object::Array(x), Object::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(a, b)| same(a, b))
        }
        (Object::Dictionary(x), Object::Dictionary(y)) => same_dict(x, y),
        (Object::Stream(x), Object::Stream(y)) => same_dict(&x.dict, &y.dict) && x.content == y.content,
        _ => a == b,
    }
}

fn same_dict(x: &Dictionary, y: &Dictionary) -> bool {
    x.len() == y.len() && x.iter().all(|(k, v)| y.get(k).is_ok_and(|w| same(v, w)))
}

fn same_opt(a: Option<&Object>, b: Option<&Object>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => same(a, b),
        (None, None) => true,
        _ => false,
    }
}

/// A dictionary of names, integers and booleans only (no references, no strings).
fn plain_dict(obj: &Object) -> bool {
    match obj {
        Object::Dictionary(d) => d.iter().all(|(_, v)| plain_dict(v)),
        Object::Name(_) | Object::Integer(_) | Object::Real(_) | Object::Boolean(_) => true,
        _ => false,
    }
}

/// The catalog of `next` differs from that of `prev` only in its store, its
/// version and an `/ESIC` entry of a direct `/Extensions` dictionary.
fn catalog_ok(prev: &Dictionary, next: &Dictionary) -> bool {
    let keys: HashSet<&[u8]> = prev
        .iter()
        .chain(next.iter())
        .map(|(k, _)| k.as_slice())
        .collect();
    keys.into_iter().all(|k| {
        let (a, b) = (prev.get(k).ok(), next.get(k).ok());
        match k {
            b"DSS" => true,
            b"Version" => b.is_none_or(|v| v.as_name().is_ok()) && (same_opt(a, b) || b.is_some()),
            b"Extensions" if !same_opt(a, b) => {
                let strip = |o: Option<&Object>| -> Option<Option<Dictionary>> {
                    match o {
                        None => Some(None),
                        Some(Object::Dictionary(d)) => {
                            let mut d = d.clone();
                            d.remove(b"ESIC");
                            Some(Some(d))
                        }
                        Some(_) => None,
                    }
                };
                let esic_plain = b
                    .and_then(|e| e.as_dict().ok())
                    .and_then(|e| e.get(b"ESIC").ok())
                    .is_none_or(plain_dict);
                let unchanged = |x: &Option<Dictionary>, y: &Option<Dictionary>| match (x, y) {
                    (Some(x), Some(y)) => same_dict(x, y),
                    (None, None) => true,
                    (None, Some(y)) => y.is_empty(),
                    (Some(_), None) => false,
                };
                matches!((strip(a), strip(b)), (Some(x), Some(y)) if unchanged(&x, &y)) && esic_plain
            }
            _ => same_opt(a, b),
        }
    })
}

/// The document information of `next` differs from that of `prev` only in
/// its modification date and producer (direct strings).
fn info_ok(prev: Option<&Dictionary>, next: Option<&Dictionary>) -> bool {
    let empty = Dictionary::new();
    let (Some(next), prev) = (next, prev.unwrap_or(&empty)) else {
        return prev.is_none();
    };
    let keys: HashSet<&[u8]> = prev
        .iter()
        .chain(next.iter())
        .map(|(k, _)| k.as_slice())
        .collect();
    keys.into_iter().all(|k| match k {
        b"ModDate" | b"Producer" => next.get(k).is_ok_and(|v| matches!(v, Object::String(..))),
        _ => same_opt(prev.get(k).ok(), next.get(k).ok()),
    })
}

/// An entry of a cross-reference section, as the section itself says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Entry {
    Free,
    Normal { offset: usize, generation: u16 },
    Compressed { container: u32, index: u32 },
}

fn pdf_space(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\r' | b'\n' | b'\x0c' | 0)
}

/// The tokens of a cross-reference table (numbers, `n`, `f`, `trailer`).
struct Tokens<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Tokens<'a> {
    fn next_token(&mut self) -> Option<&'a [u8]> {
        while self.data.get(self.pos).is_some_and(|c| pdf_space(*c)) {
            self.pos += 1;
        }
        let start = self.pos;
        while self
            .data
            .get(self.pos)
            .is_some_and(|c| !pdf_space(*c) && *c != b'<')
        {
            self.pos += 1;
        }
        (self.pos > start).then(|| &self.data[start..self.pos])
    }
}

fn number<T: std::str::FromStr>(t: &[u8]) -> Option<T> {
    std::str::from_utf8(t).ok()?.parse().ok()
}

/// The number and generation of the object header at `pos` (`N G obj`).
fn header_at(data: &[u8], pos: usize) -> Option<(u32, u16)> {
    let mut t = Tokens { data, pos };
    let n = number(t.next_token()?)?;
    let g = number(t.next_token()?)?;
    let rest = data.get(t.pos..)?;
    let rest = &rest[rest.iter().take_while(|c| pdf_space(**c)).count()..];
    if !rest.starts_with(b"obj") || rest.get(3).is_some_and(|c| c.is_ascii_alphanumeric()) {
        return None;
    }
    // The tokens stop at "obj" glued to the generation ("0obj") too: refuse it.
    if !data.get(t.pos).is_some_and(|c| pdf_space(*c)) {
        return None;
    }
    Some((n, g))
}

/// At most this many entries are read from the section of one revision.
const MAX_ENTRIES: usize = 4_000_000;

/// A cross-reference section as the section itself says it.
struct Section {
    entries: Vec<(u32, Entry)>,
    /// Its subsections: first number and count.
    ranges: Vec<(u32, usize)>,
    /// It names another section (`/XRefStm`: a hybrid file).
    hybrid: bool,
    /// Its entries are written the regular way (table lines of ten and five
    /// digits, every row of a stream an entry of a known type).
    regular: bool,
    /// Where its trailer ends (tables).
    end: usize,
}

/// The cross-reference table at `at`, up to the end of its trailer.
fn table_section(data: &[u8], at: usize) -> Option<Section> {
    let rest = data.get(at..)?;
    if !rest.starts_with(b"xref") {
        return None;
    }
    let mut t = Tokens { data: rest, pos: 4 };
    let mut entries = Vec::new();
    let mut ranges = Vec::new();
    let mut regular = true;
    loop {
        let tok = t.next_token()?;
        if tok == b"trailer" {
            break;
        }
        let start: u32 = number(tok)?;
        let count: usize = number(t.next_token()?)?;
        if entries.len() + count > MAX_ENTRIES {
            return None;
        }
        ranges.push((start, count));
        for i in 0..count {
            let (o, g) = (t.next_token()?, t.next_token()?);
            regular &= o.len() == 10 && g.len() == 5;
            let offset: usize = number(o)?;
            let generation: u32 = number(g)?;
            let entry = match t.next_token()? {
                b"n" => Entry::Normal {
                    offset,
                    generation: u16::try_from(generation).ok()?,
                },
                b"f" => Entry::Free,
                _ => return None,
            };
            entries.push((start.checked_add(u32::try_from(i).ok()?)?, entry));
        }
    }
    let dict_at = skip_space(data, at + t.pos);
    let end = value_end(data, dict_at)?;
    let trailer = &data[dict_at..end];
    Some(Section {
        entries,
        ranges,
        hybrid: encrypted::find_bytes(trailer, b"/XRefStm").is_some(),
        regular,
        end,
    })
}

fn int_of(d: &Dictionary, key: &[u8]) -> Option<i64> {
    d.get(key).ok()?.as_i64().ok()
}

/// The cross-reference stream at `at` (the object the reader loaded at
/// that very offset).
fn stream_section(data: &[u8], at: usize, base: usize, next: &lopdf::Document) -> Option<Section> {
    let (num, generation) = header_at(data, at)?;
    let offset = u32::try_from(at.checked_sub(base)?).ok()?;
    if !matches!(next.reference_table.get(num), Some(XrefEntry::Normal { offset: o, generation: g }) if *o == offset && *g == generation)
    {
        return None;
    }
    let Object::Stream(s) = next.objects.get(&(num, generation))? else {
        return None;
    };
    if s.dict.get(b"Type").ok()?.as_name().ok()? != b"XRef" {
        return None;
    }
    let w: Vec<usize> = s
        .dict
        .get(b"W")
        .ok()?
        .as_array()
        .ok()?
        .iter()
        .map(|v| {
            v.as_i64()
                .ok()
                .and_then(|v| usize::try_from(v).ok())
                .filter(|v| *v <= 8)
        })
        .collect::<Option<_>>()?;
    if w.len() != 3 {
        return None;
    }
    let size = int_of(&s.dict, b"Size")?;
    let index: Vec<i64> = match s.dict.get(b"Index") {
        Ok(i) => i
            .as_array()
            .ok()?
            .iter()
            .map(|v| v.as_i64().ok())
            .collect::<Option<_>>()?,
        Err(_) => vec![0, size],
    };
    if !index.len().is_multiple_of(2) {
        return None;
    }
    let content = s.decompressed_content_with_limit(MAX_STREAM).ok()?;
    let row = w[0] + w[1] + w[2];
    let field = |bytes: &[u8]| bytes.iter().fold(0u64, |a, b| (a << 8) | u64::from(*b));
    let mut entries = Vec::new();
    let mut ranges = Vec::new();
    let mut regular = true;
    let mut at_row = 0usize;
    for pair in index.chunks(2) {
        let (start, count) = (u32::try_from(pair[0]).ok()?, usize::try_from(pair[1]).ok()?);
        if entries.len() + count > MAX_ENTRIES {
            return None;
        }
        ranges.push((start, count));
        for i in 0..count {
            let r = content.get(at_row..at_row + row)?;
            at_row += row;
            let kind = if w[0] == 0 { 1 } else { field(&r[..w[0]]) };
            let (a, b) = (field(&r[w[0]..w[0] + w[1]]), field(&r[w[0] + w[1]..]));
            let entry = match kind {
                0 => Entry::Free,
                1 => Entry::Normal {
                    offset: usize::try_from(a).ok()?,
                    generation: u16::try_from(b).ok()?,
                },
                2 => Entry::Compressed {
                    container: u32::try_from(a).ok()?,
                    index: u32::try_from(b).ok()?,
                },
                // Other types are references to the null object.
                _ => {
                    regular = false;
                    continue;
                }
            };
            entries.push((start.checked_add(u32::try_from(i).ok()?)?, entry));
        }
    }
    // Every row of the stream is an entry.
    regular &= at_row == content.len();
    Some(Section {
        entries,
        ranges,
        hybrid: s.dict.has(b"XRefStm"),
        regular,
        end: 0,
    })
}

/// The position after white space and comments from `pos`.
fn skip_space(data: &[u8], mut pos: usize) -> usize {
    loop {
        while data.get(pos).is_some_and(|c| pdf_space(*c)) {
            pos += 1;
        }
        if data.get(pos) == Some(&b'%') {
            while data.get(pos).is_some_and(|c| *c != b'\n' && *c != b'\r') {
                pos += 1;
            }
            continue;
        }
        return pos;
    }
}

fn delimiter(c: u8) -> bool {
    matches!(
        c,
        b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b'/' | b'%'
    )
}

/// A token of PDF syntax.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tok {
    Open,
    Close,
    Str,
    Word,
}

/// The token at `pos` (white space and comments skipped): its kind, where
/// it starts and where it ends.
fn token(data: &[u8], pos: usize) -> Option<(Tok, usize, usize)> {
    let i = skip_space(data, pos);
    let c = *data.get(i)?;
    let two = data.get(i + 1).copied();
    let (tok, end) = match c {
        b'<' if two == Some(b'<') => (Tok::Open, i + 2),
        b'>' if two == Some(b'>') => (Tok::Close, i + 2),
        b'[' => (Tok::Open, i + 1),
        b']' => (Tok::Close, i + 1),
        b'<' => (
            Tok::Str,
            i + 1 + data[i + 1..].iter().position(|c| *c == b'>')? + 1,
        ),
        b'(' => {
            let mut depth = 0usize;
            let mut j = i;
            loop {
                match *data.get(j)? {
                    b'\\' => j += 1,
                    b'(' => depth += 1,
                    b')' => {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                    _ => {}
                }
                j += 1;
            }
            (Tok::Str, j + 1)
        }
        b')' | b'>' | b'{' | b'}' => return None,
        b'/' => {
            let mut j = i + 1;
            while data.get(j).is_some_and(|c| !pdf_space(*c) && !delimiter(*c)) {
                j += 1;
            }
            (Tok::Word, j)
        }
        _ => {
            let mut j = i + 1;
            while data.get(j).is_some_and(|c| !pdf_space(*c) && !delimiter(*c)) {
                j += 1;
            }
            (Tok::Word, j)
        }
    };
    Some((tok, i, end))
}

/// Where the value (a dictionary, an array, a string or a word) at `pos` ends.
fn value_end(data: &[u8], pos: usize) -> Option<usize> {
    let (tok, _, mut end) = token(data, pos)?;
    match tok {
        Tok::Close => return None,
        Tok::Open => {}
        _ => return Some(end),
    }
    let mut depth = 1usize;
    for _ in 0..1_000_000 {
        let (tok, _, e) = token(data, end)?;
        end = e;
        match tok {
            Tok::Open => depth += 1,
            Tok::Close => {
                depth -= 1;
                if depth == 0 {
                    return Some(end);
                }
            }
            _ => {}
        }
    }
    None
}

/// The declared `/Length` of the stream dictionary between `from` and `to`
/// (a number, or a reference to a number among `objects`).
fn declared_length(
    data: &[u8],
    from: usize,
    to: usize,
    objects: &BTreeMap<ObjectId, Object>,
) -> Option<usize> {
    let mut pos = from;
    let mut depth = 0usize;
    while pos < to {
        let (tok, start, end) = token(data, pos)?;
        pos = end;
        match tok {
            Tok::Open => depth += 1,
            Tok::Close => depth = depth.saturating_sub(1),
            Tok::Word if depth == 1 && &data[start..end] == b"/Length" => {
                let (_, s1, e1) = token(data, pos)?;
                let n: i64 = number(&data[s1..e1])?;
                if let Some((Tok::Word, s2, e2)) = token(data, e1) {
                    if let Some((Tok::Word, s3, e3)) = token(data, e2) {
                        if &data[s3..e3] == b"R" {
                            let g: u16 = number(&data[s2..e2])?;
                            let id = (u32::try_from(n).ok()?, g);
                            return usize::try_from(objects.get(&id)?.as_i64().ok()?).ok();
                        }
                    }
                }
                return usize::try_from(n).ok();
            }
            _ => {}
        }
    }
    None
}

/// Where the object whose header is at `at` ends (after `endobj`), when it
/// is one value, or one dictionary followed by the data of its declared
/// length.
fn object_extent(data: &[u8], at: usize, objects: &BTreeMap<ObjectId, Object>) -> Option<usize> {
    header_at(data, at)?;
    let obj = at + encrypted::find_bytes(&data[at..data.len().min(at + 64)], b"obj")? + 3;
    let value = skip_space(data, obj);
    let mut pos = value_end(data, value)?;
    let (tok, start, end) = token(data, pos)?;
    if tok == Tok::Word && &data[start..end] == b"stream" {
        let length = declared_length(data, value, pos, objects)?;
        let mut d = end;
        if data.get(d) == Some(&b'\r') {
            d += 1;
        }
        if data.get(d) != Some(&b'\n') {
            return None;
        }
        d += 1;
        let after = d.checked_add(length)?;
        let (tok, s, e) = token(data, after)?;
        if tok != Tok::Word || &data[s..e] != b"endstream" {
            return None;
        }
        pos = e;
    }
    let (tok, s, e) = token(data, pos)?;
    (tok == Tok::Word && &data[s..e] == b"endobj").then_some(e)
}

/// The object numbers an object stream of `doc` holds, in order, and
/// whether it extends another one.
fn container_numbers(doc: &lopdf::Document, container: u32) -> Option<(Vec<u32>, bool)> {
    let Object::Stream(s) = doc.objects.get(&(container, 0))? else {
        return None;
    };
    if s.dict.get(b"Type").ok()?.as_name().ok()? != b"ObjStm" {
        return None;
    }
    let n = usize::try_from(int_of(&s.dict, b"N")?).ok()?;
    let first = usize::try_from(int_of(&s.dict, b"First")?).ok()?;
    let content = s.decompressed_content_with_limit(MAX_STREAM).ok()?;
    let mut t = Tokens {
        data: content.get(..first)?,
        pos: 0,
    };
    let mut out = Vec::with_capacity(n.min(100_000));
    for _ in 0..n {
        out.push(number(t.next_token()?)?);
        let _offset: usize = number(t.next_token()?)?;
    }
    Some((out, s.dict.has(b"Extends")))
}

/// Every object of `ids` is the same for the reader of the renderer and for
/// the PDF library (`doc`), in the file `bytes` (encrypted objects read with
/// the key of `state`).
fn renderer_agrees(
    bytes: Vec<u8>,
    ids: &HashSet<ObjectId>,
    doc: &lopdf::Document,
    state: Option<&lopdf::EncryptionState>,
) -> bool {
    use hayro::hayro_syntax::Pdf;
    use hayro::hayro_syntax::object::ObjectIdentifier;
    let Ok(pdf) = Pdf::new(bytes) else {
        return false;
    };
    let xref = pdf.xref();
    ids.iter().all(|id| {
        let Ok(n) = i32::try_from(id.0) else {
            return false;
        };
        let g = i32::from(id.1);
        let theirs = xref.get::<hayro::hayro_syntax::object::Object<'_>>(ObjectIdentifier::new(n, g));
        match (theirs, doc.objects.get(id)) {
            (None, None) | (None, Some(Object::Null)) => true,
            (Some(t), Some(o)) => agrees(&t, o, *id, state, false),
            _ => false,
        }
    })
}

/// A value of the renderer's reader and one of the PDF library agree:
/// strings and streams compared by their bytes (decrypted with `state` when
/// the file is encrypted; the value of a signature is never encrypted),
/// numbers by their value, references by their numbers.
fn agrees(
    t: &hayro::hayro_syntax::object::Object<'_>,
    o: &Object,
    id: ObjectId,
    state: Option<&lopdf::EncryptionState>,
    plain: bool,
) -> bool {
    use hayro::hayro_syntax::object::{MaybeRef, Object as H};
    let decrypt = |bytes: &[u8], stream: bool| -> Vec<u8> {
        match state.filter(|_| !plain) {
            Some(st) => {
                let mut obj = if stream {
                    Object::Stream(Stream::new(Dictionary::new(), bytes.to_vec()))
                } else {
                    Object::String(bytes.to_vec(), StringFormat::Literal)
                };
                let _ = lopdf::encryption::decrypt_object(st, id, &mut obj);
                match obj {
                    Object::Stream(s) => s.content,
                    Object::String(b, _) => b,
                    _ => Vec::new(),
                }
            }
            None => bytes.to_vec(),
        }
    };
    let item = |m: MaybeRef<H<'_>>, o: &Object, plain: bool| match (m, o) {
        (MaybeRef::Ref(r), Object::Reference(l)) => {
            i64::from(r.obj_number) == i64::from(l.0) && i64::from(r.gen_number) == i64::from(l.1)
        }
        (MaybeRef::NotRef(v), o) => agrees(&v, o, id, state, plain),
        _ => false,
    };
    // The length of a stream is compared through its data (the library
    // gives the length of the decrypted data).
    let dict = |d: &hayro::hayro_syntax::object::Dict<'_>, l: &Dictionary, stream: bool| {
        let signature = encrypted::is_signature_dict(l);
        let skip = |k: &[u8]| stream && k == b"Length";
        d.keys().filter(|k| !skip(k)).count() == l.iter().filter(|(k, _)| !skip(k)).count()
            && d.entries().all(|(k, v)| {
                let key: &[u8] = &k;
                let plain = plain || signature && key == b"Contents";
                skip(key) || l.get(key).is_ok_and(|o| item(v, o, plain))
            })
    };
    match (t, o) {
        (H::Null(_), Object::Null) => true,
        (H::Boolean(a), Object::Boolean(b)) => a == b,
        (H::Number(a), Object::Integer(b)) => a.as_f64() == *b as f64,
        (H::Number(a), Object::Real(b)) => {
            (a.as_f64() - f64::from(*b)).abs() <= 1e-4 * f64::from(b.abs()).max(1.0)
        }
        (H::String(a), Object::String(b, _)) => decrypt(a.as_bytes(), false) == *b,
        (H::Name(a), Object::Name(b)) => {
            let a: &[u8] = a;
            a == b.as_slice()
        }
        (H::Array(a), Object::Array(b)) => {
            let items: Vec<_> = a.raw_iter().collect();
            items.len() == b.len() && items.into_iter().zip(b).all(|(m, o)| item(m, o, plain))
        }
        (H::Dict(a), Object::Dictionary(b)) => dict(a, b, false),
        (H::Stream(a), Object::Stream(b)) => {
            dict(a.dict(), &b.dict, true) && decrypt(&a.raw_data(), true) == b.content
        }
        _ => false,
    }
}

/// Two entries of the reader's tables are the same.
fn same_entry(a: Option<&XrefEntry>, b: Option<&XrefEntry>) -> bool {
    use XrefEntry::*;
    match (a, b) {
        (None, None) => true,
        (Some(Free), Some(Free)) | (Some(UnusableFree), Some(UnusableFree)) => true,
        (
            Some(Normal {
                offset: o1,
                generation: g1,
            }),
            Some(Normal {
                offset: o2,
                generation: g2,
            }),
        ) => o1 == o2 && g1 == g2,
        (
            Some(Compressed {
                container: c1,
                index: i1,
            }),
            Some(Compressed {
                container: c2,
                index: i2,
            }),
        ) => c1 == c2 && i1 == i2,
        _ => false,
    }
}

/// The bytes of the first `end` bytes of `data` the renderer's reader can
/// read without a password (an encrypted file with its encryption entry
/// renamed: its objects are decrypted by the caller).
fn renderer_bytes(data: &[u8], end: usize) -> Option<Vec<u8>> {
    let mut bytes = data.get(..end)?.to_vec();
    if encrypted::last_trailer_declares_encryption(&bytes) {
        hide_encryption(&mut bytes)?;
    }
    Some(bytes)
}

/// The number is used in `doc` (an object, or an entry in use).
fn in_use(doc: &lopdf::Document, n: u32) -> bool {
    doc.objects.keys().any(|id| id.0 == n)
        || matches!(
            doc.reference_table.get(n),
            Some(XrefEntry::Normal { .. } | XrefEntry::Compressed { .. })
        )
}

/// What the revision of `data` between `prev_end` and `end` does, from its
/// structure (never from what it says of itself). It only adds validation
/// material when it has the regular shape of a careful writer's update, and
/// everything the document shows is unchanged for two readers; anything
/// else is a change:
/// - its bytes are objects (a header, one value or a dictionary and the
///   data of its declared length, `endobj`) with only white space and
///   comments between them, then one cross-reference section (a table and
///   its trailer, or the stream object last, never both, never hybrid),
///   `startxref` with that section's offset and `%%EOF`;
/// - the section (read here, not through the merged table of a reader)
///   follows the one before (`/Prev`), declares a `/Size` above its numbers
///   and not below the one before, has subsections in order that do not
///   overlap, names each number once, frees no number in use, and neither
///   defines nor frees any number the document reaches (its catalog and
///   document information excepted);
/// - each object it writes sits inside it at the offset of its entry, under
///   the entry's number and generation, and every object header in its bytes
///   is such an entry; its object streams (never extending others) hold only
///   new numbers, each at the index of its entry, in order;
/// - every object the document reaches from its catalog (without the store)
///   and its document information keeps its entry and is unchanged,
///   dictionary and content, and the renderer's own reader resolves each of
///   them, before and after, to the same values as the PDF library;
/// - the catalog changes only in `/DSS`, `/Version` and a direct
///   `/Extensions` `/ESIC`, the document information only in its
///   modification date and producer;
/// - every object of the store is new or belonged only to the store before,
///   and none is reached from the document.
///
/// Objects nothing reaches are ignored; no object is exempted by its type.
pub(crate) fn classify(
    data: &[u8],
    prev_end: usize,
    end: usize,
    prev: &lopdf::Document,
    next: &lopdf::Document,
    protection: Option<&Protection>,
) -> RevisionKind {
    use RevisionKind::Other;
    let base = encrypted::find_bytes(data, b"%PDF-").unwrap_or(0);
    let Some(section) = encrypted::last_startxref(&data[..end]) else {
        return Other;
    };
    let section_at = section + base;
    if next.xref_start != section || section_at < prev_end || section_at >= end {
        return Other;
    }
    let declared = encrypted::last_startxref(&data[..prev_end]);
    if declared.is_none() || declared_prev(&data[..end], section_at) != declared {
        return Other;
    }
    let table = data[section_at..].starts_with(b"xref");
    let own = if table {
        table_section(&data[..end], section_at)
    } else {
        stream_section(&data[..end], section_at, base, next)
    };
    let Some(Section {
        entries,
        mut ranges,
        hybrid,
        regular,
        end: table_end,
    }) = own
    else {
        return Other;
    };
    if hybrid || !regular {
        return Other;
    }
    // Subsections in order, not overlapping (a stream lists them in order).
    let listed = ranges.clone();
    ranges.sort();
    if !table && listed != ranges
        || ranges
            .windows(2)
            .any(|w| u64::from(w[0].0) + w[0].1 as u64 > u64::from(w[1].0))
    {
        return Other;
    }
    let mut numbers = HashSet::new();
    if !entries.iter().all(|(n, _)| numbers.insert(*n)) {
        return Other;
    }
    // /Size above every number of the section and not below the one before.
    let size_of = |doc: &lopdf::Document| doc.trailer.get(b"Size").ok().and_then(|s| s.as_i64().ok());
    let (Some(size), Some(prev_size)) = (size_of(next), size_of(prev)) else {
        return Other;
    };
    if size < prev_size || entries.iter().any(|(n, _)| i64::from(*n) >= size) {
        return Other;
    }
    let (Some(root), Some(prev_root)) = (trailer_ref(next, b"Root"), trailer_ref(prev, b"Root")) else {
        return Other;
    };
    if root != prev_root {
        return Other;
    }
    let (prev_info, next_info) = (trailer_ref(prev, b"Info"), trailer_ref(next, b"Info"));
    if prev_info.is_some() && prev_info != next_info
        || prev_info.is_none() && next_info.is_some_and(|i| prev.objects.contains_key(&i))
    {
        return Other;
    }
    let (Some(r_next), Some(r_prev)) = (
        document_reach(next, root, next_info),
        document_reach(prev, root, prev_info),
    ) else {
        return Other;
    };
    // The numbers the document reaches, the catalog and the document
    // information excepted: the revision may not touch them.
    let special: HashSet<u32> = std::iter::once(root.0).chain(next_info.map(|i| i.0)).collect();
    let document: HashSet<u32> = r_next
        .iter()
        .chain(r_prev.iter())
        .map(|id| id.0)
        .filter(|n| !special.contains(n))
        .collect();
    let mut written: Vec<usize> = Vec::new();
    let mut containers: HashMap<u32, Vec<(u32, u32)>> = HashMap::new();
    for (n, e) in &entries {
        if document.contains(n) {
            return Other;
        }
        match *e {
            // The head of the list of free numbers, or a number never used.
            Entry::Free if *n != 0 && in_use(prev, *n) => return Other,
            Entry::Free => {}
            Entry::Normal { offset, generation } => {
                let at = offset + base;
                if at < prev_end || at >= end || header_at(data, at) != Some((*n, generation)) {
                    return Other;
                }
                // One section only: no other cross-reference stream.
                let xref_type = next
                    .objects
                    .get(&(*n, generation))
                    .and_then(|o| o.type_name().ok())
                    == Some(b"XRef".as_slice());
                if xref_type && (table || at != section_at) {
                    return Other;
                }
                written.push(at);
            }
            Entry::Compressed { container, index } => {
                containers.entry(container).or_default().push((*n, index))
            }
        }
    }
    // The section object of a stream is one of its own entries.
    if !table && !written.contains(&section_at) {
        return Other;
    }
    // Every object written in the revision's bytes is one of its entries.
    for (pos, n, g) in object_headers(&data[prev_end..end]) {
        let at = prev_end + pos;
        if !written.contains(&at) || header_at(data, at) != Some((n, g)) {
            return Other;
        }
    }
    // The shape: objects, then the section, then startxref and %%EOF.
    written.sort_unstable();
    let mut pos = skip_space(data, prev_end);
    for at in &written {
        if pos != *at {
            return Other;
        }
        let Some(after) = object_extent(&data[..end], *at, &next.objects) else {
            return Other;
        };
        pos = skip_space(data, after);
    }
    if table {
        if pos != section_at {
            return Other;
        }
        pos = skip_space(data, table_end);
    } else if written.last() != Some(&section_at) {
        return Other;
    }
    let Some((Tok::Word, s, e)) = token(&data[..end], pos) else {
        return Other;
    };
    let Some((Tok::Word, ns, ne)) = token(&data[..end], e) else {
        return Other;
    };
    let mut tail = ne;
    while data.get(tail).is_some_and(|c| pdf_space(*c)) {
        tail += 1;
    }
    if &data[s..e] != b"startxref"
        || number::<usize>(&data[ns..ne]) != Some(section)
        || data.get(tail..end) != Some(b"%%EOF".as_slice())
    {
        return Other;
    }
    // Object streams of the revision: written by it, extending none, holding
    // only new numbers, each at the index of its entry, in order.
    for (container, members) in &containers {
        let own_container = entries
            .iter()
            .any(|(n, e)| n == container && matches!(e, Entry::Normal { .. }));
        let Some((held, extends)) = container_numbers(next, *container).filter(|_| own_container) else {
            return Other;
        };
        let mut members = members.clone();
        members.sort_by_key(|(_, index)| *index);
        let in_order = members.len() == held.len()
            && members
                .iter()
                .enumerate()
                .all(|(i, (n, index))| *index as usize == i && held[i] == *n);
        if extends || !in_order || held.iter().any(|n| in_use(prev, *n) || document.contains(n)) {
            return Other;
        }
    }
    // The reader resolves every object of the document as before.
    for n in &document {
        if !same_entry(prev.reference_table.get(*n), next.reference_table.get(*n)) {
            return Other;
        }
    }
    let dict = |doc: &lopdf::Document, id: Option<ObjectId>| -> Option<Dictionary> {
        id.and_then(|i| doc.objects.get(&i))
            .and_then(|o| o.as_dict().ok())
            .cloned()
    };
    let (Some(prev_cat), Some(next_cat)) = (dict(prev, Some(root)), dict(next, Some(root))) else {
        return Other;
    };
    if !catalog_ok(&prev_cat, &next_cat)
        || !info_ok(dict(prev, prev_info).as_ref(), dict(next, next_info).as_ref())
    {
        return Other;
    }
    for id in &r_next {
        if *id == root || Some(*id) == next_info {
            continue;
        }
        match (prev.objects.get(id), next.objects.get(id)) {
            (None, None) => {}
            (Some(a), Some(b)) if same(a, b) => {}
            _ => return Other,
        }
    }
    let (Some(d_next), Some(d_prev)) = (store_reach(next, root), store_reach(prev, root)) else {
        return Other;
    };
    let mut store_changed = !same_opt(prev_cat.get(b"DSS").ok(), next_cat.get(b"DSS").ok());
    for id in &d_next {
        if r_next.contains(id) {
            return Other;
        }
        match (prev.objects.get(id), next.objects.get(id)) {
            (Some(a), Some(b)) => {
                if !d_prev.contains(id) || r_prev.contains(id) {
                    return Other;
                }
                store_changed |= !same(a, b);
            }
            (None, Some(_)) => store_changed = true,
            _ => {}
        }
    }
    // A second, independent reader (the renderer's) resolves the objects of
    // the document to the same values, before and after.
    let state = protection.map(|p| p.state());
    let agreed = |len: usize, ids: &HashSet<ObjectId>, doc: &lopdf::Document| {
        renderer_bytes(data, len).is_some_and(|bytes| renderer_agrees(bytes, ids, doc, state))
    };
    if !agreed(prev_end, &r_prev, prev) || !agreed(end, &r_next, next) {
        return Other;
    }
    if store_changed {
        RevisionKind::Material
    } else {
        RevisionKind::Neutral
    }
}

/// The entry of a signature in a plan: its key and the indices of its
/// certificates, OCSP answers and lists in the plan.
pub(crate) type VriPlan = (String, Vec<usize>, Vec<usize>, Vec<usize>);

/// Validation material to add, worked out from a validation (see
/// [`crate::digsig::ValidationJob::plan_material`]).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MaterialPlan {
    /// The state of the document the plan was made for.
    pub(crate) stamp: (u64, u64),
    pub(crate) certs: Vec<Vec<u8>>,
    pub(crate) ocsps: Vec<Vec<u8>>,
    pub(crate) crls: Vec<Vec<u8>>,
    /// Entries per signature: key and indices into the three lists.
    pub(crate) vri: Vec<VriPlan>,
    /// Signatures whose material is added.
    pub signatures: usize,
    /// Signatures left out (their integrity, signer or revocation failed, or
    /// the signer is not trusted), by field name.
    pub skipped: Vec<String>,
    /// Signatures whose material is added without an entry of their own
    /// (their validation did not succeed: the revocation of a certificate
    /// cannot be determined).
    pub without_entry: usize,
    /// Certificates whose revocation is still not determined, with the
    /// first reason.
    pub missing: Vec<(String, RevocationNote)>,
}

impl MaterialPlan {
    pub(crate) fn push(list: &mut Vec<Vec<u8>>, der: &[u8]) -> usize {
        match list.iter().position(|d| d == der) {
            Some(i) => i,
            None => {
                list.push(der.to_vec());
                list.len() - 1
            }
        }
    }

    /// Requests nothing at all.
    pub fn is_empty(&self) -> bool {
        self.certs.is_empty() && self.ocsps.is_empty() && self.crls.is_empty() && self.vri.is_empty()
    }
}

/// What adding validation material did.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MaterialReport {
    /// The document changed (one undo step).
    pub changed: bool,
    pub signatures: usize,
    pub skipped: usize,
    pub without_entry: usize,
    pub certs_added: usize,
    pub ocsps_added: usize,
    pub crls_added: usize,
    /// Signature entries (`/VRI`) written or extended.
    pub vri_written: usize,
    pub missing: Vec<(String, RevocationNote)>,
}

/// An array of the store: direct in a dictionary, or an object of its own.
enum ArrayAt {
    Direct,
    Object(ObjectId),
}

fn deflate(data: &[u8]) -> Vec<u8> {
    use std::io::Write;
    let mut e = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::best());
    let _ = e.write_all(data);
    e.finish().unwrap_or_default()
}

/// The effective version of a document: its header, or the catalog's
/// `/Version` when that is later.
fn effective_version(doc: &lopdf::Document) -> (u32, u32) {
    let parse = |v: &str| -> (u32, u32) {
        let mut parts = v.trim().split('.');
        let major = parts.next().and_then(|p| p.parse().ok()).unwrap_or(1);
        let minor = parts.next().and_then(|p| p.parse().ok()).unwrap_or(0);
        (major, minor)
    };
    let header = parse(&doc.version);
    let catalog = doc
        .catalog()
        .ok()
        .and_then(|c| c.get(b"Version").ok())
        .and_then(|v| v.as_name().ok())
        .map(|v| parse(&String::from_utf8_lossy(v)));
    catalog.map_or(header, |c| c.max(header))
}

impl Document {
    /// The document security store of the current state.
    pub fn dss(&self) -> Dss {
        SavedDss::of(&self.inner).dss().clone()
    }

    /// Validation material can be added: the document is signed and is the
    /// file it was opened from (or last saved to), which the material
    /// updates without touching its signed bytes. Checked before anything is
    /// asked online.
    pub fn can_add_validation_material(&self) -> Result<()> {
        if !self.has_signatures() {
            return Err(Error::Invalid("the document has no signatures".into()));
        }
        let Some(src) = self.source.as_ref().filter(|s| s.signed) else {
            return Err(Error::Invalid(
                "validation material is added as an update of the signed file, and this document has no signed file to update (save it first)"
                    .into(),
            ));
        };
        if self.is_protected() != src.decrypted.is_some() {
            return Err(Error::ProtectionNotKept(
                "the signed file and the document are not protected the same way".into(),
            ));
        }
        if encrypted::last_startxref(&src.bytes).is_none() {
            return Err(Error::Invalid(
                "the signed file has no cross-reference section an update could follow".into(),
            ));
        }
        Ok(())
    }

    /// Add the certificates and revocation data the signatures need to the
    /// document security store, with an entry per signature whose
    /// validation succeeded (one undo step; saved as an incremental update).
    pub fn add_validation_material(
        &mut self,
        trusted: &[Certificate],
        pool: &super::RevocationPool,
        now: i64,
    ) -> Result<MaterialReport> {
        self.can_add_validation_material()?;
        let plan = self.validation_job().plan_material(trusted, pool, now)?;
        self.apply_validation_material(&plan, now)
    }

    /// Add the material of `plan` (made for the current state) to the store.
    pub fn apply_validation_material(&mut self, plan: &MaterialPlan, now: i64) -> Result<MaterialReport> {
        self.can_add_validation_material()?;
        if plan.stamp != self.validation_stamp() {
            return Err(Error::Invalid(
                "the document changed while the validation material was prepared; try again".into(),
            ));
        }
        let mut report = MaterialReport {
            signatures: plan.signatures,
            skipped: plan.skipped.len(),
            without_entry: plan.without_entry,
            missing: plan.missing.clone(),
            ..MaterialReport::default()
        };
        // What the store has, by the SHA-256 of each item.
        let current = self.store_items();
        let new_count = |list: &[Vec<u8>], have: &HashMap<[u8; 32], ObjectId>| {
            list.iter().filter(|d| !have.contains_key(&sha256(d))).count()
        };
        report.certs_added = new_count(&plan.certs, &current.0[0]);
        report.ocsps_added = new_count(&plan.ocsps, &current.0[1]);
        report.crls_added = new_count(&plan.crls, &current.0[2]);
        let vri_changes = self.vri_changes(plan, &current);
        if report.certs_added + report.ocsps_added + report.crls_added == 0 && vri_changes == 0 {
            return Ok(report);
        }
        self.begin_edit();
        match self.write_material(plan, now) {
            Ok(written) => {
                report.vri_written = written;
                if let Err(e) = self.commit() {
                    self.abandon_edit();
                    return Err(e);
                }
                report.changed = true;
                Ok(report)
            }
            Err(e) => {
                self.abandon_edit();
                Err(e)
            }
        }
    }

    /// The items of the store of the current state (certificates, OCSP
    /// answers, lists) by the SHA-256 of their content, with their objects,
    /// and the VRI entries with the objects they list.
    #[allow(clippy::type_complexity)]
    fn store_items(
        &self,
    ) -> (
        [HashMap<[u8; 32], ObjectId>; 3],
        HashMap<String, HashSet<ObjectId>>,
    ) {
        let doc = &self.inner;
        let mut out: [HashMap<[u8; 32], ObjectId>; 3] = Default::default();
        let mut vri = HashMap::new();
        let Some(d) = doc
            .catalog()
            .ok()
            .and_then(|c| c.get(b"DSS").ok())
            .and_then(|v| resolve(&doc.objects, v))
            .and_then(|v| v.as_dict().ok())
        else {
            return (out, vri);
        };
        let mut budget = MAX_DECODED;
        let mut cache = HashMap::new();
        for (i, key) in [&b"Certs"[..], b"OCSPs", b"CRLs"].into_iter().enumerate() {
            let Some(arr) = d
                .get(key)
                .ok()
                .and_then(|a| resolve(&doc.objects, a))
                .and_then(|a| a.as_array().ok())
            else {
                continue;
            };
            for item in arr.iter().take(MAX_ITEMS) {
                if let (Ok(id), Some(bytes)) = (
                    item.as_reference(),
                    stream_bytes(&doc.objects, item, &mut budget, MAX_ITEM, &mut cache),
                ) {
                    out[i].entry(sha256(&bytes)).or_insert(id);
                }
            }
        }
        if let Some(v) = d
            .get(b"VRI")
            .ok()
            .and_then(|v| resolve(&doc.objects, v))
            .and_then(|v| v.as_dict().ok())
        {
            for (key, entry) in v.iter() {
                let mut ids = Vec::new();
                if let Some(e) = resolve(&doc.objects, entry) {
                    refs_in(e, &mut ids);
                }
                vri.insert(
                    String::from_utf8_lossy(key).to_ascii_uppercase(),
                    ids.into_iter().collect(),
                );
            }
        }
        (out, vri)
    }

    /// How many entries of `plan` are new or would list more items.
    #[allow(clippy::type_complexity)]
    fn vri_changes(
        &self,
        plan: &MaterialPlan,
        current: &(
            [HashMap<[u8; 32], ObjectId>; 3],
            HashMap<String, HashSet<ObjectId>>,
        ),
    ) -> usize {
        plan.vri
            .iter()
            .filter(|(key, certs, ocsps, crls)| {
                let Some(listed) = current.1.get(key) else {
                    return true;
                };
                let lists = [
                    (certs, &plan.certs, 0),
                    (ocsps, &plan.ocsps, 1),
                    (crls, &plan.crls, 2),
                ];
                lists.iter().any(|(idx, items, k)| {
                    idx.iter().any(|i| {
                        current.0[*k]
                            .get(&sha256(&items[*i]))
                            .is_none_or(|id| !listed.contains(id))
                    })
                })
            })
            .count()
    }

    /// Write the material of `plan` into the store; returns how many
    /// signature entries were written or extended.
    fn write_material(&mut self, plan: &MaterialPlan, now: i64) -> Result<usize> {
        let current = self.store_items();
        // New items become streams (always compressed).
        let mut refs: [Vec<ObjectId>; 3] = Default::default();
        let mut added: [Vec<ObjectId>; 3] = Default::default();
        for (k, list) in [&plan.certs, &plan.ocsps, &plan.crls].into_iter().enumerate() {
            for der in list {
                let id = match current.0[k].get(&sha256(der)) {
                    Some(id) => *id,
                    None => {
                        let stream = Stream::new(dictionary! { "Filter" => "FlateDecode" }, deflate(der));
                        let id = self.inner.add_object(stream);
                        added[k].push(id);
                        id
                    }
                };
                refs[k].push(id);
            }
        }
        // The store dictionary: an object of its own, edited in place.
        let catalog_dss = self.inner.catalog()?.get(b"DSS").ok().cloned();
        let dss_id = match catalog_dss {
            Some(Object::Reference(id)) if self.inner.get_dictionary(id).is_ok() => id,
            Some(Object::Dictionary(d)) => {
                let id = self.inner.add_object(d);
                self.inner.catalog_mut()?.set("DSS", id);
                id
            }
            _ => {
                let id = self.inner.add_object(dictionary! { "Type" => "DSS" });
                self.inner.catalog_mut()?.set("DSS", id);
                id
            }
        };
        for (k, key) in [&b"Certs"[..], b"OCSPs", b"CRLs"].into_iter().enumerate() {
            if !added[k].is_empty() {
                self.append_refs(dss_id, key, &added[k])?;
            }
        }
        // Entries per signature: existing ones extended (their time entry
        // kept), new ones with the time they were made.
        let vri_id = match self.inner.get_dictionary(dss_id)?.get(b"VRI").ok().cloned() {
            Some(Object::Reference(id)) if self.inner.get_dictionary(id).is_ok() => id,
            Some(Object::Dictionary(d)) => {
                let id = self.inner.add_object(d);
                self.inner.get_dictionary_mut(dss_id)?.set("VRI", id);
                id
            }
            _ => {
                let id = self.inner.add_object(Dictionary::new());
                self.inner.get_dictionary_mut(dss_id)?.set("VRI", id);
                id
            }
        };
        let tu = Object::String(
            super::pdf::pdf_date(now.max(0) as u64).into_bytes(),
            StringFormat::Literal,
        );
        let mut written = 0;
        for (key, certs, ocsps, crls) in &plan.vri {
            let lists: [Vec<ObjectId>; 3] = [
                certs.iter().map(|i| refs[0][*i]).collect(),
                ocsps.iter().map(|i| refs[1][*i]).collect(),
                crls.iter().map(|i| refs[2][*i]).collect(),
            ];
            let existing = self
                .inner
                .get_dictionary(vri_id)?
                .iter()
                .find(|(k, _)| String::from_utf8_lossy(k).eq_ignore_ascii_case(key))
                .map(|(k, v)| (k.clone(), v.clone()));
            let entry_id = match existing {
                Some((_, Object::Reference(id))) if self.inner.get_dictionary(id).is_ok() => id,
                Some((k, Object::Dictionary(d))) => {
                    let id = self.inner.add_object(d);
                    self.inner.get_dictionary_mut(vri_id)?.set(k, id);
                    id
                }
                _ => {
                    let id = self.inner.add_object(dictionary! { "Type" => "VRI" });
                    self.inner
                        .get_dictionary_mut(vri_id)?
                        .set(key.as_bytes().to_vec(), id);
                    id
                }
            };
            let mut changed = false;
            for (k, name) in [&b"Cert"[..], b"OCSP", b"CRL"].into_iter().enumerate() {
                if lists[k].is_empty() {
                    continue;
                }
                let have: HashSet<ObjectId> = {
                    let mut ids = Vec::new();
                    if let Ok(v) = self.inner.get_dictionary(entry_id)?.get(name) {
                        if let Some(o) = resolve(&self.inner.objects, v) {
                            refs_in(o, &mut ids);
                        }
                    }
                    ids.into_iter().collect()
                };
                let missing: Vec<ObjectId> = lists[k].iter().copied().filter(|r| !have.contains(r)).collect();
                if !missing.is_empty() {
                    self.append_refs(entry_id, name, &missing)?;
                    changed = true;
                }
            }
            let e = self.inner.get_dictionary_mut(entry_id)?;
            if changed && !e.has(b"TU") && !e.has(b"TS") {
                e.set("TU", tu.clone());
            }
            written += usize::from(changed);
        }
        self.mark_esic()?;
        Ok(written)
    }

    /// Append references to the array `key` of the dictionary `holder`
    /// (direct, or an array object edited in place).
    fn append_refs(&mut self, holder: ObjectId, key: &[u8], ids: &[ObjectId]) -> Result<()> {
        let at = match self.inner.get_dictionary(holder)?.get(key).ok() {
            Some(Object::Reference(id)) if self.inner.get_object(*id).and_then(Object::as_array).is_ok() => {
                ArrayAt::Object(*id)
            }
            _ => ArrayAt::Direct,
        };
        let refs = ids.iter().map(|id| Object::Reference(*id));
        match at {
            ArrayAt::Object(id) => {
                if let Object::Array(a) = self.inner.get_object_mut(id)? {
                    a.extend(refs);
                }
            }
            ArrayAt::Direct => {
                let d = self.inner.get_dictionary_mut(holder)?;
                let mut arr = match d.get(key) {
                    Ok(Object::Array(a)) => a.clone(),
                    _ => Vec::new(),
                };
                arr.extend(refs);
                d.set(key.to_vec(), Object::Array(arr));
            }
        }
        Ok(())
    }

    /// Declare the extension of the document security store (EN 319 142-1
    /// 5.6): `/ESIC` with base version 1.7, the catalog version raised to 1.7
    /// when it is lower. Nothing for PDF 2.0, which has the store, or when an
    /// indirect `/Extensions` would have to change.
    fn mark_esic(&mut self) -> Result<()> {
        let version = effective_version(&self.inner);
        if version >= (2, 0) {
            return Ok(());
        }
        let cat = self.inner.catalog_mut()?;
        let mut ext = match cat.get(b"Extensions") {
            Ok(Object::Dictionary(d)) => d.clone(),
            Ok(_) => return Ok(()),
            Err(_) => Dictionary::new(),
        };
        if !ext.has(b"ESIC") {
            ext.set(
                "ESIC",
                dictionary! { "BaseVersion" => Object::Name(b"1.7".to_vec()), "ExtensionLevel" => 1 },
            );
            cat.set("Extensions", ext);
        }
        if version < (1, 7) {
            cat.set("Version", Object::Name(b"1.7".to_vec()));
        }
        Ok(())
    }

    /// Remove the document security store and its `/ESIC` declaration (its
    /// objects too, so a file written whole no longer holds them).
    pub(crate) fn remove_dss(&mut self) {
        let doc = &self.inner;
        let Some(root) = trailer_ref(doc, b"Root") else {
            return;
        };
        let info = trailer_ref(doc, b"Info");
        let store = store_reach(doc, root).unwrap_or_default();
        let rest = document_reach(doc, root, info).unwrap_or_default();
        let gone: Vec<ObjectId> = store.difference(&rest).copied().collect();
        for id in gone {
            self.inner.objects.remove(&id);
        }
        if let Ok(cat) = self.inner.catalog_mut() {
            cat.remove(b"DSS");
            let empty = match cat.get_mut(b"Extensions") {
                Ok(Object::Dictionary(e)) => {
                    e.remove(b"ESIC");
                    e.is_empty()
                }
                _ => false,
            };
            if empty {
                cat.remove(b"Extensions");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vri_key_is_uppercase_sha1_of_the_whole_contents() {
        let mut contents = vec![0x30, 0x03, 0x02, 0x01, 0x05];
        contents.resize(16, 0);
        // SHA-1 of the 16 bytes, computed independently.
        assert_eq!(vri_key(b"abc"), "A9993E364706816ABA3E25717850C26C9CD0D89D");
        let whole = vri_key(&contents);
        let der = vri_key(&contents[..5]);
        assert_ne!(whole, der);
        let mut dss = Dss::default();
        dss.vri.insert(der.clone(), DssVri::default());
        assert!(dss.vri_for(&contents).is_some());
        dss.vri.clear();
        dss.vri.insert(whole, DssVri::default());
        assert!(dss.vri_for(&contents).is_some());
    }

    #[test]
    fn object_headers_are_found() {
        let data = b"%PDF-1.7\n1 0 obj\n<<>>\nendobj\n12 3 obj <<>> endobj\nxobj 4 0 objx\n";
        let h = object_headers(data);
        assert_eq!(h, vec![(9, 1, 0), (29, 12, 3)]);
    }

    #[test]
    fn the_renderers_reader_must_agree() {
        let mut doc = lopdf::Document::with_version("1.7");
        let content = doc.add_object(Stream::new(dictionary! {}, b"0 0 1 rg 0 0 10 10 re f".to_vec()));
        let page = doc.add_object(dictionary! {
            "Type" => "Page",
            "MediaBox" => vec![0.into(), 0.into(), 10.into(), Object::Real(10.5)],
            "Contents" => content,
            "Title" => Object::string_literal("a (page)"),
        });
        let pages = doc.add_object(
            dictionary! { "Type" => "Pages", "Kids" => vec![Object::Reference(page)], "Count" => 1 },
        );
        if let Ok(p) = doc.get_dictionary_mut(page) {
            p.set("Parent", pages);
        }
        let root = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages });
        doc.trailer.set("Root", root);
        let mut bytes = Vec::new();
        doc.save_to(&mut bytes).unwrap();
        let read = lopdf::Document::load_mem(&bytes).unwrap();
        let ids: HashSet<ObjectId> = [content, page, pages, root].into_iter().collect();
        assert!(renderer_agrees(bytes.clone(), &ids, &read, None));
        // What the library read differs from the file: the readers disagree.
        let mut changed = read.clone();
        if let Ok(p) = changed.get_dictionary_mut(page) {
            p.set("Title", Object::string_literal("another"));
        }
        assert!(!renderer_agrees(bytes.clone(), &ids, &changed, None));
        let mut content_changed = read.clone();
        if let Ok(Object::Stream(s)) = content_changed.get_object_mut(content) {
            s.set_content(b"1 0 0 rg 0 0 10 10 re f".to_vec());
        }
        assert!(!renderer_agrees(bytes.clone(), &ids, &content_changed, None));
        // An object the renderer cannot read at all.
        let mut extra = ids.clone();
        let missing = (99, 0);
        extra.insert(missing);
        let mut with_missing = read.clone();
        with_missing.objects.insert(missing, Object::Integer(1));
        assert!(!renderer_agrees(bytes, &extra, &with_missing, None));
    }

    #[test]
    fn catalogs_may_only_gain_the_store() {
        let prev = dictionary! { "Type" => "Catalog", "Pages" => Object::Reference((2, 0)) };
        let mut next = prev.clone();
        next.set("DSS", Object::Reference((9, 0)));
        next.set(
            "Extensions",
            dictionary! { "ESIC" => dictionary! { "BaseVersion" => Object::Name(b"1.7".to_vec()), "ExtensionLevel" => 1 } },
        );
        next.set("Version", Object::Name(b"1.7".to_vec()));
        assert!(catalog_ok(&prev, &next));
        let mut other = next.clone();
        other.set("OpenAction", Object::Reference((3, 0)));
        assert!(!catalog_ok(&prev, &other));
        let mut sneaky = next.clone();
        sneaky.set("Extensions", dictionary! { "ESIC" => Object::Reference((2, 0)) });
        assert!(!catalog_ok(&prev, &sneaky));
        let mut pages = next.clone();
        pages.set("Pages", Object::Reference((5, 0)));
        assert!(!catalog_ok(&prev, &pages));
        // The notation of a string is not a change (an encrypted update
        // writes strings in hexadecimal).
        let mut lang = prev.clone();
        lang.set("Lang", Object::String(b"en".to_vec(), StringFormat::Literal));
        let mut hex = lang.clone();
        hex.set("Lang", Object::String(b"en".to_vec(), StringFormat::Hexadecimal));
        hex.set("DSS", Object::Reference((9, 0)));
        assert!(catalog_ok(&lang, &hex));
        hex.set("Lang", Object::String(b"it".to_vec(), StringFormat::Hexadecimal));
        assert!(!catalog_ok(&lang, &hex));
    }
}
