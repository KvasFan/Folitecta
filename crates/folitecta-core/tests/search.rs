//! Search: diacritics, words near each other, and places beyond the page text.

use folitecta_core::search::{FoundIn, Proximity, SearchScopes, match_text};
use folitecta_core::{Document, FieldProps, NewField, SearchOptions, TextLayout};

fn doc() -> Document {
    let text = "Café crème au lait with a résumé.\n\n\
                The quick brown fox jumps over the lazy dog.\n\n\
                A dog sleeps here. Much later in another paragraph the fox appears.\n\
                \u{c}\
                Second page about the fox only.";
    Document::from_text(text, TextLayout::default()).unwrap()
}

fn plain() -> SearchOptions {
    SearchOptions::default()
}

#[test]
fn diacritics_can_be_ignored() {
    let doc = doc();
    assert!(doc.search("cafe", plain()).is_empty());
    let loose = SearchOptions {
        ignore_diacritics: true,
        ..Default::default()
    };
    assert_eq!(doc.search("cafe creme", loose).len(), 1);
    assert_eq!(doc.search("RESUME", loose).len(), 1);
    // Decomposed accents (letter + combining mark) match too.
    assert_eq!(
        match_text("cafe\u{301} noir", "café", loose, Proximity::Phrase),
        Some((0, 5))
    );
    assert_eq!(match_text("Café", "cafe", plain(), Proximity::Phrase), None);
}

#[test]
fn words_near_each_other() {
    let doc = doc();
    let o = plain();
    // As a phrase "fox dog" does not occur.
    assert!(doc.search_words("fox dog", o, Proximity::Phrase).is_empty());
    // Within a few words: only the first fox sentence.
    let near = doc.search_words("fox dog", o, Proximity::Adjacent);
    assert!(!near.is_empty());
    assert!(near.iter().all(|h| h.page == 0));
    let words: Vec<usize> = near.iter().map(|h| h.word).collect();
    assert!(words.contains(&0) && words.contains(&1));
    // Same paragraph: the second and third paragraphs.
    let para = doc.search_words("fox dog", o, Proximity::Paragraph);
    assert!(para.len() > near.len(), "{} vs {}", para.len(), near.len());
    // Same page: page 0 only (page 1 has no dog).
    let page = doc.search_words("fox dog", o, Proximity::Page);
    assert!(page.iter().all(|h| h.page == 0));
    // Same document: every fox and dog.
    let all = doc.search_words("fox dog", o, Proximity::Document);
    assert!(all.iter().any(|h| h.page == 1));
    assert!(doc.search_words("fox unicorn", o, Proximity::Document).is_empty());
}

#[test]
fn other_places_are_searched() {
    let mut doc = doc();
    doc.add_note(0, 100.0, 100.0, "Check the fox picture", "Ann", [1.0, 1.0, 0.0])
        .unwrap();
    doc.add_bookmark("Fox chapter", 1, &[]).unwrap();
    let props = FieldProps {
        name: "animal".into(),
        ..FieldProps::default()
    };
    let field = doc
        .add_form_field(0, NewField::Text, [50.0, 50.0, 200.0, 70.0], &props)
        .unwrap();
    doc.set_field_value(field, "a red fox").unwrap();
    doc.set_info("Fox report", "", "", "").unwrap();
    let inner = Document::from_text("The attached fox document.", TextLayout::default()).unwrap();
    doc.add_attachment("inner.pdf", inner.bytes().to_vec(), "", Some("application/pdf"))
        .unwrap();
    doc.add_uri_link(0, [10.0, 10.0, 60.0, 30.0], "https://example.org/fox")
        .unwrap();
    let scopes = SearchScopes {
        page_text: true,
        comments: true,
        bookmarks: true,
        fields: true,
        doc_info: true,
        attachments: true,
        links: true,
    };
    let hits = doc.search_other("fox", plain(), Proximity::Phrase, &scopes);
    let kinds: Vec<&FoundIn> = hits.iter().map(|h| &h.found_in).collect();
    assert!(
        kinds
            .iter()
            .any(|k| matches!(k, FoundIn::Comment { page: 0, .. }))
    );
    assert!(
        kinds
            .iter()
            .any(|k| matches!(k, FoundIn::Bookmark { page: Some(1), .. }))
    );
    assert!(
        kinds
            .iter()
            .any(|k| matches!(k, FoundIn::Field { name, .. } if name == "animal"))
    );
    assert!(
        kinds
            .iter()
            .any(|k| matches!(k, FoundIn::DocInfo { key } if key == "Title"))
    );
    assert!(
        kinds
            .iter()
            .any(|k| matches!(k, FoundIn::AttachmentPage { page: 0, .. }))
    );
    assert!(kinds.iter().any(|k| matches!(k, FoundIn::Link { page: 0, .. })));
    // Nothing outside the page text unless asked.
    assert!(
        doc.search_other("fox", plain(), Proximity::Phrase, &SearchScopes::default())
            .is_empty()
    );
}
