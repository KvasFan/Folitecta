use folitecta_core::Document;
use folitecta_core::structure::{StructKid, StructProps};
use folitecta_core::textdoc::TextLayout;
use lopdf::{Object, Stream, dictionary};

/// A page with a title, two paragraphs, a picture, a rule and a page number.
fn sample() -> Document {
    let mut doc = Document::new_blank(2, 595.0, 842.0).unwrap();
    let ids = doc.page_ids();
    {
        let inner = doc.inner_mut();
        let font = inner.add_object(dictionary! {
            "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica", "Encoding" => "WinAnsiEncoding",
        });
        let image = inner.add_object(Stream::new(
            dictionary! {
                "Type" => "XObject", "Subtype" => "Image", "Width" => 2, "Height" => 2,
                "ColorSpace" => "DeviceGray", "BitsPerComponent" => 8,
            },
            vec![0, 255, 255, 0],
        ));
        for (n, pid) in ids.iter().enumerate() {
            let content = format!(
                "BT /F1 24 Tf 56 770 Td (Annual report) Tj ET\n\
                 BT /F1 11 Tf 13.75 TL 56 730 Td (The first paragraph has two lines of) Tj T* (ordinary text in it.) Tj ET\n\
                 BT /F1 11 Tf 13.75 TL 56 660 Td (A second paragraph follows after a gap.) Tj ET\n\
                 q 120 0 0 90 56 520 cm /Im1 Do Q\n\
                 0.5 w 56 500 m 539 500 l S\n\
                 BT /F1 9 Tf 290 30 Td (Page {}) Tj ET\n",
                n + 1
            );
            let sid = inner.add_object(Stream::new(dictionary! {}, content.into_bytes()));
            let page = inner.get_dictionary_mut(*pid).unwrap();
            page.set("Contents", Object::Reference(sid));
            page.set(
                "Resources",
                dictionary! { "Font" => dictionary! { "F1" => font }, "XObject" => dictionary! { "Im1" => image } },
            );
        }
    }
    doc.commit().unwrap();
    doc.add_uri_link(0, [56.0, 655.0, 200.0, 672.0], "https://example.org")
        .unwrap();
    doc
}

#[test]
fn auto_tag_builds_headings_paragraphs_figures_and_artifacts() {
    let mut doc = sample();
    assert!(!doc.is_tagged() && doc.structure().is_none());
    let report = doc.auto_tag().unwrap();
    assert!(doc.is_tagged());
    assert_eq!(report.headings, 2, "{report:?}");
    assert_eq!(report.paragraphs, 4, "{report:?}");
    assert_eq!(report.figures, 2, "{report:?}");
    assert!(report.artifacts >= 4, "{report:?}");
    assert_eq!(report.annotations, 1);
    let tree = doc.structure().unwrap();
    let document = tree.children().next().unwrap();
    assert_eq!(document.tag, "Document");
    let tags: Vec<&str> = document.children().map(|e| e.tag.as_str()).collect();
    assert_eq!(&tags[..4], &["H1", "P", "P", "Figure"]);
    // The paragraph of two lines is one element; the link sits in the paragraph it covers.
    let first = document.children().nth(1).unwrap();
    assert_eq!(
        doc.struct_text(first),
        "The first paragraph has two lines of ordinary text in it."
    );
    let second = document.children().nth(2).unwrap();
    assert!(
        second
            .children()
            .any(|e| e.tag == "Link" && !e.annotations().is_empty())
    );
    // Every drawing is tagged or an artifact.
    let marked = doc.marked_content(0);
    assert!(
        marked.iter().all(|m| m.mcid.is_some() || m.artifact),
        "{marked:?}"
    );
    assert!(marked.iter().any(|m| m.artifact && m.has_path));
    // Annotations are found through their element, content through the parent tree.
    assert!(doc.struct_for_annotation(0, 0).is_some());
    assert_eq!(
        doc.struct_for_content(0, 0),
        Some(document.children().next().unwrap().id)
    );
    // One undo step.
    doc.undo().unwrap();
    assert!(doc.structure().is_none());
}

