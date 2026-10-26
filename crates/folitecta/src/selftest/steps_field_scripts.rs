//! Field scripts: a calculate and a format script typed in the field
//! properties and applied; the field calculated and shown with them, the
//! value itself kept; a validate script refuses a value.

use folitecta_core::{FieldProps, NewField};

use super::*;

/// Type `code` into the script box and apply it.
async fn apply_script(code: &str) {
    let code = serde_json::to_string(code).unwrap_or_default();
    let _ = eval(&format!(
        "const b = document.querySelector('.script-box'); if (b) {{ b.value = {code}; b.dispatchEvent(new Event('input', {{ bubbles: true }})); }}"
    ))
    .await;
    pause(400).await;
    let _ = eval("document.querySelector('.script-apply')?.click();").await;
    pause(500).await;
}

/// Choose the script of `trigger` in the scripts' list.
async fn choose(trigger: &str) {
    let _ = eval(&format!(
        "const s = [...document.querySelectorAll('select')].find(s => [...s.options].some(o => o.value === 'F')); if (s) {{ s.value = '{trigger}'; s.dispatchEvent(new Event('change', {{ bubbles: true }})); }}"
    ))
    .await;
    pause(400).await;
}

pub(super) async fn run(cx: &mut Ctx<'_>) {
    let st = cx.st;
    let page = st
        .read_active(|t| t.doc.page_count().saturating_sub(1))
        .unwrap_or(0);
    let made = st.with_active(|t| {
        let mut add = |name: &str, y: f32| {
            let props = FieldProps {
                name: name.into(),
                ..FieldProps::default()
            };
            t.doc
                .add_form_field(page, NewField::Text, [260.0, y, 420.0, y + 22.0], &props)
        };
        let ids = (add("snet", 700.0), add("sgross", 670.0));
        t.refresh();
        ids
    });
    let Some((Ok(net), Ok(gross))) = made else {
        cx.record("field scripts", false, "the fields could not be made".into());
        return;
    };
    let selected = st
        .with_active(|t| {
            let widget = *t.doc.field_widgets(gross).first()?;
            let index = t
                .doc
                .annotations(page)
                .ok()?
                .into_iter()
                .find(|x| x.id == Some(widget))?
                .index;
            t.selected_annot = Some((page, index));
            Some(())
        })
        .flatten()
        .is_some();
    st.ui_mut().show_right_pane = true;
    st.go_to_page(page);
    pause(600).await;
    apply_script("event.value = (Number(this.getField('snet').value) * 1.22).toFixed(2);").await;
    choose("F").await;
    apply_script("AFNumber_Format(2, 0, 0, 0, '', false);").await;
    choose("V").await;
    apply_script("if (Number(event.value) < 0) { app.alert('Not below zero'); event.rc = false; }").await;
    // Back to the calculation: its script is shown again.
    choose("C").await;
    let shown = eval("return document.querySelector('.script-box')?.value || '';")
        .await
        .ok()
        .and_then(|v| v.as_str().map(String::from))
        .unwrap_or_default();
    st.with_active(|t| {
        let _ = t.doc.set_field_value(net, "1000");
        t.refresh();
    });
    pause(500).await;
    let _ = capture_window(&cx.dir.join("interaction-field-scripts.png"));
    let (value, calculated, refused) = st
        .with_active(|t| {
            let value = t
                .doc
                .form_fields()
                .into_iter()
                .find(|f| f.name == "sgross")
                .map(|f| f.value)
                .unwrap_or_default();
            let calculated = t.doc.calculation_order().contains(&gross);
            let refused = t
                .doc
                .set_field_value(gross, "-5")
                .err()
                .map(|e| e.to_string())
                .unwrap_or_default();
            (value, calculated, refused)
        })
        .unwrap_or_default();
    let formatted = st
        .read_active(|t| t.doc.run_field_script(gross, b"F", &value))
        .flatten()
        .and_then(|r| r.event_value)
        .unwrap_or_default();
    st.with_active(|t| t.selected_annot = None);
    cx.record(
        "field scripts: calculate, format and validate scripts typed in the properties; calculated, shown formatted, a value refused",
        selected
            && shown.contains("1.22")
            && value == "1220.00"
            && calculated
            && formatted == "1,220.00"
            && refused.contains("Not below zero"),
        format!(
            "selected {selected}, shown {shown:?}, value {value:?}, calculated {calculated}, formatted {formatted:?}, refused {refused:?}"
        ),
    );
}
