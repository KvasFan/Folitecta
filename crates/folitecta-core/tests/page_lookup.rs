//! The pages a document finds by index (kept for a revision instead of
//! walking the page tree at every call) always are those of the page tree:
//! after every change of the tree, undo and redo, a failed operation, a
//! change made through the object model, and in the file saved and reopened.

use folitecta_core::{Document, Error};
use lopdf::{Object, ObjectId};

/// The pages as the page tree lists them.
fn tree(doc: &Document) -> Vec<ObjectId> {
    doc.inner().get_pages().into_values().collect()
}

/// Every way of finding a page agrees with the page tree.
fn check(doc: &Document, what: &str) {
    let ids = tree(doc);
    assert_eq!(doc.page_ids(), ids, "{what}: page ids");
    assert_eq!(doc.page_count(), ids.len(), "{what}: count");
    for (i, id) in ids.iter().enumerate() {
        assert_eq!(doc.page_id(i).unwrap(), *id, "{what}: page {i}");
        assert_eq!(doc.page_index(*id), Some(i), "{what}: index of page {i}");
    }
    assert!(
        matches!(doc.page_id(ids.len()), Err(Error::InvalidPage(_))),
        "{what}: a page after the last"
    );
    // Asked twice (the second time from what was kept): the same answers.
    assert_eq!(doc.page_ids(), ids, "{what}: page ids again");
}

/// The page sizes give each page its own width, to tell them apart.
fn widths(doc: &Document) -> Vec<f32> {
    doc.page_sizes().iter().map(|s| s.width).collect()
}

#[test]
fn pages_are_found_after_every_change_of_the_page_tree() {
    let mut doc = Document::new_blank(3, 600.0, 800.0).unwrap();
    check(&doc, "new");
    doc.insert_blank_pages(1, 2, 300.0, 400.0).unwrap();
    check(&doc, "insert");
    assert_eq!(widths(&doc), vec![600.0, 300.0, 300.0, 600.0, 600.0]);
    doc.delete_pages(&[0]).unwrap();
    check(&doc, "delete");
    doc.move_pages(&[0], 3).unwrap();
    check(&doc, "move");
    doc.duplicate_pages(&[1]).unwrap();
    check(&doc, "duplicate");
    doc.reverse_pages().unwrap();
    check(&doc, "reverse");
    let after = tree(&doc);
    doc.undo().unwrap();
    check(&doc, "undo");
    doc.undo().unwrap();
    check(&doc, "undo twice");
    doc.redo().unwrap();
    doc.redo().unwrap();
    check(&doc, "redo");
    assert_eq!(tree(&doc), after);
    // A change that is not of the tree keeps the same pages.
    doc.rotate_pages(&[0], 90).unwrap();
    check(&doc, "rotate");
    // Pages of another document.
    let other = Document::new_blank(2, 200.0, 200.0).unwrap();
    let before = doc.page_count();
    doc.insert_document(1, &other, None).unwrap();
    check(&doc, "insert document");
    assert_eq!(doc.page_count(), before + 2);
    assert_eq!(widths(&doc)[1], 200.0);
}

#[test]
fn a_failed_change_leaves_the_pages_found_as_they_are() {
    let mut doc = Document::new_blank(2, 600.0, 800.0).unwrap();
    check(&doc, "new");
    assert!(doc.delete_pages(&[0, 1]).is_err());
    check(&doc, "deleting every page");
    assert!(doc.move_pages(&[7], 0).is_err());
    check(&doc, "moving a page that does not exist");
    assert_eq!(doc.page_count(), 2);
}

#[test]
fn a_change_of_the_object_model_is_seen_at_once() {
    let mut doc = Document::new_blank(3, 600.0, 800.0).unwrap();
    check(&doc, "new");
    // Take the last page out of the tree by hand, without a commit.
    let pages_ref = doc
        .inner()
        .catalog()
        .unwrap()
        .get(b"Pages")
        .unwrap()
        .as_reference()
        .unwrap();
    {
        let inner = doc.inner_mut();
        let pages = inner.get_dictionary_mut(pages_ref).unwrap();
        let mut kids = pages.get(b"Kids").unwrap().as_array().unwrap().clone();
        kids.pop();
        pages.set("Kids", Object::Array(kids));
        pages.set("Count", 2);
    }
    check(&doc, "before the commit");
    assert_eq!(doc.page_count(), 2);
    doc.commit().unwrap();
    check(&doc, "after the commit");
}

#[test]
fn the_saved_file_has_the_same_pages() {
    let dir = std::env::temp_dir().join(format!("folitecta-page-lookup-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("pages.pdf");
    let mut doc = Document::new_blank(4, 600.0, 800.0).unwrap();
    doc.insert_blank_pages(2, 1, 100.0, 100.0).unwrap();
    doc.move_pages(&[2], 0).unwrap();
    doc.delete_pages(&[4]).unwrap();
    check(&doc, "edited");
    let shown = widths(&doc);
    doc.save_as(&path).unwrap();
    let reopened = Document::open(&path).unwrap();
    check(&reopened, "reopened");
    assert_eq!(widths(&reopened), shown);
    assert_eq!(widths(&reopened), vec![100.0, 600.0, 600.0, 600.0]);
    let _ = std::fs::remove_dir_all(&dir);
}
