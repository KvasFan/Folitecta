//! Export to office formats: a document with headings, paragraphs on two
//! pages, a picture and a comment exported to DOCX (a ZIP whose parts are
//! well-formed XML, the headings styled, the text in reading order, the
//! picture among it, a page break between the pages), RTF (Unicode escaped,
//! the picture in it) and HTML (the picture in the page); JPEG pictures kept
//! as JPEG, specks left out; what was left out reported.

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
    // OFFICE_DUMP=<dir> writes the exports, to open them elsewhere.
    if let Ok(dir) = std::env::var("OFFICE_DUMP") {
        std::fs::write(format!("{dir}/report.docx"), &out.bytes).unwrap();
        let n = doc.page_count();
        for f in [OfficeFormat::Html, OfficeFormat::Rtf] {
            let bytes = doc.export_office(f, 0..n, true).unwrap().bytes;
            std::fs::write(format!("{dir}/report.{}", f.extension()), bytes).unwrap();
        }
    }
    assert!(
        out.headings >= 3 && out.paragraphs >= 3,
        "{} {}",
        out.headings,
        out.paragraphs
    );
    assert_eq!(out.pictures, 1);
    assert_eq!(out.left_out, vec![("comments", 1)]);
    let parts = read(&out.bytes).unwrap();
    let names: Vec<&str> = parts.iter().map(|p| p.name.as_str()).collect();
    for n in [
        "[Content_Types].xml",
        "_rels/.rels",
        "word/document.xml",
        "word/styles.xml",
        "word/_rels/document.xml.rels",
        "word/media/image1.png",
    ] {
        assert!(names.contains(&n), "{n} in {names:?}");
    }
    let png = &parts
        .iter()
        .find(|p| p.name == "word/media/image1.png")
        .unwrap()
        .data;
    let picture = image::load_from_memory(png).unwrap();
    assert_eq!((picture.width(), picture.height()), (20, 20));
    for p in parts.iter().filter(|p| !p.name.starts_with("word/media/")) {
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
    // The picture, 40 points a side, after the title of its page.
    let blip = body.find("r:embed=\"rIdImg1\"").unwrap();
    assert!(title < blip && blip < sales, "{title} {blip} {sales}");
    assert!(body.contains("cx=\"508000\" cy=\"508000\""), "40 points in EMU");
    let rels = String::from_utf8(
        parts
            .iter()
            .find(|p| p.name == "word/_rels/document.xml.rels")
            .unwrap()
            .data
            .clone(),
    )
    .unwrap();
    assert!(rels.contains("Id=\"rIdImg1\"") && rels.contains("media/image1.png"));
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
    assert!(
        rtf.contains("{\\pict\\pngblip\\picw20\\pich20\\picwgoal800\\pichgoal800"),
        "the picture"
    );
    let html = String::from_utf8(doc.export_office(OfficeFormat::Html, 0..1, true).unwrap().bytes).unwrap();
    assert!(html.contains("<h1>Отчёт annuale</h1>") && html.contains("<p>"));
    assert!(
        html.contains("<img src=\"data:image/png;base64,") && html.contains("width=\"53\""),
        "the picture"
    );
    assert!(!html.contains("Sales grew"), "only the first page");
    assert!(doc.export_office(OfficeFormat::Html, 0..(n + 3), true).is_err());
    let blank = Document::new_blank(1, 200.0, 200.0).unwrap();
    assert!(
        blank.export_office(OfficeFormat::Docx, 0..1, true).is_err(),
        "no text"
    );
}

