//! Interaction step: revocation of signature certificates, with an OCSP
//! responder and a revocation list served on this computer (127.0.0.1). While
//! online checks are off nothing is asked; turned on, Validate All Signatures
//! sends one OCSP request that carries only the certificate's identifiers;
//! the details show the revocation of the certificate and where it came
//! from; the validation material added and saved lets the reopened file pass
//! without asking anything.

use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use folitecta_core::digsig::crl;
use folitecta_core::digsig::ocsp::{self, OcspAnswer, OcspTimes};
use folitecta_core::digsig::{
    CertProfile, Check, DigitalId, KeyKind, RevocationKind, RevocationNote, RevocationSource,
    RevocationState, SignOptions, SignatureStatus, cert_info,
};

use crate::commands::revocation::{ONLINE_KEY, set_online};
use crate::commands::signatures::{add_trusted, current_status, remove_trusted};
use crate::i18n::t;
use crate::state::AfterUnsaved;

use super::steps_timestamp::{identity, local_server};
use super::*;

/// The requests the server got: (first line, body).
type Requests = Arc<Mutex<Vec<(String, Vec<u8>)>>>;

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Answer one request: an OCSP POST with "good" from `ca`, the GET of the
/// (empty) revocation list of `ca`.
fn answer(mut s: std::net::TcpStream, ca: &DigitalId, log: &Requests) -> std::io::Result<()> {
    s.set_read_timeout(Some(Duration::from_secs(10)))?;
    let mut data = Vec::new();
    let mut buf = [0u8; 4096];
    let (head, body) = loop {
        let n = s.read(&mut buf)?;
        if n == 0 {
            return Ok(());
        }
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
                break (head, data[end + 4..end + 4 + len].to_vec());
            }
        }
    };
    let first = head.lines().next().unwrap_or_default().to_string();
    if let Ok(mut l) = log.lock() {
        l.push((first.clone(), body.clone()));
    }
    let t = now();
    let reply = if first.starts_with("POST /ocsp ") {
        let times = OcspTimes {
            produced_at: t,
            this_update: t,
            next_update: Some(t + 7 * 86_400),
        };
        ocsp::respond(ca, &ca.certificate, &body, &|_| OcspAnswer::Good, times).ok()
    } else if first.starts_with("GET /ca.crl ") {
        crl::make(ca, &[], t - 60, Some(t + 7 * 86_400), 1).ok()
    } else {
        None
    };
    match reply {
        Some(r) => {
            write!(
                s,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                r.len()
            )?;
            s.write_all(&r)
        }
        None => s.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"),
    }
}

/// A server of OCSP answers and revocation lists of `ca` on this computer:
/// its address, and the requests it gets.
fn serve(ca: DigitalId) -> Option<(String, Requests)> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").ok()?;
    let base = format!("http://{}", listener.local_addr().ok()?);
    let log: Requests = Arc::default();
    let log2 = log.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let _ = answer(stream, &ca, &log2);
        }
    });
    Some((base, log))
}

async fn count(selector: &str) -> u64 {
    eval(&format!(
        "return document.querySelectorAll({}).length;",
        serde_json::to_string(selector).unwrap_or_default()
    ))
    .await
    .ok()
    .and_then(|v| v.as_u64())
    .unwrap_or(0)
}

async fn text_of(selector: &str) -> String {
    eval(&format!(
        "const e = document.querySelector({}); return e ? e.textContent : '';",
        serde_json::to_string(selector).unwrap_or_default()
    ))
    .await
    .ok()
    .and_then(|v| v.as_str().map(str::to_string))
    .unwrap_or_default()
}

fn requests(log: &Requests) -> Vec<(String, Vec<u8>)> {
    log.lock().map(|l| l.clone()).unwrap_or_default()
}

fn first_signature(s: &[SignatureStatus]) -> Option<&SignatureStatus> {
    s.iter().find(|s| !s.document_time_stamp)
}

fn describe(s: Option<&SignatureStatus>) -> String {
    s.map(|s| {
        format!(
            "valid {}, checks {:?}, material {}, revocation {:?}",
            s.valid,
            s.checks,
            s.later_revisions_material,
            s.revocation
                .iter()
                .map(|e| (e.state, e.source, e.kind, e.notes.clone()))
                .collect::<Vec<_>>()
        )
    })
    .unwrap_or_else(|| "no signature".into())
}

