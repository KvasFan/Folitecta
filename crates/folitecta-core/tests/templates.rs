//! New documents from templates (FOL-L010): papers without fields on the
//! paper chosen, month planners with their days in the right columns, words
//! in the language given (beyond WinAnsi in the embedded font), forms that
//! keep their fields when saved, an invoice that works out its sums.

use std::collections::BTreeMap;

use folitecta_core::forms::FieldKind;
use folitecta_core::{Document, Template, TemplateOptions};

fn text(doc: &Document, page: usize) -> String {
    doc.page_text(page)
}

#[test]
fn every_template_makes_pages_of_the_paper_chosen() {
    let opts = TemplateOptions {
        width: 612.0,
        height: 792.0,
        pages: 2,
        ..TemplateOptions::default()
    };
    for t in Template::ALL {
        let doc = Document::from_template(t, &opts).unwrap_or_else(|e| panic!("{t:?}: {e}"));
        let forms_one_page = matches!(t, Template::Letter | Template::Meeting | Template::Invoice);
        assert_eq!(doc.page_count(), if forms_one_page { 1 } else { 2 }, "{t:?}");
        let size = doc.page_size(0).unwrap();
        assert!(
            (size.width - 612.0).abs() < 0.5 && (size.height - 792.0).abs() < 0.5,
            "{t:?}"
        );
        assert_eq!(!doc.form_fields().is_empty(), t.fillable(), "{t:?}");
        assert!(doc.path().is_none() && !doc.can_undo(), "{t:?}");
    }
}

#[test]
fn month_planners_follow_the_calendar() {
    let opts = TemplateOptions {
        pages: 3,
        month: 11,
        year: 2026,
        ..TemplateOptions::default()
    };
    let doc = Document::from_template(Template::Month, &opts).unwrap();
    assert_eq!(doc.page_count(), 3);
    let (nov, dec, jan) = (text(&doc, 0), text(&doc, 1), text(&doc, 2));
    assert!(nov.contains("November 2026"), "{nov}");
    assert!(dec.contains("December 2026"), "{dec}");
    assert!(jan.contains("January 2027"), "{jan}");
    assert!(nov.contains("30") && !nov.contains("31"), "{nov}");
    assert!(dec.contains("31"));
    // 1 November 2026 is a Sunday: the last column when weeks start on
    // Monday, so the first row holds that one day only.
    let words: Vec<&str> = nov.split_whitespace().collect();
    let sun = words.iter().position(|w| *w == "Sun").unwrap();
    let one = words.iter().position(|w| *w == "1").unwrap();
    assert!(one > sun && words[one + 1] == "2", "{nov}");
    // The 1st in the column of Sunday.
    let layer = doc.text_layer(0).unwrap();
    let chars: Vec<char> = layer.text.chars().collect();
    let sun = layer.text[..layer.text.find("Sun").unwrap()].chars().count();
    let one = (sun..chars.len()).find(|&i| chars[i] == '1').unwrap();
    let left = |i: usize| layer.range_rects(i, i + 1)[0][0];
    assert!(
        (left(one) - left(sun)).abs() < 40.0,
        "{} {}",
        left(one),
        left(sun)
    );

    let sunday_first = Document::from_template(
        Template::Month,
        &TemplateOptions {
            monday_first: false,
            ..opts.clone()
        },
    )
    .unwrap();
    let t = text(&sunday_first, 0);
    let words: Vec<&str> = t.split_whitespace().collect();
    let first_day = words.iter().position(|w| ["Sun", "Mon"].contains(w)).unwrap();
    assert_eq!(words[first_day], "Sun", "{t}");
}

#[test]
fn words_come_in_the_language_given() {
    let mut labels = BTreeMap::new();
    for (k, v) in [
        ("title_week", "Settimana del"),
        ("day_1", "Lunedì"),
        ("day_7", "Domenica"),
        ("notes", "Note"),
    ] {
        labels.insert(k.to_string(), v.to_string());
    }
    let opts = TemplateOptions {
        labels,
        ..TemplateOptions::default()
    };
    let doc = Document::from_template(Template::Week, &opts).unwrap();
    let t = text(&doc, 0);
    for w in ["Settimana del", "Lunedì", "Domenica", "Note", "Tuesday"] {
        assert!(t.contains(w), "{w} in {t}");
    }

    let mut labels = BTreeMap::new();
    labels.insert("title_todo".to_string(), "Список дел".to_string());
    let doc = Document::from_template(
        Template::ToDo,
        &TemplateOptions {
            labels,
            ..TemplateOptions::default()
        },
    )
    .unwrap();
    assert!(text(&doc, 0).contains("Список дел"));
}

