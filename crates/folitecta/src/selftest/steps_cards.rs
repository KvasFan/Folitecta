//! Digital IDs on cards and tokens, with the stand-in card of the self-test
//! (on for the whole run: never the devices or stores of the system): listed in Digital IDs,
//! offered in the signing dialog with a PIN field, a wrong PIN refused
//! there, and the document signed with the card's key and valid.

use folitecta_core::digsig::SignOptions;

use super::*;

pub(super) async fn run(cx: &mut Ctx<'_>) {
    let st = cx.st;
    crate::commands::signatures::look_outside(st);
    st.set_dialog(Dialog::DigitalIds);
    let mut listed = false;
    for _ in 0..20 {
        pause(300).await;
        listed = eval("return [...document.querySelectorAll('.outside-ids .list-item')].some(i => i.textContent.includes('Folitecta Test Card'));")
            .await
            .ok()
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if listed {
            break;
        }
    }
    let _ = capture_window(&cx.dir.join("interaction-cards-ids.png"));
    st.set_dialog(Dialog::None);
    pause(300).await;

    // The signing dialog: the card chosen, a PIN asked, a wrong one refused.
    st.set_dialog(Dialog::Sign {
        certify: 0,
        invisible: true,
        field: None,
    });
    pause(600).await;
    let _ = eval(
        "const s = [...document.querySelectorAll('.dialog select')].find(s => [...s.options].some(o => o.value.startsWith('standin:'))); if (s) { s.value = [...s.options].find(o => o.value.startsWith('standin:')).value; s.dispatchEvent(new Event('change', { bubbles: true })); }",
    )
    .await;
    pause(400).await;
    let pin_field = eval("return !!document.querySelector('.dialog .sig-pin');")
        .await
        .ok()
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let _ = eval(
        "const p = document.querySelector('.dialog .sig-pin'); if (p) { p.value = '0000'; p.dispatchEvent(new Event('input', { bubbles: true })); }",
    )
    .await;
    pause(300).await;
    let _ = eval("document.querySelector('.dialog-foot .tbtn.primary')?.click();").await;
    let mut refused = String::new();
    for _ in 0..20 {
        pause(300).await;
        refused = eval("return document.querySelector('.dialog .callout.error')?.textContent || '';")
            .await
            .ok()
            .and_then(|v| v.as_str().map(String::from))
            .unwrap_or_default();
        if !refused.is_empty() {
            break;
        }
    }
    let _ = capture_window(&cx.dir.join("interaction-cards-sign.png"));
    let still_open = matches!((st.dialog)(), Dialog::Sign { .. });
    st.set_dialog(Dialog::None);

    // The right PIN: the document signed by the card, valid.
    let card = crate::commands::signatures::OUTSIDE
        .peek()
        .as_ref()
        .and_then(|f| f.ids.first().cloned());
    let mut detail = String::new();
    let mut valid = false;
    if let Some(card) = card {
        match crate::commands::signatures::unlock_outside(card.clone(), "1234".into()).await {
            Ok(id) => {
                let opts = SignOptions {
                    reason: "Self-test of a card".into(),
                    ..SignOptions::default()
                };
                let external = id.is_external();
                match crate::commands::signatures::signed_bytes(st, id.clone(), opts, None).await {
                    Ok(signed) => {
                        let trusted = [id.certificate.clone()];
                        let status = folitecta_core::Document::from_bytes(signed.bytes.clone(), None, None)
                            .map(|d| d.validate_signatures(&trusted))
                            .unwrap_or_default();
                        valid = external && status.len() == 1 && status[0].valid && status[0].trusted;
                        detail = format!(
                            "external {external}, status {:?}",
                            status
                                .iter()
                                .map(|s| (s.valid, s.trusted, s.problems.clone()))
                                .collect::<Vec<_>>()
                        );
                    }
                    Err(e) => detail = e,
                }
            }
            Err(e) => detail = e,
        }
    }
    *crate::commands::signatures::OUTSIDE.write() = None;
    cx.record(
        "cards and tokens: the card listed, its PIN asked when signing, a wrong PIN refused, the document signed by the card",
        listed && pin_field && refused.contains("wrong PIN") && still_open && valid,
        format!("listed {listed}, PIN field {pin_field}, refused {refused:?}, still open {still_open}, {detail}"),
    );
}
