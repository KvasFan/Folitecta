//! The sheets of a print job, as pure geometry: which page goes where on
//! which side of which sheet of paper.
//!
//! Matrices are `[a b c d e f]`, mapping `(x, y)` to
//! `(a x + c y + e, b x + d y + f)`; `mul(A, B)` applies `B`, then `A`.
//! Every sheet is a page of the upright paper (`width <= height`). A sheet
//! laid out across the paper (`landscape`) is drawn turned a quarter turn
//! counter-clockwise by `L = [0 1 -1 0 Pw 0]`: the top of the layout is on
//! the left edge of the paper. A page drawn on a sheet goes through its
//! placement matrix `m` (from the user space of the page to the layout),
//! then through `L`.
//!
//! Several pages per sheet turn the whole layout, never the pages of the
//! majority, so they read in order on the turned sheet; only a page whose
//! orientation differs from the others is turned in its cell.

use std::collections::HashMap;
use std::fmt;

use super::{
    Binding, BookletSides, Layout, NupOrder, Orientation, PageGeom, Paper, PrintOptions, Sides, Sizing,
    Subset,
};

/// The most sides a plan may have.
pub const MAX_SIDES: usize = 10_000;

/// Where one page goes on a sheet.
#[derive(Debug, Clone, PartialEq)]
pub struct Placement {
    /// Zero-based page of the document.
    pub page: usize,
    /// From the user space of the page to the layout of the sheet.
    pub m: [f32; 6],
    /// The area the page is clipped to (x, y, width, height in the layout).
    pub clip: [f32; 4],
    /// A thin frame around the placed page (x, y, width, height).
    pub border: Option<[f32; 4]>,
}

/// A mark drawn on a sheet, in the layout.
#[derive(Debug, Clone, PartialEq)]
pub enum Mark {
    /// A thin grey line from (x1, y1) to (x2, y2).
    Line([f32; 4]),
    /// A small label (ASCII only).
    Label { x: f32, y: f32, text: String },
}

/// The side of the paper a sheet prints on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Face {
    Single,
    Front,
    Back,
}

/// One side printed: a page of the print file.
#[derive(Debug, Clone, PartialEq)]
pub struct Sheet {
    pub paper: Paper,
    /// The layout is across the paper (turned onto it by `L`).
    pub landscape: bool,
    pub placements: Vec<Placement>,
    pub marks: Vec<Mark>,
    pub face: Face,
    /// Zero-based sheet of paper this side is on.
    pub physical: usize,
}

impl Sheet {
    /// The size of the layout: the paper, across it when `landscape`.
    pub fn layout_size(&self) -> (f32, f32) {
        layout_size(&self.paper, self.landscape)
    }

    /// The matrix from the layout to the paper.
    pub fn layout_matrix(&self) -> [f32; 6] {
        if self.landscape {
            [0.0, 1.0, -1.0, 0.0, self.paper.width, 0.0]
        } else {
            IDENTITY
        }
    }

    fn blank(paper: Paper) -> Self {
        Sheet {
            paper,
            landscape: false,
            placements: Vec::new(),
            marks: Vec::new(),
            face: Face::Single,
            physical: 0,
        }
    }
}

/// Something the user should know about a plan.
#[derive(Debug, Clone, PartialEq)]
pub enum PlanWarning {
    /// These pages (zero-based) are larger than the printable area and cut.
    Cut(Vec<usize>),
    /// Blank pages added to complete the booklet.
    Blanks(usize),
    /// A poster page is printed on `cols` x `rows` sheets.
    Tiles { cols: u32, rows: u32 },
}

/// The sheets of a print job.
#[derive(Debug, Clone, PartialEq)]
pub struct PrintPlan {
    /// The sides, in printing order.
    pub sheets: Vec<Sheet>,
    /// The pages printed, in order, after the subset.
    pub pages: Vec<usize>,
    /// One- or two-sided, as it prints (a booklet turns on the short edge).
    pub sides: Sides,
    /// Sheets of paper used by one copy.
    pub physical_sheets: usize,
    pub warnings: Vec<PlanWarning>,
}

/// Why a plan cannot be made.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanError {
    NoPages,
    BadScale,
    OverlapTooLarge,
    NoPrintableArea,
    TooManySheets,
}

impl fmt::Display for PlanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            PlanError::NoPages => "there are no pages to print",
            PlanError::BadScale => "the scale must be between 10% and 1000%",
            PlanError::OverlapTooLarge => "the overlap is too large for the paper",
            PlanError::NoPrintableArea => "the printer margins leave no room on this paper",
            PlanError::TooManySheets => "too many sheets to print at once",
        })
    }
}

impl std::error::Error for PlanError {}

const IDENTITY: [f32; 6] = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

/// `a · b`: apply `b`, then `a`.
pub fn mul(a: [f32; 6], b: [f32; 6]) -> [f32; 6] {
    [
        a[0] * b[0] + a[2] * b[1],
        a[1] * b[0] + a[3] * b[1],
        a[0] * b[2] + a[2] * b[3],
        a[1] * b[2] + a[3] * b[3],
        a[0] * b[4] + a[2] * b[5] + a[4],
        a[1] * b[4] + a[3] * b[5] + a[5],
    ]
}

/// A point through a matrix.
pub fn apply(m: [f32; 6], x: f32, y: f32) -> (f32, f32) {
    (m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5])
}

/// From the user space of a page to the page as shown, with its bottom left
/// corner at the origin: crop box moved, user unit and rotation applied.
pub fn page_matrix(g: &PageGeom) -> [f32; 6] {
    let u = g.user_unit;
    let [x0, y0, x1, y1] = g.crop;
    let (w0, h0) = ((x1 - x0) * u, (y1 - y0) * u);
    let n = [u, 0.0, 0.0, u, -x0 * u, -y0 * u];
    let r = match g.rotation {
        90 => [0.0, -1.0, 1.0, 0.0, 0.0, w0],
        180 => [-1.0, 0.0, 0.0, -1.0, w0, h0],
        270 => [0.0, 1.0, -1.0, 0.0, h0, 0.0],
        _ => IDENTITY,
    };
    mul(r, n)
}

/// A quarter turn counter-clockwise of a page of `w` x `h` (its top goes to
/// the left); the turned page is `h` x `w`.
fn quarter_turn(h: f32) -> [f32; 6] {
    [0.0, 1.0, -1.0, 0.0, h, 0.0]
}

fn layout_size(paper: &Paper, landscape: bool) -> (f32, f32) {
    if landscape {
        (paper.height, paper.width)
    } else {
        (paper.width, paper.height)
    }
}

