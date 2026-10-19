//! Digital signatures: the store of digital IDs and trusted certificates,
//! signing (visible, invisible, certifying), validation and clearing.

use std::path::PathBuf;
use std::sync::Mutex;

use dioxus::prelude::*;
use folitecta_core::digsig::{
    CertInfo, Certificate, DigitalId, IdentityInfo, KeyKind, SignOptions, SignPlacement, SignatureStatus,
    SignedFile, Transport, cert_info, cert_to_der, parse_certificates,
};

use crate::commands::{handlers, no_doc, show_pane};
use crate::files;
use crate::i18n::{t, tf};
use crate::state::{AppState, ConfirmAction, Dialog, Pane, PasswordTarget, ToastKind, Tool, app_data_dir};

/// A digital ID kept in Folitecta's data folder.
#[derive(Debug, Clone, PartialEq)]
pub struct StoredId {
    /// The PKCS#12 file (protected by the ID's password).
    pub file: PathBuf,
    pub info: CertInfo,
}

fn ids_dir() -> Option<PathBuf> {
    app_data_dir().map(|d| d.join("ids"))
}

fn trusted_dir() -> Option<PathBuf> {
    app_data_dir().map(|d| d.join("trusted"))
}

/// File-name friendly text.
fn slug(s: &str) -> String {
    let s: String = s
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' { c } else { '_' })
        .collect();
    s.chars().take(40).collect()
}

/// Digital IDs of the store, sorted by name.
pub fn stored_ids() -> Vec<StoredId> {
    let Some(dir) = ids_dir() else { return Vec::new() };
    let mut out: Vec<StoredId> = std::fs::read_dir(&dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().map(|e| e == "cer").unwrap_or(false))
        .filter_map(|cer| {
            let der = std::fs::read(&cer).ok()?;
            let cert = parse_certificates(&der).ok()?.into_iter().next()?;
            let file = cer.with_extension("pfx");
            file.exists().then(|| StoredId {
                file,
                info: cert_info(&cert),
            })
        })
        .collect();
    out.sort_by(|a, b| {
        a.info
            .common_name
            .to_lowercase()
            .cmp(&b.info.common_name.to_lowercase())
    });
    out
}

fn store_id(id: &DigitalId, pfx: &[u8]) -> Result<StoredId, String> {
    let dir = ids_dir().ok_or("no data folder")?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let info = id.info();
    let base = format!("{}-{}", slug(&info.common_name), &info.sha1[..8]);
    let file = dir.join(format!("{base}.pfx"));
    std::fs::write(&file, pfx).map_err(|e| e.to_string())?;
    std::fs::write(dir.join(format!("{base}.cer")), cert_to_der(&id.certificate))
        .map_err(|e| e.to_string())?;
    Ok(StoredId { file, info })
}

/// Add a PKCS#12 file to the store (the password is checked).
pub fn import_id(data: &[u8], password: &str) -> Result<StoredId, String> {
    let id = DigitalId::from_pkcs12(data, password).map_err(|e| e.to_string())?;
    store_id(&id, data)
}

/// Create a self-signed digital ID and add it to the store.
pub fn create_id(info: &IdentityInfo, kind: KeyKind, years: u32, password: &str) -> Result<StoredId, String> {
    if password.chars().count() < 6 {
        return Err("the password needs at least 6 characters".into());
    }
    let id = DigitalId::create_self_signed(info, kind, years).map_err(|e| e.to_string())?;
    let pfx = id.to_pkcs12(password).map_err(|e| e.to_string())?;
    store_id(&id, &pfx)
}

pub fn delete_id(id: &StoredId) {
    let _ = std::fs::remove_file(&id.file);
    let _ = std::fs::remove_file(id.file.with_extension("cer"));
}

/// Unlock a stored ID with its password.
pub fn load_id(id: &StoredId, password: &str) -> Result<DigitalId, String> {
    let data = std::fs::read(&id.file).map_err(|e| e.to_string())?;
    DigitalId::from_pkcs12(&data, password).map_err(|e| e.to_string())
}

