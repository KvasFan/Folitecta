//! Barcodes: drawn on a page as vector shapes, and found on the pages.
//!
//! rxing (Apache-2.0, the Rust port of ZXing) writes the modules of a
//! barcode and reads the barcodes in the pixels of rendered pages. Pages are
//! searched side by side with rayon.

use std::sync::Arc;

use image::{DynamicImage, GrayImage};
use lopdf::{Object, Stream, dictionary};
use rayon::prelude::*;
use rxing::common::HybridBinarizer;
use rxing::{
    BarcodeFormat, BinaryBitmap, DecodeHints, EncodeHints, Luma8LuminanceSource, MultiFormatReader,
    MultiFormatWriter, RXingResult, Reader, Writer,
};

use crate::document::Document;
use crate::error::{Error, Result};
use crate::images::display_axes;
use crate::marks::display_to_pdf;
use crate::render::{RenderOpts, Renderer};
use crate::textops::{covered_by, display_rect_to_pdf};
use crate::util::{fmt_num, helvetica_width, pdf_literal};

/// Kinds of barcode that can be drawn (every kind rxing reads is found).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BarcodeKind {
    Qr,
    DataMatrix,
    Pdf417,
    Aztec,
    Code128,
    Code39,
    Code93,
    Ean13,
    Ean8,
    UpcA,
    UpcE,
    Itf,
    Codabar,
}

impl BarcodeKind {
    pub const ALL: [BarcodeKind; 13] = [
        BarcodeKind::Qr,
        BarcodeKind::DataMatrix,
        BarcodeKind::Pdf417,
        BarcodeKind::Aztec,
        BarcodeKind::Code128,
        BarcodeKind::Code39,
        BarcodeKind::Code93,
        BarcodeKind::Ean13,
        BarcodeKind::Ean8,
        BarcodeKind::UpcA,
        BarcodeKind::UpcE,
        BarcodeKind::Itf,
        BarcodeKind::Codabar,
    ];

    pub fn id(self) -> &'static str {
        match self {
            BarcodeKind::Qr => "qr",
            BarcodeKind::DataMatrix => "datamatrix",
            BarcodeKind::Pdf417 => "pdf417",
            BarcodeKind::Aztec => "aztec",
            BarcodeKind::Code128 => "code128",
            BarcodeKind::Code39 => "code39",
            BarcodeKind::Code93 => "code93",
            BarcodeKind::Ean13 => "ean13",
            BarcodeKind::Ean8 => "ean8",
            BarcodeKind::UpcA => "upca",
            BarcodeKind::UpcE => "upce",
            BarcodeKind::Itf => "itf",
            BarcodeKind::Codabar => "codabar",
        }
    }

    pub fn from_id(id: &str) -> Option<BarcodeKind> {
        BarcodeKind::ALL.into_iter().find(|k| k.id() == id)
    }

    /// The name of the kind (the same in every language).
    pub fn label(self) -> &'static str {
        match self {
            BarcodeKind::Qr => "QR Code",
            BarcodeKind::DataMatrix => "Data Matrix",
            BarcodeKind::Pdf417 => "PDF417",
            BarcodeKind::Aztec => "Aztec",
            BarcodeKind::Code128 => "Code 128",
            BarcodeKind::Code39 => "Code 39",
            BarcodeKind::Code93 => "Code 93",
            BarcodeKind::Ean13 => "EAN-13",
            BarcodeKind::Ean8 => "EAN-8",
            BarcodeKind::UpcA => "UPC-A",
            BarcodeKind::UpcE => "UPC-E",
            BarcodeKind::Itf => "ITF",
            BarcodeKind::Codabar => "Codabar",
        }
    }

    /// Bars in one row; the other kinds are grids of square modules.
    pub fn is_linear(self) -> bool {
        !matches!(
            self,
            BarcodeKind::Qr | BarcodeKind::DataMatrix | BarcodeKind::Pdf417 | BarcodeKind::Aztec
        )
    }

    fn format(self) -> BarcodeFormat {
        match self {
            BarcodeKind::Qr => BarcodeFormat::QR_CODE,
            BarcodeKind::DataMatrix => BarcodeFormat::DATA_MATRIX,
            BarcodeKind::Pdf417 => BarcodeFormat::PDF_417,
            BarcodeKind::Aztec => BarcodeFormat::AZTEC,
            BarcodeKind::Code128 => BarcodeFormat::CODE_128,
            BarcodeKind::Code39 => BarcodeFormat::CODE_39,
            BarcodeKind::Code93 => BarcodeFormat::CODE_93,
            BarcodeKind::Ean13 => BarcodeFormat::EAN_13,
            BarcodeKind::Ean8 => BarcodeFormat::EAN_8,
            BarcodeKind::UpcA => BarcodeFormat::UPC_A,
            BarcodeKind::UpcE => BarcodeFormat::UPC_E,
            BarcodeKind::Itf => BarcodeFormat::ITF,
            BarcodeKind::Codabar => BarcodeFormat::CODABAR,
        }
    }

    fn from_format(f: &BarcodeFormat) -> Option<BarcodeKind> {
        BarcodeKind::ALL.into_iter().find(|k| k.format() == *f)
    }

    /// Light modules kept around the code so scanners find where it starts.
    fn quiet_zone(self) -> u32 {
        match self {
            BarcodeKind::Qr | BarcodeKind::Pdf417 => 2,
            BarcodeKind::DataMatrix | BarcodeKind::Aztec => 1,
            _ => 8,
        }
    }
}

