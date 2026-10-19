//! Digital signatures: the store of digital IDs and trusted certificates,
//! signing (visible, invisible, certifying), validation and clearing.

use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use dioxus::prelude::*;
use folitecta_core::digsig::{
    CertInfo, Certificate, Check, DigitalId, IdentityInfo, KeyKind, SignOptions, SignPlacement,
    SignatureStatus, SignedFile, Transport, cert_info, cert_to_der, parse_certificates,
};

use crate::commands::{handlers, no_doc, show_pane};
use crate::files;
use crate::i18n::{t, tf};
use crate::state::{
    AppState, ConfirmAction, Dialog, DocTab, Pane, PasswordTarget, ToastKind, Tool, app_data_dir,
};

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

/// The state validation results are for: the document's revision and saves
/// (the file it is checked against), the online data of its tab, and the
/// generation of the trusted list and options.
pub type StatusKey = ((u64, u64), u64, u64);

/// Bumped when the trusted list or the options of validation change.
static STATUS_GEN: AtomicU64 = AtomicU64::new(0);

/// Holds the checks before they start (the self-test looks at the views
/// while checks are under way); set only through [`HoldChecks`].
static HOLD_CHECKS: AtomicBool = AtomicBool::new(false);

/// The longest a check waits for a hold to end.
const HOLD_LIMIT: std::time::Duration = std::time::Duration::from_secs(10);

/// Holds the checks while it lives; dropped (also when the code that made
/// it stops half-way), the checks go on.
pub(crate) struct HoldChecks;

impl HoldChecks {
    pub(crate) fn new() -> HoldChecks {
        HOLD_CHECKS.store(true, Ordering::SeqCst);
        HoldChecks
    }
}

impl Drop for HoldChecks {
    fn drop(&mut self) {
        HOLD_CHECKS.store(false, Ordering::SeqCst);
    }
}