/// Trusted certificates.
pub fn trusted_certs() -> Vec<Certificate> {
    let Some(dir) = trusted_dir() else {
        return Vec::new();
    };
    std::fs::read_dir(&dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| std::fs::read(e.path()).ok())
        .filter_map(|d| parse_certificates(&d).ok())
        .flatten()
        .collect()
}

/// Add certificates to the trusted list. Returns how many were new.
pub fn add_trusted(certs: &[Certificate]) -> Result<usize, String> {
    let dir = trusted_dir().ok_or("no data folder")?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let mut added = 0;
    for c in certs {
        let info = cert_info(c);
        let path = dir.join(format!("{}.cer", info.sha1));
        if !path.exists() {
            std::fs::write(&path, cert_to_der(c)).map_err(|e| e.to_string())?;
            added += 1;
        }
    }
    Ok(added)
}

pub fn remove_trusted(sha1: &str) {
    if let Some(dir) = trusted_dir() {
        let _ = std::fs::remove_file(dir.join(format!("{sha1}.cer")));
    }
}

/// Signature validation results of the active document (cached per revision).
pub fn signature_status(st: AppState) -> Vec<SignatureStatus> {
    st.with_active(|t| {
        if let Some((rev, s)) = &t.sig_status {
            if *rev == t.revision {
                return s.clone();
            }
        }
        let s = t.doc.validate_signatures(&trusted_certs());
        t.sig_status = Some((t.revision, s.clone()));
        s
    })
    .unwrap_or_default()
}

/// Forget cached validation results (after the trusted list changed).
pub fn invalidate_status(st: AppState) {
    for t in st.docs_mut().iter_mut() {
        t.sig_status = None;
    }
}

/// Media types of time stamp requests and answers (RFC 3161).
const TIME_STAMP_QUERY: &str = "application/timestamp-query";
const TIME_STAMP_REPLY: &str = "application/timestamp-reply";

/// The way time stamp requests reach the server at `url`: an HTTP POST.
pub fn tsa_transport(url: String) -> impl FnMut(&[u8]) -> folitecta_core::Result<Vec<u8>> {
    move |request| {
        crate::http::post(&url, TIME_STAMP_QUERY, TIME_STAMP_REPLY, request)
            .map_err(|e| folitecta_core::Error::Invalid(format!("{url}: {e}")))
    }
}

struct SignRequest {
    id: DigitalId,
    opts: SignOptions,
    /// Address of the time stamp server, when the signature gets a time stamp.
    tsa: Option<String>,
}

/// Signature waiting for the user to draw its rectangle.
static PENDING: Mutex<Option<SignRequest>> = Mutex::new(None);

/// Sign now, or let the user draw the signature rectangle first; with
/// `tsa`, the signature gets a time stamp from that server.
pub fn start_sign(st: AppState, id: DigitalId, opts: SignOptions, tsa: Option<String>) {
    match &opts.placement {
        SignPlacement::New { rect, .. } if rect[2] - rect[0] < 1.0 => {
            *PENDING.lock().expect("pending signature") = Some(SignRequest { id, opts, tsa });
            st.set_tool(Tool::SignArea);
            st.toast(ToastKind::Info, t(st.lang(), "toast.draw_signature"), "");
        }
        _ => finish_sign(st, id, opts, tsa),
    }
}

/// The signature rectangle was drawn on `page` (PDF space).
pub fn finish_sign_area(st: AppState, page: usize, rect: [f32; 4]) {
    let req = PENDING.lock().expect("pending signature").take();
    st.set_tool(Tool::Hand);
    if let Some(mut req) = req {
        req.opts.placement = SignPlacement::New { page, rect };
        finish_sign(st, req.id, req.opts, req.tsa);
    }
}

/// Drop a signature that waits for its rectangle.
pub fn cancel_pending() {
    PENDING.lock().expect("pending signature").take();
}

/// The active document signed by `id` (with a time stamp from the server
/// at `tsa`), made on a worker thread while the server answers: the signed
/// file (encrypted like the document when it is protected) and the
/// document it opens as.
pub async fn signed_bytes(
    st: AppState,
    id: DigitalId,
    opts: SignOptions,
    tsa: Option<String>,
) -> Result<SignedFile, String> {
    let copy = st
        .read_active(|t| t.doc.work_copy())
        .ok_or("no document")?
        .map_err(|e| st.core_error_text(&e))?;
    tokio::task::spawn_blocking(move || {
        let mut send = tsa.map(tsa_transport);
        let send: Option<&mut Transport<'_>> = send.as_mut().map(|s| s as &mut Transport<'_>);
        copy.sign_to_file(&id, &opts, send)
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| st.core_error_text(&e))
}