/// Left, bottom, right and top margins of the layout.
fn layout_margins(paper: &Paper, landscape: bool) -> [f32; 4] {
    let [l, b, r, t] = paper.margins.map(|m| m.max(0.0));
    if landscape { [b, r, t, l] } else { [l, b, r, t] }
}

/// The printable area of the layout: x, y, width, height.
fn area(paper: &Paper, landscape: bool) -> [f32; 4] {
    let (lw, lh) = layout_size(paper, landscape);
    let [l, b, r, t] = layout_margins(paper, landscape);
    [l, b, lw - l - r, lh - b - t]
}

fn fit(cw: f32, ch: f32, w: f32, h: f32) -> f32 {
    (cw / w.max(0.01)).min(ch / h.max(0.01))
}

/// The pages of `pages` kept by the subset (by the parity of their number).
pub fn select(pages: &[usize], subset: Subset) -> Vec<usize> {
    pages
        .iter()
        .copied()
        .filter(|p| match subset {
            Subset::All => true,
            Subset::Odd => p % 2 == 0,
            Subset::Even => p % 2 == 1,
        })
        .collect()
}

/// Columns and rows of the grid of `per_sheet` pages, the long way first.
pub fn nup_grid(per_sheet: u8) -> (u32, u32) {
    match per_sheet {
        2 => (2, 1),
        4 => (2, 2),
        6 => (3, 2),
        9 => (3, 3),
        16 => (4, 4),
        n => {
            let n = u32::from(n.max(1));
            let c = (n as f32).sqrt().ceil() as u32;
            (c, n.div_ceil(c))
        }
    }
}

/// Plan the sheets of `pages` (zero-based, in order; repeats allowed).
pub fn plan(pages: &[usize], geoms: &[PageGeom], opts: &PrintOptions) -> Result<PrintPlan, PlanError> {
    let booklet = matches!(opts.layout, Layout::Booklet { .. });
    let pages: Vec<usize> = if booklet {
        pages.to_vec()
    } else {
        select(pages, opts.subset)
    }
    .into_iter()
    .filter(|p| *p < geoms.len())
    .collect();
    if pages.is_empty() {
        return Err(PlanError::NoPages);
    }
    let mut warnings = Vec::new();
    let mut fixed_sides = None;
    let mut sheets = match &opts.layout {
        Layout::Single {
            sizing,
            auto_rotate,
            centre,
            paper_per_page,
        } => single(
            &pages,
            geoms,
            opts,
            *sizing,
            *auto_rotate,
            *centre,
            *paper_per_page,
            &mut warnings,
        )?,
        Layout::NUp {
            per_sheet,
            order,
            gutter,
            borders,
            auto_rotate,
        } => nup(
            &pages,
            geoms,
            opts,
            *per_sheet,
            *order,
            *gutter,
            *borders,
            *auto_rotate,
        )?,
        Layout::Booklet {
            binding,
            sides,
            sheets_per_booklet,
            back_cover,
            gutter,
        } => {
            let (sheets, blanks) = booklet_sheets(
                &pages,
                geoms,
                &opts.paper,
                *binding,
                *sides,
                *sheets_per_booklet,
                *back_cover,
                *gutter,
            )?;
            if blanks > 0 {
                warnings.push(PlanWarning::Blanks(blanks));
            }
            fixed_sides = Some(match sides {
                BookletSides::Both => Sides::ShortEdge,
                _ => Sides::OneSided,
            });
            sheets
        }
        Layout::Poster {
            scale_pct,
            overlap,
            cut_marks,
            labels,
        } => {
            fixed_sides = Some(Sides::OneSided);
            poster(
                &pages,
                geoms,
                opts,
                *scale_pct,
                *overlap,
                *cut_marks,
                *labels,
                &mut warnings,
            )?
        }
    };
    let sides = fixed_sides.unwrap_or(opts.sides);
    if sheets.len() > MAX_SIDES {
        return Err(PlanError::TooManySheets);
    }
    let physical_sheets = if sides.duplex() {
        if sheets.len() % 2 == 1 {
            let paper = sheets.last().map(|s| s.paper.clone()).unwrap_or_default();
            sheets.push(Sheet::blank(paper));
        }
        for (i, s) in sheets.iter_mut().enumerate() {
            s.face = if i % 2 == 0 { Face::Front } else { Face::Back };
        }
        if opts.reverse {
            let mut pairs: Vec<Vec<Sheet>> = Vec::new();
            let mut it = sheets.into_iter();
            while let (Some(a), Some(b)) = (it.next(), it.next()) {
                pairs.push(vec![a, b]);
            }
            pairs.reverse();
            sheets = pairs.into_iter().flatten().collect();
        }
        for (i, s) in sheets.iter_mut().enumerate() {
            s.physical = i / 2;
        }
        sheets.len() / 2
    } else {
        if opts.reverse {
            sheets.reverse();
        }
        for (i, s) in sheets.iter_mut().enumerate() {
            s.physical = i;
        }
        sheets.len()
    };
    Ok(PrintPlan {
        sheets,
        pages,
        sides,
        physical_sheets,
        warnings,
    })
}

fn landscape_for(orientation: Orientation, w: f32, h: f32) -> bool {
    match orientation {
        Orientation::Auto => w > h,
        Orientation::Portrait => false,
        Orientation::Landscape => true,
    }
}