pub(super) async fn run(cx: &mut Ctx<'_>) {
    let (st, dir) = (cx.st, cx.dir);
    let lang = st.lang();
    let t0 = now();
    let ca = DigitalId::create_with(
        &identity("Self Test Root CA"),
        KeyKind::EcP256,
        &CertProfile {
            not_before: t0 - 86_400,
            not_after: t0 + 5 * 365 * 86_400,
            ca: true,
            ..Default::default()
        },
    );
    let authority = DigitalId::create_time_stamp_authority(
        &identity("Self Test Revocation Time Authority"),
        KeyKind::EcP256,
        1,
    );
    let (Ok(ca), Ok(authority)) = (ca, authority) else {
        cx.record(
            "certification authority and time stamp authority created",
            false,
            String::new(),
        );
        return;
    };
    let (Some((base, log)), Some(tsa_url)) = (serve(ca.clone()), local_server(authority.clone())) else {
        cx.record(
            "revocation and time stamp servers on this computer",
            false,
            String::new(),
        );
        return;
    };
    let signer = ca.issue(
        &identity("Self Test Revocation Signer"),
        KeyKind::EcP256,
        &CertProfile {
            not_before: t0 - 3600,
            not_after: t0 + 365 * 86_400,
            ocsp_url: Some(format!("{base}/ocsp")),
            crl_url: Some(format!("{base}/ca.crl")),
            ..Default::default()
        },
    );
    let Ok(signer) = signer else {
        cx.record("signer issued by the authority", false, String::new());
        return;
    };
    let _ = add_trusted(&[ca.certificate.clone(), authority.certificate.clone()]);
    let was_online = st.value(ONLINE_KEY);
    set_online(st, false);
    // A signature with a time stamp (its time is proven), through the
    // signing code of the interface.
    let opts = SignOptions {
        reason: "Self-test of revocation".into(),
        ..Default::default()
    };
    let signed = crate::commands::signatures::signed_bytes(st, signer.clone(), opts, Some(tsa_url)).await;
    let Ok(signed) = signed else {
        cx.record(
            "signature with a time stamp",
            false,
            signed.err().unwrap_or_default(),
        );
        return;
    };
    let signed_path = dir.join("revocation-signed.pdf");
    let _ = std::fs::write(&signed_path, &signed.bytes);
    crate::files::open_path(st, signed_path, None);
    pause(1000).await;
    {
        let mut ui = st.ui_mut();
        ui.show_left_pane = true;
        ui.left_pane = "signatures".into();
    }

    // 1. Offline: cannot be determined, and nothing is asked.
    crate::commands::dispatch(st, "document.signatures.validateAll");
    pause(1500).await;
    let status = current_status(st).await;
    let s = first_signature(&status);
    let offline_ok = s.is_some_and(|s| {
        s.checks.revocation == Check::Warning
            && s.revocation.len() == 1
            && s.revocation[0].state == RevocationState::Unknown
            && s.revocation[0].notes.contains(&RevocationNote::NoData)
    });
    let sent = requests(&log).len();
    cx.record(
        "revocation cannot be determined offline, and nothing is sent while online checks are off",
        offline_ok && sent == 0,
        format!("{} request(s); {}", sent, describe(s)),
    );

    // 2. Online: one OCSP request with the certificate's identifiers only.
    set_online(st, true);
    crate::commands::dispatch(st, "document.signatures.validateAll");
    let mut fetched = 0;
    for _ in 0..50 {
        pause(200).await;
        fetched = st.read_active(|t| t.revocation.data.ocsps.len()).unwrap_or(0);
        if fetched == 1 {
            break;
        }
    }
    let status = current_status(st).await;
    let s = first_signature(&status);
    let online_ok = s.is_some_and(|s| {
        s.checks.revocation == Check::Passed
            && s.revocation.first().is_some_and(|e| {
                e.source == Some(RevocationSource::Fetched) && e.kind == Some(RevocationKind::Ocsp)
            })
    });
    let reqs = requests(&log);
    let expected = ocsp::request(&signer.certificate, &ca.certificate).unwrap_or_default();
    let request_ok = reqs.len() == 1
        && reqs[0].0.starts_with("POST /ocsp ")
        && reqs[0].1 == expected
        && expected.len() < 150;
    cx.record(
        "online check: one OCSP request with only the certificate's identifiers",
        fetched == 1 && online_ok && request_ok,
        format!(
            "{fetched} answer(s) kept, requests {:?} ({} bytes); {}",
            reqs.iter().map(|r| r.0.clone()).collect::<Vec<_>>(),
            reqs.first().map_or(0, |r| r.1.len()),
            describe(s)
        ),
    );

    // 3. The details show the revocation of the certificate and its source.
    let field = s.map(|s| s.field.clone()).unwrap_or_default();
    st.set_dialog(Dialog::SignatureDetails(field));
    let mut rows = 0;
    for _ in 0..30 {
        pause(150).await;
        rows = count(".dialog .rev-row").await;
        if rows == 1 {
            break;
        }
    }
    let ok_rows = count(".dialog .rev-row.ok").await;
    let source = text_of(".dialog .chip.rev-source").await;
    let checks_ok = count(".dialog .sig-check.ok").await;
    let _ = capture_window(&dir.join("interaction-revocation.png"));
    st.set_dialog(Dialog::None);
    cx.record(
        "signature details show the revocation of the certificate with its source",
        rows == 1 && ok_rows == 1 && source == t(lang, "sig.rev_source_fetched") && checks_ok == 4,
        format!("{rows} row(s), {ok_rows} ok, source {source:?}, {checks_ok} check(s) passed"),
    );

    // While the signatures are checked again (held here), the pane keeps the
    // last results dimmed and says that they are being checked.
    crate::commands::signatures::HOLD_CHECKS.store(true, std::sync::atomic::Ordering::SeqCst);
    crate::commands::signatures::invalidate_status(st);
    let (mut label, mut stale) = (0, 0);
    for _ in 0..20 {
        pause(150).await;
        label = count(".sig-checking").await;
        stale = count(".sig-item.stale").await;
        if label > 0 && stale > 0 {
            break;
        }
    }
    crate::commands::signatures::HOLD_CHECKS.store(false, std::sync::atomic::Ordering::SeqCst);
    let mut cleared = false;
    for _ in 0..40 {
        pause(150).await;
        if count(".sig-checking").await == 0 && count(".sig-item.stale").await == 0 {
            cleared = true;
            break;
        }
    }
    cx.record(
        "while the signatures are checked again the last results stay, dimmed, with a note",
        label == 1 && stale == 1 && cleared,
        format!("{label} note(s), {stale} dimmed item(s), cleared afterwards {cleared}"),
    );

    // 4. Validation material, saved; the reopened file passes from its store
    // without asking anything.
    st.set_dialog(Dialog::ValidationMaterial);
    pause(800).await;
    let _ = eval("document.querySelector('.dialog-foot .tbtn.primary')?.click();").await;
    let mut dirty = false;
    for _ in 0..50 {
        pause(200).await;
        dirty = st.read_active(|t| t.doc.is_dirty()).unwrap_or(false);
        if dirty {
            break;
        }
    }
    let saved_path = dir.join("with-material.pdf");
    let saved = st.with_active(|t| t.doc.save_as(&saved_path)).map(|r| r.is_ok());
    if let Some(id) = st.read_active(|t| t.id) {
        crate::files::close_tab(st, id, true, AfterUnsaved::Close);
        pause(300).await;
    }
    set_online(st, false);
    let before = requests(&log).len();
    crate::files::open_path(st, saved_path, None);
    pause(1000).await;
    let status = current_status(st).await;
    let s = first_signature(&status);
    let reopened_ok = s.is_some_and(|s| {
        s.valid
            && s.checks.integrity == Check::Passed
            && s.later_revisions_material
            && s.checks.revocation == Check::Passed
            && s.revocation
                .first()
                .is_some_and(|e| e.source == Some(RevocationSource::Dss))
    });
    let after = requests(&log).len();
    cx.record(
        "validation material saved: the reopened file passes from its DSS without the network",
        dirty && saved == Some(true) && reopened_ok && after == before,
        format!(
            "dirty {dirty}, saved {saved:?}, requests {before} -> {after}; {}",
            describe(s)
        ),
    );

    // Leave things as they were.
    if let Some(id) = st.read_active(|t| t.id) {
        crate::files::close_tab(st, id, true, AfterUnsaved::Close);
    }
    remove_trusted(&cert_info(&ca.certificate).sha1);
    remove_trusted(&cert_info(&authority.certificate).sha1);
    st.set_value(ONLINE_KEY, was_online);
    crate::commands::signatures::invalidate_status(st);
    st.ui_mut().left_pane = "thumbnails".into();
    pause(400).await;
}
