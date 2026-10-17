use folitecta_core::{Document, TextLayout};
use lopdf::{Object, Stream, dictionary};

/// A one-page document drawing `content` with font /F1 (`encoding` as given).
fn page_with(content: &str, encoding: Object) -> Document {
    let mut doc = Document::new_blank(1, 600.0, 800.0).unwrap();
    let inner = doc.inner_mut();
    let font = inner.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
        "Encoding" => encoding,
    });
    let content = inner.add_object(Stream::new(dictionary! {}, content.as_bytes().to_vec()));
    let page = inner.get_pages()[&1];
    let dict = inner.get_dictionary_mut(page).unwrap();
    dict.set(
        "Resources",
        dictionary! { "Font" => dictionary! { "F1" => font } },
    );
    dict.set("Contents", content);
    doc.commit().unwrap();
    doc
}

fn first_x(doc: &Document, ch: char) -> f32 {
    let layer = doc.text_layer(0).unwrap();
    let i = layer.text.chars().position(|c| c == ch).unwrap();
    let g = layer.char_glyph[i].unwrap();
    layer.glyphs[g].bbox[0]
}

#[test]
fn words_are_deleted_inserted_and_replaced() {
    let mut doc = Document::from_text("Hello brave world", TextLayout::default()).unwrap();
    assert_eq!(doc.page_text(0), "Hello brave world");
    // Delete "brave " (with the space after it).
    let r = doc.edit_page_text(0, 6, 12, "").unwrap();
    assert!(r.substituted.is_empty());
    assert_eq!(doc.page_text(0), "Hello world");
    // Insert before "world".
    doc.edit_page_text(0, 6, 6, "big ").unwrap();
    assert_eq!(doc.page_text(0), "Hello big world");
    // Replace "big" with "bold": the rest of the line follows.
    doc.edit_page_text(0, 6, 9, "bold").unwrap();
    assert_eq!(doc.page_text(0), "Hello bold world");
    let start = first_x(&doc, 'H');
    let w = first_x(&doc, 'w');
    let expected = start + folitecta_core::util::helvetica_width("Hello bold ", 11.0);
    assert!((w - expected).abs() < 1.0, "w at {w}, expected {expected}");
    // Each edit is one step of undo.
    doc.undo().unwrap();
    assert_eq!(doc.page_text(0), "Hello big world");
    doc.undo().unwrap();
    doc.undo().unwrap();
    assert_eq!(doc.page_text(0), "Hello brave world");
}

#[test]
fn every_occurrence_is_replaced_in_one_undo_step() {
    let mut doc = Document::from_text(
        "the cat and the dog\nthe end\u{c}theory of the cat",
        TextLayout::default(),
    )
    .unwrap();
    let words = folitecta_core::SearchOptions {
        whole_words: true,
        ..Default::default()
    };
    let first = doc.page_text(0);
    let r = doc.replace_page_text("the", "a", words, None).unwrap();
    assert_eq!((r.replaced, r.refused), (4, 0), "{r:?}");
    assert_eq!(doc.page_text(0), first.replace("the ", "a "), "from {first:?}");
    // Whole words: "theory" stays.
    assert_eq!(doc.page_text(1), "theory of a cat");
    // Only the chosen pages; the replacement may contain what was found.
    let r = doc
        .replace_page_text("cat", "cats", Default::default(), Some(&[1]))
        .unwrap();
    assert_eq!(r.replaced, 1);
    assert_eq!(doc.page_text(0), first.replace("the ", "a "));
    assert_eq!(doc.page_text(1), "theory of a cats");
    doc.undo().unwrap();
    doc.undo().unwrap();
    assert_eq!(doc.page_text(0), first);
    assert_eq!(doc.page_text(1), "theory of the cat");
    assert!(doc.replace_page_text("", "x", Default::default(), None).is_err());
}

#[test]
fn adjustments_between_pieces_are_kept() {
    let mut doc = page_with(
        "BT /F1 12 Tf 72 700 Td [(Hel) -50 (lo) -400 (wor) 20 (ld)] TJ ET",
        Object::Name(b"WinAnsiEncoding".to_vec()),
    );
    assert_eq!(doc.page_text(0), "Hello world");
    let before = first_x(&doc, 'w');
    // "lo" goes; the gap before "world" stays.
    doc.edit_page_text(0, 3, 5, "").unwrap();
    assert_eq!(doc.page_text(0), "Hel world");
    let after = first_x(&doc, 'w');
    let lo = folitecta_core::util::helvetica_width("lo", 12.0);
    assert!(
        ((before - after) - lo).abs() < 1.0,
        "moved {}, expected {lo}",
        before - after
    );
    // Text added at the end of a line.
    let n = doc.page_text(0).chars().count();
    doc.edit_page_text(0, n, n, "!").unwrap();
    assert_eq!(doc.page_text(0), "Hel world!");
}

