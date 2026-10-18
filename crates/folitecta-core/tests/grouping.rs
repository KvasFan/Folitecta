//! Groups of comments: made, merged, broken up, moved and deleted together,
//! each in one undo step, written as `/IRT` and `/RT /Group`.

use folitecta_core::marks::pdf_rect_to_display;
use folitecta_core::{Document, Shape};

/// Three boxes on a page of 400 × 300 (display space, y down), the page turned or not.
fn three(turn: i32) -> Document {
    let mut doc = Document::new_blank(1, 400.0, 300.0).unwrap();
    if turn != 0 {
        doc.rotate_pages(&[0], turn).unwrap();
    }
    let size = doc.page_size(0).unwrap();
    for r in [
        [30.0, 40.0, 90.0, 80.0],
        [150.0, 100.0, 230.0, 160.0],
        [260.0, 20.0, 300.0, 50.0],
    ] {
        let pdf = folitecta_core::textops::display_rect_to_pdf(&size, r);
        doc.add_shape(0, Shape::Square, pdf, [1.0, 0.0, 0.0], None, 1.0, "Ann", "")
            .unwrap();
    }
    doc.clear_history();
    doc
}

fn shown(doc: &Document) -> Vec<[f32; 4]> {
    let size = doc.page_size(0).unwrap();
    doc.annotations(0)
        .unwrap()
        .iter()
        .map(|a| pdf_rect_to_display(&size, a.rect).map(|v| (v * 10.0).round() / 10.0))
        .collect()
}

#[test]
fn comments_are_grouped_merged_and_ungrouped() {
    let mut doc = three(0);
    assert_eq!(doc.annotation_group(0, 0), [0]);
    assert!(doc.group_annotations(0, &[0]).is_err());
    // The first one chosen stands for the group.
    assert_eq!(doc.group_annotations(0, &[1, 0]).unwrap(), [1, 0]);
    assert_eq!(doc.annotation_group(0, 0), [0, 1]);
    assert_eq!(doc.annotation_group(0, 1), [1, 0]);
    assert!(doc.is_grouped(0, 0) && !doc.is_grouped(0, 2));
    // Members are not replies: every comment is still on the page.
    assert!(doc.annotations(0).unwrap().iter().all(|a| !a.is_reply));
    // Grouping with a member of a group takes the whole group.
    assert_eq!(doc.group_annotations(0, &[2, 0]).unwrap(), [2, 0, 1]);
    assert_eq!(doc.annotation_group(0, 1), [1, 0, 2]);
    // The group survives saving and opening again.
    let reopened = Document::from_bytes(doc.bytes().to_vec(), None, None).unwrap();
    assert_eq!(reopened.annotation_group(0, 0), [0, 1, 2]);
    // Broken up from any of its comments.
    assert_eq!(doc.ungroup_annotations(0, &[1]).unwrap(), 2);
    assert_eq!(doc.annotation_group(0, 0), [0]);
    assert_eq!(doc.ungroup_annotations(0, &[0]).unwrap(), 0);
    // One undo step each.
    doc.undo().unwrap();
    assert_eq!(doc.annotation_group(0, 2), [2, 0, 1]);
    doc.undo().unwrap();
    assert_eq!(doc.annotation_group(0, 1), [1, 0]);
}

#[test]
fn comments_move_together_as_the_page_is_shown() {
    for turn in [0, 90] {
        let mut doc = three(turn);
        let before = shown(&doc);
        doc.shift_annotations(0, &[0, 2], 10.0, -5.0).unwrap();
        let after = shown(&doc);
        for i in [0, 2] {
            let (a, b) = (before[i], after[i]);
            assert!(
                (b[0] - a[0] - 10.0).abs() < 0.6 && (b[1] - a[1] + 5.0).abs() < 0.6,
                "turn {turn}: {a:?} -> {b:?}"
            );
        }
        assert_eq!(before[1], after[1]);
        doc.undo().unwrap();
        assert_eq!(shown(&doc), before);
    }
}

#[test]
fn a_group_is_deleted_with_its_first_comment() {
    let mut doc = three(0);
    doc.group_annotations(0, &[0, 1]).unwrap();
    // Deleting a member leaves the others.
    assert_eq!(doc.delete_annotations(0, &[1]).unwrap(), 1);
    assert_eq!(doc.annotations(0).unwrap().len(), 2);
    doc.undo().unwrap();
    // Deleting the first takes the whole group, in one step.
    doc.delete_annotations(0, &[0]).unwrap();
    assert_eq!(doc.annotations(0).unwrap().len(), 1);
    doc.undo().unwrap();
    assert_eq!(doc.annotations(0).unwrap().len(), 3);
}
