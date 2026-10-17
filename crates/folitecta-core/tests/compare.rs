//! Comparing documents word by word.

use folitecta_core::Document;
use folitecta_core::compare::DiffKind;

fn doc(text: &str) -> Document {
    Document::from_text(text, Default::default()).unwrap()
}

#[test]
fn insertions_deletions_and_changes_are_found() {
    let old = doc(
        "The quick brown fox jumps over the lazy dog.\n\nA second paragraph stays the same.\u{c}Page two old ending.",
    );
    let new = doc(
        "The quick red fox jumps over the dog.\n\nA second paragraph stays the same and grows.\u{c}Page two new ending.",
    );
    let c = old.compare_with(&new, false);
    let kinds: Vec<(DiffKind, &str, &str)> = c
        .differences
        .iter()
        .map(|d| (d.kind, d.old_text.as_str(), d.new_text.as_str()))
        .collect();
    assert!(kinds.contains(&(DiffKind::Changed, "brown", "red")), "{kinds:?}");
    assert!(kinds.contains(&(DiffKind::Deleted, "lazy", "")), "{kinds:?}");
    assert!(
        kinds.contains(&(DiffKind::Inserted, "", "and grows")),
        "{kinds:?}"
    );
    assert!(kinds.contains(&(DiffKind::Changed, "old", "new")), "{kinds:?}");
    let last = c.differences.iter().find(|d| d.old_text == "old").unwrap();
    assert_eq!((last.old_page, last.new_page), (1, 1));
    assert!(!last.old[0].rects.is_empty() && !last.new[0].rects.is_empty());
    assert_eq!(c.new_pages_changed(), vec![0, 1]);
    assert_eq!(c.count(DiffKind::Changed), 2);
    // Case can be ignored.
    let upper = doc("THE QUICK BROWN FOX");
    let lower = doc("the quick brown fox");
    assert_eq!(upper.compare_with(&lower, false).differences.len(), 1);
    assert!(upper.compare_with(&lower, true).differences.is_empty());
    // Identical documents.
    assert!(old.compare_with(&old, false).differences.is_empty());
}
