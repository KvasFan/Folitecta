//! Interaction step: time stamps from a time stamp server on this computer (a
//! small RFC 3161 authority answering over HTTP). A signature with a time
//! stamp and a document time stamp go through the interface's HTTP client;
//! the signatures pane and the separate checks of the details show them.

use std::io::{Read, Write};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use folitecta_core::digsig::{Check, DigitalId, IdentityInfo, KeyKind, SignOptions, timestamp};

use crate::state::AfterUnsaved;

use super::*;

/// Answer one HTTP request of a time stamp with `authority`.
fn answer(mut s: std::net::TcpStream, authority: &DigitalId, serial: u64) -> std::io::Result<()> {
    s.set_read_timeout(Some(Duration::from_secs(10)))?;
    let mut data = Vec::new();
    let mut buf = [0u8; 4096];
    let body = loop {
        let n = s.read(&mut buf)?;
        if n == 0 {
            return Ok(());
        }
        data.extend_from_slice(&buf[..n]);
        if let Some(end) = data.windows(4).position(|w| w == b"\r\n\r\n") {
            let head = String::from_utf8_lossy(&data[..end]).to_ascii_lowercase();
            let len = head
                .lines()
                .find_map(|l| l.strip_prefix("content-length:"))
                .and_then(|v| v.trim().parse::<usize>().ok())
                .unwrap_or(0);
            if data.len() >= end + 4 + len {
                break data[end + 4..end + 4 + len].to_vec();
            }
        }
    };
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let reply = timestamp::respond(authority, &body, now, serial).unwrap_or_default();
    write!(
        s,
        "HTTP/1.1 200 OK\r\nContent-Type: application/timestamp-reply\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        reply.len()
    )?;
    s.write_all(&reply)
}

/// A time stamp server on this computer answering with `authority`; its address.
fn local_server(authority: DigitalId) -> Option<String> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").ok()?;
    let url = format!("http://{}/tsa", listener.local_addr().ok()?);
    std::thread::spawn(move || {
        for (serial, stream) in listener.incoming().flatten().enumerate() {
            let _ = answer(stream, &authority, serial as u64 + 1);
        }
    });
    Some(url)
}

fn identity(name: &str) -> IdentityInfo {
    IdentityInfo {
        name: name.into(),
        organization: "Folitecta".into(),
        ..Default::default()
    }
}

async fn count(selector: &str) -> u64 {
    document::eval(&format!("return document.querySelectorAll('{selector}').length;"))
        .await
        .ok()
        .and_then(|v| v.as_u64())
        .unwrap_or(0)
}

pub(super) async fn run(cx: &mut Ctx<'_>) {
    let (st, dir) = (cx.st, cx.dir);
    let authority =
        DigitalId::create_time_stamp_authority(&identity("Self Test Time Authority"), KeyKind::EcP256, 1);
    let signer = DigitalId::create_self_signed(&identity("Self Test Stamped Signer"), KeyKind::EcP256, 1);
    let (Ok(authority), Ok(signer)) = (authority, signer) else {
        cx.record("time stamp authority and signer created", false, String::new());
        return;
    };
    let Some(url) = local_server(authority.clone()) else {
        cx.record("time stamp server on this computer", false, String::new());
        return;
    };
    // A signature with a time stamp, through the signing code of the interface.
    let opts = SignOptions {
        reason: "Self-test with a time stamp".into(),
        ..Default::default()
    };
    let signed = crate::commands::signatures::signed_bytes(st, signer.clone(), opts, Some(url.clone())).await;
    let Ok(signed) = signed else {
        cx.record(
            "signature with a time stamp",
            false,
            signed.err().unwrap_or_default(),
        );
        return;
    };
    let signed_path = dir.join("signed-with-time-stamp.pdf");
    let _ = std::fs::write(&signed_path, &signed);
    let before = st.docs.read().len();
    crate::files::open_path(st, signed_path, None);
    pause(1000).await;
    let signed_tab = st.read_active(|t| t.id);
    // A document time stamp over the signed file.
    let stamped = crate::commands::signatures::time_stamped_bytes(st, url.clone()).await;
    if let Some(id) = signed_tab.filter(|_| st.docs.read().len() > before) {
        crate::files::close_tab(st, id, true, AfterUnsaved::Close);
        pause(300).await;
    }
    let Ok(stamped) = stamped else {
        cx.record("document time stamp", false, stamped.err().unwrap_or_default());
        return;
    };
    let stamped_path = dir.join("time-stamped.pdf");
    let _ = std::fs::write(&stamped_path, &stamped);
    crate::files::open_path(st, stamped_path, None);
    pause(1000).await;
    let trusted = [authority.certificate.clone(), signer.certificate.clone()];
    let status = st
        .read_active(|t| t.doc.validate_signatures(&trusted))
        .unwrap_or_default();
    let sig = status.iter().find(|s| !s.document_time_stamp);
    let doc_stamp = status.iter().find(|s| s.document_time_stamp);
    let sig_ok = sig
        .map(|s| {
            s.valid
                && s.time_stamp.as_ref().is_some_and(|t| t.valid && t.trusted)
                && s.checks.time == Check::Passed
                && s.checks.identity == Check::Passed
                && s.revision_len == signed.len()
                && s.later_revisions_time_stamped
        })
        .unwrap_or(false);
    let stamp_ok = doc_stamp
        .map(|s| s.valid && s.covers_document && s.signer == "Self Test Time Authority")
        .unwrap_or(false);
    cx.record(
        "a signature and a document time stamp from a time stamp server, both valid",
        status.len() == 2 && sig_ok && stamp_ok,
        format!(
            "{} signature(s); signature: {:?}; document time stamp: {:?}",
            status.len(),
            sig.map(|s| (s.valid, s.checks, s.problems.clone())),
            doc_stamp.map(|s| (s.valid, s.covers_document, s.problems.clone()))
        ),
    );
    // The pane and the details with the separate checks. Neither the signer
    // nor the authority is in the trusted list of the interface, and a later
    // revision adds the document time stamp: integrity, signer and time are
    // warnings, revocation is not checked.
    {
        let mut ui = st.ui_mut();
        ui.show_left_pane = true;
        ui.left_pane = "signatures".into();
    }
    pause(800).await;
    let items = count(".sig-item").await;
    let stamp_chips = count(".chip.sig-stamp").await;
    let field = sig.map(|s| s.field.clone()).unwrap_or_default();
    st.set_dialog(Dialog::SignatureDetails(field));
    let mut rows = 0;
    for _ in 0..20 {
        pause(150).await;
        rows = count(".dialog .sig-check").await;
        if rows == 4 {
            break;
        }
    }
    let warnings = count(".dialog .sig-check.warn").await;
    let failed = count(".dialog .sig-check.bad").await;
    let not_checked = count(".dialog .sig-check.none").await;
    let _ = capture_window(&dir.join("interaction-timestamp.png"));
    st.set_dialog(Dialog::None);
    cx.record(
        "time stamps in the signatures pane and the separate checks of a signature",
        items == 2 && stamp_chips == 1 && rows == 4 && warnings == 3 && failed == 0 && not_checked == 1,
        format!(
            "{items} item(s), {stamp_chips} stamp chip(s), {rows} check(s): {warnings} warning(s), {failed} failed, {not_checked} not checked"
        ),
    );
    if let Some(id) = st.read_active(|t| t.id) {
        crate::files::close_tab(st, id, true, AfterUnsaved::Close);
    }
    st.ui_mut().left_pane = "thumbnails".into();
    pause(400).await;
}
