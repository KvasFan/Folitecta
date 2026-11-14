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

#[test]
fn nodes_are_added_and_removed() {
    let mut doc = drawing();
    let at = |doc: &Document| -> Vec<[f32; 2]> {
        doc.page_paths(0).unwrap()[0].nodes.iter().map(|n| n.at).collect()
    };
    assert_eq!(at(&doc), vec![[20.0, 20.0], [120.0, 20.0], [70.0, 100.0]]);
    // A node after the first: a straight segment to it.
    doc.add_path_node(0, 0, 0, [70.0, 0.0]).unwrap();
    assert_eq!(
        at(&doc),
        vec![[20.0, 20.0], [70.0, 0.0], [120.0, 20.0], [70.0, 100.0]]
    );
    // The node at the far corner goes: its neighbours are joined.
    doc.remove_path_node(0, 0, 2).unwrap();
    assert_eq!(at(&doc), vec![[20.0, 20.0], [70.0, 0.0], [70.0, 100.0]]);
    // The first goes: the path starts at the next one.
    doc.remove_path_node(0, 0, 0).unwrap();
    assert_eq!(at(&reopened(&doc)), vec![[70.0, 0.0], [70.0, 100.0]]);
    // Two nodes stay; a rectangle has none to add or remove.
    assert!(doc.remove_path_node(0, 0, 0).is_err());
    assert!(doc.add_path_node(0, 1, 0, [10.0, 10.0]).is_err());
    assert!(doc.remove_path_node(0, 1, 0).is_err());
    // Each change one undo step.
    doc.undo().unwrap();
    doc.undo().unwrap();
    doc.undo().unwrap();
    assert_eq!(at(&doc), vec![[20.0, 20.0], [120.0, 20.0], [70.0, 100.0]]);
}

#[test]
fn drawings_snap_to_the_corners_and_ends_of_the_paths() {
    let doc = drawing();
    let points = doc.snap_points(0);
    // The triangle's three nodes and the middles of its sides (the one that
    // closes it too), the rectangle's four corners and the middles of its
    // sides; nothing else (no two sides cross).
    for p in [
        [20.0, 20.0],
        [120.0, 20.0],
        [70.0, 100.0],
        [70.0, 20.0],
        [95.0, 60.0],
        [45.0, 60.0],
        [150.0, 150.0],
        [210.0, 150.0],
        [210.0, 190.0],
        [150.0, 190.0],
        [180.0, 150.0],
        [210.0, 170.0],
        [180.0, 190.0],
        [150.0, 170.0],
    ] {
        assert!(points.contains(&p), "{p:?} in {points:?}");
    }
    assert_eq!(points.len(), 14, "{points:?}");
}

#[test]
fn pictures_and_paths_deleted_together_in_one_step() {
    let mut doc = Document::from_bytes(folitecta_core::testdocs::text_doc(1), None, None).unwrap();
    let mut png = std::io::Cursor::new(Vec::new());
    image::RgbaImage::from_pixel(10, 10, image::Rgba([0, 0, 255, 255]))
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
    doc.add_image(0, png.into_inner(), Some([100.0, 100.0, 150.0, 150.0]))
        .unwrap();
    doc.add_three_point_shape(
        0,
        folitecta_core::ThreePoint::Circle,
        [[300.0, 300.0], [350.0, 350.0], [400.0, 300.0]],
        [0.0; 3],
        1.0,
    )
    .unwrap();
    doc.add_three_point_shape(
        0,
        folitecta_core::ThreePoint::Rect,
        [[100.0, 500.0], [200.0, 500.0], [200.0, 560.0]],
        [0.0; 3],
        1.0,
    )
    .unwrap();
    let (images, paths) = (
        doc.page_images(0).unwrap().len(),
        doc.page_paths(0).unwrap().len(),
    );
    assert!(images >= 1 && paths >= 2);
    doc.delete_content_items(0, &[images - 1], &[paths - 2, paths - 1])
        .unwrap();
    assert_eq!(doc.page_images(0).unwrap().len(), images - 1);
    assert_eq!(doc.page_paths(0).unwrap().len(), paths - 2);
    doc.undo().unwrap();
    assert_eq!(doc.page_images(0).unwrap().len(), images);
    assert_eq!(doc.page_paths(0).unwrap().len(), paths);
}
