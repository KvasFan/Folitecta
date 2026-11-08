//! Push buttons with pictures for their states and a highlight mode.

use folitecta_core::{ButtonBehaviour, ButtonState, Document, FieldProps, NewField, Renderer, TextLayout};

/// A small picture (a page of text drawn to PNG).
fn picture() -> Vec<u8> {
    let doc = Document::from_text("Go", TextLayout::default()).unwrap();
    Renderer::new(doc.bytes())
        .unwrap()
        .render_page(0, 0.2)
        .unwrap()
        .png
}

/// The content of an appearance state of the button's widget.
fn appearance(doc: &Document, field: lopdf::ObjectId, state: &[u8]) -> Option<String> {
    let inner = doc.inner();
    let wid = *doc.field_widgets(field).first()?;
    let ap = inner.get_dictionary(wid).ok()?.get(b"AP").ok()?.as_dict().ok()?;
    let id = ap.get(state).ok()?.as_reference().ok()?;
    let s = inner.get_object(id).ok()?.as_stream().ok()?;
    Some(String::from_utf8_lossy(&s.decompressed_content().unwrap_or_else(|_| s.content.clone())).to_string())
}

#[test]
fn pictures_of_the_states_and_the_highlight_mode() {
    let mut doc = Document::new_blank(1, 400.0, 300.0).unwrap();
    let props = FieldProps {
        name: "go".into(),
        caption: "Go".into(),
        ..FieldProps::default()
    };
    let button = doc
        .add_form_field(0, NewField::PushButton, [40.0, 40.0, 160.0, 100.0], &props)
        .unwrap();
    doc.clear_history();
    assert_eq!(doc.button_icons(button), [false; 3]);
    assert!(!appearance(&doc, button, b"N").unwrap().contains("FoliIcon"));

    doc.set_button_icon(button, ButtonState::Up, Some(picture()))
        .unwrap();
    assert_eq!(doc.button_icons(button), [true, false, false]);
    let normal = appearance(&doc, button, b"N").unwrap();
    // The picture, with the caption under it.
    assert!(
        normal.contains("/FoliIcon Do") && normal.contains("(Go) Tj"),
        "{normal}"
    );
    // The pressed state shows the normal picture too.
    assert!(appearance(&doc, button, b"D").unwrap().contains("/FoliIcon Do"));
    assert!(appearance(&doc, button, b"R").is_none());

    doc.set_button_icon(button, ButtonState::Rollover, Some(picture()))
        .unwrap();
    assert!(appearance(&doc, button, b"R").unwrap().contains("/FoliIcon Do"));
    doc.set_button_icon(button, ButtonState::Up, None).unwrap();
    assert_eq!(doc.button_icons(button), [false, false, true]);

    assert_eq!(doc.button_behaviour(button), ButtonBehaviour::Invert);
    doc.set_button_behaviour(button, ButtonBehaviour::Push).unwrap();
    assert_eq!(doc.button_behaviour(button), ButtonBehaviour::Push);
    // Each change is one undo step.
    doc.undo().unwrap();
    doc.undo().unwrap();
    assert_eq!(doc.button_icons(button), [true, false, true]);

    // Only push buttons take pictures.
    let text = doc
        .add_form_field(
            0,
            NewField::Text,
            [40.0, 150.0, 160.0, 170.0],
            &FieldProps::default(),
        )
        .unwrap();
    assert!(
        doc.set_button_icon(text, ButtonState::Up, Some(picture()))
            .is_err()
    );
}

#[test]
fn a_picture_field_asks_for_its_picture_and_shows_it_alone() {
    let mut doc = Document::new_blank(1, 400.0, 300.0).unwrap();
    let field = doc
        .add_form_field(
            0,
            NewField::Picture,
            [40.0, 40.0, 200.0, 160.0],
            &FieldProps::default(),
        )
        .unwrap();
    let plain = doc
        .add_form_field(
            0,
            NewField::PushButton,
            [220.0, 40.0, 300.0, 70.0],
            &FieldProps::default(),
        )
        .unwrap();
    let f = doc.form_fields().into_iter().find(|f| f.id == field).unwrap();
    assert_eq!(f.name, "Picture1");
    assert_eq!(f.kind, folitecta_core::FieldKind::PushButton);
    assert!(doc.is_picture_field(field) && !doc.is_picture_field(plain));
    // Its script, as other programs write it, asks for the picture.
    let actions = doc.button_actions(field);
    let Some(folitecta_core::ButtonAction::Script(code)) = actions.first() else {
        panic!("{actions:?}");
    };
    assert_eq!(code, folitecta_core::form_design::PICTURE_SCRIPT);
    let ctx = serde_json::json!({ "fields": { "Picture1": { "type": "button", "value": "" } } });
    let event = serde_json::json!({ "target": "Picture1", "name": "Mouse Up" });
    let run = folitecta_core::js::run_script(code, &ctx, Some(&event));
    assert!(run.ok, "{run:?}");
    assert_eq!(
        run.ops,
        vec![serde_json::json!({ "op": "importIcon", "name": "Picture1" })]
    );

    // The picture given, alone: no caption under it.
    doc.set_button_icon(field, ButtonState::Up, Some(picture()))
        .unwrap();
    let normal = appearance(&doc, field, b"N").unwrap();
    assert!(
        normal.contains("/FoliIcon Do") && !normal.contains(" Tj"),
        "{normal}"
    );
    // Saved and opened again, it is still a picture field with its picture.
    let again = Document::from_bytes(doc.bytes().as_ref().clone(), None, None).unwrap();
    assert!(again.is_picture_field(field));
    assert_eq!(again.button_icons(field), [true, false, false]);
}
