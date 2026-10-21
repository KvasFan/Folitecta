//! Text recognition (OCR): the lines of text of a picture found and read by
//! two PaddleOCR models run with tract (pure Rust), and the text written over
//! the pages as invisible text, so scanned pages can be searched, selected
//! and copied.
//!
//! The detection model gives the probability of text at every pixel of a
//! picture scaled down; the connected areas above a threshold are the lines,
//! each fitted with a turned rectangle (from the moments of its pixels) and
//! grown a little. The recognition model reads each line, cut out straight
//! and scaled to 48 pixels high, as a sequence of character probabilities
//! (CTC); the steps where each character is read give the place of every
//! word. The models are files read at run time (see `ocr/README.md` in the
//! source tree).
//!
//! Words read with doubt (a letter whose probability stays below
//! [`DOUBTFUL_BELOW`]) are written inside a marked-content sequence
//! `/FolitectaDoubt <</Confidence c>> BDC ... EMC`, so they can be checked
//! later, after saving too: [`Document::doubtful_words`] lists them and
//! [`Document::review_ocr_word`] keeps, corrects or removes one.

use std::collections::HashMap;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use image::RgbaImage;
use rayon::prelude::*;
use tract_onnx::prelude::*;

use lopdf::ObjectId;
use lopdf::content::{Content, Operation};

use crate::content_lex::{self, name_of};
use crate::document::{Document, PageSize};
use crate::error::{Error, Result};
use crate::marks::{display_to_pdf, pdf_to_display};
use crate::render::{RenderOpts, Renderer};
use crate::util::{clean_text, fmt_num, helvetica_width, pdf_literal, winansi_byte};

/// Tag of the content streams that hold recognized text.
pub const OCR_MARK: &str = "OCR";

/// The files of the models.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OcrModels {
    pub det: PathBuf,
    pub rec: PathBuf,
    pub dict: PathBuf,
}

/// The first folder of `dirs` with the models (`det.onnx`,
/// `rec-latin.onnx`, `dict-latin.txt`).
pub fn find_models(dirs: &[PathBuf]) -> Option<OcrModels> {
    dirs.iter().find_map(|d| {
        let m = OcrModels {
            det: d.join("det.onnx"),
            rec: d.join("rec-latin.onnx"),
            dict: d.join("dict-latin.txt"),
        };
        (m.det.is_file() && m.rec.is_file() && m.dict.is_file()).then_some(m)
    })
}

/// A word read in a picture.
#[derive(Debug, Clone, PartialEq)]
pub struct OcrWord {
    pub text: String,
    /// Corners in pixels: upper-left, upper-right, lower-right, lower-left.
    pub quad: [[f32; 2]; 4],
    /// Probability of its least sure letter (0 to 1).
    pub confidence: f32,
}

/// Words with a letter read with less probability than this are doubtful.
pub const DOUBTFUL_BELOW: f32 = 0.8;

/// Tag of the marked-content sequences of doubtful words.
const DOUBT_TAG: &str = "FolitectaDoubt";

/// A recognized word written with doubt, waiting to be checked.
#[derive(Debug, Clone, PartialEq)]
pub struct DoubtfulWord {
    pub page: usize,
    /// Its place among the doubtful words of its page, in writing order.
    pub index: usize,
    pub text: String,
    /// Probability of its least sure letter (0 to 1).
    pub confidence: f32,
    /// Where it was read (display points, y down).
    pub rect: [f32; 4],
}

/// A line of text read in a picture.
#[derive(Debug, Clone, PartialEq)]
pub struct OcrLine {
    pub text: String,
    /// Corners in pixels: upper-left, upper-right, lower-right, lower-left.
    pub quad: [[f32; 2]; 4],
    pub words: Vec<OcrWord>,
    /// Mean probability of the characters read (0 to 1).
    pub confidence: f32,
}

/// The text read on a page.
#[derive(Debug, Clone, PartialEq)]
pub struct PageOcr {
    pub page: usize,
    /// Pixels per point of the picture the text was read in (the page as shown).
    pub scale: f32,
    pub lines: Vec<OcrLine>,
}

/// How hard to look: the resolution pages are drawn at and the size the
/// text is searched at.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OcrOptions {
    /// Dots per inch the pages are drawn at for reading.
    pub dpi: f32,
    /// Longest side of the picture the lines are searched in (pixels).
    pub search_side: usize,
}

impl Default for OcrOptions {
    fn default() -> Self {
        OcrOptions {
            dpi: 200.0,
            search_side: 1280,
        }
    }
}

type Plan = Arc<TypedRunnableModel<TypedModel>>;

/// Widths a line is read at (padded up to the next one), so a few plans
/// with fixed sizes serve every line.
const REC_WIDTHS: [usize; 7] = [320, 640, 960, 1280, 1600, 2048, 2560];
const REC_HEIGHT: usize = 48;

