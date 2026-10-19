//! Protected files: objects encrypted with the key of the document (the
//! values of signatures excepted), encrypted incremental updates, signature
//! values read back from the bytes their byte range leaves out, and the check
//! that a written file really carries its encryption.
//!
//! The value of a signature (`/Contents` of a signature dictionary) is never
//! encrypted: the byte range of the signature leaves exactly that string out
//! of the signed bytes, so it is written last and read back from the file as
//! it is. This is how widely used readers and writers treat it; it is read
//! tolerantly (an encrypted value of another producer is decrypted).

use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;

use lopdf::xref::XrefEntry;
use lopdf::{Dictionary, EncryptionState, IncrementalDocument, Object, ObjectId, StringFormat};

use super::security::Protection;
use crate::error::{Error, Result};

/// What an encrypted incremental update builds on: the objects of the file
/// as it was written (decrypted), and the skeleton of its last revision (its
/// trailer without `/Encrypt`, where its cross-reference section starts, the
/// highest object number and the kind of cross-reference section; no
/// objects).
#[derive(Clone)]
pub(crate) struct DecryptedBase {
    pub(crate) objects: Arc<BTreeMap<ObjectId, Object>>,
    pub(crate) skeleton: lopdf::Document,
    /// The encryption dictionary of the file, which every update points to.
    pub(crate) encrypt_ref: ObjectId,
}

/// The base of the updates of `doc`, read from a file whose last
/// cross-reference section starts at `xref_start` (call it before
/// serializing `doc`, which changes its trailer).
pub(crate) fn decrypted_base(
    doc: &lopdf::Document,
    xref_start: usize,
    encrypt_ref: ObjectId,
) -> DecryptedBase {
    DecryptedBase {
        objects: Arc::new(doc.objects.clone()),
        skeleton: skeleton(doc, &doc.trailer, xref_start, doc.max_id),
        encrypt_ref,
    }
}

/// A document without objects that stands for the last revision of a file.
fn skeleton(doc: &lopdf::Document, trailer: &Dictionary, xref_start: usize, max_id: u32) -> lopdf::Document {
    let mut s = lopdf::Document::new();
    s.version = doc.version.clone();
    s.trailer = trailer.clone();
    s.trailer.remove(b"Encrypt");
    s.trailer.remove(b"Prev");
    clean_trailer(&mut s.trailer);
    s.reference_table = lopdf::xref::Xref::new(0, doc.reference_table.cross_reference_type);
    s.xref_start = xref_start;
    s.max_id = max_id;
    s
}

/// Remove from a trailer copied from a cross-reference stream what would
/// describe the new section wrongly (it is written uncompressed).
pub(crate) fn clean_trailer(trailer: &mut Dictionary) {
    for key in [&b"DecodeParms"[..], b"Filter"] {
        trailer.remove(key);
    }
}

/// A signature dictionary (a signature or a document time stamp). Names and
/// integers are never encrypted, so this holds for encrypted and decrypted
/// objects alike.
pub(crate) fn is_signature_dict(d: &Dictionary) -> bool {
    let kind = d.get(b"Type").and_then(Object::as_name).ok();
    matches!(kind, Some(b"Sig") | Some(b"DocTimeStamp"))
        || (matches!(d.get(b"ByteRange"), Ok(Object::Array(_)))
            && matches!(d.get(b"Contents"), Ok(Object::String(..))))
}

