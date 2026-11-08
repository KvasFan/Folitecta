//! Comparing documents word by word.

use folitecta_core::Document;
use folitecta_core::compare::DiffKind;

fn doc(text: &str) -> Document {
    Document::from_text(text, Default::default()).unwrap()
}

#[test]
fn insertions_deletions_and_changes_are_found() {
    let old = doc(
        "The quick brown fox jumps over the lazy dog.\n\nA second paragraph stays the same.\u{c}Page two old ending.",
    );
    let new = doc(
        "The quick red fox jumps over the dog.\n\nA second paragraph stays the same and grows.\u{c}Page two new ending.",
    );
    let c = old.compare_with(&new, false);
    let kinds: Vec<(DiffKind, &str, &str)> = c
        .differences
        .iter()
        .map(|d| (d.kind, d.old_text.as_str(), d.new_text.as_str()))
        .collect();
    assert!(kinds.contains(&(DiffKind::Changed, "brown", "red")), "{kinds:?}");
    assert!(kinds.contains(&(DiffKind::Deleted, "lazy", "")), "{kinds:?}");
    assert!(
        kinds.contains(&(DiffKind::Inserted, "", "and grows")),
        "{kinds:?}"
    );
    assert!(kinds.contains(&(DiffKind::Changed, "old", "new")), "{kinds:?}");
    let last = c.differences.iter().find(|d| d.old_text == "old").unwrap();
    assert_eq!((last.old_page, last.new_page), (1, 1));
    assert!(!last.old[0].rects.is_empty() && !last.new[0].rects.is_empty());
    assert_eq!(c.new_pages_changed(), vec![0, 1]);
    assert_eq!(c.count(DiffKind::Changed), 2);
    // Case can be ignored.
    let upper = doc("THE QUICK BROWN FOX");
    let lower = doc("the quick brown fox");
    assert_eq!(upper.compare_with(&lower, false).differences.len(), 1);
    assert!(upper.compare_with(&lower, true).differences.is_empty());
    // Identical documents.
    assert!(old.compare_with(&old, false).differences.is_empty());
}

fn png(r: u8, g: u8, b: u8) -> Vec<u8> {
    let img = image::RgbImage::from_pixel(24, 16, image::Rgb([r, g, b]));
    let mut out = std::io::Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png).unwrap();
    out.into_inner()
}

#[test]
fn pictures_added_removed_replaced_and_moved_are_differences() {
    let (red, green, blue, grey) = (png(200, 0, 0), png(0, 200, 0), png(0, 0, 200), png(90, 90, 90));
    let mut old = Document::new_blank(2, 400.0, 400.0).unwrap();
    old.add_image(0, red.clone(), Some([20.0, 20.0, 80.0, 60.0]))
        .unwrap();
    old.add_image(0, green.clone(), Some([20.0, 200.0, 80.0, 240.0]))
        .unwrap();
    old.add_image(1, blue.clone(), Some([100.0, 100.0, 160.0, 140.0]))
        .unwrap();
    let mut new = Document::new_blank(2, 400.0, 400.0).unwrap();
    // Kept where it was; moved; added; replaced by another where it stood.
    new.add_image(0, red, Some([20.0, 20.0, 80.0, 60.0])).unwrap();
    new.add_image(0, green, Some([200.0, 200.0, 260.0, 240.0]))
        .unwrap();
    new.add_image(0, grey.clone(), Some([200.0, 20.0, 260.0, 60.0]))
        .unwrap();
    new.add_image(1, grey, Some([102.0, 100.0, 162.0, 140.0]))
        .unwrap();
    let c = old.compare_with(&new, false);
    let got: Vec<(DiffKind, usize, usize, bool, bool)> = c
        .differences
        .iter()
        .map(|d| {
            (
                d.kind,
                d.old_page,
                d.new_page,
                !d.old.is_empty(),
                !d.new.is_empty(),
            )
        })
        .collect();
    assert!(c.differences.iter().all(|d| d.picture), "{c:#?}");
    assert_eq!(
        got,
        vec![
            (DiffKind::Changed, 0, 0, true, true),
            (DiffKind::Inserted, 0, 0, false, true),
            (DiffKind::Changed, 1, 1, true, true),
        ],
        "{c:#?}"
    );
    // The moved picture: where it was and where it is (display space).
    let moved = &c.differences[0];
    assert!((moved.old[0].rects[0][0] - 20.0).abs() < 0.5 && (moved.new[0].rects[0][0] - 200.0).abs() < 0.5);
    // The same pictures in the same places: no difference.
    assert!(old.compare_with(&old, false).differences.is_empty());
    // A picture taken away: the blue one of the second page.
    let mut fewer = Document::new_blank(2, 400.0, 400.0).unwrap();
    fewer
        .add_image(0, png(200, 0, 0), Some([20.0, 20.0, 80.0, 60.0]))
        .unwrap();
    fewer
        .add_image(0, png(0, 200, 0), Some([20.0, 200.0, 80.0, 240.0]))
        .unwrap();
    let c = old.compare_with(&fewer, false);
    assert_eq!(c.count(DiffKind::Deleted), 1, "{c:#?}");
}
