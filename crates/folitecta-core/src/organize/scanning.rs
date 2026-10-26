//! Documents from a scanner (FOL-L011): one page per scanned picture, as
//! large as the paper was (its pixels at the resolution of the scan), and
//! the backs of the sheets scanned as a second stack put back between the
//! fronts.

use crate::document::Document;
use crate::error::{Error, Result};

/// A picture from the scanner and its resolution (dots per inch; 0 when
/// unknown: the one written in the picture, else 300).
#[derive(Debug, Clone, PartialEq)]
pub struct ScannedPicture {
    pub data: Vec<u8>,
    pub dpi: f32,
}

/// The resolution written in a PNG (`pHYs` in metres), JPEG (JFIF density)
/// or BMP (pixels per metre) picture.
pub fn picture_dpi(data: &[u8]) -> Option<f32> {
    let per_metre = |v: u32| v as f32 * 0.0254;
    if data.starts_with(b"\x89PNG\r\n\x1a\n") {
        let mut at = 8;
        while at + 8 <= data.len() {
            let len = u32::from_be_bytes(data[at..at + 4].try_into().ok()?) as usize;
            let kind = &data[at + 4..at + 8];
            let body = data.get(at + 8..at + 8 + len)?;
            if kind == b"pHYs" && len >= 9 && body[8] == 1 {
                let x = u32::from_be_bytes(body[0..4].try_into().ok()?);
                return Some(per_metre(x)).filter(|d| *d >= 10.0);
            }
            if kind == b"IDAT" {
                return None;
            }
            at += 12 + len;
        }
        return None;
    }
    if data.starts_with(&[0xFF, 0xD8]) {
        // APP0 "JFIF\0": version (2), units (1), x density (2), y density (2).
        let at = data.windows(5).take(64).position(|w| w == b"JFIF\0")?;
        let d = data.get(at + 5..at + 12)?;
        let x = f32::from(u16::from_be_bytes([d[3], d[4]]));
        return match d[2] {
            1 => Some(x),
            2 => Some(x * 2.54),
            _ => None,
        }
        .filter(|d| *d >= 10.0);
    }
    if data.starts_with(b"BM") {
        let x = u32::from_le_bytes(data.get(38..42)?.try_into().ok()?);
        return Some(per_metre(x)).filter(|d| *d >= 10.0);
    }
    None
}

/// The order of the pages when `fronts` front sides are followed by the
/// `backs` back sides of the same sheets: front 1, back 1, front 2, ...
/// With `reversed` the backs were scanned last sheet first (the stack
/// turned over). There are as many backs as fronts, or one less (the last
/// sheet printed on one side).
pub fn duplex_order(fronts: usize, backs: usize, reversed: bool) -> Result<Vec<usize>> {
    if fronts == 0 || !(backs == fronts || backs + 1 == fronts) {
        return Err(Error::Invalid(format!(
            "{fronts} front sides and {backs} back sides do not make sheets"
        )));
    }
    let mut order = Vec::with_capacity(fronts + backs);
    for i in 0..fronts {
        order.push(i);
        if i < backs {
            let back = if reversed {
                // The last sheet's back came first, the missing one at the start.
                fronts + (fronts - 1 - i) - (fronts - backs)
            } else {
                fronts + i
            };
            order.push(back);
        }
    }
    Ok(order)
}

impl Document {
    /// A new document with one page per scanned picture, each page the size
    /// of the paper scanned.
    pub fn from_scans(pictures: Vec<ScannedPicture>) -> Result<Document> {
        if pictures.is_empty() {
            return Err(Error::Invalid("nothing was scanned".into()));
        }
        let mut doc = Document::new_blank(1, 595.0, 842.0)?;
        doc.append_scans(pictures)?;
        doc.delete_pages(&[0])?;
        doc.clear_history();
        Ok(doc)
    }

    /// Add scanned pictures as pages at the end (one undo step).
    pub fn append_scans(&mut self, pictures: Vec<ScannedPicture>) -> Result<usize> {
        let mut sized = Vec::with_capacity(pictures.len());
        for p in pictures {
            let dpi = if p.dpi >= 10.0 {
                p.dpi
            } else {
                picture_dpi(&p.data).unwrap_or(300.0)
            };
            let (w, h) = Document::image_size(&p.data)?;
            // Points at the resolution of the scan, kept within PDF's page limits.
            let size = |px: u32| (px as f32 * 72.0 / dpi).clamp(3.0, 14400.0);
            sized.push((p.data, size(w), size(h)));
        }
        self.insert_sized_image_pages(self.page_count(), sized)
    }

    /// Put the back sides scanned after the `fronts` front sides between
    /// them (see [`duplex_order`]); one undo step.
    pub fn interleave_backs(&mut self, fronts: usize, reversed: bool) -> Result<()> {
        let n = self.page_count();
        let order = duplex_order(fronts, n.saturating_sub(fronts), reversed)?;
        self.reorder_pages(&order)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backs_go_between_the_fronts() {
        assert_eq!(duplex_order(3, 3, false).unwrap(), vec![0, 3, 1, 4, 2, 5]);
        // Backs scanned with the stack turned over: the last sheet's back first.
        assert_eq!(duplex_order(3, 3, true).unwrap(), vec![0, 5, 1, 4, 2, 3]);
        // The last sheet has no back.
        assert_eq!(duplex_order(3, 2, false).unwrap(), vec![0, 3, 1, 4, 2]);
        assert_eq!(duplex_order(3, 2, true).unwrap(), vec![0, 4, 1, 3, 2]);
        assert_eq!(duplex_order(1, 0, true).unwrap(), vec![0]);
        assert!(duplex_order(3, 5, false).is_err());
        assert!(duplex_order(0, 0, false).is_err());
    }
}
