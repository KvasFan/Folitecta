//! Barcodes drawn on pages are read back from the rendered pages, and web
//! links are made over those that hold an address.

use folitecta_core::barcodes::find_barcodes;
use folitecta_core::{BarcodeKind, Document};

const ADDRESS: &str = "https://github.com/KvasFan/Folitecta";

#[test]
fn drawn_barcodes_are_read_back() {
    let mut doc = Document::new_blank(2, 595.0, 842.0).unwrap();
    doc.add_barcode(0, [60.0, 60.0, 200.0, 200.0], BarcodeKind::Qr, ADDRESS, false)
        .unwrap();
    doc.add_barcode(
        0,
        [300.0, 80.0, 540.0, 170.0],
        BarcodeKind::Code128,
        "FOLI-0042",
        true,
    )
    .unwrap();
    doc.add_barcode(
        1,
        [100.0, 400.0, 220.0, 520.0],
        BarcodeKind::DataMatrix,
        "Pagina due",
        false,
    )
    .unwrap();
    let found = find_barcodes(doc.bytes(), &[0, 1], None).unwrap();
    let get = |text: &str| found.iter().find(|f| f.text == text);
    let qr = get(ADDRESS).expect("the QR code is read");
    assert_eq!((qr.page, qr.kind.as_str()), (0, "QR Code"));
    // Where it was drawn (display space, y down), within a few points.
    for (a, b) in qr.rect.iter().zip([60.0, 60.0, 200.0, 200.0]) {
        assert!((a - b).abs() < 16.0, "{:?}", qr.rect);
    }
    let bars = get("FOLI-0042").expect("the Code 128 is read");
    assert_eq!(bars.kind, "Code 128");
    assert!(bars.rect[0] > 290.0 && bars.rect[2] < 550.0, "{:?}", bars.rect);
    let dm = get("Pagina due").expect("the Data Matrix is read");
    assert_eq!((dm.page, dm.kind.as_str()), (1, "Data Matrix"));
    // A region around the Code 128 finds only that one.
    let region = find_barcodes(doc.bytes(), &[0], Some([280.0, 60.0, 560.0, 190.0])).unwrap();
    assert_eq!(region.len(), 1, "{region:?}");
    assert_eq!(region[0].text, "FOLI-0042");
    // One undo removes a barcode.
    doc.undo().unwrap();
    assert!(find_barcodes(doc.bytes(), &[1], None).unwrap().is_empty());
}

#[test]
fn barcodes_stand_upright_on_turned_pages() {
    let mut doc = Document::new_blank(1, 595.0, 842.0).unwrap();
    doc.rotate_pages(&[0], 90).unwrap();
    doc.add_barcode(
        0,
        [500.0, 300.0, 700.0, 380.0],
        BarcodeKind::Ean13,
        "590123412345",
        true,
    )
    .unwrap();
    let found = find_barcodes(doc.bytes(), &[0], None).unwrap();
    assert_eq!(found.len(), 1, "{found:?}");
    // The check digit is added to the twelve digits given.
    assert_eq!(found[0].text, "5901234123457");
    assert!(
        found[0].rect[0] > 480.0 && found[0].rect[2] < 720.0,
        "{:?}",
        found[0].rect
    );
}

#[test]
fn web_links_over_barcodes_in_one_undo_step() {
    let mut doc = Document::new_blank(2, 595.0, 842.0).unwrap();
    doc.add_barcode(0, [60.0, 60.0, 200.0, 200.0], BarcodeKind::Qr, ADDRESS, false)
        .unwrap();
    doc.add_barcode(
        1,
        [60.0, 60.0, 200.0, 200.0],
        BarcodeKind::Aztec,
        "www.example.org",
        false,
    )
    .unwrap();
    doc.add_barcode(
        1,
        [300.0, 60.0, 500.0, 140.0],
        BarcodeKind::Code39,
        "NO LINK",
        false,
    )
    .unwrap();
    let found = find_barcodes(doc.bytes(), &[1], None).unwrap();
    assert_eq!(found.len(), 2, "{found:?}");
    // The box of the bars, inside the white margin drawn around them.
    let bars = found.iter().find(|f| f.kind == "Code 39").unwrap();
    assert!(bars.rect[0] > 300.0 && bars.rect[0] < 320.0, "{:?}", bars.rect);
    assert!(bars.rect[2] > 480.0 && bars.rect[2] < 500.0, "{:?}", bars.rect);
    assert_eq!(doc.create_links_from_barcodes(&[0, 1]).unwrap(), 2);
    let uris: Vec<String> = doc.all_links().iter().map(|l| l.target.describe()).collect();
    assert!(uris.iter().any(|u| u.contains(ADDRESS)), "{uris:?}");
    assert!(
        uris.iter().any(|u| u.contains("http://www.example.org")),
        "{uris:?}"
    );
    // Barcodes already under a link get no second one.
    assert_eq!(doc.create_links_from_barcodes(&[0, 1]).unwrap(), 0);
    doc.undo().unwrap();
    assert!(doc.all_links().is_empty());
}

#[test]
fn barcodes_among_text_are_found() {
    let text = "Folitecta renders, annotates and organizes PDF documents. ".repeat(400);
    let mut doc = Document::from_text(&text, Default::default()).unwrap();
    doc.add_barcode(
        0,
        [380.0, 100.0, 520.0, 240.0],
        BarcodeKind::Aztec,
        "Aztec among text",
        false,
    )
    .unwrap();
    doc.add_barcode(
        0,
        [80.0, 500.0, 200.0, 620.0],
        BarcodeKind::DataMatrix,
        "DM among text",
        false,
    )
    .unwrap();
    doc.add_barcode(
        0,
        [250.0, 700.0, 500.0, 780.0],
        BarcodeKind::Ean8,
        "9638507",
        true,
    )
    .unwrap();
    let found = find_barcodes(doc.bytes(), &[0], None).unwrap();
    let mut texts: Vec<&str> = found.iter().map(|f| f.text.as_str()).collect();
    texts.sort();
    assert_eq!(
        texts,
        ["96385074", "Aztec among text", "DM among text"],
        "{found:?}"
    );
}

#[test]
fn text_is_not_read_as_barcodes() {
    let text = (0..60)
        .map(|i| {
            format!(
                "Invoice {i:04} | 7409928 74746{i:02} | 1111 2222 3333 | IIII llll |||| mmmm WWWW 0101 1001 {}",
                "Folitecta renders, annotates and organizes PDF documents."
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let doc = Document::from_text(&text, Default::default()).unwrap();
    let pages: Vec<usize> = (0..doc.page_count()).collect();
    let found = find_barcodes(doc.bytes(), &pages, None).unwrap();
    assert!(found.is_empty(), "{found:?}");
}

#[test]
fn links_from_addresses_in_the_text_are_one_undo_step() {
    let text = (0..30)
        .map(|i| format!("Page {i}: see https://example.org/{i} for more."))
        .collect::<Vec<_>>()
        .join("\n");
    let mut doc = Document::from_text(&text, Default::default()).unwrap();
    let pages: Vec<usize> = (0..doc.page_count()).collect();
    assert_eq!(doc.create_links_from_urls(&pages).unwrap(), 30);
    doc.undo().unwrap();
    assert!(doc.all_links().is_empty());
    assert!(!doc.can_undo());
}
