//! Clean-up of scanned pages (FOL-L012): pages that are a picture of paper
//! are looked at for what a scanner leaves behind — blank sides and sheets
//! fed askew — and cleaned in one step, the askew pictures straightened and
//! the blank pages taken out.

use image::{RgbaImage, imageops};

use crate::document::Document;
use crate::error::{Error, Result};
use crate::imaging::{Edit, Work, skew_angle};

/// Share of the page a picture must cover for the page to be a scan.
const SCAN_COVER: f32 = 0.85;

/// Share of each side left out when ink is counted: the shadows and holes a
/// scanner sees at the edges of the sheet.
const EDGE: f32 = 0.04;

/// Pixels darker than this (0 to 255) are ink. Show-through of the other side
/// and the grey of recycled paper are lighter.
const INK: u8 = 128;

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
}

/// When a scanned page counts as blank and when as askew.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScanThresholds {
    /// Blank below this share of ink (0 to 1).
    pub blank_ink: f32,
    /// Askew from this many degrees.
    pub min_skew: f32,
}

impl Default for ScanThresholds {
    fn default() -> Self {
        ScanThresholds {
            blank_ink: 0.002,
            min_skew: 0.3,
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
}

/// One change of a clean-up.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ScanFix {
    /// Turn the picture of the page back by the angle its lines are turned.
    Straighten { page: usize, picture: usize, angle: f32 },
    /// Take the page out.
    Delete { page: usize },
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
    looks
        .iter()
        .filter_map(|l| {
            if l.blank(thresholds) {
                Some(ScanFix::Delete { page: l.page })
            } else if l.askew(thresholds) {
                Some(ScanFix::Straighten {
                    page: l.page,
                    picture: l.picture,
                    angle: l.skew,
                })
            } else {
                None
            }
        })
        .collect()
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
        Ok(Some(ScanLook {
            page,
            picture,
            ink: ink_share(&px),
            skew: skew_angle(&px),
        }))
    }

    /// Make `fixes` in one undo step: the pictures straightened (the corners
    /// they uncover white), then the pages deleted. Returns how many pages
    /// were straightened and how many deleted. Every page cannot be deleted.
    pub fn clean_up_scans(&mut self, fixes: &[ScanFix]) -> Result<(usize, usize)> {
        let n = self.page_count();
        let mut delete: Vec<usize> = Vec::new();
        for f in fixes {
            let page = match *f {
                ScanFix::Straighten { page, .. } | ScanFix::Delete { page } => page,
            };
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
            let mut straightened = 0;
            for f in fixes {
                let ScanFix::Straighten { page, picture, angle } = *f else {
                    continue;
                };
                if delete.contains(&page) || angle == 0.0 {
                    continue;
                }
                let images = doc.page_images(page)?;
                let im = images
                    .get(picture)
                    .ok_or_else(|| Error::Invalid("the picture is not on the page".into()))?;
                let (matrix, colors) = (im.matrix, im.colors);
                let mut work = Work::new(doc.image_pixels(page, picture)?, matrix, colors);
                work.apply(Edit::Turn(-angle));
                doc.put_page_image(page, picture, &work.pixels, work.placement, work.colors)?;
                straightened += 1;
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
            Ok((straightened, delete.len()))
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
    fn blank_pages_are_deleted_and_askew_ones_straightened() {
        let t = ScanThresholds::default();
        let looks = [
            ScanLook {
                page: 0,
                picture: 0,
                ink: 0.06,
                skew: 0.1,
            },
            ScanLook {
                page: 1,
                picture: 0,
                ink: 0.0004,
                skew: 2.0,
            },
            ScanLook {
                page: 2,
                picture: 1,
                ink: 0.04,
                skew: -1.5,
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
        };
        assert!(scan_fixes(&looks, &strict).is_empty());
    }
}
