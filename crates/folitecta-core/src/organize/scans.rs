//! Clean-up of scanned pages (FOL-L012): pages that are a picture of paper
//! are looked at for what a scanner leaves behind — blank sides, sheets fed
//! askew, upside down or sideways, and the dark edges of its lid — and
//! cleaned in one step: the borders whitened, the askew pictures
//! straightened, the pages turned the right way up and the blank pages
//! taken out.

use image::{RgbaImage, imageops};

use crate::document::Document;
use crate::error::{Error, Result};
use crate::imaging::{Edit, Format, Work, skew_angle, turn as turned_by, upright_turn};

/// Share of the page a picture must cover for the page to be a scan.
const SCAN_COVER: f32 = 0.85;

/// Share of each side left out when ink is counted: the shadows and holes a
/// scanner sees at the edges of the sheet.
const EDGE: f32 = 0.04;

/// Pixels darker than this (0 to 255) are ink. Show-through of the other side
/// and the grey of recycled paper are lighter.
const INK: u8 = 128;

/// A row or column at the edge is part of a dark border when at least this
/// share of it is dark: a line of text across the page is not.
const BORDER_DARK: f32 = 0.8;

/// A border reaches at most this far in: past it, the page is a dark picture.
const BORDER_MOST: f32 = 0.15;

/// What a scanned page looks like.
#[derive(Debug, Clone, PartialEq)]
pub struct ScanLook {
    pub page: usize,
    /// The picture that is the page (its place among the pictures of the page).
    pub picture: usize,
    /// Share of the page that is ink, away from the edges (0 to 1).
    pub ink: f32,
    /// Degrees the lines of the page are turned counterclockwise (0 when
    /// none stand out).
    pub skew: f32,
    /// How far dark borders reach in from the left, top, right and bottom,
    /// as shares of the width and height of the picture.
    pub borders: [f32; 4],
    /// Degrees clockwise the page must turn for its text to read upright:
    /// 0, 90, 180 or 270 (0 also when it cannot be told).
    pub turn: i32,
}

/// When a scanned page counts as blank and when as askew.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScanThresholds {
    /// Blank below this share of ink (0 to 1).
    pub blank_ink: f32,
    /// Askew from this many degrees.
    pub min_skew: f32,
    /// Dark borders count from this share of the side.
    pub min_border: f32,
    /// Remove the specks of every scanned page that is kept.
    pub despeckle: bool,
    /// Make every scanned page that is kept black and white.
    pub black_white: bool,
}

impl Default for ScanThresholds {
    fn default() -> Self {
        ScanThresholds {
            blank_ink: 0.002,
            min_skew: 0.3,
            min_border: 0.003,
            despeckle: false,
            black_white: false,
        }
    }
}

impl ScanLook {
    pub fn blank(&self, t: &ScanThresholds) -> bool {
        self.ink < t.blank_ink
    }

    /// Askew, and not blank (a blank page is taken out, not straightened).
    pub fn askew(&self, t: &ScanThresholds) -> bool {
        !self.blank(t) && self.skew.abs() >= t.min_skew
    }

    /// Upside down or sideways, and not blank.
    pub fn turned(&self, t: &ScanThresholds) -> bool {
        !self.blank(t) && self.turn != 0
    }

    /// The borders that count, and not on a blank page.
    pub fn dark_borders(&self, t: &ScanThresholds) -> Option<[f32; 4]> {
        (!self.blank(t) && self.borders.iter().any(|b| *b >= t.min_border))
            .then(|| self.borders.map(|b| if b >= t.min_border { b } else { 0.0 }))
    }
}

/// One change of a clean-up.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ScanFix {
    /// Turn the picture of the page back by the angle its lines are turned.
    Straighten { page: usize, picture: usize, angle: f32 },
    /// Take the page out.
    Delete { page: usize },
    /// Whiten the dark borders of the picture (see [`ScanLook::borders`]).
    Whiten {
        page: usize,
        picture: usize,
        borders: [f32; 4],
    },
    /// Turn the page clockwise by `degrees` (a quarter turn or more), so its
    /// text reads upright; the picture is left as it is.
    Turn { page: usize, degrees: i32 },
    /// Remove the isolated specks of the picture.
    Despeckle { page: usize, picture: usize },
    /// Make the picture black and white (one bit a pixel).
    BlackWhite { page: usize, picture: usize },
}

