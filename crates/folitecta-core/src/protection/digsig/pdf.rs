//! Signing PDF documents, time-stamping them, validating their signatures,
//! incremental updates.

use der::Encode;
use lopdf::{Dictionary, Object, ObjectId, Stream, StringFormat, dictionary};
use x509_cert::Certificate;

use std::path::PathBuf;
use std::sync::Arc;

use super::cms_sig::{issued_by, sign_detached_stamped, verify_detached};
use super::id::{CertInfo, DigitalId, cert_info};
use super::timestamp::{self, TimeStampCheck, Transport};
use crate::document::{Document, Source};
use crate::error::{Error, Result};
use crate::protection::encrypted;
use crate::util::{self, deref, dict_text, fmt_num, helvetica_width, pdf_literal, rect_of};

/// Where the signature goes.
#[derive(Debug, Clone, PartialEq)]
pub enum SignPlacement {
    /// An existing empty signature field.
    Field(String),
    /// A new visible signature on `page` inside `rect` (PDF space).
    New { page: usize, rect: [f32; 4] },
    /// No visible signature.
    Invisible,
}

/// What a visible signature shows.
#[derive(Debug, Clone, PartialEq)]
pub struct SigAppearance {
    pub show_name: bool,
    pub show_date: bool,
    pub show_reason: bool,
    pub show_location: bool,
    pub show_labels: bool,
    /// Picture shown on the left (PNG or JPEG), e.g. a handwritten signature.
    pub image: Option<Vec<u8>>,
}

impl Default for SigAppearance {
    fn default() -> Self {
        SigAppearance {
            show_name: true,
            show_date: true,
            show_reason: true,
            show_location: true,
            show_labels: true,
            image: None,
        }
    }
}

/// Options of a signature.
#[derive(Debug, Clone, PartialEq)]
pub struct SignOptions {
    pub reason: String,
    pub location: String,
    pub contact: String,
    /// Certification: 0 none, 1 no changes allowed, 2 form filling and
    /// signing, 3 also comments.
    pub certify: u8,
    pub placement: SignPlacement,
    pub appearance: SigAppearance,
    /// Signing time (Unix seconds); the current time when zero.
    pub time: u64,
    /// Labels of the appearance: "Digitally signed by", "Date", "Reason", "Location".
    pub labels: [String; 4],
}

impl Default for SignOptions {
    fn default() -> Self {
        SignOptions {
            reason: String::new(),
            location: String::new(),
            contact: String::new(),
            certify: 0,
            placement: SignPlacement::Invisible,
            appearance: SigAppearance::default(),
            time: 0,
            labels: [
                "Digitally signed by".into(),
                "Date".into(),
                "Reason".into(),
                "Location".into(),
            ],
        }
    }
}

/// Result of validating one signature.
#[derive(Debug, Clone, PartialEq)]
pub struct SignatureStatus {
    pub field: String,
    pub page: Option<usize>,
    pub signer: String,
    /// Signing time as text, from the signature or the signature dictionary.
    pub time: String,
    pub reason: String,
    pub location: String,
    pub contact: String,
    pub sub_filter: String,
    pub digest_algorithm: String,
    /// The signature is intact: the signed bytes match and the value verifies.
    pub valid: bool,
    /// The signed revision is the whole file.
    pub covers_document: bool,
    /// Certification level (DocMDP) when the signature certifies the document.
    pub certify: Option<u8>,
    /// The signer's certificate (or one of its issuers) is trusted.
    pub trusted: bool,
    pub cert: Option<CertInfo>,
    /// Issuers found in the signature, nearest first.
    pub chain: Vec<CertInfo>,
    pub problems: Vec<String>,
    /// Length of the signed revision in bytes.
    pub revision_len: usize,
    /// Every later revision of the file ends with a valid document time
    /// stamp (what else those revisions change is not analysed).
    pub later_revisions_time_stamped: bool,
    /// A document time stamp (`ETSI.RFC3161`): the time of a revision, proven
    /// by an authority, without a signer.
    pub document_time_stamp: bool,
    /// The time stamp of the signature, or the document time stamp itself.
    pub time_stamp: Option<TimeStampStatus>,
    /// The separate checks of the signature.
    pub checks: SignatureChecks,
}

/// Result of one of the separate checks of a signature.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Check {
    Passed,
    /// Passed with a reservation: an untrusted certificate, a time from the
    /// signer's computer, later revisions.
    Warning,
    Failed,
    /// Not checked by Folitecta.
    #[default]
    NotChecked,
}

/// The separate checks of a signature.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SignatureChecks {
    /// The signed bytes are unchanged (a warning when later revisions follow).
    pub integrity: Check,
    /// The signature verifies with the signer's certificate, which was valid
    /// at the signing time and is trusted (a warning when it is not trusted).
    pub identity: Check,
    /// The signing time is proven by a valid time stamp of a trusted
    /// authority (a warning when it comes from the signer's computer or from
    /// an authority that is not trusted).
    pub time: Check,
    /// Revocation of the certificates (revocation lists and OCSP answers are
    /// not read yet).
    pub revocation: Check,
}

/// The time stamp of a signature, or a document time stamp.
#[derive(Debug, Clone, PartialEq)]
pub struct TimeStampStatus {
    /// Time of the stamp as text (UTC).
    pub time: String,
    /// Time of the stamp (Unix seconds).
    pub time_unix: i64,
    /// Name of the time stamp authority.
    pub authority: String,
    pub authority_cert: Option<CertInfo>,
    /// The stamp is for this signature (or revision), verifies with the key
    /// of its authority and the authority's certificate is meant for stamps.
    pub valid: bool,
    /// The authority, or one of its issuers, is trusted.
    pub trusted: bool,
    pub problems: Vec<String>,
}

const PLACEHOLDER_RANGE: &[u8] = b"[0 1000000000 1000000000 1000000000]";
/// Space reserved for a time stamp token (bytes; written as hexadecimal).
const TOKEN_RESERVE: usize = 16 * 1024;

use encrypted::find_bytes as find;

/// A signed (or time-stamped) file, and the document it opens as.
pub struct SignedFile {
    /// The bytes of the file.
    pub bytes: Vec<u8>,
    /// For a protected document: the signed state itself, so the file opens
    /// without asking for its password again.
    document: Option<Document>,
}

impl SignedFile {
    /// The file is encrypted like the protected document it was made from.
    pub fn is_protected(&self) -> bool {
        self.document.is_some()
    }

