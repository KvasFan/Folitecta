use folitecta_core::Document;
use folitecta_core::combine::{Source, source_pages};
use folitecta_core::textdoc::TextLayout;

#[test]
fn pdf_pictures_and_text_combine_in_order() {
    let dir = std::env::temp_dir().join("folitecta-combine-test");
    let _ = std::fs::create_dir_all(&dir);
    let pdf = dir.join("three.pdf");
    let mut doc = Document::from_text("one\u{c}two\u{c}three", TextLayout::default()).unwrap();
    doc.save_as(&pdf).unwrap();
    let png = dir.join("dot.png");
    let img = image::RgbImage::from_pixel(8, 4, image::Rgb([200, 30, 30]));
    img.save(&png).unwrap();
    let txt = dir.join("notes.txt");
    std::fs::write(&txt, "Combined notes").unwrap();
    let missing = dir.join("missing.pdf");
    assert_eq!(source_pages(&pdf).unwrap(), 3);
    assert_eq!(source_pages(&png).unwrap(), 1);
    let (combined, problems) = Document::combine(&[
        Source {
            path: pdf.clone(),
            pages: Some(vec![2, 0]),
        },
        Source {
            path: png.clone(),
            pages: None,
        },
        Source {
            path: missing.clone(),
            pages: None,
        },
        Source {
            path: txt.clone(),
            pages: None,
        },
    ])
    .unwrap();
    assert_eq!(combined.page_count(), 4);
    assert_eq!(combined.page_text(0), "three");
    assert_eq!(combined.page_text(1), "one");
    assert_eq!(combined.page_text(3), "Combined notes");
    assert_eq!(problems.len(), 1);
    assert_eq!(problems[0].0, missing);
    assert!(combined.path().is_none() && !combined.can_undo());
    // Nothing readable: an error.
    assert!(
        Document::combine(&[Source {
            path: missing,
            pages: None
        }])
        .is_err()
    );
    let _ = std::fs::remove_dir_all(&dir);
}