impl ScanFix {
    pub fn page(&self) -> usize {
        match *self {
            ScanFix::Straighten { page, .. }
            | ScanFix::Delete { page }
            | ScanFix::Whiten { page, .. }
            | ScanFix::Turn { page, .. }
            | ScanFix::Despeckle { page, .. }
            | ScanFix::BlackWhite { page, .. } => page,
        }
    }
}

/// How far dark borders reach in from each side of `px` (left, top, right,
/// bottom), as shares of its width and height.
pub fn dark_borders(px: &RgbaImage) -> [f32; 4] {
    let (w, h) = px.dimensions();
    if w == 0 || h == 0 {
        return [0.0; 4];
    }
    let dark = |x: u32, y: u32| {
        let p = px.get_pixel(x, y);
        p[3] > 127
            && (u32::from(p[0]) * 299 + u32::from(p[1]) * 587 + u32::from(p[2]) * 114) / 1000 < u32::from(INK)
    };
    // Every eighth pixel of a line tells whether it is dark.
    let column = |x: u32| (0..h).step_by(8).filter(|y| dark(x, *y)).count() as f32 / h.div_ceil(8) as f32;
    let row = |y: u32| (0..w).step_by(8).filter(|x| dark(*x, y)).count() as f32 / w.div_ceil(8) as f32;
    let reach = |side: u32, share: &dyn Fn(u32) -> f32| {
        let most = (side as f32 * BORDER_MOST).ceil() as u32;
        let n = (0..most).take_while(|i| share(*i) >= BORDER_DARK).count() as u32;
        if n >= most { 0.0 } else { n as f32 / side as f32 }
    };
    [
        reach(w, &column),
        reach(h, &row),
        reach(w, &|i| column(w - 1 - i)),
        reach(h, &|i| row(h - 1 - i)),
    ]
}

/// Paint the borders of `px` the colour of its paper.
fn whiten(px: &mut RgbaImage, borders: [f32; 4]) {
    let (w, h) = px.dimensions();
    let inner = |share: f32, side: u32| ((share * side as f32).ceil() as u32 + 2).min(side);
    let (l, t, r, b) = (
        if borders[0] > 0.0 { inner(borders[0], w) } else { 0 },
        if borders[1] > 0.0 { inner(borders[1], h) } else { 0 },
        if borders[2] > 0.0 { inner(borders[2], w) } else { 0 },
        if borders[3] > 0.0 { inner(borders[3], h) } else { 0 },
    );
    let paper = *px.get_pixel(w / 2, (h / 2).min(h.saturating_sub(1)));
    let paper = if paper[0] > 200 && paper[1] > 200 && paper[2] > 200 {
        paper
    } else {
        image::Rgba([255, 255, 255, 255])
    };
    for y in 0..h {
        for x in 0..w {
            if x < l || y < t || x >= w - r || y >= h - b {
                px.put_pixel(x, y, paper);
            }
        }
    }
}

/// Share of `px` that is ink, away from the edges, measured on a copy at most
/// 1000 pixels on its longer side: specks smaller than a few pixels of the
/// scan become grey there and do not count.
pub fn ink_share(px: &RgbaImage) -> f32 {
    let (w, h) = px.dimensions();
    if w == 0 || h == 0 {
        return 0.0;
    }
    let scale = (1000.0 / w.max(h) as f32).min(1.0);
    let small = if scale < 1.0 {
        imageops::resize(
            px,
            ((w as f32 * scale).round() as u32).max(1),
            ((h as f32 * scale).round() as u32).max(1),
            imageops::FilterType::Triangle,
        )
    } else {
        px.clone()
    };
    let (sw, sh) = small.dimensions();
    let (mx, my) = (
        (sw as f32 * EDGE).round() as u32,
        (sh as f32 * EDGE).round() as u32,
    );
    let (x0, x1, y0, y1) = (mx, sw.saturating_sub(mx), my, sh.saturating_sub(my));
    if x1 <= x0 || y1 <= y0 {
        return 0.0;
    }
    let mut dark = 0u64;
    for y in y0..y1 {
        for x in x0..x1 {
            let p = small.get_pixel(x, y);
            // Transparent pixels show the white of the page.
            let l = (u32::from(p[0]) * 299 + u32::from(p[1]) * 587 + u32::from(p[2]) * 114) / 1000;
            if p[3] > 127 && l < u32::from(INK) {
                dark += 1;
            }
        }
    }
    dark as f32 / ((x1 - x0) as u64 * (y1 - y0) as u64) as f32
}

