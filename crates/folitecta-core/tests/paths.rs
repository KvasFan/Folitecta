//! Editing the vector paths of a page: a triangle and a rectangle found
//! with their nodes and colours; a node moved, a fill changed and a path
//! deleted in the content itself, each one undo step, still so when the
//! file is opened again; the paths of a template found by a click near
//! one of its lines.

use folitecta_core::{Document, Renderer, Template, TemplateOptions};
use lopdf::{Object, Stream, dictionary};

fn drawing() -> Document {
    let mut doc = Document::new_blank(1, 300.0, 300.0).unwrap();
    let pid = doc.page_ids()[0];
    let inner = doc.inner_mut();
    let content = inner.add_object(Stream::new(
        dictionary! {},
        b"q 1 0 0 RG 2 w 20 20 m 120 20 l 70 100 l h S Q 0 0 1 rg 150 150 60 40 re f".to_vec(),
    ));
    inner
        .get_dictionary_mut(pid)
        .unwrap()
        .set("Contents", Object::Reference(content));
    doc.commit().unwrap();
    doc.clear_history();
    doc
}

fn reopened(doc: &Document) -> Document {
    Document::from_bytes(doc.bytes().to_vec(), None, None).unwrap()
}

#[test]
fn paths_are_found_with_their_nodes_and_colours() {
    let doc = drawing();
    let paths = doc.page_paths(0).unwrap();
    assert_eq!(paths.len(), 2);
    assert_eq!(paths[0].nodes.len(), 3);
    assert_eq!(paths[0].stroke, Some([1.0, 0.0, 0.0]));
    assert!(paths[0].stroked && !paths[0].filled);
    assert_eq!(paths[1].fill, Some([0.0, 0.0, 1.0]));
    assert_eq!(paths[1].bounds, [150.0, 150.0, 210.0, 190.0]);
    assert_eq!(doc.path_at(0, 180.0, 170.0, 2.0).map(|p| p.index), Some(1));
    assert_eq!(doc.path_at(0, 5.0, 280.0, 2.0), None);
}

#[test]
fn a_moved_node_stays_moved_in_the_file() {
    let mut doc = drawing();
    let before = doc.bytes();
    doc.move_path_node(0, 0, 2, [70.0, 200.0]).unwrap();
    let again = reopened(&doc);
    let paths = again.page_paths(0).unwrap();
    assert_eq!(paths[0].nodes[2].at, [70.0, 200.0]);
    assert_eq!(paths[0].nodes[0].at, [20.0, 20.0], "the other nodes stay");
    assert!(Renderer::new(again.bytes()).unwrap().render_page(0, 0.5).is_ok());
    // The far corner of the rectangle changes its size.
    doc.move_path_node(0, 1, 1, [250.0, 260.0]).unwrap();
    assert_eq!(
        reopened(&doc).page_paths(0).unwrap()[1].bounds,
        [150.0, 150.0, 250.0, 260.0]
    );
    doc.undo().unwrap();
    doc.undo().unwrap();
    assert_eq!(doc.bytes(), before);
}

#[test]
fn colours_width_and_deletion() {
    let mut doc = drawing();
    doc.set_path_style(0, 1, None, Some([0.0, 0.6, 0.0]), None)
        .unwrap();
    doc.set_path_style(0, 0, Some([0.0, 0.0, 0.0]), None, Some(5.0))
        .unwrap();
    let paths = reopened(&doc).page_paths(0).unwrap();
    assert_eq!(paths.len(), 2);
    assert_eq!(paths[1].fill, Some([0.0, 0.6, 0.0]));
    assert_eq!(paths[0].stroke, Some([0.0, 0.0, 0.0]));
    assert_eq!(paths[0].width, 5.0);
    doc.delete_path(0, 0).unwrap();
    let left = reopened(&doc).page_paths(0).unwrap();
    assert_eq!(left.len(), 1);
    assert_eq!(left[0].fill, Some([0.0, 0.6, 0.0]));
    assert!(doc.move_path_node(0, 5, 0, [0.0, 0.0]).is_err());
}

#[test]
fn the_lines_of_a_template_are_paths() {
    let doc = Document::from_template(Template::Ruled, &TemplateOptions::default()).unwrap();
    let paths = doc.page_paths(0).unwrap();
    assert!(paths.len() > 20, "{}", paths.len());
    let line = &paths[3];
    let mid = [
        (line.bounds[0] + line.bounds[2]) / 2.0,
        (line.bounds[1] + line.bounds[3]) / 2.0,
    ];
    assert!(doc.path_at(0, mid[0], mid[1] + 1.0, 3.0).is_some());
}
