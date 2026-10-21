//! Tables of contents: headings found by the size of their text (running
//! heads left out, lines of one heading joined) made into nested bookmarks,
//! and contents pages listing the bookmarks with the numbers the pages have
//! once they are in, every entry a link to its place.

use folitecta_core::links::LinkTarget;
use folitecta_core::{Document, MarkPosition, TextLayout};

fn para() -> String {
    "The quick brown fox jumps over the lazy dog and runs into the woods again. ".repeat(14)
}

/// A report of several pages with three levels of headings, a long one on
/// two lines, and a running head on every page.
fn report() -> Document {
    let long = "A heading long enough to be set on two lines across the width of the page";
    let md = format!(
        "# Annual report\n\n{p}\n\n## Introduction\n\n{p}\n\n{p}\n\n## Results\n\n{p}\n\n### Sales\n\n{p}\n\n{p}\n\n### Costs\n\n{p}\n\n## {long}\n\n{p}\n\n{p}\n",
        p = para()
    );
    let mut doc = Document::from_markdown(&md, TextLayout::default(), "").unwrap();
    let all: Vec<usize> = (0..doc.page_count()).collect();
    doc.add_page_numbers(
        &all,
        "Annual report 2026 — page {page}",
        1,
        MarkPosition::TopCenter,
        16.0,
        20.0,
        [0.0; 3],
    )
    .unwrap();
    // Without the bookmarks the Markdown made: they are found again.
    doc.set_outline(&[]).unwrap();
    doc.clear_history();
    doc
}

#[test]
fn headings_are_found_by_their_size() {
    let doc = report();
    assert!(doc.page_count() >= 3, "{}", doc.page_count());
    let found = doc.find_headings();
    let titles: Vec<(u8, &str)> = found.iter().map(|h| (h.level, h.title.as_str())).collect();
    assert_eq!(
        titles,
        [
            (1, "Annual report"),
            (2, "Introduction"),
            (2, "Results"),
            (3, "Sales"),
            (3, "Costs"),
            (
                2,
                "A heading long enough to be set on two lines across the width of the page"
            ),
        ],
        "{found:?}"
    );
    // On their pages, in order.
    assert!(
        found
            .windows(2)
            .all(|w| (w[0].page, w[0].top) <= (w[1].page, w[1].top))
    );
}

#[test]
fn headings_become_nested_bookmarks_in_one_step() {
    let mut doc = report();
    let found = doc.find_headings();
    assert_eq!(doc.bookmarks_from_headings(&found, true).unwrap(), 6);
    let outline = doc.outline();
    assert_eq!(outline.len(), 1);
    let second: Vec<&str> = outline[0].children.iter().map(|o| o.title.as_str()).collect();
    assert_eq!(second.len(), 3);
    assert_eq!(outline[0].children[1].children.len(), 2);
    assert_eq!(outline[0].children[1].children[1].title, "Costs");
    let costs = &outline[0].children[1].children[1];
    assert_eq!(costs.page, Some(found[4].page));
    // Added after the bookmarks there are, then undone in one step.
    assert_eq!(doc.bookmarks_from_headings(&found[..1], false).unwrap(), 1);
    assert_eq!(doc.outline().len(), 2);
    doc.undo().unwrap();
    assert_eq!(doc.outline().len(), 1);
    doc.undo().unwrap();
    assert!(doc.outline().is_empty());
    assert!(doc.bookmarks_from_headings(&[], true).is_err());
}

#[test]
fn contents_list_the_bookmarks_with_their_new_page_numbers_and_links() {
    let mut doc = report();
    let found = doc.find_headings();
    doc.bookmarks_from_headings(&found, true).unwrap();
    let before = doc.page_count();
    let made = doc.insert_contents(0, "Contents").unwrap();
    assert_eq!(made, 1);
    assert_eq!(doc.page_count(), before + 1);
    let text = doc.page_text(0);
    // Read in order: the title, then each entry before its dots and number.
    assert!(text.starts_with("Contents Annual report . ."), "{text}");
    // Each entry with the number its page has now.
    let costs = found[4].page + 2;
    assert!(
        text.contains("Costs . ") && text.contains(&format!(" {costs}")),
        "{text}"
    );
    assert!(text.contains("A heading long enough"), "{text}");
    // Every entry a link to its page.
    let targets: Vec<LinkTarget> = doc.links(0).unwrap().into_iter().map(|l| l.target).collect();
    assert_eq!(targets.len(), 6, "{targets:?}");
    assert_eq!(targets[4], LinkTarget::Page(found[4].page + 1));
    assert_eq!(targets[0], LinkTarget::Page(found[0].page + 1));
    // The bookmarks still open their pages.
    assert_eq!(
        doc.outline()[0].children[1].children[1].page,
        Some(found[4].page + 1)
    );
    doc.undo().unwrap();
    assert_eq!(doc.page_count(), before);
}

#[test]
fn contents_need_bookmarks() {
    let mut doc = Document::from_text("No headings here.", TextLayout::default()).unwrap();
    assert!(doc.find_headings().is_empty());
    assert!(doc.insert_contents(0, "Contents").is_err());
    assert!(!doc.can_undo());
}