#[allow(clippy::too_many_arguments)]
fn single(
    pages: &[usize],
    geoms: &[PageGeom],
    opts: &PrintOptions,
    sizing: Sizing,
    auto_rotate: bool,
    centre: bool,
    paper_per_page: bool,
    warnings: &mut Vec<PlanWarning>,
) -> Result<Vec<Sheet>, PlanError> {
    if let Sizing::Custom(pct) = sizing {
        if !(10.0..=1000.0).contains(&pct) {
            return Err(PlanError::BadScale);
        }
    }
    let standard: Vec<Paper>;
    let alternatives: &[Paper] = if opts.alternatives.is_empty() {
        standard = super::STANDARD_PAPERS
            .iter()
            .map(|p| Paper::new(p.key, p.width, p.height))
            .collect();
        &standard
    } else {
        &opts.alternatives
    };
    let mut cut = Vec::new();
    let mut sheets = Vec::with_capacity(pages.len());
    for &p in pages {
        let g = &geoms[p];
        let (w, h) = g.display_size();
        let t = page_matrix(g);
        let own = if paper_per_page {
            super::closest_paper(w, h, alternatives, 3.0).cloned()
        } else {
            None
        };
        let (paper, sizing, landscape, may_turn) = match own {
            Some(paper) => (paper, Sizing::Actual, w > h, false),
            None => (
                opts.paper.clone(),
                sizing,
                landscape_for(opts.orientation, w, h),
                auto_rotate && opts.orientation != Orientation::Auto,
            ),
        };
        let a = area(&paper, landscape);
        if a[2] <= 1.0 || a[3] <= 1.0 {
            return Err(PlanError::NoPrintableArea);
        }
        let (lw, lh) = layout_size(&paper, landscape);
        let turn = may_turn && fit(a[2], a[3], h, w) > fit(a[2], a[3], w, h) * 1.0001;
        let (pw, ph, base) = if turn {
            (h, w, mul(quarter_turn(h), t))
        } else {
            (w, h, t)
        };
        let f = fit(a[2], a[3], pw, ph);
        let s = match sizing {
            Sizing::Fit => f,
            Sizing::Actual => 1.0,
            Sizing::ShrinkOversized => f.min(1.0),
            Sizing::Custom(pct) => pct / 100.0,
        };
        let whole = matches!(sizing, Sizing::Actual | Sizing::Custom(_));
        let cell = if whole && centre { [0.0, 0.0, lw, lh] } else { a };
        let (px, py) = if centre {
            (
                cell[0] + (cell[2] - pw * s) / 2.0,
                cell[1] + (cell[3] - ph * s) / 2.0,
            )
        } else {
            (cell[0], cell[1] + cell[3] - ph * s)
        };
        let clip = if whole { [0.0, 0.0, lw, lh] } else { a };
        let placed = [px, py, pw * s, ph * s];
        if outside(placed, a) {
            cut.push(p);
        }
        sheets.push(Sheet {
            paper,
            landscape,
            placements: vec![Placement {
                page: p,
                m: mul([s, 0.0, 0.0, s, px, py], base),
                clip,
                border: None,
            }],
            marks: Vec::new(),
            face: Face::Single,
            physical: 0,
        });
    }
    if !cut.is_empty() {
        cut.dedup();
        warnings.push(PlanWarning::Cut(cut));
    }
    Ok(sheets)
}

/// The placed box leaves the area by more than half a point.
fn outside(b: [f32; 4], a: [f32; 4]) -> bool {
    b[0] < a[0] - 0.5
        || b[1] < a[1] - 0.5
        || b[0] + b[2] > a[0] + a[2] + 0.5
        || b[1] + b[3] > a[1] + a[3] + 0.5
}

/// The display size shared by most pages (rounded to a point).
fn majority_size(pages: &[usize], geoms: &[PageGeom]) -> (f32, f32) {
    let mut counts: HashMap<(i32, i32), (usize, usize)> = HashMap::new();
    for (i, &p) in pages.iter().enumerate() {
        let (w, h) = geoms[p].display_size();
        let e = counts
            .entry((w.round() as i32, h.round() as i32))
            .or_insert((0, i));
        e.0 += 1;
    }
    counts
        .into_iter()
        .max_by(|a, b| a.1.0.cmp(&b.1.0).then(b.1.1.cmp(&a.1.1)))
        .map(|((w, h), _)| (w.max(1) as f32, h.max(1) as f32))
        .unwrap_or((612.0, 792.0))
}

fn cell_of(k: u32, cols: u32, rows: u32, order: NupOrder) -> (u32, u32) {
    match order {
        NupOrder::Horizontal => (k % cols, k / cols),
        NupOrder::HorizontalReversed => (cols - 1 - k % cols, k / cols),
        NupOrder::Vertical => (k / rows, k % rows),
        NupOrder::VerticalReversed => (cols - 1 - k / rows, k % rows),
    }
}

#[allow(clippy::too_many_arguments)]
fn nup(
    pages: &[usize],
    geoms: &[PageGeom],
    opts: &PrintOptions,
    per_sheet: u8,
    order: NupOrder,
    gutter: f32,
    borders: bool,
    auto_rotate: bool,
) -> Result<Vec<Sheet>, PlanError> {
    let (a, b) = nup_grid(per_sheet);
    let n = (a * b) as usize;
    let g = gutter.max(0.0);
    let (mw, mh) = majority_size(pages, geoms);
    let orientations: &[bool] = match opts.orientation {
        Orientation::Auto => &[false, true],
        Orientation::Portrait => &[false],
        Orientation::Landscape => &[true],
    };
    // Each candidate: (score, landscape, cols, rows). The pages of the
    // majority are never turned, so the order reads on the sheet as laid out.
    let mut best: Option<(f32, bool, u32, u32)> = None;
    for &landscape in orientations {
        let grids = if a == b {
            vec![(a, b)]
        } else {
            vec![(a, b), (b, a)]
        };
        for (cols, rows) in grids {
            let ar = area(&opts.paper, landscape);
            let cw = (ar[2] - (cols - 1) as f32 * g) / cols as f32;
            let ch = (ar[3] - (rows - 1) as f32 * g) / rows as f32;
            if cw <= 1.0 || ch <= 1.0 {
                continue;
            }
            let score = fit(cw, ch, mw, mh);
            let better = match best {
                None => true,
                Some((s, bl, bc, br)) => {
                    if score > s * (1.0 + 1e-5) {
                        true
                    } else if score < s * (1.0 - 1e-5) {
                        false
                    } else {
                        // A tie: portrait first, then more columns than rows.
                        (bl && !landscape) || (bl == landscape && bc < br && cols >= rows)
                    }
                }
            };
            if better {
                best = Some((score, landscape, cols, rows));
            }
        }
    }
    let Some((_, landscape, cols, rows)) = best else {
        return Err(PlanError::NoPrintableArea);
    };
    let ar = area(&opts.paper, landscape);
    let cw = (ar[2] - (cols - 1) as f32 * g) / cols as f32;
    let ch = (ar[3] - (rows - 1) as f32 * g) / rows as f32;
    let majority_wide = mw > mh;
    let mut sheets = Vec::new();
    for chunk in pages.chunks(n) {
        let mut placements = Vec::with_capacity(chunk.len());
        for (k, &p) in chunk.iter().enumerate() {
            let (col, row) = cell_of(k as u32, cols, rows, order);
            let cx = ar[0] + col as f32 * (cw + g);
            let cy = ar[1] + ar[3] - (row + 1) as f32 * ch - row as f32 * g;
            let geom = &geoms[p];
            let (w, h) = geom.display_size();
            let t = page_matrix(geom);
            let turn =
                auto_rotate && (w > h) != majority_wide && fit(cw, ch, h, w) > fit(cw, ch, w, h) * 1.0001;
            let (pw, ph, base) = if turn {
                (h, w, mul(quarter_turn(h), t))
            } else {
                (w, h, t)
            };
            let s = fit(cw, ch, pw, ph);
            let px = cx + (cw - pw * s) / 2.0;
            let py = cy + (ch - ph * s) / 2.0;
            placements.push(Placement {
                page: p,
                m: mul([s, 0.0, 0.0, s, px, py], base),
                clip: [cx, cy, cw, ch],
                border: borders.then_some([px, py, pw * s, ph * s]),
            });
        }
        sheets.push(Sheet {
            paper: opts.paper.clone(),
            landscape,
            placements,
            marks: Vec::new(),
            face: Face::Single,
            physical: 0,
        });
    }
    Ok(sheets)
}