    /// The document the file opens as, once written at `path`: the signed
    /// state of a protected document as it is, else the file read again.
    pub fn into_document(self, path: Option<PathBuf>) -> Result<Document> {
        match self.document {
            Some(mut doc) => {
                doc.set_path(path);
                Ok(doc)
            }
            None => Document::from_bytes(self.bytes, path, None),
        }
    }
}

fn hex_upper(data: &[u8]) -> Vec<u8> {
    const H: &[u8; 16] = b"0123456789ABCDEF";
    let mut out = Vec::with_capacity(data.len() * 2);
    for b in data {
        out.push(H[(b >> 4) as usize]);
        out.push(H[(b & 15) as usize]);
    }
    out
}

fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// PDF date string of a Unix time (UTC).
fn pdf_date(secs: u64) -> String {
    time::OffsetDateTime::from_unix_timestamp(secs as i64)
        .ok()
        .and_then(|t| {
            t.format(&time::macros::format_description!(
                "[year][month][day][hour][minute][second]"
            ))
            .ok()
        })
        .map(|s| format!("D:{s}Z"))
        .unwrap_or_else(util::pdf_date_now)
}

fn display_date(secs: u64) -> String {
    time::OffsetDateTime::from_unix_timestamp(secs as i64)
        .ok()
        .and_then(|t| {
            t.format(&time::macros::format_description!(
                "[year]-[month]-[day] [hour]:[minute]:[second] UTC"
            ))
            .ok()
        })
        .unwrap_or_default()
}

/// Issuers of `cert` found among `certificates`, nearest first, and whether
/// `cert` or one of them is trusted or issued by a trusted certificate.
fn chain_of(
    cert: &Certificate,
    certificates: &[Certificate],
    trusted: &[Certificate],
) -> (Vec<Certificate>, bool) {
    let mut chain: Vec<Certificate> = Vec::new();
    let mut cur = cert.clone();
    for _ in 0..8 {
        if cur.tbs_certificate.subject == cur.tbs_certificate.issuer {
            break;
        }
        match certificates.iter().find(|c| issued_by(&cur, c)) {
            Some(issuer) => {
                chain.push(issuer.clone());
                cur = issuer.clone();
            }
            None => break,
        }
    }
    let candidates: Vec<&Certificate> = std::iter::once(cert).chain(chain.iter()).collect();
    let ok = candidates.iter().any(|c| trusted.iter().any(|t| t == *c))
        || trusted
            .iter()
            .any(|t| issued_by(candidates.last().copied().unwrap_or(cert), t));
    (chain, ok)
}

fn time_stamp_status(check: &TimeStampCheck, trusted: &[Certificate]) -> TimeStampStatus {
    let (authority, authority_cert, is_trusted) = match &check.authority {
        Some(cert) => {
            let info = cert_info(cert);
            let (_, ok) = chain_of(cert, &check.certificates, trusted);
            (info.common_name.clone(), Some(info), ok)
        }
        None => (String::new(), None, false),
    };
    TimeStampStatus {
        time: display_date(check.time.max(0) as u64),
        time_unix: check.time,
        authority,
        authority_cert,
        valid: check.valid() && check.usage_ok,
        trusted: is_trusted,
        problems: check.problems.clone(),
    }
}

/// Where each revision of `data` ends: just after each `%%EOF` marker.
fn revision_ends(data: &[u8]) -> Vec<usize> {
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(i) = find(&data[from..], b"%%EOF") {
        from += i + 5;
        out.push(from);
    }
    out
}

/// The end of the first `len` bytes of `data` without the line ends and
/// spaces that close them.
fn content_end(data: &[u8], len: usize) -> usize {
    let mut end = len.min(data.len());
    while end > 0 && matches!(data[end - 1], b'\r' | b'\n' | b' ' | b'\t' | 0) {
        end -= 1;
    }
    end
}

/// The separate checks of a validated signature: `value_ok` when its value
/// verifies with the certificate, `cert_in_time` when the certificate was
/// valid at the (proven or claimed) signing time.
fn checks_of(s: &SignatureStatus, value_ok: bool, cert_in_time: bool) -> SignatureChecks {
    let integrity = if !s.valid {
        Check::Failed
    } else if !s.covers_document {
        Check::Warning
    } else {
        Check::Passed
    };
    let identity = if s.cert.is_none() || !value_ok || !cert_in_time {
        Check::Failed
    } else if s.trusted {
        Check::Passed
    } else {
        Check::Warning
    };
    let time = match &s.time_stamp {
        Some(ts) if !ts.valid => Check::Failed,
        Some(ts) if ts.trusted => Check::Passed,
        _ => Check::Warning,
    };
    SignatureChecks {
        integrity,
        identity,
        time,
        revocation: Check::NotChecked,
    }
}

impl Document {
    /// Signature fields that carry a signature: `(field object, widget, signature dictionary)`.
    fn signed_fields(&self) -> Vec<(ObjectId, String, Dictionary)> {
        let doc = &self.inner;
        self.form_fields()
            .into_iter()
            .filter(|f| f.kind == crate::forms::FieldKind::Signature)
            .filter_map(|f| {
                let d = doc.get_dictionary(f.id).ok()?;
                let v = d.get(b"V").ok().map(|v| deref(doc, v))?.as_dict().ok()?.clone();
                Some((f.id, f.name, v))
            })
            .collect()
    }

    /// The document carries digital signatures.
    pub fn has_signatures(&self) -> bool {
        !self.signed_fields().is_empty()
    }

    /// The current state as a plain incremental update of the opened (or last
    /// saved) file, when that file and the document carry signatures (saving
    /// it whole would break them).
    pub(crate) fn incremental_bytes(&self) -> Result<Option<Vec<u8>>> {
        let Some(src) = &self.source else {
            return Ok(None);
        };
        if src.decrypted.is_some() || !src.signed || !self.has_signatures() {
            return Ok(None);
        }
        let prev = lopdf::Document::load_mem(&src.bytes)?;
        if prev.xref_start == 0 {
            return Err(Error::Invalid(
                "the cross-reference table of the signed file was rebuilt when it was opened, so its signatures cannot be kept by an update; nothing was written".into(),
            ));
        }
        let changed = encrypted::changed_objects(&self.inner.objects, &prev.objects);
        if changed.is_empty() {
            return Ok(Some(src.bytes.to_vec()));
        }
        let mut inc = lopdf::IncrementalDocument::create_from(src.bytes.to_vec(), prev);
        for id in changed {
            inc.new_document
                .objects
                .insert(id, self.inner.objects[&id].clone());
        }
        inc.new_document.max_id = inc.new_document.max_id.max(self.inner.max_id);
        encrypted::clean_trailer(&mut inc.new_document.trailer);
        for key in [&b"Root"[..], b"Info"] {
            if let Ok(v) = self.inner.trailer.get(key) {
                inc.new_document.trailer.set(key, v.clone());
            }
        }
        let mut out = Vec::new();
        inc.save_to(&mut out)?;
        Ok(Some(out))
    }

