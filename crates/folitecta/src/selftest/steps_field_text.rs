//! Interaction step: field_text — a field filled on the page with letters
//! beyond WinAnsi keeps them; text the embedded font cannot draw is refused,
//! named, and stays in the editor to be corrected.

use super::*;

async fn text(js: &str) -> String {
    eval(js)
        .await
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default()
}

async fn type_value(value: &str) {
    let _ =
        eval("const h = [...document.querySelectorAll('#pg-0 .field-hit')]; if (h[0]) h[0].click();").await;
    pause(500).await;
    let _ = eval(&format!(
        "const i = document.querySelector('#pg-0 .field-input'); if (i) {{ i.value = {}; i.dispatchEvent(new Event('input', {{ bubbles: true }})); i.dispatchEvent(new KeyboardEvent('keydown', {{ key: 'Enter', bubbles: true }})); }}",
        serde_json::to_string(value).unwrap_or_default()
    ))
    .await;
    pause(600).await;
}

pub(super) async fn run(cx: &mut Ctx<'_>) {
    let st = cx.st;
    let Ok(doc) = folitecta_core::Document::from_bytes(folitecta_core::testdocs::form_doc(1, 1), None, None)
    else {
        cx.record("field text: the form", false, String::new());
        return;
    };
    crate::files::open_document_tab(st, doc);
    let tab = st.read_active(|t| t.id);
    st.set_tool(Tool::Hand);
    st.go_to_page(0);
    pause(800).await;

    let value = || {
        st.read_active(|t| t.doc.form_fields()[0].value.clone())
            .unwrap_or_default()
    };
    type_value("Łódź Ελληνικά").await;
    let kept = value();
    type_value("漢字").await;
    let after = value();
    let draft = text("return document.querySelector('#pg-0 .field-input')?.value || '';").await;
    let said = st.toasts.peek().iter().any(|t| t.message.contains('漢'));
    let _ = eval("const i = document.querySelector('#pg-0 .field-input'); if (i) i.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));").await;
    pause(300).await;
    cx.record(
        "field text: letters beyond WinAnsi are kept, text that cannot be drawn is refused and stays in the editor",
        kept == "Łódź Ελληνικά" && after == kept && draft == "漢字" && said,
        format!("kept {kept:?}, after the refusal {after:?}, draft {draft:?}, named {said}"),
    );
    if let Some(id) = tab {
        crate::files::close_tab(st, id, true, crate::state::AfterUnsaved::Close);
        pause(300).await;
    }
}