#[test]
fn jpeg_pictures_stay_jpeg_and_specks_are_left_out() {
    let mut doc = Document::from_text("A page with a photograph.", TextLayout::default()).unwrap();
    let photo = image::RgbImage::from_fn(64, 48, |x, y| image::Rgb([(x * 4) as u8, (y * 5) as u8, 90]));
    let mut jpeg = std::io::Cursor::new(Vec::new());
    photo.write_to(&mut jpeg, image::ImageFormat::Jpeg).unwrap();
    let jpeg = jpeg.into_inner();
    doc.add_image(0, jpeg.clone(), Some([100.0, 400.0, 260.0, 520.0]))
        .unwrap();
    // A speck of 4 points.
    let speck = image::RgbImage::from_pixel(20, 20, image::Rgb([0, 0, 0]));
    let mut png = std::io::Cursor::new(Vec::new());
    speck.write_to(&mut png, image::ImageFormat::Png).unwrap();
    doc.add_image(0, png.into_inner(), Some([300.0, 300.0, 304.0, 304.0]))
        .unwrap();
    let out = doc.export_office(OfficeFormat::Docx, 0..1, false).unwrap();
    assert_eq!(out.pictures, 1);
    assert_eq!(out.left_out, vec![("pictures", 1)]);
    let parts = read(&out.bytes).unwrap();
    let media: Vec<&str> = parts
        .iter()
        .map(|p| p.name.as_str())
        .filter(|n| n.starts_with("word/media/"))
        .collect();
    assert_eq!(media, ["word/media/image1.jpeg"]);
    let data = &parts
        .iter()
        .find(|p| p.name == "word/media/image1.jpeg")
        .unwrap()
        .data;
    assert!(data.starts_with(&[0xff, 0xd8]), "JPEG data");
    let html = String::from_utf8(doc.export_office(OfficeFormat::Html, 0..1, false).unwrap().bytes).unwrap();
    assert!(html.contains("data:image/jpeg;base64,"));
    // A page that is only a picture is exported too.
    let mut only = Document::new_blank(1, 300.0, 300.0).unwrap();
    only.add_image(0, jpeg, Some([20.0, 20.0, 280.0, 280.0])).unwrap();
    let out = only.export_office(OfficeFormat::Html, 0..1, false).unwrap();
    assert_eq!((out.pictures, out.paragraphs), (1, 0));
}

// Tables found in the text of the pages and exported as spreadsheets.

use folitecta_core::testdocs::{Builder, standard_fonts, text_doc};
use folitecta_core::{CellValue, TableFormat};

const CSV: &str = "Fruit,Price,Share\nApples,\"1,234.50\",12%\nPears,7.25,8.5%\nPlums,(3.10),80%\n";

fn rows_of_csv() -> Vec<Vec<String>> {
    vec![
        vec!["Fruit".into(), "Price".into(), "Share".into()],
        vec!["Apples".into(), "1,234.50".into(), "12%".into()],
        vec!["Pears".into(), "7.25".into(), "8.5%".into()],
        vec!["Plums".into(), "(3.10)".into(), "80%".into()],
    ]
}

#[test]
fn a_table_set_from_csv_is_found_again() {
    let doc = Document::from_csv(CSV, None, TextLayout::default(), "Fruit").unwrap();
    let tables = doc.page_tables(0..doc.page_count());
    assert_eq!(tables.len(), 1, "{tables:?}");
    assert_eq!(tables[0].page, 0);
    assert_eq!(tables[0].rows, rows_of_csv());
    let b = tables[0].bbox;
    assert!(b[0] < b[2] && b[1] < b[3], "{b:?}");
}

#[test]
fn a_table_drawn_column_by_column_comes_out_in_rows() {
    // Each column drawn whole before the next, the numbers right-aligned,
    // a title above and a line of text below that are not part of it.
    let mut c = String::from("BT /F3 14 Tf 60 780 Td (Stock at the end of the month) Tj ET\n");
    let names = ["Item", "Bolts", "Nuts", "Washers"];
    let counts = ["Count", "1200", "85", "7"];
    for (k, name) in names.iter().enumerate() {
        c.push_str(&format!("BT /F1 10 Tf 60 {} Td ({name}) Tj ET\n", 740 - 16 * k));
    }
    for (k, count) in counts.iter().enumerate() {
        // Right-aligned at 300: about 5.6 points a digit.
        let x = 300.0 - 5.6 * count.len() as f32;
        c.push_str(&format!("BT /F1 10 Tf {x} {} Td ({count}) Tj ET\n", 740 - 16 * k));
    }
    c.push_str(
        "BT /F2 10 Tf 60 640 Td (The counts are taken by hand on the last Friday of each month.) Tj ET\n",
    );
    let mut b = Builder::new();
    b.page((595.0, 842.0), c.into_bytes(), dictionary_fonts(), vec![]);
    let doc = Document::from_bytes(b.finish(), None, None).unwrap();
    let tables = doc.page_tables(0..1);
    assert_eq!(tables.len(), 1, "{tables:?}");
    assert_eq!(
        tables[0].rows,
        vec![
            vec!["Item".to_string(), "Count".into()],
            vec!["Bolts".into(), "1200".into()],
            vec!["Nuts".into(), "85".into()],
            vec!["Washers".into(), "7".into()],
        ]
    );
}

