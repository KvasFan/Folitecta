//! Signing PDF documents, validating their signatures, incremental updates.

use der::Encode;
use lopdf::{Dictionary, Object, ObjectId, Stream, StringFormat, dictionary};
use x509_cert::Certificate;

use super::cms_sig::{issued_by, sign_detached, verify_detached};
use super::id::{CertInfo, DigitalId, cert_info};
use crate::document::Document;
use crate::error::{Error, Result};
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
}

const PLACEHOLDER_RANGE: &[u8] = b"[0 1000000000 1000000000 1000000000]";

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
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

    /// The current state as an incremental update of the opened file, when the
    /// file carries signatures (saving it whole would break them).
    pub(crate) fn incremental_bytes(&self) -> Result<Option<Vec<u8>>> {
        let Some(src) = self.source.clone() else {
            return Ok(None);
        };
        let prev = lopdf::Document::load_mem(&src)?;
        let prev_signed = Document::from_bytes(src.to_vec(), None, None)
            .map(|d| d.has_signatures())
            .unwrap_or(false);
        if !prev_signed {
            return Ok(None);
        }
        let mut inc = lopdf::IncrementalDocument::create_from(src.to_vec(), prev);
        let changed: Vec<(ObjectId, Object)> = self
            .inner
            .objects
            .iter()
            .filter(|(id, o)| {
                inc.get_prev_documents()
                    .get_object(**id)
                    .map(|p| p != *o)
                    .unwrap_or(true)
            })
            .map(|(id, o)| (*id, o.clone()))
            .collect();
        if changed.is_empty() {
            return Ok(Some(src.to_vec()));
        }
        for (id, o) in changed {
            inc.new_document.objects.insert(id, o);
        }
        inc.new_document.max_id = inc.new_document.max_id.max(self.inner.max_id);
        for key in [&b"Root"[..], b"Info"] {
            if let Ok(v) = self.inner.trailer.get(key) {
                inc.new_document.trailer.set(key, v.clone());
            }
        }
        let mut out = Vec::new();
        inc.save_to(&mut out)?;
        Ok(Some(out))
    }

    /// Sign the document with `id`. Returns the bytes of the signed file (the
    /// document itself is not changed; open the result to continue).
    pub fn sign(&self, id: &DigitalId, opts: &SignOptions) -> Result<Vec<u8>> {
        let time = if opts.time == 0 { now_unix() } else { opts.time };
        let info = id.info();
        let cert_sizes: usize = std::iter::once(&id.certificate)
            .chain(id.chain.iter())
            .map(|c| c.to_der().map(|d| d.len()).unwrap_or(2000))
            .sum();
        let reserve = (cert_sizes + 4096).max(8192);
        // Work on a copy of the current state.
        let mut work =
            Document::from_bytes(self.bytes().to_vec(), self.path().map(|p| p.to_path_buf()), None)?;
        work.source = self.source.clone();
        let mut sig = dictionary! {
            "Type" => "Sig",
            "Filter" => "Adobe.PPKLite",
            "SubFilter" => "adbe.pkcs7.detached",
            "ByteRange" => vec![0.into(), 1_000_000_000i64.into(), 1_000_000_000i64.into(), 1_000_000_000i64.into()],
            "Contents" => Object::String(vec![0u8; reserve], StringFormat::Hexadecimal),
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
        let sig_id = work.inner.add_object(sig);
        if opts.certify > 0 {
            work.inner
                .catalog_mut()?
                .set("Perms", dictionary! { "DocMDP" => sig_id });
        }
        work.attach_signature(sig_id, &info, opts, time)?;
        work.commit()?;
        let mut data = match work.incremental_bytes()? {
            Some(b) => b,
            None => work.bytes().to_vec(),
        };
        // Byte range: everything except the /Contents value.
        let zeros = vec![b'0'; reserve * 2];
        let mut needle = vec![b'<'];
        needle.extend(&zeros);
        needle.push(b'>');
        let start =
            find(&data, &needle).ok_or_else(|| Error::Invalid("signature placeholder not found".into()))?;
        let end = start + needle.len();
        let range_pos = find(&data, PLACEHOLDER_RANGE)
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
        let cms = sign_detached(id, &[&data[..start], &data[end..]], time)?;
        let hex = hex_upper(&cms);
        if hex.len() > zeros.len() {
            return Err(Error::Invalid(
                "the signature does not fit the reserved space".into(),
            ));
        }
        data[start + 1..start + 1 + hex.len()].copy_from_slice(&hex);
        Ok(data)
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
                let n = [
                    rect[0].min(rect[2]),
                    rect[1].min(rect[3]),
                    rect[0].max(rect[2]),
                    rect[1].max(rect[3]),
                ];
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
            SignPlacement::Invisible => {
                let page_id = self.page_id(0)?;
                let name = self.unique_field_name("Signature");
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
                (id, [0.0; 4])
            }
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

    /// Validate every signature against the file as it was opened or saved.
    pub fn validate_signatures(&self, trusted: &[Certificate]) -> Vec<SignatureStatus> {
        let data = self.source.clone().unwrap_or_else(|| self.bytes());
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
            let contents = match sig.get(b"Contents").ok().map(|c| deref(doc, c)) {
                Some(Object::String(b, _)) => b.clone(),
                _ => Vec::new(),
            };
            if ranges.len() != 4 || contents.is_empty() {
                status
                    .problems
                    .push("the signature has no byte range or contents".into());
                out.push(status);
                continue;
            }
            let (a, b, c, d) = (ranges[0], ranges[1], ranges[2], ranges[3]);
            if a + b > data.len() || c + d > data.len() || a + b > c {
                status
                    .problems
                    .push("the byte range does not fit the file".into());
                out.push(status);
                continue;
            }
            status.revision_len = c + d;
            status.covers_document = c + d == data.len();
            match verify_detached(&contents, &[&data[a..a + b], &data[c..c + d]]) {
                Ok(check) => {
                    status.valid = check.digest_ok && check.signature_ok;
                    status.digest_algorithm = check.digest_algorithm.clone();
                    status.problems.extend(check.problems.iter().cloned());
                    if let Some(t) = check.signing_time {
                        status.time = display_date(t.max(0) as u64);
                    }
                    if let Some(cert) = &check.signer {
                        let ci = cert_info(cert);
                        if status.signer.is_empty() {
                            status.signer = ci.common_name.clone();
                        }
                        // Certificate validity at signing time.
                        if let Some(t) = check.signing_time {
                            if t < ci.not_before_unix || t > ci.not_after_unix {
                                status
                                    .problems
                                    .push("the certificate was not valid at the signing time".into());
                            }
                        }
                        // Issuer chain from the certificates in the signature.
                        let mut chain: Vec<Certificate> = Vec::new();
                        let mut cur = cert.clone();
                        for _ in 0..8 {
                            if cur.tbs_certificate.subject == cur.tbs_certificate.issuer {
                                break;
                            }
                            match check.certificates.iter().find(|c| issued_by(&cur, c)) {
                                Some(issuer) => {
                                    chain.push(issuer.clone());
                                    cur = issuer.clone();
                                }
                                None => break,
                            }
                        }
                        let candidates: Vec<&Certificate> =
                            std::iter::once(cert).chain(chain.iter()).collect();
                        status.trusted = candidates.iter().any(|c| trusted.iter().any(|t| t == *c))
                            || trusted
                                .iter()
                                .any(|t| issued_by(candidates.last().copied().unwrap_or(cert), t));
                        status.chain = chain.iter().map(cert_info).collect();
                        status.cert = Some(ci);
                    }
                }
                Err(e) => status.problems.push(e.to_string()),
            }
            if !status.covers_document {
                status.problems.push(match status.certify {
                    Some(1) => {
                        "the document was changed after certification, which the certification does not allow"
                            .into()
                    }
                    _ => "the document was changed after it was signed (in a later revision)".into(),
                });
            }
            out.push(status);
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

    /// Bytes of the revision a signature covers (the document as it was signed).
    pub fn signed_revision(&self, field: &str) -> Result<Vec<u8>> {
        let data = self.source.clone().unwrap_or_else(|| self.bytes());
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
        // The file no longer needs incremental updates.
        self.source = None;
        self.commit()?;
        Ok(signed.len())
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