    /// A copy of the current state, with the same protection and base of
    /// incremental updates, to sign or time-stamp away from the document (for
    /// example on another thread while a time stamp server answers).
    pub fn work_copy(&self) -> Result<Document> {
        let mut work = self.duplicate()?;
        work.set_path(self.path().map(|p| p.to_path_buf()));
        Ok(work)
    }

    /// Sign the document with `id`. Returns the bytes of the signed file (the
    /// document itself is not changed; open the result to continue).
    pub fn sign(&self, id: &DigitalId, opts: &SignOptions) -> Result<Vec<u8>> {
        self.sign_with(id, opts, None)
    }

    /// Sign the document with `id`; with `tsa`, the signature gets a time
    /// stamp (RFC 3161) of the authority it reaches. Returns the bytes of the
    /// signed file (the document itself is not changed).
    pub fn sign_with(
        &self,
        id: &DigitalId,
        opts: &SignOptions,
        tsa: Option<&mut Transport<'_>>,
    ) -> Result<Vec<u8>> {
        Ok(self.sign_to_file(id, opts, tsa)?.bytes)
    }

    /// Sign the document with `id` (with a time stamp of the authority `tsa`
    /// reaches): the signed file, encrypted like the document when it is
    /// protected, and the document it opens as.
    pub fn sign_to_file(
        &self,
        id: &DigitalId,
        opts: &SignOptions,
        tsa: Option<&mut Transport<'_>>,
    ) -> Result<SignedFile> {
        let time = if opts.time == 0 { now_unix() } else { opts.time };
        let info = id.info();
        let cert_sizes: usize = std::iter::once(&id.certificate)
            .chain(id.chain.iter())
            .map(|c| c.to_der().map(|d| d.len()).unwrap_or(2000))
            .sum();
        let reserve = (cert_sizes + 4096).max(8192) + if tsa.is_some() { TOKEN_RESERVE } else { 0 };
        let mut work = self.work_copy()?;
        let mut sig = dictionary! {
            "Type" => "Sig",
            "Filter" => "Adobe.PPKLite",
            "SubFilter" => "adbe.pkcs7.detached",
            "ByteRange" => vec![0.into(), 1_000_000_000i64.into(), 1_000_000_000i64.into(), 1_000_000_000i64.into()],
            "M" => Object::string_literal(pdf_date(time)),
            "Name" => lopdf::text_string(&info.common_name),
            "Prop_Build" => dictionary! { "App" => dictionary! { "Name" => "Folitecta" } },
        };
        for (key, v) in [
            ("Reason", &opts.reason),
            ("Location", &opts.location),
            ("ContactInfo", &opts.contact),
        ] {
            if !v.trim().is_empty() {
                sig.set(key, lopdf::text_string(v.trim()));
            }
        }
        if opts.certify > 0 {
            if work.has_signatures() {
                return Err(Error::Invalid("a signed document cannot be certified".into()));
            }
            sig.set(
                "Reference",
                vec![Object::Dictionary(dictionary! {
                    "Type" => "SigRef",
                    "TransformMethod" => "DocMDP",
                    "TransformParams" => dictionary! { "Type" => "TransformParams", "P" => i64::from(opts.certify.min(3)), "V" => "1.2" },
                })],
            );
        }
        // The value comes last: a reader that stops decrypting at it reads
        // every other entry first.
        sig.set(
            "Contents",
            Object::String(vec![0u8; reserve], StringFormat::Hexadecimal),
        );
        let sig_id = work.inner.add_object(sig);
        if opts.certify > 0 {
            work.inner
                .catalog_mut()?
                .set("Perms", dictionary! { "DocMDP" => sig_id });
        }
        work.attach_signature(sig_id, &info, opts, time)?;
        work.commit()?;
        work.signed_file(sig_id, reserve, |parts| {
            sign_detached_stamped(id, parts, time, tsa)
        })
    }

    /// Add a document time stamp (RFC 3161, `ETSI.RFC3161`) from the
    /// authority reached by `tsa`: the time of the document as it is now,
    /// proven by the authority. Returns the bytes of the stamped file (an
    /// incremental update of a signed file, so its signatures stay valid).
    pub fn time_stamp(&self, tsa: &mut Transport<'_>) -> Result<Vec<u8>> {
        Ok(self.time_stamp_to_file(tsa)?.bytes)
    }

    /// Add a document time stamp: the stamped file, encrypted like the
    /// document when it is protected, and the document it opens as.
    pub fn time_stamp_to_file(&self, tsa: &mut Transport<'_>) -> Result<SignedFile> {
        let mut work = self.work_copy()?;
        let sig = dictionary! {
            "Type" => "DocTimeStamp",
            "Filter" => "Adobe.PPKLite",
            "SubFilter" => "ETSI.RFC3161",
            "ByteRange" => vec![0.into(), 1_000_000_000i64.into(), 1_000_000_000i64.into(), 1_000_000_000i64.into()],
            "Contents" => Object::String(vec![0u8; TOKEN_RESERVE], StringFormat::Hexadecimal),
        };
        let sig_id = work.inner.add_object(sig);
        let acro = work.ensure_acroform()?;
        if let Ok(a) = work.inner.get_dictionary_mut(acro) {
            a.set("SigFlags", 3);
        }
        work.invisible_field(acro, sig_id, "Timestamp")?;
        work.commit()?;
        work.signed_file(sig_id, TOKEN_RESERVE, |parts| timestamp::stamp(parts, tsa))
    }