/// The sides of a booklet and the number of blank pages added.
#[allow(clippy::too_many_arguments)]
fn booklet_sheets(
    pages: &[usize],
    geoms: &[PageGeom],
    paper: &Paper,
    binding: Binding,
    which: BookletSides,
    sheets_per_booklet: u16,
    back_cover: bool,
    gutter: f32,
) -> Result<(Vec<Sheet>, usize), PlanError> {
    // Margins equal on opposite edges, so the fold is in the middle of the
    // paper on both faces whatever the printer's margins.
    let [l, b, r, t] = paper.margins.map(|m| m.max(0.0));
    let (lw, lh) = layout_size(paper, true);
    let side_margin = b.max(t);
    let end_margin = l.max(r);
    let ar = [
        side_margin,
        end_margin,
        lw - 2.0 * side_margin,
        lh - 2.0 * end_margin,
    ];
    let g = gutter.max(0.0);
    let half = (ar[2] - g) / 2.0;
    if half <= 1.0 || ar[3] <= 1.0 {
        return Err(PlanError::NoPrintableArea);
    }
    let cells = [
        [ar[0], ar[1], half, ar[3]],
        [ar[0] + (ar[2] + g) / 2.0, ar[1], half, ar[3]],
    ];
    let per = if sheets_per_booklet == 0 {
        pages.len().max(1)
    } else {
        4 * sheets_per_booklet as usize
    };
    let mut blanks = 0;
    let mut fronts = Vec::new();
    let mut backs = Vec::new();
    for signature in pages.chunks(per) {
        let n = signature.len();
        let m = n.div_ceil(4) * 4;
        blanks += m - n;
        let mut q: Vec<Option<usize>> = signature.iter().map(|p| Some(*p)).collect();
        if back_cover && n >= 2 {
            let last = q.pop().flatten();
            q.extend(std::iter::repeat_n(None, m - n));
            q.push(last);
        } else {
            q.extend(std::iter::repeat_n(None, m - n));
        }
        for i in 0..m / 4 {
            let mut front = (q[m - 1 - 2 * i], q[2 * i]);
            let mut back = (q[2 * i + 1], q[m - 2 - 2 * i]);
            if binding == Binding::Right {
                front = (front.1, front.0);
                back = (back.1, back.0);
            }
            fronts.push(front);
            backs.push(back);
        }
    }
    let side = |(left, right): (Option<usize>, Option<usize>)| -> Sheet {
        let mut placements = Vec::new();
        for (slot, page) in [(0usize, left), (1, right)] {
            let Some(p) = page else { continue };
            let c = cells[slot];
            let geom = &geoms[p];
            let (w, h) = geom.display_size();
            let t = page_matrix(geom);
            let turn = fit(c[2], c[3], h, w) > fit(c[2], c[3], w, h) * 1.0001;
            let (pw, ph, base) = if turn {
                (h, w, mul(quarter_turn(h), t))
            } else {
                (w, h, t)
            };
            let s = fit(c[2], c[3], pw, ph);
            // Against the fold: the left page on the right of its half.
            let px = if slot == 0 { c[0] + c[2] - pw * s } else { c[0] };
            let py = c[1] + (c[3] - ph * s) / 2.0;
            placements.push(Placement {
                page: p,
                m: mul([s, 0.0, 0.0, s, px, py], base),
                clip: c,
                border: None,
            });
        }
        Sheet {
            paper: paper.clone(),
            landscape: true,
            placements,
            marks: Vec::new(),
            face: Face::Single,
            physical: 0,
        }
    };
    let sheets = match which {
        BookletSides::Both => fronts
            .into_iter()
            .zip(backs)
            .flat_map(|(f, b)| [side(f), side(b)])
            .collect(),
        BookletSides::FrontOnly => fronts
            .into_iter()
            .map(|f| Sheet {
                face: Face::Front,
                ..side(f)
            })
            .collect(),
        BookletSides::BackOnly => backs
            .into_iter()
            .map(|b| Sheet {
                face: Face::Back,
                ..side(b)
            })
            .collect(),
    };
    Ok((sheets, blanks))
}

/// The label of a row of tiles: A to Z, then AA, AB, ...
fn row_label(mut r: u32) -> String {
    let mut out = Vec::new();
    loop {
        out.push(b'A' + (r % 26) as u8);
        if r < 26 {
            break;
        }
        r = r / 26 - 1;
    }
    out.reverse();
    String::from_utf8(out).unwrap_or_default()
}