/// The modules of a barcode, row by row from the top; a linear barcode has
/// one row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Modules {
    pub columns: u32,
    pub rows: u32,
    dark: Vec<bool>,
}

impl Modules {
    pub fn is_dark(&self, x: u32, y: u32) -> bool {
        x < self.columns && y < self.rows && self.dark[(y * self.columns + x) as usize]
    }

    /// Runs of dark modules of row `y`: (first column, length).
    fn runs(&self, y: u32) -> Vec<(u32, u32)> {
        let mut out = Vec::new();
        let mut x = 0;
        while x < self.columns {
            if self.is_dark(x, y) {
                let start = x;
                while x < self.columns && self.is_dark(x, y) {
                    x += 1;
                }
                out.push((start, x - start));
            } else {
                x += 1;
            }
        }
        out
    }
}

/// The modules of `text` written as a barcode of `kind`.
pub fn encode(kind: BarcodeKind, text: &str) -> Result<Modules> {
    if text.is_empty() {
        return Err(Error::Invalid("the barcode has no content".into()));
    }
    let hints = EncodeHints {
        Margin: Some("0".into()),
        CharacterSet: (!text.is_ascii()).then(|| "UTF-8".into()),
        ErrorCorrection: (kind == BarcodeKind::Qr).then(|| "M".into()),
        ..Default::default()
    };
    let m = MultiFormatWriter
        .encode_with_hints(text, &kind.format(), 0, 0, &hints)
        .map_err(|e| Error::Invalid(format!("a {} cannot hold this content ({e})", kind.label())))?;
    let (columns, height) = (m.width(), m.height());
    if columns == 0 || height == 0 {
        return Err(Error::Invalid(format!(
            "a {} cannot hold this content",
            kind.label()
        )));
    }
    // A linear barcode comes as one row repeated: its first row is enough.
    let rows = if kind.is_linear() { 1 } else { height };
    let dark = (0..rows)
        .flat_map(|y| (0..columns).map(move |x| (x, y)))
        .map(|(x, y)| m.get(x, y))
        .collect();
    Ok(Modules { columns, rows, dark })
}

/// Content stream drawing a barcode in a box of `w` × `h` points (origin
/// bottom-left): a white background, the modules in black and, for a linear
/// barcode, its text below the bars.
fn barcode_content(kind: BarcodeKind, m: &Modules, text: &str, w: f32, h: f32, show_text: bool) -> String {
    let f = fmt_num;
    let q = kind.quiet_zone() as f32;
    let mut s = format!("q 1 g 0 0 {} {} re f 0 g\n", f(w), f(h));
    if kind.is_linear() {
        let font = (h * 0.16).clamp(5.0, 12.0);
        let caption = show_text && text.is_ascii();
        let text_h = if caption { font * 1.3 } else { 0.0 };
        let unit = w / (m.columns as f32 + 2.0 * q);
        let (y0, bar_h) = (text_h + h * 0.04, h * 0.92 - text_h);
        for (x, n) in m.runs(0) {
            s.push_str(&format!(
                "{} {} {} {} re\n",
                f((q + x as f32) * unit),
                f(y0),
                f(n as f32 * unit),
                f(bar_h.max(1.0))
            ));
        }
        s.push_str("f\n");
        if caption {
            let tw = helvetica_width(text, font);
            let lit = String::from_utf8_lossy(&pdf_literal(text)).into_owned();
            s.push_str(&format!(
                "BT /FoliHelv {} Tf {} {} Td {} Tj ET\n",
                f(font),
                f(((w - tw) / 2.0).max(0.0)),
                f(font * 0.3),
                lit
            ));
        }
    } else {
        let (ux, uy) = (m.columns as f32 + 2.0 * q, m.rows as f32 + 2.0 * q);
        let unit = (w / ux).min(h / uy);
        let (ox, oy) = ((w - ux * unit) / 2.0 + q * unit, (h - uy * unit) / 2.0 + q * unit);
        for y in 0..m.rows {
            // Row 0 is the top row: PDF space goes up.
            let py = oy + (m.rows - 1 - y) as f32 * unit;
            for (x, n) in m.runs(y) {
                s.push_str(&format!(
                    "{} {} {} {} re\n",
                    f(ox + x as f32 * unit),
                    f(py),
                    f(n as f32 * unit),
                    f(unit)
                ));
            }
        }
        s.push_str("f\n");
    }
    s.push_str("Q\n");
    s
}

