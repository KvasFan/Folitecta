//! Every page keeps the revision at which its picture last changed, so the
//! interface draws again only the pages an edit touched. A page whose
//! revision did not change must look exactly as before the edit; the pages
//! the edit changed must have the new revision; the text of a page is kept
//! across edits of its comments only.

use std::sync::Arc;

use folitecta_core::testdocs::{long_doc, report_doc};
use folitecta_core::{Document, Markup, RenderOpts, Renderer, Shape, TextMarkOptions};

/// The pixels of every page.
fn pictures(doc: &Document) -> Vec<Vec<u8>> {
    let r = Renderer::new(doc.bytes()).unwrap();
    (0..doc.page_count())
        .map(|p| r.render_rgba(p, 0.4, RenderOpts::default()).unwrap().into_raw())
        .collect()
}

fn revisions(doc: &Document) -> Vec<u64> {
    (0..doc.page_count()).map(|p| doc.page_revision(p)).collect()
}

fn text_revisions(doc: &Document) -> Vec<u64> {
    (0..doc.page_count()).map(|p| doc.text_revision(p)).collect()
}

/// Run `edit` and check the pages: those not given the new revision look
/// as before; returns the pages that got it.
fn check(doc: &mut Document, what: &str, edit: impl FnOnce(&mut Document)) -> Vec<usize> {
    let before = pictures(doc);
    let revs = revisions(doc);
    edit(doc);
    let after = pictures(doc);
    let now = revisions(doc);
    assert_eq!(before.len(), after.len(), "{what}: page count");
    let mut changed = Vec::new();
    for p in 0..after.len() {
        if now[p] == revs[p] {
            assert!(
                after[p] == before[p],
                "{what}: page {p} looks different but kept its revision"
            );
        } else {
            assert_eq!(now[p], doc.revision(), "{what}: page {p}");
            changed.push(p);
        }
    }
    changed
}

#[test]
fn an_edit_of_one_page_changes_the_revision_of_that_page_only() {
    let mut doc = Document::from_bytes(long_doc(6), None, None).unwrap();
    let changed = check(&mut doc, "a note", |d| {
        d.add_note(2, 300.0, 300.0, "A note", "Reviewer", [1.0, 0.8, 0.2])
            .unwrap();
    });
    assert_eq!(changed, vec![2]);
    let changed = check(&mut doc, "a rectangle", |d| {
        d.add_shape(
            4,
            Shape::Square,
            [100.0, 100.0, 200.0, 160.0],
            [0.0, 0.0, 1.0],
            None,
            2.0,
            "Reviewer",
            "",
        )
        .unwrap();
    });
    assert_eq!(changed, vec![4]);
    let index = doc.annotations(4).unwrap().last().unwrap().index;
    let changed = check(&mut doc, "a rectangle moved", |d| {
        d.set_annotation_rect(4, index, [120.0, 120.0, 220.0, 180.0])
            .unwrap();
    });
    assert_eq!(changed, vec![4]);
    let changed = check(&mut doc, "a comment text", |d| {
        d.set_annotation_contents(4, index, "Changed").unwrap();
    });
    assert!(changed.iter().all(|p| *p == 4), "{changed:?}");
    let changed = check(&mut doc, "a comment deleted", |d| {
        d.delete_annotation(4, index).unwrap();
    });
    assert_eq!(changed, vec![4]);
    let quad = [60.0, 760.0, 300.0, 760.0, 60.0, 748.0, 300.0, 748.0];
    let changed = check(&mut doc, "a highlight", |d| {
        d.add_text_markup(1, Markup::Highlight, &[quad], [1.0, 0.9, 0.2], "Reviewer", "")
            .unwrap();
    });
    assert_eq!(changed, vec![1]);
    let changed = check(&mut doc, "a page turned", |d| d.rotate_pages(&[3], 90).unwrap());
    assert_eq!(changed, vec![3]);
    let changed = check(&mut doc, "the metadata", |d| {
        d.set_info("Title", "", "", "").unwrap()
    });
    assert!(changed.is_empty(), "{changed:?}");
    let changed = check(&mut doc, "a bookmark", |d| {
        d.add_bookmark("Chapter", 5, &[]).unwrap();
    });
    assert!(changed.len() <= doc.page_count());
    let changed = check(&mut doc, "an undo", |d| d.undo().unwrap());
    assert!(changed.is_empty() || changed.len() == 6, "{changed:?}");
    let changed = check(&mut doc, "an undo of the turn", |d| {
        d.undo().unwrap();
        d.undo().unwrap();
    });
    assert!(changed.contains(&3), "{changed:?}");
    let changed = check(&mut doc, "a redo", |d| d.redo().unwrap());
    assert!(changed.contains(&3) || changed.is_empty(), "{changed:?}");
    // Revisions never go back: an undo is a new revision.
    assert!(revisions(&doc).iter().all(|r| *r <= doc.revision()));
}

#[test]
fn a_change_of_what_pages_share_changes_every_page_using_it() {
    // Pages set in the same embedded fonts, with comments.
    let mut doc = Document::from_bytes(report_doc(5), None, None).unwrap();
    let changed = check(&mut doc, "a watermark on page 1", |d| {
        d.add_text_watermark(&[0], &TextMarkOptions::default()).unwrap();
    });
    assert!(changed.contains(&0), "{changed:?}");
    let changed = check(&mut doc, "page numbers on every page", |d| {
        d.add_page_numbers(
            &[0, 1, 2, 3, 4],
            "{page}",
            1,
            folitecta_core::MarkPosition::BottomCenter,
            9.0,
            24.0,
            [0.0, 0.0, 0.0],
        )
        .unwrap();
    });
    assert_eq!(changed, vec![0, 1, 2, 3, 4]);
    // Pages coming or going: every page.
    let before = revisions(&doc);
    doc.delete_pages(&[1]).unwrap();
    assert!(revisions(&doc).iter().all(|r| *r == doc.revision()));
    assert_eq!(before.len(), doc.page_count() + 1);
}

