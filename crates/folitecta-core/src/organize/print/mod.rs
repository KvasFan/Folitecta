//! Printing: the sheets a print job puts on paper, and the print file made of
//! them.
//!
//! [`plan`] is pure geometry: which pages go on which side of which sheet of
//! paper, at which matrix (one page per sheet, several pages per sheet, a
//! booklet, a poster cut into tiles), with the order of two-sided output.
//! [`build`] writes the planned sheets as a vector PDF: every sheet is one
//! page of the size of the paper, always upright (portrait); the pages are
//! drawn on it as form XObjects with their printable annotations, and the
//! optional content is fixed to its print state. [`raster`] draws the sheets
//! as pictures (printing as an image, and the bands of the printers that
//! receive pixels). The document itself never changes.

pub mod paper;
pub mod plan;

pub use paper::{
    STANDARD_PAPERS, StandardPaper, closest_paper, default_paper_key, media_size, paper_for_key, same_size,
    standard_name,
};
pub use plan::{
    Face, MAX_SIDES, Mark, Placement, PlanError, PlanWarning, PrintPlan, Sheet, apply, mul, nup_grid,
    page_matrix, plan, select,
};

use serde::{Deserialize, Serialize};

use crate::document::PageSize;

/// The geometry of a source page that the plan needs.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PageGeom {
    /// The crop box intersected with the media box (PDF user space).
    pub crop: [f32; 4],
    /// The page rotation, snapped to 0, 90, 180 or 270 degrees (clockwise).
    pub rotation: i32,
    /// The size of a user space unit in points (`/UserUnit`, 1 by default).
    pub user_unit: f32,
}

/// Snap a `/Rotate` value to the nearest quarter turn (0, 90, 180, 270).
pub fn snap_rotation(r: i32) -> i32 {
    ((r + 45).div_euclid(90) * 90).rem_euclid(360)
}

impl PageGeom {
    /// The geometry of a page from its size and its user unit.
    pub fn from_size(s: &PageSize, user_unit: f32) -> Self {
        let u = if user_unit.is_finite() {
            user_unit.clamp(0.1, 100.0)
        } else {
            1.0
        };
        PageGeom {
            crop: s.crop_box,
            rotation: snap_rotation(s.rotation),
            user_unit: u,
        }
    }

    /// Width and height in points as the page is shown (user unit and
    /// rotation applied).
    pub fn display_size(&self) -> (f32, f32) {
        let u = self.user_unit;
        let (w, h) = (
            (self.crop[2] - self.crop[0]).abs() * u,
            (self.crop[3] - self.crop[1]).abs() * u,
        );
        if self.rotation == 90 || self.rotation == 270 {
            (h, w)
        } else {
            (w, h)
        }
    }
}

/// Which pages of the chosen range print, by the parity of their number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Subset {
    #[default]
    All,
    Odd,
    Even,
}

/// What of the document prints.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Content {
    /// The pages and their form fields (no comments).
    Document,
    /// The pages, their form fields and the printable comments.
    #[default]
    DocumentAndComments,
    /// Only the form fields, on blank paper (pre-printed forms).
    FormFieldsOnly,
}

/// How a page is scaled onto one sheet.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Sizing {
    /// Scaled to fill the printable area.
    #[default]
    Fit,
    /// At its own size (cut when it is larger than the paper).
    Actual,
    /// At its own size, made smaller only when it does not fit.
    ShrinkOversized,
    /// At a percentage of its own size (10 to 1000).
    Custom(f32),
}

/// The orientation of the layout on the paper.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Orientation {
    /// Chosen for each page (or each sheet) to fit best.
    #[default]
    Auto,
    Portrait,
    Landscape,
}

/// The order of the pages on a sheet with several pages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NupOrder {
    /// Rows from the top, each from the left.
    #[default]
    Horizontal,
    /// Rows from the top, each from the right.
    HorizontalReversed,
    /// Columns from the left, each from the top.
    Vertical,
    /// Columns from the right, each from the top.
    VerticalReversed,
}

/// The edge a booklet is bound on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Binding {
    #[default]
    Left,
    Right,
}

/// Which sides of the booklet sheets print (one side only for printing the
/// other side by hand).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BookletSides {
    #[default]
    Both,
    FrontOnly,
    BackOnly,
}

/// One-sided or two-sided output, and the edge the paper turns on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Sides {
    #[default]
    OneSided,
    /// Two-sided, turned on the long edge of the paper.
    LongEdge,
    /// Two-sided, turned on the short edge of the paper.
    ShortEdge,
}

impl Sides {
    pub fn duplex(self) -> bool {
        self != Sides::OneSided
    }
}

