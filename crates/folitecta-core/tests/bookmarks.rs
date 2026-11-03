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

#[test]
fn duplicate_bookmarks_go_and_their_children_stay() {
    let mut doc = doc_with_outline();
    // "Part A" again, with a child of its own and a second "A.1".
    doc.add_bookmark("Part A", 1, &[]).unwrap();
    doc.add_bookmark("A.3", 5, &[3]).unwrap();
    doc.add_bookmark("A.1", 2, &[3]).unwrap();
    // The same title opening another page is no duplicate.
    doc.add_bookmark("Intro", 5, &[]).unwrap();
    doc.clear_history();
    assert_eq!(doc.duplicate_bookmarks(), [vec![3], vec![3, 1]]);
    assert_eq!(doc.remove_duplicate_bookmarks().unwrap(), 2);
    assert_eq!(
        titles(&doc),
        [
            (0, "Intro".to_string()),
            (0, "Part A".to_string()),
            (1, "A.1".to_string()),
            (1, "A.2".to_string()),
            (0, "Part B".to_string()),
            (0, "A.3".to_string()),
            (0, "Intro".to_string()),
        ]
    );
    assert!(doc.duplicate_bookmarks().is_empty());
    // Nothing more to remove: no undo step.
    assert_eq!(doc.remove_duplicate_bookmarks().unwrap(), 0);
    // One undo step brings them back.
    doc.undo().unwrap();
    assert_eq!(doc.bookmark_count(), 9);
    assert!(!doc.can_undo());
}

#[test]
fn bookmarks_to_the_same_page_at_other_places_are_no_duplicates() {
    let mut doc = Document::new_blank(2, 400.0, 500.0).unwrap();
    let view = |top| {
        Some(DestView::Xyz {
            left: None,
            top: Some(top),
            zoom: None,
        })
    };
    let a = doc.new_bookmark_item("Notes", 1, view(400.0)).unwrap();
    let b = doc.new_bookmark_item("Notes", 1, view(400.5)).unwrap();
    let c = doc.new_bookmark_item("Notes", 1, view(120.0)).unwrap();
    doc.set_outline(&[a, b, c]).unwrap();
    // Half a point lower is the same place; the foot of the page is not.
    assert_eq!(doc.duplicate_bookmarks(), [vec![1]]);
}

#[test]
fn article_threads_are_read_bead_by_bead() {
    use lopdf::{Object, dictionary};
    let mut doc = Document::new_blank(3, 400.0, 500.0).unwrap();
    let pages: Vec<lopdf::ObjectId> = doc.inner().get_pages().into_values().collect();
    {
        let inner = doc.inner_mut();
        // "Story": from the top of page 1 to page 3; a second thread of one bead.
        let story = inner.new_object_id();
        let (b1, b2) = (inner.new_object_id(), inner.new_object_id());
        let bead = |thread, page, rect: [f32; 4], next, prev| {
            Object::Dictionary(dictionary! {
                "Type" => "Bead",
                "T" => Object::Reference(thread),
                "P" => Object::Reference(page),
                "R" => rect.iter().map(|v| Object::Real(*v)).collect::<Vec<_>>(),
                "N" => Object::Reference(next),
                "V" => Object::Reference(prev),
            })
        };
        inner
            .objects
            .insert(b1, bead(story, pages[0], [36.0, 250.0, 364.0, 464.0], b2, b2));
        inner
            .objects
            .insert(b2, bead(story, pages[2], [36.0, 36.0, 364.0, 250.0], b1, b1));
        inner.objects.insert(
            story,
            Object::Dictionary(dictionary! {
                "Type" => "Thread",
                "F" => Object::Reference(b1),
                "I" => dictionary! { "Title" => Object::string_literal("Story") },
            }),
        );
        let other = inner.new_object_id();
        let b3 = inner.new_object_id();
        inner
            .objects
            .insert(b3, bead(other, pages[1], [0.0, 0.0, 400.0, 500.0], b3, b3));
        inner.objects.insert(
            other,
            Object::Dictionary(dictionary! { "Type" => "Thread", "F" => Object::Reference(b3) }),
        );
        inner.catalog_mut().unwrap().set(
            "Threads",
            vec![Object::Reference(story), Object::Reference(other)],
        );
    }
    doc.commit().unwrap();
    let articles = doc.articles();
    assert_eq!(articles.len(), 2);
    assert_eq!(articles[0].title, "Story");
    assert_eq!(
        articles[0].beads,
        [(0, [36.0, 250.0, 364.0, 464.0]), (2, [36.0, 36.0, 364.0, 250.0])]
    );
    assert_eq!(articles[1].title, "");
    assert_eq!(articles[1].beads, [(1, [0.0, 0.0, 400.0, 500.0])]);
    // Saved and read again.
    let again = Document::from_bytes(doc.bytes().as_ref().clone(), None, None).unwrap();
    assert_eq!(again.articles(), articles);
    assert!(
        Document::new_blank(1, 100.0, 100.0)
            .unwrap()
            .articles()
            .is_empty()
    );
}