#[test]
fn reading_order_moves_and_retags() {
    let mut doc = sample();
    doc.auto_tag().unwrap();
    let order = doc.reading_order(0, false);
    let tags: Vec<&str> = order.iter().map(|o| o.tag.as_str()).collect();
    assert_eq!(tags, vec!["H1", "P", "P", "Figure"]);
    assert!(order[0].text.contains("Annual report"));
    assert!(order[3].rect[2] > order[3].rect[0]);
    doc.move_reading_order(0, 0, 2, false).unwrap();
    let tags: Vec<String> = doc
        .reading_order(0, false)
        .iter()
        .map(|o| o.tag.clone())
        .collect();
    assert_eq!(tags, vec!["P", "P", "H1", "Figure"]);
    let id = doc.reading_order(0, false)[0].id;
    doc.set_struct_tag(id, "H2").unwrap();
    assert_eq!(doc.reading_order(0, false)[0].tag, "H2");
    // Neighbouring paragraphs with the same tag join.
    doc.set_struct_tag(id, "P").unwrap();
    let joined = doc.group_similar(0, false).unwrap();
    assert_eq!(joined, 1);
    let order = doc.reading_order(0, false);
    assert_eq!(order.len(), 3);
    assert!(order[0].text.contains("ordinary text") && order[0].text.contains("second paragraph"));
}

#[test]
fn edit_tags_artifacts_and_empty_elements() {
    let mut doc = sample();
    doc.auto_tag().unwrap();
    let tree = doc.structure().unwrap();
    let document = tree.children().next().unwrap().id;
    let figure = doc.struct_elems_of("Figure")[0].clone();
    doc.set_struct_props(
        figure.id,
        &StructProps {
            tag: "Figure".into(),
            alt: "A chequered square".into(),
            ..StructProps::default()
        },
    )
    .unwrap();
    assert_eq!(doc.struct_elems_of("Figure")[0].alt, "A chequered square");
    // The second figure becomes decoration.
    let second = doc.struct_elems_of("Figure")[1].id;
    doc.struct_to_artifact(second).unwrap();
    assert_eq!(doc.struct_elems_of("Figure").len(), 1);
    assert!(doc.marked_content(1).iter().any(|m| m.artifact && m.has_image));
    // New elements, a move, deleting the empty ones.
    let sect = doc
        .add_struct_elem(Some(document), Some(0), "Sect", "Part one")
        .unwrap();
    let h1 = doc.struct_elems_of("H1")[0].id;
    doc.move_struct_elem(h1, Some(sect), 0).unwrap();
    let tree = doc.structure().unwrap();
    assert_eq!(tree.find(sect).unwrap().children().next().unwrap().id, h1);
    assert_eq!(doc.struct_for_content(0, 0), Some(h1));
    doc.add_struct_elem(Some(document), None, "Div", "").unwrap();
    assert_eq!(doc.delete_empty_struct().unwrap(), 1);
    assert!(doc.move_struct_elem(sect, Some(h1), 0).is_err());
    doc.delete_struct_elem(sect).unwrap();
    assert!(doc.structure().unwrap().find(h1).is_none());
    assert_eq!(doc.struct_for_content(0, 0), None);
}

