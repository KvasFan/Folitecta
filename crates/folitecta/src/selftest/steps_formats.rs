//! Interaction step: formats — the Date tool makes a date field in the
//! format of the language; filling it with a date that does not exist is
//! refused with the format named and the editor keeps what was typed; a day
//! picked on its calendar is written in the field's format; the properties
//! pane turns another field into a number field with a range.

use folitecta_core::{FieldFormat, FieldProps, NewField};

use super::*;

async fn text(js: &str) -> String {
    eval(js)
        .await
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default()
}

pub(super) async fn run(cx: &mut Ctx<'_>) {
    let st = cx.st;
    let Ok(doc) = folitecta_core::Document::new_blank(1, 595.0, 842.0) else {
        cx.record("formats", false, "the document could not be made".into());
        return;
    };
    crate::files::open_document_tab(st, doc);
    let tab = st.read_active(|t| t.id).unwrap_or(0);
    pause(700).await;

    // The Date tool, and the field it makes (as a drag would).
    crate::commands::dispatch(st, "tool.editFields.mode.date");
    pause(200).await;
    let tool = *st.tool.peek();
    let tool_makes_dates = tool == Tool::FieldDate && tool.new_field() == Some(NewField::Date);
    let made = st.with_active(|t| {
        let props = FieldProps {
            format: FieldFormat::Date(crate::i18n::t(st.lang(), "field.date_format_default")),
            ..FieldProps::default()
        };
        let date = t
            .doc
            .add_form_field(0, NewField::Date, [100.0, 700.0, 220.0, 722.0], &props)?;
        let amount = t.doc.add_form_field(
            0,
            NewField::Text,
            [100.0, 640.0, 260.0, 662.0],
            &FieldProps::default(),
        )?;
        Ok::<_, folitecta_core::Error>((date, amount))
    });
    let Some(Ok((date, amount))) = made else {
        cx.record(
            "formats",
            false,
            format!("the fields could not be made: {made:?}"),
        );
        return;
    };
    let format = st.read_active(|t| t.doc.field_format(date)).unwrap_or_default();
    crate::commands::dispatch(st, "tool.hand");
    pause(600).await;

    // A date that does not exist.
    let _ =
        eval("const h = [...document.querySelectorAll('#pg-0 .field-hit')]; if (h[0]) h[0].click();").await;
    pause(500).await;
    let _ = eval("const i = document.querySelector('#pg-0 .field-input'); if (i) { i.value = '02/30/2026'; i.dispatchEvent(new Event('input', { bubbles: true })); i.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true })); }").await;
    pause(600).await;
    let kept = text("return document.querySelector('#pg-0 .field-input')?.value || '';").await;
    let refused = st
        .read_active(|t| t.doc.form_fields()[0].value.is_empty())
        .unwrap_or(false);
    let said = st
        .toasts
        .peek()
        .iter()
        .any(|t| t.title.contains("mm/dd/yyyy") || t.message.contains("mm/dd/yyyy"));
    cx.record(
        "formats: a date field refuses a date that does not exist, names its format and keeps what was typed",
        tool_makes_dates
            && format == FieldFormat::Date("mm/dd/yyyy".into())
            && refused
            && said
            && kept == "02/30/2026",
        format!("tool {tool_makes_dates}, format {format:?}, refused {refused}, said {said}, kept {kept:?}"),
    );

    // The calendar: the 15th of the month shown.
    let _ = eval("document.querySelector('#pg-0 .field-calendar-btn')?.click();").await;
    pause(400).await;
    let title = text("return document.querySelector('.date-calendar-title')?.textContent || '';").await;
    let _ = eval("document.querySelector('.date-calendar [data-day=\"15\"]')?.click();").await;
    pause(600).await;
    let value = st
        .read_active(|t| t.doc.form_fields()[0].value.clone())
        .unwrap_or_default();
    let closed = st.editing_field.peek().is_none();
    let picked = value.len() == 10
        && value[3..5] == *"15"
        && folitecta_core::form_format::scan_date("mm/dd/yyyy", &value).is_some();
    cx.record(
        "formats: a day picked on the calendar of a date field is written in its format",
        !title.is_empty() && picked && closed,
        format!("calendar {title:?}, value {value:?}, editor closed {closed}"),
    );

    // The properties pane: a number with two decimals between 0 and 100.
    crate::commands::dispatch(st, "tool.editFields.mode.select");
    let index = st
        .read_active(|t| {
            let w = t.doc.field_widgets(amount)[0];
            t.doc
                .annotations(0)
                .ok()?
                .into_iter()
                .find(|a| a.id == Some(w))
                .map(|a| a.index)
        })
        .flatten();
    st.with_active(|t| t.selected_annot = index.map(|i| (0, i)));
    st.ui_mut().show_right_pane = true;
    pause(700).await;
    let _ = eval("const s = document.querySelector('.field-editor .field-format-kind'); if (s) { s.value = 'number'; s.dispatchEvent(new Event('change', { bubbles: true })); }").await;
    pause(600).await;
    let _ = eval("const s = document.querySelector('.field-editor .field-max'); if (s) { s.value = '100'; s.dispatchEvent(new Event('change', { bubbles: true })); }").await;
    pause(600).await;
    let (chosen, check) = st
        .read_active(|t| (t.doc.field_format(amount), t.doc.field_check(amount)))
        .unwrap_or_default();
    let too_big = st
        .with_active(|t| t.doc.set_field_value(amount, "250").is_err())
        .unwrap_or(false);
    let fine = st
        .with_active(|t| t.doc.set_field_value(amount, "42.5").is_ok())
        .unwrap_or(false);
    st.ui_mut().show_right_pane = false;
    cx.record(
        "formats: the properties pane makes a field a number field with a highest value",
        matches!(chosen, FieldFormat::Number { decimals: 2, .. })
            && check
                == folitecta_core::FieldCheck::Range {
                    min: None,
                    max: Some(100.0),
                }
            && too_big
            && fine,
        format!("format {chosen:?}, check {check:?}, 250 refused {too_big}, 42.5 taken {fine}"),
    );
    crate::files::close_tab(st, tab, true, crate::state::AfterUnsaved::Close);
    pause(300).await;
}
