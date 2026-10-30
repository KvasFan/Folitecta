//! Interaction step: permissions — a form protected so that only filling in
//! is allowed, opened with the user password: a field is filled; rotating a
//! page is refused, nothing changes, and the owner password is asked with
//! the setting named; a wrong one asks again; the right one lifts the
//! restrictions and the page turns. Copying the page is refused the same way.

use folitecta_core::{Document, EncryptionProfile, Restricted, SecurityPermissions};

use crate::state::PasswordTarget;

use super::*;

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

async fn text(js: &str) -> String {
    eval(js)
        .await
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default()
}

async fn owner_dialog(st: AppState) -> Option<(Restricted, bool)> {
    for _ in 0..20 {
        pause(150).await;
        if let Dialog::Password {
            target: PasswordTarget::Owner { refused, .. },
            retry,
        } = st.dialog.peek().clone()
        {
            return Some((refused, retry));
        }
    }
    None
}

pub(super) async fn run(cx: &mut Ctx<'_>) {
    let st = cx.st;
    let path = cx.dir.join("permissions-fill-only.pdf");
    let only_fill = SecurityPermissions {
        print: true,
        modify: false,
        copy: false,
        annotate: false,
        fill_forms: true,
        assemble: false,
    };
    let written = Document::from_bytes(folitecta_core::testdocs::form_doc(2, 2), None, None)
        .and_then(|d| d.save_encrypted(&path, "user-pw", "owner-pw", only_fill, EncryptionProfile::Aes256));
    if let Err(e) = written {
        cx.record("permissions: the protected form", false, e.to_string());
        return;
    }
    crate::files::open_path(st, path.clone(), Some("user-pw"));
    pause(800).await;
    let Some(tab) = st
        .read_active(|t| {
            t.doc
                .path()
                .map(|p| p == path.as_path())
                .unwrap_or(false)
                .then_some(t.id)
        })
        .flatten()
    else {
        cx.record("permissions: the protected form opens", false, "no tab".into());
        return;
    };

    // Filling in is allowed.
    let field = st
        .read_active(|t| t.doc.form_fields().first().map(|f| f.id))
        .flatten();
    let filled = field
        .and_then(|id| st.with_active(|t| t.doc.set_field_value(id, "Allowed")))
        .is_some_and(|r| r.is_ok());

    // Rotating a page is refused and asks for the owner password.
    let before = st.read_active(|t| t.doc.bytes().to_vec()).unwrap_or_default();
    crate::commands::dispatch(st, "document.rotatePages.cw");
    let asked = owner_dialog(st).await;
    let body = text("return document.querySelector('.dialog .password-body')?.textContent || '';").await;
    let unchanged = st.read_active(|t| t.doc.bytes().to_vec()).unwrap_or_default() == before;
    type_in(".dialog .password-input", "wrong").await;
    click(".dialog-foot .tbtn.primary").await;
    let again = owner_dialog(st).await;
    let still = st.read_active(|t| t.doc.restrictions().len()).unwrap_or(0);
    type_in(".dialog .password-input", "owner-pw").await;
    click(".dialog-foot .tbtn.primary").await;
    pause(400).await;
    let lifted = st
        .read_active(|t| t.doc.restrictions().is_empty())
        .unwrap_or(false);
    crate::commands::dispatch(st, "document.rotatePages.cw");
    pause(400).await;
    let turned = st
        .read_active(|t| t.doc.page_size(t.page).map(|s| s.rotation).unwrap_or(0))
        .unwrap_or(0);
    cx.record(
        "permissions: a page turn the settings forbid asks for the owner password, which lifts them",
        filled
            && asked == Some((Restricted::Assemble, false))
            && !body.is_empty()
            && unchanged
            && again == Some((Restricted::Assemble, true))
            && still == 4
            && lifted
            && turned == 90,
        format!(
            "filled {filled}, asked {asked:?}, body {body:?}, unchanged {unchanged}, again {again:?}, restrictions before the owner password {still}, lifted {lifted}, rotation {turned}"
        ),
    );
    crate::files::close_tab(st, tab, true, crate::state::AfterUnsaved::Close);
    pause(300).await;

    // Copying is refused too, until the owner password.
    crate::files::open_path(st, path.clone(), Some("user-pw"));
    pause(800).await;
    let copy_tab = st.read_active(|t| t.id);
    crate::commands::dispatch(st, "copy");
    let copy_asked = owner_dialog(st).await;
    st.set_dialog(Dialog::None);
    pause(200).await;
    cx.record(
        "permissions: copying a page the settings forbid asks for the owner password",
        copy_asked == Some((Restricted::Copy, false)),
        format!("asked {copy_asked:?}"),
    );
    if let Some(id) = copy_tab {
        crate::files::close_tab(st, id, true, crate::state::AfterUnsaved::Close);
        pause(300).await;
    }
}