fn dictionary_fonts() -> lopdf::Dictionary {
    let mut d = lopdf::Dictionary::new();
    d.set("Font", standard_fonts());
    d
}

#[test]
fn columns_of_running_text_are_no_table() {
    let doc = Document::from_bytes(text_doc(2), None, None).unwrap();
    assert_eq!(doc.page_tables(0..2), vec![]);
    let doc = document();
    assert_eq!(doc.page_tables(0..doc.page_count()), vec![]);
}

#[test]
fn tables_become_a_workbook_and_csv() {
    let doc = Document::from_csv(CSV, None, TextLayout::default(), "Fruit").unwrap();
    let tables = doc.page_tables(0..1);
    let names = vec!["Page 1".to_string()];
    let out = doc.export_tables(&tables, TableFormat::Xlsx, &names).unwrap();
    assert_eq!((out.tables, out.rows, out.numbers), (1, 4, 6));
    let parts = read(&out.bytes).unwrap();
    let part =
        |name: &str| String::from_utf8(parts.iter().find(|p| p.name == name).unwrap().data.clone()).unwrap();
    for p in &parts {
        if p.name.ends_with(".xml") || p.name.ends_with(".rels") {
            let text = std::str::from_utf8(&p.data).unwrap();
            let body = &text[text.find("?>").map(|i| i + 2).unwrap_or(0)..];
            assert!(folitecta_core::xml::parse(body).is_ok(), "{}", p.name);
        }
    }
    assert!(part("xl/workbook.xml").contains("<sheet name=\"Page 1\" sheetId=\"1\" r:id=\"rId1\"/>"));
    let sheet = part("xl/worksheets/sheet1.xml");
    assert!(
        sheet.contains("<c r=\"A1\" t=\"inlineStr\" s=\"1\"><is><t xml:space=\"preserve\">Fruit</t>"),
        "{sheet}"
    );
    assert!(sheet.contains("<c r=\"B2\"><v>1234.5</v></c>"), "{sheet}");
    assert!(sheet.contains("<c r=\"C2\" s=\"2\"><v>0.12</v></c>"), "{sheet}");
    assert!(sheet.contains("<c r=\"C3\" s=\"3\"><v>0.085</v></c>"), "{sheet}");
    assert!(sheet.contains("<c r=\"B4\"><v>-3.1</v></c>"), "{sheet}");
    assert!(sheet.contains("<dimension ref=\"A1:C4\"/>"), "{sheet}");
    // OFFICE_DUMP=<dir> writes the workbook, to open it elsewhere.
    if let Ok(dir) = std::env::var("OFFICE_DUMP") {
        std::fs::write(format!("{dir}/tables.xlsx"), &out.bytes).unwrap();
    }

    let csv = doc.export_tables(&tables, TableFormat::Csv, &names).unwrap();
    assert_eq!(
        String::from_utf8(csv.bytes).unwrap(),
        "\"Fruit\",\"Price\",\"Share\"\n\"Apples\",\"1,234.50\",\"12%\"\n\"Pears\",\"7.25\",\"8.5%\"\n\"Plums\",\"(3.10)\",\"80%\"\n"
    );
    assert!(doc.export_tables(&[], TableFormat::Csv, &[]).is_err());
}

#[test]
fn decimal_commas_are_told_by_the_numbers_of_the_table() {
    let csv = "Città;Abitanti;Quota\nRoma;2.749.031;4,6%\nMilano;1.371.498;2,3%\nNapoli;909.048;1,5%\n";
    let doc = Document::from_csv(csv, None, TextLayout::default(), "Città").unwrap();
    let tables = doc.page_tables(0..1);
    assert_eq!(tables.len(), 1, "{tables:?}");
    let out = doc
        .export_tables(&tables, TableFormat::Xlsx, &["Città".into()])
        .unwrap();
    let parts = read(&out.bytes).unwrap();
    let sheet = String::from_utf8(
        parts
            .iter()
            .find(|p| p.name == "xl/worksheets/sheet1.xml")
            .unwrap()
            .data
            .clone(),
    )
    .unwrap();
    assert!(sheet.contains("<c r=\"B2\"><v>2749031</v></c>"), "{sheet}");
    assert!(sheet.contains("<c r=\"C2\" s=\"3\"><v>0.046</v></c>"), "{sheet}");
    assert_eq!(out.numbers, 6);
    assert_eq!(
        folitecta_core::sheets::cell_value("Roma", true),
        CellValue::Text("Roma".into())
    );
}
