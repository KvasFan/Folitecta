//! Layers: tree, visibility, editing, merging, flattening, extraction, import.

use std::collections::{HashMap, HashSet};

use folitecta_core::{Document, LayerNode};
use lopdf::{Object, Stream, dictionary};

/// One page with base content, content on layers A and B, content shown when
/// A or B is on, a form on layer C and a square comment on B. B is hidden.
fn layered() -> Document {
    let mut doc = lopdf::Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    let font = doc.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica",
    });
    let a = doc.add_object(dictionary! { "Type" => "OCG", "Name" => Object::string_literal("Alpha") });
    let b = doc.add_object(dictionary! { "Type" => "OCG", "Name" => Object::string_literal("Bravo") });
    let c = doc.add_object(dictionary! { "Type" => "OCG", "Name" => Object::string_literal("Charlie") });
    let m = doc.add_object(dictionary! {
        "Type" => "OCMD", "OCGs" => vec![a.into(), b.into()], "P" => "AnyOn",
    });
    let form = doc.add_object(Stream::new(
        dictionary! {
            "Type" => "XObject", "Subtype" => "Form", "BBox" => vec![0.into(), 0.into(), 200.into(), 200.into()],
            "OC" => c,
            "Resources" => dictionary! { "Font" => dictionary! { "F1" => font } },
        },
        b"BT /F1 12 Tf 10 90 Td (charlie) Tj ET".to_vec(),
    ));
    let content = doc.add_object(Stream::new(
        dictionary! {},
        b"q BT /F1 12 Tf 10 10 Td (base) Tj ET Q\n\
          /OC /PA BDC BT /F1 12 Tf 10 30 Td (alpha) Tj ET EMC\n\
          /OC /PB BDC BT /F1 12 Tf 10 50 Td (bravo) Tj ET EMC\n\
          /OC /PM BDC BT /F1 12 Tf 10 70 Td (either) Tj ET EMC\n\
          /Fx Do"
            .to_vec(),
    ));
    let square = doc.add_object(dictionary! {
        "Type" => "Annot", "Subtype" => "Square",
        "Rect" => vec![100.into(), 100.into(), 150.into(), 150.into()],
        "OC" => b,
    });
    let page = doc.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "MediaBox" => vec![0.into(), 0.into(), 300.into(), 300.into()],
        "Contents" => content,
        "Annots" => vec![square.into()],
        "Resources" => dictionary! {
            "Font" => dictionary! { "F1" => font },
            "XObject" => dictionary! { "Fx" => form },
            "Properties" => dictionary! { "PA" => a, "PB" => b, "PM" => m },
        },
    });
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! { "Type" => "Pages", "Kids" => vec![page.into()], "Count" => 1 }),
    );
    let catalog = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
        "OCProperties" => dictionary! {
            "OCGs" => vec![a.into(), b.into(), c.into()],
            "D" => dictionary! {
                "Order" => vec![
                    a.into(),
                    Object::Array(vec![b.into()]),
                    Object::Array(vec![Object::string_literal("Group"), c.into()]),
                ],
                "OFF" => vec![b.into()],
            },
        },
    });
    doc.trailer.set("Root", catalog);
    let mut out = Vec::new();
    doc.save_to(&mut out).unwrap();
    Document::from_bytes(out, None, None).unwrap()
}

fn content(doc: &Document) -> String {
    let pid = doc.page_id(0).unwrap();
    String::from_utf8_lossy(&doc.inner().get_page_content(pid)).to_string()
}

fn ids(doc: &Document) -> (lopdf::ObjectId, lopdf::ObjectId, lopdf::ObjectId) {
    let l = doc.layers();
    (l[0].id, l[1].id, l[2].id)
}

#[test]
fn tree_visibility_and_usage() {
    let doc = layered();
    let layers = doc.layers();
    let names: Vec<&str> = layers.iter().map(|l| l.name.as_str()).collect();
    assert_eq!(names, ["Alpha", "Bravo", "Charlie"]);
    assert_eq!(
        layers.iter().map(|l| l.visible).collect::<Vec<_>>(),
        [true, false, true]
    );
    let (a, b, c) = ids(&doc);
    assert_eq!(
        doc.layer_tree(),
        vec![
            LayerNode::Layer {
                id: a,
                children: vec![LayerNode::Layer {
                    id: b,
                    children: vec![]
                }],
            },
            LayerNode::Group {
                label: "Group".into(),
                children: vec![LayerNode::Layer {
                    id: c,
                    children: vec![]
                }],
            },
        ]
    );
    assert_eq!(doc.layers_on_page(0), HashSet::from([a, b, c]));
    assert_eq!(doc.layer_annotations(&HashSet::from([b])).len(), 1);
    // A hidden parent hides its children.
    let mut states = HashMap::from([(a, false), (b, true)]);
    folitecta_core::layers::inherit_visibility(&doc.layer_tree(), &mut states);
    assert!(!states[&b]);
    // A view with Bravo visible does not change the document.
    let view = doc
        .view_bytes(&HashSet::new(), &HashMap::from([(b, true)]))
        .unwrap();
    let shown = Document::from_bytes(view.to_vec(), None, None).unwrap();
    assert!(shown.layers().iter().all(|l| l.visible));
    assert!(!doc.layers()[1].visible);
    doc.render_page_png(0, 1.0).unwrap();
}