#[test]
fn characters_the_font_lacks_use_a_standard_font() {
    // An encoding dictionary: only the codes the page uses are known.
    let mut doc = page_with(
        "BT /F1 14 Tf 0.8 0 0 rg 72 700 Td (Hello) Tj ET",
        Object::Dictionary(dictionary! { "Type" => "Encoding", "BaseEncoding" => "WinAnsiEncoding" }),
    );
    let r = doc.edit_page_text(0, 5, 5, " Hez").unwrap();
    assert_eq!(doc.page_text(0), "Hello Hez");
    // H and e come from the page font; the space and z from a standard font.
    assert_eq!(r.substituted, " z");
    let fonts = doc
        .inner()
        .get_dictionary(doc.page_id(0).unwrap())
        .unwrap()
        .get(b"Resources")
        .unwrap()
        .clone();
    let has = match fonts {
        Object::Dictionary(d) => d
            .get(b"Font")
            .unwrap()
            .as_dict()
            .unwrap()
            .has(b"FoliTx-Helvetica"),
        _ => false,
    };
    assert!(has);
}

#[test]
fn text_in_reused_drawings_is_refused() {
    let mut doc = Document::new_blank(1, 600.0, 800.0).unwrap();
    let inner = doc.inner_mut();
    let font = inner.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica", "Encoding" => "WinAnsiEncoding",
    });
    let form = inner.add_object(Stream::new(
        dictionary! {
            "Type" => "XObject", "Subtype" => "Form", "BBox" => vec![0.into(), 0.into(), 600.into(), 800.into()],
            "Resources" => dictionary! { "Font" => dictionary! { "F1" => font } },
        },
        b"BT /F1 12 Tf 72 700 Td (Inside) Tj ET".to_vec(),
    ));
    let content = inner.add_object(Stream::new(
        dictionary! {},
        b"/Fm Do BT /F1 12 Tf 72 600 Td (Outside) Tj ET".to_vec(),
    ));
    let page = inner.get_pages()[&1];
    let dict = inner.get_dictionary_mut(page).unwrap();
    dict.set(
        "Resources",
        dictionary! { "Font" => dictionary! { "F1" => font }, "XObject" => dictionary! { "Fm" => form } },
    );
    dict.set("Contents", content);
    doc.commit().unwrap();
    let text = doc.page_text(0);
    assert!(text.contains("Inside") && text.contains("Outside"), "{text}");
    let at = text.find("Inside").unwrap();
    assert!(doc.edit_page_text(0, at, at + 6, "").is_err());
    let at = text.find("Outside").unwrap();
    doc.edit_page_text(0, at, at + 3, "In").unwrap();
    assert!(doc.page_text(0).contains("Inside"));
    assert!(doc.page_text(0).contains("Inside\n") || doc.page_text(0).ends_with("Inside"));
}

#[test]
fn comments_can_be_added_to_edited_text() {
    let para = "The quick brown fox jumps over the lazy dog. ";
    let mut doc =
        Document::from_text(&format!("Section 1\n{}", para.repeat(3)), TextLayout::default()).unwrap();
    let text = doc.page_text(0);
    let at = |doc: &Document, w: &str| {
        let t = doc.page_text(0);
        let b = t.find(w).unwrap();
        let s = t[..b].chars().count();
        (s, s + w.chars().count())
    };
    let (a, b) = at(&doc, "quick ");
    doc.edit_page_text(0, a, b, "").unwrap();
    doc.edit_page_text(0, 0, 0, "New ").unwrap();
    let (a, b) = at(&doc, "brown");
    doc.edit_page_text(0, a, b, "golden").unwrap();
    assert_ne!(doc.page_text(0), text);
    let before = doc.annotations(0).unwrap().len();
    let (a, b) = at(&doc, "lazy");
    let edited = doc.page_text(0);
    doc.replace_text_range(0, a, b, "sleepy", [0.1, 0.4, 0.9], "Reviewer")
        .unwrap();
    assert!(doc.annotations(0).unwrap().len() > before);
    assert_eq!(doc.page_text(0), edited);
}