#[allow(clippy::too_many_arguments)]
fn poster(
    pages: &[usize],
    geoms: &[PageGeom],
    opts: &PrintOptions,
    scale_pct: f32,
    overlap: f32,
    cut_marks: bool,
    labels: bool,
    warnings: &mut Vec<PlanWarning>,
) -> Result<Vec<Sheet>, PlanError> {
    if !(10.0..=1000.0).contains(&scale_pct) {
        return Err(PlanError::BadScale);
    }
    let t = scale_pct / 100.0;
    let o = overlap.max(0.0);
    let orientations: &[bool] = match opts.orientation {
        Orientation::Auto => &[false, true],
        Orientation::Portrait => &[false],
        Orientation::Landscape => &[true],
    };
    let mut sheets = Vec::new();
    let mut first_grid = None;
    for &p in pages {
        let geom = &geoms[p];
        let (w, h) = geom.display_size();
        let (pw, ph) = (w * t, h * t);
        let mut best: Option<(u32, u32, bool)> = None;
        let mut any_area = false;
        for &landscape in orientations {
            let ar = area(&opts.paper, landscape);
            if ar[2] <= 1.0 || ar[3] <= 1.0 {
                continue;
            }
            any_area = true;
            if o >= ar[2].min(ar[3]) / 2.0 {
                continue;
            }
            let (sx, sy) = (ar[2] - o, ar[3] - o);
            let cols = (((pw - o) / sx - 1e-4).ceil() as i64).max(1) as u32;
            let rows = (((ph - o) / sy - 1e-4).ceil() as i64).max(1) as u32;
            if best.is_none_or(|(c, r, _)| cols * rows < c * r) {
                best = Some((cols, rows, landscape));
            }
        }
        let Some((cols, rows, landscape)) = best else {
            return Err(if any_area {
                PlanError::OverlapTooLarge
            } else {
                PlanError::NoPrintableArea
            });
        };
        if (cols as usize) * (rows as usize) + sheets.len() > MAX_SIDES {
            return Err(PlanError::TooManySheets);
        }
        first_grid.get_or_insert((cols, rows));
        let ar = area(&opts.paper, landscape);
        let (sx, sy) = (ar[2] - o, ar[3] - o);
        let ox = (cols as f32 * sx + o - pw) / 2.0;
        let oy = (rows as f32 * sy + o - ph) / 2.0;
        let base = page_matrix(geom);
        let n = cols * rows;
        for r in 0..rows {
            for c in 0..cols {
                let x0 = c as f32 * sx - ox;
                let y_top = ph + oy - r as f32 * sy;
                let y0 = y_top - ar[3];
                let m = mul([t, 0.0, 0.0, t, ar[0] - x0, ar[1] - y0], base);
                let mut marks = Vec::new();
                if cut_marks {
                    const TICK: f32 = 10.0;
                    if c + 1 < cols {
                        let x = ar[0] + ar[2] - o;
                        marks.push(Mark::Line([x, ar[1], x, ar[1] + TICK]));
                        marks.push(Mark::Line([x, ar[1] + ar[3] - TICK, x, ar[1] + ar[3]]));
                    }
                    if r + 1 < rows {
                        let y = ar[1] + o;
                        marks.push(Mark::Line([ar[0], y, ar[0] + TICK, y]));
                        marks.push(Mark::Line([ar[0] + ar[2] - TICK, y, ar[0] + ar[2], y]));
                    }
                }
                if labels {
                    let k = r * cols + c + 1;
                    marks.push(Mark::Label {
                        x: ar[0] + 4.0,
                        y: ar[1] + 4.0,
                        text: format!("p{} {}{} ({k}/{n})", p + 1, row_label(r), c + 1),
                    });
                }
                sheets.push(Sheet {
                    paper: opts.paper.clone(),
                    landscape,
                    placements: vec![Placement {
                        page: p,
                        m,
                        clip: ar,
                        border: None,
                    }],
                    marks,
                    face: Face::Single,
                    physical: 0,
                });
            }
        }
    }
    if let Some((cols, rows)) = first_grid {
        warnings.push(PlanWarning::Tiles { cols, rows });
    }
    Ok(sheets)
}

#[cfg(test)]
mod tests {
    use super::*;

    const A4: (f32, f32) = (595.28, 841.89);

    fn geom(w: f32, h: f32) -> PageGeom {
        PageGeom {
            crop: [0.0, 0.0, w, h],
            rotation: 0,
            user_unit: 1.0,
        }
    }

    fn a4() -> Paper {
        Paper::new("iso_a4_210x297mm", A4.0, A4.1)
    }

    fn opts(layout: Layout) -> PrintOptions {
        PrintOptions {
            layout,
            paper: a4(),
            ..Default::default()
        }
    }

    fn near(a: [f32; 6], b: [f32; 6], tol: f32) -> bool {
        a.iter().zip(b.iter()).all(|(x, y)| (x - y).abs() <= tol)
    }

    /// Where a point of a page lands on the upright paper.
    fn on_paper(s: &Sheet, pl: &Placement, x: f32, y: f32) -> (f32, f32) {
        let m = mul(s.layout_matrix(), pl.m);
        apply(m, x, y)
    }

    fn nup_layout(per_sheet: u8, order: NupOrder) -> Layout {
        Layout::NUp {
            per_sheet,
            order,
            gutter: 0.0,
            borders: false,
            auto_rotate: true,
        }
    }

    fn booklet(binding: Binding, sides: BookletSides, per: u16, back_cover: bool) -> Layout {
        Layout::Booklet {
            binding,
            sides,
            sheets_per_booklet: per,
            back_cover,
            gutter: 0.0,
        }
    }

    /// Pages of each side, 1-based, 0 for a blank half: (left, right) of the layout.
    fn halves(plan: &PrintPlan) -> Vec<(usize, usize)> {
        plan.sheets
            .iter()
            .map(|s| {
                let mid = s.layout_size().0 / 2.0;
                let mut lr = (0, 0);
                for pl in &s.placements {
                    if pl.clip[0] + pl.clip[2] / 2.0 < mid {
                        lr.0 = pl.page + 1;
                    } else {
                        lr.1 = pl.page + 1;
                    }
                }
                lr
            })
            .collect()
    }

    #[test]
    fn letter_fits_on_a4() {
        let p = plan(&[0], &[geom(612.0, 792.0)], &opts(Layout::default())).unwrap();
        assert_eq!(p.sheets.len(), 1);
        let s = &p.sheets[0];
        assert!(!s.landscape);
        let k = 595.28 / 612.0;
        let m = s.placements[0].m;
        assert!(
            near(m, [k, 0.0, 0.0, k, 0.0, (841.89 - 792.0 * k) / 2.0], 0.01),
            "{m:?}"
        );
        assert!((m[0] - 0.97268).abs() < 1e-4 && (m[5] - 35.76).abs() < 0.01);
    }

    #[test]
    fn a_landscape_page_turns_the_layout() {
        let p = plan(&[0], &[geom(842.0, 595.0)], &opts(Layout::default())).unwrap();
        let s = &p.sheets[0];
        assert!(s.landscape);
        assert_eq!(s.layout_matrix(), [0.0, 1.0, -1.0, 0.0, 595.28, 0.0]);
        let both = mul(s.layout_matrix(), s.placements[0].m);
        let k = (841.89f32 / 842.0).min(595.28 / 595.0);
        assert!((k - 0.99987).abs() < 1e-4);
        assert!(
            near(
                both,
                [0.0, k, -k, 0.0, 595.28 - (595.28 - 595.0 * k) / 2.0, 0.0],
                0.02
            ),
            "{both:?}"
        );
        // The top of the page is on the left edge of the paper.
        let (x, _) = on_paper(s, &s.placements[0], 421.0, 595.0);
        assert!(x < 1.0);
    }

