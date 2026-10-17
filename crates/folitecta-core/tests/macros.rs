//! Macros of page-mark texts.

use folitecta_core::{Document, MACROS, MarkPosition, TextMarkOptions};

/// The test document, saved in a folder of its own for each test (tests run
/// at the same time).
fn doc(test: &str) -> (Document, std::path::PathBuf) {
    let mut doc = Document::new_blank(3, 300.0, 400.0).unwrap();
    doc.set_info("Annual report", "Ada", "Finance", "money, plans")
        .unwrap();
    doc.set_page_labels(&[(0, "r", "", 1), (1, "D", "P-", 1)])
        .unwrap();
    doc.add_bookmark("Intro", 0, &[]).unwrap();
    doc.add_bookmark("Results", 2, &[]).unwrap();
    let dir = std::env::temp_dir().join(format!("foli-macros-{}-{test}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("report.pdf");
    doc.save_as(&path).unwrap();
    (Document::open(&path).unwrap(), dir)
}

#[test]
fn macros_expand_per_page() {
    let (doc, dir) = doc("expand");
    let x = |s: &str, p: usize| doc.expand_macros(s, p, 1);
    assert_eq!(x("{page}/{pages} {page:3}", 1), "2/3 002");
    assert_eq!(x("{label}", 0), "i");
    assert_eq!(x("{label}", 2), "P-2");
    assert_eq!(x("{bates:5} {autonumber:2}", 2), "00003 03");
    assert_eq!(
        x("{title}|{author}|{subject}|{keywords}", 0),
        "Annual report|Ada|Finance|money, plans"
    );
    assert_eq!(x("{docinfo:Author} {docinfo:Missing}.", 0), "Ada .");
    assert_eq!(x("{file} {filename} {ext}", 0), "report.pdf report pdf");
    assert_eq!(x("{foldername}", 0), dir.file_name().unwrap().to_string_lossy());
    assert_eq!(x("{folder}", 0), dir.display().to_string());
    assert_eq!(x("{bookmark}", 0), "Intro");
    assert_eq!(x("{bookmark}", 1), "Intro");
    assert_eq!(x("{bookmark}", 2), "Results");
    assert_eq!(x("{docnumber} {srcpages} {imagenumber}", 1), "1 3 2");
    assert!(x("{app}", 0).starts_with("Folitecta "));
    // Unknown and unterminated braces stay as typed.
    assert_eq!(x("{nope} {page", 0), "{nope} {page");
    let year = x("{year}", 0);
    assert_eq!(year.len(), 4);
    assert_eq!(
        x("{date}", 0),
        format!("{year}-{}-{}", x("{month}", 0), x("{day}", 0))
    );
    assert_eq!(x("{time}", 0).len(), 5);
    let path = std::env::var("PATH").unwrap_or_default();
    assert_eq!(x("{env:PATH}", 0), path);
    // Every listed macro expands (no braces left except the environment one).
    for (token, _) in MACROS {
        if !token.starts_with("{env") {
            assert!(!x(token, 0).contains('{'), "{token}");
        }
    }
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn page_numbers_use_the_page_and_the_running_number() {
    let (mut doc, dir) = doc("numbers");
    doc.add_page_numbers(
        &[1, 2],
        "{label} #{bates:4}",
        10,
        MarkPosition::BottomRight,
        9.0,
        20.0,
        [0.0, 0.0, 0.0],
    )
    .unwrap();
    assert!(doc.page_text(1).contains("P-1 #0010"));
    assert!(doc.page_text(2).contains("P-2 #0011"));
    doc.add_text_watermark(
        &[0],
        &TextMarkOptions {
            text: "{title} {page}".into(),
            position: MarkPosition::TopLeft,
            rotation: 0.0,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(doc.page_text(0).contains("Annual report 1"));
    let _ = std::fs::remove_dir_all(dir);
}