/// The active document with a document time stamp from the server at `url`.
pub async fn time_stamped_bytes(st: AppState, url: String) -> Result<SignedFile, String> {
    let copy = st
        .read_active(|t| t.doc.work_copy())
        .ok_or("no document")?
        .map_err(|e| st.core_error_text(&e))?;
    tokio::task::spawn_blocking(move || copy.time_stamp_to_file(&mut tsa_transport(url)))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| st.core_error_text(&e))
}

/// Ask where to save a signed (or time-stamped) file made from the document
/// of tab `tab`, write it and show it in place of that document. A
/// protected document's signed file is shown as it was made: its password
/// is not asked again.
pub async fn save_signed(st: AppState, tab: u64, signed: SignedFile, suffix: &str, done: &str) {
    let name = files::suggested_name(st, suffix);
    let Some(file) = rfd::AsyncFileDialog::new()
        .set_title(t(st.lang(), "sig.save_signed"))
        .add_filter("PDF", &["pdf"])
        .set_file_name(&name)
        .save_file()
        .await
    else {
        st.toast(ToastKind::Warn, t(st.lang(), "toast.sign_cancelled"), "");
        return;
    };
    let path = file.path().to_path_buf();
    write_signed(st, tab, signed, path, done);
}

/// Write a signed file to `path` and show it in place of the document of
/// tab `tab`.
pub fn write_signed(st: AppState, tab: u64, signed: SignedFile, path: PathBuf, done: &str) {
    if let Err(e) = folitecta_core::document::write_pdf_file(&path, &signed.bytes) {
        st.core_error(&e);
        return;
    }
    let protected = signed.is_protected();
    match signed.into_document(Some(path)) {
        Ok(doc) => files::replace_tab_document(st, tab, doc, true),
        Err(e) => {
            st.core_error(&e);
            return;
        }
    }
    show_pane(st, Pane::Signatures);
    let detail = if protected {
        t(st.lang(), "toast.protected_kept")
    } else {
        String::new()
    };
    st.toast(ToastKind::Ok, t(st.lang(), done), detail);
}

/// Sign the active document and save the signed file.
fn finish_sign(st: AppState, id: DigitalId, opts: SignOptions, tsa: Option<String>) {
    if no_doc(st) {
        return;
    }
    let Some(tab) = st.read_active(|t| t.id) else {
        return;
    };
    if tsa.is_some() {
        st.toast(ToastKind::Info, t(st.lang(), "toast.time_stamp_asking"), "");
    }
    // The dialog that asked has closed: not tied to it.
    dioxus::core::spawn_forever(async move {
        match signed_bytes(st, id, opts, tsa).await {
            Ok(signed) => save_signed(st, tab, signed, "-signed", "toast.signed").await,
            Err(e) => st.error(e),
        }
    });
}

/// Add a document time stamp from the server at `url` and save the stamped file.
pub fn time_stamp_document(st: AppState, url: String) {
    if no_doc(st) {
        return;
    }
    let Some(tab) = st.read_active(|t| t.id) else {
        return;
    };
    st.toast(ToastKind::Info, t(st.lang(), "toast.time_stamp_asking"), "");
    dioxus::core::spawn_forever(async move {
        match time_stamped_bytes(st, url).await {
            Ok(stamped) => save_signed(st, tab, stamped, "-timestamped", "toast.time_stamped").await,
            Err(e) => st.error(e),
        }
    });
}

fn sign_dialog(st: AppState, certify: u8, invisible: bool) {
    if no_doc(st) {
        return;
    }
    st.set_dialog(Dialog::Sign {
        certify,
        invisible,
        field: None,
    });
}

