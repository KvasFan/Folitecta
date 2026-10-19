//! Interaction step: protection. Password Security offers AES-256 by default
//! and policies carry their encryption (older ones load as AES-256); the
//! dialog refuses passwords the chosen encryption cannot keep; a protected
//! copy asks for its password again after a wrong one; signing and
//! time-stamping a protected document write encrypted files that take the
//! place of the document without asking for the password; Revert and the
//! signed revision ask for it; and a protected document leaves no recovery
//! copy and no history thumbnail.

use folitecta_core::digsig::{DigitalId, KeyKind, SignOptions};
use folitecta_core::{Document, EncryptionProfile, SecurityPermissions};

use super::steps_timestamp::{identity, local_server};
use super::*;
use crate::commands::policies::policies;
use crate::state::{AfterUnsaved, PasswordTarget};

const USER: &str = "user-pw";

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

async fn click(selector: &str) {
    let _ = eval(&format!(
        "document.querySelector({})?.click();",
        serde_json::to_string(selector).unwrap_or_default()
    ))
    .await;
    pause(300).await;
}

async fn type_in(selector: &str, text: &str) {
    let _ = eval(&format!(
        "const i = document.querySelector({}); if (i) {{ i.value = {}; i.dispatchEvent(new Event('input', {{ bubbles: true }})); }}",
        serde_json::to_string(selector).unwrap_or_default(),
        serde_json::to_string(text).unwrap_or_default()
    ))
    .await;
    pause(200).await;
}

/// Choose `value` in the select `selector` (as a user does: a change event).
async fn choose(selector: &str, value: &str) {
    let _ = eval(&format!(
        "const s = document.querySelector({}); if (s) {{ s.value = {}; s.dispatchEvent(new Event('change', {{ bubbles: true }})); }}",
        serde_json::to_string(selector).unwrap_or_default(),
        serde_json::to_string(value).unwrap_or_default()
    ))
    .await;
    pause(300).await;
}

async fn value_of(selector: &str) -> String {
    eval(&format!(
        "return document.querySelector({})?.value || '';",
        serde_json::to_string(selector).unwrap_or_default()
    ))
    .await
    .ok()
    .and_then(|v| v.as_str().map(str::to_string))
    .unwrap_or_default()
}

fn contains(hay: &[u8], needle: &[u8]) -> bool {
    hay.windows(needle.len()).any(|w| w == needle)
}

/// Wait (a few seconds at most) for the dialog to be a password dialog.
async fn password_dialog(st: AppState) -> Option<(PasswordTarget, bool)> {
    for _ in 0..20 {
        pause(150).await;
        if let Dialog::Password { target, retry } = st.dialog.peek().clone() {
            return Some((target, retry));
        }
    }
    None
}

pub(super) async fn run(cx: &mut Ctx<'_>) {
    let (st, dir) = (cx.st, cx.dir.to_path_buf());
    if !st.has_doc() {
        crate::files::new_blank(st, 1, 595.0, 842.0);
        pause(500).await;
    }
    policies_and_dialog(cx).await;
    let Some(tab) = open_protected_copy(cx, &dir).await else {
        return;
    };
    sign_and_stamp(cx, &dir, tab).await;
    revert_revision_and_traces(cx, tab).await;
    st.set_dialog(Dialog::None);
}

