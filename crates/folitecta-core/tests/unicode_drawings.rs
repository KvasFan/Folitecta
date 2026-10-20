//! Generated text beyond WinAnsi in tables, calculator tapes and designed
//! stamps (FOL-L018/S02): drawn with an embedded subset of FreeSans and its
//! ToUnicode map, saved and opened again, found after flattening; bold text
//! in that font is stroked; WinAnsi text keeps the standard Helvetica as
//! before; text that cannot be drawn is refused before any change.

use std::path::PathBuf;

use folitecta_core::calculator::{Tape, TapeLine};
use folitecta_core::stamp_design::{StampDesign, StampElement, StampValues};
use folitecta_core::tables::Table;
use folitecta_core::{Document, Error, FlattenScope, GlyphProblem};
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