#[test]
fn the_invoice_works_out_its_sums_and_keeps_them_when_saved() {
    let doc = Document::from_template(Template::Invoice, &TemplateOptions::default()).unwrap();
    let bytes = doc.bytes().to_vec();
    let mut doc = Document::from_bytes(bytes, None, None).unwrap();
    let names: Vec<String> = doc.form_fields().iter().map(|f| f.name.clone()).collect();
    for n in [
        "From", "Number", "Date", "DueDate", "BillTo", "Item1", "Qty10", "Subtotal", "Tax", "Total",
        "Payment",
    ] {
        assert!(names.iter().any(|f| f == n), "{n} in {names:?}");
    }
    let values: Vec<(String, String)> = [
        ("Qty1", "2"),
        ("Price1", "10.5"),
        ("Qty2", "3"),
        ("Price2", "4"),
        ("Tax", "5"),
    ]
    .iter()
    .map(|(a, b)| (a.to_string(), b.to_string()))
    .collect();
    doc.set_field_values(&values).unwrap();
    let value = |n: &str| {
        doc.form_fields()
            .into_iter()
            .find(|f| f.name == n)
            .map(|f| f.value)
            .unwrap()
    };
    assert_eq!(value("Amount1"), "21");
    assert_eq!(value("Amount2"), "12");
    assert_eq!(value("Subtotal"), "33");
    assert_eq!(value("Total"), "38");
    let total = doc.form_fields().into_iter().find(|f| f.name == "Total").unwrap();
    assert!(total.read_only);
}

#[test]
fn lists_and_notes_are_forms_to_fill_in() {
    let doc = Document::from_template(
        Template::ToDo,
        &TemplateOptions {
            pages: 2,
            ..TemplateOptions::default()
        },
    )
    .unwrap();
    let fields = doc.form_fields();
    let boxes = fields.iter().filter(|f| f.kind == FieldKind::CheckBox).count();
    let lines = fields.iter().filter(|f| f.kind == FieldKind::Text).count();
    assert!(boxes >= 40 && boxes == lines, "{boxes} boxes, {lines} lines");
    assert!(fields.iter().any(|f| f.name == "Done2_1" && f.page == Some(1)));
    assert!(fields.iter().all(|f| !f.name.contains('.')));

    let meeting = Document::from_template(Template::Meeting, &TemplateOptions::default()).unwrap();
    let names: Vec<String> = meeting.form_fields().iter().map(|f| f.name.clone()).collect();
    for n in [
        "Date",
        "Time",
        "Place",
        "Attendees",
        "Agenda",
        "Notes",
        "Decisions",
        "Action1What",
        "Action6When",
    ] {
        assert!(names.iter().any(|f| f == n), "{n} in {names:?}");
    }
    // Every field lies on the page.
    for f in meeting.form_fields() {
        let r = f.rect.unwrap();
        assert!(
            r[0] >= 0.0 && r[1] >= 0.0 && r[2] <= 595.3 && r[3] <= 842.0,
            "{} {r:?}",
            f.name
        );
        assert!(r[3] - r[1] >= 4.0, "{} {r:?}", f.name);
    }
}

#[test]
fn small_papers_still_fit_their_forms() {
    // A6 and a wide receipt: the forms are squeezed, never off the page.
    for (w, h) in [(297.64, 419.53), (842.0, 300.0)] {
        for t in [
            Template::Letter,
            Template::Meeting,
            Template::Invoice,
            Template::Week,
            Template::Month,
        ] {
            let opts = TemplateOptions {
                width: w,
                height: h,
                ..TemplateOptions::default()
            };
            let doc = Document::from_template(t, &opts).unwrap_or_else(|e| panic!("{t:?} {w}x{h}: {e}"));
            for f in doc.form_fields() {
                let r = f.rect.unwrap();
                assert!(
                    r[0] >= 0.0 && r[1] >= 0.0 && r[2] <= w && r[3] <= h,
                    "{t:?} {} {r:?}",
                    f.name
                );
            }
        }
    }
}
