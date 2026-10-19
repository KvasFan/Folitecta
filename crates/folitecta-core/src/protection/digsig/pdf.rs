//! Signing PDF documents, time-stamping them, validating their signatures,
//! incremental updates.

use der::Encode;
use lopdf::{Dictionary, Object, ObjectId, Stream, StringFormat, dictionary};
use x509_cert::Certificate;

use std::path::PathBuf;
use std::sync::Arc;

use std::collections::HashMap;

use super::cms_sig::{issued_by, sign_detached_stamped, verify_detached};
use super::dss::{MaterialPlan, RevisionKind, SourceAnalysis};
use super::id::{CertInfo, DigitalId, cert_info};
use super::revocation::{
    CertRevocation, ChainCheck, EvidenceStore, FetchFailure, FetchProblem, FetchSummary, Finding,
    RevocationArchive, RevocationKind, RevocationNote, RevocationPool, RevocationRequest, RevocationSource,
    RevocationState, RevocationTransport, TimeBasis, TimeFrame, check_chain,
};
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
    /// Revocation data written into the signature (its signed revocation
    /// information attribute, ISO 32000-1 12.8.3.3.2); none by default.
    pub revocation_archive: RevocationArchive,
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
            revocation_archive: RevocationArchive::default(),
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
    /// stamp or only adds validation material (what else the time stamp
    /// revisions change is not analysed), and one at least is a time stamp.
    pub later_revisions_time_stamped: bool,
    /// There are later revisions and each only adds validation material (or
    /// changes nothing the document shows): what was signed is unchanged.
    pub later_revisions_material: bool,
    /// A document time stamp (`ETSI.RFC3161`): the time of a revision, proven
    /// by an authority, without a signer.
    pub document_time_stamp: bool,
    /// The time stamp of the signature, or the document time stamp itself.
    pub time_stamp: Option<TimeStampStatus>,
    /// The separate checks of the signature.
    pub checks: SignatureChecks,
    /// The revocation of each certificate checked (the signer and its
    /// issuers up to a trusted certificate; for a document time stamp its
    /// authority and issuers).
    pub revocation: Vec<CertRevocation>,
    /// The time the revocation was checked for.
    pub revocation_at: Option<i64>,
    pub revocation_basis: TimeBasis,
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
    /// Revocation of the certificates of the chain at the signing time, from
    /// OCSP answers and revocation lists of the signature, its time stamp,
    /// the document security store or fetched online (not applicable to
    /// self-signed and directly trusted certificates without an issuer).
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
    /// The revocation of the certificates of the authority of a signature's
    /// time stamp, at the time of the stamp.
    pub revocation: Vec<CertRevocation>,
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
pub(crate) fn pdf_date(secs: u64) -> String {
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
        // Candidates named as the issuer, a few at most (a store may carry
        // many certificates).
        let found = certificates
            .iter()
            .filter(|c| c.tbs_certificate.subject == cur.tbs_certificate.issuer)
            .take(16)
            .find(|c| issued_by(&cur, c));
        match found {
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

fn time_stamp_status(
    check: &TimeStampCheck,
    trusted: &[Certificate],
    dss_certs: &[Certificate],
) -> TimeStampStatus {
    let (authority, authority_cert, is_trusted) = match &check.authority {
        Some(cert) => {
            let info = cert_info(cert);
            let (_, ok) = chain_of(cert, &with(&check.certificates, dss_certs), trusted);
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
        revocation: Vec::new(),
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
/// valid at the (proven or claimed) signing time, `revocation` the check of
/// its chain (the integrity of signatures followed by later revisions is
/// settled once those are analysed).
fn checks_of(s: &SignatureStatus, value_ok: bool, cert_in_time: bool, revocation: Check) -> SignatureChecks {
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
        revocation,
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
        let archive = &opts.revocation_archive;
        let reserve = (cert_sizes + 4096).max(8192)
            + if tsa.is_some() { TOKEN_RESERVE } else { 0 }
            + if archive.is_empty() {
                0
            } else {
                archive.byte_len() + 64
            };
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
            sign_detached_stamped(id, parts, time, archive, tsa)
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
        let analysis = Arc::new(SourceAnalysis::of(&self.inner));
        let doc = Document::signed_state(
            self,
            protection,
            Source {
                bytes,
                signed: true,
                decrypted: Some(Arc::new(next)),
                analysis,
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

    /// What the validation of the signatures reads from the document, taken
    /// quickly: the checks themselves can then run on another thread.
    pub fn validation_job(&self) -> ValidationJob {
        let doc = &self.inner;
        let pages = self.page_ids();
        let sigs = self
            .signed_fields()
            .into_iter()
            .map(|(field_id, name, sig)| {
                let text = |k: &[u8]| dict_text(doc, &sig, k).unwrap_or_default();
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
                SigInput {
                    page: self
                        .field_widgets(field_id)
                        .first()
                        .and_then(|w| doc.get_dictionary(*w).ok())
                        .and_then(|w| w.get(b"P").ok())
                        .and_then(|p| p.as_reference().ok())
                        .and_then(|p| pages.iter().position(|x| *x == p)),
                    holder: self.signature_holder(field_id),
                    signer: text(b"Name"),
                    m: text(b"M"),
                    reason: text(b"Reason"),
                    location: text(b"Location"),
                    contact: text(b"ContactInfo"),
                    sub_filter: sig
                        .get(b"SubFilter")
                        .ok()
                        .and_then(|s| s.as_name().ok())
                        .map(|s| String::from_utf8_lossy(s).to_string())
                        .unwrap_or_default(),
                    certify,
                    ranges: sig
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
                        .unwrap_or_default(),
                    contents: match sig.get(b"Contents").ok().map(|c| deref(doc, c)) {
                        Some(Object::String(b, _)) => b.clone(),
                        _ => Vec::new(),
                    },
                    field: name,
                }
            })
            .collect();
        let (data, analysis) = match &self.source {
            Some(s) => (s.bytes.clone(), s.analysis.clone()),
            None => (self.bytes(), Arc::new(SourceAnalysis::of(&self.inner))),
        };
        let protection = match (&self.source, self.protection()) {
            (
                Some(Source {
                    decrypted: Some(_), ..
                }),
                Some(p),
            ) => Some(p.clone()),
            _ => None,
        };
        ValidationJob {
            data,
            protection,
            sigs,
            analysis,
            stamp: self.validation_stamp(),
        }
    }

    /// Validate every signature against the file as it was opened or saved,
    /// now, with the revocation data the document carries.
    pub fn validate_signatures(&self, trusted: &[Certificate]) -> Vec<SignatureStatus> {
        self.validation_job()
            .run(trusted, &RevocationPool::default(), now_unix() as i64)
    }

    /// Validate every signature at `now` with the revocation data the
    /// document carries and those of `pool` (fetched online).
    pub fn validate_signatures_with(
        &self,
        trusted: &[Certificate],
        pool: &RevocationPool,
        now: i64,
    ) -> Vec<SignatureStatus> {
        self.validation_job().run(trusted, pool, now)
    }

    /// The requests an online check would send now (see
    /// [`ValidationJob::revocation_requests`]).
    pub fn revocation_requests(
        &self,
        trusted: &[Certificate],
        pool: &RevocationPool,
        now: i64,
    ) -> Vec<RevocationRequest> {
        self.validation_job().revocation_requests(trusted, pool, now)
    }

    /// Ask online for the revocation data the signatures miss (see
    /// [`ValidationJob::fetch_revocation`]).
    pub fn fetch_revocation(
        &self,
        trusted: &[Certificate],
        pool: &mut RevocationPool,
        now: i64,
        send: &mut RevocationTransport<'_>,
    ) -> FetchSummary {
        self.validation_job().fetch_revocation(trusted, pool, now, send)
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

    /// Remove every signature (the fields stay, empty), the certification
    /// and the document security store (one undo step). Returns how many
    /// signatures were removed.
    pub fn clear_signatures(&mut self) -> Result<usize> {
        let signed = self.signed_fields();
        if signed.is_empty() {
            return Ok(0);
        }
        self.begin_edit();
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
        }
        // The validation material of the signatures goes with them: the
        // certificates and revocation data name the signers. So do the
        // signature dictionaries no field holds any more.
        self.remove_dss();
        self.inner.prune_objects();
        // Without signatures the document is written whole (the base of
        // updates stays, for an undo that brings them back).
        self.commit()?;
        Ok(signed.len())
    }
}

/// A signature field to validate, as read from the document.
#[derive(Debug, Clone)]
struct SigInput {
    field: String,
    page: Option<usize>,
    /// The object of the signature dictionary.
    holder: ObjectId,
    signer: String,
    m: String,
    reason: String,
    location: String,
    contact: String,
    sub_filter: String,
    certify: Option<u8>,
    ranges: Vec<usize>,
    /// `/Contents` as the document model has it.
    contents: Vec<u8>,
}

/// What the validation of the signatures of a document needs, taken from it
/// quickly ([`Document::validation_job`]): the file the signatures are
/// checked against, the signature dictionaries and the document security
/// store of that file. Running it takes the time (signatures, revocation
/// data, later revisions); it can run on another thread.
#[derive(Clone)]
pub struct ValidationJob {
    data: Arc<Vec<u8>>,
    /// The protection of an encrypted file (its key reads the values and
    /// the revisions).
    protection: Option<crate::security::Protection>,
    sigs: Vec<SigInput>,
    analysis: Arc<SourceAnalysis>,
    stamp: (u64, u64),
}

/// A signature checked before its revocation and later revisions are.
struct Checked {
    status: SignatureStatus,
    value_ok: bool,
    cert_in_time: bool,
    /// The value of `/Contents` (the key of its store entry).
    contents: Vec<u8>,
    /// The signer, or the authority of a document time stamp.
    signer: Option<Certificate>,
    /// Certificates carried by the signature (and its time stamp).
    certs: Vec<Certificate>,
    archive: RevocationArchive,
    /// The time stamp of a signature, or the token of a document time stamp.
    stamp: Option<TimeStampCheck>,
    claimed: Option<i64>,
}

/// The revocation checks of one validated signature, for the validation
/// material and the online requests.
pub(crate) struct SignatureChain {
    pub(crate) contents: Vec<u8>,
    /// Certificates the signature (or its time stamp) carries itself.
    pub(crate) own_certs: Vec<Certificate>,
    pub(crate) rev: ChainCheck,
    pub(crate) stamp_rev: Option<ChainCheck>,
}

/// The text of the problem of a signature followed by other revisions.
fn later_problem(s: &SignatureStatus) -> String {
    match s.certify {
        Some(1) => {
            "the document was changed after certification, which the certification does not allow".into()
        }
        _ if s.document_time_stamp => {
            "the document was changed after the time stamp (in a later revision)".into()
        }
        _ => "the document was changed after it was signed (in a later revision)".into(),
    }
}

/// At most this many requests are sent in one online check.
const MAX_REQUESTS: usize = 16;
/// An online check stops sending requests after this time.
const RUN_LIMIT: std::time::Duration = std::time::Duration::from_secs(180);

impl ValidationJob {
    /// The state of the document the job was taken from (its revision and
    /// how many times it was saved).
    pub fn stamp(&self) -> (u64, u64) {
        self.stamp
    }

    /// The document has signatures.
    pub fn has_signatures(&self) -> bool {
        !self.sigs.is_empty()
    }

    /// Validate every signature at `now`, with the revocation data the file
    /// carries and those of `pool`.
    pub fn run(&self, trusted: &[Certificate], pool: &RevocationPool, now: i64) -> Vec<SignatureStatus> {
        self.run_with_chains(trusted, pool, now)
            .into_iter()
            .map(|(s, _)| s)
            .collect()
    }

    /// Check the byte range, value, signer and time stamp of one signature.
    fn check_one(&self, input: &SigInput, trusted: &[Certificate], dss_certs: &[Certificate]) -> Checked {
        let data = &self.data;
        let state = self.protection.as_ref().map(|p| p.state());
        let status = SignatureStatus {
            field: input.field.clone(),
            page: input.page,
            signer: input.signer.clone(),
            time: util::pdf_date_display(&input.m),
            reason: input.reason.clone(),
            location: input.location.clone(),
            contact: input.contact.clone(),
            sub_filter: input.sub_filter.clone(),
            digest_algorithm: String::new(),
            valid: false,
            covers_document: false,
            certify: input.certify,
            trusted: false,
            cert: None,
            chain: Vec::new(),
            problems: Vec::new(),
            revision_len: 0,
            later_revisions_time_stamped: false,
            later_revisions_material: false,
            document_time_stamp: false,
            time_stamp: None,
            checks: SignatureChecks::default(),
            revocation: Vec::new(),
            revocation_at: None,
            revocation_basis: TimeBasis::Now,
        };
        let mut out = Checked {
            status,
            value_ok: false,
            cert_in_time: true,
            contents: Vec::new(),
            signer: None,
            certs: Vec::new(),
            archive: RevocationArchive::default(),
            stamp: None,
            claimed: util::pdf_date_unix(&input.m),
        };
        let status = &mut out.status;
        let ranges = &input.ranges;
        if ranges.len() != 4 || input.contents.is_empty() {
            status
                .problems
                .push("the signature has no byte range or contents".into());
            out.cert_in_time = false;
            return out;
        }
        let (a, b, c, d) = (ranges[0], ranges[1], ranges[2], ranges[3]);
        if a + b > data.len() || c + d > data.len() || a + b > c {
            status
                .problems
                .push("the byte range does not fit the file".into());
            out.cert_in_time = false;
            return out;
        }
        // The value is the one the byte range leaves out, and that gap is
        // exactly the /Contents string of this signature: a gap moved or
        // widened (to hide other bytes) makes the signature invalid.
        let contents = encrypted::bound_value(data, (a + b, c), input.holder)
            .map(|raw| encrypted::readable_value(state, input.holder, &raw))
            .filter(|v| *v == input.contents);
        let Some(contents) = contents else {
            status
                .problems
                .push("the bytes the byte range leaves out are not the value of this signature".into());
            out.cert_in_time = false;
            return out;
        };
        status.revision_len = c + d;
        status.covers_document = c + d == data.len();
        let parts = [&data[a..a + b], &data[c..c + d]];
        if status.sub_filter == "ETSI.RFC3161" {
            // A document time stamp: the token itself is the signature.
            status.document_time_stamp = true;
            match timestamp::check_token(&contents, &parts) {
                Ok(check) => {
                    let ts = time_stamp_status(&check, trusted, dss_certs);
                    status.valid = check.valid();
                    out.value_ok = check.signature_ok;
                    status.digest_algorithm = check.digest_algorithm.clone();
                    status.problems.extend(check.problems.iter().cloned());
                    status.signer = ts.authority.clone();
                    status.time = ts.time.clone();
                    status.trusted = ts.trusted;
                    status.cert = ts.authority_cert.clone();
                    let available = with(&check.certificates, dss_certs);
                    if let Some(cert) = &check.authority {
                        let (chain, _) = chain_of(cert, &available, trusted);
                        status.chain = chain.iter().map(cert_info).collect();
                    }
                    status.time_stamp = Some(ts);
                    out.signer = check.authority.clone();
                    out.certs = check.certificates.clone();
                    out.claimed = Some(check.time);
                    out.stamp = Some(check);
                }
                Err(e) => status.problems.push(e.to_string()),
            }
        } else {
            match verify_detached(&contents, &parts) {
                Ok(check) => {
                    status.valid = check.digest_ok && check.signature_ok;
                    out.value_ok = check.signature_ok;
                    status.digest_algorithm = check.digest_algorithm.clone();
                    status.problems.extend(check.problems.iter().cloned());
                    if let Some(t) = check.signing_time {
                        status.time = display_date(t.max(0) as u64);
                    }
                    if let Some(tc) = &check.time_stamp {
                        let ts = time_stamp_status(tc, trusted, dss_certs);
                        for p in &ts.problems {
                            status.problems.push(format!("time stamp: {p}"));
                        }
                        status.time_stamp = Some(ts);
                    }
                    let available = with(&check.certificates, dss_certs);
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
                                out.cert_in_time = false;
                                status
                                    .problems
                                    .push("the certificate was not valid at the signing time".into());
                            }
                        }
                        // Issuer chain from the certificates in the signature and the store.
                        let (chain, ok) = chain_of(cert, &available, trusted);
                        status.trusted = ok;
                        status.chain = chain.iter().map(cert_info).collect();
                        status.cert = Some(ci);
                    }
                    out.signer = check.signer.clone();
                    out.certs = check.certificates.clone();
                    if let Some(tc) = &check.time_stamp {
                        for c in &tc.certificates {
                            if !out.certs.contains(c) {
                                out.certs.push(c.clone());
                            }
                        }
                    }
                    out.archive = check.revocation.clone();
                    out.claimed = check.signing_time.or(out.claimed);
                    out.stamp = check.time_stamp;
                }
                Err(e) => status.problems.push(e.to_string()),
            }
        }
        out.contents = contents;
        out
    }

    /// Validate every signature, with the revocation checks of each.
    pub(crate) fn run_with_chains(
        &self,
        trusted: &[Certificate],
        pool: &RevocationPool,
        now: i64,
    ) -> Vec<(SignatureStatus, SignatureChain)> {
        let saved = &self.analysis.dss;
        let dss_certs = saved.certificates();
        let dss_archive = saved.dss().archive();
        let checked: Vec<Checked> = self
            .sigs
            .iter()
            .map(|s| self.check_one(s, trusted, dss_certs))
            .collect();
        // Document time stamps that prove a revision existed: valid and of a
        // trusted authority (revision length, time).
        let proofs: Vec<(usize, i64)> = checked
            .iter()
            .filter(|c| c.status.document_time_stamp && c.status.valid)
            .filter_map(|c| {
                c.status
                    .time_stamp
                    .as_ref()
                    .filter(|ts| ts.valid && ts.trusted)
                    .map(|ts| (c.status.revision_len, ts.time_unix))
            })
            .collect();
        let proven_after = |len: usize| proofs.iter().filter(|(l, _)| *l > len).map(|(_, t)| *t).min();
        let mut store = EvidenceStore::default();
        let mut out = Vec::new();
        for mut c in checked {
            let len = c.status.revision_len;
            let exist_of = move |at: i64| proven_after(len).unwrap_or(now).max(at);
            let mut stamp_rev = None;
            let (rev, frame) = match &c.signer {
                None => (ChainCheck::default(), None),
                Some(signer) if c.status.document_time_stamp => {
                    let token = c.stamp.as_ref();
                    let at = token.map_or(now, |t| t.time);
                    let frame = TimeFrame {
                        at,
                        basis: TimeBasis::Claimed,
                        exist: exist_of(at),
                        now,
                    };
                    let token_data = token.map(|t| t.revocation.clone()).unwrap_or_default();
                    let evidence = store.gather(&[
                        (RevocationSource::TimeStamp, &token_data),
                        (RevocationSource::Dss, &dss_archive),
                        (RevocationSource::Fetched, &pool.data),
                    ]);
                    let available = with(&c.certs, dss_certs);
                    let rev = check_chain(
                        signer,
                        &available,
                        trusted,
                        &store,
                        &evidence,
                        &pool.failures,
                        frame,
                    );
                    (rev, Some(frame))
                }
                Some(signer) => {
                    // The time stamp's authority, at the time of the stamp: a
                    // revocation of it then (or of its key) takes the proof away.
                    let stamp = c.stamp.as_ref().filter(|t| t.valid() && t.usage_ok);
                    let ts_status = c.status.time_stamp.clone();
                    let mut proven = false;
                    if let (Some(tc), Some(authority)) = (stamp, stamp.and_then(|t| t.authority.as_ref())) {
                        let frame = TimeFrame {
                            at: tc.time,
                            basis: TimeBasis::Claimed,
                            exist: exist_of(tc.time),
                            now,
                        };
                        let evidence = store.gather(&[
                            (RevocationSource::TimeStamp, &tc.revocation),
                            (RevocationSource::Dss, &dss_archive),
                            (RevocationSource::Fetched, &pool.data),
                        ]);
                        let available = with(&tc.certificates, dss_certs);
                        let ts_rev = check_chain(
                            authority,
                            &available,
                            trusted,
                            &store,
                            &evidence,
                            &pool.failures,
                            frame,
                        );
                        let compromised = ts_rev.entries.iter().any(|e| {
                            matches!(e.state, RevocationState::Revoked { time, reason }
                                if time <= tc.time || matches!(reason, Some(1) | Some(2)))
                        });
                        if compromised {
                            c.status.problems.push(
                                "time stamp: the certificate of the authority was revoked by the time of the stamp".into(),
                            );
                        }
                        proven = ts_status.as_ref().is_some_and(|t| t.valid && t.trusted) && !compromised;
                        if let Some(ts) = c.status.time_stamp.as_mut() {
                            ts.revocation = ts_rev.entries.clone();
                        }
                        stamp_rev = Some(ts_rev);
                    }
                    let (at, basis) = match (proven, stamp, c.claimed) {
                        (true, Some(tc), _) => (tc.time, TimeBasis::TimeStamp),
                        (_, _, Some(t)) => (t, TimeBasis::Claimed),
                        _ => (now, TimeBasis::Now),
                    };
                    let frame = TimeFrame {
                        at,
                        basis,
                        exist: if proven { at } else { exist_of(at) },
                        now,
                    };
                    let stamp_data = c.stamp.as_ref().map(|t| t.revocation.clone()).unwrap_or_default();
                    let evidence = store.gather(&[
                        (RevocationSource::Signature, &c.archive),
                        (RevocationSource::TimeStamp, &stamp_data),
                        (RevocationSource::Dss, &dss_archive),
                        (RevocationSource::Fetched, &pool.data),
                    ]);
                    let available = with(&c.certs, dss_certs);
                    let rev = check_chain(
                        signer,
                        &available,
                        trusted,
                        &store,
                        &evidence,
                        &pool.failures,
                        frame,
                    );
                    (rev, Some(frame))
                }
            };
            c.status.revocation = rev.entries.clone();
            if let Some(f) = frame {
                c.status.revocation_at = Some(f.at);
                c.status.revocation_basis = f.basis;
            }
            c.status.checks = checks_of(&c.status, c.value_ok, c.cert_in_time, rev.check);
            out.push((
                c.status,
                SignatureChain {
                    contents: c.contents,
                    own_certs: c.certs,
                    rev,
                    stamp_rev,
                },
            ));
        }
        self.later_revisions(&mut out);
        out
    }

    /// What the revisions after each signature do: signatures followed only
    /// by validation material keep their integrity; followed by document
    /// time stamps (and material), they are reported as such.
    fn later_revisions(&self, out: &mut [(SignatureStatus, SignatureChain)]) {
        let data = &self.data;
        let stamped: Vec<usize> = out
            .iter()
            .filter(|(s, _)| s.document_time_stamp && s.valid)
            .map(|(s, _)| content_end(data, s.revision_len))
            .collect();
        let ends = revision_ends(data);
        // Bytes after the last end of a revision belong to no revision.
        let trailing = ends.last().is_none_or(|e| *e < content_end(data, data.len()));
        for (s, _) in out.iter_mut() {
            if s.covers_document || s.revision_len == 0 {
                continue;
            }
            let own = content_end(data, s.revision_len);
            let later: Vec<usize> = ends.iter().copied().filter(|e| *e > own).collect();
            let mut kinds = if later.is_empty() {
                Vec::new()
            } else {
                self.analysis
                    .kinds(data, own, &later, &stamped, self.protection.as_ref())
            };
            if trailing || kinds.is_empty() {
                kinds.push(RevisionKind::Other);
            }
            let material = |k: &RevisionKind| matches!(k, RevisionKind::Material | RevisionKind::Neutral);
            s.later_revisions_material = kinds.iter().all(material);
            s.later_revisions_time_stamped =
                kinds.iter().all(|k| material(k) || *k == RevisionKind::TimeStamp)
                    && kinds.contains(&RevisionKind::TimeStamp);
            if s.later_revisions_material {
                // Validation material does not change what was signed (EN 319
                // 142-1 5.4.2.3): no problem, certification included.
            } else if s.later_revisions_time_stamped && s.certify != Some(1) {
                s.problems.push(
                    "later revisions add document time stamps or validation material (the other changes of time stamp revisions are not analysed)"
                        .into(),
                );
            } else {
                let problem = later_problem(s);
                s.problems.push(problem);
            }
            s.checks.integrity = if !s.valid {
                Check::Failed
            } else if s.later_revisions_material {
                Check::Passed
            } else {
                Check::Warning
            };
        }
    }

    /// The certificates whose revocation is not determined and that can be
    /// asked about: of chains that end at a trusted certificate, with their
    /// issuer at hand; each with its requests (OCSP first, then the lists).
    fn targets(&self, trusted: &[Certificate], pool: &RevocationPool, now: i64) -> Vec<Target> {
        let mut out: Vec<Target> = Vec::new();
        for (_, chain) in self.run_with_chains(trusted, pool, now) {
            for rc in std::iter::once(&chain.rev).chain(chain.stamp_rev.iter()) {
                if !rc.trusted {
                    continue;
                }
                for (entry, (cert, issuer)) in rc.entries.iter().zip(&rc.subjects) {
                    let Some(issuer) = issuer else { continue };
                    if entry.state != RevocationState::Unknown
                        || out.iter().any(|t| t.sha == entry.cert_sha256)
                    {
                        continue;
                    }
                    let mut requests = Vec::new();
                    if let Ok(body) = super::ocsp::request(cert, issuer) {
                        for url in super::revocation::ocsp_urls(cert) {
                            requests.push(RevocationRequest {
                                cert: entry.cert_sha256.clone(),
                                subject: entry.subject.clone(),
                                url,
                                ocsp: Some(body.clone()),
                            });
                        }
                    }
                    for url in super::revocation::crl_urls(cert) {
                        requests.push(RevocationRequest {
                            cert: entry.cert_sha256.clone(),
                            subject: entry.subject.clone(),
                            url,
                            ocsp: None,
                        });
                    }
                    if requests.is_empty() {
                        continue;
                    }
                    out.push(Target {
                        sha: entry.cert_sha256.clone(),
                        cert: cert.clone(),
                        issuer: issuer.clone(),
                        available: rc.certs.clone(),
                        requests,
                    });
                }
            }
        }
        out
    }

    /// The requests an online check would send now, in order (at most 16):
    /// only for certificates of chains that end at a trusted certificate and
    /// whose revocation is not determined.
    pub fn revocation_requests(
        &self,
        trusted: &[Certificate],
        pool: &RevocationPool,
        now: i64,
    ) -> Vec<RevocationRequest> {
        self.targets(trusted, pool, now)
            .into_iter()
            .flat_map(|t| t.requests)
            .take(MAX_REQUESTS)
            .collect()
    }

    /// Ask online for the revocation data the signatures miss: for each
    /// certificate, its OCSP responders first, then its revocation lists,
    /// until one answer decides. Answers are checked before they are kept in
    /// `pool`; failures are kept for the certificates still undetermined.
    /// At most 16 requests, and none after three minutes.
    pub fn fetch_revocation(
        &self,
        trusted: &[Certificate],
        pool: &mut RevocationPool,
        now: i64,
        send: &mut RevocationTransport<'_>,
    ) -> FetchSummary {
        let started = std::time::Instant::now();
        let mut summary = FetchSummary::default();
        let mut lists: HashMap<String, std::result::Result<Vec<u8>, String>> = HashMap::new();
        for t in self.targets(trusted, pool, now) {
            summary.certificates += 1;
            let mut failures = Vec::new();
            let mut decided = false;
            for req in &t.requests {
                if decided {
                    break;
                }
                let fail = |problem| FetchFailure {
                    cert: t.sha.clone(),
                    url: req.url.clone(),
                    problem,
                };
                let cached = req.ocsp.is_none().then(|| lists.get(&req.url).cloned()).flatten();
                let body = match cached {
                    Some(b) => b,
                    None if summary.requests >= MAX_REQUESTS || started.elapsed() > RUN_LIMIT => {
                        failures.push(fail(FetchProblem::NotAsked));
                        continue;
                    }
                    None => {
                        summary.requests += 1;
                        let b = send(req).map_err(|e| e.to_string());
                        if req.ocsp.is_none() {
                            lists.insert(req.url.clone(), b.clone());
                        }
                        b
                    }
                };
                let body = match body {
                    Ok(b) => b,
                    Err(e) => {
                        failures.push(fail(FetchProblem::Transport(e)));
                        continue;
                    }
                };
                let (finding, kept) = if req.ocsp.is_some() {
                    match super::ocsp::parse(&body) {
                        Ok(p) => (
                            p.finding(&t.cert, &t.issuer, &t.available, now),
                            p.response.clone(),
                        ),
                        Err(n) => (Finding::Unusable(n), Vec::new()),
                    }
                } else {
                    match super::crl::parse(&body) {
                        Ok(p) => (p.finding(&t.cert, &t.issuer, now), p.der.clone()),
                        Err(n) => (Finding::Unusable(n), Vec::new()),
                    }
                };
                match finding {
                    Finding::Good { .. } | Finding::Revoked { .. } => {
                        if req.ocsp.is_some() {
                            pool.data.push_ocsp(kept);
                        } else {
                            pool.data.push_crl(kept);
                        }
                        decided = true;
                    }
                    Finding::NotAbout => failures.push(fail(FetchProblem::AnotherCertificate)),
                    Finding::Unusable(n) => failures.push(fail(FetchProblem::Answer(Box::new(n)))),
                }
            }
            summary.failures += failures.len();
            pool.failures.retain(|f| f.cert != t.sha);
            if decided {
                summary.decided += 1;
            } else {
                pool.failures.extend(failures);
            }
        }
        summary
    }

    /// The validation material the signatures need: for each signature
    /// whose integrity, signer and revocation did not fail and whose chain
    /// ends at a trusted certificate, the revocation data that decided and
    /// the certificates of its chain (those it carries itself left out),
    /// and an entry of its own when its validation succeeded.
    pub fn plan_material(
        &self,
        trusted: &[Certificate],
        pool: &RevocationPool,
        now: i64,
    ) -> Result<MaterialPlan> {
        if self.sigs.is_empty() {
            return Err(Error::Invalid("the document has no signatures".into()));
        }
        let mut plan = MaterialPlan {
            stamp: self.stamp,
            ..MaterialPlan::default()
        };
        for (s, chain) in self.run_with_chains(trusted, pool, now) {
            let c = &s.checks;
            if !s.valid
                || c.integrity == Check::Failed
                || c.identity != Check::Passed
                || c.revocation == Check::Failed
            {
                plan.skipped.push(s.field.clone());
                continue;
            }
            plan.signatures += 1;
            let (mut certs, mut ocsps, mut crls) = (Vec::new(), Vec::new(), Vec::new());
            let mut inside: Vec<Certificate> = chain.own_certs.clone();
            for rc in std::iter::once(&chain.rev).chain(chain.stamp_rev.iter()) {
                for (kind, der, _) in &rc.used {
                    match kind {
                        RevocationKind::Ocsp => {
                            if let Ok(p) = super::ocsp::parse(der) {
                                inside.extend(p.certs);
                            }
                            ocsps.push(MaterialPlan::push(&mut plan.ocsps, der));
                        }
                        RevocationKind::Crl => crls.push(MaterialPlan::push(&mut plan.crls, der)),
                    }
                }
            }
            for rc in std::iter::once(&chain.rev).chain(chain.stamp_rev.iter()) {
                for cert in &rc.certs {
                    if inside.contains(cert) {
                        continue;
                    }
                    if let Ok(der) = cert.to_der() {
                        let i = MaterialPlan::push(&mut plan.certs, &der);
                        if !certs.contains(&i) {
                            certs.push(i);
                        }
                    }
                }
            }
            for e in chain
                .rev
                .entries
                .iter()
                .filter(|e| e.state == RevocationState::Unknown)
            {
                plan.missing.push((
                    e.subject.clone(),
                    e.notes.first().cloned().unwrap_or(RevocationNote::NoData),
                ));
            }
            let succeeded = matches!(c.revocation, Check::Passed | Check::NotChecked);
            if !succeeded {
                plan.without_entry += 1;
            } else if !(certs.is_empty() && ocsps.is_empty() && crls.is_empty()) {
                plan.vri
                    .push((super::dss::vri_key(&chain.contents), certs, ocsps, crls));
            }
        }
        Ok(plan)
    }
}

/// A certificate an online check asks about.
struct Target {
    sha: String,
    cert: Certificate,
    issuer: Certificate,
    available: Vec<Certificate>,
    requests: Vec<RevocationRequest>,
}

/// `a` followed by the certificates of `b` it does not have.
fn with(a: &[Certificate], b: &[Certificate]) -> Vec<Certificate> {
    let mut out = a.to_vec();
    for c in b {
        if !out.contains(c) {
            out.push(c.clone());
        }
    }
    out
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