/// The models loaded, with the plans made for the sizes met so far.
pub struct OcrEngine {
    det: InferenceModel,
    rec: InferenceModel,
    dict: Vec<String>,
    det_plans: Mutex<HashMap<(usize, usize), Plan>>,
    rec_plans: Mutex<HashMap<usize, Plan>>,
}

impl std::fmt::Debug for OcrEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OcrEngine")
            .field("characters", &self.dict.len())
            .finish_non_exhaustive()
    }
}

fn tract_err(e: impl std::fmt::Display) -> Error {
    Error::Invalid(format!("text recognition: {e}"))
}

/// Read an ONNX model. The names some converters give the free dimensions
/// ("p2o.DynamicDimension.0") are not ones tract reads: they are made plain,
/// with the same length so the file keeps its framing.
fn load_model(path: &Path) -> Result<InferenceModel> {
    let mut bytes = std::fs::read(path).map_err(|e| Error::io(path, e))?;
    for (from, to) in [
        (&b"DynamicDimension."[..], &b"DynamicDimension_"[..]),
        (b"p2o.", b"p2o_"),
    ] {
        let mut i = 0;
        while i + from.len() <= bytes.len() {
            if &bytes[i..i + from.len()] == from {
                bytes[i..i + from.len()].copy_from_slice(to);
                i += from.len();
            } else {
                i += 1;
            }
        }
    }
    tract_onnx::onnx()
        .model_for_read(&mut Cursor::new(bytes))
        .map_err(tract_err)
}

/// A turned rectangle: its middle, its half sizes along and across the
/// line, and the direction along the line.
#[derive(Debug, Clone, Copy)]
struct Turned {
    cx: f32,
    cy: f32,
    half_w: f32,
    half_h: f32,
    cos: f32,
    sin: f32,
}

impl Turned {
    /// Corners: upper-left, upper-right, lower-right, lower-left (y down).
    fn quad(&self) -> [[f32; 2]; 4] {
        let (ux, uy) = (self.cos * self.half_w, self.sin * self.half_w);
        let (vx, vy) = (-self.sin * self.half_h, self.cos * self.half_h);
        [
            [self.cx - ux - vx, self.cy - uy - vy],
            [self.cx + ux - vx, self.cy + uy - vy],
            [self.cx + ux + vx, self.cy + uy + vy],
            [self.cx - ux + vx, self.cy - uy + vy],
        ]
    }

    /// The point at `t` (0 to 1) along the line and `s` (0 top, 1 bottom) across.
    fn at(&self, t: f32, s: f32) -> [f32; 2] {
        let a = (2.0 * t - 1.0) * self.half_w;
        let b = (2.0 * s - 1.0) * self.half_h;
        [
            self.cx + a * self.cos - b * self.sin,
            self.cy + a * self.sin + b * self.cos,
        ]
    }
}

/// The lines of a probability map (`w` × `h`, row by row): connected areas
/// above 0.3 whose mean probability reaches 0.6, fitted with turned
/// rectangles and grown (in map pixels).
fn find_lines(prob: &[f32], w: usize, h: usize) -> Vec<Turned> {
    const BIN: f32 = 0.3;
    let mut seen = vec![false; w * h];
    let mut out = Vec::new();
    let mut stack = Vec::new();
    let mut pts: Vec<(f32, f32)> = Vec::new();
    for start in 0..w * h {
        if seen[start] || prob[start] <= BIN {
            continue;
        }
        seen[start] = true;
        stack.push(start);
        pts.clear();
        let mut sum = 0.0f32;
        while let Some(i) = stack.pop() {
            let (x, y) = (i % w, i / w);
            sum += prob[i];
            pts.push((x as f32, y as f32));
            for (dx, dy) in [
                (-1i32, 0i32),
                (1, 0),
                (0, -1),
                (0, 1),
                (-1, -1),
                (1, -1),
                (-1, 1),
                (1, 1),
            ] {
                let (nx, ny) = (x as i32 + dx, y as i32 + dy);
                if nx < 0 || ny < 0 || nx >= w as i32 || ny >= h as i32 {
                    continue;
                }
                let j = ny as usize * w + nx as usize;
                if !seen[j] && prob[j] > BIN {
                    seen[j] = true;
                    stack.push(j);
                }
            }
        }
        if pts.len() < 8 || sum / (pts.len() as f32) < 0.6 {
            continue;
        }
        // The direction of the line from the second moments of its pixels,
        // kept within 45 degrees of the horizontal (text read left to right).
        let n = pts.len() as f32;
        let (mx, my) = pts.iter().fold((0.0, 0.0), |a, p| (a.0 + p.0, a.1 + p.1));
        let (mx, my) = (mx / n, my / n);
        let (mut sxx, mut syy, mut sxy) = (0.0f32, 0.0f32, 0.0f32);
        for (x, y) in &pts {
            sxx += (x - mx) * (x - mx);
            syy += (y - my) * (y - my);
            sxy += (x - mx) * (y - my);
        }
        let mut angle = 0.5 * (2.0 * sxy).atan2(sxx - syy);
        if angle > std::f32::consts::FRAC_PI_4 {
            angle -= std::f32::consts::FRAC_PI_2;
        } else if angle < -std::f32::consts::FRAC_PI_4 {
            angle += std::f32::consts::FRAC_PI_2;
        }
        // Nearly level lines are taken as level.
        if angle.abs() < 0.6f32.to_radians() {
            angle = 0.0;
        }
        let (sin, cos) = angle.sin_cos();
        let (mut u0, mut u1, mut v0, mut v1) = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
        for (x, y) in &pts {
            let (dx, dy) = (x - mx, y - my);
            let u = dx * cos + dy * sin;
            let v = -dx * sin + dy * cos;
            u0 = u0.min(u);
            u1 = u1.max(u);
            v0 = v0.min(v);
            v1 = v1.max(v);
        }
        let (bw, bh) = (u1 - u0 + 1.0, v1 - v0 + 1.0);
        if bh.min(bw) < 3.0 {
            continue;
        }
        // Grown by area × 1.5 / perimeter on every side, as the model was trained.
        let d = bw * bh * 1.5 / (2.0 * (bw + bh));
        let (cu, cv) = ((u0 + u1) / 2.0, (v0 + v1) / 2.0);
        out.push(Turned {
            cx: mx + cu * cos - cv * sin,
            cy: my + cu * sin + cv * cos,
            half_w: bw / 2.0 + d,
            half_h: bh / 2.0 + d,
            cos,
            sin,
        });
    }
    out
}