/// Encrypt the strings and streams of `obj` (of object `id`) for a file,
/// except the value of a signature.
pub(crate) fn encrypt_for_file(state: &EncryptionState, id: ObjectId, obj: &mut Object) -> Result<()> {
    match obj {
        Object::Dictionary(d) => {
            let signature = is_signature_dict(d);
            for (key, value) in d.iter_mut() {
                if signature && key == b"Contents" {
                    continue;
                }
                encrypt_for_file(state, id, value)?;
            }
            Ok(())
        }
        Object::Array(items) => {
            for value in items {
                encrypt_for_file(state, id, value)?;
            }
            Ok(())
        }
        Object::String(..) | Object::Stream(_) => {
            lopdf::encryption::encrypt_object(state, id, obj)
                .map_err(|e| Error::ProtectionNotKept(format!("an object cannot be encrypted: {e}")))?;
            // Encrypted bytes are written in hexadecimal: in a literal string,
            // readers turn the line ends among them into other bytes.
            if let Object::String(_, format) = obj {
                *format = StringFormat::Hexadecimal;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

/// Objects the writer leaves out (rebuilt at every save).
fn written_by_the_writer(obj: &Object) -> bool {
    matches!(obj.type_name(), Ok(b"ObjStm" | b"XRef" | b"Linearized"))
}

/// Encrypt every object of `doc` with `p`, add its encryption dictionary
/// and the trailer entries it needs. Returns the dictionary's object id.
pub(crate) fn encrypt_document(doc: &mut lopdf::Document, p: &Protection) -> Result<ObjectId> {
    let dict = p.dictionary()?;
    let state = p.state();
    for (&id, obj) in doc.objects.iter_mut() {
        if written_by_the_writer(obj) {
            continue;
        }
        encrypt_for_file(state, id, obj)?;
    }
    doc.encryption_state = None;
    let id = doc.add_object(dict);
    doc.trailer.set("Encrypt", Object::Reference(id));
    ensure_id(&mut doc.trailer, p)?;
    Ok(id)
}

/// The trailer `/ID` an encrypted file needs: the one the key was made with
/// (revisions 2 to 4), else the document's own or a new one.
fn ensure_id(trailer: &mut Dictionary, p: &Protection) -> Result<()> {
    if p.state().revision() <= 4 {
        let id = p.id().cloned().ok_or_else(|| {
            Error::ProtectionNotKept("the file identifier of the encryption is missing".into())
        })?;
        trailer.set("ID", id);
    } else if trailer.get(b"ID").is_err() {
        let mut id = vec![0u8; 16];
        rand_core::RngCore::fill_bytes(&mut rand_core::OsRng, &mut id);
        trailer.set(
            "ID",
            Object::Array(vec![
                Object::String(id.clone(), StringFormat::Hexadecimal),
                Object::String(id, StringFormat::Hexadecimal),
            ]),
        );
    }
    Ok(())
}

pub(crate) fn find_bytes(hay: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || hay.len() < needle.len() {
        return None;
    }
    hay.windows(needle.len()).position(|w| w == needle)
}

fn rfind_bytes(hay: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || hay.len() < needle.len() {
        return None;
    }
    hay.windows(needle.len()).rposition(|w| w == needle)
}

/// The four numbers of a signature's `/ByteRange`.
pub(crate) fn byte_range(d: &Dictionary) -> Option<[usize; 4]> {
    let items = d.get(b"ByteRange").ok()?.as_array().ok()?;
    if items.len() != 4 {
        return None;
    }
    let mut out = [0usize; 4];
    for (o, v) in out.iter_mut().zip(items) {
        *o = usize::try_from(v.as_i64().ok()?).ok()?;
    }
    Some(out)
}

/// The bytes a byte range `[a b c d]` leaves out: `(a + b, c)`.
pub(crate) fn byte_range_gap(r: [usize; 4]) -> Option<(usize, usize)> {
    let start = r[0].checked_add(r[1])?;
    (start <= r[2]).then_some((start, r[2]))
}

/// The hexadecimal string that fills `raw[start..end]` exactly (from `<` to
/// `>`), decoded; white space is skipped and an odd last digit is followed
/// by 0.
pub(crate) fn hex_in_gap(raw: &[u8], start: usize, end: usize) -> Option<Vec<u8>> {
    if end > raw.len() || end < start + 2 || raw[start] != b'<' || raw[end - 1] != b'>' {
        return None;
    }
    let mut out = Vec::with_capacity((end - start) / 2);
    let mut high: Option<u8> = None;
    for &c in &raw[start + 1..end - 1] {
        let v = match c {
            b'0'..=b'9' => c - b'0',
            b'a'..=b'f' => c - b'a' + 10,
            b'A'..=b'F' => c - b'A' + 10,
            b' ' | b'\t' | b'\r' | b'\n' | b'\x0c' | 0 => continue,
            _ => return None,
        };
        match high.take() {
            None => high = Some(v),
            Some(h) => out.push(h << 4 | v),
        }
    }
    if let Some(h) = high {
        out.push(h << 4);
    }
    Some(out)
}

fn is_space(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\r' | b'\n' | b'\x0c' | 0)
}

/// A token of PDF syntax: what it is, where it starts and ends.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Token {
    DictOpen,
    DictClose,
    ArrayOpen,
    ArrayClose,
    Name,
    Hex,
    /// A literal string, a number, a keyword (`R`, `true`, `endobj`, ...).
    Other,
}

fn is_delimiter(c: u8) -> bool {
    matches!(
        c,
        b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b'/' | b'%'
    )
}

/// The token at or after `i` (white space and comments skipped).
fn next_token(raw: &[u8], mut i: usize) -> Option<(Token, usize, usize)> {
    loop {
        while i < raw.len() && is_space(raw[i]) {
            i += 1;
        }
        if raw.get(i) == Some(&b'%') {
            while i < raw.len() && raw[i] != b'\n' && raw[i] != b'\r' {
                i += 1;
            }
            continue;
        }
        break;
    }
    let start = i;
    let c = *raw.get(i)?;
    let two = raw.get(i + 1).copied();
    let (token, end) = match c {
        b'<' if two == Some(b'<') => (Token::DictOpen, i + 2),
        b'>' if two == Some(b'>') => (Token::DictClose, i + 2),
        b'<' => (
            Token::Hex,
            i + 1 + raw[i + 1..].iter().position(|c| *c == b'>')? + 1,
        ),
        b'[' => (Token::ArrayOpen, i + 1),
        b']' => (Token::ArrayClose, i + 1),
        b'(' => {
            // A literal string: balanced parentheses, backslash escapes.
            let mut depth = 0usize;
            let mut j = i;
            loop {
                match *raw.get(j)? {
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
            (Token::Other, j + 1)
        }
        b'/' => {
            let mut j = i + 1;
            while j < raw.len() && !is_space(raw[j]) && !is_delimiter(raw[j]) {
                j += 1;
            }
            (Token::Name, j)
        }
        _ => {
            let mut j = i + 1;
            while j < raw.len() && !is_space(raw[j]) && !is_delimiter(raw[j]) {
                j += 1;
            }
            (Token::Other, j)
        }
    };
    Some((token, start, end))
}

/// The top-level `/Contents` hexadecimal string of the dictionary of the
/// object whose header ends at `from`.
fn contents_value(raw: &[u8], from: usize) -> Option<(usize, usize)> {
    let (first, _, mut at) = next_token(raw, from)?;
    if first != Token::DictOpen {
        return None;
    }
    let mut depth = 1usize;
    let mut contents_key = false;
    // A dictionary of a signature is small; the scan stops well before the end.
    for _ in 0..100_000 {
        let (token, start, end) = next_token(raw, at)?;
        at = end;
        let key = contents_key;
        contents_key = false;
        match token {
            Token::DictOpen | Token::ArrayOpen => depth += 1,
            Token::DictClose | Token::ArrayClose => {
                depth -= 1;
                if depth == 0 {
                    return None;
                }
            }
            // A name followed by a hexadecimal string is a key and its value.
            Token::Name => contents_key = depth == 1 && &raw[start..end] == b"/Contents",
            Token::Hex if key => return Some((start, end)),
            Token::Hex => {}
            Token::Other => {
                if &raw[start..end] == b"endobj" {
                    return None;
                }
            }
        }
    }
    None
}

/// Where the headers of object `id` (`N G obj`) end in `raw` before `pos`,
/// the nearest first.
fn headers_before(raw: &[u8], pos: usize, id: ObjectId) -> Vec<usize> {
    let pattern = format!("{} {} obj", id.0, id.1);
    let mut out = Vec::new();
    let mut end = pos.min(raw.len());
    while out.len() < 16 {
        let Some(at) = rfind_bytes(&raw[..end], pattern.as_bytes()) else {
            break;
        };
        let alone = at == 0 || is_space(raw[at - 1]) || is_delimiter(raw[at - 1]);
        let after = raw.get(at + pattern.len());
        if alone && after.is_none_or(|c| is_space(*c) || is_delimiter(*c)) {
            out.push(at + pattern.len());
        }
        end = at;
    }
    out
}

/// The signature value a byte range leaves out of `raw`, when that gap is
/// exactly the top-level `/Contents` hexadecimal string of the dictionary of
/// object `holder`: a widened or moved gap (which could hide other bytes, or
/// pass another value) gives `None`. The dictionary is read token by token
/// (strings skipped), so text such as "obj" or "endobj" in its other
/// entries does not matter.
pub(crate) fn bound_value(raw: &[u8], gap: (usize, usize), holder: ObjectId) -> Option<Vec<u8>> {
    let (start, end) = gap;
    let value = hex_in_gap(raw, start, end)?;
    headers_before(raw, start, holder)
        .into_iter()
        .any(|from| contents_value(raw, from) == Some((start, end)))
        .then_some(value)
}

/// A string of object `id` decrypted with the string filter of `state`.
fn decrypt_string(state: &EncryptionState, id: ObjectId, data: &[u8]) -> Option<Vec<u8>> {
    let filter = state.get_string_filter();
    let key = filter.compute_key(state.file_encryption_key(), id).ok()?;
    filter.decrypt(&key, data).ok()
}

/// The readable signature value of a raw value found in a file: as it is
/// when it is a DER structure, else decrypted when that gives one (a
/// producer that encrypted it), else as it is.
pub(crate) fn readable_value(state: Option<&EncryptionState>, id: ObjectId, raw: &[u8]) -> Vec<u8> {
    if raw.first() == Some(&0x30) {
        return raw.to_vec();
    }
    state
        .and_then(|s| decrypt_string(s, id, raw))
        .filter(|p| p.first() == Some(&0x30))
        .unwrap_or_else(|| raw.to_vec())
}

/// After the reader decrypted `doc` (read from `raw`): give the signature
/// dictionaries their values as the file has them, and decrypt the entries
/// the reader left encrypted after a value it could not decrypt (it stops a
/// dictionary at its first failing entry). Signature dictionaries inside
/// object streams have no value in the file and are left alone.
pub(crate) fn restore_signature_values(doc: &mut lopdf::Document, state: &EncryptionState, raw: &[u8]) {
    let compressed: HashSet<u32> = doc
        .reference_table
        .entries
        .iter()
        .filter(|(_, e)| matches!(e, XrefEntry::Compressed { .. }))
        .map(|(n, _)| *n)
        .collect();
    let ids: Vec<ObjectId> = doc
        .objects
        .iter()
        .filter(|(id, o)| !compressed.contains(&id.0) && o.as_dict().is_ok_and(is_signature_dict))
        .map(|(id, _)| *id)
        .collect();
    for id in ids {
        let Some(Ok(d)) = doc.objects.get_mut(&id).map(Object::as_dict_mut) else {
            continue;
        };
        let Some(gap) = byte_range(d).and_then(byte_range_gap) else {
            continue;
        };
        let Some(raw_value) = bound_value(raw, gap, id) else {
            continue;
        };
        let value = readable_value(Some(state), id, &raw_value);
        // What the reader did with that value: when decrypting it failed, it
        // left the entries after it encrypted.
        let stopped = decrypt_string(state, id, &raw_value).is_none();
        let mut after = false;
        for (key, v) in d.iter_mut() {
            if key == b"Contents" {
                *v = Object::String(value.clone(), StringFormat::Hexadecimal);
                after = true;
            } else if after && stopped {
                let _ = lopdf::encryption::decrypt_object(state, id, v);
            }
        }
    }
}

/// Where the last cross-reference section of `data` starts (the number
/// after its last `startxref`).
pub(crate) fn last_startxref(data: &[u8]) -> Option<usize> {
    let tail_from = data.len().saturating_sub(4096);
    let at = tail_from + rfind_bytes(&data[tail_from..], b"startxref")?;
    let rest = &data[at + 9..];
    let digits: Vec<u8> = rest
        .iter()
        .copied()
        .skip_while(|c| is_space(*c))
        .take_while(u8::is_ascii_digit)
        .collect();
    std::str::from_utf8(&digits).ok()?.parse().ok()
}

/// The last trailer of `data` (the trailer dictionary of a cross-reference
/// table, or the dictionary of a cross-reference stream) has `/Encrypt`.
pub(crate) fn last_trailer_declares_encryption(data: &[u8]) -> bool {
    let Some(at) = last_startxref(data).filter(|at| *at < data.len()) else {
        return false;
    };
    let rest = &data[at..];
    let dict = if rest.starts_with(b"xref") {
        let Some(t) = find_bytes(rest, b"trailer") else {
            return false;
        };
        let end = find_bytes(&rest[t..], b"startxref").map_or(rest.len(), |e| t + e);
        &rest[t..end]
    } else {
        let end = find_bytes(rest, b"stream").unwrap_or(rest.len());
        &rest[..end]
    };
    find_bytes(dict, b"/Encrypt").is_some()
}

/// The written file `data` carries its encryption, or nothing is written.
pub(crate) fn check_written(data: &[u8]) -> Result<()> {
    if last_trailer_declares_encryption(data) {
        Ok(())
    } else {
        Err(Error::ProtectionNotKept(
            "the written file has no encryption dictionary".into(),
        ))
    }
}

/// Two objects are the same, whatever the place their stream had in the file
/// they were read from.
pub(crate) fn same_object(a: &Object, b: &Object) -> bool {
    match (a, b) {
        (Object::Stream(x), Object::Stream(y)) => x.dict == y.dict && x.content == y.content,
        _ => a == b,
    }
}

/// The objects of `objects` that differ from those of `base`, in order.
pub(crate) fn changed_objects(
    objects: &BTreeMap<ObjectId, Object>,
    base: &BTreeMap<ObjectId, Object>,
) -> Vec<ObjectId> {
    objects
        .iter()
        .filter(|(_, o)| !written_by_the_writer(o))
        .filter(|(id, o)| base.get(id).is_none_or(|b| !same_object(b, o)))
        .map(|(id, _)| *id)
        .collect()
}

/// An encrypted incremental update of the protected file `base_bytes`
/// (whose decrypted objects are `base`): the objects of the current state
/// (`objects`, `trailer`, `max_id` of a decrypted document) that changed,
/// encrypted with the key of `p` (signature values excepted), a
/// cross-reference section of the base's kind and a trailer that keeps the
/// base's `/ID` and points to its encryption dictionary. Returns the whole
/// file and the base of the next update.
///
/// This is the writer of every encrypted update: saves of signed protected
/// documents, signatures and time stamps added to them, and any other
/// revision appended to a protected signed file (for example validation
/// material) — add the new objects to the document, then call it through
/// `Document::file_bytes_from`.
pub(crate) fn protected_update(
    objects: &BTreeMap<ObjectId, Object>,
    trailer: &Dictionary,
    max_id: u32,
    base_bytes: &Arc<Vec<u8>>,
    base: &DecryptedBase,
    p: &Protection,
) -> Result<(Vec<u8>, DecryptedBase)> {
    p.writable()?;
    if base.skeleton.xref_start == 0 {
        return Err(Error::ProtectionNotKept(
            "the cross-reference table of the file was rebuilt when it was opened, so an update cannot follow it"
                .into(),
        ));
    }
    if objects.contains_key(&base.encrypt_ref) {
        return Err(Error::ProtectionNotKept(
            "an object of the document has the number of the encryption dictionary".into(),
        ));
    }
    let changed = changed_objects(objects, &base.objects);
    if changed.is_empty() {
        check_written(base_bytes)?;
        return Ok((base_bytes.to_vec(), base.clone()));
    }
    let mut inc = IncrementalDocument::create_from(base_bytes.to_vec(), base.skeleton.clone());
    for id in &changed {
        let mut o = objects[id].clone();
        encrypt_for_file(p.state(), *id, &mut o)?;
        inc.new_document.objects.insert(*id, o);
    }
    inc.new_document.max_id = inc.new_document.max_id.max(max_id);
    for key in [&b"Root"[..], b"Info"] {
        if let Ok(v) = trailer.get(key) {
            inc.new_document.trailer.set(key, v.clone());
        }
    }
    inc.new_document
        .trailer
        .set("Encrypt", Object::Reference(base.encrypt_ref));
    let mut out = Vec::new();
    inc.save_to(&mut out)
        .map_err(|e| Error::ProtectionNotKept(format!("the update cannot be written: {e}")))?;
    check_written(&out)?;
    let xref_start = last_startxref(&out)
        .ok_or_else(|| Error::ProtectionNotKept("the update has no cross-reference section".into()))?;
    let mut next = (*base.objects).clone();
    for id in &changed {
        next.insert(*id, objects[id].clone());
    }
    let new = &inc.new_document;
    let skeleton = skeleton(&base.skeleton, &new.trailer, xref_start, new.max_id);
    Ok((
        out,
        DecryptedBase {
            objects: Arc::new(next),
            skeleton,
            encrypt_ref: base.encrypt_ref,
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::dictionary;

    fn aes_state(profile: crate::security::EncryptionProfile) -> Protection {
        let mut doc = lopdf::Document::with_version("1.7");
        Protection::new("pw", "owner", Default::default(), profile, &mut doc).unwrap()
    }

    #[test]
    fn signature_values_are_not_encrypted() {
        for profile in crate::security::EncryptionProfile::ALL {
            let p = aes_state(profile);
            let sig = dictionary! {
                "Type" => "Sig",
                "ByteRange" => vec![0.into(), 10.into(), 20.into(), 30.into()],
                "Contents" => Object::String(vec![0x30, 3, 2, 1, 5], StringFormat::Hexadecimal),
                "Reason" => Object::string_literal("Secret Reason"),
            };
            let mut obj = Object::Array(vec![Object::Dictionary(sig.clone())]);
            encrypt_for_file(p.state(), (7, 0), &mut obj).unwrap();
            let d = obj.as_array().unwrap()[0].as_dict().unwrap();
            assert_eq!(d.get(b"Contents").unwrap().as_str().unwrap(), &[0x30, 3, 2, 1, 5]);
            assert_ne!(d.get(b"Reason").unwrap().as_str().unwrap(), b"Secret Reason");
            // An annotation's /Contents is ordinary text: encrypted.
            let mut note = Object::Dictionary(
                dictionary! { "Type" => "Annot", "Contents" => Object::string_literal("text") },
            );
            encrypt_for_file(p.state(), (8, 0), &mut note).unwrap();
            assert_ne!(
                note.as_dict()
                    .unwrap()
                    .get(b"Contents")
                    .unwrap()
                    .as_str()
                    .unwrap(),
                b"text"
            );
        }
    }

    #[test]
    fn gaps_bind_the_value_to_its_object() {
        let raw = b"%PDF-1.7\n5 0 obj\n<</Type /Sig /ByteRange [0 30 40 10] /Contents <3003020105>>>\nendobj\n6 0 obj\n<</A <00>>>\nendobj\n";
        let start = find_bytes(raw, b"<3003").unwrap();
        let end = start + b"<3003020105>".len();
        assert_eq!(
            bound_value(raw, (start, end), (5, 0)),
            Some(vec![0x30, 3, 2, 1, 5])
        );
        // Another object, a wider gap, a gap that is not the /Contents value.
        assert_eq!(bound_value(raw, (start, end), (6, 0)), None);
        assert_eq!(bound_value(raw, (start, end + 2), (5, 0)), None);
        let other = find_bytes(raw, b"<00>").unwrap();
        assert_eq!(bound_value(raw, (other, other + 4), (6, 0)), None);
        assert_eq!(hex_in_gap(b"<3 0a>", 0, 6), Some(vec![0x30, 0xa0]));
    }

    #[test]
    fn only_files_with_an_encryption_dictionary_pass_the_check() {
        let make = |table: bool, encrypt: bool, text: &str| {
            let mut d = crate::Document::from_text(text, Default::default()).unwrap();
            if table {
                d.inner_mut().reference_table.cross_reference_type =
                    lopdf::xref::XrefType::CrossReferenceTable;
            }
            d.commit().unwrap();
            if encrypt {
                let path = std::env::temp_dir().join(format!(
                    "folitecta-check-{}-{table}-{}.pdf",
                    std::process::id(),
                    text.len()
                ));
                d.save_encrypted(&path, "u", "o", Default::default(), Default::default())
                    .unwrap();
                let data = std::fs::read(&path).unwrap();
                let _ = std::fs::remove_file(&path);
                data
            } else {
                d.bytes().to_vec()
            }
        };
        for table in [false, true] {
            assert!(!last_trailer_declares_encryption(&make(table, false, "plain")));
            assert!(last_trailer_declares_encryption(&make(table, true, "protected")));
            assert!(!last_trailer_declares_encryption(&make(
                table,
                false,
                "/Encrypt in the text"
            )));
        }
    }
}
