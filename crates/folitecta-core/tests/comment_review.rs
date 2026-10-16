//! Replies, review states, check marks, flags, pop-up notes and filtered views.

use std::collections::HashSet;

use folitecta_core::comment_review::{FLAG_LOCKED, FLAG_PRINT};
use folitecta_core::{Document, Shape, SummaryOptions};

fn doc_with_comment() -> Document {
    let mut doc = Document::new_blank(1, 400.0, 500.0).unwrap();
    doc.add_shape(
        0,
        Shape::Square,
        [50.0, 300.0, 150.0, 380.0],
        [0.9, 0.1, 0.1],
        None,
        2.0,
        "Ann",
        "please check",
    )
    .unwrap();
    doc
}

#[test]
fn replies_are_threaded_and_deleted_with_their_comment() {
    let mut doc = doc_with_comment();
    doc.add_reply(0, 0, "done", "Bob").unwrap();
    doc.add_reply(0, 0, "thanks", "Ann").unwrap();
    assert!(doc.add_reply(0, 0, "  ", "Ann").is_err());
    let infos = doc.comment_infos();
    assert_eq!(infos.len(), 3);
    let parent = infos[0].id;
    assert!(infos[1..].iter().all(|c| c.reply_to == Some(parent)));
    let annots = doc.annotations(0).unwrap();
    assert!(!annots[0].is_reply && annots[1].is_reply);
    // The summary lists replies under the comment.
    let entries = doc.summary_entries(false, folitecta_core::SummarySort::Page);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].replies.len(), 2);
    doc.delete_annotation(0, 0).unwrap();
    assert!(
        doc.annotations(0).unwrap().is_empty(),
        "replies follow their comment"
    );
}

#[test]
fn review_states_and_check_marks() {
    let mut doc = doc_with_comment();
    doc.set_review_state(0, 0, "Review", "Accepted", "Bob", "Accepted set by Bob")
        .unwrap();
    doc.set_review_state(0, 0, "Marked", "Marked", "Bob", "Marked")
        .unwrap();
    assert!(doc.set_review_state(0, 0, "Review", "Maybe", "Bob", "").is_err());
    let c = &doc.comment_infos()[0];
    assert_eq!(c.review, "Accepted");
    assert!(c.marked);
    // State changes are not drawn but are listed.
    let states: Vec<_> = doc.comment_infos().into_iter().filter(|c| c.is_state).collect();
    assert_eq!(states.len(), 2);
    assert!(states.iter().all(|s| s.hidden));
    let history = doc.review_history(0, 0).unwrap();
    assert_eq!(history.len(), 2);
    assert_eq!(history[0].state, "Accepted");
    // The summary shows the state with the comment even though states are hidden.
    let summary = doc.summary_entries(false, folitecta_core::SummarySort::Page);
    assert_eq!(summary[0].replies.len(), 2);
    assert!(doc.summarize_comments(&SummaryOptions::default()).is_ok());
}

#[test]
fn flags_toggle() {
    let mut doc = doc_with_comment();
    doc.set_annotation_flag(0, 0, FLAG_LOCKED, true).unwrap();
    doc.set_annotation_flag(0, 0, FLAG_PRINT, false).unwrap();
    let c = &doc.comment_infos()[0];
    assert!(c.locked && !c.printable);
    doc.undo().unwrap();
    assert!(doc.comment_infos()[0].printable);
}

#[test]
fn popup_notes_open_close_and_move() {
    let mut doc = doc_with_comment();
    doc.add_note(0, 300.0, 450.0, "", "Cy", [1.0, 0.9, 0.2]).unwrap();
    doc.set_popup_open(0, 0, true).unwrap();
    let c = &doc.comment_infos()[0];
    let (_, open, rect) = c.popup.unwrap();
    assert!(open);
    assert!(rect[0] >= 150.0 && rect[2] <= 400.0, "{rect:?}");
    doc.set_popup_rect(0, 0, [10.0, 10.0, 110.0, 60.0]).unwrap();
    assert_eq!(doc.comment_infos()[0].popup.unwrap().2, [10.0, 10.0, 110.0, 60.0]);
    doc.reset_popup_location(0, 0).unwrap();
    assert_ne!(doc.comment_infos()[0].popup.unwrap().2, [10.0, 10.0, 110.0, 60.0]);
    // Popups are not listed as comments.
    assert_eq!(doc.comment_infos().len(), 2);
    // Open only the non-empty ones: the note without text stays closed.
    doc.set_all_popups_open(false, false).unwrap();
    assert_eq!(doc.set_all_popups_open(true, true).unwrap(), 1);
    let infos = doc.comment_infos();
    assert!(infos[0].popup.unwrap().1);
    assert!(infos[1].popup.is_none());
    doc.render_page_png(0, 1.0).unwrap();
}

#[test]
fn filtered_view_hides_without_changing_the_document() {
    let doc = doc_with_comment();
    let id = doc.comment_infos()[0].id;
    let bytes = doc.bytes_hiding(&HashSet::from([id])).unwrap();
    let filtered = Document::from_bytes(bytes.to_vec(), None, None).unwrap();
    assert!(filtered.comment_infos()[0].hidden);
    assert!(!doc.comment_infos()[0].hidden);
}

#[test]
fn lines_flip_their_ends() {
    let mut doc = Document::new_blank(1, 400.0, 500.0).unwrap();
    doc.add_line(
        0,
        (10.0, 10.0),
        (200.0, 100.0),
        [0.0, 0.0, 1.0],
        1.0,
        true,
        "A",
        "",
    )
    .unwrap();
    doc.flip_line(0, 0).unwrap();
    let style = doc.annotation_style(0, 0).unwrap();
    assert_eq!(style.line_start, "OpenArrow");
    assert_eq!(style.line_end, "None");
}
