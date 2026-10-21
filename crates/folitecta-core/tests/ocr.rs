//! Text recognition with the models of the source tree: a page turned into a
//! picture is read back, and its text written invisibly where the picture
//! shows it, so it can be searched and selected; reading again replaces it.

use std::path::PathBuf;
use std::sync::OnceLock;

use folitecta_core::ocr::find_models;
use folitecta_core::{
    DOUBTFUL_BELOW, Document, OcrEngine, OcrLine, OcrOptions, OcrWord, PageOcr, SearchOptions, TextLayout,
};

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
    // A clean scan leaves nothing to check.
    assert!(scan.doubtful_words(0).is_empty(), "{:?}", scan.doubtful_words(0));
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

/// A word read in a picture at 2 pixels per point, on the line 100 to 124 pixels down.
fn word(text: &str, x: f32, confidence: f32) -> OcrWord {
    OcrWord {
        text: text.into(),
        quad: [[x, 100.0], [x + 60.0, 100.0], [x + 60.0, 124.0], [x, 124.0]],
        confidence,
    }
}

fn doubtful_page() -> Document {
    doubtful_page_turned(0)
}

/// The page turned by `degrees` before the words are written (the pixels are
/// those of the page as shown).
fn doubtful_page_turned(degrees: i32) -> Document {
    let mut doc = Document::new_blank(1, 595.0, 842.0).unwrap();
    if degrees != 0 {
        doc.rotate_pages(&[0], degrees).unwrap();
    }
    let words = vec![
        word("Invoice", 100.0, 0.98),
        word("nunber", 180.0, 0.55),
        word("2O26", 260.0, 0.62),
        word("~", 340.0, 0.3),
    ];
    let line = OcrLine {
        text: "Invoice nunber 2O26 ~".into(),
        quad: [[100.0, 100.0], [400.0, 100.0], [400.0, 124.0], [100.0, 124.0]],
        words,
        confidence: 0.7,
    };
    let read = PageOcr {
        page: 0,
        scale: 2.0,
        lines: vec![line],
    };
    doc.add_ocr_text(&[read]).unwrap();
    doc
}

fn texts(doc: &Document) -> Vec<String> {
    doc.doubtful_words(0).into_iter().map(|w| w.text).collect()
}

#[test]
fn doubtful_words_are_listed_where_they_were_read() {
    let doc = doubtful_page();
    let doubtful = doc.doubtful_words(0);
    assert_eq!(texts(&doc), ["nunber", "2O26", "~"]);
    assert_eq!(doubtful.iter().map(|w| w.index).collect::<Vec<_>>(), [0, 1, 2]);
    assert!((doubtful[0].confidence - 0.55).abs() < 0.01, "{doubtful:?}");
    // Display points: the pixels halved.
    let r = doubtful[0].rect;
    let expected = [90.0, 50.0, 120.0, 62.0];
    assert!(r.iter().zip(expected).all(|(a, b)| (a - b).abs() < 0.5), "{r:?}");
    // The same place on a page turned to be shown sideways.
    let turned = doubtful_page_turned(90);
    let r = turned.doubtful_words(0)[0].rect;
    assert!(r.iter().zip(expected).all(|(a, b)| (a - b).abs() < 0.5), "{r:?}");
    // All the words are text of the page, the doubtful ones too.
    assert!(
        doc.page_text(0).contains("Invoice nunber 2O26"),
        "{}",
        doc.page_text(0)
    );
}

#[test]
fn doubtful_words_are_corrected_kept_or_taken_away_one_step_each() {
    let mut doc = doubtful_page();
    doc.review_ocr_word(0, 0, Some("number")).unwrap();
    assert_eq!(texts(&doc), ["2O26", "~"]);
    // The correction takes the place of the word, at its length.
    let opts = SearchOptions::default();
    let found = &doc.search("number", opts)[0];
    let r = found.rects[0];
    assert!((r[0] - 90.0).abs() < 1.0 && (r[2] - 120.0).abs() < 1.5, "{r:?}");
    doc.review_ocr_word(0, 0, None).unwrap();
    assert_eq!(texts(&doc), ["~"]);
    doc.review_ocr_word(0, 0, Some("  ")).unwrap();
    assert!(doc.doubtful_words(0).is_empty());
    let text = doc.page_text(0);
    assert!(
        text.contains("Invoice number 2O26") && !text.contains('~'),
        "{text}"
    );
    assert!(doc.review_ocr_word(0, 0, None).is_err());
    // Each check is one undo step.
    doc.undo().unwrap();
    assert_eq!(texts(&doc), ["~"]);
    doc.undo().unwrap();
    doc.undo().unwrap();
    assert_eq!(texts(&doc), ["nunber", "2O26", "~"]);
}

#[test]
fn doubtful_words_stay_doubtful_in_the_saved_file() {
    let mut doc = doubtful_page();
    doc.review_ocr_word(0, 1, Some("2026")).unwrap();
    let path = std::env::temp_dir().join(format!("folitecta-ocr-doubt-{}.pdf", std::process::id()));
    doc.save_as(&path).unwrap();
    let mut back = Document::open(&path).unwrap();
    let _ = std::fs::remove_file(&path);
    assert_eq!(texts(&back), ["nunber", "~"]);
    back.review_ocr_word(0, 0, Some("número")).unwrap();
    assert_eq!(texts(&back), ["~"]);
    assert!(back.page_text(0).contains("número 2026"), "{}", back.page_text(0));
    // Reading the page again replaces the words, doubtful ones included.
    back.remove_ocr_text(&[0]).unwrap();
    assert!(back.doubtful_words(0).is_empty());
}

#[test]
fn words_read_wrong_in_a_poor_scan_are_doubtful() {
    let original = Document::from_text(TEXT, TextLayout::default()).unwrap();
    let mut scan = Document::from_bytes(original.bytes().to_vec(), None, None).unwrap();
    scan.rasterize_pages(&[0], 45.0, false).unwrap();
    let read = engine()
        .read_page(scan.bytes(), 0, OcrOptions::default())
        .unwrap();
    let expected: Vec<Vec<&str>> = TEXT.lines().map(|l| l.split(' ').collect()).collect();
    assert_eq!(read.lines.len(), expected.len());
    for (line, want) in read.lines.iter().zip(&expected) {
        assert_eq!(line.words.len(), want.len(), "{:?}", line.text);
        for (w, t) in line.words.iter().zip(want) {
            assert!(
                w.text == *t || w.confidence < DOUBTFUL_BELOW,
                "{:?} read as {:?}, {}",
                t,
                w.text,
                w.confidence
            );
        }
    }
    let doubtful: Vec<String> = read
        .lines
        .iter()
        .flat_map(|l| &l.words)
        .filter(|w| w.confidence < DOUBTFUL_BELOW)
        .map(|w| w.text.clone())
        .collect();
    assert!(!doubtful.is_empty());
    // Written with doubt, they are listed to be checked.
    scan.add_ocr_text(std::slice::from_ref(&read)).unwrap();
    assert_eq!(texts(&scan), doubtful);
}
