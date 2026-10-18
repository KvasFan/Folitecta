//! Interaction step 55: the items of a combo box — Form › Items on the
//! chosen combo box lists its items; one is typed and added with Enter, one
//! moved down, the list kept sorted, and OK writes it to the field.

use crate::commands::field_items::ITEMS;

use super::*;

async fn count_of(selector: &str) -> u64 {
    document::eval(&format!(
        "return document.querySelectorAll({}).length;",
        serde_json::to_string(selector).unwrap_or_default()
    ))
    .await
    .ok()
    .and_then(|v| v.as_u64())
    .unwrap_or(0)
}

pub(super) async fn run(cx: &mut Ctx<'_>) {
    let st = cx.st;
    // 55. A combo box on the last page, chosen with the field tool.
    let page = st
        .read_active(|t| t.doc.page_count().saturating_sub(1))
        .unwrap_or(0);
    let made = st
        .with_active(|t| {
            let props = folitecta_core::FieldProps {
                name: "fruit".into(),
                options: vec!["Pear".into(), "Apple".into()],
                ..Default::default()
            };
            let id = t
                .doc
                .add_form_field(
                    page,
                    folitecta_core::NewField::ComboBox,
                    [60.0, 60.0, 220.0, 82.0],
                    &props,
                )
                .ok()?;
            t.refresh();
            let wid = *t.doc.field_widgets(id).first()?;
            let index = t
                .doc
                .annotations(page)
                .ok()?
                .iter()
                .position(|a| a.id == Some(wid))?;
            t.selected_annot = Some((page, index));
            Some(id)
        })
        .flatten();
    let Some(field) = made else {
        cx.record("field items", false, "the combo box could not be made".into());
        return;
    };
    st.set_tool(Tool::EditFields);
    crate::commands::dispatch(st, "combobox.items");
    pause(400).await;
    let listed = document::eval("return document.querySelectorAll('.dialog .field-item').length;")
        .await
        .ok()
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let _ = document::eval(
        "const i = document.querySelector('.dialog .field-items-draft'); if (i) { i.value = 'Cherry'; i.dispatchEvent(new Event('input', { bubbles: true })); }",
    )
    .await;
    pause(200).await;
    let _ = document::eval(
        "document.querySelector('.dialog .field-items-draft')?.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));",
    )
    .await;
    pause(300).await;
    let added = ITEMS.peek().as_ref().map(|e| e.items.clone()).unwrap_or_default();
    // The first one moved down, then the list sorted.
    let _ = document::eval("document.querySelector('.dialog .field-item')?.click();").await;
    pause(200).await;
    let _ = document::eval("document.querySelector('.dialog .field-items-down')?.click();").await;
    pause(200).await;
    let moved = ITEMS.peek().as_ref().map(|e| e.items.clone()).unwrap_or_default();
    let _ = document::eval("document.querySelector('.dialog .field-items-sorted')?.click();").await;
    pause(300).await;
    let _ = capture_window(&cx.dir.join("interaction-field-items.png"));
    let _ = document::eval("document.querySelector('.dialog-foot .tbtn.primary')?.click();").await;
    pause(500).await;
    let props = st.read_active(|t| t.doc.field_props(field).ok()).flatten();
    let closed = matches!((st.dialog)(), Dialog::None);

    // A push button with a picture: its pane shows the pictures, and the
    // behaviour chosen there is written to it.
    let button = st
        .with_active(|t| {
            let props = folitecta_core::FieldProps {
                name: "go".into(),
                caption: "Go".into(),
                ..Default::default()
            };
            let id = t
                .doc
                .add_form_field(
                    page,
                    folitecta_core::NewField::PushButton,
                    [260.0, 60.0, 380.0, 110.0],
                    &props,
                )
                .ok()?;
            let png = folitecta_core::Renderer::new(t.doc.bytes())
                .ok()?
                .render_page(0, 0.1)
                .ok()?
                .png;
            t.doc
                .set_button_icon(id, folitecta_core::ButtonState::Up, Some(png))
                .ok()?;
            t.refresh();
            let wid = *t.doc.field_widgets(id).first()?;
            let index = t
                .doc
                .annotations(page)
                .ok()?
                .iter()
                .position(|a| a.id == Some(wid))?;
            t.selected_annot = Some((page, index));
            Some(id)
        })
        .flatten();
    crate::commands::dispatch(st, "btn.behaviour");
    pause(500).await;
    let pictures = count_of(".button-picture").await;
    let removable = count_of(".button-picture .tbtn.danger").await;
    let _ = document::eval(
        "const s = document.querySelector('.button-behaviour'); if (s) { s.value = 'push'; s.dispatchEvent(new Event('change', { bubbles: true })); }",
    )
    .await;
    pause(400).await;
    let _ = capture_window(&cx.dir.join("interaction-button-pictures.png"));
    let behaviour = button.and_then(|b| st.read_active(|t| t.doc.button_behaviour(b)));
    st.ui_mut().show_right_pane = false;
    st.set_tool(Tool::Hand);
    st.with_active(|t| t.selected_annot = None);
    cx.record(
        "push button pictures: the pane lists the three pictures with the one set, and the behaviour chosen is written",
        pictures == 3 && removable == 1 && behaviour == Some(folitecta_core::ButtonBehaviour::Push),
        format!("pictures {pictures}, removable {removable}, behaviour {behaviour:?}"),
    );
    cx.record(
        "field items: a combo box's items listed, one added with Enter, one moved, the list sorted and written",
        listed == 2
            && added == ["Pear", "Apple", "Cherry"]
            && moved == ["Apple", "Pear", "Cherry"]
            && closed
            && props
                .as_ref()
                .is_some_and(|p| p.sorted && p.options == ["Apple", "Cherry", "Pear"]),
        format!("listed {listed}, added {added:?}, moved {moved:?}, closed {closed}, field {:?}", props.map(|p| (p.sorted, p.options))),
    );
}
