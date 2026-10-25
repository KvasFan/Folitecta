//! The text of a document as it is read: paragraphs in order with their
//! place, headings by size, running heads and page numbers left out.

use folitecta_core::{Document, MarkPosition, TextLayout};

#[test]
fn documents_are_read_paragraph_by_paragraph() {
    let p = "The quick brown fox jumps over the lazy dog and runs into the woods again. ".repeat(16);
    let md = format!(
        "# Report\n\n{p}\n\n{p}\n\n## Part one\n\nA short paragraph.\n\n{p}\n\n{p}\n\n{p}\n\n## Part two\n\n{p}\n\n{p}\n\n{p}\n\n{p}\n"
    );
    let mut doc = Document::from_markdown(&md, TextLayout::default(), "").unwrap();
    let all: Vec<usize> = (0..doc.page_count()).collect();
    doc.add_page_numbers(
        &all,
        "Report 2026 — {page}",
        1,
        MarkPosition::BottomCenter,
        9.0,
        20.0,
        [0.0; 3],
    )
    .unwrap();
    assert!(doc.page_count() >= 3, "{}", doc.page_count());
    let blocks = doc.reading_blocks(0..doc.page_count());
    let headings: Vec<(u8, &str)> = blocks
        .iter()
        .filter(|b| b.level > 0)
        .map(|b| (b.level, b.text.as_str()))
        .collect();
    assert_eq!(headings, [(1, "Report"), (2, "Part one"), (2, "Part two")]);
    // No running foot, every paragraph a whole one.
    assert!(
        blocks.iter().all(|b| !b.text.starts_with("Report 2026")),
        "{blocks:?}"
    );
    assert!(blocks.iter().any(|b| b.text == "A short paragraph."));
    let long = blocks
        .iter()
        .find(|b| b.level == 0 && b.text.len() > 400)
        .unwrap();
    assert!(long.text.starts_with("The quick brown fox") && !long.text.contains('\n'));
    // In order, each in its page's text.
    assert!(
        blocks
            .windows(2)
            .all(|w| (w[0].page, w[0].start) < (w[1].page, w[1].start))
    );
    let b = &blocks[1];
    let chars: Vec<char> = doc.text_layer(b.page).unwrap().text.chars().collect();
    let slice: String = chars[b.start..b.end].iter().collect();
    assert!(slice.starts_with("The quick brown fox"), "{slice}");
    // Some pages only.
    let last = doc.page_count() - 1;
    assert!(doc.reading_blocks(last..last + 1).iter().all(|b| b.page == last));
}
