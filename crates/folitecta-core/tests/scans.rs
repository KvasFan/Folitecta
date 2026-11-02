//! Clean-up of scanned pages: a blank sheet is found and taken out, a sheet
//! fed askew is found and straightened, pages that are not scans are left
//! alone; the clean-up is one undo step.

use folitecta_core::imaging::{skew_angle, turn};
use folitecta_core::{Document, ScanFix, ScanThresholds, TextLayout, scan_fixes};
use image::Rgba;

fn text(lines: usize) -> String {
    (0..lines)
        .map(|i| format!("Line {i} of the letter: the quick brown fox jumps over the lazy dog."))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Three scanned sheets: a letter, a blank side, a letter fed 2 degrees askew;
/// and a fourth page that is not a scan.
fn scans() -> Document {
    let source = format!("{}\u{c}\u{c}{}\u{c}{}", text(40), text(40), text(3));
    let mut doc = Document::from_text(&source, TextLayout::default()).unwrap();
    assert_eq!(doc.page_count(), 4);
    doc.rasterize_pages(&[0, 1, 2], 100.0, false).unwrap();
    let px = doc.image_pixels(2, 0).unwrap();
    let im = doc.page_images(2).unwrap()[0].clone();
    let turned = turn(&px, 2.0, Rgba([255, 255, 255, 255]));
    doc.replace_page_image(2, 0, &turned, [1.0, 0.0, 0.0, 1.0, 0.0, 0.0], im.colors)
        .unwrap();
    doc.clear_history();
    doc
}

#[test]
fn dark_borders_are_found_and_whitened() {
    let mut doc = scans();
    let mut px = doc.image_pixels(0, 0).unwrap();
    let (w, h) = px.dimensions();
    let wide = w / 30;
    for y in 0..h {
        for x in 0..wide {
            px.put_pixel(x, y, Rgba([12, 12, 12, 255]));
        }
    }
    let colors = doc.page_images(0).unwrap()[0].colors;
    doc.replace_page_image(0, 0, &px, [1.0, 0.0, 0.0, 1.0, 0.0, 0.0], colors)
        .unwrap();
    let look = doc.look_at_scan(0).unwrap().unwrap();
    assert!((look.borders[0] - 1.0 / 30.0).abs() < 0.01, "{look:?}");
    let fixes = scan_fixes(&[look], &ScanThresholds::default());
    assert!(
        matches!(fixes[..], [ScanFix::Whiten { page: 0, .. }]),
        "{fixes:?}"
    );
    assert_eq!(doc.clean_up_scans(&fixes).unwrap(), (0, 0, 1));
    assert_eq!(doc.look_at_scan(0).unwrap().unwrap().borders, [0.0; 4]);
}

#[test]
fn blank_and_askew_scans_are_found() {
    let doc = scans();
    let looks: Vec<_> = (0..4).map(|p| doc.look_at_scan(p).unwrap()).collect();
    let [Some(letter), Some(blank), Some(askew), None] = &looks[..] else {
        panic!("{looks:?}");
    };
    let t = ScanThresholds::default();
    assert!(letter.ink > 0.01 && letter.skew.abs() < t.min_skew, "{letter:?}");
    assert!(blank.blank(&t), "{blank:?}");
    assert!(askew.askew(&t) && (askew.skew - 2.0).abs() < 0.3, "{askew:?}");
    let askew_skew = askew.skew;
    let looks: Vec<_> = looks.iter().flatten().cloned().collect();
    assert_eq!(
        scan_fixes(&looks, &t),
        [
            ScanFix::Delete { page: 1 },
            ScanFix::Straighten {
                page: 2,
                picture: 0,
                angle: askew_skew
            }
        ]
    );
}

#[test]
fn a_clean_up_straightens_and_deletes_in_one_step() {
    let mut doc = scans();
    let looks: Vec<_> = (0..4).filter_map(|p| doc.look_at_scan(p).unwrap()).collect();
    let fixes = scan_fixes(&looks, &ScanThresholds::default());
    assert_eq!(doc.clean_up_scans(&fixes).unwrap(), (1, 1, 0));
    assert_eq!(doc.page_count(), 3);
    // The askew letter, now page 2, is straight; the page that is not a scan stays.
    let straight = doc.image_pixels(1, 0).unwrap();
    assert!(skew_angle(&straight).abs() < 0.3, "{}", skew_angle(&straight));
    assert!(doc.page_text(2).contains("Line 2 of the letter"));
    doc.undo().unwrap();
    assert_eq!(doc.page_count(), 4);
    assert!((skew_angle(&doc.image_pixels(2, 0).unwrap()) - 2.0).abs() < 0.3);
}

#[test]
fn every_page_cannot_be_deleted() {
    let mut doc = Document::new_blank(2, 300.0, 400.0).unwrap();
    let fixes = [ScanFix::Delete { page: 0 }, ScanFix::Delete { page: 1 }];
    assert!(doc.clean_up_scans(&fixes).is_err());
    assert_eq!(doc.page_count(), 2);
    assert!(!doc.can_undo());
    assert!(doc.clean_up_scans(&[ScanFix::Delete { page: 5 }]).is_err());
}
