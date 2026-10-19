//! Revocation of signature certificates from the interface: the online
//! option (off unless the user turns it on), the requests sent over HTTP
//! when it is on, and the validation material added to the document.
//!
//! Documents are data: the addresses written in their certificates are asked
//! only when the option is on, on an explicit action (Validate All
//! Signatures, Add Validation Material), and only for chains that end at a
//! trusted certificate.

use std::sync::atomic::{AtomicBool, Ordering};

use folitecta_core::digsig::{FetchSummary, RevocationPool, RevocationRequest};

use crate::commands::handlers;
use crate::commands::signatures::{invalidate_status, trusted_certs};
use crate::i18n::{t, tf};
use crate::state::{AppState, Dialog, DocTab, ToastKind};

/// The value of the online option ("1" when on).
pub const ONLINE_KEY: &str = "sign.online_revocation";

/// Media types of OCSP (RFC 6960 appendix A) and revocation lists (RFC 2585).
const OCSP_REQUEST: &str = "application/ocsp-request";
const OCSP_RESPONSE: &str = "application/ocsp-response";
const CRL_TYPES: &str = "application/pkix-crl, application/x-pkcs7-crl;q=0.5, */*;q=0.1";

/// Online revocation checks are on.
pub fn online(st: AppState) -> bool {
    st.value(ONLINE_KEY) == "1"
}

/// Turn online revocation checks on or off.
pub fn set_online(st: AppState, on: bool) {
    st.set_value(ONLINE_KEY, if on { "1" } else { "" });
    invalidate_status(st);
}

/// The way revocation requests reach their servers: an OCSP POST, or the
/// GET of a revocation list.
pub fn revocation_transport() -> impl FnMut(&RevocationRequest) -> folitecta_core::Result<Vec<u8>> {
    |req: &RevocationRequest| {
        match &req.ocsp {
            Some(body) => crate::http::post(&req.url, OCSP_REQUEST, OCSP_RESPONSE, body),
            None => crate::http::get(&req.url, CRL_TYPES),
        }
        .map_err(folitecta_core::Error::Invalid)
    }
}

/// An online check is running (one at a time).
static FETCHING: AtomicBool = AtomicBool::new(false);

/// Ends the running mark when the check ends, however it ends.
struct Running;

impl Drop for Running {
    fn drop(&mut self) {
        FETCHING.store(false, Ordering::SeqCst);
    }
}

pub(crate) fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn with_tab<T>(st: AppState, id: u64, f: impl FnOnce(&mut DocTab) -> T) -> Option<T> {
    let mut docs = st.docs_mut();
    let tab = docs.iter_mut().find(|t| t.id == id)?;
    let out = f(tab);
    tab.refresh();
    Some(out)
}

/// Ask online for the revocation data the signatures of the document of
/// tab `tab` miss (on another thread), keep the answers for the tab, then
/// call `then`.
fn fetch_then(st: AppState, tab: u64, then: impl FnOnce(AppState, u64, FetchSummary) + 'static) {
    if FETCHING.swap(true, Ordering::SeqCst) {
        st.toast(ToastKind::Info, t(st.lang(), "toast.revocation_busy"), "");
        return;
    }
    let running = Running;
    let Some((job, pool)) = with_tab(st, tab, |t| (t.doc.validation_job(), t.revocation.clone())) else {
        return;
    };
    dioxus::core::spawn_forever(async move {
        let _running = running;
        // Nothing is sent, and nothing said, when nothing needs asking.
        let (ask, held) = (job.clone(), pool.clone());
        let (needed, trusted) = tokio::task::spawn_blocking(move || {
            let trusted = trusted_certs();
            let needed = !ask.revocation_requests(&trusted, &held, now_secs()).is_empty();
            (needed, trusted)
        })
        .await
        .unwrap_or_default();
        if !needed {
            then(st, tab, FetchSummary::default());
            return;
        }
        st.toast(ToastKind::Info, t(st.lang(), "toast.revocation_asking"), "");
        let done = tokio::task::spawn_blocking(move || {
            let mut pool = pool;
            let mut send = revocation_transport();
            let summary = job.fetch_revocation(&trusted, &mut pool, now_secs(), &mut send);
            (pool, summary)
        })
        .await;
        match done {
            Ok((pool, summary)) => {
                with_tab(st, tab, |t| {
                    t.revocation = pool;
                    t.revocation_gen += 1;
                    t.sig_status = None;
                });
                then(st, tab, summary);
            }
            Err(e) => st.error(e),
        }
    });
}