/// Lines in reading order: top to bottom, and left to right within a row.
fn reading_order(lines: &mut [OcrLine]) {
    let top = |l: &OcrLine| l.quad.iter().map(|p| p[1]).fold(f32::MAX, f32::min);
    let bottom = |l: &OcrLine| l.quad.iter().map(|p| p[1]).fold(f32::MIN, f32::max);
    let left = |l: &OcrLine| l.quad.iter().map(|p| p[0]).fold(f32::MAX, f32::min);
    lines.sort_by(|a, b| (top(a) + bottom(a)).total_cmp(&(top(b) + bottom(b))));
    // Lines whose middles are within half a line of each other are one row.
    let mut i = 0;
    while i < lines.len() {
        let mid = (top(&lines[i]) + bottom(&lines[i])) / 2.0;
        let height = bottom(&lines[i]) - top(&lines[i]);
        let mut j = i + 1;
        while j < lines.len() && ((top(&lines[j]) + bottom(&lines[j])) / 2.0 - mid).abs() < height * 0.5 {
            j += 1;
        }
        lines[i..j].sort_by(|a, b| left(a).total_cmp(&left(b)));
        i = j;
    }
}

/// The colour of a picture at a point, interpolated between its pixels
/// (white outside it), each channel 0 to 1.
fn sample(img: &RgbaImage, x: f32, y: f32) -> [f32; 3] {
    let (w, h) = (img.width() as i64, img.height() as i64);
    let px = img.as_raw();
    let (x, y) = (x - 0.5, y - 0.5);
    let (x0, y0) = (x.floor() as i64, y.floor() as i64);
    let (fx, fy) = (x - x0 as f32, y - y0 as f32);
    let get = |xx: i64, yy: i64, c: usize| -> f32 {
        if xx < 0 || yy < 0 || xx >= w || yy >= h {
            1.0
        } else {
            px[((yy * w + xx) * 4) as usize + c] as f32 / 255.0
        }
    };
    let mut out = [0.0; 3];
    for (c, o) in out.iter_mut().enumerate() {
        let top = get(x0, y0, c) * (1.0 - fx) + get(x0 + 1, y0, c) * fx;
        let bottom = get(x0, y0 + 1, c) * (1.0 - fx) + get(x0 + 1, y0 + 1, c) * fx;
        *o = top * (1.0 - fy) + bottom * fy;
    }
    out
}

impl OcrEngine {
    /// Read the models and the characters of the recognition model.
    pub fn load(models: &OcrModels) -> Result<OcrEngine> {
        let dict: Vec<String> = std::fs::read_to_string(&models.dict)
            .map_err(|e| Error::io(&models.dict, e))?
            .lines()
            .map(str::to_string)
            .collect();
        if dict.is_empty() {
            return Err(Error::Invalid(
                "the character list of the text recognition is empty".into(),
            ));
        }
        Ok(OcrEngine {
            det: load_model(&models.det)?,
            rec: load_model(&models.rec)?,
            dict,
            det_plans: Mutex::new(HashMap::new()),
            rec_plans: Mutex::new(HashMap::new()),
        })
    }