    /// The file of this signed state: the byte range written and the
    /// signature made by `make` over the bytes around the `/Contents` value
    /// of `sig_id` (`reserve` bytes of zeros) put in its place. A protected
    /// document is written encrypted (the value excepted) and becomes the
    /// document the file opens as.
    fn signed_file(
        mut self,
        sig_id: ObjectId,
        reserve: usize,
        make: impl FnOnce(&[&[u8]]) -> Result<Vec<u8>>,
    ) -> Result<SignedFile> {
        let file = self.file_bytes_from()?;
        let (mut data, from) = (file.bytes, file.from);
        // Byte range: everything except the /Contents value (searched in what
        // this write added, never in an earlier revision).
        let zeros = vec![b'0'; reserve * 2];
        let mut needle = vec![b'<'];
        needle.extend(&zeros);
        needle.push(b'>');
        let start = from
            + find(&data[from..], &needle)
                .ok_or_else(|| Error::Invalid("signature placeholder not found".into()))?;
        let end = start + needle.len();
        let range_pos = from
            + find(&data[from..], PLACEHOLDER_RANGE)
                .ok_or_else(|| Error::Invalid("byte range placeholder not found".into()))?;
        let ranges = [0usize, start, end, data.len() - end];
        let mut text = format!("[{} {} {} {}]", ranges[0], ranges[1], ranges[2], ranges[3]).into_bytes();
        if text.len() > PLACEHOLDER_RANGE.len() {
            return Err(Error::Invalid("the document is too large to sign".into()));
        }
        // Keep the length: pad before the closing bracket.
        let close = text.pop().expect("bracket");
        text.resize(PLACEHOLDER_RANGE.len() - 1, b' ');
        text.push(close);
        data[range_pos..range_pos + text.len()].copy_from_slice(&text);
        let cms = make(&[&data[..start], &data[end..]])?;
        let hex = hex_upper(&cms);
        if hex.len() > zeros.len() {
            return Err(Error::Invalid(
                "the signature does not fit the reserved space".into(),
            ));
        }
        data[start + 1..start + 1 + hex.len()].copy_from_slice(&hex);
        let Some(protection) = self.protection().cloned() else {
            return Ok(SignedFile {
                bytes: data,
                document: None,
            });
        };
        // The signed state of a protected document is the document the file
        // opens as: its signature dictionary gets what the file has.
        let mut value = cms;
        value.resize(reserve, 0);
        let fill = |objects: &mut std::collections::BTreeMap<ObjectId, Object>| {
            if let Some(Ok(d)) = objects.get_mut(&sig_id).map(Object::as_dict_mut) {
                d.set(
                    "ByteRange",
                    ranges
                        .iter()
                        .map(|v| Object::Integer(*v as i64))
                        .collect::<Vec<_>>(),
                );
                d.set(
                    "Contents",
                    Object::String(value.clone(), StringFormat::Hexadecimal),
                );
            }
        };
        let mut next = file
            .next
            .ok_or_else(|| Error::ProtectionNotKept("the signed file has no base for its updates".into()))?;
        fill(&mut self.inner.objects);
        fill(Arc::make_mut(&mut next.objects));
        self.inner.max_id = self.inner.max_id.max(next.skeleton.max_id);
        let bytes = Arc::new(data.clone());
        let doc = Document::signed_state(
            self,
            protection,
            Source {
                bytes,
                signed: true,
                decrypted: Some(Arc::new(next)),
            },
        )?;
        Ok(SignedFile {
            bytes: data,
            document: Some(doc),
        })
    }

    /// Put the signature dictionary into a field and draw its appearance.
    fn attach_signature(
        &mut self,
        sig_id: ObjectId,
        info: &CertInfo,
        opts: &SignOptions,
        time: u64,
    ) -> Result<()> {
        let acro = self.ensure_acroform()?;
        if let Ok(a) = self.inner.get_dictionary_mut(acro) {
            a.set("SigFlags", 3);
        }
        let (widget, rect) = match &opts.placement {
            SignPlacement::Field(name) => {
                let field = self
                    .form_fields()
                    .into_iter()
                    .find(|f| f.name == *name && f.kind == crate::forms::FieldKind::Signature)
                    .ok_or_else(|| Error::Invalid(format!("no signature field named '{name}'")))?;
                if self.inner.get_dictionary(field.id)?.has(b"V") {
                    return Err(Error::Invalid("the signature field is already signed".into()));
                }
                self.inner.get_dictionary_mut(field.id)?.set("V", sig_id);
                let widget = self.field_widgets(field.id).first().copied().unwrap_or(field.id);
                let rect = self
                    .inner
                    .get_dictionary(widget)?
                    .get(b"Rect")
                    .ok()
                    .and_then(|r| rect_of(&self.inner, r))
                    .unwrap_or([0.0; 4]);
                (widget, rect)
            }
            SignPlacement::New { page, rect } => {
                let page_id = self.page_id(*page)?;
                let name = self.unique_field_name("Signature");
                let n = crate::geom::normalize(*rect);
                let d = dictionary! {
                    "Type" => "Annot",
                    "Subtype" => "Widget",
                    "FT" => "Sig",
                    "T" => lopdf::text_string(&name),
                    "Rect" => n.iter().map(|v| Object::Real(*v)).collect::<Vec<_>>(),
                    "F" => 132,
                    "P" => page_id,
                    "V" => sig_id,
                };
                let id = self.push_annotation(page_id, d)?;
                self.add_acroform_field(acro, id)?;
                (id, n)
            }
            SignPlacement::Invisible => (self.invisible_field(acro, sig_id, "Signature")?, [0.0; 4]),
        };
        let (w, h) = (rect[2] - rect[0], rect[3] - rect[1]);
        if w >= 2.0 && h >= 2.0 {
            let ap = self.signature_appearance(w, h, info, opts, time)?;
            let wd = self.inner.get_dictionary_mut(widget)?;
            wd.set("AP", dictionary! { "N" => ap });
            wd.set("F", 132);
        }
        Ok(())
    }

    /// A new invisible signature field on the first page holding the
    /// signature dictionary `sig_id`, named after `base`.
    fn invisible_field(&mut self, acro: ObjectId, sig_id: ObjectId, base: &str) -> Result<ObjectId> {
        let page_id = self.page_id(0)?;
        let name = self.unique_field_name(base);
        let d = dictionary! {
            "Type" => "Annot",
            "Subtype" => "Widget",
            "FT" => "Sig",
            "T" => lopdf::text_string(&name),
            "Rect" => vec![0.into(), 0.into(), 0.into(), 0.into()],
            "F" => 132,
            "P" => page_id,
            "V" => sig_id,
        };
        let id = self.push_annotation(page_id, d)?;
        self.add_acroform_field(acro, id)?;
        Ok(id)
    }

    fn add_acroform_field(&mut self, acro: ObjectId, field: ObjectId) -> Result<()> {
        let fields = self.inner.get_dictionary(acro)?.get(b"Fields").ok().cloned();
        let mut arr = match fields {
            Some(Object::Array(a)) => a,
            Some(Object::Reference(r)) => self.inner.get_object(r)?.as_array()?.clone(),
            _ => Vec::new(),
        };
        arr.push(Object::Reference(field));
        self.inner
            .get_dictionary_mut(acro)?
            .set("Fields", Object::Array(arr));
        Ok(())
    }

