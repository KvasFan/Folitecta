//! Editing the pixels of a picture: turning, mirroring, cropping,
//! straightening, pixel formats and filters, keeping track of where the
//! edited picture goes on the page.

use hayro::kurbo::{Affine, Vec2};
use image::{Rgba, RgbaImage, imageops};

use crate::pictures::Colors;

/// Pixel formats a picture can be converted to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// One bit per pixel: black and white.
    Bilevel,
    /// 256 colours from a palette.
    Indexed,
    /// 256 shades of gray.
    Gray,
    /// Shades of gray with transparency.
    GrayAlpha,
    /// Colours, 24 bits per pixel.
    Rgb,
    /// Colours with transparency, 32 bits per pixel.
    Rgba,
}

/// 3 × 3 convolution filters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Filter {
    Smooth,
    GaussianBlur,
    MeanRemoval,
    Sharpen,
    EmbossLaplacian,
    EmbossHorzVert,
    EmbossAllDirections,
    EmbossLossy,
    EdgeDetect,
}

impl Filter {
    /// Weights, divisor and offset.
    fn kernel(self) -> ([i32; 9], i32, i32) {
        match self {
            Filter::Smooth => ([1, 1, 1, 1, 1, 1, 1, 1, 1], 9, 0),
            Filter::GaussianBlur => ([1, 2, 1, 2, 4, 2, 1, 2, 1], 16, 0),
            Filter::MeanRemoval => ([-1, -1, -1, -1, 9, -1, -1, -1, -1], 1, 0),
            Filter::Sharpen => ([0, -2, 0, -2, 11, -2, 0, -2, 0], 3, 0),
            Filter::EmbossLaplacian => ([-1, 0, -1, 0, 4, 0, -1, 0, -1], 1, 127),
            Filter::EmbossHorzVert => ([0, -1, 0, -1, 4, -1, 0, -1, 0], 1, 127),
            Filter::EmbossAllDirections => ([-1, -1, -1, -1, 8, -1, -1, -1, -1], 1, 127),
            Filter::EmbossLossy => ([1, -2, 1, -2, 4, -2, -2, 1, -2], 1, 127),
            Filter::EdgeDetect => ([1, 1, 1, 0, 0, 0, -1, -1, -1], 1, 127),
        }
    }
}

/// One edit of a picture.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Edit {
    /// A quarter turn clockwise.
    RotateRight,
    /// A quarter turn counterclockwise.
    RotateLeft,
    FlipHorizontal,
    FlipVertical,
    Grayscale,
    /// Keep the pixels `[x0, y0, x1, y1)` (origin at the top left).
    Crop([u32; 4]),
    /// Turn by degrees (counterclockwise as shown), keeping the size.
    Turn(f32),
    /// Lighter (positive) or darker (negative), -255 to 255.
    Brightness(i32),
    /// More (positive) or less (negative) contrast, -100 to 100.
    Contrast(f32),
    Invert,
    Sepia,
    /// Remove isolated specks (median of 3 × 3 pixels).
    Despeckle,
    Filter(Filter),
    Convert(Format),
}

/// A picture being edited.
#[derive(Debug, Clone, PartialEq)]
pub struct Work {
    pub pixels: RgbaImage,
    /// Where the unit square of `pixels` goes in the unit square of the
    /// picture on the page (what [`crate::Document::replace_page_image`] takes).
    pub placement: [f64; 6],
    /// Where the unit square of the picture on the page goes on the page.
    pub page_matrix: [f64; 6],
    pub colors: Colors,
    /// Transparency is kept.
    pub alpha: bool,
}

/// Scale by `sx`, `sy` around the centre of the unit square.
fn about_center(sx: f64, sy: f64) -> Affine {
    Affine::translate(Vec2::new(0.5, 0.5))
        * Affine::scale_non_uniform(sx, sy)
        * Affine::translate(Vec2::new(-0.5, -0.5))
}

impl Work {
    pub fn new(pixels: RgbaImage, page_matrix: [f64; 6], colors: Colors) -> Self {
        let alpha = pixels.pixels().any(|p| p[3] < 255);
        Work {
            pixels,
            placement: Affine::IDENTITY.as_coeffs(),
            page_matrix,
            colors,
            alpha,
        }
    }

