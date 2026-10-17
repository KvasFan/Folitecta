//! Tests for the positioned text layer and the operations built on it.

use folitecta_core::marks::{MarkPosition, TextMarkOptions};
use folitecta_core::{Document, Markup, SearchOptions};

fn sample() -> Document {
    let mut doc = Document::new_blank(2, 595.0, 842.0).unwrap();
    doc.add_header_footer(
        &[0, 1],
        ["Quarterly report", "", ""],
        ["", "Visit https://example.org/docs today", ""],
        12.0,
        40.0,
        [0.0, 0.0, 0.0],
    )
    .unwrap();
    doc.add_text_watermark(
        &[1],
        &TextMarkOptions {
            text: "Rotated words here".into(),
            rotation: 30.0,
            font_size: 30.0,
            position: MarkPosition::Center,
            ..Default::default()
        },
    )
    .unwrap();
    doc.clear_history();
    doc
}

#[test]
fn glyphs_have_positions_in_display_space() {
    let doc = sample();
    let layer = doc.text_layer(0).unwrap();
    assert_eq!((layer.width, layer.height), (595.0, 842.0));
    assert!(layer.text.contains("Quarterly report"), "{:?}", layer.text);
    assert_eq!(layer.lines.len(), 2, "{:?}", layer.lines);
    // The header is near the top of the displayed page (y grows downwards).
    let first = &layer.glyphs[0];
    assert!(first.bbox[1] < 80.0 && first.bbox[0] < 60.0, "{:?}", first.bbox);
    assert!((first.size - 12.0).abs() < 0.5, "font size {}", first.size);
    // Every non-synthetic character maps to a glyph.
    assert_eq!(layer.char_glyph.len(), layer.text.chars().count());
}

#[test]
fn search_returns_rectangles() {
    let doc = sample();
    let hits = doc.search("report", SearchOptions::default());
    assert_eq!(hits.len(), 2);
    assert_eq!(hits[0].rects.len(), 1);
    let r = hits[0].rects[0];
    assert!(r[2] > r[0] && r[3] > r[1] && r[3] < 90.0, "{r:?}");
    assert_eq!(doc.text_range(0, hits[0].start, hits[0].end), "report");
    // Whitespace in the query matches the gap between words.
    assert_eq!(
        doc.search("Quarterly   report", SearchOptions::default()).len(),
        2
    );
    assert!(
        doc.search(
            "REPORT",
            SearchOptions {
                case_sensitive: true,
                whole_words: false,
                ..Default::default()
            }
        )
        .is_empty()
    );
    assert_eq!(
        doc.search(
            "port",
            SearchOptions {
                case_sensitive: false,
                whole_words: true,
                ..Default::default()
            }
        )
        .len(),
        0
    );
}

#[test]
fn rotated_text_is_found_and_selectable() {
    let doc = sample();
    let hits = doc.search("Rotated words", SearchOptions::default());
    assert_eq!(hits.len(), 1, "{:?}", doc.text_layer(1).unwrap().text);
    let layer = doc.text_layer(1).unwrap();
    let quads = layer.range_quads(hits[0].start, hits[0].end);
    assert_eq!(quads.len(), 1);
    let q = quads[0];
    // Text rotated by 30 degrees: the upper-right corner is above (smaller y) the upper-left one.
    assert!(q[3] < q[1] - 5.0, "{q:?}");
}

#[test]
fn point_to_character_and_words() {
    let doc = sample();
    let layer = doc.text_layer(0).unwrap();
    let hit = doc.search("report", SearchOptions::default())[0].clone();
    let r = hit.rects[0];
    let inside = layer.char_at(r[0] + 1.0, (r[1] + r[3]) / 2.0).unwrap();
    assert_eq!(inside, hit.start);
    let (ws, we) = layer.word_at(hit.start + 2);
    assert_eq!(layer.slice(ws, we), "report");
    let (ls, le) = layer.line_at(hit.start);
    assert_eq!(layer.slice(ls, le), "Quarterly report");
    assert!(layer.char_at(10.0, 10.0).is_some());
}

#[test]
fn markup_and_redaction_from_text() {
    let mut doc = sample();
    let hit = doc.search("Quarterly", SearchOptions::default())[0].clone();
    doc.markup_text_range(0, hit.start, hit.end, Markup::Highlight, [1.0, 0.9, 0.0], "me")
        .unwrap();
    let a = &doc.annotations(0).unwrap()[0];
    assert_eq!(a.subtype, "Highlight");
    assert_eq!(a.contents, "Quarterly");
    let n = doc
        .markup_search_results(
            "report",
            SearchOptions::default(),
            Markup::Underline,
            [0.0, 0.0, 1.0],
            "me",
        )
        .unwrap();
    assert_eq!(n, 2);
    let marked = doc
        .redact_search_results("example", SearchOptions::default(), "me")
        .unwrap();
    assert_eq!(marked, 2);
    assert_eq!(doc.pending_redactions().len(), 2);
    doc.apply_redactions(None).unwrap();
    assert!(
        doc.search("example", SearchOptions::default()).is_empty(),
        "redacted text is gone"
    );
}

#[test]
fn links_from_urls() {
    let mut doc = sample();
    let layer = doc.text_layer(0).unwrap();
    let urls = layer.urls();
    assert_eq!(urls.len(), 1);
    assert_eq!(urls[0].2, "https://example.org/docs");
    let created = doc.create_links_from_urls(&[0, 1]).unwrap();
    assert_eq!(created, 2);
    let links = doc.links(0).unwrap();
    assert_eq!(links.len(), 1);
    assert_eq!(
        links[0].target,
        folitecta_core::LinkTarget::Uri("https://example.org/docs".into())
    );
    // Running again does not duplicate links.
    assert_eq!(doc.create_links_from_urls(&[0, 1]).unwrap(), 0);
}