/// A barcode found on a page.
#[derive(Debug, Clone, PartialEq)]
pub struct FoundBarcode {
    pub page: usize,
    /// The name of its kind ("QR Code", "EAN-13", ...).
    pub kind: String,
    pub text: String,
    /// Where it is: display space of the page, points, y down.
    pub rect: [f32; 4],
}

impl FoundBarcode {
    /// The web address the barcode holds, if it holds one.
    pub fn web_address(&self) -> Option<String> {
        let t = self.text.trim();
        let lower = t.to_ascii_lowercase();
        if t.contains(char::is_whitespace) {
            return None;
        }
        if lower.starts_with("http://") || lower.starts_with("https://") || lower.starts_with("mailto:") {
            Some(t.to_string())
        } else if lower.starts_with("www.") && t.len() > 5 {
            Some(format!("http://{t}"))
        } else {
            None
        }
    }
}

/// Pixels per point used to read the barcodes of a page (180 dpi), and of a
/// region (288 dpi: it is small, and its codes may be too).
const PAGE_SCALE: f32 = 2.5;
const REGION_SCALE: f32 = 4.0;

/// The barcodes of the given pages of a document (its bytes), or of a region
/// of one page (display space, points, y down). Pages are read side by side.
pub fn find_barcodes(
    bytes: Arc<Vec<u8>>,
    pages: &[usize],
    region: Option<[f32; 4]>,
) -> Result<Vec<FoundBarcode>> {
    let found: Vec<Vec<FoundBarcode>> = pages
        .par_iter()
        .map(|&page| find_on_page(bytes.clone(), page, region))
        .collect::<Result<_>>()?;
    Ok(found.into_iter().flatten().collect())
}

/// Codes read on one page at most.
const MAX_CODES: usize = 32;

/// One code read in `luma` (the first the readers find), if any.
fn read_one(luma: &GrayImage, hints: &DecodeHints) -> Option<RXingResult> {
    let (w, h) = luma.dimensions();
    let source = Luma8LuminanceSource::new(luma.as_raw().clone(), w, h).ok()?;
    let mut bitmap = BinaryBitmap::new(HybridBinarizer::new(source));
    MultiFormatReader::default()
        .decode_with_hints(&mut bitmap, hints)
        .ok()
}

fn is_dark(luma: &GrayImage, x: u32, y: u32) -> bool {
    luma.get_pixel(x, y)[0] < 128
}