/// After a validation: when online checks are on, ask for what the
/// signatures of the active document miss.
pub fn refresh_online(st: AppState) {
    if !online(st) {
        return;
    }
    let Some(tab) = st.read_active(|t| t.id) else {
        return;
    };
    fetch_then(st, tab, |st, _, summary| {
        if summary.certificates == 0 {
            return;
        }
        let kind = if summary.decided == summary.certificates {
            ToastKind::Ok
        } else {
            ToastKind::Warn
        };
        st.toast(
            kind,
            tf(
                st.lang(),
                "toast.revocation_fetched",
                &[
                    ("decided", &summary.decided.to_string()),
                    ("count", &summary.certificates.to_string()),
                ],
            ),
            "",
        );
    });
}

/// Add validation material to the active document: first what is asked
/// online (when the option is on), then the material of the data at hand.
pub fn add_material(st: AppState) {
    let Some((tab, can)) = st.read_active(|t| (t.id, t.doc.can_add_validation_material())) else {
        return;
    };
    if let Err(e) = can {
        if matches!(&e, folitecta_core::Error::Invalid(m) if m.contains("no signatures")) {
            st.toast(ToastKind::Info, t(st.lang(), "toast.no_signatures"), "");
        } else {
            st.core_error(&e);
        }
        return;
    }
    if online(st) {
        fetch_then(st, tab, |st, tab, _| apply_material(st, tab));
    } else {
        apply_material(st, tab);
    }
}

/// Work out the material on another thread, then add it to the document of
/// tab `tab` (one undo step).
fn apply_material(st: AppState, tab: u64) {
    let Some((job, pool)) = with_tab(st, tab, |t| (t.doc.validation_job(), t.revocation.clone())) else {
        return;
    };
    dioxus::core::spawn_forever(async move {
        let now = now_secs();
        let plan = tokio::task::spawn_blocking(move || job.plan_material(&trusted_certs(), &pool, now)).await;
        let plan = match plan {
            Ok(Ok(plan)) => plan,
            Ok(Err(e)) => return st.core_error(&e),
            Err(e) => return st.error(e),
        };
        let Some(result) = with_tab(st, tab, |t| {
            let r = t.doc.apply_validation_material(&plan, now);
            t.sig_status = None;
            r
        }) else {
            return;
        };
        let lang = st.lang();
        match result {
            Ok(report) if report.changed => {
                st.toast(
                    ToastKind::Ok,
                    tf(
                        lang,
                        "toast.material_added",
                        &[
                            ("certs", &report.certs_added.to_string()),
                            ("ocsp", &report.ocsps_added.to_string()),
                            ("crls", &report.crls_added.to_string()),
                        ],
                    ),
                    "",
                );
                if !report.missing.is_empty() {
                    st.toast(
                        ToastKind::Warn,
                        tf(
                            lang,
                            "toast.material_missing",
                            &[("count", &report.missing.len().to_string())],
                        ),
                        "",
                    );
                }
                crate::files::set_window_title(st);
            }
            Ok(report) if report.signatures == 0 && report.skipped > 0 => st.toast(
                ToastKind::Warn,
                tf(
                    lang,
                    "toast.material_skipped",
                    &[("count", &report.skipped.to_string())],
                ),
                "",
            ),
            Ok(_) => st.toast(ToastKind::Info, t(lang, "toast.material_nothing"), ""),
            Err(e) => st.core_error(&e),
        }
    });
}

/// The online data of a tab, after its document changed file (a new file
/// gets its own).
pub fn forget(t: &mut DocTab) {
    t.revocation = RevocationPool::default();
    t.revocation_gen += 1;
}

handlers! {
    "signaturesView.options" => |st| st.set_dialog(Dialog::SignatureOptions),
}

#[cfg(test)]
mod tests {
    use super::*;
    use folitecta_core::Document;
    use folitecta_core::digsig::crl;
    use folitecta_core::digsig::{
        CertProfile, Check, DigitalId, IdentityInfo, KeyKind, RevocationKind, RevocationSource,
    };
    use std::io::{Read, Write};

    const DAY: i64 = 86_400;

    fn who(name: &str) -> IdentityInfo {
        IdentityInfo {
            name: name.into(),
            ..Default::default()
        }
    }

