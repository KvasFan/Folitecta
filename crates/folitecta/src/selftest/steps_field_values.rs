//! Interaction step: field_values — a value too long for its field is kept and
//! said; a table of values goes through the checks of the fields, those refused
//! stay out and are named.

use folitecta_core::form_design::FieldProps;
use folitecta_core::{Document, FieldFormat, FormDataFormat};

use super::*;

pub(super) async fn run(cx: &mut Ctx<'_>) {
    let st = cx.st;
    let Ok(doc) = Document::from_bytes(folitecta_core::testdocs::form_doc(1, 2), None, None) else {
        cx.record("field values: the document", false, String::new());
        return;
    };
    crate::files::open_document_tab(st, doc);
    let tab = st.read_active(|t| t.id);
    st.set_tool(Tool::Hand);
    pause(800).await;

    // Typed on the page, longer than the field.
    let long = "A value far too long for a field two hundred points wide, on and on and on";
    let _ =
        eval("const h = [...document.querySelectorAll('#pg-0 .field-hit')]; if (h[1]) h[1].click();").await;
    pause(400).await;
    let _ = eval(&format!(
        "const i = document.querySelector('#pg-0 .field-input'); if (i) {{ i.value = {}; i.dispatchEvent(new Event('input', {{ bubbles: true }})); i.dispatchEvent(new KeyboardEvent('keydown', {{ key: 'Enter', bubbles: true }})); }}",
        serde_json::to_string(long).unwrap_or_default()
    ))
    .await;
    pause(600).await;
    let kept = st
        .read_active(|t| {
            t.doc
                .form_fields()
                .iter()
                .any(|f| f.name == "p1f2" && f.value == long)
        })
        .unwrap_or(false);
    let told = st
        .toasts
        .peek()
        .iter()
        .any(|m| m.title == crate::i18n::t(st.lang(), "toast.field_overflow"));
    cx.record(
        "field values: a value too long for its field is kept and said",
        kept && told,
        format!("kept {kept}, told {told}"),
    );

    // From a table, into a number field.
    let report = st.with_active(|t| {
        let id = t.doc.form_fields().first().map(|f| f.id)?;
        let props = FieldProps {
            format: FieldFormat::Number {
                decimals: 2,
                separators: 0,
                parentheses: false,
                currency: String::new(),
                before: false,
            },
            ..t.doc.field_props(id).ok()?
        };
        t.doc.set_field_props(id, &props).ok()?;
        t.doc.import_form_data(FormDataFormat::Csv, b"p1f1\nwords\n").ok()
    });
    let refused = report.flatten().map(|r| r.refused.len()).unwrap_or(0);
    let value = st
        .read_active(|t| t.doc.form_fields().first().map(|f| f.value.clone()))
        .flatten()
        .unwrap_or_default();
    cx.record(
        "field values: a word in a number field stays out when it comes from a file",
        refused == 1 && value != "words",
        format!("refused {refused}, value {value:?}"),
    );
    if let Some(id) = tab {
        crate::files::close_tab(st, id, true, crate::state::AfterUnsaved::Close);
        pause(300).await;
    }
}