/// Step 1: the encryption of Password Security, of policies and of the policy editor.
async fn policies_and_dialog(cx: &mut Ctx<'_>) {
    let st = cx.st;
    let saved = st.value("security.policies");
    st.set_value(
        "security.policies",
        r#"[{"name":"Old style","permissions":{"print":false}},{"name":"Legacy readers","permissions":{},"encryption":"aes-128"}]"#
            .to_string(),
    );
    crate::commands::dispatch(st, "document.security");
    pause(500).await;
    let default = value_of(".dialog .security-encryption").await;
    choose(".dialog .security-policy", "Legacy readers").await;
    let legacy = value_of(".dialog .security-encryption").await;
    choose(".dialog .security-policy", "Old style").await;
    let old = value_of(".dialog .security-encryption").await;
    // Printing is restricted and no owner password is typed: the hint says
    // who can lift the restrictions.
    let owner_hint = count(".dialog .security-owner-hint").await;
    // AES-128 cannot keep Chinese characters: the dialog stays open and says so.
    choose(".dialog .security-encryption", "aes-128").await;
    type_in(".dialog .security-user", "密码").await;
    click(".dialog-foot .tbtn.primary").await;
    let warned = st
        .toasts
        .peek()
        .iter()
        .any(|x| x.title == crate::i18n::t(st.lang(), "dialog.security.password_chars"));
    let still_open = matches!((st.dialog)(), Dialog::Security);
    let _ = capture_window(&cx.dir.join("interaction-protection-dialog.png"));
    st.set_dialog(Dialog::None);
    pause(300).await;
    // The editor of the policies: the chip of AES-128 and its select.
    crate::commands::dispatch(st, "securityPolicies");
    pause(500).await;
    let chips_before = count(".dialog .policy-row .policy-aes128").await;
    let _ = eval(
        "[...document.querySelectorAll('.dialog .policy-row')].find(r => r.textContent.includes('Old style'))?.click();",
    )
    .await;
    pause(200).await;
    click(".dialog .policy-edit").await;
    choose(".dialog .policy-encryption", "aes-128").await;
    click(".dialog .policy-save").await;
    let chips_after = count(".dialog .policy-row .policy-aes128").await;
    let edited = policies(st)
        .iter()
        .find(|p| p.name == "Old style")
        .map(|p| p.encryption);
    st.set_dialog(Dialog::None);
    if saved.is_empty() {
        st.ui_mut().values.remove("security.policies");
    } else {
        st.set_value("security.policies", saved);
    }
    pause(300).await;
    cx.record(
        "Password Security offers AES-256 by default, policies carry their encryption (an older one loads as AES-256), AES-128 refuses characters it cannot keep and the policy editor sets the encryption",
        default == "aes-256"
            && legacy == "aes-128"
            && old == "aes-256"
            && owner_hint == 1
            && warned
            && still_open
            && chips_before == 1
            && chips_after == 2
            && edited == Some(EncryptionProfile::Aes128),
        format!(
            "default {default:?}, legacy {legacy:?}, old {old:?}, owner hint {owner_hint}, warned {warned}, still open {still_open}, chips {chips_before} -> {chips_after}, edited {edited:?}"
        ),
    );
}

/// Step 2: an AES-256 copy; a wrong password asks again, the right one opens it.
async fn open_protected_copy(cx: &mut Ctx<'_>, dir: &std::path::Path) -> Option<u64> {
    let st = cx.st;
    let path = dir.join("protected-aes256.pdf");
    let perms = SecurityPermissions {
        print: false,
        ..SecurityPermissions::default()
    };
    let written = st.read_active(|t| {
        t.doc
            .save_encrypted(&path, USER, "owner-pw", perms, EncryptionProfile::Aes256)
    });
    if !matches!(written, Some(Ok(()))) {
        cx.record("protected copy written", false, format!("{written:?}"));
        return None;
    }
    crate::files::open_path(st, path.clone(), None);
    let first = password_dialog(st).await;
    type_in(".dialog .password-input", "wrong").await;
    click(".dialog-foot .tbtn.primary").await;
    let second = password_dialog(st).await;
    let wrong_shown = count(".dialog .password-wrong").await;
    type_in(".dialog .password-input", USER).await;
    click(".dialog-foot .tbtn.primary").await;
    pause(800).await;
    let opened = st.read_active(|t| {
        (
            t.id,
            t.doc.path() == Some(path.as_path()),
            t.doc.is_protected(),
            t.doc.encryption().map(|e| e.method),
        )
    });
    st.set_dialog(Dialog::DocProps);
    pause(400).await;
    let props = eval("return document.querySelector('.dialog .props-encryption')?.textContent || '';")
        .await
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default();
    st.set_dialog(Dialog::None);
    let ok = matches!(first, Some((PasswordTarget::File(_), false)))
        && matches!(second, Some((PasswordTarget::File(_), true)))
        && wrong_shown == 1
        && opened
            .as_ref()
            .is_some_and(|(_, at, prot, m)| *at && *prot && m.as_deref() == Some("AES-256"))
        && props.contains("AES-256");
    cx.record(
        "an AES-256 copy asks again after a wrong password and opens protected with the right one",
        ok,
        format!("first {first:?}, second {second:?}, wrong shown {wrong_shown}, opened {opened:?}, properties {props:?}"),
    );
    opened.filter(|_| ok).map(|(id, ..)| id)
}