    /// Appearance of a visible signature: picture on the left, text on the right.
    fn signature_appearance(
        &mut self,
        w: f32,
        h: f32,
        info: &CertInfo,
        opts: &SignOptions,
        time: u64,
    ) -> Result<ObjectId> {
        let a = &opts.appearance;
        let l = &opts.labels;
        let label = |i: usize, v: &str| {
            if a.show_labels {
                format!("{}: {v}", l[i])
            } else {
                v.to_string()
            }
        };
        let mut lines = Vec::new();
        if a.show_name {
            if a.show_labels {
                lines.push(l[0].clone());
            }
            lines.push(info.common_name.clone());
        }
        if a.show_date {
            lines.push(label(1, &display_date(time)));
        }
        if a.show_reason && !opts.reason.trim().is_empty() {
            lines.push(label(2, opts.reason.trim()));
        }
        if a.show_location && !opts.location.trim().is_empty() {
            lines.push(label(3, opts.location.trim()));
        }
        let mut resources = Dictionary::new();
        let mut body = String::new();
        let f = fmt_num;
        let mut text_x = 2.0;
        if let Some(img) = &a.image {
            let obj = crate::images::image_object(img.clone())?;
            let (iw, ih) = (obj.width.max(1.0), obj.height.max(1.0));
            let mut stream = obj.stream;
            if let Some(mask) = obj.smask {
                let mid = self.inner.add_object(mask);
                stream.dict.set("SMask", mid);
            }
            let img_id = self.inner.add_object(stream);
            resources.set("XObject", dictionary! { "Img" => img_id });
            let box_w = if lines.is_empty() { w } else { w * 0.42 };
            let s = (box_w / iw).min(h / ih);
            let (dw, dh) = (iw * s, ih * s);
            body.push_str(&format!(
                "q {} 0 0 {} {} {} cm /Img Do Q\n",
                f(dw),
                f(dh),
                f((box_w - dw) / 2.0),
                f((h - dh) / 2.0)
            ));
            text_x = box_w + 4.0;
        }
        if !lines.is_empty() {
            let font = self.helvetica_font_id();
            resources.set("Font", dictionary! { "Helv" => font });
            let avail_w = (w - text_x - 2.0).max(10.0);
            let widest = lines
                .iter()
                .map(|l| helvetica_width(l, 1.0))
                .fold(1.0f32, f32::max);
            let fs = (avail_w / widest)
                .min((h - 4.0) / (lines.len() as f32 * 1.2))
                .clamp(3.0, 14.0);
            let block = lines.len() as f32 * fs * 1.2;
            let mut y = h - (h - block) / 2.0 - fs;
            body.push_str(&format!("BT /Helv {} Tf 0 0 0 rg\n", f(fs)));
            for line in &lines {
                body.push_str(&format!(
                    "1 0 0 1 {} {} Tm {} Tj\n",
                    f(text_x),
                    f(y),
                    String::from_utf8_lossy(&pdf_literal(line))
                ));
                y -= fs * 1.2;
            }
            body.push_str("ET\n");
        }
        let stream = Stream::new(
            dictionary! {
                "Type" => "XObject",
                "Subtype" => "Form",
                "BBox" => vec![0.into(), 0.into(), w.into(), h.into()],
                "Resources" => resources,
            },
            body.into_bytes(),
        );
        Ok(self.inner.add_object(stream))
    }

    /// The bytes signatures are checked against: the file as it was opened or
    /// last saved (encrypted when it is), else the current state.
    fn signed_bytes(&self) -> Arc<Vec<u8>> {
        self.source
            .as_ref()
            .map(|s| s.bytes.clone())
            .unwrap_or_else(|| self.bytes())
    }

    /// The object that holds the signature dictionary of field `field_id`.
    fn signature_holder(&self, field_id: ObjectId) -> ObjectId {
        self.inner
            .get_dictionary(field_id)
            .ok()
            .and_then(|d| d.get(b"V").ok())
            .and_then(|v| v.as_reference().ok())
            .unwrap_or(field_id)
    }