    pub fn size(&self) -> (u32, u32) {
        self.pixels.dimensions()
    }

    pub fn format(&self) -> Format {
        match (self.colors, self.alpha) {
            (Colors::BlackWhite, _) => Format::Bilevel,
            (Colors::Indexed, _) => Format::Indexed,
            (Colors::Gray, false) => Format::Gray,
            (Colors::Gray, true) => Format::GrayAlpha,
            (Colors::Color, false) => Format::Rgb,
            (Colors::Color, true) => Format::Rgba,
        }
    }

    /// Lengths on the page of the bottom and left edges of the edited picture.
    pub fn sides(&self) -> (f64, f64) {
        let m = (Affine::new(self.page_matrix) * Affine::new(self.placement)).as_coeffs();
        (m[0].hypot(m[1]), m[2].hypot(m[3]))
    }

    fn place(&mut self, then: Affine) {
        self.placement = (Affine::new(self.placement) * then).as_coeffs();
    }

    /// After a quarter turn the picture keeps its centre and its pixels their
    /// size: its sides on the page trade places.
    fn quarter_turned(&mut self) {
        let (a, b) = self.sides();
        if a > 1e-9 && b > 1e-9 {
            self.place(about_center(b / a, a / b));
        }
    }

    /// The picture is drawn mirrored on the page: turning its pixels one way
    /// turns it the other way on the page.
    fn mirrored(&self) -> bool {
        (Affine::new(self.page_matrix) * Affine::new(self.placement)).determinant() < 0.0
    }

    pub fn apply(&mut self, edit: Edit) {
        // Quarter turns are meant as seen on the page.
        let edit = match edit {
            Edit::RotateRight if self.mirrored() => Edit::RotateLeft,
            Edit::RotateLeft if self.mirrored() => Edit::RotateRight,
            e => e,
        };
        let px = &mut self.pixels;
        match edit {
            Edit::RotateRight => {
                *px = imageops::rotate90(px);
                self.quarter_turned();
            }
            Edit::RotateLeft => {
                *px = imageops::rotate270(px);
                self.quarter_turned();
            }
            Edit::FlipHorizontal => imageops::flip_horizontal_in_place(px),
            Edit::FlipVertical => imageops::flip_vertical_in_place(px),
            Edit::Grayscale => {
                gray_in_place(px);
                if matches!(self.colors, Colors::Color | Colors::Indexed) {
                    self.colors = Colors::Gray;
                }
            }
            Edit::Crop(r) => {
                let (w, h) = px.dimensions();
                let x0 = r[0].min(w.saturating_sub(1));
                let y0 = r[1].min(h.saturating_sub(1));
                let x1 = r[2].clamp(x0 + 1, w);
                let y1 = r[3].clamp(y0 + 1, h);
                *px = imageops::crop_imm(px, x0, y0, x1 - x0, y1 - y0).to_image();
                let (wf, hf) = (w as f64, h as f64);
                self.place(Affine::new([
                    (x1 - x0) as f64 / wf,
                    0.0,
                    0.0,
                    (y1 - y0) as f64 / hf,
                    x0 as f64 / wf,
                    (hf - y1 as f64) / hf,
                ]));
            }
            Edit::Turn(deg) => {
                let fill = if self.alpha {
                    Rgba([0, 0, 0, 0])
                } else {
                    Rgba([255, 255, 255, 255])
                };
                *px = turn(px, deg, fill);
            }
            Edit::Brightness(d) => map_rgb(px, |v| (v as i32 + d).clamp(0, 255) as u8),
            Edit::Contrast(c) => {
                let f = ((100.0 + c.clamp(-100.0, 100.0)) / 100.0).powi(2);
                map_rgb(px, |v| {
                    (((v as f32 / 255.0 - 0.5) * f + 0.5) * 255.0)
                        .round()
                        .clamp(0.0, 255.0) as u8
                });
            }
            Edit::Invert => map_rgb(px, |v| 255 - v),
            Edit::Sepia => {
                for p in px.pixels_mut() {
                    let (r, g, b) = (p[0] as f32, p[1] as f32, p[2] as f32);
                    let q = |v: f32| v.round().clamp(0.0, 255.0) as u8;
                    p[0] = q(0.393 * r + 0.769 * g + 0.189 * b);
                    p[1] = q(0.349 * r + 0.686 * g + 0.168 * b);
                    p[2] = q(0.272 * r + 0.534 * g + 0.131 * b);
                }
                if self.colors != Colors::Indexed {
                    self.colors = Colors::Color;
                }
            }
            Edit::Despeckle => *px = median3(px),
            Edit::Filter(f) => *px = convolve(px, f),
            Edit::Convert(format) => self.convert(format),
        }
    }

