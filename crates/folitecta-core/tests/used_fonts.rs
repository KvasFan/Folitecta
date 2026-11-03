//! The fonts of a document, wherever its pages have them.

use folitecta_core::testdocs::fonts_doc;
use folitecta_core::{Document, FontKind, TextLayout};

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

#[test]
fn fonts_listed_but_not_used_do_not_count() {
    use lopdf::{Object, dictionary};
    let mut doc = Document::from_text("Some text in the standard font.", TextLayout::default()).unwrap();
    let before: Vec<String> = doc.used_fonts().into_iter().map(|f| f.name).collect();
    // The page lists a font its content never names.
    let page: lopdf::ObjectId = doc.inner().get_pages().into_values().next().unwrap();
    {
        let inner = doc.inner_mut();
        let font = inner.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "ZapfDingbats",
        });
        let res = match inner
            .get_dictionary(page)
            .unwrap()
            .get(b"Resources")
            .unwrap()
            .clone()
        {
            Object::Reference(r) => r,
            Object::Dictionary(d) => {
                let id = inner.add_object(d);
                inner
                    .get_dictionary_mut(page)
                    .unwrap()
                    .set("Resources", Object::Reference(id));
                id
            }
            _ => panic!("no resources"),
        };
        let res = inner.get_dictionary_mut(res).unwrap();
        let mut fonts = match res.get(b"Font").unwrap().clone() {
            Object::Dictionary(d) => d,
            other => panic!("{other:?}"),
        };
        fonts.set("FUnused", Object::Reference(font));
        res.set("Font", fonts);
    }
    doc.commit().unwrap();
    let after: Vec<String> = doc.used_fonts().into_iter().map(|f| f.name).collect();
    assert_eq!(after, before);
    assert!(!after.iter().any(|n| n == "ZapfDingbats"));
}