    /// Validate every signature against the file as it was opened or saved.
    pub fn validate_signatures(&self, trusted: &[Certificate]) -> Vec<SignatureStatus> {
        let data = self.signed_bytes();
        let state = match (&self.source, self.protection()) {
            (
                Some(Source {
                    decrypted: Some(_), ..
                }),
                Some(p),
            ) => Some(p.state()),
            _ => None,
        };
        let doc = &self.inner;
        let pages = self.page_ids();
        let mut out = Vec::new();
        for (field_id, name, sig) in self.signed_fields() {
            let text = |k: &[u8]| dict_text(doc, &sig, k).unwrap_or_default();
            let sub_filter = sig
                .get(b"SubFilter")
                .ok()
                .and_then(|s| s.as_name().ok())
                .map(|s| String::from_utf8_lossy(s).to_string())
                .unwrap_or_default();
            let page = self
                .field_widgets(field_id)
                .first()
                .and_then(|w| doc.get_dictionary(*w).ok())
                .and_then(|w| w.get(b"P").ok())
                .and_then(|p| p.as_reference().ok())
                .and_then(|p| pages.iter().position(|x| *x == p));
            let certify = sig
                .get(b"Reference")
                .ok()
                .map(|r| deref(doc, r))
                .and_then(|r| r.as_array().ok())
                .and_then(|refs| {
                    refs.iter().find_map(|r| {
                        let r = deref(doc, r).as_dict().ok()?;
                        if r.get(b"TransformMethod").ok()?.as_name().ok()? != b"DocMDP" {
                            return None;
                        }
                        let p = r
                            .get(b"TransformParams")
                            .ok()
                            .map(|t| deref(doc, t))
                            .and_then(|t| t.as_dict().ok())
                            .and_then(|t| t.get(b"P").ok())
                            .and_then(|p| p.as_i64().ok())
                            .unwrap_or(2);
                        Some(p.clamp(1, 3) as u8)
                    })
                });
            let mut status = SignatureStatus {
                field: name,
                page,
                signer: text(b"Name"),
                time: util::pdf_date_display(&text(b"M")),
                reason: text(b"Reason"),
                location: text(b"Location"),
                contact: text(b"ContactInfo"),
                sub_filter,
                digest_algorithm: String::new(),
                valid: false,
                covers_document: false,
                certify,
                trusted: false,
                cert: None,
                chain: Vec::new(),
                problems: Vec::new(),
                revision_len: 0,
                later_revisions_time_stamped: false,
                document_time_stamp: false,
                time_stamp: None,
                checks: SignatureChecks::default(),
            };
            let ranges: Vec<usize> = sig
                .get(b"ByteRange")
                .ok()
                .map(|r| deref(doc, r))
                .and_then(|r| r.as_array().ok())
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_i64().ok())
                        .map(|v| v.max(0) as usize)
                        .collect()
                })
                .unwrap_or_default();
            let model_contents = match sig.get(b"Contents").ok().map(|c| deref(doc, c)) {
                Some(Object::String(b, _)) => b.clone(),
                _ => Vec::new(),
            };
            if ranges.len() != 4 || model_contents.is_empty() {
                status
                    .problems
                    .push("the signature has no byte range or contents".into());
                status.checks = checks_of(&status, false, false);
                out.push(status);
                continue;
            }
            let (a, b, c, d) = (ranges[0], ranges[1], ranges[2], ranges[3]);
            if a + b > data.len() || c + d > data.len() || a + b > c {
                status
                    .problems
                    .push("the byte range does not fit the file".into());
                status.checks = checks_of(&status, false, false);
                out.push(status);
                continue;
            }
            // The value is the one the byte range leaves out, and that gap is
            // exactly the /Contents string of this signature: a gap moved or
            // widened (to hide other bytes) makes the signature invalid.
            let holder = self.signature_holder(field_id);
            let contents = encrypted::bound_value(&data, (a + b, c), holder)
                .map(|raw| encrypted::readable_value(state, holder, &raw))
                .filter(|v| *v == model_contents);
            let Some(contents) = contents else {
                status
                    .problems
                    .push("the bytes the byte range leaves out are not the value of this signature".into());
                status.checks = checks_of(&status, false, false);
                out.push(status);
                continue;
            };
            status.revision_len = c + d;
            status.covers_document = c + d == data.len();
            let parts = [&data[a..a + b], &data[c..c + d]];
            let mut value_ok = false;
            let mut cert_in_time = true;
            if status.sub_filter == "ETSI.RFC3161" {
                // A document time stamp: the token itself is the signature.
                status.document_time_stamp = true;
                match timestamp::check_token(&contents, &parts) {
                    Ok(check) => {
                        let ts = time_stamp_status(&check, trusted);
                        status.valid = check.valid();
                        value_ok = check.signature_ok;
                        status.digest_algorithm = check.digest_algorithm.clone();
                        status.problems.extend(check.problems.iter().cloned());
                        status.signer = ts.authority.clone();
                        status.time = ts.time.clone();
                        status.trusted = ts.trusted;
                        status.cert = ts.authority_cert.clone();
                        if let Some(cert) = &check.authority {
                            let (chain, _) = chain_of(cert, &check.certificates, trusted);
                            status.chain = chain.iter().map(cert_info).collect();
                        }
                        status.time_stamp = Some(ts);
                    }
                    Err(e) => status.problems.push(e.to_string()),
                }
            } else {
                match verify_detached(&contents, &parts) {
                    Ok(check) => {
                        status.valid = check.digest_ok && check.signature_ok;
                        value_ok = check.signature_ok;
                        status.digest_algorithm = check.digest_algorithm.clone();
                        status.problems.extend(check.problems.iter().cloned());
                        if let Some(t) = check.signing_time {
                            status.time = display_date(t.max(0) as u64);
                        }
                        if let Some(tc) = &check.time_stamp {
                            let ts = time_stamp_status(tc, trusted);
                            for p in &ts.problems {
                                status.problems.push(format!("time stamp: {p}"));
                            }
                            status.time_stamp = Some(ts);
                        }
                        if let Some(cert) = &check.signer {
                            let ci = cert_info(cert);
                            if status.signer.is_empty() {
                                status.signer = ci.common_name.clone();
                            }
                            // Certificate validity at the signing time: the time of a
                            // valid time stamp, otherwise the time the signer wrote.
                            let at = status
                                .time_stamp
                                .as_ref()
                                .filter(|ts| ts.valid)
                                .map(|ts| ts.time_unix)
                                .or(check.signing_time);
                            if let Some(t) = at {
                                if t < ci.not_before_unix || t > ci.not_after_unix {
                                    cert_in_time = false;
                                    status
                                        .problems
                                        .push("the certificate was not valid at the signing time".into());
                                }
                            }
                            // Issuer chain from the certificates in the signature.
                            let (chain, ok) = chain_of(cert, &check.certificates, trusted);
                            status.trusted = ok;
                            status.chain = chain.iter().map(cert_info).collect();
                            status.cert = Some(ci);
                        }
                    }
                    Err(e) => status.problems.push(e.to_string()),
                }
            }
            if !status.covers_document {
                status.problems.push(match status.certify {
                    Some(1) => {
                        "the document was changed after certification, which the certification does not allow"
                            .into()
                    }
                    _ if status.document_time_stamp => {
                        "the document was changed after the time stamp (in a later revision)".into()
                    }
                    _ => "the document was changed after it was signed (in a later revision)".into(),
                });
            }
            status.checks = checks_of(&status, value_ok, cert_in_time);
            out.push(status);
        }
        // Signatures followed only by revisions that end with document time stamps.
        let stamped: Vec<usize> = out
            .iter()
            .filter(|s| s.document_time_stamp && s.valid)
            .map(|s| content_end(&data, s.revision_len))
            .collect();
        let ends = revision_ends(&data);
        for s in out
            .iter_mut()
            .filter(|s| !s.covers_document && s.revision_len > 0)
        {
            let own = content_end(&data, s.revision_len);
            let later: Vec<usize> = ends.iter().copied().filter(|e| *e > own).collect();
            if later.is_empty() || !later.iter().all(|e| stamped.contains(e)) {
                continue;
            }
            s.later_revisions_time_stamped = true;
            if s.certify != Some(1) {
                for p in s
                    .problems
                    .iter_mut()
                    .filter(|p| p.ends_with("(in a later revision)"))
                {
                    *p = "later revisions add document time stamps (their other changes are not analysed)"
                        .into();
                }
            }
        }
        out
    }

    /// Certificates carried by a signature, the signer's first.
    pub fn signer_certificates(&self, field: &str) -> Vec<Certificate> {
        let doc = &self.inner;
        let Some((_, _, sig)) = self.signed_fields().into_iter().find(|(_, n, _)| n == field) else {
            return Vec::new();
        };
        let contents = match sig.get(b"Contents").ok().map(|c| deref(doc, c)) {
            Some(Object::String(b, _)) => b.clone(),
            _ => return Vec::new(),
        };
        match verify_detached(&contents, &[]) {
            Ok(check) => {
                let mut out: Vec<Certificate> = check.signer.into_iter().collect();
                for c in check.certificates {
                    if !out.contains(&c) {
                        out.push(c);
                    }
                }
                out
            }
            Err(_) => Vec::new(),
        }
    }

    /// Bytes of the revision a signature covers (the document as it was
    /// signed; encrypted like the file when it is).
    pub fn signed_revision(&self, field: &str) -> Result<Vec<u8>> {
        let data = self.signed_bytes();
        let (_, _, sig) = self
            .signed_fields()
            .into_iter()
            .find(|(_, n, _)| n == field)
            .ok_or_else(|| Error::Invalid(format!("no signature named '{field}'")))?;
        let ranges: Vec<usize> = sig
            .get(b"ByteRange")
            .ok()
            .and_then(|r| r.as_array().ok())
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_i64().ok())
                    .map(|v| v.max(0) as usize)
                    .collect()
            })
            .unwrap_or_default();
        if ranges.len() != 4 || ranges[2] + ranges[3] > data.len() {
            return Err(Error::Invalid("the signature has no valid byte range".into()));
        }
        Ok(data[..ranges[2] + ranges[3]].to_vec())
    }

    /// Remove every signature (the fields stay, empty) and the certification
    /// (one undo step). Returns how many signatures were removed.
    pub fn clear_signatures(&mut self) -> Result<usize> {
        if self.signed_fields().is_empty() {
            return Ok(0);
        }
        self.begin_edit();
        let removed = self.clear_signatures_inner();
        self.commit()?;
        Ok(removed)
    }

    /// The number of signed signature fields.
    pub(crate) fn signed_field_count(&self) -> usize {
        self.signed_fields().len()
    }

    /// Remove every signature, the certification and the validation material
    /// of the signers (`/DSS`), without an undo step: the fields stay, empty,
    /// and the form no longer says it holds signatures to append to. Returns
    /// how many signatures were removed.
    pub(crate) fn clear_signatures_inner(&mut self) -> usize {
        let signed = self.signed_fields();
        if signed.is_empty() {
            return 0;
        }
        for (field, _, _) in &signed {
            if let Ok(d) = self.inner.get_dictionary_mut(*field) {
                d.remove(b"V");
            }
            for w in self.field_widgets(*field) {
                if let Ok(d) = self.inner.get_dictionary_mut(w) {
                    d.remove(b"AP");
                }
            }
        }
        if let Ok(c) = self.inner.catalog_mut() {
            c.remove(b"Perms");
            c.remove(b"DSS");
        }
        // SignaturesExist and AppendOnly (ISO 32000-1, Table 219).
        let form = self
            .inner
            .catalog()
            .ok()
            .and_then(|c| c.get(b"AcroForm").ok())
            .and_then(|f| f.as_reference().ok());
        let clear = |d: &mut lopdf::Dictionary| {
            d.remove(b"SigFlags");
        };
        match form {
            Some(id) => {
                if let Ok(d) = self.inner.get_dictionary_mut(id) {
                    clear(d);
                }
            }
            None => {
                if let Ok(Object::Dictionary(d)) =
                    self.inner.catalog_mut().and_then(|c| c.get_mut(b"AcroForm"))
                {
                    clear(d);
                }
            }
        }
        // Without signatures the document is written whole (the base of
        // updates stays, for an undo that brings them back).
        signed.len()
    }
}