    fn convert(&mut self, format: Format) {
        let px = &mut self.pixels;
        let keep_alpha = matches!(format, Format::GrayAlpha | Format::Rgba);
        if !keep_alpha {
            flatten(px);
        }
        match format {
            Format::Bilevel => {
                let t = otsu(px);
                for p in px.pixels_mut() {
                    let v = if luma(p) > t { 255 } else { 0 };
                    p.0 = [v, v, v, 255];
                }
                self.colors = Colors::BlackWhite;
            }
            Format::Indexed => {
                quantize(px, 256);
                self.colors = Colors::Indexed;
            }
            Format::Gray | Format::GrayAlpha => {
                gray_in_place(px);
                self.colors = Colors::Gray;
            }
            Format::Rgb | Format::Rgba => self.colors = Colors::Color,
        }
        self.alpha = keep_alpha;
    }

    /// Pixels made elsewhere take the place of these: the picture keeps its
    /// centre and the size of its pixels on the page.
    pub fn replace(&mut self, pixels: RgbaImage) {
        let (w, h) = self.size();
        let (nw, nh) = pixels.dimensions();
        if w > 0 && h > 0 && (nw, nh) != (w, h) {
            self.place(about_center(nw as f64 / w as f64, nh as f64 / h as f64));
        }
        self.colors = colors_of(&pixels, self.colors);
        self.alpha = pixels.pixels().any(|p| p[3] < 255);
        self.pixels = pixels;
    }

    /// A picture made elsewhere to take the place of one of `size` pixels
    /// whose colours were kept as `was`.
    pub fn replacing(size: (u32, u32), pixels: RgbaImage, page_matrix: [f64; 6], was: Colors) -> Self {
        let mut w = Work::new(pixels, page_matrix, was);
        let (nw, nh) = w.size();
        if size.0 > 0 && size.1 > 0 && (nw, nh) != size {
            w.place(about_center(nw as f64 / size.0 as f64, nh as f64 / size.1 as f64));
        }
        w.colors = colors_of(&w.pixels, was);
        w
    }
}

/// How to keep the colours of pixels made elsewhere for a picture whose
/// colours were kept as `was`: black and white or gray stay so while the
/// pixels allow it.
pub fn colors_of(px: &RgbaImage, was: Colors) -> Colors {
    let gray = px.pixels().all(|p| p[0] == p[1] && p[1] == p[2]);
    match was {
        Colors::BlackWhite if gray && px.pixels().all(|p| p[0] == 0 || p[0] == 255) => Colors::BlackWhite,
        Colors::Indexed => Colors::Indexed,
        _ if gray => Colors::Gray,
        _ => Colors::Color,
    }
}

fn luma(p: &Rgba<u8>) -> u8 {
    ((p[0] as u32 * 299 + p[1] as u32 * 587 + p[2] as u32 * 114 + 500) / 1000) as u8
}

fn gray_in_place(px: &mut RgbaImage) {
    for p in px.pixels_mut() {
        let v = luma(p);
        p[0] = v;
        p[1] = v;
        p[2] = v;
    }
}

/// Transparent pixels laid on white.
fn flatten(px: &mut RgbaImage) {
    for p in px.pixels_mut() {
        let a = p[3] as u32;
        if a < 255 {
            for c in 0..3 {
                p[c] = ((p[c] as u32 * a + 255 * (255 - a) + 127) / 255) as u8;
            }
            p[3] = 255;
        }
    }
}

/// At most `colors` colours, chosen to fit the picture (NeuQuant).
pub fn quantize(px: &mut RgbaImage, colors: usize) {
    let mut seen = std::collections::HashSet::new();
    if px.pixels().all(|p| {
        seen.insert([p[0], p[1], p[2]]);
        seen.len() <= colors
    }) {
        return;
    }
    let nq = color_quant::NeuQuant::new(10, colors, px.as_raw());
    for p in px.pixels_mut() {
        // The palette is for the colours: transparency stays as it was.
        let alpha = p[3];
        nq.map_pixel(&mut p.0);
        p[3] = alpha;
    }
}