/// How the pages are arranged on the sheets.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Layout {
    /// One page on each sheet.
    Single {
        sizing: Sizing,
        /// Turn pages a quarter turn when they fit the paper better so.
        auto_rotate: bool,
        /// Centre the page on the paper (else at the top left).
        centre: bool,
        /// Each page on the paper of its own size, when the printer has it.
        paper_per_page: bool,
    },
    /// Several pages on each sheet (2, 4, 6, 9 or 16).
    NUp {
        per_sheet: u8,
        order: NupOrder,
        /// Space between the pages, in points.
        gutter: f32,
        borders: bool,
        auto_rotate: bool,
    },
    /// Two pages on each side of a sheet folded in the middle.
    Booklet {
        binding: Binding,
        sides: BookletSides,
        /// Sheets of each folded booklet (0: the whole document is one).
        sheets_per_booklet: u16,
        /// The last page stays on the back of the booklet.
        back_cover: bool,
        /// Space between the two pages of a side (at the fold), in points.
        gutter: f32,
    },
    /// Every page enlarged and cut into tiles printed on several sheets.
    Poster {
        /// The size of the poster as a percentage of the page (10 to 1000).
        scale_pct: f32,
        /// How much neighbouring tiles share, in points.
        overlap: f32,
        cut_marks: bool,
        labels: bool,
    },
}

impl Default for Layout {
    fn default() -> Self {
        Layout::Single {
            sizing: Sizing::Fit,
            auto_rotate: true,
            centre: true,
            paper_per_page: false,
        }
    }
}

/// A paper size with the margins the printer cannot print on.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Paper {
    /// The PWG media keyword (`iso_a4_210x297mm`) or a printer's own code.
    pub key: String,
    /// Width and height in points, as the paper is fed (`width <= height`).
    pub width: f32,
    pub height: f32,
    /// Left, bottom, right and top margins of the upright paper, in points.
    pub margins: [f32; 4],
}

impl Paper {
    /// A paper of `width` x `height` points (put upright) without margins.
    pub fn new(key: &str, width: f32, height: f32) -> Self {
        Paper {
            key: key.to_string(),
            width: width.min(height),
            height: width.max(height),
            margins: [0.0; 4],
        }
    }

    pub fn with_margins(mut self, margins: [f32; 4]) -> Self {
        self.margins = margins;
        self
    }
}

impl Default for Paper {
    fn default() -> Self {
        paper_for_key("iso_a4_210x297mm").unwrap_or_else(|| Paper::new("iso_a4_210x297mm", 595.28, 841.89))
    }
}

/// Everything the plan needs besides the pages.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PrintOptions {
    pub subset: Subset,
    /// Print the last sheet first.
    pub reverse: bool,
    pub layout: Layout,
    pub orientation: Orientation,
    pub paper: Paper,
    /// The papers of the printer, for the paper chosen by page size.
    pub alternatives: Vec<Paper>,
    pub sides: Sides,
}

/// What the security settings of a protected document allow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrintPermission {
    Allowed,
    /// Only a low-resolution picture of the pages may print.
    LowQuality,
    Forbidden,
}

impl crate::document::Document {
    /// What the security settings of the file the document was opened from
    /// allow for printing (everything for files without them).
    pub fn print_permission(&self) -> PrintPermission {
        permission_of(self.protection())
    }
}

pub(crate) fn permission_of(state: Option<&lopdf::EncryptionState>) -> PrintPermission {
    use lopdf::Permissions;
    let Some(state) = state else {
        return PrintPermission::Allowed;
    };
    let p = state.permissions();
    if !p.contains(Permissions::PRINTABLE) {
        PrintPermission::Forbidden
    } else if state.revision() >= 3 && !p.contains(Permissions::PRINTABLE_IN_HIGH_QUALITY) {
        PrintPermission::LowQuality
    } else {
        PrintPermission::Allowed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotations_snap_to_quarter_turns() {
        assert_eq!(snap_rotation(0), 0);
        assert_eq!(snap_rotation(44), 0);
        assert_eq!(snap_rotation(45), 90);
        assert_eq!(snap_rotation(-90), 270);
        assert_eq!(snap_rotation(450), 90);
        assert_eq!(snap_rotation(181), 180);
    }

    #[test]
    fn display_sizes_apply_user_unit_and_rotation() {
        let g = PageGeom {
            crop: [100.0, 50.0, 400.0, 650.0],
            rotation: 90,
            user_unit: 2.0,
        };
        assert_eq!(g.display_size(), (1200.0, 600.0));
    }
}