    /// A server on this computer answering POST /ocsp with 500 and GET
    /// /ca.crl with `list`; the heads of the requests it got.
    fn serve(list: Vec<u8>, count: usize) -> (String, std::thread::JoinHandle<Vec<String>>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let handle = std::thread::spawn(move || {
            let mut heads = Vec::new();
            for _ in 0..count {
                let (mut s, _) = listener.accept().unwrap();
                let mut data = Vec::new();
                let mut buf = [0u8; 4096];
                loop {
                    let n = s.read(&mut buf).unwrap();
                    data.extend_from_slice(&buf[..n]);
                    if let Some(end) = data.windows(4).position(|w| w == b"\r\n\r\n") {
                        let head = String::from_utf8_lossy(&data[..end]).to_string();
                        let len = head
                            .to_ascii_lowercase()
                            .lines()
                            .find_map(|l| {
                                l.strip_prefix("content-length:")
                                    .map(|v| v.trim().parse::<usize>().unwrap_or(0))
                            })
                            .unwrap_or(0);
                        if data.len() >= end + 4 + len {
                            heads.push(head);
                            break;
                        }
                    }
                    if n == 0 {
                        break;
                    }
                }
                let head = heads.last().cloned().unwrap_or_default();
                if head.starts_with("GET /ca.crl") {
                    let _ = write!(
                        s,
                        "HTTP/1.1 200 OK\r\nContent-Type: application/pkix-crl\r\nContent-Length: {}\r\n\r\n",
                        list.len()
                    );
                    let _ = s.write_all(&list);
                } else {
                    let _ = s.write_all(b"HTTP/1.1 500 Internal Server Error\r\nContent-Length: 0\r\n\r\n");
                }
            }
            heads
        });
        (base, handle)
    }

    #[test]
    fn fetches_over_http_ocsp_first_then_the_list() {
        let now = now_secs();
        let ca = DigitalId::create_with(
            &who("Local Root CA"),
            KeyKind::EcP256,
            &CertProfile {
                not_before: now - 400 * DAY,
                not_after: now + 400 * DAY,
                ca: true,
                ..Default::default()
            },
        )
        .unwrap();
        let list = crl::make(&ca, &[], now - 60, Some(now + DAY), 1).unwrap();
        let (base, server) = serve(list, 2);
        let signer = ca
            .issue(
                &who("Local Signer"),
                KeyKind::EcP256,
                &CertProfile {
                    not_before: now - DAY,
                    not_after: now + DAY,
                    ocsp_url: Some(format!("{base}/ocsp")),
                    crl_url: Some(format!("{base}/ca.crl")),
                    ..Default::default()
                },
            )
            .unwrap();
        let signed = Document::new_blank(1, 200.0, 200.0)
            .unwrap()
            .sign(&signer, &Default::default())
            .unwrap();
        let doc = Document::from_bytes(signed, None, None).unwrap();
        let trusted = [ca.certificate.clone()];
        let mut pool = RevocationPool::default();
        let summary = doc.fetch_revocation(&trusted, &mut pool, now_secs(), &mut revocation_transport());
        assert_eq!((summary.decided, summary.requests), (1, 2), "{:?}", pool.failures);
        assert_eq!(pool.data.crls.len(), 1);
        let heads = server.join().unwrap();
        assert!(heads[0].starts_with("POST /ocsp HTTP/1.1"), "{}", heads[0]);
        assert!(heads[0].contains("Content-Type: application/ocsp-request"));
        assert!(heads[1].starts_with("GET /ca.crl HTTP/1.1"), "{}", heads[1]);
        let st = doc.validate_signatures_with(&trusted, &pool, now_secs());
        assert_eq!(st[0].checks.revocation, Check::Passed, "{:?}", st[0].revocation);
        assert_eq!(st[0].revocation[0].source, Some(RevocationSource::Fetched));
        assert_eq!(st[0].revocation[0].kind, Some(RevocationKind::Crl));
        // A closed port: two failures, nothing decided.
        let closed = {
            let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            l.local_addr().unwrap()
        };
        let lone = ca
            .issue(
                &who("Unreachable Signer"),
                KeyKind::EcP256,
                &CertProfile {
                    not_before: now - DAY,
                    not_after: now + DAY,
                    ocsp_url: Some(format!("http://{closed}/ocsp")),
                    crl_url: Some(format!("http://{closed}/ca.crl")),
                    ..Default::default()
                },
            )
            .unwrap();
        let signed = Document::new_blank(1, 200.0, 200.0)
            .unwrap()
            .sign(&lone, &Default::default())
            .unwrap();
        let doc = Document::from_bytes(signed, None, None).unwrap();
        let mut pool = RevocationPool::default();
        let summary = doc.fetch_revocation(&trusted, &mut pool, now_secs(), &mut revocation_transport());
        assert_eq!((summary.decided, summary.failures), (0, 2));
        let st = doc.validate_signatures_with(&trusted, &pool, now_secs());
        assert_eq!(st[0].checks.revocation, Check::Warning);
    }
}