fn map_rgb(px: &mut RgbaImage, f: impl Fn(u8) -> u8) {
    let table: Vec<u8> = (0..=255u8).map(f).collect();
    for p in px.pixels_mut() {
        for c in 0..3 {
            p[c] = table[p[c] as usize];
        }
    }
}

/// A 3 × 3 convolution of the colours (edges repeat the border pixels).
fn convolve(src: &RgbaImage, filter: Filter) -> RgbaImage {
    let (k, div, offset) = filter.kernel();
    let (w, h) = src.dimensions();
    RgbaImage::from_fn(w, h, |x, y| {
        let mut acc = [0i32; 3];
        for (i, weight) in k.iter().enumerate() {
            if *weight == 0 {
                continue;
            }
            let sx = (x as i64 + (i % 3) as i64 - 1).clamp(0, w as i64 - 1) as u32;
            let sy = (y as i64 + (i / 3) as i64 - 1).clamp(0, h as i64 - 1) as u32;
            let p = src.get_pixel(sx, sy);
            for c in 0..3 {
                acc[c] += p[c] as i32 * weight;
            }
        }
        let mut out = *src.get_pixel(x, y);
        for c in 0..3 {
            out[c] = (acc[c] / div + offset).clamp(0, 255) as u8;
        }
        out
    })
}

/// The threshold that best splits the pixels into dark and light (Otsu).
pub fn otsu(px: &RgbaImage) -> u8 {
    let mut hist = [0u64; 256];
    for p in px.pixels() {
        hist[luma(p) as usize] += 1;
    }
    let total: u64 = hist.iter().sum();
    if total == 0 {
        return 127;
    }
    let sum: f64 = hist.iter().enumerate().map(|(i, n)| i as f64 * *n as f64).sum();
    let (mut sum_b, mut w_b) = (0.0f64, 0u64);
    let (mut best, mut best_var) = (127u8, -1.0f64);
    for (t, n) in hist.iter().enumerate() {
        w_b += n;
        if w_b == 0 {
            continue;
        }
        let w_f = total - w_b;
        if w_f == 0 {
            break;
        }
        sum_b += t as f64 * *n as f64;
        let m_b = sum_b / w_b as f64;
        let m_f = (sum - sum_b) / w_f as f64;
        let var = w_b as f64 * w_f as f64 * (m_b - m_f).powi(2);
        if var > best_var {
            best_var = var;
            best = t as u8;
        }
    }
    best
}

fn bilinear(src: &RgbaImage, x: f64, y: f64) -> Option<Rgba<u8>> {
    let (w, h) = src.dimensions();
    if x < -0.5 || y < -0.5 || x > w as f64 - 0.5 || y > h as f64 - 0.5 {
        return None;
    }
    let x = x.clamp(0.0, (w - 1) as f64);
    let y = y.clamp(0.0, (h - 1) as f64);
    let (x0, y0) = (x.floor() as u32, y.floor() as u32);
    let (x1, y1) = ((x0 + 1).min(w - 1), (y0 + 1).min(h - 1));
    let (fx, fy) = (x - x0 as f64, y - y0 as f64);
    let (a, b, c, d) = (
        src.get_pixel(x0, y0),
        src.get_pixel(x1, y0),
        src.get_pixel(x0, y1),
        src.get_pixel(x1, y1),
    );
    let mut out = [0u8; 4];
    for (k, o) in out.iter_mut().enumerate() {
        let top = a[k] as f64 * (1.0 - fx) + b[k] as f64 * fx;
        let bottom = c[k] as f64 * (1.0 - fx) + d[k] as f64 * fx;
        *o = (top * (1.0 - fy) + bottom * fy).round() as u8;
    }
    Some(Rgba(out))
}

