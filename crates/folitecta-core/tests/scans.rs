//! Clean-up of scanned pages: a blank sheet is found and taken out, a sheet
//! fed askew is found and straightened, sheets upside down or sideways are
//! turned upright, pages that are not scans are left alone; the clean-up is
//! one undo step.

use folitecta_core::imaging::{skew_angle, turn};
use folitecta_core::{Document, ScanFix, ScanThresholds, ScansCleaned, TextLayout, scan_fixes};
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
    assert_eq!(
        doc.clean_up_scans(&fixes).unwrap(),
        ScansCleaned {
            whitened: 1,
            ..ScansCleaned::default()
        }
    );
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
    assert_eq!(
        doc.clean_up_scans(&fixes).unwrap(),
        ScansCleaned {
            straightened: 1,
            deleted: 1,
            ..ScansCleaned::default()
        }
    );
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

/// Lines of a letter that differ from each other, as in a real page (the
/// same sentence on every line would put its words in columns).
fn prose(lines: usize) -> String {
    const WORDS: [&str; 24] = [
        "the", "quick", "brown", "fox", "jumps", "over", "lazy", "dogs", "while", "a", "kind", "old", "lady",
        "reads", "her", "daily", "paper", "by", "the", "bright", "window", "of", "its", "shop",
    ];
    (0..lines)
        .map(|i| {
            let n = 9 + (i * 5) % 6;
            (0..n)
                .map(|k| WORDS[(i * 7 + k * 5) % WORDS.len()])
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn pages_upside_down_and_sideways_are_turned_upright() {
    let source = [prose(40), prose(40), prose(40), prose(40)].join("\u{c}");
    let mut doc = Document::from_text(&source, TextLayout::default()).unwrap();
    doc.rasterize_pages(&[0, 1, 2, 3], 100.0, false).unwrap();
    // Page 1 upside down, page 2 turned a quarter clockwise, page 3 a
    // quarter counterclockwise; page 0 upright.
    for (page, turned) in [
        (
            1,
            image::imageops::rotate180 as fn(&image::RgbaImage) -> image::RgbaImage,
        ),
        (2, image::imageops::rotate90),
        (3, image::imageops::rotate270),
    ] {
        let px = turned(&doc.image_pixels(page, 0).unwrap());
        let colors = doc.page_images(page).unwrap()[0].colors;
        doc.replace_page_image(page, 0, &px, [1.0, 0.0, 0.0, 1.0, 0.0, 0.0], colors)
            .unwrap();
    }
    doc.clear_history();
    let looks: Vec<_> = (0..4).map(|p| doc.look_at_scan(p).unwrap().unwrap()).collect();
    let turns: Vec<i32> = looks.iter().map(|l| l.turn).collect();
    assert_eq!(turns, [0, 180, 270, 90]);
    let fixes: Vec<ScanFix> = scan_fixes(&looks, &ScanThresholds::default())
        .into_iter()
        .filter(|f| matches!(f, ScanFix::Turn { .. }))
        .collect();
    assert_eq!(doc.clean_up_scans(&fixes).unwrap().turned, 3);
    let rotations: Vec<i32> = doc.page_sizes().iter().map(|s| s.rotation).collect();
    assert_eq!(rotations, [0, 180, 270, 90]);
    // Turned, they read upright: nothing more to turn.
    let again: Vec<i32> = (0..4)
        .map(|p| doc.look_at_scan(p).unwrap().unwrap().turn)
        .collect();
    assert_eq!(again, [0; 4]);
    doc.undo().unwrap();
    assert!(doc.page_sizes().iter().all(|s| s.rotation == 0));
}

#[test]
fn pictures_without_text_are_not_turned() {
    let mut doc = Document::new_blank(1, 400.0, 500.0).unwrap();
    doc.insert_image_pages(
        0,
        vec![{
            let px = image::RgbaImage::from_fn(400, 500, |x, y| {
                let v = ((x * 7 + y * 3) % 255) as u8;
                image::Rgba([v, 255 - v, 128, 255])
            });
            let mut out = std::io::Cursor::new(Vec::new());
            px.write_to(&mut out, image::ImageFormat::Png).unwrap();
            out.into_inner()
        }],
    )
    .unwrap();
    if let Some(look) = doc.look_at_scan(0).unwrap() {
        assert_eq!(look.turn, 0);
    }
}

#[test]
fn specks_go_and_pages_become_black_and_white_when_asked() {
    let mut doc = scans();
    // Specks of one pixel on the paper of the letter.
    let mut px = doc.image_pixels(0, 0).unwrap();
    let (w, h) = px.dimensions();
    let paper = |px: &image::RgbaImage, x: u32, y: u32| {
        (x.saturating_sub(2)..(x + 3).min(w))
            .all(|i| (y.saturating_sub(2)..(y + 3).min(h)).all(|j| px.get_pixel(i, j)[0] > 200))
    };
    let mut specks = Vec::new();
    for i in 0..2000u32 {
        let (x, y) = ((i * 37) % w, (i * 53) % h);
        if specks.len() < 300 && paper(&px, x, y) {
            specks.push((x, y));
        }
    }
    for &(x, y) in &specks {
        px.put_pixel(x, y, Rgba([0, 0, 0, 255]));
    }
    let colors = doc.page_images(0).unwrap()[0].colors;
    doc.replace_page_image(0, 0, &px, [1.0, 0.0, 0.0, 1.0, 0.0, 0.0], colors)
        .unwrap();
    doc.clear_history();
    let t = ScanThresholds {
        despeckle: true,
        black_white: true,
        ..ScanThresholds::default()
    };
    let looks: Vec<_> = (0..4).filter_map(|p| doc.look_at_scan(p).unwrap()).collect();
    let fixes = scan_fixes(&looks, &t);
    // Not on the blank page, which goes; black and white last on each page.
    assert!(
        !fixes
            .iter()
            .any(|f| f.page() == 1 && !matches!(f, ScanFix::Delete { .. }))
    );
    assert!(matches!(fixes.last(), Some(ScanFix::BlackWhite { .. })));
    let done = doc.clean_up_scans(&fixes).unwrap();
    assert_eq!((done.despeckled, done.black_white), (2, 2));
    assert_eq!(done.deleted, 1);
    // The specks are gone; the page is one bit a pixel.
    let after = doc.image_pixels(0, 0).unwrap();
    let left = specks
        .iter()
        .filter(|&&(x, y)| after.get_pixel(x, y)[0] < 128)
        .count();
    assert!(
        specks.len() > 200 && left * 20 < specks.len(),
        "{left} of {} specks left",
        specks.len()
    );
    assert_eq!(
        doc.page_images(0).unwrap()[0].colors,
        folitecta_core::pictures::Colors::BlackWhite
    );
}

#[test]
fn a_sheet_both_sideways_and_askew_is_straightened_and_turned() {
    let mut doc = Document::from_text(&prose(40), TextLayout::default()).unwrap();
    doc.rasterize_pages(&[0], 100.0, false).unwrap();
    // Fed 2 degrees askew, and sideways.
    let px = doc.image_pixels(0, 0).unwrap();
    let askew = turn(&px, 2.0, Rgba([255, 255, 255, 255]));
    let sideways = image::imageops::rotate90(&askew);
    let colors = doc.page_images(0).unwrap()[0].colors;
    doc.replace_page_image(0, 0, &sideways, [1.0, 0.0, 0.0, 1.0, 0.0, 0.0], colors)
        .unwrap();
    doc.clear_history();
    let look = doc.look_at_scan(0).unwrap().unwrap();
    let t = ScanThresholds::default();
    assert_eq!(look.turn, 270, "{look:?}");
    assert!(look.askew(&t) && (look.skew.abs() - 2.0).abs() < 0.3, "{look:?}");
    let fixes = scan_fixes(&[look], &t);
    let done = doc.clean_up_scans(&fixes).unwrap();
    assert_eq!((done.straightened, done.turned), (1, 1), "{done:?}");
    // Its lines run straight down the picture now, straight across the page.
    let straight = image::imageops::rotate90(&doc.image_pixels(0, 0).unwrap());
    assert!(skew_angle(&straight).abs() < 0.3, "{}", skew_angle(&straight));
    assert_eq!(doc.look_at_scan(0).unwrap().unwrap().turn, 0);
}