/// The changes `thresholds` call for on the pages looked at.
pub fn scan_fixes(looks: &[ScanLook], thresholds: &ScanThresholds) -> Vec<ScanFix> {
    let mut fixes = Vec::new();
    for l in looks {
        if l.blank(thresholds) {
            fixes.push(ScanFix::Delete { page: l.page });
            continue;
        }
        if let Some(borders) = l.dark_borders(thresholds) {
            fixes.push(ScanFix::Whiten {
                page: l.page,
                picture: l.picture,
                borders,
            });
        }
        if l.askew(thresholds) {
            fixes.push(ScanFix::Straighten {
                page: l.page,
                picture: l.picture,
                angle: l.skew,
            });
        }
        if l.turned(thresholds) {
            fixes.push(ScanFix::Turn {
                page: l.page,
                degrees: l.turn,
            });
        }
        if thresholds.despeckle {
            fixes.push(ScanFix::Despeckle {
                page: l.page,
                picture: l.picture,
            });
        }
        // Last: the other changes are made on the colours.
        if thresholds.black_white {
            fixes.push(ScanFix::BlackWhite {
                page: l.page,
                picture: l.picture,
            });
        }
    }
    fixes
}

/// How a scanned picture must turn to read upright (`upright_turn`) and
/// how askew its lines are. Lines running down are measured across, on the
/// picture turned a quarter (a turn keeps the angle); when the way up
/// cannot be told, it is told again on a copy straightened by the skew,
/// as it lies and turned a quarter.
fn way_and_skew(px: &image::RgbaImage) -> (Option<i32>, f32) {
    let white = image::Rgba([255, 255, 255, 255]);
    let straight = |p: &image::RgbaImage, a: f32| {
        if a.abs() >= 0.05 {
            turned_by(p, -a, white)
        } else {
            p.clone()
        }
    };
    match upright_turn(px) {
        Some(t @ (90 | 270)) => (Some(t), skew_angle(&image::imageops::rotate90(px))),
        Some(t) => (Some(t), skew_angle(px)),
        None => {
            let s = skew_angle(px);
            if let Some(t @ (0 | 180)) = upright_turn(&straight(px, s)) {
                return (Some(t), s);
            }
            let quarter = image::imageops::rotate90(px);
            let s2 = skew_angle(&quarter);
            match upright_turn(&straight(&quarter, s2)) {
                // The quarter turned copy reads upright after `t` more.
                Some(t @ (0 | 180)) => (Some((t + 90) % 360), s2),
                _ => (None, s),
            }
        }
    }
}

/// Degrees clockwise a picture placed with `matrix` shows its pixels turned,
/// to the nearest quarter turn.
fn placed_turn(matrix: &[f64; 6]) -> i32 {
    // Where the first row of pixels runs, on the page as seen (y down).
    let deg = (-matrix[1]).atan2(matrix[0]).to_degrees();
    ((deg / 90.0).round() as i32 * 90).rem_euclid(360)
}

/// What a clean-up of scans changed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ScansCleaned {
    pub straightened: usize,
    pub deleted: usize,
    pub whitened: usize,
    pub turned: usize,
    pub despeckled: usize,
    pub black_white: usize,
}

impl Document {
    /// The picture that is page `page` when the page is a scan: one picture
    /// covering most of the page.
    pub fn scan_picture(&self, page: usize) -> Option<usize> {
        let size = self.page_size_of(self.page_id(page).ok()?);
        let page_area = size.width * size.height;
        if page_area <= 0.0 {
            return None;
        }
        let images = self.page_images(page).ok()?;
        let area = |r: [f32; 4]| (r[2] - r[0]).abs() * (r[3] - r[1]).abs();
        images
            .iter()
            .filter(|im| im.width >= 16 && im.height >= 16)
            .max_by(|a, b| area(a.rect).total_cmp(&area(b.rect)))
            .filter(|im| area(im.rect) >= SCAN_COVER * page_area)
            .map(|im| im.index)
    }

