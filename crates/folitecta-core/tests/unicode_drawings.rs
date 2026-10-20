//! Generated text beyond WinAnsi in tables, calculator tapes and designed
//! stamps (FOL-L018/S02), in page marks, comment summaries and documents
//! made of text or e-mail messages (FOL-L018/S03): drawn with an embedded
//! subset of FreeSans and its ToUnicode map, saved and opened again, found
//! after flattening; bold text in that font is stroked; WinAnsi text keeps
//! the standard Helvetica as before; text that cannot be drawn is refused
//! before any change.

use std::collections::HashSet;
use std::path::PathBuf;

use folitecta_core::calculator::{Tape, TapeLine};
use folitecta_core::email::{EmailLabels, EmailMessage};
use folitecta_core::stamp_design::{StampDesign, StampElement, StampValues};
use folitecta_core::tables::Table;
use folitecta_core::{
    Document, Error, FlattenScope, GlyphProblem, SummaryLayout, SummaryOptions, TextLayout,
};
use lopdf::{Dictionary, Object, ObjectId};

const PLACE: [f32; 4] = [50.0, 500.0, 350.0, 620.0];

/// A folder of its own for one test, removed when the test ends.
struct Folder(PathBuf);

impl Drop for Folder {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn folder(name: &str) -> Folder {
    let dir = std::env::temp_dir().join(format!(
        "folitecta-unicode-drawings-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    Folder(dir)
}

fn blank() -> Document {
    Document::new_blank(1, 595.0, 842.0).unwrap()
}

/// Save the document and open the file again.
fn reopen(doc: &mut Document, name: &str) -> (Document, Folder) {
    let dir = folder(name);
    let path = dir.0.join("saved.pdf");
    doc.save_as(&path).unwrap();
    (Document::open(&path).unwrap(), dir)
}

fn get<'a>(doc: &'a lopdf::Document, obj: &'a Object) -> &'a Object {
    match obj {
        Object::Reference(id) => doc.get_object(*id).unwrap(),
        other => other,
    }
}

/// The normal appearance stream of comment `index` of page 0.
fn appearance(doc: &Document, index: usize) -> &lopdf::Stream {
    let id = doc.annotations(0).unwrap()[index].id.unwrap();
    let annot = doc.inner().get_dictionary(id).unwrap();
    let ap = get(doc.inner(), annot.get(b"AP").unwrap()).as_dict().unwrap();
    get(doc.inner(), ap.get(b"N").unwrap()).as_stream().unwrap()
}

/// The fonts of that appearance: resource name, object id, dictionary.
fn fonts(doc: &Document, index: usize) -> Vec<(String, Option<ObjectId>, Dictionary)> {
    let stream = appearance(doc, index);
    let resources = get(doc.inner(), stream.dict.get(b"Resources").unwrap())
        .as_dict()
        .unwrap();
    let fonts = get(doc.inner(), resources.get(b"Font").unwrap())
        .as_dict()
        .unwrap();
    let mut out: Vec<_> = fonts
        .iter()
        .map(|(name, obj)| {
            (
                String::from_utf8_lossy(name).to_string(),
                obj.as_reference().ok(),
                get(doc.inner(), obj).as_dict().unwrap().clone(),
            )
        })
        .collect();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

fn name_of(d: &Dictionary, key: &[u8]) -> String {
    String::from_utf8_lossy(d.get(key).unwrap().as_name().unwrap()).to_string()
}

fn content(doc: &Document, index: usize) -> String {
    let stream = appearance(doc, index);
    String::from_utf8_lossy(
        &stream
            .decompressed_content()
            .unwrap_or_else(|_| stream.content.clone()),
    )
    .to_string()
}

/// The appearance draws with the embedded Unicode font only, and writes no `?`.
fn assert_unicode(doc: &Document, index: usize) {
    let fonts = fonts(doc, index);
    assert_eq!(fonts.len(), 1, "{fonts:?}");
    let (name, _, font) = &fonts[0];
    assert_eq!(name, "FoliUni");
    assert_eq!(name_of(font, b"Subtype"), "Type0");
    assert!(font.has(b"ToUnicode"));
    assert!(!content(doc, index).contains('?'), "{}", content(doc, index));
}

fn text_after_flattening(doc: &Document) -> String {
    let mut flat = Document::from_bytes(doc.bytes().to_vec(), None, None).unwrap();
    flat.flatten(&[0], FlattenScope::Comments).unwrap();
    flat.page_text(0)
}

/// The document did not change at all: revision, bytes, comments and history.
fn unchanged(doc: &Document, before: &(u64, Vec<u8>, usize, bool, bool)) {
    assert_eq!(doc.revision(), before.0);
    assert_eq!(*doc.bytes(), before.1);
    assert_eq!(doc.annotations(0).unwrap().len(), before.2);
    assert_eq!(doc.can_undo(), before.3);
    assert_eq!(doc.can_redo(), before.4, "the redo history is kept");
}

fn state(doc: &Document) -> (u64, Vec<u8>, usize, bool, bool) {
    (
        doc.revision(),
        doc.bytes().to_vec(),
        doc.annotations(0).unwrap().len(),
        doc.can_undo(),
        doc.can_redo(),
    )
}

fn table(texts: [&str; 4]) -> Table {
    let mut t = Table::new(2, 2);
    for (k, text) in texts.iter().enumerate() {
        t.cells[k / 2][k % 2].text = text.to_string();
    }
    t
}

#[test]
fn tables_draw_text_beyond_winansi_with_an_embedded_subset() {
    let mut doc = blank();
    let mut t = table(["Łódź", "Ελληνικά", "Кириллица", "plain"]);
    t.cells[0][1].bold = true;
    doc.add_table(0, PLACE, &t, "Reviewer").unwrap();
    let (back, _dir) = reopen(&mut doc, "table");
    assert_unicode(&back, 0);
    // The bold cell is filled and stroked, inside q/Q.
    assert!(content(&back, 0).contains(" 2 Tr "), "{}", content(&back, 0));
    let text = text_after_flattening(&back);
    for word in ["Łódź", "Ελληνικά", "Кириллица", "plain"] {
        assert!(text.contains(word), "{word} in {text:?}");
    }
    assert_eq!(back.table_at(0, 0).unwrap(), t);
}

#[test]
fn winansi_tables_keep_the_standard_helvetica() {
    let mut doc = blank();
    let mut t = table(["Città", "Total", "", "1 234,50 €"]);
    t.cells[0][1].bold = true;
    doc.add_table(0, PLACE, &t, "Reviewer").unwrap();
    let fonts = fonts(&doc, 0);
    let names: Vec<(&str, String)> = fonts
        .iter()
        .map(|(n, _, d)| (n.as_str(), name_of(d, b"BaseFont")))
        .collect();
    assert_eq!(
        names,
        [("TB", "Helvetica-Bold".into()), ("TR", "Helvetica".into())]
    );
    let c = content(&doc, 0);
    assert!(c.contains("BT /TR "), "{c}");
    assert!(c.contains("(Citt\\340)"), "{c}");
    assert!(!c.contains("Tr "), "{c}");
}

#[test]
fn text_a_table_cannot_draw_is_refused_before_any_change() {
    let mut doc = blank();
    let before = state(&doc);
    let err = doc
        .add_table(0, PLACE, &table(["ok", "שלום", "", ""]), "Reviewer")
        .unwrap_err();
    assert!(
        matches!(err, Error::Glyphs(GlyphProblem::NeedsLayout(_))),
        "{err:?}"
    );
    unchanged(&doc, &before);
    // An existing table keeps its text when the new one is refused.
    let good = table(["Łódź", "", "", ""]);
    doc.add_table(0, PLACE, &good, "Reviewer").unwrap();
    let before = state(&doc);
    let err = doc.set_table(0, 0, &table(["漢字", "", "", ""])).unwrap_err();
    assert!(
        matches!(err, Error::Glyphs(GlyphProblem::Missing { .. })),
        "{err:?}"
    );
    unchanged(&doc, &before);
    assert_eq!(doc.table_at(0, 0).unwrap(), good);
}

#[test]
fn a_table_drawn_again_with_the_same_characters_uses_its_font_again() {
    let mut doc = blank();
    let mut t = table(["Łódź", "Kraków", "", ""]);
    doc.add_table(0, PLACE, &t, "Reviewer").unwrap();
    let font = fonts(&doc, 0)[0].1.unwrap();
    // Another alignment: the same characters, the same subset.
    t.cells[0][0].align = "right".into();
    doc.set_table(0, 0, &t).unwrap();
    assert_eq!(fonts(&doc, 0)[0].1.unwrap(), font);
    // Resized: drawn again from its data, still with that font.
    doc.set_annotation_rect(0, 0, [40.0, 480.0, 400.0, 640.0])
        .unwrap();
    assert_unicode(&doc, 0);
    assert_eq!(fonts(&doc, 0)[0].1.unwrap(), font);
    // New characters: a new subset that has them.
    t.cells[1][1].text = "Gdańsk".into();
    doc.set_table(0, 0, &t).unwrap();
    assert_ne!(fonts(&doc, 0)[0].1.unwrap(), font);
    assert!(text_after_flattening(&doc).contains("Gdańsk"));
    // One undo step back to the table before.
    doc.undo().unwrap();
    assert_eq!(fonts(&doc, 0)[0].1.unwrap(), font);
}

fn tape(comment: &str) -> Tape {
    Tape {
        lines: vec![
            TapeLine {
                op: '+',
                amount: 12.5,
                comment: comment.into(),
            },
            TapeLine::new('+', 7.5),
            TapeLine::new('=', 0.0),
        ],
        show_date: true,
        date: "2026-10-20".into(),
        ..Tape::default()
    }
}

#[test]
fn calculator_tapes_write_comments_beyond_winansi() {
    let mut doc = blank();
    doc.add_calculator(0, PLACE, &tape("Zażółć gęślą"), "Reviewer")
        .unwrap();
    let (back, _dir) = reopen(&mut doc, "tape");
    assert_unicode(&back, 0);
    // The totals are bold.
    assert!(content(&back, 0).contains(" 2 Tr "));
    let text = text_after_flattening(&back);
    for word in ["Zażółć", "gęślą", "20.00", "2026-10-20"] {
        assert!(text.contains(word), "{word} in {text:?}");
    }
    // Plain tapes keep the standard fonts.
    let mut plain = blank();
    plain
        .add_calculator(0, PLACE, &tape("Lunch"), "Reviewer")
        .unwrap();
    let names: Vec<String> = fonts(&plain, 0).into_iter().map(|(n, ..)| n).collect();
    assert_eq!(names, ["TB", "TR"]);
}

#[test]
fn a_tape_comment_that_cannot_be_drawn_is_refused_before_any_change() {
    let mut doc = blank();
    let before = state(&doc);
    let err = doc
        .add_calculator(0, PLACE, &tape("مرحبا"), "Reviewer")
        .unwrap_err();
    assert!(
        matches!(err, Error::Glyphs(GlyphProblem::NeedsLayout(_))),
        "{err:?}"
    );
    unchanged(&doc, &before);
    doc.add_calculator(0, PLACE, &tape("Łódź"), "Reviewer").unwrap();
    let before = state(&doc);
    assert!(doc.set_calculator(0, 0, &tape("漢")).is_err());
    unchanged(&doc, &before);
    assert_eq!(doc.calculator_at(0, 0).unwrap(), tape("Łódź"));
}

fn design(text: &str, bold: bool) -> StampDesign {
    let mut el = StampElement::new("text", 200.0, 70.0);
    el.text = text.into();
    el.bold = bold;
    el.font_size = 18.0;
    StampDesign {
        elements: vec![el],
        ..StampDesign::default()
    }
}

#[test]
fn designed_stamps_write_text_beyond_winansi() {
    let mut doc = blank();
    doc.add_designed_stamp(
        0,
        PLACE,
        &design("ZATWIERDZONO — Łódź", true),
        &StampValues::default(),
        "Reviewer",
        "Approved",
    )
    .unwrap();
    let (back, _dir) = reopen(&mut doc, "stamp");
    assert_unicode(&back, 0);
    assert!(content(&back, 0).contains(" 2 Tr "));
    assert!(text_after_flattening(&back).contains("ZATWIERDZONO — Łódź"));
    // WinAnsi stamps keep the standard Helvetica.
    let mut plain = blank();
    plain
        .add_designed_stamp(
            0,
            PLACE,
            &design("APPROVED", true),
            &StampValues::default(),
            "Reviewer",
            "Approved",
        )
        .unwrap();
    let names: Vec<String> = fonts(&plain, 0).into_iter().map(|(n, ..)| n).collect();
    assert_eq!(names, ["H", "HB"]);
    assert!(content(&plain, 0).contains("BT /HB "));
}

#[test]
fn stamp_text_that_cannot_be_drawn_is_refused_before_any_change() {
    let mut doc = blank();
    let before = state(&doc);
    let err = doc
        .add_designed_stamp(
            0,
            PLACE,
            &design("漢字", false),
            &StampValues::default(),
            "Reviewer",
            "Approved",
        )
        .unwrap_err();
    assert!(
        matches!(&err, Error::Glyphs(GlyphProblem::Missing { chars, .. }) if chars == &['漢', '字']),
        "{err:?}"
    );
    unchanged(&doc, &before);
}

// Page marks (FOL-L018/S03): watermarks, headers and footers, page and Bates
// numbers drawn into the content of the pages.

fn two_pages() -> Document {
    Document::new_blank(2, 595.0, 842.0).unwrap()
}

/// The fonts in the resources of page `page`: name and object id.
fn page_fonts(doc: &Document, page: usize) -> Vec<(String, ObjectId)> {
    let id = doc.page_id(page).unwrap();
    let page = doc.inner().get_dictionary(id).unwrap();
    let res = get(doc.inner(), page.get(b"Resources").unwrap())
        .as_dict()
        .unwrap();
    let Ok(fonts) = res.get(b"Font") else {
        return Vec::new();
    };
    let fonts = get(doc.inner(), fonts).as_dict().unwrap();
    let mut out: Vec<_> = fonts
        .iter()
        .map(|(k, v)| (String::from_utf8_lossy(k).to_string(), v.as_reference().unwrap()))
        .collect();
    out.sort();
    out
}

fn page_content(doc: &Document, page: usize) -> String {
    let id = doc.page_id(page).unwrap();
    String::from_utf8_lossy(&doc.inner().get_page_content(id)).to_string()
}

fn mark(text: &str) -> folitecta_core::TextMarkOptions {
    folitecta_core::TextMarkOptions {
        text: text.into(),
        ..folitecta_core::TextMarkOptions::default()
    }
}

#[test]
fn watermarks_beyond_winansi_share_one_subset_on_every_page() {
    let mut doc = two_pages();
    doc.add_text_watermark(&[0, 1], &mark("POUFNE — Łódź")).unwrap();
    let (back, _dir) = reopen(&mut doc, "watermark");
    for page in 0..2 {
        let fonts = page_fonts(&back, page);
        assert_eq!(fonts.len(), 1, "{fonts:?}");
        assert_eq!(fonts[0].0, "FoliUni1");
        let font = back.inner().get_dictionary(fonts[0].1).unwrap();
        assert_eq!(name_of(font, b"Subtype"), "Type0");
        assert!(font.has(b"ToUnicode"));
        assert!(!page_content(&back, page).contains('?'));
        assert!(
            back.page_text(page).contains("POUFNE — Łódź"),
            "{:?}",
            back.page_text(page)
        );
    }
    // One subset for the two pages.
    assert_eq!(page_fonts(&back, 0)[0].1, page_fonts(&back, 1)[0].1);
}

#[test]
fn a_later_mark_takes_its_own_font_name_and_leaves_the_others() {
    let mut doc = two_pages();
    doc.add_text_watermark(&[0, 1], &mark("Łódź")).unwrap();
    let first = page_fonts(&doc, 0);
    doc.add_header_footer(
        &[0, 1],
        ["Kraków", "", ""],
        ["", "", "Gdańsk"],
        10.0,
        30.0,
        [0.0; 3],
    )
    .unwrap();
    let both = page_fonts(&doc, 0);
    assert_eq!(both.len(), 2, "{both:?}");
    assert_eq!(both[0], first[0], "the watermark keeps its font");
    assert_eq!(both[1].0, "FoliUni2");
    let (back, _dir) = reopen(&mut doc, "two-marks");
    let text = back.page_text(1);
    for word in ["Łódź", "Kraków", "Gdańsk"] {
        assert!(text.contains(word), "{word} in {text:?}");
    }
    // Removing the header leaves the watermark.
    let mut doc = back;
    doc.remove_marks(&[0, 1], Some("HeaderFooter")).unwrap();
    let text = doc.page_text(0);
    assert!(text.contains("Łódź") && !text.contains("Kraków"), "{text:?}");
}

#[test]
fn page_and_bates_numbers_write_text_beyond_winansi() {
    let mut doc = two_pages();
    doc.add_page_numbers(
        &[0, 1],
        "Strona {page} z {pages} · ŁD-{bates:4}",
        1,
        folitecta_core::MarkPosition::BottomCenter,
        10.0,
        28.0,
        [0.0; 3],
    )
    .unwrap();
    let (back, _dir) = reopen(&mut doc, "numbers");
    assert!(
        back.page_text(0).contains("Strona 1 z 2 · ŁD-0001"),
        "{:?}",
        back.page_text(0)
    );
    assert!(
        back.page_text(1).contains("Strona 2 z 2 · ŁD-0002"),
        "{:?}",
        back.page_text(1)
    );
}

#[test]
fn winansi_marks_keep_the_standard_helvetica() {
    let mut doc = two_pages();
    doc.add_text_watermark(&[0], &mark("CONFIDENTIAL")).unwrap();
    doc.add_header_footer(&[0], ["Città", "", ""], ["", "", ""], 10.0, 30.0, [0.0; 3])
        .unwrap();
    let fonts = page_fonts(&doc, 0);
    assert_eq!(fonts.len(), 1, "{fonts:?}");
    assert_eq!(fonts[0].0, "FoliHelv");
    let content = page_content(&doc, 0);
    assert!(
        content.contains("(CONFIDENTIAL) Tj") && content.contains("(Citt\\340) Tj"),
        "{content}"
    );
}

#[test]
fn mark_text_that_cannot_be_drawn_is_refused_before_any_change() {
    let mut doc = two_pages();
    let before = state(&doc);
    let err = doc.add_text_watermark(&[0, 1], &mark("漢字")).unwrap_err();
    assert!(
        matches!(err, Error::Glyphs(GlyphProblem::Missing { .. })),
        "{err:?}"
    );
    unchanged(&doc, &before);
    // Macros are expanded first: a title that cannot be drawn is refused too.
    doc.set_info("שלום", "", "", "").unwrap();
    let before = state(&doc);
    let err = doc
        .add_header_footer(&[0, 1], ["{title}", "", ""], ["", "", ""], 10.0, 30.0, [0.0; 3])
        .unwrap_err();
    assert!(
        matches!(err, Error::Glyphs(GlyphProblem::NeedsLayout(_))),
        "{err:?}"
    );
    unchanged(&doc, &before);
}

// Generated documents (FOL-L018/S03): comment summaries, documents made of
// text and of e-mail messages.

fn commented(note: &str, author: &str) -> Document {
    let mut doc = Document::new_blank(2, 400.0, 500.0).unwrap();
    doc.add_note(0, 30.0, 470.0, note, author, [1.0, 0.8, 0.2])
        .unwrap();
    doc
}

fn all_text(doc: &Document) -> String {
    (0..doc.page_count())
        .map(|p| doc.page_text(p))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The fonts named `name` on any page of `doc`.
fn fonts_named(doc: &Document, name: &str) -> HashSet<ObjectId> {
    (0..doc.page_count())
        .flat_map(|p| page_fonts(doc, p))
        .filter(|(n, _)| n == name)
        .map(|(_, id)| id)
        .collect()
}

fn summary(doc: &Document, layout: SummaryLayout) -> folitecta_core::Result<Document> {
    doc.summarize_comments(&SummaryOptions {
        layout,
        ..SummaryOptions::default()
    })
}

const LAYOUTS: [SummaryLayout; 3] = [
    SummaryLayout::CommentsOnly,
    SummaryLayout::SeparatePages,
    SummaryLayout::ConnectorLines,
];

#[test]
fn comment_summaries_write_text_beyond_winansi_in_every_layout() {
    let mut doc = commented("Zażółć gęślą jaźń", "Łukasz");
    doc.add_reply(0, 0, "Согласен", "Ольга").unwrap();
    for layout in LAYOUTS {
        let mut made = summary(&doc, layout).unwrap();
        let (back, _dir) = reopen(&mut made, &format!("summary-{layout:?}"));
        let text = all_text(&back);
        for words in ["Zażółć gęślą jaźń", "Łukasz", "Согласен", "Ольга"] {
            assert!(text.contains(words), "{layout:?}: {words} in {text:?}");
        }
        // One subset for the whole summary.
        let unicode = fonts_named(&back, "FoliUni");
        assert_eq!(unicode.len(), 1, "{layout:?}");
        let font = back
            .inner()
            .get_dictionary(*unicode.iter().next().unwrap())
            .unwrap();
        assert_eq!(name_of(font, b"Subtype"), "Type0");
        assert!(font.has(b"ToUnicode"));
    }
}

#[test]
fn winansi_summaries_keep_the_standard_helvetica() {
    let doc = commented("Città", "Ann");
    for layout in LAYOUTS {
        let made = summary(&doc, layout).unwrap();
        assert!(fonts_named(&made, "FoliUni").is_empty(), "{layout:?}");
        assert!(!fonts_named(&made, "FB").is_empty(), "{layout:?}");
        assert!(all_text(&made).contains("Città"), "{layout:?}");
    }
}

#[test]
fn summary_text_that_cannot_be_drawn_is_refused() {
    for layout in LAYOUTS {
        let err = summary(&commented("漢字", "Ann"), layout).unwrap_err();
        assert!(
            matches!(err, Error::Glyphs(GlyphProblem::Missing { .. })),
            "{layout:?}: {err:?}"
        );
        let err = summary(&commented("note", "שלום"), layout).unwrap_err();
        assert!(
            matches!(err, Error::Glyphs(GlyphProblem::NeedsLayout(_))),
            "{layout:?}: {err:?}"
        );
    }
}

#[test]
fn text_documents_beyond_winansi_use_one_subset_for_every_page() {
    let mut doc = Document::from_text("Zażółć gęślą jaźń\u{c}Привет, мир", TextLayout::default()).unwrap();
    assert_eq!(doc.page_count(), 2);
    let (back, _dir) = reopen(&mut doc, "text-document");
    assert!(
        back.page_text(0).contains("Zażółć gęślą jaźń"),
        "{:?}",
        back.page_text(0)
    );
    assert!(
        back.page_text(1).contains("Привет, мир"),
        "{:?}",
        back.page_text(1)
    );
    let first = page_fonts(&back, 0);
    assert_eq!(first.len(), 1, "{first:?}");
    assert_eq!(first[0].0, "FoliUni");
    assert_eq!(page_fonts(&back, 1), first);
    // WinAnsi text keeps the standard Helvetica.
    let plain = Document::from_text("Città", TextLayout::default()).unwrap();
    assert_eq!(page_fonts(&plain, 0)[0].0, "FoliHelv");
    assert!(page_content(&plain, 0).contains("(Citt\\340) Tj"));
    let err = Document::from_text("漢字", TextLayout::default()).unwrap_err();
    assert!(
        matches!(err, Error::Glyphs(GlyphProblem::Missing { .. })),
        "{err:?}"
    );
}

fn message(subject: &str, body: &str) -> EmailMessage {
    EmailMessage {
        subject: subject.into(),
        from: "Łukasz Nowak <lukasz@example.org>".into(),
        to: "Ольга <olga@example.org>".into(),
        cc: String::new(),
        date: "2026-10-20 09:30 +02:00".into(),
        body: body.into(),
        attachments: vec![("Sprawozdanie — październik.pdf".into(), b"%PDF-1.7".to_vec())],
    }
}

#[test]
fn email_documents_write_text_beyond_winansi() {
    let msg = message("Spotkanie w Łodzi", "Dzień dobry,\nprzesyłam sprawozdanie.");
    let mut doc = Document::from_email(&msg, &EmailLabels::default(), TextLayout::default()).unwrap();
    let (back, _dir) = reopen(&mut doc, "email-document");
    let text = back.page_text(0);
    for words in [
        "Spotkanie w Łodzi",
        "Łukasz Nowak",
        "Ольга",
        "Dzień dobry,",
        "Sprawozdanie — październik.pdf",
    ] {
        assert!(text.contains(words), "{words} in {text:?}");
    }
    let fonts = page_fonts(&back, 0);
    assert_eq!(fonts.len(), 1, "{fonts:?}");
    assert_eq!(fonts[0].0, "FoliUni");
    // The bold subject and labels are stroked.
    assert!(page_content(&back, 0).contains(" 2 Tr"));
    // WinAnsi messages keep the standard Helvetica and its bold.
    let plain = EmailMessage {
        from: "Ada <ada@example.org>".into(),
        to: "Team <team@example.org>".into(),
        attachments: Vec::new(),
        ..message("Verbale — lunedì", "Ciao a tutti.")
    };
    let doc = Document::from_email(&plain, &EmailLabels::default(), TextLayout::default()).unwrap();
    let names: Vec<_> = page_fonts(&doc, 0).into_iter().map(|(n, _)| n).collect();
    assert_eq!(names, ["FoliHelv", "FoliHelvB"]);
}

#[test]
fn email_text_that_cannot_be_drawn_is_refused() {
    let labels = EmailLabels::default();
    let err = Document::from_email(&message("漢字", "body"), &labels, TextLayout::default()).unwrap_err();
    assert!(
        matches!(err, Error::Glyphs(GlyphProblem::Missing { .. })),
        "{err:?}"
    );
    let err = Document::from_email(&message("Subject", "שלום"), &labels, TextLayout::default()).unwrap_err();
    assert!(
        matches!(err, Error::Glyphs(GlyphProblem::NeedsLayout(_))),
        "{err:?}"
    );
}
