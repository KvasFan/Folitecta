use folitecta_core::{Document, LinkTarget};

#[test]
fn links_open_files_go_to_destinations_and_run_scripts() {
    let mut doc = Document::new_blank(3, 300.0, 400.0).unwrap();
    let r = [20.0, 20.0, 120.0, 40.0];
    doc.add_named_destination("Summary", 2, None).unwrap();
    doc.add_named_link(0, r, "Summary").unwrap();
    doc.add_launch_link(0, r, r"C:\Reports\annex.pdf").unwrap();
    doc.add_script_link(0, r, "app.alert('Hello');").unwrap();
    doc.add_page_link(0, r, 1).unwrap();
    let targets: Vec<LinkTarget> = doc.links(0).unwrap().into_iter().map(|l| l.target).collect();
    assert_eq!(targets.len(), 4, "{targets:?}");
    assert!(
        matches!(&targets[0], LinkTarget::Named(n) if n == "Summary") || targets[0] == LinkTarget::Page(2),
        "{:?}",
        targets[0]
    );
    // Shown as typed (the Unicode name of the file specification).
    assert_eq!(targets[1], LinkTarget::Launch(r"C:\Reports\annex.pdf".into()));
    assert_eq!(targets[2], LinkTarget::Other("JavaScript".into()));
    assert_eq!(targets[3], LinkTarget::Page(1));
    // Nothing to point at: refused.
    assert!(doc.add_named_link(0, r, "Missing").is_err());
    assert!(doc.add_launch_link(0, r, "  ").is_err());
    assert!(doc.add_script_link(0, r, "").is_err());
    // Saved and opened again, the links stay.
    let bytes = doc.bytes().as_ref().clone();
    let again = Document::from_bytes(bytes, None, None).unwrap();
    assert_eq!(again.links(0).unwrap().len(), 4);
}

#[test]
fn named_actions_form_actions_and_buttons() {
    use folitecta_core::testdocs::Builder;
    use lopdf::{Object, dictionary};
    let mut b = Builder::new();
    let link = |action: lopdf::Dictionary| {
        Object::Dictionary(dictionary! {
            "Type" => "Annot", "Subtype" => "Link", "Rect" => vec![10.into(), 10.into(), 60.into(), 30.into()],
            "A" => action,
        })
    };
    let button = b.doc.add_object(dictionary! {
        "Type" => "Annot", "Subtype" => "Widget", "FT" => "Btn", "Ff" => 65536,
        "T" => Object::string_literal("Send"), "Rect" => vec![100.into(), 10.into(), 160.into(), 30.into()],
        "AA" => dictionary! { "U" => dictionary! { "S" => "SubmitForm", "F" => Object::string_literal("https://example.org/form") } },
    });
    let plain = b.doc.add_object(dictionary! {
        "Type" => "Annot", "Subtype" => "Widget", "FT" => "Btn", "Ff" => 65536,
        "T" => Object::string_literal("Nothing"), "Rect" => vec![200.into(), 10.into(), 260.into(), 30.into()],
    });
    b.page(
        (300.0, 300.0),
        Vec::new(),
        lopdf::Dictionary::new(),
        vec![
            link(dictionary! { "S" => "Named", "N" => "NextPage" }),
            link(dictionary! { "S" => "SubmitForm", "F" => Object::string_literal("https://example.org/form") }),
            link(dictionary! { "S" => "ResetForm" }),
            Object::Reference(button),
            Object::Reference(plain),
        ],
    );
    b.catalog_entry(
        "AcroForm",
        dictionary! { "Fields" => vec![button.into(), plain.into()] }.into(),
    );
    let doc = Document::from_bytes(b.finish(), None, None).unwrap();
    let targets: Vec<LinkTarget> = doc.links(0).unwrap().into_iter().map(|l| l.target).collect();
    assert_eq!(
        targets,
        [
            LinkTarget::NamedAction("NextPage".into()),
            LinkTarget::Submit("https://example.org/form".into()),
            LinkTarget::Reset,
        ]
    );
    assert_eq!(
        doc.button_target(button),
        Some(LinkTarget::Submit("https://example.org/form".into()))
    );
    assert_eq!(doc.button_target(plain), None);
}

#[test]
fn the_actions_of_a_button_follow_their_chain_and_a_loop_ends() {
    use folitecta_core::ButtonAction;
    use folitecta_core::testdocs::{Builder, action_buttons_doc};
    use lopdf::{Object, dictionary};
    let doc = Document::from_bytes(action_buttons_doc(), None, None).unwrap();
    let fill = doc.form_fields().into_iter().find(|f| f.name == "Fill").unwrap();
    assert_eq!(
        doc.button_actions(fill.id),
        [
            ButtonAction::Script("this.getField('Total').value = '42';".into()),
            ButtonAction::Script("app.alert('Done');".into()),
        ]
    );
    // A link does the same, a destination of its own being one action.
    let mut plain = Document::new_blank(3, 300.0, 300.0).unwrap();
    let r = [20.0, 20.0, 120.0, 40.0];
    plain.add_page_link(0, r, 2).unwrap();
    plain.add_script_link(0, r, "app.alert('Hello');").unwrap();
    let links = plain.links(0).unwrap();
    assert_eq!(
        plain.link_actions(0, links[0].index),
        [ButtonAction::Target(LinkTarget::Page(2))]
    );
    assert_eq!(
        plain.link_actions(0, links[1].index),
        [ButtonAction::Script("app.alert('Hello');".into())]
    );
    // An action whose chain comes back to itself.
    let mut b = Builder::new();
    let id = b.doc.new_object_id();
    b.doc.objects.insert(
        id,
        dictionary! { "S" => "Named", "N" => "NextPage", "Next" => Object::Reference(id) }.into(),
    );
    let button = b.doc.add_object(dictionary! {
        "Type" => "Annot", "Subtype" => "Widget", "FT" => "Btn", "Ff" => 65536,
        "T" => Object::string_literal("Loop"), "Rect" => vec![0.into(), 0.into(), 10.into(), 10.into()],
        "A" => Object::Reference(id),
    });
    b.page(
        (300.0, 300.0),
        Vec::new(),
        lopdf::Dictionary::new(),
        vec![Object::Reference(button)],
    );
    b.catalog_entry("AcroForm", dictionary! { "Fields" => vec![button.into()] }.into());
    let doc = Document::from_bytes(b.finish(), None, None).unwrap();
    let chain = doc.button_actions(button);
    assert!(!chain.is_empty() && chain.len() <= 65, "{}", chain.len());
}

