//! Form fields: creation, appearances, values, properties and deletion.

use folitecta_core::{Document, FieldKind, FieldProps, NewField};

fn blank() -> Document {
    Document::new_blank(2, 400.0, 400.0).unwrap()
}

fn ap_text(doc: &Document, widget: lopdf::ObjectId) -> String {
    let d = doc.inner().get_dictionary(widget).unwrap();
    let n = d.get(b"AP").unwrap().as_dict().unwrap().get(b"N").unwrap();
    let id = n.as_reference().unwrap();
    String::from_utf8_lossy(&doc.inner().get_object(id).unwrap().as_stream().unwrap().content).to_string()
}

#[test]
fn every_field_kind_can_be_created_and_rendered() {
    let mut doc = blank();
    let kinds = [
        NewField::Text,
        NewField::CheckBox,
        NewField::RadioButton,
        NewField::ComboBox,
        NewField::ListBox,
        NewField::PushButton,
        NewField::Signature,
    ];
    for (i, k) in kinds.iter().enumerate() {
        let y = 360.0 - i as f32 * 45.0;
        let props = FieldProps {
            options: vec!["Red".into(), "Green".into()],
            caption: "Send".into(),
            ..FieldProps::default()
        };
        doc.add_form_field(0, *k, [40.0, y - 30.0, 220.0, y], &props)
            .unwrap();
    }
    let fields = doc.form_fields();
    let got: Vec<FieldKind> = fields.iter().map(|f| f.kind.clone()).collect();
    assert_eq!(
        got,
        [
            FieldKind::Text,
            FieldKind::CheckBox,
            FieldKind::RadioButton,
            FieldKind::ComboBox,
            FieldKind::ListBox,
            FieldKind::PushButton,
            FieldKind::Signature
        ]
    );
    assert!(fields.iter().all(|f| f.page == Some(0)));
    let names: Vec<&str> = fields.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "Text1",
            "Check1",
            "Group1",
            "Combo1",
            "List1",
            "Button1",
            "Signature1"
        ]
    );
    assert!(doc.has_form());
    let _ = doc.render_page_png(0, 1.0).unwrap();
    // Names must be unique.
    let dup = FieldProps {
        name: "Text1".into(),
        ..FieldProps::default()
    };
    assert!(
        doc.add_form_field(1, NewField::Text, [10.0, 10.0, 100.0, 30.0], &dup)
            .is_err()
    );
}

#[test]
fn text_values_are_drawn() {
    let mut doc = blank();
    let id = doc
        .add_form_field(
            0,
            NewField::Text,
            [40.0, 300.0, 240.0, 330.0],
            &FieldProps::default(),
        )
        .unwrap();
    doc.set_field_value(id, "Hello world").unwrap();
    assert_eq!(doc.form_fields()[0].value, "Hello world");
    assert!(ap_text(&doc, id).contains("(Hello world) Tj"));
    // Password fields hide the value, comb fields space it.
    let mut props = doc.field_props(id).unwrap();
    props.password = true;
    doc.set_field_props(id, &props).unwrap();
    assert!(ap_text(&doc, id).contains("(***********) Tj"));
    doc.undo().unwrap();
    assert!(ap_text(&doc, id).contains("(Hello world) Tj"));
}

#[test]
fn check_boxes_and_radio_groups() {
    let mut doc = blank();
    let cb = doc
        .add_form_field(
            0,
            NewField::CheckBox,
            [40.0, 300.0, 60.0, 320.0],
            &FieldProps::default(),
        )
        .unwrap();
    doc.set_field_value(cb, "Yes").unwrap();
    let f = doc.form_fields().into_iter().find(|f| f.id == cb).unwrap();
    assert_eq!(f.value, "Yes");
    let as_state = |doc: &Document, w| {
        doc.inner()
            .get_dictionary(w)
            .unwrap()
            .get(b"AS")
            .unwrap()
            .as_name()
            .unwrap()
            .to_vec()
    };
    assert_eq!(as_state(&doc, cb), b"Yes");
    doc.set_field_value(cb, "Off").unwrap();
    assert_eq!(as_state(&doc, cb), b"Off");

    let a = FieldProps {
        name: "Size".into(),
        export_value: "Small".into(),
        ..FieldProps::default()
    };
    let b = FieldProps {
        name: "Size".into(),
        export_value: "Large".into(),
        ..FieldProps::default()
    };
    let w1 = doc
        .add_form_field(0, NewField::RadioButton, [40.0, 200.0, 60.0, 220.0], &a)
        .unwrap();
    let w2 = doc
        .add_form_field(0, NewField::RadioButton, [80.0, 200.0, 100.0, 220.0], &b)
        .unwrap();
    let group = doc.form_fields().into_iter().find(|f| f.name == "Size").unwrap();
    assert_eq!(group.kind, FieldKind::RadioButton);
    assert_eq!(doc.field_widgets(group.id), vec![w1, w2]);
    doc.set_field_value(group.id, "Large").unwrap();
    assert_eq!(as_state(&doc, w1), b"Off");
    assert_eq!(as_state(&doc, w2), b"Large");
    let _ = doc.render_page_png(0, 1.0).unwrap();
}