/// The box (pixels, `[x0, y0, x1, y1)`) of the bars of a linear barcode read
/// on row `y` between `xa` and `xb`: from the bar nearest the middle, the
/// bars go on while the light gaps between them stay narrower than six of
/// its thinnest bars, and the rows above and below belong to the code while
/// they are dark at the same places.
fn linear_box(luma: &GrayImage, xa: f32, xb: f32, y: f32) -> Option<[u32; 4]> {
    let (w, h) = luma.dimensions();
    let clamp_x = |v: f32| v.round().clamp(0.0, (w - 1) as f32) as u32;
    let x = clamp_x((xa + xb) / 2.0);
    let reach = (xa - xb).abs() as u32 / 2 + 12;
    let y = y.round().clamp(0.0, (h - 1) as f32) as u32;
    let mut runs: Vec<(u32, u32)> = Vec::new();
    let mut i = 0;
    while i < w {
        if is_dark(luma, i, y) {
            let s = i;
            while i < w && is_dark(luma, i, y) {
                i += 1;
            }
            runs.push((s, i));
        } else {
            i += 1;
        }
    }
    let distance = |r: &(u32, u32)| {
        if x < r.0 { r.0 - x } else { x.saturating_sub(r.1) }
    };
    let near = (0..runs.len()).min_by_key(|i| distance(&runs[*i]))?;
    if distance(&runs[near]) > reach {
        return None;
    }
    let around = &runs[near.saturating_sub(4)..(near + 5).min(runs.len())];
    let module = around.iter().map(|r| r.1 - r.0).min().unwrap_or(1).max(1);
    let gap = 6 * module + 1;
    let (mut lo, mut hi) = (near, near);
    while lo > 0 && runs[lo].0 - runs[lo - 1].1 <= gap {
        lo -= 1;
    }
    while hi + 1 < runs.len() && runs[hi + 1].0 - runs[hi].1 <= gap {
        hi += 1;
    }
    let (x0, x1) = (runs[lo].0, runs[hi].1);
    let pattern: Vec<bool> = (x0..x1).map(|i| is_dark(luma, i, y)).collect();
    let same = |row: u32| {
        let differ = (x0..x1)
            .zip(&pattern)
            .filter(|(i, d)| is_dark(luma, *i, row) != **d)
            .count();
        differ * 10 <= pattern.len()
    };
    let (mut y0, mut y1) = (y, y);
    while y0 > 0 && same(y0 - 1) {
        y0 -= 1;
    }
    while y1 + 1 < h && same(y1 + 1) {
        y1 += 1;
    }
    Some([x0, y0, x1, y1 + 1])
}

/// The box of a code from points inside it, grown while the line just
/// outside a side still crosses dark pixels (up to a quarter of its size), so
/// it reaches the light margin around the code.
fn grown_box(luma: &GrayImage, points: &[[f32; 2]]) -> [u32; 4] {
    let (w, h) = luma.dimensions();
    let fold = |i: usize| {
        points
            .iter()
            .fold((f32::MAX, f32::MIN), |(a, b), p| (a.min(p[i]), b.max(p[i])))
    };
    let ((ax, bx), (ay, by)) = (fold(0), fold(1));
    let mut b = [
        ax.floor().max(0.0) as u32,
        ay.floor().max(0.0) as u32,
        (bx.ceil() as u32 + 1).min(w),
        (by.ceil() as u32 + 1).min(h),
    ];
    let limit = ((b[2] - b[0]).max(b[3] - b[1]) / 4 + 3) as i64;
    let mut grown = [0i64; 4];
    loop {
        let mut changed = false;
        let column_dark = |x: u32, b: &[u32; 4]| (b[1]..b[3]).any(|y| is_dark(luma, x, y));
        let row_dark = |y: u32, b: &[u32; 4]| (b[0]..b[2]).any(|x| is_dark(luma, x, y));
        if b[0] > 0 && grown[0] < limit && column_dark(b[0] - 1, &b) {
            b[0] -= 1;
            grown[0] += 1;
            changed = true;
        }
        if b[2] < w && grown[2] < limit && column_dark(b[2], &b) {
            b[2] += 1;
            grown[2] += 1;
            changed = true;
        }
        if b[1] > 0 && grown[1] < limit && row_dark(b[1] - 1, &b) {
            b[1] -= 1;
            grown[1] += 1;
            changed = true;
        }
        if b[3] < h && grown[3] < limit && row_dark(b[3], &b) {
            b[3] += 1;
            grown[3] += 1;
            changed = true;
        }
        if !changed {
            return b;
        }
    }
}

/// What a read turned out to be (pixels of `luma`).
enum Spot {
    /// A code, in this box.
    Code([u32; 4]),
    /// Something read as a linear code that is not one (a line of text, a
    /// table): this box is left out of the next reads.
    Noise([u32; 4]),
}

