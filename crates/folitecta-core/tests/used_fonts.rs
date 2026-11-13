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

#[test]
fn a_font_replaced_with_a_free_font_keeps_the_text_in_place() {
    use folitecta_core::fonts::Family;
    let mut doc = Document::from_bytes(folitecta_core::testdocs::embeddable_fonts_doc(), None, None).unwrap();
    let text = doc.page_text(0);
    let before = doc.text_layer(0).unwrap();
    let at = |l: &folitecta_core::textlayer::PageText, s: &str| {
        let i = l.text.find(s).unwrap();
        let g = l.char_glyph[l.text[..i].chars().count()].unwrap();
        l.glyphs[g].bbox
    };
    // Helvetica drawn with FreeSerif, italic: embedded, the text where it was.
    assert_eq!(
        doc.replace_font("Helvetica", Family::Serif, false, true).unwrap(),
        1
    );
    let fonts = doc.used_fonts();
    let serif = fonts
        .iter()
        .find(|f| f.name == "FreeSerifItalic")
        .expect("the replacement is listed");
    assert!(serif.embedded && serif.subset);
    assert!(!fonts.iter().any(|f| f.name == "Helvetica"));
    assert_eq!(doc.page_text(0), text);
    let after = doc.text_layer(0).unwrap();
    let (a, b) = (at(&before, "text"), at(&after, "text"));
    assert!(
        (a[0] - b[0]).abs() < 0.01,
        "the line keeps its widths: {a:?} {b:?}"
    );
    // An embedded font replaced again: FreeMono, bold.
    assert_eq!(
        doc.replace_font("FreeSerifItalic", Family::Mono, true, false)
            .unwrap(),
        1
    );
    assert!(
        doc.used_fonts()
            .iter()
            .any(|f| f.name == "FreeMonoBold" && f.embedded)
    );
    // One undo step each; unknown names and symbolic fonts refused.
    doc.undo().unwrap();
    doc.undo().unwrap();
    assert!(
        doc.used_fonts()
            .iter()
            .any(|f| f.name == "Helvetica" && !f.embedded)
    );
    assert!(doc.replace_font("Nowhere", Family::Sans, false, false).is_err());
    assert!(
        doc.replace_font("ZapfDingbats", Family::Sans, false, false)
            .is_err()
    );
}

#[test]
fn replaced_fonts_come_back_and_maps_are_repaired() {
    use folitecta_core::fonts::Family;
    // A page drawing with Arial, not embedded, WinAnsi: no ToUnicode map.
    let mut doc = Document::from_bytes(folitecta_core::testdocs::text_doc(1), None, None).unwrap();
    let before = doc.used_fonts();
    let name = before
        .iter()
        .find(|f| !f.embedded)
        .map(|f| f.name.clone())
        .expect("a font not embedded");
    let repaired = doc.repair_to_unicode().unwrap();
    assert!(repaired >= 1, "{repaired}");
    assert_eq!(
        doc.repair_to_unicode().unwrap(),
        0,
        "a second time there is nothing to do"
    );
    let n = doc.replace_font(&name, Family::Serif, false, false).unwrap();
    assert!(n >= 1);
    assert_eq!(doc.substituted_fonts(), vec![name.clone()]);
    let back = doc.reset_substituted_fonts(Some(&name)).unwrap();
    assert_eq!(back, n);
    assert!(doc.substituted_fonts().is_empty());
    let again = Document::from_bytes(doc.bytes().to_vec(), None, None).unwrap();
    assert!(again.used_fonts().iter().any(|f| f.name == name && !f.embedded));
}