    /// The plan of the detection model for a picture of `w` × `h` pixels.
    fn det_plan(&self, w: usize, h: usize) -> Result<Plan> {
        if let Some(p) = self
            .det_plans
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&(w, h))
        {
            return Ok(p.clone());
        }
        let plan = Arc::new(
            self.det
                .clone()
                .with_input_fact(0, f32::fact([1, 3, h, w]).into())
                .and_then(|m| m.into_optimized())
                .and_then(|m| m.into_runnable())
                .map_err(tract_err)?,
        );
        let mut plans = self.det_plans.lock().unwrap_or_else(|e| e.into_inner());
        if plans.len() > 8 {
            plans.clear();
        }
        plans.insert((w, h), plan.clone());
        Ok(plan)
    }

    /// The plan of the recognition model for lines `w` pixels wide.
    fn rec_plan(&self, w: usize) -> Result<Plan> {
        if let Some(p) = self.rec_plans.lock().unwrap_or_else(|e| e.into_inner()).get(&w) {
            return Ok(p.clone());
        }
        let plan = Arc::new(
            self.rec
                .clone()
                .with_input_fact(0, f32::fact([1, 3, REC_HEIGHT, w]).into())
                .and_then(|m| m.into_optimized())
                .and_then(|m| m.into_runnable())
                .map_err(tract_err)?,
        );
        self.rec_plans
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(w, plan.clone());
        Ok(plan)
    }

    /// The lines of text of a picture, found in it scaled so its longest
    /// side is at most `search_side` pixels.
    fn detect(&self, img: &RgbaImage, search_side: usize) -> Result<Vec<Turned>> {
        let (w, h) = (img.width() as usize, img.height() as usize);
        if w < 8 || h < 8 {
            return Ok(Vec::new());
        }
        let scale = (search_side as f32 / w.max(h) as f32).min(1.0);
        let round32 = |v: f32| ((v / 32.0).round() as usize).max(1) * 32;
        let (dw, dh) = (round32(w as f32 * scale), round32(h as f32 * scale));
        let (sx, sy) = (w as f32 / dw as f32, h as f32 / dh as f32);
        let mean = [0.485f32, 0.456, 0.406];
        let std = [0.229f32, 0.224, 0.225];
        let input: Tensor = tract_ndarray::Array4::from_shape_fn((1, 3, dh, dw), |(_, c, y, x)| {
            let v = sample(img, (x as f32 + 0.5) * sx, (y as f32 + 0.5) * sy)[c];
            (v - mean[c]) / std[c]
        })
        .into();
        let out = self
            .det_plan(dw, dh)?
            .run(tvec!(input.into()))
            .map_err(tract_err)?;
        let prob = out[0].to_array_view::<f32>().map_err(tract_err)?;
        let shape = prob.shape().to_vec();
        let (ph, pw) = (shape[shape.len() - 2], shape[shape.len() - 1]);
        let flat: Vec<f32> = prob.iter().copied().collect();
        let (fx, fy) = (w as f32 / pw as f32, h as f32 / ph as f32);
        Ok(find_lines(&flat, pw, ph)
            .into_iter()
            .map(|t| Turned {
                cx: (t.cx + 0.5) * fx,
                cy: (t.cy + 0.5) * fy,
                // The map is scaled alike in both directions (to 32 pixels).
                half_w: t.half_w * (fx * t.cos.abs() + fy * t.sin.abs()),
                half_h: t.half_h * (fy * t.cos.abs() + fx * t.sin.abs()),
                ..t
            })
            .collect())
    }

    /// Read one line: its text, the words with their places, the confidence.
    fn read_line(&self, img: &RgbaImage, line: &Turned) -> Result<Option<OcrLine>> {
        let (lw, lh) = (2.0 * line.half_w, 2.0 * line.half_h);
        if lw < 2.0 || lh < 2.0 {
            return Ok(None);
        }
        let natural = (lw * REC_HEIGHT as f32 / lh).ceil().max(8.0) as usize;
        let max = *REC_WIDTHS.last().unwrap_or(&2560);
        let used = natural.min(max);
        let width = REC_WIDTHS.iter().copied().find(|w| *w >= used).unwrap_or(max);
        let input: Tensor =
            tract_ndarray::Array4::from_shape_fn((1, 3, REC_HEIGHT, width), |(_, c, y, x)| {
                if x >= used {
                    return 0.0;
                }
                let t = (x as f32 + 0.5) / used as f32;
                let s = (y as f32 + 0.5) / REC_HEIGHT as f32;
                let p = line.at(t, s);
                (sample(img, p[0], p[1])[c] - 0.5) / 0.5
            })
            .into();
        let out = self
            .rec_plan(width)?
            .run(tvec!(input.into()))
            .map_err(tract_err)?;
        let probs = out[0].to_array_view::<f32>().map_err(tract_err)?;
        let shape = probs.shape().to_vec();
        let (steps, classes) = (shape[1], shape[2]);
        // Greedy CTC: the best class of every step, repeats joined, blanks dropped.
        let mut chars: Vec<(String, usize, usize, f32)> = Vec::new();
        let mut last = 0usize;
        for i in 0..steps {
            let (mut best, mut p) = (0usize, f32::MIN);
            for c in 0..classes {
                let v = probs[[0, i, c]];
                if v > p {
                    best = c;
                    p = v;
                }
            }
            if best != 0 && best == last {
                if let Some(ch) = chars.last_mut() {
                    ch.2 = i;
                    ch.3 = ch.3.max(p);
                }
            } else if best != 0 {
                let s = self.dict.get(best - 1).map(String::as_str).unwrap_or(" ");
                chars.push((s.to_string(), i, i, p));
            }
            last = best;
        }
        // Steps beyond the line itself (the padding) are not read.
        let step_w = width as f32 / steps as f32;
        let along = |step: f32| ((step * step_w) / used as f32).clamp(0.0, 1.0);
        let text: String = chars.iter().map(|c| c.0.as_str()).collect::<String>();
        let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
        if text.is_empty() {
            return Ok(None);
        }
        let confidence = chars.iter().map(|c| c.3.clamp(0.0, 1.0)).sum::<f32>() / chars.len().max(1) as f32;
        let mut words = Vec::new();
        let mut cur: Option<(String, usize, usize, f32)> = None;
        for (s, a, b, p) in chars
            .iter()
            .cloned()
            .chain(std::iter::once((" ".to_string(), steps, steps, 0.0)))
        {
            if s.trim().is_empty() {
                if let Some((w, a0, b0, least)) = cur.take() {
                    let (t0, t1) = (along(a0 as f32), along(b0 as f32 + 1.0));
                    words.push(OcrWord {
                        text: w,
                        quad: [
                            line.at(t0, 0.0),
                            line.at(t1, 0.0),
                            line.at(t1, 1.0),
                            line.at(t0, 1.0),
                        ],
                        confidence: least,
                    });
                }
            } else {
                let p = p.clamp(0.0, 1.0);
                match cur.as_mut() {
                    Some(c) => {
                        c.0.push_str(&s);
                        c.2 = b;
                        c.3 = c.3.min(p);
                    }
                    None => cur = Some((s, a, b, p)),
                }
            }
        }
        Ok(Some(OcrLine {
            text,
            quad: line.quad(),
            words,
            confidence,
        }))
    }

    /// Every line of text of a picture, in reading order (pixels of the picture).
    pub fn read(&self, img: &RgbaImage, search_side: usize) -> Result<Vec<OcrLine>> {
        let found = self.detect(img, search_side)?;
        let read: Vec<Result<Option<OcrLine>>> = found.par_iter().map(|l| self.read_line(img, l)).collect();
        let mut lines = Vec::new();
        for r in read {
            if let Some(l) = r? {
                lines.push(l);
            }
        }
        reading_order(&mut lines);
        Ok(lines)
    }

    /// Draw a page of a document (its content, without comments) and read it.
    pub fn read_page(&self, bytes: Arc<Vec<u8>>, page: usize, opts: OcrOptions) -> Result<PageOcr> {
        self.read_page_area(bytes, page, None, opts)
    }

    /// Draw a page and read it, or only the area `rect` of it (display
    /// points, y down); the lines are in pixels of the whole page.
    pub fn read_page_area(
        &self,
        bytes: Arc<Vec<u8>>,
        page: usize,
        rect: Option<[f32; 4]>,
        opts: OcrOptions,
    ) -> Result<PageOcr> {
        let renderer = Renderer::new(bytes)?;
        let scale = opts.dpi / 72.0;
        let img = renderer.render_rgba(
            page,
            scale,
            RenderOpts {
                annotations: false,
                transparent: false,
            },
        )?;
        let lines = match rect {
            None => self.read(&img, opts.search_side)?,
            Some(r) => {
                let (w, h) = (img.width(), img.height());
                let x0 = ((r[0].min(r[2]) * scale).floor().max(0.0) as u32).min(w.saturating_sub(1));
                let y0 = ((r[1].min(r[3]) * scale).floor().max(0.0) as u32).min(h.saturating_sub(1));
                let x1 = ((r[0].max(r[2]) * scale).ceil() as u32).clamp(x0 + 1, w);
                let y1 = ((r[1].max(r[3]) * scale).ceil() as u32).clamp(y0 + 1, h);
                let part = image::imageops::crop_imm(&img, x0, y0, x1 - x0, y1 - y0).to_image();
                let side = (x1 - x0).max(y1 - y0) as usize;
                let mut lines = self.read(&part, side.clamp(320, opts.search_side.max(1600)))?;
                let shift = |p: &mut [f32; 2]| {
                    p[0] += x0 as f32;
                    p[1] += y0 as f32;
                };
                for l in &mut lines {
                    l.quad.iter_mut().for_each(shift);
                    for w in &mut l.words {
                        w.quad.iter_mut().for_each(shift);
                    }
                }
                lines
            }
        };
        Ok(PageOcr { page, scale, lines })
    }
}

