//! Export to office formats: a document with headings, paragraphs on two
//! pages, a picture and a comment exported to DOCX (a ZIP whose parts are
//! well-formed XML, the headings styled, the text in reading order, a page
//! break between the pages), RTF (Unicode escaped) and HTML; what was left
//! out reported.

use folitecta_core::zip::read;
use folitecta_core::{Document, OfficeFormat, TextLayout};

fn document() -> Document {
    let long = "Plain text of the first part, with a few words more. ".repeat(80);
    let md = format!(
        "# Отчёт annuale\n\n{long}\n\n## Results\n\nThe results are good.\n\n### Sales\n\nSales grew.\n"
    );
    let mut doc = Document::from_markdown(&md, TextLayout::default(), "Report").unwrap();
    let img = image::RgbImage::from_pixel(20, 20, image::Rgb([200, 30, 30]));
    let mut png = std::io::Cursor::new(Vec::new());
    img.write_to(&mut png, image::ImageFormat::Png).unwrap();
    doc.add_image(0, png.into_inner(), Some([100.0, 100.0, 140.0, 140.0]))
        .unwrap();
    doc.add_note(0, 300.0, 300.0, "a note", "Tester", [1.0, 1.0, 0.0])
        .unwrap();
    doc
}

#[test]
fn a_document_becomes_a_word_document() {
    let doc = document();
    assert!(doc.page_count() >= 2);
    let before = doc.bytes();
    let out = doc
        .export_office(OfficeFormat::Docx, 0..doc.page_count(), true)
        .unwrap();
    assert_eq!(doc.bytes(), before);
    assert!(
        out.headings >= 3 && out.paragraphs >= 3,
        "{} {}",
        out.headings,
        out.paragraphs
    );
    assert_eq!(out.left_out, vec![("pictures", 1), ("comments", 1)]);
    let parts = read(&out.bytes).unwrap();
    let names: Vec<&str> = parts.iter().map(|p| p.name.as_str()).collect();
    for n in [
        "[Content_Types].xml",
        "_rels/.rels",
        "word/document.xml",
        "word/styles.xml",
        "word/_rels/document.xml.rels",
    ] {
        assert!(names.contains(&n), "{n} in {names:?}");
    }
    for p in &parts {
        let text = String::from_utf8(p.data.clone()).unwrap();
        assert!(
            folitecta_core::xml::parse(&text[text.find("?>").map(|i| i + 2).unwrap_or(0)..]).is_ok(),
            "{} is XML",
            p.name
        );
    }
    let body = String::from_utf8(
        parts
            .iter()
            .find(|p| p.name == "word/document.xml")
            .unwrap()
            .data
            .clone(),
    )
    .unwrap();
    let h1 = body.find("Heading1").unwrap();
    let title = body.find("Отчёт annuale").unwrap();
    let results = body.find("Results").unwrap();
    let sales = body.find("Sales grew").unwrap();
    assert!(h1 < title && title < results && results < sales);
    assert!(body.contains("w:type=\"page\""), "a page break");
    assert!(body.contains("Heading2") && body.contains("Heading3"));
}

#[test]
fn rich_text_and_web_pages() {
    let doc = document();
    let n = doc.page_count();
    let rtf = String::from_utf8(doc.export_office(OfficeFormat::Rtf, 0..n, false).unwrap().bytes).unwrap();
    assert!(rtf.starts_with("{\\rtf1") && rtf.trim_end().ends_with('}'));
    assert!(rtf.contains("\\u1054?"), "Cyrillic escaped");
    assert!(!rtf.contains("\\page"), "no page breaks asked for");
    let html = String::from_utf8(doc.export_office(OfficeFormat::Html, 0..1, true).unwrap().bytes).unwrap();
    assert!(html.contains("<h1>Отчёт annuale</h1>") && html.contains("<p>"));
    assert!(!html.contains("Sales grew"), "only the first page");
    assert!(doc.export_office(OfficeFormat::Html, 0..(n + 3), true).is_err());
    let blank = Document::new_blank(1, 200.0, 200.0).unwrap();
    assert!(
        blank.export_office(OfficeFormat::Docx, 0..1, true).is_err(),
        "no text"
    );
}