/// Where a code that was read is. A linear code must have bars along the
/// row it was read on, at least an eighth as tall as they are wide: lines
/// of text read as codes do not.
fn locate(luma: &GrayImage, r: &RXingResult, dx: f32, dy: f32) -> Option<Spot> {
    let points: Vec<[f32; 2]> = r.getPoints().iter().map(|p| [p.x + dx, p.y + dy]).collect();
    let first = *points.first()?;
    let linear = BarcodeKind::from_format(r.getBarcodeFormat()).is_some_and(|k| k.is_linear());
    let flat = points.iter().all(|p| (p[1] - first[1]).abs() < 2.0);
    if !(linear && flat) {
        return Some(Spot::Code(grown_box(luma, &points)));
    }
    let (xa, xb) = points
        .iter()
        .fold((f32::MAX, f32::MIN), |(a, b), p| (a.min(p[0]), b.max(p[0])));
    match linear_box(luma, xa, xb, first[1]) {
        Some(b) if (b[3] - b[1]) >= 12 && (b[3] - b[1]) * 8 >= b[2] - b[0] => Some(Spot::Code(b)),
        found => {
            let (w, h) = luma.dimensions();
            let y = first[1].max(0.0) as u32;
            let row = [
                xa.max(0.0) as u32,
                y.saturating_sub(6),
                (xb as u32 + 1).min(w),
                (y + 7).min(h),
            ];
            // The whole line it was read on, or the rows its "bars" span.
            let b = found.map_or(row, |b| {
                [
                    b[0].min(row[0]),
                    b[1].min(row[1]),
                    b[2].max(row[2]),
                    b[3].max(row[3]),
                ]
            });
            Some(Spot::Noise(b))
        }
    }
}

/// Runs of a line of pixels: (first pixel, length, dark).
fn runs_of(len: u32, dark: impl Fn(u32) -> bool) -> Vec<(u32, u32, bool)> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < len {
        let d = dark(i);
        let s = i;
        while i < len && dark(i) == d {
            i += 1;
        }
        out.push((s, i - s, d));
    }
    out
}

/// The module size when nine runs from `runs[i]`, the first dark, cross the
/// rings of a bullseye: the seven inside have about the same length, the two
/// outside may run on into the modules around the bullseye.
fn even_rings(runs: &[(u32, u32, bool)], i: usize) -> Option<f32> {
    let ring = runs.get(i..i + 9)?;
    if !ring[0].2 {
        return None;
    }
    let inner = &ring[1..8];
    let avg = inner.iter().map(|r| r.1 as f32).sum::<f32>() / 7.0;
    let even = inner.iter().all(|r| (r.1 as f32 - avg).abs() <= avg * 0.4);
    let outer = [ring[0], ring[8]].iter().all(|r| r.1 as f32 >= avg * 0.6);
    (avg >= 2.0 && even && outer).then_some(avg)
}

/// Centres (pixels) and module sizes of the bullseyes of Aztec codes: a row
/// where nine runs of about the same length alternate dark and light, and the
/// column through the middle one does the same around the same pixel.
fn bullseyes(luma: &GrayImage) -> Vec<(u32, u32, f32)> {
    let (w, h) = luma.dimensions();
    let mut out: Vec<(u32, u32, f32)> = Vec::new();
    for y in (0..h).step_by(2) {
        let row = runs_of(w, |x| is_dark(luma, x, y));
        for i in 0..row.len() {
            let Some(module) = even_rings(&row, i) else {
                continue;
            };
            let mid = row[i + 4];
            let x = mid.0 + mid.1 / 2;
            if out
                .iter()
                .any(|c| c.0.abs_diff(x) < (module * 4.0) as u32 && c.1.abs_diff(y) < (module * 4.0) as u32)
            {
                continue;
            }
            let column = runs_of(h, |v| is_dark(luma, x, v));
            let Some(k) = column.iter().position(|r| r.0 <= y && y < r.0 + r.1) else {
                continue;
            };
            if k >= 4 && even_rings(&column, k - 4).is_some_and(|m| (m - module).abs() <= module * 0.5) {
                out.push((x, y, module));
            }
        }
    }
    out
}

/// Pixels of a cell of the grid that finds the dark areas of a page.
const CELL: u32 = 6;

