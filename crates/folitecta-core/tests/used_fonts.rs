//! The fonts of a document, wherever its pages have them.

use folitecta_core::testdocs::fonts_doc;
use folitecta_core::{Document, FontKind};

#[test]
fn fonts_of_pages_forms_and_comments() {
    let doc = Document::from_bytes(fonts_doc(), None, None).unwrap();
    let fonts = doc.used_fonts();
    let summary: Vec<_> = fonts
        .iter()
        .map(|f| {
            (
                f.name.as_str(),
                f.kind,
                f.embedded,
                f.subset,
                f.standard,
                f.pages.clone(),
            )
        })
        .collect();
    assert_eq!(
        summary,
        [
            ("Body", FontKind::TrueType, true, true, false, vec![0, 1]),
            ("Drawn", FontKind::Type3, true, false, false, vec![1]),
            ("Elsewhere", FontKind::Composite, false, false, false, vec![1]),
            ("Helvetica", FontKind::Type1, false, false, true, vec![0]),
        ]
    );
}

#[test]
fn a_blank_page_has_no_fonts() {
    assert!(
        Document::new_blank(2, 100.0, 100.0)
            .unwrap()
            .used_fonts()
            .is_empty()
    );
}