#[test]
fn tags_from_an_area_and_annotations() {
    let mut doc = Document::from_text(
        "First line of text\nSecond line of text\n\nThird paragraph",
        TextLayout::default(),
    )
    .unwrap();
    doc.add_uri_link(0, [56.0, 700.0, 200.0, 715.0], "https://example.org")
        .unwrap();
    // The second line only (display space, y down).
    let layer = doc.text_layer(0).unwrap();
    let r = layer.lines[1].bbox;
    let id = doc
        .tag_area(0, [r[0], r[1] + 1.0, r[2], r[3] - 1.0], "P", None, None)
        .unwrap();
    let tree = doc.structure().unwrap();
    let elem = tree.find(id).unwrap();
    assert_eq!(doc.struct_text(elem), "Second line of text");
    assert!(matches!(elem.kids[0], StructKid::Content { page: 0, mcid: 0 }));
    // The same area again takes the tagged content into a new element.
    let id2 = doc.tag_area(0, r, "Span", Some(id), None).unwrap();
    let tree = doc.structure().unwrap();
    assert!(
        tree.find(id)
            .unwrap()
            .kids
            .iter()
            .all(|k| matches!(k, StructKid::Elem(_)))
    );
    assert_eq!(doc.struct_for_content(0, 0), Some(id2));
    assert!(
        doc.tag_area(0, [500.0, 800.0, 520.0, 820.0], "P", None, None)
            .is_err()
    );
    assert_eq!(doc.tag_annotations().unwrap(), 1);
    assert_eq!(doc.tag_annotations().unwrap(), 0);
    assert!(doc.struct_for_annotation(0, 0).is_some());
    assert_eq!(
        doc.inner()
            .get_dictionary(doc.page_id(0).unwrap())
            .unwrap()
            .get(b"Tabs")
            .unwrap()
            .as_name()
            .unwrap(),
        b"S"
    );
    doc.clear_structure().unwrap();
    assert!(!doc.has_structure() && !doc.is_tagged());
    assert!(doc.marked_content(0).iter().all(|m| m.mcid.is_none()));
}

#[test]
fn marks_stay_artifacts_in_their_own_streams() {
    let mut doc = Document::from_text(
        "Body text of the page.\n\nAnother paragraph.",
        TextLayout::default(),
    )
    .unwrap();
    doc.add_text_watermark(
        &[0],
        &folitecta_core::marks::TextMarkOptions {
            text: "DRAFT".into(),
            font_size: 72.0,
            ..Default::default()
        },
    )
    .unwrap();
    // A new watermark is an artifact already.
    assert!(
        doc.marked_content(0)
            .iter()
            .any(|m| m.artifact && m.text.contains("DRAFT"))
    );
    let report = doc.auto_tag().unwrap();
    assert_eq!(report.headings, 0, "{report:?}");
    assert_eq!(report.paragraphs, 2);
    assert_eq!(doc.marks_on_page(0), vec!["Watermark".to_string()]);
    // The watermark goes away with its stream; the tags stay.
    assert_eq!(doc.remove_marks_of(&[0], &["Watermark"]).unwrap(), 1);
    assert!(!doc.page_text(0).contains("DRAFT"));
    assert_eq!(doc.struct_elems_of("P").len(), 2);
    assert!(
        doc.marked_content(0)
            .iter()
            .all(|m| m.mcid.is_some() || m.artifact)
    );
}

#[test]
fn role_map_and_class_map() {
    let mut doc = sample();
    doc.create_struct_root().unwrap();
    assert!(doc.create_struct_root().is_err());
    let custom = doc.add_struct_elem(None, None, "Heading", "").unwrap();
    doc.set_role_map(&[("Heading".into(), "H1".into())]).unwrap();
    assert_eq!(doc.role_map(), vec![("Heading".to_string(), "H1".to_string())]);
    let tree = doc.structure().unwrap();
    let e = tree.find(custom).unwrap();
    assert_eq!((e.tag.as_str(), e.role.as_str()), ("Heading", "H1"));
    assert_eq!(doc.apply_role_map().unwrap(), 1);
    assert_eq!(doc.structure().unwrap().find(custom).unwrap().tag, "H1");
    doc.set_class_map(&[("Indented".into(), "O=Layout StartIndent=18".into())])
        .unwrap();
    let cm = doc.class_map();
    assert_eq!(cm.len(), 1);
    assert!(cm[0].1.contains("StartIndent=18"));
    doc.set_tagged(false).unwrap();
    assert!(!doc.is_tagged());
}