/// The text written for a word: what WinAnsi has, accents taken off the
/// letters it lacks.
fn writable(text: &str) -> String {
    text.chars()
        .map(|c| {
            if c != '?' && winansi_byte(c) == b'?' {
                crate::textlayer::strip_diacritic(c)
            } else {
                c
            }
        })
        .collect()
}

/// Text written in WinAnsi bytes.
fn winansi_text(bytes: &[u8]) -> String {
    const HIGH: &str = "€‚ƒ„…†‡ˆ‰Š‹ŒŽ‘’“”•–—˜™š›œžŸ";
    bytes
        .iter()
        .map(|&b| {
            if !(0x80..0xA0).contains(&b) {
                b as char
            } else {
                HIGH.chars().find(|&c| winansi_byte(c) == b).unwrap_or('?')
            }
        })
        .collect()
}

/// A recognized word as written: font, size, horizontal stretch (percent),
/// text matrix (unit directions along and across the line, start of the
/// baseline) and text.
#[derive(Debug, Clone, PartialEq)]
struct Placed {
    font: String,
    size: f32,
    stretch: f32,
    matrix: [f32; 6],
    text: String,
}

impl Placed {
    /// Its length along the line (points).
    fn length(&self) -> f32 {
        helvetica_width(&self.text, self.size) * self.stretch / 100.0
    }