/// Wait while the checks are held, at most `limit`.
async fn wait_while_held(limit: std::time::Duration) {
    let started = std::time::Instant::now();
    while HOLD_CHECKS.load(Ordering::SeqCst) && started.elapsed() < limit {
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
}

/// Validations under way: (tab, key).
static CHECKING: Mutex<Vec<(u64, StatusKey)>> = Mutex::new(Vec::new());

pub fn status_key(t: &DocTab) -> StatusKey {
    (
        t.doc.validation_stamp(),
        t.revocation_gen,
        STATUS_GEN.load(Ordering::SeqCst),
    )
}

fn with_tab<T>(st: AppState, id: u64, f: impl FnOnce(&mut DocTab) -> T) -> Option<T> {
    let mut docs = st.docs_mut();
    let tab = docs.iter_mut().find(|t| t.id == id)?;
    let out = f(tab);
    tab.refresh();
    Some(out)
}

/// Validate the document of tab `tab` on another thread (the checks of
/// signatures, revocation data and later revisions take time) and keep the
/// result for the state it was made for.
async fn validate_tab(st: AppState, tab: u64) -> Option<Vec<SignatureStatus>> {
    wait_while_held(HOLD_LIMIT).await;
    let (key, job, pool) = st
        .docs
        .peek()
        .iter()
        .find(|t| t.id == tab)
        .map(|t| (status_key(t), t.doc.validation_job(), t.revocation.clone()))?;
    let result =
        tokio::task::spawn_blocking(move || job.run(&trusted_certs(), &pool, super::revocation::now_secs()))
            .await
            .ok();
    if let Some(s) = &result {
        let current = st.docs.peek().iter().find(|t| t.id == tab).map(status_key);
        if current == Some(key) {
            with_tab(st, tab, |t| t.sig_status = Some((key, s.clone())));
        }
    }
    result
}

/// Signature validation results of the active document: the results for its
/// current state when they are ready, else the last ones (while the checks
/// run on another thread).
pub fn signature_status(st: AppState) -> Vec<SignatureStatus> {
    let Some((tab, key, cached)) = st.read_active(|t| (t.id, status_key(t), t.sig_status.clone())) else {
        return Vec::new();
    };
    if let Some((k, s)) = &cached {
        if *k == key {
            return s.clone();
        }
    }
    let start = {
        let mut running = CHECKING.lock().expect("validations");
        let new = !running.contains(&(tab, key));
        if new {
            running.push((tab, key));
        }
        new
    };
    if start {
        dioxus::core::spawn_forever(async move {
            validate_tab(st, tab).await;
            CHECKING.lock().expect("validations").retain(|r| *r != (tab, key));
        });
    }
    cached.map(|(_, s)| s).unwrap_or_default()
}

/// The checks of the active document are running (its results are not for
/// its current state yet).
pub fn signatures_checking(st: AppState) -> bool {
    st.read_active(|t| t.sig_status.as_ref().map(|(k, _)| *k) != Some(status_key(t)))
        .unwrap_or(false)
}

/// Signature validation results of the active document for its current
/// state, waiting for the checks when they are not ready.
pub async fn current_status(st: AppState) -> Vec<SignatureStatus> {
    let Some((tab, key, cached)) = st.read_active(|t| (t.id, status_key(t), t.sig_status.clone())) else {
        return Vec::new();
    };
    match cached {
        Some((k, s)) if k == key => s,
        _ => validate_tab(st, tab).await.unwrap_or_default(),
    }
}

/// Forget cached validation results (after the trusted list or the options
/// changed): they are checked again, and the views that show them draw
/// again (the last results stay in view meanwhile).
pub fn invalidate_status(st: AppState) {
    STATUS_GEN.fetch_add(1, Ordering::SeqCst);
    // A write of the open tabs makes their views draw again.
    drop(st.docs_mut());
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
    show_pane(st, Pane::Signatures);
    dioxus::core::spawn_forever(async move {
        let s = current_status(st).await;
        if s.is_empty() {
            st.toast(ToastKind::Info, t(st.lang(), "toast.no_signatures"), "");
            return;
        }
        let valid = s
            .iter()
            .filter(|x| x.valid && x.checks.revocation != Check::Failed)
            .count();
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
        // With online checks on, ask for what the signatures miss.
        super::revocation::refresh_online(st);
    });
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
    dioxus::core::spawn_forever(async move {
        let s = current_status(st).await;
        match s.first() {
            Some(first) => st.set_dialog(Dialog::SignatureDetails(first.field.clone())),
            None => st.toast(ToastKind::Info, t(st.lang(), "toast.no_signatures"), ""),
        }
    });
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn holds_end_with_their_guard_and_after_their_limit() {
        {
            let _hold = HoldChecks::new();
            assert!(HOLD_CHECKS.load(Ordering::SeqCst));
        }
        assert!(!HOLD_CHECKS.load(Ordering::SeqCst));
        // Code that stops half-way (a panic, a dropped future) ends it too.
        let stopped = std::panic::catch_unwind(|| {
            let _hold = HoldChecks::new();
            panic!("stopped");
        });
        assert!(stopped.is_err());
        assert!(!HOLD_CHECKS.load(Ordering::SeqCst));
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap();
        runtime.block_on(async {
            let hold = HoldChecks::new();
            let held = async move {
                let _hold = hold;
                std::future::pending::<()>().await;
            };
            // The future holding it is dropped before it ends.
            let _ = tokio::time::timeout(std::time::Duration::from_millis(50), held).await;
        });
        assert!(!HOLD_CHECKS.load(Ordering::SeqCst));
        // A hold that is never ended gives way after its limit.
        let hold = HoldChecks::new();
        let started = std::time::Instant::now();
        runtime.block_on(wait_while_held(std::time::Duration::from_millis(200)));
        assert!(started.elapsed() < std::time::Duration::from_secs(2));
        drop(hold);
    }
}