/// Steps 3 and 4: signed and time-stamped, encrypted files in place of the
/// document, no password asked.
async fn sign_and_stamp(cx: &mut Ctx<'_>, dir: &std::path::Path, tab: u64) {
    let st = cx.st;
    let signer = DigitalId::create_self_signed(&identity("Self Test Protected Signer"), KeyKind::EcP256, 1);
    let authority = DigitalId::create_time_stamp_authority(
        &identity("Self Test Protected Authority"),
        KeyKind::EcP256,
        1,
    );
    let (Ok(signer), Ok(authority)) = (signer, authority) else {
        cx.record("signer and authority created", false, String::new());
        return;
    };
    let tabs = st.docs.peek().len();
    let opts = SignOptions {
        reason: "Self-test of a protected document".into(),
        ..SignOptions::default()
    };
    let signed = crate::commands::signatures::signed_bytes(st, signer.clone(), opts, None).await;
    let Ok(signed) = signed else {
        cx.record(
            "protected document signed",
            false,
            signed.err().unwrap_or_default(),
        );
        return;
    };
    let encrypted = contains(&signed.bytes, b"/Encrypt")
        && !contains(&signed.bytes, b"Self-test of a protected document")
        && signed.is_protected();
    let signed_path = dir.join("protected-signed.pdf");
    crate::commands::signatures::write_signed(st, tab, signed, signed_path.clone(), "toast.signed");
    pause(800).await;
    let in_place = st.read_active(|t| {
        (
            t.id,
            t.doc.path() == Some(signed_path.as_path()),
            t.doc.is_dirty(),
        )
    });
    let no_prompt = matches!((st.dialog)(), Dialog::None);
    let trusted = [signer.certificate.clone(), authority.certificate.clone()];
    let status = st
        .read_active(|t| t.doc.validate_signatures(&trusted))
        .unwrap_or_default();
    let valid = status.len() == 1 && status[0].valid && status[0].covers_document && status[0].trusted;
    cx.record(
        "a protected document signed: the file is encrypted and takes its place without asking for the password",
        encrypted
            && in_place == Some((tab, true, false))
            && no_prompt
            && st.docs.peek().len() == tabs
            && valid,
        format!(
            "encrypted {encrypted}, in place {in_place:?}, no prompt {no_prompt}, tabs {tabs} -> {}, status {:?}",
            st.docs.peek().len(),
            status.iter().map(|s| (s.valid, s.covers_document, s.trusted, s.problems.clone())).collect::<Vec<_>>()
        ),
    );
    // A document time stamp from a time stamp server on this computer.
    let Some(url) = local_server(authority.clone()) else {
        cx.record("time stamp server on this computer", false, String::new());
        return;
    };
    let stamped = crate::commands::signatures::time_stamped_bytes(st, url).await;
    let Ok(stamped) = stamped else {
        cx.record(
            "protected document time-stamped",
            false,
            stamped.err().unwrap_or_default(),
        );
        return;
    };
    let stamped_encrypted = contains(&stamped.bytes, b"/Encrypt");
    let reopened = Document::from_bytes(stamped.bytes.clone(), None, Some(USER))
        .map(|d| d.validate_signatures(&trusted))
        .unwrap_or_default();
    let stamped_path = dir.join("protected-stamped.pdf");
    crate::commands::signatures::write_signed(st, tab, stamped, stamped_path, "toast.time_stamped");
    pause(800).await;
    let sig = reopened.iter().find(|s| !s.document_time_stamp);
    let ok = stamped_encrypted
        && reopened.len() == 2
        && reopened.iter().all(|s| s.valid)
        && sig.is_some_and(|s| s.later_revisions_time_stamped);
    cx.record(
        "a protected document time-stamped: still encrypted, both entries valid after opening with the password",
        ok,
        format!(
            "encrypted {stamped_encrypted}, entries {:?}",
            reopened
                .iter()
                .map(|s| (s.document_time_stamp, s.valid, s.later_revisions_time_stamped, s.problems.clone()))
                .collect::<Vec<_>>()
        ),
    );
}