    /// The same place with `text`, stretched to `len` points.
    fn with_text(&self, text: &str, len: f32) -> Placed {
        let natural = helvetica_width(text, self.size).max(0.01);
        Placed {
            stretch: (100.0 * len / natural).clamp(5.0, 2000.0),
            text: text.to_string(),
            ..self.clone()
        }
    }

    /// The operations that write it.
    fn ops(&self) -> String {
        let (f, m) = (fmt_num, self.matrix);
        format!(
            "/{} {} Tf {} Tz {} {} {} {} {} {} Tm {} Tj",
            self.font,
            f(self.size),
            f(self.stretch),
            f(m[0]),
            f(m[1]),
            f(m[2]),
            f(m[3]),
            f(m[4]),
            f(m[5]),
            String::from_utf8_lossy(&pdf_literal(&self.text)).replace('\n', " ")
        )
    }

    /// Its box on the page (PDF points), as the word was read: the size is
    /// most of the height of the box and the baseline a fifth of it up.
    fn corners(&self) -> [(f32, f32); 4] {
        let [ux, uy, nx, ny, ox, oy] = self.matrix;
        let height = self.size / 0.72;
        let lift = height * 0.22;
        let len = self.length();
        let ll = (ox - nx * lift, oy - ny * lift);
        let ul = (ll.0 + nx * height, ll.1 + ny * height);
        [
            ll,
            (ll.0 + ux * len, ll.1 + uy * len),
            (ul.0 + ux * len, ul.1 + uy * len),
            ul,
        ]
    }

    /// Read back from the operations that write one word.
    fn parse(ops: &[Operation]) -> Option<Placed> {
        let num = |o: &Operation, i: usize| o.operands.get(i).and_then(|v| v.as_float().ok());
        let (mut font, mut size, mut stretch, mut matrix, mut text) = (None, None, 100.0, None, None);
        for o in ops {
            match o.operator.as_str() {
                "Tf" => {
                    font = o
                        .operands
                        .first()
                        .and_then(|n| n.as_name().ok())
                        .map(|n| String::from_utf8_lossy(n).to_string());
                    size = num(o, 1);
                }
                "Tz" => stretch = num(o, 0)?,
                "Tm" => {
                    let m: Vec<f32> = (0..6).filter_map(|i| num(o, i)).collect();
                    matrix = <[f32; 6]>::try_from(m).ok();
                }
                "Tj" => text = o.operands.first().and_then(|t| t.as_str().ok()).map(winansi_text),
                _ => {}
            }
        }
        Some(Placed {
            font: font?,
            size: size?,
            stretch,
            matrix: matrix?,
            text: text?,
        })
    }
}

/// A doubtful word in a stream of recognized text: the bytes of its
/// marked-content sequence, those of the operations inside it, how sure the
/// reading was and the word as written.
struct Doubt {
    span: (usize, usize),
    inner: (usize, usize),
    confidence: f32,
    placed: Placed,
}

/// The doubtful words of a stream of recognized text, in writing order.
fn doubts(data: &[u8]) -> Vec<Doubt> {
    let ops = content_lex::operations(data);
    let mut out = Vec::new();
    let mut open = None;
    for (i, op) in ops.iter().enumerate() {
        match op.operator {
            b"BDC" => {
                let tag = op.operands.first().and_then(|r| name_of(data, *r));
                open = (tag.as_deref() == Some(DOUBT_TAG.as_bytes())).then_some(i);
            }
            b"EMC" => {
                let Some(b) = open.take() else { continue };
                if b + 1 >= i {
                    continue;
                }
                let (first, last) = (&ops[b + 1], &ops[i - 1]);
                let Ok(bdc) = Content::decode(&data[ops[b].start..ops[b].end]) else {
                    continue;
                };
                let confidence = bdc
                    .operations
                    .first()
                    .and_then(|o| o.operands.get(1))
                    .and_then(|d| d.as_dict().ok())
                    .and_then(|d| d.get(b"Confidence").ok())
                    .and_then(|c| c.as_float().ok())
                    .unwrap_or(0.0);
                let Some(placed) = Content::decode(&data[first.start..last.end])
                    .ok()
                    .and_then(|c| Placed::parse(&c.operations))
                else {
                    continue;
                };
                out.push(Doubt {
                    span: (ops[b].start, op.end),
                    inner: (first.start, last.end),
                    confidence,
                    placed,
                });
            }
            _ => {}
        }
    }
    out
}