#[test]
fn choice_fields_props_and_deletion() {
    let mut doc = blank();
    let props = FieldProps {
        options: vec!["One".into(), "Two".into(), "Three".into()],
        ..FieldProps::default()
    };
    let combo = doc
        .add_form_field(0, NewField::ComboBox, [40.0, 300.0, 200.0, 320.0], &props)
        .unwrap();
    doc.set_field_value(combo, "Two").unwrap();
    assert!(ap_text(&doc, combo).contains("(Two) Tj"));
    let mut p = doc.field_props(combo).unwrap();
    assert_eq!(p.options.len(), 3);
    p.name = "Number".into();
    p.tooltip = "Pick a number".into();
    p.required = true;
    p.font_size = 14.0;
    p.text_color = [0.8, 0.0, 0.0];
    p.options.push("Four".into());
    doc.set_field_props(combo, &p).unwrap();
    let back = doc.field_props(combo).unwrap();
    assert_eq!(
        (
            back.name.as_str(),
            back.tooltip.as_str(),
            back.required,
            back.font_size
        ),
        ("Number", "Pick a number", true, 14.0)
    );
    assert_eq!(back.options.len(), 4);
    assert!(doc.form_fields()[0].required);

    let list = doc
        .add_form_field(
            0,
            NewField::ListBox,
            [40.0, 200.0, 200.0, 280.0],
            &FieldProps {
                multi_select: true,
                ..props.clone()
            },
        )
        .unwrap();
    doc.set_field_value(list, "One\nThree").unwrap();
    assert_eq!(
        doc.form_fields()
            .into_iter()
            .find(|f| f.id == list)
            .unwrap()
            .value,
        "One, Three"
    );

    doc.delete_form_field(combo).unwrap();
    assert_eq!(doc.form_fields().len(), 1);
    assert_eq!(doc.annotations(0).unwrap().len(), 1);
    doc.set_tab_order("R").unwrap();
    assert!(doc.set_tab_order("X").is_err());
}

#[test]
fn bulk_values_and_reset() {
    let mut doc = blank();
    let t = doc
        .add_form_field(
            0,
            NewField::Text,
            [40.0, 300.0, 240.0, 330.0],
            &FieldProps {
                default_value: "start".into(),
                ..FieldProps::default()
            },
        )
        .unwrap();
    let c = doc
        .add_form_field(
            0,
            NewField::CheckBox,
            [40.0, 250.0, 60.0, 270.0],
            &FieldProps::default(),
        )
        .unwrap();
    let n = doc
        .set_field_values(&[
            ("Text1".into(), "filled".into()),
            ("Check1".into(), "Yes".into()),
            ("Nope".into(), "x".into()),
        ])
        .unwrap();
    assert_eq!(n, 2);
    doc.reset_form().unwrap();
    let fields = doc.form_fields();
    assert_eq!(fields.iter().find(|f| f.id == t).unwrap().value, "start");
    assert_eq!(fields.iter().find(|f| f.id == c).unwrap().value, "Off");
}
#[test]
fn form_data_round_trips_through_every_format() {
    use folitecta_core::FormDataFormat;
    for format in [
        FormDataFormat::Xfdf,
        FormDataFormat::Fdf,
        FormDataFormat::Csv,
        FormDataFormat::Text,
    ] {
        let mut src = blank();
        let t = src
            .add_form_field(
                0,
                NewField::Text,
                [40.0, 300.0, 240.0, 330.0],
                &FieldProps {
                    name: "Name".into(),
                    ..FieldProps::default()
                },
            )
            .unwrap();
        let c = src
            .add_form_field(
                0,
                NewField::CheckBox,
                [40.0, 250.0, 60.0, 270.0],
                &FieldProps {
                    name: "Agree".into(),
                    ..FieldProps::default()
                },
            )
            .unwrap();
        src.set_field_value(t, "Ada, \"Countess\" (L)").unwrap();
        src.set_field_value(c, "Yes").unwrap();
        let data = src.export_form_data(format);
        // A blank copy of the same form receives the values.
        let mut dst = blank();
        dst.add_form_field(
            0,
            NewField::Text,
            [40.0, 300.0, 240.0, 330.0],
            &FieldProps {
                name: "Name".into(),
                ..FieldProps::default()
            },
        )
        .unwrap();
        dst.add_form_field(
            0,
            NewField::CheckBox,
            [40.0, 250.0, 60.0, 270.0],
            &FieldProps {
                name: "Agree".into(),
                ..FieldProps::default()
            },
        )
        .unwrap();
        let n = dst.import_form_data(format, &data).unwrap();
        assert_eq!(n, 2, "{format:?}: {}", String::from_utf8_lossy(&data));
        let fields = dst.form_fields();
        assert_eq!(fields[0].value, "Ada, \"Countess\" (L)", "{format:?}");
        assert_eq!(fields[1].value, "Yes", "{format:?}");
        let widgets = dst.page_widgets(0);
        assert_eq!(widgets.len(), 2);
        assert_eq!(widgets[1].on_state, "Yes");
    }
}

#[test]
fn widgets_move_in_the_manual_tab_order() {
    let mut doc = blank();
    let props = FieldProps::default();
    for i in 0..3 {
        let y = 350.0 - i as f32 * 50.0;
        doc.add_form_field(0, NewField::Text, [40.0, y - 30.0, 220.0, y], &props)
            .unwrap();
    }
    doc.set_tab_order("R").unwrap();
    let fields = doc.form_fields();
    let ids: Vec<lopdf::ObjectId> = fields.iter().map(|f| f.widgets[0]).collect();
    assert_eq!(doc.widget_order(0), ids);
    assert!(fields.iter().all(|f| f.short_name() == f.name && !f.barcode));
    assert!(doc.move_widget_in_tab_order(ids[2], -1).unwrap());
    assert_eq!(doc.widget_order(0), vec![ids[0], ids[2], ids[1]]);
    assert!(!doc.move_widget_in_tab_order(ids[0], -1).unwrap());
    let page = doc.inner().get_dictionary(doc.page_id(0).unwrap()).unwrap();
    assert!(!page.has(b"Tabs"), "the page follows its annotation order");
    doc.undo().unwrap();
    assert_eq!(doc.widget_order(0), ids);
}