/// Turn the picture by `deg` degrees counterclockwise (as shown) around its
/// centre, keeping its size; uncovered corners take `fill`.
pub fn turn(src: &RgbaImage, deg: f32, fill: Rgba<u8>) -> RgbaImage {
    let (w, h) = src.dimensions();
    let (s, c) = (deg as f64).to_radians().sin_cos();
    let (cx, cy) = (w as f64 / 2.0, h as f64 / 2.0);
    RgbaImage::from_fn(w, h, |x, y| {
        let (dx, dy) = (x as f64 + 0.5 - cx, y as f64 + 0.5 - cy);
        // The pixel shown here came from the point turned back.
        let sx = dx * c - dy * s + cx - 0.5;
        let sy = dx * s + dy * c + cy - 0.5;
        bilinear(src, sx, sy).unwrap_or(fill)
    })
}

/// How many degrees the lines of a picture (text, rules) are turned
/// counterclockwise, between -15 and 15; 0 when nothing stands out.
pub fn skew_angle(px: &RgbaImage) -> f32 {
    let (w, h) = px.dimensions();
    if w < 8 || h < 8 {
        return 0.0;
    }
    // A smaller copy is enough to find the angle.
    let scale = (1200.0 / w.max(h) as f64).min(1.0);
    let small = if scale < 1.0 {
        imageops::resize(
            px,
            ((w as f64 * scale).round() as u32).max(1),
            ((h as f64 * scale).round() as u32).max(1),
            imageops::FilterType::Triangle,
        )
    } else {
        px.clone()
    };
    let t = otsu(&small);
    let (sw, sh) = small.dimensions();
    let (cx, cy) = (sw as f64 / 2.0, sh as f64 / 2.0);
    let mut dark: Vec<(f64, f64)> = small
        .enumerate_pixels()
        .filter(|(_, _, p)| p[3] > 127 && luma(p) <= t)
        .map(|(x, y, _)| (x as f64 - cx, y as f64 - cy))
        .collect();
    // Dark pixels must stand out from a light background.
    if dark.len() < 50 || dark.len() * 2 > (sw * sh) as usize {
        return 0.0;
    }
    let step = dark.len().div_ceil(150_000);
    if step > 1 {
        dark = dark.into_iter().step_by(step).collect();
    }
    let rows = (sw + sh) as usize * 2 + 4;
    let offset = (sw + sh) as f64;
    let mut hist = vec![0u32; rows];
    let mut score = |deg: f64| -> f64 {
        let (s, c) = deg.to_radians().sin_cos();
        hist.iter_mut().for_each(|v| *v = 0);
        for (dx, dy) in &dark {
            // Height of the point once the picture is turned back.
            let y = dx * s + dy * c + offset;
            hist[(y as usize).min(rows - 1)] += 1;
        }
        hist.iter().map(|n| (*n as f64).powi(2)).sum()
    };
    let mut best = (0.0f64, score(0.0));
    let mut deg = -15.0;
    while deg <= 15.0 {
        let s = score(deg);
        if s > best.1 * 1.0001 {
            best = (deg, s);
        }
        deg += 0.25;
    }
    let coarse = best.0;
    let mut deg = coarse - 0.25;
    while deg <= coarse + 0.25 {
        let s = score(deg);
        if s > best.1 {
            best = (deg, s);
        }
        deg += 0.02;
    }
    let angle = (best.0 * 100.0).round() / 100.0;
    if angle.abs() < 0.05 { 0.0 } else { angle as f32 }
}