#[test]
fn article_threads_are_made_in_one_step() {
    let mut doc = Document::new_blank(2, 400.0, 500.0).unwrap();
    doc.add_article(
        "Lead",
        &[(0, [36.0, 36.0, 200.0, 464.0]), (1, [36.0, 36.0, 364.0, 464.0])],
    )
    .unwrap();
    let a = doc.articles();
    assert_eq!(a.len(), 1);
    assert_eq!(a[0].title, "Lead");
    assert_eq!(a[0].beads.len(), 2);
    assert!(doc.add_article("", &[]).is_err());
    assert!(doc.add_article("", &[(5, [0.0; 4])]).is_err());
    doc.undo().unwrap();
    assert!(doc.articles().is_empty());
}

#[test]
fn article_threads_come_with_the_pages_of_another_file() {
    let mut other = Document::new_blank(2, 400.0, 500.0).unwrap();
    other
        .add_article(
            "Feature",
            &[(0, [36.0, 36.0, 364.0, 464.0]), (1, [36.0, 36.0, 364.0, 464.0])],
        )
        .unwrap();
    let mut doc = Document::new_blank(1, 400.0, 500.0).unwrap();
    doc.add_article("Lead", &[(0, [36.0, 36.0, 364.0, 464.0])])
        .unwrap();
    doc.insert_document(1, &other, None).unwrap();
    let articles = doc.articles();
    let titles: Vec<&str> = articles.iter().map(|a| a.title.as_str()).collect();
    assert_eq!(titles, ["Lead", "Feature"]);
    let pages: Vec<usize> = articles[1].beads.iter().map(|b| b.0).collect();
    assert_eq!(pages, [1, 2]);
    // Saved and read again.
    let again = Document::from_bytes(doc.bytes().as_ref().clone(), None, None).unwrap();
    assert_eq!(again.articles(), articles);
    // Only one of its pages: the thread that ran on the other one does not come.
    let mut part = Document::new_blank(1, 400.0, 500.0).unwrap();
    part.insert_document(1, &other, Some(&[0])).unwrap();
    assert!(part.articles().is_empty());
}

#[test]
fn articles_are_renamed_cut_and_deleted_one_step_each() {
    let mut doc = Document::new_blank(3, 400.0, 500.0).unwrap();
    let r = [36.0, 36.0, 364.0, 464.0];
    doc.add_article("Long", &[(0, r), (1, r), (2, r)]).unwrap();
    doc.add_article("Short", &[(1, r)]).unwrap();
    doc.clear_history();
    doc.rename_article(0, "Feature").unwrap();
    assert_eq!(doc.articles()[0].title, "Feature");
    // The first part out: the thread begins with the second.
    doc.delete_article_part(0, 0).unwrap();
    let pages: Vec<usize> = doc.articles()[0].beads.iter().map(|b| b.0).collect();
    assert_eq!(pages, [1, 2]);
    // The middle one out of three would join the others; here the last.
    doc.delete_article_part(0, 1).unwrap();
    let pages: Vec<usize> = doc.articles()[0].beads.iter().map(|b| b.0).collect();
    assert_eq!(pages, [1]);
    // The last part of an article takes it away.
    doc.delete_article_part(1, 0).unwrap();
    assert_eq!(doc.articles().len(), 1);
    doc.delete_article(0).unwrap();
    assert!(doc.articles().is_empty());
    assert!(doc.delete_article(0).is_err());
    // Saved and read again, nothing left behind.
    let again = Document::from_bytes(doc.bytes().as_ref().clone(), None, None).unwrap();
    assert!(again.articles().is_empty());
    for _ in 0..5 {
        doc.undo().unwrap();
    }
    let titles: Vec<String> = doc.articles().into_iter().map(|a| a.title).collect();
    assert_eq!(titles, ["Long", "Short"]);
    assert_eq!(doc.articles()[0].beads.len(), 3);
}

#[test]
fn parts_are_added_to_articles_and_moved_in_them() {
    let mut doc = Document::new_blank(3, 400.0, 500.0).unwrap();
    let r = |x: f32| [x, 36.0, x + 100.0, 464.0];
    doc.add_article("Story", &[(0, r(10.0))]).unwrap();
    doc.clear_history();
    doc.add_article_parts(0, &[(1, r(20.0)), (2, r(30.0))]).unwrap();
    let pages = |doc: &Document| doc.articles()[0].beads.iter().map(|b| b.0).collect::<Vec<_>>();
    assert_eq!(pages(&doc), [0, 1, 2]);
    // The last part first, then the first part last.
    doc.move_article_part(0, 2, 0).unwrap();
    assert_eq!(pages(&doc), [2, 0, 1]);
    doc.move_article_part(0, 0, 2).unwrap();
    assert_eq!(pages(&doc), [0, 1, 2]);
    assert!(doc.move_article_part(0, 3, 0).is_err());
    let again = Document::from_bytes(doc.bytes().as_ref().clone(), None, None).unwrap();
    assert_eq!(pages(&again), [0, 1, 2]);
    doc.undo().unwrap();
    doc.undo().unwrap();
    doc.undo().unwrap();
    assert_eq!(pages(&doc), [0]);
}
