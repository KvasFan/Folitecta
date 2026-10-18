//! Interaction step 56: security policies — the list starts with two
//! examples; a new policy that forbids printing is kept, cloned and the
//! clone deleted; applied, it opens Password Security with its permissions,
//! where the permissions are saved as another policy.

use crate::commands::policies::policies;

use super::*;

async fn count(selector: &str) -> u64 {
    document::eval(&format!(
        "return document.querySelectorAll({}).length;",
        serde_json::to_string(selector).unwrap_or_default()
    ))
    .await
    .ok()
    .and_then(|v| v.as_u64())
    .unwrap_or(0)
}

async fn click(selector: &str) {
    let _ = document::eval(&format!(
        "document.querySelector({})?.click();",
        serde_json::to_string(selector).unwrap_or_default()
    ))
    .await;
    pause(300).await;
}

async fn type_in(selector: &str, text: &str) {
    let _ = document::eval(&format!(
        "const i = document.querySelector({}); if (i) {{ i.value = {}; i.dispatchEvent(new Event('input', {{ bubbles: true }})); }}",
        serde_json::to_string(selector).unwrap_or_default(),
        serde_json::to_string(text).unwrap_or_default()
    ))
    .await;
    pause(200).await;
}

pub(super) async fn run(cx: &mut Ctx<'_>) {
    let st = cx.st;
    // 56. The examples, then a policy of our own.
    let saved = st.value("security.policies");
    st.ui_mut().values.remove("security.policies");
    crate::commands::dispatch(st, "securityPolicies");
    pause(400).await;
    let examples = count(".dialog .policy-row").await;
    click(".dialog .policy-add").await;
    type_in(".dialog .policy-name-input", "Strict").await;
    // Printing is the first permission.
    click(".dialog .policy-editor .check-grid input[type=checkbox]").await;
    click(".dialog .policy-save").await;
    let kept = count(".dialog .policy-row").await;
    click(".dialog .policy-clone").await;
    click(".dialog .policy-save").await;
    let cloned = policies(st).iter().map(|p| p.name.clone()).collect::<Vec<_>>();
    click(".dialog .policy-delete").await;
    let after_delete = count(".dialog .policy-row").await;
    let _ = capture_window(&cx.dir.join("interaction-policies.png"));
    // Applied: Password Security with the policy chosen and printing off.
    let _ = document::eval(
        "[...document.querySelectorAll('.dialog .policy-row')].find(r => r.textContent.includes('Strict'))?.click();",
    )
    .await;
    pause(200).await;
    click(".dialog-foot .tbtn.primary").await;
    pause(300).await;
    let security = matches!((st.dialog)(), Dialog::Security);
    let chosen = document::eval("return document.querySelector('.dialog .security-policy')?.value || '';")
        .await
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default();
    let print_off = document::eval(
        "const c = document.querySelector('.dialog .check-grid input[type=checkbox]'); return c ? !c.checked : false;",
    )
    .await
    .ok()
    .and_then(|v| v.as_bool())
    .unwrap_or(false);
    let _ = capture_window(&cx.dir.join("interaction-policy-applied.png"));
    type_in(".dialog .security-policy-name", "From the dialog").await;
    click(".dialog .security-save-policy").await;
    let names: Vec<String> = policies(st).into_iter().map(|p| p.name).collect();
    st.set_dialog(Dialog::None);
    if saved.is_empty() {
        st.ui_mut().values.remove("security.policies");
    } else {
        st.set_value("security.policies", saved);
    }
    cx.record(
        "security policies: two examples, one kept that forbids printing, cloned and the clone deleted, applied to Password Security, and its permissions saved again",
        examples == 2
            && kept == 3
            && cloned.iter().any(|n| n == "Strict (2)")
            && after_delete == 3
            && security
            && chosen == "Strict"
            && print_off
            && names.len() == 4
            && names.iter().any(|n| n == "From the dialog"),
        format!(
            "examples {examples}, kept {kept}, cloned {cloned:?}, after delete {after_delete}, security {security}, chosen {chosen:?}, print off {print_off}, names {names:?}"
        ),
    );
}