#[test]
fn the_actions_of_a_link_added_moved_replaced_and_removed() {
    use folitecta_core::{ButtonAction, LinkTarget};
    let mut doc = Document::new_blank(3, 300.0, 300.0).unwrap();
    doc.add_page_link(0, [10.0, 10.0, 100.0, 40.0], 2).unwrap();
    let link = doc
        .annotations(0)
        .unwrap()
        .iter()
        .position(|a| a.subtype == "Link")
        .unwrap();
    let list = |d: &Document| d.link_actions(0, link);
    assert_eq!(list(&doc), vec![ButtonAction::Target(LinkTarget::Page(2))]);
    // A script and a web address after it.
    doc.add_link_action(0, link, &ButtonAction::Script("app.alert('hi');".into()))
        .unwrap();
    doc.add_link_action(
        0,
        link,
        &ButtonAction::Target(LinkTarget::Uri("https://example.org".into())),
    )
    .unwrap();
    assert_eq!(list(&doc).len(), 3);
    // The address first, the page last, the script made a next-page action.
    doc.move_link_action(0, link, 2, true).unwrap();
    doc.move_link_action(0, link, 1, true).unwrap();
    doc.move_link_action(0, link, 2, false).unwrap();
    doc.replace_link_action(
        0,
        link,
        2,
        &ButtonAction::Target(LinkTarget::NamedAction("NextPage".into())),
    )
    .unwrap();
    let reopened = Document::from_bytes(doc.bytes().to_vec(), None, None).unwrap();
    assert_eq!(
        reopened.link_actions(0, link),
        vec![
            ButtonAction::Target(LinkTarget::Uri("https://example.org".into())),
            ButtonAction::Target(LinkTarget::Page(2)),
            ButtonAction::Target(LinkTarget::NamedAction("NextPage".into())),
        ]
    );
    // Removed down to one; the last stays.
    doc.remove_link_action(0, link, 0).unwrap();
    doc.remove_link_action(0, link, 1).unwrap();
    assert_eq!(list(&doc), vec![ButtonAction::Target(LinkTarget::Page(2))]);
    assert!(doc.remove_link_action(0, link, 0).is_err());
    // One undo step each.
    doc.undo().unwrap();
    assert_eq!(list(&doc).len(), 2);
}

#[test]
fn go_to_views_and_layer_actions_of_a_link() {
    use folitecta_core::outline::DestView;
    use folitecta_core::{ActionInfo, LayerState};
    let mut doc = Document::new_blank(3, 595.0, 842.0).unwrap();
    doc.add_layer("Notes", &[]).unwrap();
    doc.add_layer("Plans", &[]).unwrap();
    doc.add_page_link(0, [100.0, 100.0, 200.0, 120.0], 1).unwrap();
    let index = doc
        .annotations(0)
        .unwrap()
        .iter()
        .position(|a| a.subtype == "Link")
        .unwrap();
    // The go-to gets a view, then a name of its own, then the name changes.
    let view = DestView::Xyz {
        left: Some(10.0),
        top: Some(700.0),
        zoom: Some(1.5),
    };
    doc.set_link_goto(0, index, 0, 2, &view).unwrap();
    match doc.link_action_info(0, index, 0) {
        Some(ActionInfo::GoTo {
            page: Some(2),
            view: Some(DestView::Xyz { zoom: Some(z), .. }),
            named: None,
        }) => assert!((z - 1.5).abs() < 1e-3),
        other => panic!("{other:?}"),
    }
    doc.name_link_goto(0, index, 0, "Plans page").unwrap();
    assert!(
        matches!(doc.link_action_info(0, index, 0), Some(ActionInfo::GoTo { page: Some(2), named: Some(ref n), .. }) if n == "Plans page")
    );
    doc.name_link_goto(0, index, 0, "Plans").unwrap();
    assert!(
        matches!(doc.link_action_info(0, index, 0), Some(ActionInfo::GoTo { named: Some(ref n), .. }) if n == "Plans")
    );
    // Two layer actions added, then one: Simplify makes one of them.
    doc.set_link_layer_action(0, index, 9, LayerState::Off, None)
        .unwrap();
    let first = doc.layers()[0].id;
    doc.set_link_layer_action(0, index, 9, LayerState::Toggle, Some(&[first]))
        .unwrap();
    assert!(
        matches!(doc.link_action_info(0, index, 2), Some(ActionInfo::Layers { ref state, ref layers }) if state == "Toggle" && layers == &[first])
    );
    assert_eq!(doc.simplify_link_actions(0, index).unwrap(), 1);
    match doc.link_action_info(0, index, 1) {
        Some(ActionInfo::Layers { state, layers }) => {
            assert_eq!(state, "Mixed");
            assert_eq!(layers.len(), 3);
        }
        other => panic!("{other:?}"),
    }
    // Kept in the file.
    let again = Document::from_bytes(doc.bytes().to_vec(), None, None).unwrap();
    assert_eq!(again.link_actions(0, index).len(), 2);
}