/// Content stream operations that write `words` invisibly (text render
/// mode 3) over a page, each word stretched to the place it was read at.
fn ocr_content(size: &PageSize, scale: f32, lines: &[OcrLine], font: &str) -> Vec<u8> {
    let to_pdf = |p: [f32; 2]| display_to_pdf(size, p[0] / scale, size.height - p[1] / scale);
    let f = fmt_num;
    let mut out = String::from("BT\n3 Tr\n");
    for line in lines {
        let words: Vec<OcrWord> = if line.words.is_empty() {
            vec![OcrWord {
                text: line.text.clone(),
                quad: line.quad,
                confidence: line.confidence,
            }]
        } else {
            line.words.clone()
        };
        for w in &words {
            let text = writable(&w.text);
            if text.trim().is_empty() {
                continue;
            }
            let (ul, lr, ll) = (to_pdf(w.quad[0]), to_pdf(w.quad[2]), to_pdf(w.quad[3]));
            let (bx, by) = (lr.0 - ll.0, lr.1 - ll.1);
            let len = (bx * bx + by * by).sqrt();
            let height = ((ul.0 - ll.0).powi(2) + (ul.1 - ll.1).powi(2)).sqrt();
            if len < 0.5 || height < 0.5 {
                continue;
            }
            let (ux, uy) = (bx / len, by / len);
            // The box holds the letters with some room: the size is most of
            // its height and the baseline a fifth of it up.
            let size_pt = (height * 0.72).max(1.0);
            let lift = height * 0.22;
            let (ox, oy) = (ll.0 - uy * lift, ll.1 + ux * lift);
            let doubtful = w.confidence < DOUBTFUL_BELOW;
            if doubtful {
                out.push_str(&format!(
                    "/{DOUBT_TAG} <</Confidence {}>> BDC ",
                    f(w.confidence.clamp(0.0, 1.0))
                ));
            }
            let placed = Placed {
                font: font.to_string(),
                size: size_pt,
                stretch: 0.0,
                matrix: [ux, uy, -uy, ux, ox, oy],
                text: String::new(),
            };
            out.push_str(&placed.with_text(&text, len).ops());
            out.push_str(if doubtful { " EMC\n" } else { "\n" });
        }
    }
    out.push_str("ET\n");
    out.into_bytes()
}

impl Document {
    /// Whether a page holds text written by text recognition.
    pub fn has_ocr_text(&self, page: usize) -> bool {
        self.marks_on_page(page).iter().any(|m| m == OCR_MARK)
    }

    /// Write recognized text invisibly over its pages, in place of text
    /// recognized before (one undo step). Returns how many pages got text.
    pub fn add_ocr_text(&mut self, results: &[PageOcr]) -> Result<usize> {
        self.write_ocr_text(results, true)
    }

    /// Write the text recognized in an area of a page, next to the text
    /// recognized before (one undo step).
    pub fn append_ocr_text(&mut self, result: &PageOcr) -> Result<usize> {
        self.write_ocr_text(std::slice::from_ref(result), false)
    }

    fn write_ocr_text(&mut self, results: &[PageOcr], replace: bool) -> Result<usize> {
        let ids = self.page_ids();
        for r in results {
            if r.page >= ids.len() {
                return Err(Error::InvalidPage(r.page));
            }
        }
        self.begin_edit();
        if replace {
            let pages: Vec<usize> = results.iter().map(|r| r.page).collect();
            self.strip_marks(&pages, &[OCR_MARK])?;
        }
        let font = self.helvetica_font_id();
        let mut written = 0;
        for r in results {
            if r.lines.is_empty() {
                continue;
            }
            let page_id = ids[r.page];
            let size = self.page_size_of(page_id);
            self.add_page_resource(page_id, "Font", "FoliOcr", lopdf::Object::Reference(font))?;
            let content = ocr_content(&size, r.scale, &r.lines, "FoliOcr");
            let mut wrapped = b"q\n".to_vec();
            wrapped.extend(content);
            wrapped.extend_from_slice(b"Q\n");
            self.append_page_content(page_id, wrapped, Some(OCR_MARK))?;
            written += 1;
        }
        self.commit()?;
        Ok(written)
    }

    /// The streams of text written by text recognition on page `page`.
    fn ocr_streams(&self, page: usize) -> Vec<(ObjectId, Vec<u8>)> {
        let Ok(id) = self.page_id(page) else {
            return Vec::new();
        };
        self.page_content_ids(id)
            .into_iter()
            .filter_map(|cid| {
                let stream = self.inner.get_object(cid).ok()?.as_stream().ok()?;
                let mark = stream.dict.get(b"FolitectaMark").ok()?.as_name().ok()?;
                (mark == OCR_MARK.as_bytes())
                    .then(|| stream.get_plain_content().ok())
                    .flatten()
                    .map(|data| (cid, data))
            })
            .collect()
    }