/// Median of the 3 × 3 neighbourhood of every pixel (colours only).
fn median3(src: &RgbaImage) -> RgbaImage {
    let (w, h) = src.dimensions();
    RgbaImage::from_fn(w, h, |x, y| {
        let mut out = *src.get_pixel(x, y);
        for c in 0..3 {
            let mut v = [0u8; 9];
            let mut n = 0;
            for yy in y.saturating_sub(1)..=(y + 1).min(h - 1) {
                for xx in x.saturating_sub(1)..=(x + 1).min(w - 1) {
                    v[n] = src.get_pixel(xx, yy)[c];
                    n += 1;
                }
            }
            v[..n].sort_unstable();
            out[c] = v[n / 2];
        }
        out
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(w: u32, h: u32) -> RgbaImage {
        RgbaImage::from_fn(w, h, |_, y| {
            if y % 20 < 3 && y > 10 && y < h - 10 {
                Rgba([0, 0, 0, 255])
            } else {
                Rgba([255, 255, 255, 255])
            }
        })
    }

    #[test]
    fn quarter_turns_keep_the_centre_and_swap_the_sides() {
        let px = RgbaImage::from_pixel(40, 20, Rgba([10, 20, 30, 255]));
        // Drawn 200 × 100 points at (50, 60).
        let mut w = Work::new(px, [200.0, 0.0, 0.0, 100.0, 50.0, 60.0], Colors::Color);
        w.apply(Edit::RotateRight);
        assert_eq!(w.size(), (20, 40));
        let (a, b) = w.sides();
        assert!((a - 100.0).abs() < 1e-6 && (b - 200.0).abs() < 1e-6, "{a} {b}");
        let m = Affine::new(w.page_matrix) * Affine::new(w.placement);
        let c = m * hayro::kurbo::Point::new(0.5, 0.5);
        assert!((c.x - 150.0).abs() < 1e-6 && (c.y - 110.0).abs() < 1e-6);
        w.apply(Edit::RotateLeft);
        assert_eq!(w.size(), (40, 20));
        assert!(
            Affine::new(w.placement)
                .as_coeffs()
                .iter()
                .zip(Affine::IDENTITY.as_coeffs())
                .all(|(a, b)| (a - b).abs() < 1e-9)
        );
    }

    #[test]
    fn crops_stay_where_they_were() {
        let px = RgbaImage::from_pixel(100, 50, Rgba([0, 0, 0, 255]));
        let mut w = Work::new(px, [100.0, 0.0, 0.0, 50.0, 0.0, 0.0], Colors::Color);
        // The top-right quarter.
        w.apply(Edit::Crop([50, 0, 100, 25]));
        assert_eq!(w.size(), (50, 25));
        let m = Affine::new(w.page_matrix) * Affine::new(w.placement);
        let r = m.transform_rect_bbox(hayro::kurbo::Rect::new(0.0, 0.0, 1.0, 1.0));
        assert!((r.x0 - 50.0).abs() < 1e-6 && (r.x1 - 100.0).abs() < 1e-6);
        assert!((r.y0 - 25.0).abs() < 1e-6 && (r.y1 - 50.0).abs() < 1e-6, "{r:?}");
    }

    #[test]
    fn turned_lines_are_found_and_straightened() {
        let straight = lines(400, 300);
        assert_eq!(skew_angle(&straight), 0.0);
        let white = Rgba([255, 255, 255, 255]);
        let turned = turn(&straight, 3.0, white);
        let found = skew_angle(&turned);
        assert!((found - 3.0).abs() < 0.15, "found {found}");
        let back = turn(&turned, -found, white);
        assert!(skew_angle(&back).abs() < 0.15);
        let other = turn(&straight, -2.0, white);
        assert!((skew_angle(&other) + 2.0).abs() < 0.15);
    }

    #[test]
    fn formats_and_filters() {
        let mut px = RgbaImage::from_fn(10, 10, |x, _| {
            if x < 5 {
                Rgba([30, 30, 30, 255])
            } else {
                Rgba([220, 210, 200, 255])
            }
        });
        let t = otsu(&px);
        assert!((30..210).contains(&t));
        let mut w = Work::new(px.clone(), Affine::IDENTITY.as_coeffs(), Colors::Color);
        assert_eq!(w.format(), Format::Rgb);
        w.apply(Edit::Convert(Format::Bilevel));
        assert_eq!(w.pixels.get_pixel(0, 0)[0], 0);
        assert_eq!(w.pixels.get_pixel(9, 0).0, [255, 255, 255, 255]);
        assert_eq!(w.format(), Format::Bilevel);
        w.apply(Edit::Invert);
        assert_eq!(w.pixels.get_pixel(0, 0)[0], 255);
        map_rgb(&mut px, |v| v.saturating_add(10));
        assert_eq!(px.get_pixel(0, 0)[0], 40);
        let mut g = Work::new(px, Affine::IDENTITY.as_coeffs(), Colors::Color);
        g.apply(Edit::Grayscale);
        let p = g.pixels.get_pixel(9, 9);
        assert!(p[0] == p[1] && p[1] == p[2]);
        assert_eq!(g.format(), Format::Gray);
        // One stray dark pixel disappears.
        let mut speck = RgbaImage::from_pixel(5, 5, Rgba([255, 255, 255, 255]));
        speck.put_pixel(2, 2, Rgba([0, 0, 0, 255]));
        assert_eq!(median3(&speck).get_pixel(2, 2)[0], 255);
        // A flat picture stays flat when blurred; edges of a flat picture are gray.
        let flat = RgbaImage::from_pixel(6, 6, Rgba([90, 90, 90, 255]));
        assert_eq!(
            convolve(&flat, Filter::GaussianBlur).get_pixel(3, 3).0,
            [90, 90, 90, 255]
        );
        assert_eq!(convolve(&flat, Filter::EdgeDetect).get_pixel(3, 3)[0], 127);
        assert_eq!(convolve(&flat, Filter::Sharpen).get_pixel(3, 3)[0], 90);
    }

    #[test]
    fn transparency_and_palettes() {
        let px = RgbaImage::from_fn(30, 30, |x, y| {
            Rgba([(x * 8) as u8, (y * 8) as u8, 128, if x == 0 { 0 } else { 255 }])
        });
        let mut w = Work::new(px, Affine::IDENTITY.as_coeffs(), Colors::Color);
        assert_eq!(w.format(), Format::Rgba);
        w.apply(Edit::Convert(Format::GrayAlpha));
        assert_eq!(w.format(), Format::GrayAlpha);
        assert_eq!(w.pixels.get_pixel(0, 5)[3], 0);
        w.apply(Edit::Convert(Format::Rgb));
        assert_eq!(w.format(), Format::Rgb);
        // Transparent pixels are laid on white.
        assert_eq!(w.pixels.get_pixel(0, 5).0, [255, 255, 255, 255]);
        let mut many = RgbaImage::from_fn(64, 64, |x, y| {
            Rgba([(x * 4) as u8, (y * 4) as u8, ((x + y) * 2) as u8, 255])
        });
        quantize(&mut many, 256);
        let colors: std::collections::HashSet<[u8; 3]> = many.pixels().map(|p| [p[0], p[1], p[2]]).collect();
        assert!(colors.len() <= 256, "{}", colors.len());
        assert!(
            many.pixels().all(|p| p[3] == 255),
            "a palette keeps the picture opaque"
        );
    }

    #[test]
    fn quarter_turns_follow_the_page_when_mirrored() {
        // Drawn upside down (mirrored vertically): row 0 is at the bottom of the page.
        let mut px = RgbaImage::from_pixel(4, 2, Rgba([255, 255, 255, 255]));
        px.put_pixel(0, 0, Rgba([0, 0, 0, 255]));
        let mut w = Work::new(px, [40.0, 0.0, 0.0, -20.0, 0.0, 20.0], Colors::Color);
        w.apply(Edit::RotateRight);
        // Seen on the page the dark pixel was at the bottom left; turned
        // right it goes to the top left, which in this mirrored drawing is
        // the last row of the pixels.
        assert_eq!(w.size(), (2, 4));
        assert_eq!(w.pixels.get_pixel(0, 3)[0], 0, "{:?}", w.pixels);
    }

    #[test]
    fn pictures_made_elsewhere_keep_the_centre() {
        // A 40 × 20 picture drawn 200 × 100 at (0, 0) comes back 20 × 20 and gray.
        let px = RgbaImage::from_pixel(20, 20, Rgba([80, 80, 80, 255]));
        let w = Work::replacing((40, 20), px, [200.0, 0.0, 0.0, 100.0, 0.0, 0.0], Colors::Color);
        let m = Affine::new(w.page_matrix) * Affine::new(w.placement);
        let r = m.transform_rect_bbox(hayro::kurbo::Rect::new(0.0, 0.0, 1.0, 1.0));
        assert!(
            (r.x0 - 50.0).abs() < 1e-6
                && (r.x1 - 150.0).abs() < 1e-6
                && (r.y0).abs() < 1e-6
                && (r.y1 - 100.0).abs() < 1e-6,
            "{r:?}"
        );
        assert_eq!(w.colors, Colors::Gray);
        let colored = RgbaImage::from_pixel(2, 2, Rgba([255, 0, 0, 255]));
        assert_eq!(colors_of(&colored, Colors::BlackWhite), Colors::Color);
        let bw = RgbaImage::from_pixel(2, 2, Rgba([255, 255, 255, 255]));
        assert_eq!(colors_of(&bw, Colors::BlackWhite), Colors::BlackWhite);
    }
}
