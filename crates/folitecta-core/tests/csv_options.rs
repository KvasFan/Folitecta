//! A table made from CSV with its separator, first row and turn chosen
//! instead of found.

use folitecta_core::csvdoc::{CsvOptions, TableTurn, csv_shape};
use folitecta_core::{Document, TextLayout};

const CSV: &str = "Name;Amount\nRossi;1234,50\nBianchi;20,00\n";

#[test]
fn what_is_found_and_what_is_chosen() {
    let found = csv_shape(CSV, &CsvOptions::default());
    assert_eq!(found, (3, 2, ';', true));
    // A comma separates nothing here: one column.
    let comma = CsvOptions {
        separator: Some(','),
        ..CsvOptions::default()
    };
    assert_eq!(
        csv_shape(CSV, &comma).1,
        2,
        "the decimal commas split the amounts"
    );
    let no_header = CsvOptions {
        header: Some(false),
        ..CsvOptions::default()
    };
    assert!(!csv_shape(CSV, &no_header).3);
}

#[test]
fn the_pages_turn_as_chosen() {
    let make = |turn| {
        let opts = CsvOptions {
            turn,
            ..CsvOptions::default()
        };
        let doc = Document::from_csv_with(CSV, &opts, TextLayout::default(), "Ledger").unwrap();
        let s = doc.page_size(0).unwrap();
        s.width > s.height
    };
    assert!(!make(TableTurn::Auto), "a narrow table stays upright");
    assert!(make(TableTurn::Sideways));
    assert!(!make(TableTurn::Upright));
    // The text is all there whatever the choices.
    let opts = CsvOptions {
        header: Some(false),
        ..CsvOptions::default()
    };
    let doc = Document::from_csv_with(CSV, &opts, TextLayout::default(), "Ledger").unwrap();
    let text = doc.page_text(0);
    assert!(text.contains("Name") && text.contains("Bianchi"), "{text}");
}