/// Boxes (pixels) of the dark areas of `luma`: cells of a grid that hold a
/// dark pixel, joined when they touch or are one cell apart.
fn dark_areas(luma: &GrayImage) -> Vec<[u32; 4]> {
    let (w, h) = luma.dimensions();
    let (cw, ch) = (w.div_ceil(CELL) as usize, h.div_ceil(CELL) as usize);
    let mut dark = vec![false; cw * ch];
    for (x, y, p) in luma.enumerate_pixels() {
        if p[0] < 128 {
            dark[(y / CELL) as usize * cw + (x / CELL) as usize] = true;
        }
    }
    // One cell of light between two dark cells still joins them.
    let joined: Vec<bool> = (0..cw * ch)
        .map(|i| {
            let (cx, cy) = ((i % cw) as i64, (i / cw) as i64);
            (-1..=1).any(|dy| {
                (-1..=1).any(|dx| {
                    let (x, y) = (cx + dx, cy + dy);
                    x >= 0
                        && y >= 0
                        && (x as usize) < cw
                        && (y as usize) < ch
                        && dark[y as usize * cw + x as usize]
                })
            })
        })
        .collect();
    let mut seen = vec![false; cw * ch];
    let mut out = Vec::new();
    for start in 0..cw * ch {
        if !joined[start] || seen[start] {
            continue;
        }
        let mut stack = vec![start];
        seen[start] = true;
        let (mut x0, mut y0, mut x1, mut y1) = (usize::MAX, usize::MAX, 0, 0);
        while let Some(i) = stack.pop() {
            let (cx, cy) = (i % cw, i / cw);
            (x0, y0, x1, y1) = (x0.min(cx), y0.min(cy), x1.max(cx), y1.max(cy));
            let mut visit = |j: usize| {
                if joined[j] && !seen[j] {
                    seen[j] = true;
                    stack.push(j);
                }
            };
            if cx > 0 {
                visit(i - 1);
            }
            if cx + 1 < cw {
                visit(i + 1);
            }
            if cy > 0 {
                visit(i - cw);
            }
            if cy + 1 < ch {
                visit(i + cw);
            }
        }
        // The joining grew the area by a cell on every side.
        let px = |c: usize, n: u32| ((c as u32) * CELL).min(n);
        out.push([px(x0 + 1, w), px(y0 + 1, h), px(x1, w), px(y1, h)]);
    }
    out
}

fn overlap(a: [u32; 4], b: [u32; 4]) -> bool {
    a[0] < b[2] && b[0] < a[2] && a[1] < b[3] && b[1] < a[3]
}

fn erase(luma: &mut GrayImage, b: [u32; 4]) {
    let (w, h) = luma.dimensions();
    for y in b[1].saturating_sub(2)..(b[3] + 2).min(h) {
        for x in b[0].saturating_sub(2)..(b[2] + 2).min(w) {
            luma.put_pixel(x, y, image::Luma([255]));
        }
    }
}

