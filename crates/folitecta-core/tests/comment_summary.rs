//! Comment summaries in the three layouts.

use folitecta_core::{Document, Shape, SummaryLayout, SummaryOptions, SummarySort};

fn commented() -> Document {
    let mut doc = Document::new_blank(3, 400.0, 500.0).unwrap();
    doc.add_note(
        0,
        30.0,
        470.0,
        "First note with a rather long text that has to wrap over several lines in the summary",
        "Ann",
        [1.0, 0.8, 0.2],
    )
    .unwrap();
    doc.add_shape(
        0,
        Shape::Circle,
        [50.0, 50.0, 150.0, 120.0],
        [0.9, 0.1, 0.1],
        None,
        2.0,
        "Bob",
        "circle",
    )
    .unwrap();
    doc.add_note(2, 100.0, 300.0, "On the last page", "Cy", [0.3, 0.6, 1.0])
        .unwrap();
    doc
}

fn page_text(doc: &Document) -> String {
    (0..doc.page_count())
        .map(|p| doc.page_text(p))
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn comments_only_lists_every_comment() {
    let doc = commented();
    let entries = doc.summary_entries(false, SummarySort::Page);
    assert_eq!(entries.len(), 3);
    assert_eq!(
        entries.iter().map(|e| e.number).collect::<Vec<_>>(),
        vec![1, 2, 3]
    );
    // Reading order: the note at the top comes before the circle.
    assert_eq!(entries[0].author, "Ann");
    let summary = doc.summarize_comments(&SummaryOptions::default()).unwrap();
    assert!(summary.page_count() >= 1);
    let text = page_text(&summary);
    assert!(
        text.contains("circle") && text.contains("On the last page"),
        "{text}"
    );
    assert!(text.contains("Page 1") && text.contains("Page 3"), "{text}");
    summary.render_page_png(0, 1.0).unwrap();
    let by_author = doc.summary_entries(false, SummarySort::Author);
    assert_eq!(
        by_author.iter().map(|e| e.author.as_str()).collect::<Vec<_>>(),
        vec!["Ann", "Bob", "Cy"]
    );
}

#[test]
fn separate_pages_follow_each_commented_page() {
    let doc = commented();
    let opts = SummaryOptions {
        layout: SummaryLayout::SeparatePages,
        ..SummaryOptions::default()
    };
    let summary = doc.summarize_comments(&opts).unwrap();
    // Pages 1 and 3 have comments: 3 pages + 2 summary pages.
    assert_eq!(summary.page_count(), 5);
    assert!(summary.comments().is_empty(), "the comments are flattened");
    assert!(summary.page_text(1).contains("circle"));
    assert!(summary.page_text(4).contains("On the last page"));
    for p in 0..summary.page_count() {
        summary.render_page_png(p, 0.5).unwrap();
    }
    // The original document is unchanged.
    assert_eq!(doc.page_count(), 3);
    assert_eq!(doc.comments().len(), 3);
}

#[test]
fn connector_layout_puts_pages_and_comments_side_by_side() {
    let doc = commented();
    let opts = SummaryOptions {
        layout: SummaryLayout::ConnectorLines,
        font_size: 12.0,
        ..SummaryOptions::default()
    };
    let summary = doc.summarize_comments(&opts).unwrap();
    assert_eq!(summary.page_count(), 3);
    let size = summary.page_sizes()[0];
    assert!(size.width > 600.0, "{size:?}");
    assert!(summary.page_text(0).contains("circle"));
    summary.render_page_png(0, 0.5).unwrap();
}

#[test]
fn documents_without_comments_cannot_be_summarized() {
    let doc = Document::new_blank(1, 400.0, 500.0).unwrap();
    assert!(doc.summarize_comments(&SummaryOptions::default()).is_err());
}