#[test]
fn flatten_keeps_visible_content() {
    let mut doc = layered();
    let (_, b, _) = ids(&doc);
    assert_eq!(doc.flatten_layers(None, &HashMap::new()).unwrap(), 3);
    let text = content(&doc);
    for kept in ["base", "alpha", "either", "/Fx Do"] {
        assert!(text.contains(kept), "{kept} in {text}");
    }
    assert!(!text.contains("bravo"));
    assert!(!text.contains("BDC") && !text.contains("EMC"));
    assert!(doc.layers().is_empty());
    assert!(
        doc.annotations(0).unwrap().is_empty(),
        "the comment on Bravo goes"
    );
    let _ = b;
    doc.undo().unwrap();
    assert_eq!(doc.layers().len(), 3);
}

#[test]
fn flatten_selected_layers_only() {
    let mut doc = layered();
    let (a, b, c) = ids(&doc);
    // Bravo shown in the view: its content stays.
    assert_eq!(
        doc.flatten_layers(Some(&[b]), &HashMap::from([(b, true)]))
            .unwrap(),
        1
    );
    let text = content(&doc);
    assert!(text.contains("bravo") && !text.contains("/PB"));
    assert!(text.contains("/OC /PA BDC"), "{text}");
    // The membership of A and B still depends on A.
    assert!(text.contains("/OC /PM BDC"));
    assert_eq!(doc.layers().iter().map(|l| l.id).collect::<Vec<_>>(), [a, c]);
    assert_eq!(doc.annotations(0).unwrap().len(), 1);
}

#[test]
fn merge_moves_content_to_the_target() {
    let mut doc = layered();
    let (a, b, c) = ids(&doc);
    assert_eq!(doc.merge_layers(&[b], a).unwrap(), 1);
    assert_eq!(doc.layers().iter().map(|l| l.id).collect::<Vec<_>>(), [a, c]);
    assert_eq!(doc.layer_tree().len(), 2);
    assert_eq!(doc.layer_annotations(&HashSet::from([a])).len(), 1);
    assert_eq!(doc.layers_on_page(0), HashSet::from([a, c]));
    assert!(doc.merge_layers(&[a], a).unwrap() == 0);
}

#[test]
fn extract_selected_or_visible_content() {
    let doc = layered();
    let (a, _, c) = ids(&doc);
    let only_a = doc
        .extract_layers(Some(&HashSet::from([a])), &HashMap::new())
        .unwrap();
    let text = content(&only_a);
    assert!(text.contains("alpha") && text.contains("either"));
    assert!(!text.contains("(base) Tj") && !text.contains("bravo") && !text.contains("/Fx Do"));
    assert!(only_a.layers().is_empty());
    let visible = doc.extract_layers(None, &HashMap::from([(c, false)])).unwrap();
    let text = content(&visible);
    assert!(text.contains("(base) Tj") && text.contains("alpha") && !text.contains("bravo"));
    assert!(!text.contains("/Fx Do"));
    // The source document is unchanged.
    assert_eq!(doc.layers().len(), 3);
}

#[test]
fn layers_are_created_renamed_sorted_and_locked() {
    let mut doc = Document::new_blank(1, 200.0, 200.0).unwrap();
    assert!(doc.layer_tree().is_empty());
    let (z, zp) = doc.add_layer("Zulu", &[]).unwrap();
    assert_eq!(zp, vec![0]);
    let gp = doc.add_layer_group("Notes", &[]).unwrap();
    let (y, yp) = doc.add_layer("yankee", &gp).unwrap();
    assert_eq!(yp, vec![1, 0]);
    let (x, _) = doc.add_layer("Xray", &[]).unwrap();
    assert!(doc.add_layer(" ", &[]).is_err());
    doc.rename_layer_node(&[0], "Mike").unwrap();
    doc.rename_layer_node(&gp, "Annotations").unwrap();
    doc.sort_layers().unwrap();
    let tree = doc.layer_tree();
    assert!(matches!(&tree[0], LayerNode::Group { label, .. } if label == "Annotations"));
    assert_eq!(tree[1].layer_id(), Some(z));
    assert_eq!(tree[2].layer_id(), Some(x));
    assert_eq!(tree[0].children()[0].layer_id(), Some(y));
    assert_eq!(doc.layers()[0].name, "Mike");
    doc.set_layers_locked(&[x], true).unwrap();
    assert!(doc.layers().iter().find(|l| l.id == x).unwrap().locked);
    doc.set_layer_defaults(&HashMap::from([(z, false)])).unwrap();
    assert!(!doc.layers()[0].visible);
    // Saved and reopened.
    let reopened = Document::from_bytes(doc.bytes().to_vec(), None, None).unwrap();
    assert_eq!(reopened.layer_tree(), doc.layer_tree());
}

#[test]
fn pages_are_imported_as_a_layer() {
    let mut doc = Document::new_blank(2, 200.0, 200.0).unwrap();
    let other = layered();
    let undo_before = doc.can_undo();
    let id = doc.import_as_layer(&other, &[0, 1], false, "Imported").unwrap();
    assert_eq!(doc.layers().last().unwrap().id, id);
    assert!(doc.layers_on_page(1).contains(&id));
    doc.render_page_png(0, 1.0).unwrap();
    doc.undo().unwrap();
    assert!(doc.layers().is_empty(), "one undo step");
    assert_eq!(doc.can_undo(), undo_before);
}