fn find_on_page(bytes: Arc<Vec<u8>>, page: usize, region: Option<[f32; 4]>) -> Result<Vec<FoundBarcode>> {
    let scale = if region.is_some() {
        REGION_SCALE
    } else {
        PAGE_SCALE
    };
    let renderer = Renderer::new(bytes)?;
    let rgba = renderer.render_rgba(page, scale, RenderOpts::default())?;
    let (iw, ih) = rgba.dimensions();
    let mut img = DynamicImage::ImageRgba8(rgba);
    // The rendered page is the page as shown: a pixel over `scale` is a point
    // of display space.
    let (ox, oy) = match region {
        Some(r) => {
            let x0 = ((r[0].min(r[2]) * scale).floor().max(0.0) as u32).min(iw.saturating_sub(1));
            let y0 = ((r[1].min(r[3]) * scale).floor().max(0.0) as u32).min(ih.saturating_sub(1));
            let x1 = ((r[0].max(r[2]) * scale).ceil() as u32).clamp(x0 + 1, iw);
            let y1 = ((r[1].max(r[3]) * scale).ceil() as u32).clamp(y0 + 1, ih);
            img = img.crop_imm(x0, y0, x1 - x0, y1 - y0);
            (x0, y0)
        }
        None => (0, 0),
    };
    let mut luma = img.to_luma8();
    let (w, h) = luma.dimensions();
    let mut found: Vec<(RXingResult, [u32; 4])> = Vec::new();
    // One code at a time: read it, find where it is, paint it white, read
    // again. (The points of rxing's own multiple reader can be wrong for
    // linear codes: they are not used.)
    let all = DecodeHints {
        TryHarder: Some(true),
        ..Default::default()
    };
    // Painted white either way: a code is not read twice, noise not again.
    let mut note = |luma: &mut GrayImage, r: RXingResult, spot: Spot| {
        let b = match spot {
            Spot::Code(b) => b,
            Spot::Noise(b) => return erase(luma, b),
        };
        erase(luma, b);
        // The same code read again where it was already found.
        let again = found.iter_mut().find(|(o, ob)| {
            o.getText() == r.getText() && o.getBarcodeFormat() == r.getBarcodeFormat() && overlap(*ob, b)
        });
        match again {
            Some((_, ob)) => *ob = [ob[0].min(b[0]), ob[1].min(b[1]), ob[2].max(b[2]), ob[3].max(b[3])],
            None => found.push((r, b)),
        }
    };
    for _ in 0..MAX_CODES {
        let Some(r) = read_one(&luma, &all) else { break };
        let Some(spot) = locate(&luma, &r, 0.0, 0.0) else {
            break;
        };
        note(&mut luma, r, spot);
    }
    // Aztec codes, Data Matrix codes and (on busy pages) QR codes are looked
    // for from the middle of the picture: each bullseye of an Aztec code, and
    // each dark area of the page shaped like a square code, is read alone.
    let only = |formats: &[BarcodeFormat]| DecodeHints {
        TryHarder: Some(true),
        PossibleFormats: Some(formats.iter().cloned().collect()),
        ..Default::default()
    };
    let aztec = only(&[BarcodeFormat::AZTEC]);
    for (cx, cy, module) in bullseyes(&luma) {
        // Centred on the bullseye (twenty modules each way: the reader needs
        // about twelve), white where it goes past the page.
        let half = (module * 20.0) as i64;
        let (x0, y0) = (cx as i64 - half, cy as i64 - half);
        let mut part = GrayImage::from_pixel(2 * half as u32, 2 * half as u32, image::Luma([255]));
        image::imageops::replace(&mut part, &luma, -x0, -y0);
        if let Some(r) = read_one(&part, &aztec) {
            if let Some(spot) = locate(&luma, &r, x0 as f32, y0 as f32) {
                note(&mut luma, r, spot);
            }
        }
    }
    let centred = only(&[BarcodeFormat::DATA_MATRIX, BarcodeFormat::QR_CODE]);
    for a in dark_areas(&luma) {
        let (aw, ah) = (a[2].saturating_sub(a[0]), a[3].saturating_sub(a[1]));
        let squarish = aw >= 24 && ah >= 24 && aw * 5 >= ah && ah * 5 >= aw;
        if !squarish || aw > w * 3 / 4 || ah > h * 3 / 4 {
            continue;
        }
        let m = aw.max(ah) / 8 + 8;
        let (x0, y0) = (a[0].saturating_sub(m), a[1].saturating_sub(m));
        let (x1, y1) = ((a[2] + m).min(w), (a[3] + m).min(h));
        let part = image::imageops::crop_imm(&luma, x0, y0, x1 - x0, y1 - y0).to_image();
        if let Some(r) = read_one(&part, &centred) {
            if let Some(spot) = locate(&luma, &r, x0 as f32, y0 as f32) {
                note(&mut luma, r, spot);
            }
        }
    }
    Ok(found
        .into_iter()
        .map(|(r, b)| {
            let format = r.getBarcodeFormat();
            let kind = BarcodeKind::from_format(format)
                .map(|k| k.label().to_string())
                .unwrap_or_else(|| format!("{format:?}"));
            let p = |v: u32, o: u32| (v + o) as f32 / scale;
            FoundBarcode {
                page,
                kind,
                text: r.getText().to_string(),
                rect: [p(b[0], ox), p(b[1], oy), p(b[2], ox), p(b[3], oy)],
            }
        })
        .collect())
}

