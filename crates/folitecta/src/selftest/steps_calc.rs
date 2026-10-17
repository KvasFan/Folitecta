//! Interaction step 48: field calculations — a total chosen in the field
//! properties follows the fields it adds up, the calculation order lists it,
//! and the fields of one kind are highlighted on their own.

use folitecta_core::{FieldProps, NewField};

use super::*;

pub(super) async fn run(cx: &mut Ctx<'_>) {
    let st = cx.st;
    // 48. Three text fields on the last page.
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
                .add_form_field(page, NewField::Text, [60.0, y, 220.0, y + 22.0], &props)
        };
        let ids = (add("qa", 700.0), add("qb", 670.0), add("qtotal", 640.0));
        t.refresh();
        ids
    });
    let Some((Ok(a), Ok(b), Ok(total))) = made else {
        cx.record("field calculations", false, "the fields could not be made".into());
        return;
    };
    // The total selected: its properties show its calculation.
    let selected = st
        .with_active(|t| {
            let widget = *t.doc.field_widgets(total).first()?;
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
    let _ = document::eval(
        "const s = [...document.querySelectorAll('select')].find(s => [...s.options].some(o => o.value === 'SUM')); if (s) { s.value = 'SUM'; s.dispatchEvent(new Event('change', { bubbles: true })); }",
    )
    .await;
    pause(500).await;
    for name in ["qa", "qb"] {
        let _ = document::eval(&format!(
            "[...document.querySelectorAll('.calc-fields label')].find(l => l.textContent.trim() === '{name}')?.querySelector('input')?.click();"
        ))
        .await;
        pause(500).await;
    }
    let value = |name: &'static str| {
        st.read_active(move |t| {
            t.doc
                .form_fields()
                .into_iter()
                .find(|f| f.name == name)
                .map(|f| f.value)
                .unwrap_or_default()
        })
        .unwrap_or_default()
    };
    let chosen = st
        .read_active(|t| t.doc.field_calculation(total).map(|c| (c.op, c.fields)))
        .flatten();
    st.with_active(|t| {
        let _ = t.doc.set_field_value(a, "2,5");
        let _ = t.doc.set_field_value(b, "4");
        t.refresh();
    });
    pause(500).await;
    let _ = capture_window(&cx.dir.join("interaction-field-calculation.png"));
    let total_value = value("qtotal");

    // The calculation order lists it.
    crate::commands::dispatch(st, "forms.calcOrder");
    pause(500).await;
    let listed = document::eval(
        "return [...document.querySelectorAll('.dialog .calc-order-row')].filter(r => r.textContent.includes('qtotal')).length;",
    )
    .await
    .ok()
    .and_then(|v| v.as_u64())
    .unwrap_or(0);
    let _ = capture_window(&cx.dir.join("interaction-calc-order.png"));
    st.set_dialog(Dialog::None);

    // Highlighting: check boxes alone, then every field, then none.
    let state = || {
        st.read_active(|t| (t.highlight_fields, t.highlight_kinds.clone()))
            .unwrap_or_default()
    };
    crate::commands::dispatch(st, "forms.highlight.checkBox");
    let only_checks = state() == (true, vec!["checkBox"]);
    crate::commands::dispatch(st, "forms.highlight.all");
    let all = state() == (true, vec![]);
    crate::commands::dispatch(st, "forms.highlight.all");
    let none = !state().0;
    st.with_active(|t| t.selected_annot = None);
    cx.record(
        "field calculations: a sum chosen in the properties follows its fields; fields highlighted by kind",
        selected
            && chosen == Some((folitecta_core::CalcOp::Sum, vec!["qa".to_string(), "qb".to_string()]))
            && total_value == "6.5"
            && listed == 1
            && only_checks
            && all
            && none,
        format!(
            "selected {selected}, calculation {chosen:?}, total {total_value:?}, listed {listed}, highlight {only_checks}/{all}/{none}"
        ),
    );
}
