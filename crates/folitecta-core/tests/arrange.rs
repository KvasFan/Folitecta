//! Arranging comments: aligned, spread evenly, given the same size and
//! stacked, as the page is shown (turned pages included), one undo step each.

use folitecta_core::arrange::{AlignTo, Axis, Edge, SameSize, SpreadBy, Stack};
use folitecta_core::marks::pdf_rect_to_display;
use folitecta_core::{Document, Shape};

/// Three boxes of different sizes on a page of 400 × 300 (display space, y down).
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
        // Display rectangles (y down) as PDF ones.
        let pdf = folitecta_core::textops::display_rect_to_pdf(&size, r);
        doc.add_shape(0, Shape::Square, pdf, [1.0, 0.0, 0.0], None, 1.0, "Ann", "")
            .unwrap();
    }
    doc
}

/// The rectangles of the comments as the page shows them, rounded.
fn shown(doc: &Document) -> Vec<[f32; 4]> {
    let size = doc.page_size(0).unwrap();
    doc.annotations(0)
        .unwrap()
        .iter()
        .map(|a| pdf_rect_to_display(&size, a.rect).map(|v| (v * 10.0).round() / 10.0))
        .collect()
}

#[test]
fn comments_are_aligned_on_an_edge_or_a_middle() {
    for turn in [0, 90] {
        let mut doc = three(turn);
        let before = shown(&doc);
        doc.align_annotations(0, &[0, 1, 2], Edge::Left, AlignTo::Selection)
            .unwrap();
        let r = shown(&doc);
        assert!(r.iter().all(|x| (x[0] - 30.0).abs() < 0.6), "turn {turn}: {r:?}");
        // Sizes are kept.
        for (a, b) in before.iter().zip(&r) {
            assert!(
                ((a[2] - a[0]) - (b[2] - b[0])).abs() < 0.6 && ((a[3] - a[1]) - (b[3] - b[1])).abs() < 0.6
            );
        }
        doc.align_annotations(0, &[0, 1, 2], Edge::Bottom, AlignTo::Page)
            .unwrap();
        let size = doc.page_size(0).unwrap();
        assert!(
            shown(&doc).iter().all(|x| (x[3] - size.height).abs() < 0.6),
            "turn {turn}"
        );
        // The middle of the first one chosen.
        doc.align_annotations(0, &[1, 0, 2], Edge::Center, AlignTo::First)
            .unwrap();
        let r = shown(&doc);
        let mid = |x: &[f32; 4]| (x[0] + x[2]) / 2.0;
        assert!((mid(&r[0]) - mid(&r[1])).abs() < 0.6 && (mid(&r[2]) - mid(&r[1])).abs() < 0.6);
        // One undo step each.
        doc.undo().unwrap();
        doc.undo().unwrap();
        doc.undo().unwrap();
        assert_eq!(shown(&doc), before);
    }
    let mut doc = three(0);
    assert!(
        doc.align_annotations(0, &[0], Edge::Left, AlignTo::Selection)
            .is_err()
    );
    // As a group: moved together until the last one touches the right edge.
    let before = shown(&doc);
    doc.align_annotations(0, &[0, 1, 2], Edge::Right, AlignTo::PageAsGroup)
        .unwrap();
    let after = shown(&doc);
    let dx = 400.0 - 300.0;
    for (a, b) in before.iter().zip(&after) {
        assert!(
            (b[0] - a[0] - dx).abs() < 0.6 && (b[1] - a[1]).abs() < 0.6,
            "{after:?}"
        );
    }
}

#[test]
fn comments_are_spread_evenly() {
    let mut doc = three(0);
    doc.distribute_annotations(0, &[0, 1, 2], Axis::Across, SpreadBy::Start, false)
        .unwrap();
    let r = shown(&doc);
    // The left edges 30, 145, 260.
    assert!((r[1][0] - 145.0).abs() < 0.6, "{r:?}");
    doc.distribute_annotations(0, &[2, 0, 1], Axis::Across, SpreadBy::Space, false)
        .unwrap();
    let r = shown(&doc);
    let gap1 = r[1][0] - r[0][2];
    let gap2 = r[2][0] - r[1][2];
    assert!((gap1 - gap2).abs() < 0.6, "{r:?}");
    // From one side of the page to the other, down the page.
    doc.distribute_annotations(0, &[0, 1, 2], Axis::Down, SpreadBy::Middle, true)
        .unwrap();
    let r = shown(&doc);
    let mut tops: Vec<[f32; 4]> = r.clone();
    tops.sort_by(|a, b| a[1].total_cmp(&b[1]));
    assert!(
        tops[0][1].abs() < 0.6 && (tops[2][3] - 300.0).abs() < 0.6,
        "{r:?}"
    );
    assert!(
        doc.distribute_annotations(0, &[0, 1], Axis::Across, SpreadBy::Start, false)
            .is_err()
    );
}

#[test]
fn comments_take_the_size_of_the_first_and_change_their_stacking() {
    let mut doc = three(0);
    doc.size_annotations(0, &[1, 0, 2], SameSize::Both).unwrap();
    let r = shown(&doc);
    for x in &r {
        assert!(
            ((x[2] - x[0]) - 80.0).abs() < 0.6 && ((x[3] - x[1]) - 60.0).abs() < 0.6,
            "{r:?}"
        );
    }
    // Top-left corners are kept.
    assert!((r[0][0] - 30.0).abs() < 0.6 && (r[0][1] - 40.0).abs() < 0.6);
    doc.size_annotations(0, &[0, 2], SameSize::Width).unwrap();

    let ids = |doc: &Document| {
        doc.annotations(0)
            .unwrap()
            .iter()
            .map(|a| a.id)
            .collect::<Vec<_>>()
    };
    let start = ids(&doc);
    assert_eq!(doc.stack_annotations(0, &[0], Stack::Front).unwrap(), vec![2]);
    assert_eq!(ids(&doc), vec![start[1], start[2], start[0]]);
    assert_eq!(doc.stack_annotations(0, &[2], Stack::Backward).unwrap(), vec![1]);
    assert_eq!(ids(&doc), vec![start[1], start[0], start[2]]);
    assert_eq!(
        doc.stack_annotations(0, &[1, 2], Stack::Back).unwrap(),
        vec![0, 1]
    );
    assert_eq!(ids(&doc), vec![start[0], start[2], start[1]]);
    // Already at the front: nothing changes, no undo step.
    assert_eq!(doc.stack_annotations(0, &[2], Stack::Forward).unwrap(), vec![2]);
    doc.undo().unwrap();
    assert_eq!(ids(&doc), vec![start[1], start[0], start[2]]);
}