impl Document {
    /// Draw a barcode of `kind` holding `text` in a rectangle of a page
    /// (display space, points, y down), over what the page shows. The text
    /// of a linear barcode is written below its bars when `show_text`.
    pub fn add_barcode(
        &mut self,
        page: usize,
        rect: [f32; 4],
        kind: BarcodeKind,
        text: &str,
        show_text: bool,
    ) -> Result<()> {
        let modules = encode(kind, text)?;
        let page_id = self.page_id(page)?;
        let (w, h) = ((rect[2] - rect[0]).abs(), (rect[3] - rect[1]).abs());
        if w < 4.0 || h < 4.0 {
            return Err(Error::Invalid("the barcode is too small".into()));
        }
        self.begin_edit();
        self.inherit_attributes(&[page_id]);
        let size = self.page_size_of(page_id);
        let font = self.helvetica_font_id();
        let content = barcode_content(kind, &modules, text, w, h, show_text);
        let mut form = Stream::new(
            dictionary! {
                "Type" => "XObject",
                "Subtype" => "Form",
                "BBox" => vec![0.into(), 0.into(), w.into(), h.into()],
                "Resources" => dictionary! { "Font" => dictionary! { "FoliHelv" => font } },
                // What the barcode holds, to read it back without scanning.
                "FolitectaBarcode" => dictionary! {
                    "Kind" => Object::Name(kind.id().as_bytes().to_vec()),
                    "Text" => Object::string_literal(text.to_string()),
                },
            },
            content.into_bytes(),
        );
        form.compress()?;
        let xid = self.inner.add_object(form);
        let name = format!("FoliCode{}", xid.0);
        self.inner.add_xobject(page_id, name.as_str(), xid)?;
        // The bottom-left corner of the box as shown, and the axes of the
        // page as shown, in PDF space.
        let (u0, v0) = (rect[0].min(rect[2]), size.height - rect[1].max(rect[3]));
        let (du, dv) = display_axes(size.rotation);
        let (ex, ey) = display_to_pdf(&size, u0, v0);
        let f = fmt_num;
        let place = format!(
            "q {} {} {} {} {} {} cm /{} Do Q\n",
            f(du.0),
            f(du.1),
            f(dv.0),
            f(dv.1),
            f(ex),
            f(ey),
            name
        );
        let place = self.drawn_content(page_id, place.into_bytes())?;
        self.append_page_content(page_id, place, Some("Barcode"))?;
        self.commit()
    }

    /// Web links over the barcodes of the given pages that hold a web
    /// address (barcodes already under a link are left), in one undo step.
    pub fn create_links_from_barcodes(&mut self, pages: &[usize]) -> Result<usize> {
        let found = find_barcodes(self.bytes(), pages, None)?;
        self.create_links_over(&found)
    }

    /// Web links over the barcodes found (by [`find_barcodes`]) that hold a
    /// web address and are not under a link yet, in one undo step.
    pub fn create_links_over(&mut self, found: &[FoundBarcode]) -> Result<usize> {
        let mut links = Vec::new();
        for f in found {
            let Some(uri) = f.web_address() else { continue };
            let size = self.page_size(f.page)?;
            let existing: Vec<[f32; 4]> = self.links(f.page)?.into_iter().map(|l| l.rect).collect();
            let rect = display_rect_to_pdf(&size, f.rect);
            if !covered_by(&existing, rect) {
                links.push((f.page, rect, uri));
            }
        }
        self.push_uri_links(&links)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_kind_writes_its_modules() {
        let sample = |k: BarcodeKind| match k {
            BarcodeKind::Ean13 => "590123412345",
            BarcodeKind::Ean8 => "9638507",
            BarcodeKind::UpcA => "03600029145",
            BarcodeKind::UpcE => "0123456",
            BarcodeKind::Itf => "12345678",
            BarcodeKind::Codabar => "A40156B",
            BarcodeKind::Code39 | BarcodeKind::Code93 => "FOLITECTA-1",
            _ => "https://github.com/KvasFan/Folitecta",
        };
        for k in BarcodeKind::ALL {
            let m = encode(k, sample(k)).unwrap_or_else(|e| panic!("{}: {e}", k.label()));
            assert!(m.columns > 10, "{} {}x{}", k.label(), m.columns, m.rows);
            assert_eq!(m.rows == 1, k.is_linear(), "{}", k.label());
            assert!((0..m.rows).any(|y| !m.runs(y).is_empty()));
            assert_eq!(BarcodeKind::from_id(k.id()), Some(k));
        }
        assert!(encode(BarcodeKind::Ean13, "letters").is_err());
        assert!(encode(BarcodeKind::Qr, "").is_err());
    }

    #[test]
    fn runs_join_dark_modules() {
        let m = Modules {
            columns: 6,
            rows: 1,
            dark: vec![true, true, false, true, false, true],
        };
        assert_eq!(m.runs(0), vec![(0, 2), (3, 1), (5, 1)]);
    }

    #[test]
    fn web_addresses() {
        let f = |t: &str| FoundBarcode {
            page: 0,
            kind: "QR Code".into(),
            text: t.into(),
            rect: [0.0; 4],
        };
        assert_eq!(
            f("https://example.org/a").web_address().as_deref(),
            Some("https://example.org/a")
        );
        assert_eq!(
            f("www.example.org").web_address().as_deref(),
            Some("http://www.example.org")
        );
        assert!(f("mailto:someone@example.org").web_address().is_some());
        assert_eq!(f("4006381333931").web_address(), None);
        assert_eq!(f("see https://example.org").web_address(), None);
    }
}
