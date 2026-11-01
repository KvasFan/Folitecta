//! Interaction step: field_text — a field filled on the page with letters
//! beyond WinAnsi or in Hebrew keeps them; a value the embedded font cannot
//! draw stays as typed, with a warning that names its characters.

use super::*;

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
    type_value("שלום").await;
    let hebrew = value();
    let quiet = !st.toasts.peek().iter().any(|t| t.message.contains('ש'));
    type_value("漢字").await;
    let after = value();
    let said = st.toasts.peek().iter().any(|t| t.message.contains('漢'));
    cx.record(
        "field text: letters beyond WinAnsi and Hebrew are drawn, a value that cannot be drawn is kept with a warning",
        kept == "Łódź Ελληνικά" && hebrew == "שלום" && quiet && after == "漢字" && said,
        format!("kept {kept:?}, hebrew {hebrew:?}, no warning {quiet}, after {after:?}, warned {said}"),
    );
    if let Some(id) = tab {
        crate::files::close_tab(st, id, true, crate::state::AfterUnsaved::Close);
        pause(300).await;
    }
}
