//! Text recognition with the models of the source tree: a page turned into a
//! picture is read back, and its text written invisibly where the picture
//! shows it, so it can be searched and selected; reading again replaces it.

use std::path::PathBuf;
use std::sync::OnceLock;

use folitecta_core::ocr::find_models;
use folitecta_core::{Document, OcrEngine, OcrOptions, SearchOptions, TextLayout};

fn engine() -> &'static OcrEngine {
    static ENGINE: OnceLock<OcrEngine> = OnceLock::new();
    ENGINE.get_or_init(|| {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../ocr");
        OcrEngine::load(&find_models(&[dir]).expect("the models of the source tree")).unwrap()
    })
}

const TEXT: &str =
    "The quick brown fox jumps over the lazy dog.\nIl gatto \u{e8} sul tavolo, perch\u{e9} fa caldo.";

/// The middle of a rectangle.
fn mid(r: [f32; 4]) -> (f32, f32) {
    ((r[0] + r[2]) / 2.0, (r[1] + r[3]) / 2.0)
}

#[test]
fn a_scanned_page_is_read_and_its_text_written_where_it_shows() {
    let original = Document::from_text(TEXT, TextLayout::default()).unwrap();
    let mut scan = Document::from_bytes(original.bytes().to_vec(), None, None).unwrap();
    scan.rasterize_pages(&[0], 150.0, false).unwrap();
    assert!(scan.page_text(0).trim().is_empty());

    let t = std::time::Instant::now();
    let read = engine()
        .read_page(scan.bytes(), 0, OcrOptions::default())
        .unwrap();
    eprintln!("page read in {:?}", t.elapsed());
    let lines: Vec<&str> = read.lines.iter().map(|l| l.text.as_str()).collect();
    assert_eq!(lines, TEXT.lines().collect::<Vec<_>>());
    assert!(read.lines.iter().all(|l| l.confidence > 0.8));
    assert_eq!(read.lines[0].words.len(), 9);

    assert_eq!(scan.add_ocr_text(std::slice::from_ref(&read)).unwrap(), 1);
    assert!(scan.has_ocr_text(0));
    let text = scan.page_text(0);
    assert!(
        text.contains("The quick brown fox jumps over the lazy dog."),
        "{text}"
    );
    assert!(text.contains("perch\u{e9} fa caldo"), "{text}");

    // The words are found where the picture shows them.
    let opts = SearchOptions::default();
    for word in ["quick", "lazy", "tavolo"] {
        let (a, b) = (&scan.search(word, opts)[0], &original.search(word, opts)[0]);
        let ((ax, ay), (bx, by)) = (mid(a.rects[0]), mid(b.rects[0]));
        let (aw, bw) = (a.rects[0][2] - a.rects[0][0], b.rects[0][2] - b.rects[0][0]);
        assert!(
            (ax - bx).abs() < 3.0 && (ay - by).abs() < 4.0 && (aw / bw - 1.0).abs() < 0.3,
            "{word}: read at {:?}, drawn at {:?}",
            a.rects[0],
            b.rects[0]
        );
    }

    // Reading again replaces the text; it can be taken away and brought back.
    scan.add_ocr_text(std::slice::from_ref(&read)).unwrap();
    assert_eq!(scan.marks_on_page(0).iter().filter(|m| *m == "OCR").count(), 1);
    assert_eq!(scan.remove_ocr_text(&[0]).unwrap(), 1);
    assert!(!scan.has_ocr_text(0) && scan.page_text(0).trim().is_empty());
    scan.undo().unwrap();
    assert!(scan.has_ocr_text(0));
}

#[test]
fn a_page_scanned_sideways_and_turned_upright_is_read_as_shown() {
    let original = Document::from_text(TEXT, TextLayout::default()).unwrap();
    // A picture of the page lying on its side, on a page turned so it shows upright.
    let mut doc = Document::from_bytes(original.bytes().to_vec(), None, None).unwrap();
    doc.rotate_pages(&[0], 90).unwrap();
    doc.rasterize_pages(&[0], 150.0, false).unwrap();
    doc.rotate_pages(&[0], 270).unwrap();
    assert_eq!(doc.page_size(0).unwrap().rotation, 270);
    let read = engine().read_page(doc.bytes(), 0, OcrOptions::default()).unwrap();
    assert_eq!(read.lines.len(), 2);
    doc.add_ocr_text(&[read]).unwrap();
    let text = doc.page_text(0);
    assert!(text.contains("quick brown fox"), "{text}");
    // Found where the upright page shows the word.
    let opts = SearchOptions::default();
    let (a, b) = (&doc.search("lazy", opts)[0], &original.search("lazy", opts)[0]);
    let ((ax, ay), (bx, by)) = (mid(a.rects[0]), mid(b.rects[0]));
    assert!(
        (ax - bx).abs() < 3.0 && (ay - by).abs() < 4.0,
        "{:?} / {:?}",
        a.rects[0],
        b.rects[0]
    );
}