/// Helper used by tests and the interface: the signature fields that are empty.
impl Document {
    pub fn empty_signature_fields(&self) -> Vec<String> {
        let doc = &self.inner;
        self.form_fields()
            .into_iter()
            .filter(|f| f.kind == crate::forms::FieldKind::Signature)
            .filter(|f| doc.get_dictionary(f.id).map(|d| !d.has(b"V")).unwrap_or(false))
            .map(|f| f.name)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use lopdf::encryption::crypt_filters::{Aes256CryptFilter, CryptFilter};
    use lopdf::{EncryptionState, EncryptionVersion};

    use super::*;
    use crate::security::{EncryptionProfile, Protection};

    fn rc4_protection(doc: &mut Document) -> Protection {
        let id = vec![5u8; 16];
        doc.inner.trailer.set(
            "ID",
            Object::Array(vec![
                Object::String(id.clone(), StringFormat::Hexadecimal),
                Object::String(id, StringFormat::Hexadecimal),
            ]),
        );
        let state = EncryptionState::try_from(EncryptionVersion::V2 {
            document: &doc.inner,
            owner_password: "owner",
            user_password: "pw",
            key_length: 128,
            permissions: Default::default(),
        })
        .unwrap();
        Protection::opened(state, doc.inner.trailer.get(b"ID").ok().cloned(), None, None)
    }

    /// A blank document protected by `kind` (0 AES-128, 1 AES-256, 2 RC4).
    fn protected_blank(kind: usize) -> Document {
        let mut doc = Document::new_blank(1, 200.0, 200.0).unwrap();
        let p = match kind {
            2 => rc4_protection(&mut doc),
            _ => {
                let profile = EncryptionProfile::ALL[1 - kind];
                Protection::new("pw", "owner", Default::default(), profile, &mut doc.inner).unwrap()
            }
        };
        doc.inherit_protection(&p);
        doc
    }

    #[test]
    fn encrypted_strings_with_line_ends_are_written_in_hexadecimal() {
        // RC4 is deterministic: choose the plain text whose encrypted bytes
        // hold line ends, parentheses and a backslash, which a literal string
        // would not carry through a reader.
        let mut doc = protected_blank(2);
        let note = doc
            .add_note(0, 20.0, 180.0, "placeholder", "Ann", [1.0, 1.0, 0.0])
            .unwrap();
        let wanted = b"A\r\nB\n\nC\rD)E\\F(G\n".to_vec();
        let state = doc.protection().unwrap().state().clone();
        let filter = state.get_string_filter();
        let key = filter.compute_key(state.file_encryption_key(), note).unwrap();
        let plain = filter.decrypt(&key, &wanted).unwrap();
        doc.inner
            .get_dictionary_mut(note)
            .unwrap()
            .set("Contents", Object::String(plain.clone(), StringFormat::Literal));
        doc.commit().unwrap();
        let raw = doc.file_bytes_from().unwrap().bytes;
        // Written as a hexadecimal string, with the encryption dictionary.
        let hex: Vec<u8> = hex_upper(&wanted);
        let lower = String::from_utf8(hex.clone())
            .unwrap()
            .to_lowercase()
            .into_bytes();
        assert!(
            find(&raw, &hex).is_some() || find(&raw, &lower).is_some(),
            "the encrypted value is not written in hexadecimal"
        );
        assert!(find(&raw, &wanted).is_none());
        let back = Document::from_bytes(raw.clone(), None, Some("pw")).unwrap();
        let contents = back
            .inner
            .get_dictionary(note)
            .unwrap()
            .get(b"Contents")
            .unwrap()
            .as_str()
            .unwrap()
            .to_vec();
        assert_eq!(contents, plain);
        let r = crate::render::Renderer::new_with_password(std::sync::Arc::new(raw), "pw").unwrap();
        r.render_rgba(0, 1.0, Default::default()).unwrap();
        // The binary strings of encryption dictionaries too.
        for kind in 0..3 {
            let d = protected_blank(kind).protection().unwrap().dictionary().unwrap();
            for key in [&b"O"[..], b"U"] {
                assert!(
                    matches!(d.get(key), Ok(Object::String(_, StringFormat::Hexadecimal))),
                    "{kind}"
                );
            }
        }
    }

    #[test]
    fn signature_dictionaries_are_read_back_whole() {
        // What the PDF library's reader does with a signature value it cannot
        // decrypt (it stops the dictionary there) is repaired whatever the
        // encryption and wherever the value is.
        for kind in 0..3 {
            for contents_last in [false, true] {
                let mut doc = protected_blank(kind);
                let mut sig = dictionary! {
                    "Type" => "Sig",
                    "Filter" => "Adobe.PPKLite",
                    "SubFilter" => "adbe.pkcs7.detached",
                    "ByteRange" => vec![0.into(), 1_000_000_000i64.into(), 1_000_000_000i64.into(), 1_000_000_000i64.into()],
                };
                let contents = Object::String(vec![0u8; 64], StringFormat::Hexadecimal);
                if !contents_last {
                    sig.set("Contents", contents.clone());
                }
                sig.set("Reason", Object::string_literal("Secret Reason"));
                sig.set("M", Object::string_literal("D:20261019120000Z"));
                if contents_last {
                    sig.set("Contents", contents);
                }
                let sig_id = doc.inner.add_object(sig);
                let acro = doc.ensure_acroform().unwrap();
                doc.invisible_field(acro, sig_id, "Signature").unwrap();
                doc.commit().unwrap();
                let file = doc
                    .signed_file(sig_id, 64, |_| Ok(vec![0x30, 3, 2, 1, 5]))
                    .unwrap();
                assert!(!file.bytes.windows(13).any(|w| w == b"Secret Reason"));
                let back = Document::from_bytes(file.bytes, None, Some("pw")).unwrap();
                let d = back.inner.get_dictionary(sig_id).unwrap();
                let text = |k: &[u8]| d.get(k).unwrap().as_str().unwrap().to_vec();
                assert_eq!(
                    text(b"Reason"),
                    b"Secret Reason",
                    "kind {kind}, last {contents_last}"
                );
                assert_eq!(text(b"M"), b"D:20261019120000Z");
                let mut expected = vec![0x30, 3, 2, 1, 5];
                expected.resize(64, 0);
                assert_eq!(text(b"Contents"), expected);
            }
        }
    }

    #[test]
    fn values_encrypted_by_their_producer_are_read() {
        // Another producer may encrypt the value of a signature: it is read
        // decrypted, and the signature validates.
        let id = DigitalId::create_self_signed(
            &crate::digsig::IdentityInfo {
                name: "Producer".into(),
                ..Default::default()
            },
            crate::digsig::KeyKind::EcP256,
            1,
        )
        .unwrap();
        let signed = protected_blank(1).sign(&id, &SignOptions::default()).unwrap();
        let back = Document::from_bytes(signed.clone(), None, Some("pw")).unwrap();
        let (field_id, _, sig) = back.signed_fields()[0].clone();
        let holder = back.signature_holder(field_id);
        let (start, end) = encrypted::byte_range(&sig)
            .and_then(encrypted::byte_range_gap)
            .unwrap();
        let value = encrypted::hex_in_gap(&signed, start, end).unwrap();
        let state = back.protection().unwrap().state();
        let filter = state.get_string_filter();
        let key = filter.compute_key(state.file_encryption_key(), holder).unwrap();
        // The vector and the padding take 32 bytes of the room.
        let n = (value.len() - 32) / 16 * 16;
        let cipher = loop {
            let c = filter.encrypt(&key, &value[..n]).unwrap();
            if c[0] != 0x30 {
                break c;
            }
        };
        let hex = hex_upper(&cipher);
        let mut changed = signed.clone();
        changed[start + 1..start + 1 + hex.len()].copy_from_slice(&hex);
        for b in &mut changed[start + 1 + hex.len()..end - 1] {
            *b = b' ';
        }
        let v = Document::from_bytes(changed, None, Some("pw"))
            .unwrap()
            .validate_signatures(&[]);
        assert!(v[0].valid && v[0].covers_document, "{:?}", v[0].problems);
    }

    #[test]
    fn a_protection_that_cannot_encrypt_stops_the_save() {
        let mut doc = Document::new_blank(1, 200.0, 200.0).unwrap();
        doc.inner.trailer.set(
            "ID",
            Object::Array(vec![
                Object::String(vec![1; 16], StringFormat::Hexadecimal),
                Object::String(vec![1; 16], StringFormat::Hexadecimal),
            ]),
        );
        // A 128-bit key for an AES-256 filter: nothing can be encrypted.
        let mut filters: BTreeMap<Vec<u8>, std::sync::Arc<dyn CryptFilter>> = BTreeMap::new();
        filters.insert(b"StdCF".to_vec(), std::sync::Arc::new(Aes256CryptFilter));
        let state = EncryptionState::try_from(EncryptionVersion::V4 {
            document: &doc.inner,
            encrypt_metadata: true,
            crypt_filters: filters,
            stream_filter: b"StdCF".to_vec(),
            string_filter: b"StdCF".to_vec(),
            owner_password: "o",
            user_password: "u",
            permissions: Default::default(),
        })
        .unwrap();
        let p = Protection::opened(state, doc.inner.trailer.get(b"ID").ok().cloned(), None, None);
        doc.inherit_protection(&p);
        doc.add_note(0, 20.0, 180.0, "kept", "Ann", [1.0, 1.0, 0.0])
            .unwrap();
        let dir = std::env::temp_dir().join(format!("folitecta-noprotect-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let target = dir.join("existing.pdf");
        std::fs::write(&target, b"old bytes").unwrap();
        assert!(matches!(doc.save_as(&target), Err(Error::ProtectionNotKept(_))));
        assert_eq!(std::fs::read(&target).unwrap(), b"old bytes");
        assert!(doc.is_dirty());
        let copy = dir.join("copy.pdf");
        assert!(matches!(doc.save_copy(&copy), Err(Error::ProtectionNotKept(_))));
        assert!(!copy.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