    #[test]
    fn rotated_and_cropped_pages_map_their_corners() {
        let g = PageGeom {
            crop: [100.0, 50.0, 400.0, 650.0],
            rotation: 90,
            user_unit: 1.0,
        };
        // Shown 600 x 300: landscape layout, the shown page fills its width.
        let p = plan(&[0], &[g], &opts(Layout::default())).unwrap();
        let s = &p.sheets[0];
        let pl = &s.placements[0];
        let m = pl.m;
        // The bottom left corner of the crop box is shown top left.
        let (x, y) = apply(m, 100.0, 50.0);
        let (lw, lh) = s.layout_size();
        let k = (lw / 600.0).min(lh / 300.0);
        assert!((x - (lw - 600.0 * k) / 2.0).abs() < 0.05, "{x}");
        assert!((y - ((lh - 300.0 * k) / 2.0 + 300.0 * k)).abs() < 0.05, "{y}");
        // The top left corner of the crop box is shown top right.
        let (x, y) = apply(m, 100.0, 650.0);
        assert!((x - ((lw - 600.0 * k) / 2.0 + 600.0 * k)).abs() < 0.05, "{x}");
        assert!((y - ((lh - 300.0 * k) / 2.0 + 300.0 * k)).abs() < 0.05, "{y}");
    }

    #[test]
    fn user_unit_doubles_the_size() {
        let g = PageGeom {
            crop: [0.0, 0.0, 300.0, 400.0],
            rotation: 0,
            user_unit: 2.0,
        };
        let o = opts(Layout::Single {
            sizing: Sizing::Actual,
            auto_rotate: false,
            centre: true,
            paper_per_page: false,
        });
        let p = plan(&[0], &[g], &o).unwrap();
        let m = p.sheets[0].placements[0].m;
        assert!((m[0] - 2.0).abs() < 1e-5 && (m[3] - 2.0).abs() < 1e-5);
    }

    #[test]
    fn actual_size_warns_about_cut_pages_and_shrink_only_shrinks() {
        let geoms = [geom(1190.0, 1684.0), geom(300.0, 400.0)];
        let mut o = opts(Layout::Single {
            sizing: Sizing::Actual,
            auto_rotate: false,
            centre: true,
            paper_per_page: false,
        });
        let p = plan(&[0, 1], &geoms, &o).unwrap();
        assert_eq!(p.warnings, vec![PlanWarning::Cut(vec![0])]);
        assert_eq!(p.sheets[0].placements[0].m[0], 1.0);
        o.layout = Layout::Single {
            sizing: Sizing::ShrinkOversized,
            auto_rotate: false,
            centre: true,
            paper_per_page: false,
        };
        let p = plan(&[0, 1], &geoms, &o).unwrap();
        assert!(p.warnings.is_empty());
        assert!(p.sheets[0].placements[0].m[0] < 0.6);
        assert_eq!(p.sheets[1].placements[0].m[0], 1.0);
        o.layout = Layout::Single {
            sizing: Sizing::Custom(5.0),
            auto_rotate: false,
            centre: true,
            paper_per_page: false,
        };
        assert_eq!(plan(&[0], &geoms, &o), Err(PlanError::BadScale));
    }

    #[test]
    fn paper_by_page_picks_each_pages_paper() {
        let geoms = [geom(612.0, 792.0), geom(842.0, 595.0), geom(500.0, 500.0)];
        let mut o = opts(Layout::Single {
            sizing: Sizing::Fit,
            auto_rotate: true,
            centre: true,
            paper_per_page: true,
        });
        o.alternatives = vec![a4(), Paper::new("na_letter_8.5x11in", 612.0, 792.0)];
        let p = plan(&[0, 1, 2], &geoms, &o).unwrap();
        assert_eq!(p.sheets[0].paper.key, "na_letter_8.5x11in");
        assert_eq!(p.sheets[1].paper.key, "iso_a4_210x297mm");
        assert!(p.sheets[1].landscape);
        // No paper of its size: the chosen paper, fitted.
        assert_eq!(p.sheets[2].paper.key, "iso_a4_210x297mm");
        assert!((p.sheets[2].placements[0].m[0] - 595.28 / 500.0).abs() < 1e-3);
    }

    #[test]
    fn two_portrait_pages_side_by_side_on_landscape_paper() {
        let geoms = vec![geom(A4.0, A4.1); 2];
        let p = plan(&[0, 1], &geoms, &opts(nup_layout(2, NupOrder::Horizontal))).unwrap();
        assert_eq!(p.sheets.len(), 1);
        let s = &p.sheets[0];
        assert!(s.landscape, "the whole layout turns, not the pages");
        let k = (420.945f32 / 595.28).min(595.28 / 841.89);
        for pl in &s.placements {
            assert!((pl.m[0] - k).abs() < 1e-4 && pl.m[1].abs() < 1e-6, "{:?}", pl.m);
            assert!((pl.m[0] - 0.70708).abs() < 1e-4);
        }
        // Read on the turned sheet: page 1 on the left, so at the bottom of
        // the upright paper, page 2 above it.
        let y1 = on_paper(s, &s.placements[0], 300.0, 400.0).1;
        let y2 = on_paper(s, &s.placements[1], 300.0, 400.0).1;
        assert!(y1 < 421.0 && y2 > 421.0, "{y1} {y2}");
    }