fn validate_all(st: AppState) {
    if no_doc(st) {
        return;
    }
    invalidate_status(st);
    let s = signature_status(st);
    show_pane(st, Pane::Signatures);
    if s.is_empty() {
        st.toast(ToastKind::Info, t(st.lang(), "toast.no_signatures"), "");
        return;
    }
    let valid = s.iter().filter(|x| x.valid).count();
    let kind = if valid == s.len() {
        ToastKind::Ok
    } else {
        ToastKind::Warn
    };
    st.toast(
        kind,
        tf(
            st.lang(),
            "toast.signatures_checked",
            &[("valid", &valid.to_string()), ("count", &s.len().to_string())],
        ),
        "",
    );
}

fn clear_all(st: AppState) {
    if no_doc(st) {
        return;
    }
    if !st.read_active(|t| t.doc.has_signatures()).unwrap_or(false) {
        st.toast(ToastKind::Info, t(st.lang(), "toast.no_signatures"), "");
        return;
    }
    let lang = st.lang();
    st.set_dialog(Dialog::Confirm {
        title: t(lang, "cmd.document.signatures.clear"),
        body: t(lang, "dialog.confirm.clear_signatures"),
        action: ConfirmAction::ClearSignatures,
    });
}

fn summary(st: AppState) {
    if no_doc(st) {
        return;
    }
    let s = signature_status(st);
    match s.first() {
        Some(first) => st.set_dialog(Dialog::SignatureDetails(first.field.clone())),
        None => st.toast(ToastKind::Info, t(st.lang(), "toast.no_signatures"), ""),
    }
}

/// Open the revision a signature of the active document covers in a new tab.
pub fn open_signed_revision(st: AppState, field: &str) {
    if let Some(tab) = st.read_active(|t| t.id) {
        open_signed_revision_with(st, tab, field, None);
    }
}

/// Open the revision a signature of the document of tab `tab` covers in a
/// new tab; the revision of a protected document is encrypted like it, and
/// its password is asked.
pub fn open_signed_revision_with(st: AppState, tab: u64, field: &str, password: Option<&str>) {
    let bytes = st
        .docs
        .peek()
        .iter()
        .find(|t| t.id == tab)
        .map(|t| t.doc.signed_revision(field));
    let ask = |retry| {
        st.set_dialog(Dialog::Password {
            target: PasswordTarget::Revision {
                tab,
                field: field.to_string(),
            },
            retry,
        })
    };
    match bytes {
        Some(Ok(bytes)) => match folitecta_core::Document::from_bytes(bytes, None, password) {
            Ok(doc) => files::open_document_tab(st, doc),
            Err(folitecta_core::Error::PasswordRequired) => ask(false),
            Err(folitecta_core::Error::WrongPassword) => ask(true),
            Err(e) => st.core_error(&e),
        },
        Some(Err(e)) => st.core_error(&e),
        None => {}
    }
}

handlers! {
    "document.signatures.sign" => |st| sign_dialog(st, 0, false),
    "digisig.sign" => |st| sign_dialog(st, 0, false),
    "digisig.view.sign" => |st| sign_dialog(st, 0, false),
    "tool.digisig" => |st| sign_dialog(st, 0, false),
    "document.signatures.certify" => |st| sign_dialog(st, 2, false),
    "document.signatures.certify.invisible" => |st| sign_dialog(st, 2, true),
    "digisig.certify" => |st| sign_dialog(st, 2, false),
    "digisig.view.certify" => |st| sign_dialog(st, 2, false),
    "document.signatures.validateAll" => validate_all,
    "digisig.validate" => validate_all,
    "digisig.view.validate" => validate_all,
    "document.signatures.timestamp" => |st| if !no_doc(st) { st.set_dialog(Dialog::TimeStamp) },
    "document.signatures.clear" => clear_all,
    "digisig.clear" => clear_all,
    "digisig.view.clear" => clear_all,
    "digisig.summary" => summary,
    "digisig.view.summary" => summary,
    "signs.digitalIDs" => |st| st.set_dialog(Dialog::DigitalIds),
    "pubsec.cert.add" => |st| st.set_dialog(Dialog::TrustedCerts),
    "pubsec.cert.browse" => |st| st.set_dialog(Dialog::TrustedCerts),
    "pubsec.cert.details" => |st| st.set_dialog(Dialog::TrustedCerts),
    "pubsec.cert.remove" => |st| st.set_dialog(Dialog::TrustedCerts),
}
