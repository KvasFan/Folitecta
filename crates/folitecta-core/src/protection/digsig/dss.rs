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
                    (Some(p), Some(n)) => classify(data, prev_end, end, p, n),
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
        }
        out
    }
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

/// What the revision of `data` between `prev_end` and `end` does, from its
/// structure (never from what it says of itself):
/// - its cross-reference section is in its own bytes and follows the one
///   before, and every object it writes is one its section points to (no
///   object a reader rebuilding the table would see differently);
/// - every object the document reaches from its catalog (without the store)
///   and its document information is unchanged, dictionary and content;
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
) -> RevisionKind {
    use RevisionKind::Other;
    let base = encrypted::find_bytes(data, b"%PDF-").unwrap_or(0);
    let next_xref = next.xref_start + base;
    if next_xref < prev_end || next_xref >= end {
        return Other;
    }
    let declared = encrypted::last_startxref(&data[..prev_end]);
    if declared.is_none() || declared_prev(&data[..end], next.xref_start + base) != declared {
        return Other;
    }
    // Every object written by the revision is the one its section points to.
    for (pos, n, g) in object_headers(&data[prev_end..end]) {
        let offset = (prev_end + pos).checked_sub(base);
        let pointed = next.reference_table.get(n).is_some_and(|e| {
            matches!(e, XrefEntry::Normal { offset: o, generation } if Some(*o as usize) == offset && *generation == g)
        });
        if !pointed {
            return Other;
        }
    }
    let (Some(root), Some(prev_root)) = (trailer_ref(next, b"Root"), trailer_ref(prev, b"Root")) else {
        return Other;
    };
    if root != prev_root {
        return Other;
    }
    let (prev_info, next_info) = (trailer_ref(prev, b"Info"), trailer_ref(next, b"Info"));
    if prev_info.is_some() && prev_info != next_info {
        return Other;
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
    let (Some(r_next), Some(r_prev)) = (
        document_reach(next, root, next_info),
        document_reach(prev, root, prev_info),
    ) else {
        return Other;
    };
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