/// Step 5: the details of the signature, Revert and the signed revision ask
/// for the password, and no plain traces are left.
async fn revert_revision_and_traces(cx: &mut Ctx<'_>, tab: u64) {
    let st = cx.st;
    let field = st
        .read_active(|t| t.doc.validate_signatures(&[]))
        .unwrap_or_default()
        .into_iter()
        .find(|s| !s.document_time_stamp)
        .map(|s| s.field)
        .unwrap_or_default();
    st.set_dialog(Dialog::SignatureDetails(field.clone()));
    let mut rows = 0;
    for _ in 0..20 {
        pause(150).await;
        rows = count(".dialog .sig-check").await;
        if rows == 4 {
            break;
        }
    }
    let _ = capture_window(&cx.dir.join("interaction-protection.png"));
    st.set_dialog(Dialog::None);
    // Revert: the file is protected, its password is asked; the tab stays.
    crate::files::revert(st);
    let revert_asks = password_dialog(st).await;
    type_in(".dialog .password-input", USER).await;
    click(".dialog-foot .tbtn.primary").await;
    pause(800).await;
    let reverted = st.read_active(|t| (t.id, t.doc.is_protected(), t.doc.is_dirty()));
    // The signed revision: protected like the document.
    let tabs = st.docs.peek().len();
    crate::commands::signatures::open_signed_revision(st, &field);
    let revision_asks = password_dialog(st).await;
    type_in(".dialog .password-input", USER).await;
    click(".dialog-foot .tbtn.primary").await;
    pause(800).await;
    let revision = st.read_active(|t| (t.id, t.doc.is_protected()));
    let revision_opened = st.docs.peek().len() == tabs + 1 && revision.is_some_and(|(id, p)| id != tab && p);
    if let Some((id, _)) = revision.filter(|_| revision_opened) {
        crate::files::close_tab(st, id, true, AfterUnsaved::Close);
        pause(300).await;
    }
    let ok = rows == 4
        && matches!(revert_asks, Some((PasswordTarget::Replace { tab: t, .. }, false)) if t == tab)
        && reverted == Some((tab, true, false))
        && matches!(revision_asks, Some((PasswordTarget::Revision { .. }, false)))
        && revision_opened;
    cx.record(
        "the details of a protected signature; Revert and the signed revision ask for the password",
        ok,
        format!("{rows} check(s), revert {revert_asks:?} -> {reverted:?}, revision {revision_asks:?} -> {revision:?}"),
    );
    // A dirty protected document has no recovery copy; closed, no thumbnail.
    if let Some(idx) = st.docs.peek().iter().position(|t| t.id == tab) {
        st.set_active(idx);
    }
    let path = st
        .read_active(|t| t.doc.path().map(|p| p.to_path_buf()))
        .flatten();
    let protection = st.read_active(|t| t.doc.protection().cloned()).flatten();
    let _ = st.with_active(|t| {
        t.doc
            .add_note(0, 40.0, 40.0, "protected note", "Reviewer", [1.0, 0.9, 0.2])
    });
    // A PDF embedded in the protected document, opened in a tab of its own
    // (as from the attachments or a portfolio), is protected too.
    let embedded = Document::new_blank(1, 200.0, 200.0)
        .map(|d| d.bytes().to_vec())
        .unwrap_or_default();
    let opened = crate::files::open_embedded_pdf(st, embedded, "embedded.pdf", protection);
    pause(500).await;
    let embedded_tab = st.read_active(|t| (t.id, t.doc.is_protected()));
    let _ = st.with_active(|t| {
        t.doc
            .add_note(0, 20.0, 20.0, "embedded note", "Reviewer", [1.0, 0.9, 0.2])
    });
    // The private folder of decrypted files (user only).
    let private = crate::util::protected_temp_dir().ok();
    #[cfg(unix)]
    let private_mode = private.as_deref().and_then(|d| {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(d).ok().map(|m| m.permissions().mode() & 0o777)
    });
    #[cfg(not(unix))]
    let private_mode = Some(0o700);
    crate::recovery::checkpoint(st);
    pause(1500).await;
    let copy_of = |id: u64| {
        crate::recovery::folder()
            .map(|d| d.join(format!("{}-{id}.pdf", crate::recovery::session())))
            .is_some_and(|p| p.exists())
    };
    let copy = copy_of(tab);
    let embedded_copy = embedded_tab.is_some_and(|(id, _)| copy_of(id));
    let embedded_ok = opened && embedded_tab.is_some_and(|(id, p)| id != tab && p) && !embedded_copy;
    if let Some((id, _)) = embedded_tab.filter(|(id, _)| *id != tab) {
        crate::files::close_tab(st, id, true, AfterUnsaved::Close);
        pause(300).await;
    }
    if let Some(idx) = st.docs.peek().iter().position(|t| t.id == tab) {
        st.set_active(idx);
    }
    let dirty = st.read_active(|t| t.doc.is_dirty()).unwrap_or(false);
    crate::files::close_tab(st, tab, true, AfterUnsaved::Close);
    pause(500).await;
    // No protected document is open any more: the private folder is gone.
    let private_gone = private.as_deref().is_some_and(|d| !d.exists())
        || st.docs.peek().iter().any(|t| t.doc.is_protected());
    let thumb = path
        .as_deref()
        .and_then(crate::commands::history::thumb_path)
        .is_some_and(|p| p.exists());
    let title_kept = path.as_deref().is_some_and(|p| {
        st.ui
            .peek()
            .history
            .iter()
            .any(|h| h.path == p && !h.title.is_empty())
    });
    cx.record(
        "a protected document and a PDF embedded in it leave no recovery copy, no history thumbnail and no title in the history; decrypted files go to a private folder removed with the last protected document",
        dirty && !copy && !thumb && !title_kept && embedded_ok && private_mode == Some(0o700) && private_gone,
        format!(
            "dirty {dirty}, recovery copy {copy}, thumbnail {thumb}, title kept {title_kept}, embedded {embedded_tab:?} opened {opened} copy {embedded_copy}, private folder mode {:?} gone {private_gone}",
            private_mode.map(|m| format!("{m:o}"))
        ),
    );
}