    /// The words text recognition wrote with doubt on page `page` and that
    /// are not checked yet, in writing order.
    pub fn doubtful_words(&self, page: usize) -> Vec<DoubtfulWord> {
        let Ok(id) = self.page_id(page) else {
            return Vec::new();
        };
        let size = self.page_size_of(id);
        let mut out = Vec::new();
        for (_, data) in self.ocr_streams(page) {
            for d in doubts(&data) {
                let points = d.placed.corners().map(|(x, y)| pdf_to_display(&size, x, y));
                let (xs, ys) = (points.map(|p| p.0), points.map(|p| p.1));
                let min = |v: [f32; 4]| v.into_iter().fold(f32::MAX, f32::min);
                let max = |v: [f32; 4]| v.into_iter().fold(f32::MIN, f32::max);
                out.push(DoubtfulWord {
                    page,
                    index: out.len(),
                    text: d.placed.text,
                    confidence: d.confidence,
                    rect: [min(xs), min(ys), max(xs), max(ys)],
                });
            }
        }
        out
    }

    /// Check doubtful word `index` of page `page`: `None` keeps its text,
    /// `Some(text)` writes that text in its place (accents WinAnsi lacks
    /// taken off), and an empty text takes the word away. Either way it is
    /// no longer doubtful. One undo step.
    pub fn review_ocr_word(&mut self, page: usize, index: usize, text: Option<&str>) -> Result<()> {
        let mut seen = 0;
        for (cid, data) in self.ocr_streams(page) {
            let found = doubts(&data);
            let Some(d) = index.checked_sub(seen).and_then(|i| found.get(i)) else {
                seen += found.len();
                continue;
            };
            let replacement = match text {
                None => data[d.inner.0..d.inner.1].to_vec(),
                Some(t) => {
                    let t = writable(&clean_text(t));
                    let t = t.split_whitespace().collect::<Vec<_>>().join(" ");
                    if t.is_empty() {
                        Vec::new()
                    } else {
                        d.placed.with_text(&t, d.placed.length()).ops().into_bytes()
                    }
                }
            };
            let mut content = data[..d.span.0].to_vec();
            content.extend(replacement);
            content.extend_from_slice(&data[d.span.1..]);
            self.begin_edit();
            match self.inner.get_object_mut(cid).and_then(|o| o.as_stream_mut()) {
                Ok(stream) => stream.set_plain_content(content),
                Err(e) => {
                    self.abandon_edit();
                    return Err(e.into());
                }
            }
            return self.commit();
        }
        Err(Error::Invalid(format!(
            "page {} has no doubtful word {}",
            page + 1,
            index + 1
        )))
    }

    /// Take away the text written by text recognition (one undo step).
    /// Returns how many streams of it were removed.
    pub fn remove_ocr_text(&mut self, pages: &[usize]) -> Result<usize> {
        self.remove_marks_of(pages, &[OCR_MARK])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_of_a_probability_map() {
        // Two bars of text 6 pixels high on a 64 × 32 map, one turned slightly.
        let (w, h) = (64, 32);
        let mut prob = vec![0.0f32; w * h];
        for x in 4..40 {
            for y in 4..10 {
                prob[y * w + x] = 0.9;
            }
        }
        for x in 10..60 {
            let y0 = 20 + (x - 10) / 10;
            for y in y0..y0 + 5 {
                prob[y * w + x] = 0.8;
            }
        }
        // Noise too weak to be a line.
        prob[30 * w + 2] = 0.5;
        let lines = find_lines(&prob, w, h);
        assert_eq!(lines.len(), 2);
        let a = lines.iter().find(|l| l.cy < 16.0).unwrap();
        assert!(a.cos > 0.999 && (a.cx - 21.5).abs() < 0.6, "{a:?}");
        // Grown beyond the bar on every side.
        assert!(a.half_w > 18.0 && a.half_h > 3.0);
        let b = lines.iter().find(|l| l.cy > 16.0).unwrap();
        assert!(b.sin > 0.05, "{b:?}");
    }

    #[test]
    fn words_keep_what_winansi_writes() {
        assert_eq!(writable("perché Łódź"), "perché Lódz");
        assert_eq!(writable("a?b"), "a?b");
    }

    #[test]
    fn turned_rectangles() {
        let t = Turned {
            cx: 10.0,
            cy: 5.0,
            half_w: 4.0,
            half_h: 2.0,
            cos: 1.0,
            sin: 0.0,
        };
        assert_eq!(t.quad(), [[6.0, 3.0], [14.0, 3.0], [14.0, 7.0], [6.0, 7.0]]);
        assert_eq!(t.at(0.5, 0.5), [10.0, 5.0]);
        assert_eq!(t.at(1.0, 0.0), [14.0, 3.0]);
    }
}
