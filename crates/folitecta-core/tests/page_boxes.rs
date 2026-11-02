//! The bleed, trim and art boxes of pages: set, kept on saving, taken away;
//! what a page shows does not change.

use folitecta_core::Document;
use folitecta_core::pages::PageBox;

#[test]
fn boxes_are_set_and_taken_away_in_one_step() {
    let mut doc = Document::new_blank(2, 300.0, 400.0).unwrap();
    let shown = doc.page_size(0).unwrap();
    doc.set_page_box(&[0], PageBox::Trim, Some([10.0, 20.0, 30.0, 40.0]))
        .unwrap();
    let boxes = doc.page_boxes(0).unwrap();
    assert_eq!(boxes.trim, Some([10.0, 40.0, 270.0, 380.0]));
    assert_eq!(boxes.bleed, None);
    assert_eq!(doc.page_boxes(1).unwrap().trim, None);
    assert_eq!(doc.page_size(0).unwrap(), shown);

    let again = Document::from_bytes(doc.bytes().to_vec(), None, None).unwrap();
    assert_eq!(
        again.page_boxes(0).unwrap().trim,
        Some([10.0, 40.0, 270.0, 380.0])
    );

    doc.set_page_box(&[0], PageBox::Trim, None).unwrap();
    assert_eq!(doc.page_boxes(0).unwrap().trim, None);
    doc.undo().unwrap();
    assert!(doc.page_boxes(0).unwrap().trim.is_some());
}

#[test]
fn a_box_that_does_not_fit_changes_nothing() {
    let mut doc = Document::new_blank(1, 100.0, 100.0).unwrap();
    assert!(
        doc.set_page_box(&[0], PageBox::Art, Some([60.0, 0.0, 60.0, 0.0]))
            .is_err()
    );
    assert_eq!(doc.page_boxes(0).unwrap().art, None);
    assert!(!doc.can_undo());
    assert!(doc.set_page_box(&[3], PageBox::Art, None).is_err());
}

#[test]
fn a_crop_that_leaves_nothing_leaves_no_edit_open() {
    let mut doc = Document::new_blank(1, 100.0, 100.0).unwrap();
    assert!(doc.crop_pages_by_margins(&[0], 60.0, 0.0, 60.0, 0.0).is_err());
    assert!(!doc.can_undo());
    doc.rotate_pages(&[0], 90).unwrap();
    doc.undo().unwrap();
    assert_eq!(doc.page_size(0).unwrap().rotation, 0);
}