    #[test]
    fn four_up_orders() {
        let geoms = vec![geom(A4.0, A4.1); 4];
        let corner = |order| {
            let p = plan(&[0, 1, 2, 3], &geoms, &opts(nup_layout(4, order))).unwrap();
            let s = &p.sheets[0];
            assert!(!s.landscape);
            s.placements
                .iter()
                .map(|pl| {
                    let (x, y) = (pl.clip[0] + pl.clip[2] / 2.0, pl.clip[1] + pl.clip[3] / 2.0);
                    match (x < 297.0, y > 421.0) {
                        (true, true) => "TL",
                        (false, true) => "TR",
                        (true, false) => "BL",
                        (false, false) => "BR",
                    }
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(corner(NupOrder::Horizontal), ["TL", "TR", "BL", "BR"]);
        assert_eq!(corner(NupOrder::Vertical), ["TL", "BL", "TR", "BR"]);
        assert_eq!(corner(NupOrder::HorizontalReversed), ["TR", "TL", "BR", "BL"]);
        assert_eq!(corner(NupOrder::VerticalReversed), ["TR", "BR", "TL", "BL"]);
    }

    #[test]
    fn grids_gutters_borders_and_mixed_sizes() {
        for (n, cols, rows) in [(6u8, 3u32, 2u32), (9, 3, 3), (16, 4, 4)] {
            let geoms = vec![geom(A4.0, A4.1); n as usize];
            let pages: Vec<usize> = (0..n as usize).collect();
            let p = plan(&pages, &geoms, &opts(nup_layout(n, NupOrder::Horizontal))).unwrap();
            let s = &p.sheets[0];
            let mut xs: Vec<i32> = s.placements.iter().map(|pl| pl.clip[0] as i32).collect();
            let mut ys: Vec<i32> = s.placements.iter().map(|pl| pl.clip[1] as i32).collect();
            xs.sort();
            xs.dedup();
            ys.sort();
            ys.dedup();
            // Six portrait pages: three across landscape paper, two down.
            assert_eq!((xs.len() as u32, ys.len() as u32), (cols, rows), "{n}");
        }
        let geoms = vec![geom(A4.0, A4.1); 4];
        let mut o = opts(Layout::NUp {
            per_sheet: 4,
            order: NupOrder::Horizontal,
            gutter: 10.0,
            borders: true,
            auto_rotate: true,
        });
        o.paper.margins = [10.0; 4];
        let p = plan(&[0, 1, 2, 3], &geoms, &o).unwrap();
        let s = &p.sheets[0];
        let cw = (A4.0 - 20.0 - 10.0) / 2.0;
        assert!((s.placements[0].clip[2] - cw).abs() < 1e-3);
        assert!((s.placements[1].clip[0] - (10.0 + cw + 10.0)).abs() < 1e-3);
        assert!(s.placements.iter().all(|pl| pl.border.is_some()));
        // A landscape page among portrait ones turns in its cell.
        let geoms = vec![
            geom(A4.0, A4.1),
            geom(A4.1, A4.0),
            geom(A4.0, A4.1),
            geom(A4.0, A4.1),
        ];
        let p = plan(&[0, 1, 2, 3], &geoms, &opts(nup_layout(4, NupOrder::Horizontal))).unwrap();
        let s = &p.sheets[0];
        assert!(s.placements[0].m[1].abs() < 1e-6);
        assert!(s.placements[1].m[1].abs() > 0.1, "the odd page turns");
        // Fixed portrait with landscape pages: one above the other, not turned.
        let geoms = vec![geom(A4.1, A4.0); 2];
        let mut o = opts(nup_layout(2, NupOrder::Horizontal));
        o.orientation = Orientation::Portrait;
        let p = plan(&[0, 1], &geoms, &o).unwrap();
        assert!(!p.sheets[0].landscape);
        let (y1, y2) = (
            p.sheets[0].placements[0].clip[1],
            p.sheets[0].placements[1].clip[1],
        );
        assert!(y1 > y2, "page 1 on top");
    }

    #[test]
    fn booklets_of_eight_and_five_pages() {
        let geoms = vec![geom(A4.0, A4.1); 8];
        let o = opts(booklet(Binding::Left, BookletSides::Both, 0, false));
        let p = plan(&(0..8).collect::<Vec<_>>(), &geoms, &o).unwrap();
        assert_eq!(halves(&p), [(8, 1), (2, 7), (6, 3), (4, 5)]);
        assert_eq!(p.sides, Sides::ShortEdge);
        assert_eq!(p.physical_sheets, 2);
        assert_eq!(
            p.sheets.iter().map(|s| (s.face, s.physical)).collect::<Vec<_>>(),
            [
                (Face::Front, 0),
                (Face::Back, 0),
                (Face::Front, 1),
                (Face::Back, 1)
            ]
        );
        let p = plan(&(0..5).collect::<Vec<_>>(), &geoms, &o).unwrap();
        assert_eq!(halves(&p), [(0, 1), (2, 0), (0, 3), (4, 5)]);
        assert_eq!(p.warnings, vec![PlanWarning::Blanks(3)]);
        let o = opts(booklet(Binding::Left, BookletSides::Both, 0, true));
        let p = plan(&(0..5).collect::<Vec<_>>(), &geoms, &o).unwrap();
        assert_eq!(halves(&p), [(5, 1), (2, 0), (0, 3), (4, 0)]);
        // A single page stays the front, not the back cover.
        let p = plan(&[0], &geoms, &o).unwrap();
        assert_eq!(halves(&p), [(0, 1), (0, 0)]);
        let o = opts(booklet(Binding::Right, BookletSides::Both, 0, false));
        let p = plan(&(0..8).collect::<Vec<_>>(), &geoms, &o).unwrap();
        assert_eq!(halves(&p), [(1, 8), (7, 2), (3, 6), (5, 4)]);
        let o = opts(booklet(Binding::Left, BookletSides::Both, 1, false));
        let p = plan(&(0..8).collect::<Vec<_>>(), &geoms, &o).unwrap();
        assert_eq!(halves(&p), [(4, 1), (2, 3), (8, 5), (6, 7)]);
        let o = opts(booklet(Binding::Left, BookletSides::FrontOnly, 0, false));
        let p = plan(&(0..8).collect::<Vec<_>>(), &geoms, &o).unwrap();
        assert_eq!(halves(&p), [(8, 1), (6, 3)]);
        assert_eq!(p.sides, Sides::OneSided);
        let o = opts(booklet(Binding::Left, BookletSides::BackOnly, 0, false));
        let p = plan(&(0..8).collect::<Vec<_>>(), &geoms, &o).unwrap();
        assert_eq!(halves(&p), [(2, 7), (4, 5)]);
        assert!(p.sheets.iter().all(|s| s.face == Face::Back));
    }

    #[test]
    fn booklet_pages_meet_at_the_fold_in_the_middle_of_the_paper() {
        let geoms = vec![geom(A4.0, A4.1); 4];
        let mut o = opts(booklet(Binding::Left, BookletSides::Both, 0, false));
        // Unequal margins, as on many printers (a larger bottom margin).
        o.paper.margins = [12.0, 30.0, 8.0, 10.0];
        let p = plan(&[0, 1, 2, 3], &geoms, &o).unwrap();
        for s in &p.sheets {
            assert!(s.landscape);
            let left = &s.placements[0];
            let right = &s.placements[1];
            // On the upright paper the fold is the line y = height / 2.
            let left_top_edge = on_paper(s, left, A4.0, 0.0).1.max(on_paper(s, left, 0.0, 0.0).1);
            let right_low_edge = on_paper(s, right, 0.0, 0.0)
                .1
                .min(on_paper(s, right, A4.0, 0.0).1);
            assert!((left_top_edge - A4.1 / 2.0).abs() < 0.05, "{left_top_edge}");
            assert!((right_low_edge - A4.1 / 2.0).abs() < 0.05, "{right_low_edge}");
        }
    }

    #[test]
    fn posters_tile_the_enlarged_page() {
        let geoms = [geom(A4.0, A4.1)];
        let poster = |overlap: f32| {
            opts(Layout::Poster {
                scale_pct: 200.0,
                overlap,
                cut_marks: true,
                labels: true,
            })
        };
        let p = plan(&[0], &geoms, &poster(0.0)).unwrap();
        assert_eq!(p.sheets.len(), 4);
        assert!(!p.sheets[0].landscape);
        assert_eq!(p.warnings, vec![PlanWarning::Tiles { cols: 2, rows: 2 }]);
        let p = plan(&[0], &geoms, &poster(20.0)).unwrap();
        assert_eq!(p.sheets.len(), 6);
        assert!(p.sheets[0].landscape);
        assert_eq!(p.warnings, vec![PlanWarning::Tiles { cols: 2, rows: 3 }]);
        assert_eq!(p.sides, Sides::OneSided);
        // The first tile shows the top left of the poster: the poster's
        // top left corner (0, 2 x 841.89) at the top left of the area,
        // moved by the centring.
        let s = &p.sheets[0];
        let (aw, ah) = s.layout_size();
        let (sx, sy) = (aw - 20.0, ah - 20.0);
        let (pw, ph) = (A4.0 * 2.0, A4.1 * 2.0);
        let ox = (2.0 * sx + 20.0 - pw) / 2.0;
        let oy = (3.0 * sy + 20.0 - ph) / 2.0;
        let (x, y) = apply(s.placements[0].m, 0.0, A4.1);
        assert!((x - ox).abs() < 0.05 && (y - (ah - oy)).abs() < 0.05, "{x} {y}");
        // Tile B2 (second row, second column) starts where B1 ends less the overlap.
        let b1 = apply(p.sheets[2].placements[0].m, 0.0, 0.0);
        let b2 = apply(p.sheets[3].placements[0].m, 0.0, 0.0);
        assert!(((b1.0 - b2.0) - sx).abs() < 0.05);
        // Marks toward the neighbours only: the last tile has none.
        let lines = |s: &Sheet| s.marks.iter().filter(|m| matches!(m, Mark::Line(_))).count();
        assert_eq!(lines(&p.sheets[0]), 4);
        assert_eq!(lines(&p.sheets[1]), 2);
        assert_eq!(lines(&p.sheets[5]), 0);
        let label = p.sheets[5].marks.iter().find_map(|m| match m {
            Mark::Label { text, .. } => Some(text.clone()),
            _ => None,
        });
        assert_eq!(label.as_deref(), Some("p1 C2 (6/6)"));
        assert_eq!(
            plan(&[0], &geoms, &poster(400.0)),
            Err(PlanError::OverlapTooLarge)
        );
    }

    #[test]
    fn poster_rows_past_z_and_the_sheet_cap() {
        assert_eq!(row_label(0), "A");
        assert_eq!(row_label(25), "Z");
        assert_eq!(row_label(26), "AA");
        assert_eq!(row_label(27), "AB");
        assert_eq!(row_label(701), "ZZ");
        assert_eq!(row_label(702), "AAA");
        // A tall page at 1000%: more than 26 rows, ASCII labels.
        let geoms = [geom(100.0, 5000.0)];
        let o = opts(Layout::Poster {
            scale_pct: 1000.0,
            overlap: 0.0,
            cut_marks: false,
            labels: true,
        });
        let p = plan(&[0], &geoms, &o).unwrap();
        assert!(p.sheets.len() > 26 * 2);
        for s in &p.sheets {
            for m in &s.marks {
                if let Mark::Label { text, .. } = m {
                    assert!(
                        text.chars()
                            .all(|c| c.is_ascii_alphanumeric() || " ()/".contains(c)),
                        "{text}"
                    );
                }
            }
        }
        let many: Vec<usize> = vec![0; MAX_SIDES + 1];
        let p = plan(&many, &[geom(A4.0, A4.1)], &opts(Layout::default()));
        assert_eq!(p, Err(PlanError::TooManySheets));
    }

    #[test]
    fn reverse_order_and_two_sided_pairs() {
        let geoms = vec![geom(A4.0, A4.1); 5];
        let pages: Vec<usize> = (0..5).collect();
        let mut o = opts(Layout::default());
        o.reverse = true;
        let p = plan(&pages, &geoms, &o).unwrap();
        let order: Vec<usize> = p.sheets.iter().map(|s| s.placements[0].page).collect();
        assert_eq!(order, [4, 3, 2, 1, 0]);
        assert_eq!(p.physical_sheets, 5);
        o.sides = Sides::LongEdge;
        let p = plan(&pages, &geoms, &o).unwrap();
        let order: Vec<Option<usize>> = p
            .sheets
            .iter()
            .map(|s| s.placements.first().map(|pl| pl.page))
            .collect();
        assert_eq!(order, [Some(4), None, Some(2), Some(3), Some(0), Some(1)]);
        assert_eq!(p.physical_sheets, 3);
        assert_eq!(p.sheets[1].face, Face::Back);
        assert_eq!(p.sheets[2].physical, 1);
        o.reverse = false;
        o.subset = Subset::Odd;
        let p = plan(&pages, &geoms, &o).unwrap();
        assert_eq!(p.pages, [0, 2, 4]);
        o.subset = Subset::Even;
        assert_eq!(plan(&pages, &geoms, &o).unwrap().pages, [1, 3]);
        assert_eq!(plan(&[], &geoms, &o), Err(PlanError::NoPages));
        // Repeated pages print twice.
        o.subset = Subset::All;
        o.sides = Sides::OneSided;
        assert_eq!(plan(&[0, 1, 2, 1], &geoms, &o).unwrap().sheets.len(), 4);
    }

    #[test]
    fn margins_that_leave_no_room_are_an_error() {
        let mut o = opts(Layout::default());
        o.paper.margins = [300.0, 0.0, 300.0, 0.0];
        assert_eq!(
            plan(&[0], &[geom(A4.0, A4.1)], &o),
            Err(PlanError::NoPrintableArea)
        );
    }
}