    /// How page `page` looks when it is a scan (None when it is not one).
    pub fn look_at_scan(&self, page: usize) -> Result<Option<ScanLook>> {
        let Some(picture) = self.scan_picture(page) else {
            return Ok(None);
        };
        let px = self.image_pixels(page, picture)?;
        // The pixels show turned by their placement and by the page.
        let shown = self
            .page_images(page)?
            .get(picture)
            .map(|im| placed_turn(&im.matrix))
            .unwrap_or(0)
            + self.page_size_of(self.page_id(page)?).rotation;
        let (lines, skew) = way_and_skew(&px);
        let turn = lines.map(|t| (t - shown).rem_euclid(360)).unwrap_or(0);
        Ok(Some(ScanLook {
            page,
            picture,
            ink: ink_share(&px),
            skew,
            borders: dark_borders(&px),
            turn,
        }))
    }

    /// Make `fixes` in one undo step, in their order: borders whitened,
    /// pictures straightened (the corners they uncover white), pages turned,
    /// then the pages deleted. Every page cannot be deleted.
    pub fn clean_up_scans(&mut self, fixes: &[ScanFix]) -> Result<ScansCleaned> {
        let n = self.page_count();
        let mut delete: Vec<usize> = Vec::new();
        for f in fixes {
            let page = f.page();
            if page >= n {
                return Err(Error::InvalidPage(page));
            }
            if let ScanFix::Delete { page } = *f {
                if !delete.contains(&page) {
                    delete.push(page);
                }
            }
        }
        if !delete.is_empty() && delete.len() >= n {
            return Err(Error::LastPage);
        }
        self.edit_step(|doc| {
            let mut done = ScansCleaned::default();
            for f in fixes {
                if let ScanFix::Turn { page, degrees } = *f {
                    if !delete.contains(&page) && degrees.rem_euclid(360) != 0 {
                        doc.turn_page(page, degrees)?;
                        done.turned += 1;
                    }
                    continue;
                }
                let (page, picture) = match *f {
                    ScanFix::Straighten { page, picture, angle } if angle != 0.0 => (page, picture),
                    ScanFix::Whiten { page, picture, .. }
                    | ScanFix::Despeckle { page, picture }
                    | ScanFix::BlackWhite { page, picture } => (page, picture),
                    _ => continue,
                };
                if delete.contains(&page) {
                    continue;
                }
                let images = doc.page_images(page)?;
                let im = images
                    .get(picture)
                    .ok_or_else(|| Error::Invalid("the picture is not on the page".into()))?;
                let (matrix, colors) = (im.matrix, im.colors);
                let mut work = Work::new(doc.image_pixels(page, picture)?, matrix, colors);
                match *f {
                    ScanFix::Straighten { angle, .. } => {
                        work.apply(Edit::Turn(-angle));
                        done.straightened += 1;
                    }
                    ScanFix::Whiten { borders, .. } => {
                        whiten(&mut work.pixels, borders);
                        done.whitened += 1;
                    }
                    ScanFix::Despeckle { .. } => {
                        work.apply(Edit::Despeckle);
                        done.despeckled += 1;
                    }
                    ScanFix::BlackWhite { .. } => {
                        work.apply(Edit::Convert(Format::Bilevel));
                        done.black_white += 1;
                    }
                    ScanFix::Delete { .. } | ScanFix::Turn { .. } => {}
                }
                doc.put_page_image(page, picture, &work.pixels, work.placement, work.colors)?;
            }
            if !delete.is_empty() {
                let keep = doc
                    .page_ids()
                    .into_iter()
                    .enumerate()
                    .filter(|(i, _)| !delete.contains(i))
                    .map(|(_, id)| id)
                    .collect();
                doc.set_page_order(keep)?;
            }
            done.deleted = delete.len();
            Ok(done)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    fn sheet(w: u32, h: u32) -> RgbaImage {
        RgbaImage::from_pixel(w, h, Rgba([250, 250, 248, 255]))
    }

    #[test]
    fn ink_leaves_out_edges_specks_and_show_through() {
        let mut px = sheet(2000, 2800);
        // A dark scanner border, specks one pixel wide, grey show-through.
        for y in 0..2800 {
            for x in 0..40 {
                px.put_pixel(x, y, Rgba([20, 20, 20, 255]));
            }
        }
        for i in 0..400 {
            px.put_pixel(300 + i * 3, 600 + i * 4, Rgba([0, 0, 0, 255]));
        }
        for y in 1000..1400 {
            for x in 300..1700 {
                px.put_pixel(x, y, Rgba([200, 200, 200, 255]));
            }
        }
        assert!(ink_share(&px) < 0.0005, "{}", ink_share(&px));
        // Lines of text.
        for line in 0..30 {
            for y in 400 + line * 60..400 + line * 60 + 14 {
                for x in 300..1700 {
                    px.put_pixel(x, y, Rgba([30, 30, 30, 255]));
                }
            }
        }
        assert!(ink_share(&px) > 0.05, "{}", ink_share(&px));
    }

    #[test]
    fn dark_borders_are_told_from_lines_of_text() {
        let mut px = sheet(800, 1000);
        assert_eq!(dark_borders(&px), [0.0; 4]);
        for y in 0..1000 {
            for x in 0..24 {
                px.put_pixel(x, y, Rgba([15, 15, 15, 255]));
            }
        }
        for y in 960..1000 {
            for x in 0..800 {
                px.put_pixel(x, y, Rgba([25, 25, 25, 255]));
            }
        }
        // A line of text that does not reach the whole width, near the top.
        for y in 2..14 {
            for x in 60..700 {
                if x % 11 < 6 {
                    px.put_pixel(x, y, Rgba([0, 0, 0, 255]));
                }
            }
        }
        let b = dark_borders(&px);
        assert!((b[0] - 0.03).abs() < 0.002, "{b:?}");
        assert_eq!(b[1], 0.0, "{b:?}");
        assert_eq!(b[2], 0.0, "{b:?}");
        assert!((b[3] - 0.04).abs() < 0.002, "{b:?}");
        whiten(&mut px, b);
        assert_eq!(dark_borders(&px), [0.0; 4]);
        assert_eq!(px.get_pixel(5, 500), &Rgba([250, 250, 248, 255]));
        assert_eq!(px.get_pixel(100, 5), &Rgba([0, 0, 0, 255]), "the text stays");

        // A page that is dark all over has no border.
        let night = RgbaImage::from_pixel(400, 400, Rgba([10, 10, 10, 255]));
        assert_eq!(dark_borders(&night), [0.0; 4]);
    }

    #[test]
    fn blank_pages_are_deleted_and_askew_ones_straightened() {
        let t = ScanThresholds::default();
        let looks = [
            ScanLook {
                page: 0,
                picture: 0,
                ink: 0.06,
                skew: 0.1,
                borders: [0.0; 4],
                turn: 0,
            },
            ScanLook {
                page: 1,
                picture: 0,
                ink: 0.0004,
                skew: 2.0,
                borders: [0.03, 0.0, 0.0, 0.0],
                turn: 0,
            },
            ScanLook {
                page: 2,
                picture: 1,
                ink: 0.04,
                skew: -1.5,
                borders: [0.0; 4],
                turn: 0,
            },
        ];
        assert_eq!(
            scan_fixes(&looks, &t),
            [
                ScanFix::Delete { page: 1 },
                ScanFix::Straighten {
                    page: 2,
                    picture: 1,
                    angle: -1.5
                }
            ]
        );
        let strict = ScanThresholds {
            blank_ink: 0.0001,
            min_skew: 3.0,
            min_border: 0.5,
            ..ScanThresholds::default()
        };
        assert!(scan_fixes(&looks, &strict).is_empty());
    }

    #[test]
    fn pages_upside_down_or_sideways_are_turned_unless_blank() {
        let t = ScanThresholds::default();
        let look = |page, ink, turn| ScanLook {
            page,
            picture: 0,
            ink,
            skew: 0.0,
            borders: [0.0; 4],
            turn,
        };
        let looks = [
            look(0, 0.05, 180),
            look(1, 0.05, 0),
            look(2, 0.05, 90),
            look(3, 0.0001, 270),
        ];
        assert_eq!(
            scan_fixes(&looks, &t),
            [
                ScanFix::Turn {
                    page: 0,
                    degrees: 180
                },
                ScanFix::Turn { page: 2, degrees: 90 },
                ScanFix::Delete { page: 3 },
            ]
        );
    }

    #[test]
    fn placements_turn_the_pixels_they_show() {
        assert_eq!(placed_turn(&[600.0, 0.0, 0.0, 800.0, 0.0, 0.0]), 0);
        // The first row of pixels runs down the page: a quarter clockwise.
        assert_eq!(placed_turn(&[0.0, -600.0, 800.0, 0.0, 0.0, 600.0]), 90);
        assert_eq!(placed_turn(&[-600.0, 0.0, 0.0, -800.0, 600.0, 800.0]), 180);
        assert_eq!(placed_turn(&[0.0, 600.0, -800.0, 0.0, 800.0, 0.0]), 270);
    }
}
