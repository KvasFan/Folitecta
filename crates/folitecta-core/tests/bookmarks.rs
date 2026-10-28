//! Bookmark editing that preserves destinations, actions and styles; named destinations.

use folitecta_core::{DestView, Document};
use lopdf::Object;

fn doc_with_outline() -> Document {
    let mut doc = Document::new_blank(6, 400.0, 500.0).unwrap();
    doc.add_bookmark("Intro", 0, &[]).unwrap();
    doc.add_bookmark("Part A", 1, &[]).unwrap();
    doc.add_bookmark("A.1", 2, &[1]).unwrap();
    doc.add_bookmark("A.2", 3, &[1]).unwrap();
    doc.add_bookmark("Part B", 4, &[]).unwrap();
    doc
}

fn titles(doc: &Document) -> Vec<(usize, String)> {
    doc.outline_flat().into_iter().map(|(d, t, _)| (d, t)).collect()
}

#[test]
fn edits_keep_views_styles_and_actions() {
    let mut doc = doc_with_outline();
    doc.set_bookmark_target(
        &[1],
        1,
        Some(DestView::Xyz {
            left: Some(10.0),
            top: Some(300.0),
            zoom: Some(1.5),
        }),
    )
    .unwrap();
    doc.set_bookmark_style(&[1], Some([1.0, 0.0, 0.0]), true, true)
        .unwrap();
    doc.set_bookmark_uri(&[2], "https://example.org").unwrap();
    doc.set_bookmarks_open(&[vec![1]], true).unwrap();
    // An unrelated edit rewrites the tree: everything above survives.
    doc.rename_bookmark(&[0], "Introduction").unwrap();
    let items = doc.outline();
    assert_eq!(items[0].title, "Introduction");
    let a = &items[1];
    assert_eq!(a.color, Some([1.0, 0.0, 0.0]));
    assert!(a.bold && a.italic && a.open);
    assert_eq!(
        a.view,
        Some(DestView::Xyz {
            left: Some(10.0),
            top: Some(300.0),
            zoom: Some(1.5)
        })
    );
    assert_eq!(items[2].action, "https://example.org");
    assert!(items[2].valid && items[2].page.is_none());
    // Saved and reopened.
    let dir = std::env::temp_dir().join(format!("folitecta-bm-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("bm.pdf");
    doc.save_copy(&path).unwrap();
    let again = Document::open(&path).unwrap();
    let summary = |d: &Document| -> Vec<_> {
        d.outline()
            .into_iter()
            .map(|i| {
                (
                    i.title,
                    i.page,
                    i.view,
                    i.color,
                    i.bold,
                    i.italic,
                    i.open,
                    i.action,
                    i.children.len(),
                )
            })
            .collect()
    };
    assert_eq!(summary(&again), summary(&doc));
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn bookmarks_move_promote_and_demote() {
    let mut doc = doc_with_outline();
    // Part B up: before Part A.
    assert_eq!(doc.move_bookmark_up(&[2]).unwrap(), vec![1]);
    assert_eq!(titles(&doc)[1], (0, "Part B".to_string()));
    // Back down.
    assert_eq!(doc.move_bookmark_down(&[1]).unwrap(), vec![2]);
    assert_eq!(titles(&doc)[4], (0, "Part B".to_string()));
    // A.2 becomes a top-level bookmark after Part A.
    assert_eq!(doc.promote_bookmark(&[1, 1]).unwrap(), vec![2]);
    assert_eq!(titles(&doc)[3], (0, "A.2".to_string()));
    // ... and goes back under Part A.
    assert_eq!(doc.demote_bookmark(&[2]).unwrap(), vec![1, 1]);
    assert_eq!(titles(&doc)[3], (1, "A.2".to_string()));
    // Intro moves into Part B as its first child.
    let moved = doc.move_bookmark(&[0], &[2], 0).unwrap();
    assert_eq!(moved, vec![1, 0]);
    assert_eq!(titles(&doc).last().unwrap(), &(1, "Intro".to_string()));
    // A bookmark cannot move inside itself.
    assert!(doc.move_bookmark(&[0], &[0, 0], 0).is_err());
    assert!(doc.move_bookmark_up(&[0]).is_err());
    assert!(doc.promote_bookmark(&[0]).is_err());
    assert!(doc.demote_bookmark(&[0]).is_err());
}

#[test]
fn deleting_with_or_without_children() {
    let mut doc = doc_with_outline();
    doc.delete_bookmark_keep_children(&[1]).unwrap();
    assert_eq!(
        titles(&doc),
        vec![
            (0, "Intro".into()),
            (0, "A.1".into()),
            (0, "A.2".into()),
            (0, "Part B".into())
        ]
    );
    let mut doc = doc_with_outline();
    doc.delete_bookmark_children(&[1]).unwrap();
    assert_eq!(doc.bookmark_count(), 3);
    doc.delete_bookmarks(&[vec![0], vec![2]]).unwrap();
    assert_eq!(titles(&doc), vec![(0, "Part A".into())]);
}

#[test]
fn sections_invalid_bookmarks_and_text_exchange() {
    let mut doc = doc_with_outline();
    assert_eq!(doc.bookmark_section_pages(&[1]), vec![1, 2, 3]);
    assert_eq!(doc.bookmark_section_pages(&[1, 0]), vec![2]);
    assert_eq!(doc.bookmark_section_pages(&[2]), vec![4, 5]);
    // A bookmark pointing at a missing named destination is invalid.
    let root = doc
        .inner()
        .catalog()
        .unwrap()
        .get(b"Outlines")
        .unwrap()
        .as_reference()
        .unwrap();
    let first = doc
        .inner()
        .get_dictionary(root)
        .unwrap()
        .get(b"First")
        .unwrap()
        .as_reference()
        .unwrap();
    doc.inner_mut()
        .get_dictionary_mut(first)
        .unwrap()
        .set("Dest", Object::string_literal("missing"));
    doc.commit().unwrap();
    assert_eq!(doc.invalid_bookmarks(), vec![vec![0]]);
    let text = doc.export_bookmarks_text(None);
    assert!(text.contains("Part A\t2\n\tA.1\t3\n"), "{text}");
    let selected = doc.export_bookmarks_text(Some(&[vec![1]]));
    assert_eq!(selected, "Part A\t2\n\tA.1\t3\n\tA.2\t4\n");
    let mut other = Document::new_blank(6, 400.0, 500.0).unwrap();
    assert_eq!(other.import_bookmarks_text(&selected).unwrap(), 3);
    assert_eq!(other.outline()[0].children.len(), 2);
    assert_eq!(other.outline()[0].children[1].page, Some(3));
    // Indentation with spaces works too.
    let parsed = other.parse_bookmarks_text("One\t1\n  Two\t2\n    Three\n");
    assert_eq!(parsed[0].children[0].children[0].title, "Three");
}

#[test]
fn named_destinations_are_edited_and_references_follow_renames() {
    let mut doc = Document::new_blank(3, 400.0, 500.0).unwrap();
    doc.add_named_destination("intro", 0, None).unwrap();
    doc.add_named_destination("end", 2, Some(DestView::Fit)).unwrap();
    assert!(doc.add_named_destination("intro", 1, None).is_err());
    let names: Vec<_> = doc
        .named_destinations()
        .into_iter()
        .map(|d| (d.name, d.page))
        .collect();
    assert_eq!(names, vec![("end".into(), Some(2)), ("intro".into(), Some(0))]);
    assert_eq!(doc.named_destinations()[0].view, Some(DestView::Fit));
    doc.set_named_destination("intro", 1, None).unwrap();
    assert_eq!(doc.named_destinations()[1].page, Some(1));
    // A bookmark and a link use the destination by name.
    doc.add_bookmark("Go", 0, &[]).unwrap();
    let root = doc
        .inner()
        .catalog()
        .unwrap()
        .get(b"Outlines")
        .unwrap()
        .as_reference()
        .unwrap();
    let first = doc
        .inner()
        .get_dictionary(root)
        .unwrap()
        .get(b"First")
        .unwrap()
        .as_reference()
        .unwrap();
    doc.inner_mut()
        .get_dictionary_mut(first)
        .unwrap()
        .set("Dest", Object::string_literal("intro"));
    doc.commit().unwrap();
    assert_eq!(doc.outline()[0].page, Some(1));
    doc.rename_named_destination("intro", "start").unwrap();
    assert_eq!(doc.outline()[0].page, Some(1), "the bookmark follows the rename");
    assert!(doc.named_destinations().iter().any(|d| d.name == "start"));
    // A destination on a deleted page goes with the page.
    doc.delete_pages(&[2]).unwrap();
    assert_eq!(doc.named_destinations().len(), 1);
    assert_eq!(doc.remove_invalid_named_destinations().unwrap(), 0);
    assert_eq!(doc.delete_named_destinations(&["start".into()]).unwrap(), 1);
    assert!(doc.named_destinations().is_empty());
}