#[test]
fn the_text_of_a_page_is_kept_across_edits_of_comments() {
    let mut doc = Document::from_bytes(long_doc(4), None, None).unwrap();
    let text = doc.text_layers();
    let texts = text_revisions(&doc);
    doc.add_note(1, 300.0, 300.0, "A note", "Reviewer", [1.0, 0.8, 0.2])
        .unwrap();
    assert_eq!(text_revisions(&doc), texts, "a comment leaves the text");
    let again = doc.text_layers();
    assert_eq!(again.len(), text.len());
    for (a, b) in again.iter().zip(text.iter()) {
        assert_eq!(a.text, b.text);
    }
    // One page alone is extracted on demand, and kept.
    let one = doc.page_text_layer(2).unwrap();
    assert!(Arc::ptr_eq(&one, &doc.page_text_layer(2).unwrap()));
    assert_eq!(one.text, text[2].text);
    // A change of the content of a page: its text is extracted again.
    doc.add_text_watermark(&[2], &TextMarkOptions::default()).unwrap();
    assert!(doc.text_revision(2) == doc.revision());
    assert_eq!(doc.text_revision(0), texts[0]);
    let fresh = doc.page_text_layer(2).unwrap();
    assert!(!Arc::ptr_eq(&one, &fresh));
}

#[test]
fn the_saved_file_looks_as_the_pages_shown() {
    let dir = std::env::temp_dir().join(format!("folitecta-page-revisions-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("revisions.pdf");
    let mut doc = Document::from_bytes(long_doc(4), None, None).unwrap();
    doc.add_note(1, 300.0, 300.0, "A note", "Reviewer", [1.0, 0.8, 0.2])
        .unwrap();
    doc.rotate_pages(&[2], 90).unwrap();
    let shown = pictures(&doc);
    doc.save_as(&path).unwrap();
    let reopened = Document::open(&path).unwrap();
    assert!(pictures(&reopened) == shown);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_state_worked_away_from_the_document_redraws_the_pages_it_changed() {
    // Applying redaction works on a copy on another thread, then the
    // document takes its state as one edit.
    let mut doc = Document::from_bytes(long_doc(6), None, None).unwrap();
    let changed = check(&mut doc, "an adopted state", |d| {
        let mut work = d.snapshot().open().unwrap();
        work.add_note(3, 300.0, 300.0, "A note", "Reviewer", [1.0, 0.8, 0.2])
            .unwrap();
        d.adopt_state(work).unwrap();
    });
    assert!(changed.contains(&3), "{changed:?}");
    assert!(!changed.contains(&0), "{changed:?}");
}

#[test]
fn a_field_with_widgets_on_two_pages_gives_both_pages_a_new_revision() {
    use lopdf::{Dictionary, Object, dictionary};
    // A text field with a widget on page 1 and another on page 2 (the same
    // field shown twice, as a name on every page); page 3 has none.
    let mut b = folitecta_core::testdocs::Builder::new();
    let field = b.doc.new_object_id();
    let widget = |b: &mut folitecta_core::testdocs::Builder| {
        b.doc.add_object(dictionary! {
            "Type" => "Annot",
            "Subtype" => "Widget",
            "Rect" => vec![100.into(), 700.into(), 400.into(), 724.into()],
            "F" => 4,
            "Parent" => field,
        })
    };
    let widgets = [widget(&mut b), widget(&mut b)];
    for w in widgets {
        let page = b.page((595.0, 842.0), Vec::new(), Dictionary::new(), vec![w.into()]);
        if let Ok(Object::Dictionary(d)) = b.doc.get_object_mut(w) {
            d.set("P", page);
        }
    }
    b.page(
        (595.0, 842.0),
        b"0 g 50 50 100 100 re f".to_vec(),
        Dictionary::new(),
        Vec::new(),
    );
    b.doc.objects.insert(
        field,
        Object::Dictionary(dictionary! {
            "FT" => "Tx",
            "T" => Object::string_literal("name"),
            "DA" => Object::string_literal("/Helv 12 Tf 0 g"),
            "Kids" => widgets.iter().map(|w| Object::Reference(*w)).collect::<Vec<_>>(),
        }),
    );
    let helv = dictionary! { "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica", "Encoding" => "WinAnsiEncoding" };
    b.catalog_entry(
        "AcroForm",
        Object::Dictionary(dictionary! {
            "Fields" => vec![Object::Reference(field)],
            "DA" => Object::string_literal("/Helv 12 Tf 0 g"),
            "DR" => dictionary! { "Font" => dictionary! { "Helv" => helv } },
        }),
    );
    let mut doc = Document::from_bytes(b.finish(), None, None).unwrap();
    let id = doc
        .form_fields()
        .into_iter()
        .find(|f| f.name == "name")
        .expect("the field")
        .id;
    // A first edit elsewhere (after opening, every page counts as changed).
    doc.add_note(2, 300.0, 300.0, "A note", "Reviewer", [1.0, 0.8, 0.2])
        .unwrap();
    let before = pictures(&doc);
    let changed = check(&mut doc, "the value of a field on two pages", |d| {
        d.set_field_value(id, "Ada Lovelace").unwrap()
    });
    assert_eq!(changed, vec![0, 1]);
    // The value shows on both pages.
    let after = pictures(&doc);
    assert!(after[0] != before[0] && after[1] != before[1]);
    // Saved and opened again, both widgets show the value.
    let saved = Document::from_bytes(doc.bytes().to_vec(), None, None).unwrap();
    assert_eq!(pictures(&saved), pictures(&doc));
}
