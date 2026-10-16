//! Comment styles and appearance streams generated from the annotation dictionary.
//!
//! Every comment Folitecta draws gets its normal appearance (`/AP /N`) from
//! [`Document::generate_appearance`], which reads the geometry and style entries
//! of the dictionary (`/Rect`, `/C`, `/IC`, `/CA`, `/BS`, `/LE`, `/L`,
//! `/Vertices`, `/InkList`, `/QuadPoints`, `/DA`, `/Q`, `/Contents`). Changing a
//! style, moving or resizing therefore always yields a crisp appearance.

use lopdf::{Dictionary, Object, ObjectId, Stream, dictionary};

use crate::document::Document;
use crate::error::{Error, Result};
use crate::textdoc::wrap_text;
use crate::util::{deref, fmt_num, helvetica_width, num, pdf_literal, rect_of};

/// Line ending styles (PDF `/LE` names).
pub const LINE_ENDINGS: &[&str] = &[
    "None",
    "OpenArrow",
    "ClosedArrow",
    "ROpenArrow",
    "RClosedArrow",
    "Square",
    "Circle",
    "Diamond",
    "Butt",
    "Slash",
];

/// Comment subtypes whose appearance Folitecta generates.
pub const GENERATED_SUBTYPES: &[&str] = &[
    "Square",
    "Circle",
    "Line",
    "PolyLine",
    "Polygon",
    "Ink",
    "Highlight",
    "Underline",
    "StrikeOut",
    "Squiggly",
    "FreeText",
    "Text",
    "Stamp",
    "Redact",
    "Caret",
    "FileAttachment",
];

/// The visual style of a comment.
#[derive(Debug, Clone, PartialEq)]
pub struct AnnotStyle {
    /// Stroke / main colour (`/C`); `None` draws no outline.
    pub color: Option<[f32; 3]>,
    /// Interior colour (`/IC`); `None` leaves the inside transparent.
    pub fill: Option<[f32; 3]>,
    /// Constant opacity, 0 to 1 (`/CA`).
    pub opacity: f32,
    /// Border or line width in points (`/BS /W`).
    pub width: f32,
    /// Dashed border (`/BS /S /D`).
    pub dashed: bool,
    /// Line start and end styles (`/LE`), see [`LINE_ENDINGS`].
    pub line_start: String,
    pub line_end: String,
    /// Text size and colour of text boxes (`/DA`).
    pub font_size: f32,
    pub text_color: [f32; 3],
    /// Text alignment of text boxes: 0 left, 1 centre, 2 right (`/Q`).
    pub align: u8,
}

impl Default for AnnotStyle {
    fn default() -> Self {
        AnnotStyle {
            color: Some([0.0, 0.0, 0.0]),
            fill: None,
            opacity: 1.0,
            width: 1.0,
            dashed: false,
            line_start: "None".into(),
            line_end: "None".into(),
            font_size: 12.0,
            text_color: [0.0, 0.0, 0.0],
            align: 0,
        }
    }
}

fn name_of(d: &Dictionary, key: &[u8]) -> Option<String> {
    d.get(key)
        .ok()
        .and_then(|n| n.as_name().ok())
        .map(|n| String::from_utf8_lossy(n).to_string())
}

fn numbers(doc: &lopdf::Document, obj: Option<&Object>) -> Vec<f32> {
    obj.map(|o| deref(doc, o))
        .and_then(|o| o.as_array().ok())
        .map(|a| a.iter().filter_map(|v| num(deref(doc, v))).collect())
        .unwrap_or_default()
}

/// A colour array with 1 (gray), 3 (RGB) or 4 (CMYK) components; empty means none.
fn color_from(doc: &lopdf::Document, obj: Option<&Object>) -> Option<[f32; 3]> {
    let v = numbers(doc, obj);
    match v.len() {
        1 => Some([v[0], v[0], v[0]]),
        3 => Some([v[0], v[1], v[2]]),
        4 => {
            let k = 1.0 - v[3];
            Some([(1.0 - v[0]) * k, (1.0 - v[1]) * k, (1.0 - v[2]) * k])
        }
        _ => None,
    }
}

fn color_obj(c: Option<[f32; 3]>) -> Object {
    match c {
        Some(c) => Object::Array(vec![c[0].into(), c[1].into(), c[2].into()]),
        None => Object::Array(Vec::new()),
    }
}

/// Font size and fill colour from a default appearance string (`/Helv 12 Tf 1 0 0 rg`).
pub fn parse_da(da: &str) -> (f32, [f32; 3]) {
    let tokens: Vec<&str> = da.split_whitespace().collect();
    let mut size = 12.0;
    let mut color = [0.0, 0.0, 0.0];
    for (i, t) in tokens.iter().enumerate() {
        let n = |k: usize| tokens.get(i.wrapping_sub(k)).and_then(|s| s.parse::<f32>().ok());
        match *t {
            "Tf" => size = n(1).filter(|v| *v > 0.0).unwrap_or(size),
            "rg" => {
                if let (Some(r), Some(g), Some(b)) = (n(3), n(2), n(1)) {
                    color = [r, g, b];
                }
            }
            "g" => {
                if let Some(v) = n(1) {
                    color = [v, v, v];
                }
            }
            "k" => {
                if let (Some(c), Some(m), Some(y), Some(k)) = (n(4), n(3), n(2), n(1)) {
                    let w = 1.0 - k;
                    color = [(1.0 - c) * w, (1.0 - m) * w, (1.0 - y) * w];
                }
            }
            _ => {}
        }
    }
    (size, color)
}

fn rgb(c: [f32; 3], op: &str) -> String {
    format!("{} {} {} {op} ", fmt_num(c[0]), fmt_num(c[1]), fmt_num(c[2]))
}

/// Path of an ellipse inscribed in a rectangle (four Bézier curves).
fn ellipse(x0: f32, y0: f32, x1: f32, y1: f32) -> String {
    let k = 0.5523;
    let (cx, cy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
    let (rx, ry) = ((x1 - x0) / 2.0, (y1 - y0) / 2.0);
    let (ox, oy) = (rx * k, ry * k);
    let f = fmt_num;
    format!(
        "{} {} m {} {} {} {} {} {} c {} {} {} {} {} {} c {} {} {} {} {} {} c {} {} {} {} {} {} c h ",
        f(cx + rx),
        f(cy),
        f(cx + rx),
        f(cy + oy),
        f(cx + ox),
        f(cy + ry),
        f(cx),
        f(cy + ry),
        f(cx - ox),
        f(cy + ry),
        f(cx - rx),
        f(cy + oy),
        f(cx - rx),
        f(cy),
        f(cx - rx),
        f(cy - oy),
        f(cx - ox),
        f(cy - ry),
        f(cx),
        f(cy - ry),
        f(cx + ox),
        f(cy - ry),
        f(cx + rx),
        f(cy - oy),
        f(cx + rx),
        f(cy)
    )
}

fn poly(points: &[(f32, f32)], close: bool) -> String {
    let mut s = String::new();
    for (i, (x, y)) in points.iter().enumerate() {
        s.push_str(&format!(
            "{} {} {} ",
            fmt_num(*x),
            fmt_num(*y),
            if i == 0 { "m" } else { "l" }
        ));
    }
    if close {
        s.push_str("h ");
    }
    s
}

/// Size of a line ending for a stroke width.
fn ending_size(width: f32) -> f32 {
    (width * 4.0).max(7.0)
}

/// Drawing operators for one line ending at `tip`; `dir` is the unit vector
/// pointing from the line towards the tip. Closed shapes are filled when
/// `filled` (with the current fill colour) and always stroked.
fn line_ending(kind: &str, tip: (f32, f32), dir: (f32, f32), width: f32, filled: bool) -> String {
    let s = ending_size(width);
    let (dx, dy) = dir;
    let (px, py) = (-dy, dx);
    let at = |along: f32, across: f32| (tip.0 + dx * along + px * across, tip.1 + dy * along + py * across);
    let paint = if filled { "b " } else { "s " };
    match kind {
        "OpenArrow" => poly(&[at(-s, s * 0.5), tip, at(-s, -s * 0.5)], false) + "S ",
        "ClosedArrow" => poly(&[at(-s, s * 0.5), tip, at(-s, -s * 0.5)], true) + paint,
        "ROpenArrow" => poly(&[at(s, s * 0.5), tip, at(s, -s * 0.5)], false) + "S ",
        "RClosedArrow" => poly(&[at(s, s * 0.5), tip, at(s, -s * 0.5)], true) + paint,
        "Butt" => poly(&[at(0.0, s * 0.5), at(0.0, -s * 0.5)], false) + "S ",
        "Slash" => poly(&[at(-s * 0.25, -s * 0.45), at(s * 0.25, s * 0.45)], false) + "S ",
        "Square" => {
            poly(
                &[
                    at(-s * 0.3, -s * 0.3),
                    at(s * 0.3, -s * 0.3),
                    at(s * 0.3, s * 0.3),
                    at(-s * 0.3, s * 0.3),
                ],
                true,
            ) + paint
        }
        "Diamond" => {
            poly(
                &[
                    at(-s * 0.4, 0.0),
                    at(0.0, -s * 0.4),
                    at(s * 0.4, 0.0),
                    at(0.0, s * 0.4),
                ],
                true,
            ) + paint
        }
        "Circle" => {
            let r = s * 0.3;
            ellipse(tip.0 - r, tip.1 - r, tip.0 + r, tip.1 + r) + paint
        }
        _ => String::new(),
    }
}

fn unit(from: (f32, f32), to: (f32, f32)) -> (f32, f32) {
    let (dx, dy) = (to.0 - from.0, to.1 - from.1);
    let len = (dx * dx + dy * dy).sqrt().max(0.0001);
    (dx / len, dy / len)
}

fn bounds(points: &[(f32, f32)], pad: f32) -> [f32; 4] {
    let mut r = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
    for (x, y) in points {
        r = [r[0].min(*x), r[1].min(*y), r[2].max(*x), r[3].max(*y)];
    }
    [r[0] - pad, r[1] - pad, r[2] + pad, r[3] + pad]
}

fn pairs(v: &[f32]) -> Vec<(f32, f32)> {
    v.as_chunks::<2>().0.iter().map(|c| (c[0], c[1])).collect()
}

/// Cloudy border intensity (`/BE /S /C /I n`), if the annotation has one.
fn cloud_intensity(doc: &lopdf::Document, d: &Dictionary) -> Option<f32> {
    let be = d
        .get(b"BE")
        .ok()
        .map(|b| deref(doc, b))
        .and_then(|b| b.as_dict().ok())?;
    if name_of(be, b"S").as_deref() != Some("C") {
        return None;
    }
    Some(
        be.get(b"I")
            .ok()
            .and_then(|i| num(deref(doc, i)))
            .unwrap_or(1.0)
            .clamp(0.0, 2.0),
    )
}

fn signed_area(pts: &[(f32, f32)]) -> f32 {
    let n = pts.len();
    (0..n)
        .map(|i| {
            let (a, b) = (pts[i], pts[(i + 1) % n]);
            a.0 * b.1 - b.0 * a.1
        })
        .sum::<f32>()
        / 2.0
}

/// Scalloped outline around a closed polygon; returns the path and the bump radius.
fn cloud_path(pts: &[(f32, f32)], intensity: f32) -> (String, f32) {
    let n = pts.len();
    if n < 2 {
        return (String::new(), 0.0);
    }
    let ccw = signed_area(pts) >= 0.0;
    let target = 10.0 + 6.0 * intensity;
    let f = fmt_num;
    let mut s = String::new();
    let mut max_r: f32 = 0.0;
    let mut first = true;
    for i in 0..n {
        let (a, b) = (pts[i], pts[(i + 1) % n]);
        let len = ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt();
        if len < 0.01 {
            continue;
        }
        let k = ((len / target).round() as usize).max(1);
        let (dx, dy) = ((b.0 - a.0) / len, (b.1 - a.1) / len);
        let (nx, ny) = if ccw { (dy, -dx) } else { (-dy, dx) };
        for j in 0..k {
            let t0 = j as f32 / k as f32;
            let t1 = (j + 1) as f32 / k as f32;
            let p = (a.0 + (b.0 - a.0) * t0, a.1 + (b.1 - a.1) * t0);
            let q = (a.0 + (b.0 - a.0) * t1, a.1 + (b.1 - a.1) * t1);
            let r = len / k as f32 / 2.0;
            max_r = max_r.max(r);
            let h = r * 4.0 / 3.0;
            if first {
                s.push_str(&format!("{} {} m ", f(p.0), f(p.1)));
                first = false;
            }
            s.push_str(&format!(
                "{} {} {} {} {} {} c ",
                f(p.0 + nx * h),
                f(p.1 + ny * h),
                f(q.0 + nx * h),
                f(q.1 + ny * h),
                f(q.0),
                f(q.1)
            ));
        }
    }
    s.push_str("h ");
    (s, max_r)
}

/// A number format of a measure dictionary (`/X`, `/A`, ...).
struct NumFmt {
    unit: String,
    factor: f32,
    decimals: usize,
}

fn num_fmt(doc: &lopdf::Document, measure: &Dictionary, key: &[u8]) -> Option<NumFmt> {
    let arr = measure
        .get(key)
        .ok()
        .map(|a| deref(doc, a))
        .and_then(|a| a.as_array().ok())?;
    let first = arr
        .first()
        .map(|f| deref(doc, f))
        .and_then(|f| f.as_dict().ok())?;
    let unit = first
        .get(b"U")
        .ok()
        .and_then(|u| crate::util::text_of(doc, u))
        .unwrap_or_default();
    let factor = first
        .get(b"C")
        .ok()
        .and_then(|c| num(deref(doc, c)))
        .unwrap_or(1.0);
    let denominator = first
        .get(b"D")
        .ok()
        .and_then(|v| num(deref(doc, v)))
        .unwrap_or(100.0)
        .max(1.0);
    Some(NumFmt {
        unit,
        factor,
        decimals: denominator.log10().round().clamp(0.0, 6.0) as usize,
    })
}

fn path_length(pts: &[(f32, f32)], closed: bool) -> f32 {
    let mut total: f32 = pts
        .windows(2)
        .map(|w| ((w[1].0 - w[0].0).powi(2) + (w[1].1 - w[0].1).powi(2)).sqrt())
        .sum();
    if closed && pts.len() > 2 {
        let (a, b) = (pts[pts.len() - 1], pts[0]);
        total += ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt();
    }
    total
}

/// Measurement text and its anchor for a dimension annotation (`/IT ...Dimension`).
fn measure_label(doc: &lopdf::Document, d: &Dictionary, pts: &[(f32, f32)]) -> Option<(String, (f32, f32))> {
    let it = name_of(d, b"IT")?;
    let measure = d
        .get(b"Measure")
        .ok()
        .map(|m| deref(doc, m))
        .and_then(|m| m.as_dict().ok())?;
    let n = pts.len();
    let fmt = |v: f32, nf: &NumFmt| {
        format!("{:.*} {}", nf.decimals, v * nf.factor, nf.unit)
            .trim()
            .to_string()
    };
    match it.as_str() {
        "LineDimension" if n >= 2 => {
            let x = num_fmt(doc, measure, b"D").or_else(|| num_fmt(doc, measure, b"X"))?;
            let mid = ((pts[0].0 + pts[n - 1].0) / 2.0, (pts[0].1 + pts[n - 1].1) / 2.0);
            Some((fmt(path_length(pts, false), &x), mid))
        }
        "PolyLineDimension" if n >= 2 => {
            let x = num_fmt(doc, measure, b"D").or_else(|| num_fmt(doc, measure, b"X"))?;
            Some((fmt(path_length(pts, false), &x), pts[n - 1]))
        }
        "PolygonDimension" if n >= 3 => {
            let a = num_fmt(doc, measure, b"A")?;
            let c = pts.iter().fold((0.0, 0.0), |acc, p| {
                (acc.0 + p.0 / n as f32, acc.1 + p.1 / n as f32)
            });
            Some((fmt(signed_area(pts).abs(), &a), c))
        }
        _ => None,
    }
}

/// Text box contents: background, border and wrapped text inside `bx`.
fn free_text_body(style: &AnnotStyle, bx: [f32; 4], text: &str) -> String {
    let f = fmt_num;
    let w = style.width.max(0.0);
    let mut body = String::new();
    if let Some(c) = style.fill {
        body.push_str(&rgb(c, "rg"));
        body.push_str(&format!(
            "{} {} {} {} re f ",
            f(bx[0]),
            f(bx[1]),
            f(bx[2] - bx[0]),
            f(bx[3] - bx[1])
        ));
    }
    if w > 0.0 {
        if let Some(c) = style.color {
            body.push_str(&format!("{} w ", f(w)));
            if style.dashed {
                body.push_str(&format!(
                    "[{} {}] 0 d ",
                    f((w * 3.0).max(3.0)),
                    f((w * 2.0).max(2.0))
                ));
            }
            body.push_str(&rgb(c, "RG"));
            let h = w / 2.0;
            body.push_str(&format!(
                "{} {} {} {} re S [] 0 d ",
                f(bx[0] + h),
                f(bx[1] + h),
                f(bx[2] - bx[0] - w),
                f(bx[3] - bx[1] - w)
            ));
        }
    }
    let fs = style.font_size.clamp(2.0, 400.0);
    let pad = 2.0 + w;
    let inner_w = (bx[2] - bx[0] - 2.0 * pad).max(fs);
    body.push_str(&format!(
        "q {} {} {} {} re W n ",
        f(bx[0] + w),
        f(bx[1] + w),
        f((bx[2] - bx[0] - 2.0 * w).max(0.0)),
        f((bx[3] - bx[1] - 2.0 * w).max(0.0))
    ));
    body.push_str(&format!("BT /Helv {} Tf ", f(fs)));
    body.push_str(&rgb(style.text_color, "rg"));
    let mut y = bx[3] - pad - fs * 0.88;
    for line in wrap_text(text, fs, inner_w) {
        let lw = helvetica_width(&line, fs);
        let x = match style.align {
            1 => bx[0] + pad + (inner_w - lw) / 2.0,
            2 => bx[2] - pad - lw,
            _ => bx[0] + pad,
        };
        body.push_str(&format!("1 0 0 1 {} {} Tm ", f(x), f(y)));
        body.push_str(&String::from_utf8_lossy(&pdf_literal(&line)));
        body.push_str(" Tj ");
        y -= fs * 1.2;
    }
    body.push_str("ET Q ");
    body
}

/// Vertices of an ellipse inscribed in a rectangle.
fn ellipse_points(x0: f32, y0: f32, x1: f32, y1: f32, n: usize) -> Vec<(f32, f32)> {
    let (cx, cy, rx, ry) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0, (x1 - x0) / 2.0, (y1 - y0) / 2.0);
    (0..n)
        .map(|i| {
            let a = i as f32 / n as f32 * std::f32::consts::TAU;
            (cx + rx * a.cos(), cy + ry * a.sin())
        })
        .collect()
}

impl Document {
    /// Style of an annotation (defaults for missing entries).
    pub fn annotation_style(&self, page: usize, index: usize) -> Result<AnnotStyle> {
        let d = self.annotation_dict(page, index)?;
        Ok(self.style_of(&d))
    }

    fn style_of(&self, d: &Dictionary) -> AnnotStyle {
        let doc = &self.inner;
        let bs = d
            .get(b"BS")
            .ok()
            .map(|b| deref(doc, b))
            .and_then(|b| b.as_dict().ok());
        let width = bs
            .and_then(|b| b.get(b"W").ok())
            .and_then(|w| num(deref(doc, w)))
            .unwrap_or(1.0);
        let dashed = bs.and_then(|b| name_of(b, b"S")).as_deref() == Some("D");
        let le = match d.get(b"LE").ok().map(|l| deref(doc, l)) {
            Some(Object::Array(a)) => a.clone(),
            Some(Object::Name(n)) => vec![Object::Name(b"None".to_vec()), Object::Name(n.clone())],
            _ => Vec::new(),
        };
        let le_name = |i: usize| {
            le.get(i)
                .and_then(|n| n.as_name().ok())
                .map(|n| String::from_utf8_lossy(n).to_string())
                .unwrap_or_else(|| "None".into())
        };
        let da = d
            .get(b"DA")
            .ok()
            .and_then(|o| crate::util::text_of(doc, o))
            .unwrap_or_default();
        let (font_size, text_color) = parse_da(&da);
        AnnotStyle {
            color: color_from(doc, d.get(b"C").ok()),
            fill: color_from(doc, d.get(b"IC").ok()),
            opacity: d
                .get(b"CA")
                .ok()
                .and_then(|v| num(deref(doc, v)))
                .unwrap_or(1.0)
                .clamp(0.0, 1.0),
            width,
            dashed,
            line_start: le_name(0),
            line_end: le_name(1),
            font_size,
            text_color,
            align: d
                .get(b"Q")
                .ok()
                .and_then(|q| q.as_i64().ok())
                .unwrap_or(0)
                .clamp(0, 2) as u8,
        }
    }

    /// Clone of an annotation dictionary (the annotation must be an indirect object).
    fn annotation_dict(&self, page: usize, index: usize) -> Result<Dictionary> {
        let id = self.annotation_object(page, index)?;
        Ok(self.inner.get_dictionary(id)?.clone())
    }

    /// Object id of an annotation by page and position.
    pub(crate) fn annotation_object(&self, page: usize, index: usize) -> Result<ObjectId> {
        self.annotations(page)?
            .get(index)
            .and_then(|a| a.id)
            .ok_or_else(|| Error::Invalid(format!("annotation {index} cannot be edited")))
    }

    /// Change the style of a comment and regenerate its appearance.
    pub fn set_annotation_style(&mut self, page: usize, index: usize, style: &AnnotStyle) -> Result<()> {
        let id = self.annotation_object(page, index)?;
        let mut d = self.inner.get_dictionary(id)?.clone();
        let subtype = name_of(&d, b"Subtype").unwrap_or_default();
        if !GENERATED_SUBTYPES.contains(&subtype.as_str()) {
            return Err(Error::Invalid(format!(
                "the style of {subtype} comments cannot be changed"
            )));
        }
        self.begin_edit();
        apply_style(&mut d, &subtype, style);
        self.generate_appearance(&mut d)?;
        d.set("M", Object::string_literal(crate::util::pdf_date_now()));
        *self.inner.get_dictionary_mut(id)? = d;
        self.commit()
    }

    /// Regenerate the appearance of an annotation object in place (no undo step).
    /// Returns `false` for subtypes Folitecta does not draw.
    pub(crate) fn refresh_appearance(&mut self, id: ObjectId) -> Result<bool> {
        let mut d = self.inner.get_dictionary(id)?.clone();
        let generated = self.generate_appearance(&mut d)?;
        if generated {
            *self.inner.get_dictionary_mut(id)? = d;
        }
        Ok(generated)
    }

    /// Build the normal appearance of `d` from its entries and store it in `/AP`.
    /// Updates `/Rect` when the geometry needs room (line endings). Returns
    /// `false` (leaving `d` untouched) for unsupported subtypes.
    pub(crate) fn generate_appearance(&mut self, d: &mut Dictionary) -> Result<bool> {
        let subtype = name_of(d, b"Subtype").unwrap_or_default();
        if !GENERATED_SUBTYPES.contains(&subtype.as_str()) {
            return Ok(false);
        }
        let doc = &self.inner;
        let style = self.style_of(d);
        let mut rect = d
            .get(b"Rect")
            .ok()
            .and_then(|r| rect_of(doc, r))
            .unwrap_or([0.0; 4]);
        rect = [
            rect[0].min(rect[2]),
            rect[1].min(rect[3]),
            rect[0].max(rect[2]),
            rect[1].max(rect[3]),
        ];
        let w = style.width.max(0.0);
        let f = fmt_num;
        let mut body = String::new();
        let mut resources = Dictionary::new();
        let mut blend = "Normal";
        let stroke_setup = |body: &mut String| {
            body.push_str(&format!("{} w ", f(w)));
            if style.dashed {
                body.push_str(&format!(
                    "[{} {}] 0 d ",
                    f((w * 3.0).max(3.0)),
                    f((w * 2.0).max(2.0))
                ));
            }
            if let Some(c) = style.color {
                body.push_str(&rgb(c, "RG"));
            }
            if let Some(c) = style.fill {
                body.push_str(&rgb(c, "rg"));
            }
        };
        match subtype.as_str() {
            "Square" | "Circle" => {
                stroke_setup(&mut body);
                let paint = match (style.fill.is_some(), style.color.is_some() && w > 0.0) {
                    (true, true) => "B ",
                    (true, false) => "f ",
                    (false, true) => "S ",
                    (false, false) => "n ",
                };
                if let Some(intensity) = cloud_intensity(doc, d) {
                    // Bumps grow outwards: keep them inside the rectangle.
                    let inset = 5.0 + 3.0 * intensity + w;
                    let (x0, y0, x1, y1) =
                        (rect[0] + inset, rect[1] + inset, rect[2] - inset, rect[3] - inset);
                    let pts = if subtype == "Square" {
                        vec![(x0, y0), (x1, y0), (x1, y1), (x0, y1)]
                    } else {
                        ellipse_points(x0, y0, x1, y1, 24)
                    };
                    body.push_str("1 j ");
                    body.push_str(&cloud_path(&pts, intensity).0);
                } else {
                    let h = w / 2.0;
                    let (x0, y0, x1, y1) = (rect[0] + h, rect[1] + h, rect[2] - h, rect[3] - h);
                    if subtype == "Square" {
                        body.push_str(&format!(
                            "{} {} {} {} re ",
                            f(x0),
                            f(y0),
                            f((x1 - x0).max(0.0)),
                            f((y1 - y0).max(0.0))
                        ));
                    } else {
                        body.push_str(&ellipse(x0, y0, x1, y1));
                    }
                }
                body.push_str(paint);
            }
            "Line" | "PolyLine" | "Polygon" => {
                let key: &[u8] = if subtype == "Line" { b"L" } else { b"Vertices" };
                let pts = pairs(&numbers(doc, d.get(key).ok()));
                if pts.len() < 2 {
                    return Ok(false);
                }
                let closed = subtype == "Polygon";
                let cloud = if closed { cloud_intensity(doc, d) } else { None };
                let label = measure_label(doc, d, &pts);
                body.push_str("1 J 1 j ");
                stroke_setup(&mut body);
                let mut pad = ending_size(w) + w + 1.0;
                match cloud {
                    Some(intensity) => {
                        let (path, r) = cloud_path(&pts, intensity);
                        body.push_str(&path);
                        pad = pad.max(r * 1.05 + w + 1.0);
                    }
                    None => body.push_str(&poly(&pts, closed)),
                }
                body.push_str(if closed && style.fill.is_some() {
                    "B "
                } else {
                    "S "
                });
                if !closed {
                    body.push_str("[] 0 d ");
                    let n = pts.len();
                    let filled = style.fill.is_some();
                    body.push_str(&line_ending(
                        &style.line_start,
                        pts[0],
                        unit(pts[1], pts[0]),
                        w,
                        filled,
                    ));
                    body.push_str(&line_ending(
                        &style.line_end,
                        pts[n - 1],
                        unit(pts[n - 2], pts[n - 1]),
                        w,
                        filled,
                    ));
                }
                rect = bounds(&pts, pad);
                if let Some((text, at)) = label {
                    let font = self.helvetica_font_id();
                    resources.set("Font", dictionary! { "Helv" => font });
                    let fs = style.font_size.clamp(5.0, 48.0);
                    let tw = helvetica_width(&text, fs);
                    let (bx, by) = (at.0 - tw / 2.0 - 3.0, at.1 + fs * 0.35);
                    let c = style.color.unwrap_or([0.0, 0.0, 0.0]);
                    body.push_str("1 1 1 rg ");
                    body.push_str(&format!(
                        "{} {} {} {} re f ",
                        f(bx),
                        f(by),
                        f(tw + 6.0),
                        f(fs + 4.0)
                    ));
                    body.push_str(&format!("BT /Helv {} Tf ", f(fs)));
                    body.push_str(&rgb(c, "rg"));
                    body.push_str(&format!("{} {} Td ", f(bx + 3.0), f(by + 2.0 + fs * 0.18)));
                    body.push_str(&String::from_utf8_lossy(&pdf_literal(&text)));
                    body.push_str(" Tj ET ");
                    rect = [
                        rect[0].min(bx),
                        rect[1].min(by),
                        rect[2].max(bx + tw + 6.0),
                        rect[3].max(by + fs + 4.0),
                    ];
                    d.set("Contents", lopdf::text_string(&text));
                }
            }
            "Ink" => {
                let paths: Vec<Vec<(f32, f32)>> = d
                    .get(b"InkList")
                    .ok()
                    .map(|l| deref(doc, l))
                    .and_then(|l| l.as_array().ok())
                    .map(|paths| paths.iter().map(|p| pairs(&numbers(doc, Some(p)))).collect())
                    .unwrap_or_default();
                if paths.iter().all(|p| p.is_empty()) {
                    return Ok(false);
                }
                body.push_str("1 J 1 j ");
                stroke_setup(&mut body);
                for p in &paths {
                    if p.len() == 1 {
                        body.push_str(&poly(&[p[0], (p[0].0 + 0.01, p[0].1)], false));
                    } else {
                        body.push_str(&poly(p, false));
                    }
                    body.push_str("S ");
                }
                let all: Vec<(f32, f32)> = paths.into_iter().flatten().collect();
                rect = bounds(&all, w + 2.0);
            }
            "Highlight" | "Underline" | "StrikeOut" | "Squiggly" => {
                let quads: Vec<[f32; 8]> = numbers(doc, d.get(b"QuadPoints").ok())
                    .as_chunks::<8>()
                    .0
                    .to_vec();
                if quads.is_empty() {
                    return Ok(false);
                }
                let c = style.color.unwrap_or([1.0, 0.9, 0.2]);
                body.push_str(&rgb(c, "rg"));
                body.push_str(&rgb(c, "RG"));
                if subtype == "Highlight" {
                    blend = "Multiply";
                }
                let mut all = Vec::new();
                for q in &quads {
                    let (ul, ur, ll, lr) = ((q[0], q[1]), (q[2], q[3]), (q[4], q[5]), (q[6], q[7]));
                    all.extend([ul, ur, ll, lr]);
                    let (ux, uy) = (ul.0 - ll.0, ul.1 - ll.1);
                    let h = (ux * ux + uy * uy).sqrt().max(0.001);
                    let (nx, ny) = (ux / h, uy / h);
                    let t = (h * 0.07).max(0.8);
                    let along =
                        |a: (f32, f32), b: (f32, f32), k: f32| (a.0 + (b.0 - a.0) * k, a.1 + (b.1 - a.1) * k);
                    match subtype.as_str() {
                        "Highlight" => body.push_str(&(poly(&[ul, ur, lr, ll], true) + "f ")),
                        "Underline" => body.push_str(
                            &(poly(
                                &[
                                    ll,
                                    lr,
                                    (lr.0 + nx * t, lr.1 + ny * t),
                                    (ll.0 + nx * t, ll.1 + ny * t),
                                ],
                                true,
                            ) + "f "),
                        ),
                        "StrikeOut" => {
                            let (m0, m1) = (along(ll, ul, 0.42), along(lr, ur, 0.42));
                            body.push_str(
                                &(poly(
                                    &[
                                        m0,
                                        m1,
                                        (m1.0 + nx * t, m1.1 + ny * t),
                                        (m0.0 + nx * t, m0.1 + ny * t),
                                    ],
                                    true,
                                ) + "f "),
                            );
                        }
                        _ => {
                            // Squiggly: a zigzag along the bottom edge.
                            let len = ((lr.0 - ll.0).powi(2) + (lr.1 - ll.1).powi(2)).sqrt();
                            let step = (h * 0.12).max(1.5);
                            let n = ((len / step).floor() as usize).max(2);
                            let wave: Vec<(f32, f32)> = (0..=n)
                                .map(|i| {
                                    let p = along(ll, lr, i as f32 / n as f32);
                                    let up = if i % 2 == 0 { 0.0 } else { t * 1.6 };
                                    (p.0 + nx * up, p.1 + ny * up)
                                })
                                .collect();
                            body.push_str(&format!("{} w ", f(t * 0.8)));
                            body.push_str(&(poly(&wave, false) + "S "));
                        }
                    }
                }
                // Keep the given rectangle, enlarged only to contain the marked text.
                let q = bounds(&all, 0.0);
                rect = [
                    rect[0].min(q[0]),
                    rect[1].min(q[1]),
                    rect[2].max(q[2]),
                    rect[3].max(q[3]),
                ];
            }
            "FreeText" => {
                let text = crate::util::dict_text(doc, d, b"Contents").unwrap_or_default();
                let callout = name_of(d, b"IT").as_deref() == Some("FreeTextCallout");
                let cl = pairs(&numbers(doc, d.get(b"CL").ok()));
                let rd = numbers(doc, d.get(b"RD").ok());
                let font = self.helvetica_font_id();
                resources.set("Font", dictionary! { "Helv" => font });
                let bx = if rd.len() == 4 {
                    [rect[0] + rd[0], rect[1] + rd[1], rect[2] - rd[2], rect[3] - rd[3]]
                } else {
                    rect
                };
                if callout && cl.len() >= 2 {
                    // Leader line from the pointed spot (first point) to the box.
                    body.push_str("q 1 J 1 j ");
                    body.push_str(&format!("{} w ", f(w.max(0.5))));
                    let c = style.color.unwrap_or([0.0, 0.0, 0.0]);
                    body.push_str(&rgb(c, "RG"));
                    body.push_str(&rgb(c, "rg"));
                    body.push_str(&(poly(&cl, false) + "S "));
                    body.push_str(&line_ending(
                        &style.line_end,
                        cl[0],
                        unit(cl[1], cl[0]),
                        w.max(0.5),
                        true,
                    ));
                    body.push_str("Q ");
                }
                body.push_str(&free_text_body(&style, bx, &text));
                if callout && cl.len() >= 2 {
                    let lead = bounds(&cl, ending_size(w) + w + 1.0);
                    rect = [
                        bx[0].min(lead[0]),
                        bx[1].min(lead[1]),
                        bx[2].max(lead[2]),
                        bx[3].max(lead[3]),
                    ];
                    d.set(
                        "RD",
                        vec![
                            (bx[0] - rect[0]).into(),
                            (bx[1] - rect[1]).into(),
                            (rect[2] - bx[2]).into(),
                            (rect[3] - bx[3]).into(),
                        ],
                    );
                }
                let fs = style.font_size.clamp(2.0, 400.0);
                let (r, g, b) = (style.text_color[0], style.text_color[1], style.text_color[2]);
                d.set(
                    "DA",
                    Object::string_literal(format!("/Helv {} Tf {} {} {} rg", f(fs), f(r), f(g), f(b))),
                );
            }
            "Caret" => {
                let c = style.color.unwrap_or([0.1, 0.4, 0.95]);
                let (x0, y0, x1, y1) = (rect[0], rect[1], rect[2], rect[3]);
                let mid = (x0 + x1) / 2.0;
                body.push_str(&rgb(c, "rg"));
                body.push_str(
                    &(poly(
                        &[(x0, y0), (mid, y1), (x1, y0), (mid, y0 + (y1 - y0) * 0.35)],
                        true,
                    ) + "f "),
                );
            }
            "FileAttachment" => {
                // Paper-clip icon in the top-left corner of the rectangle.
                let c = style.color.unwrap_or([0.2, 0.4, 0.8]);
                let (x, y) = (rect[0], rect[3]);
                rect = [x, y - 20.0, x + 14.0, y];
                body.push_str(&rgb(c, "RG"));
                body.push_str("1.4 w 1 J 1 j ");
                body.push_str(&format!(
                    "{} {} m {} {} l {} {} {} {} {} {} c {} {} l {} {} {} {} {} {} c {} {} l S ",
                    f(x + 9.5),
                    f(y - 7.0),
                    f(x + 9.5),
                    f(y - 15.0),
                    f(x + 9.5),
                    f(y - 18.5),
                    f(x + 4.5),
                    f(y - 18.5),
                    f(x + 4.5),
                    f(y - 15.0),
                    f(x + 4.5),
                    f(y - 4.0),
                    f(x + 4.5),
                    f(y - 1.5),
                    f(x + 12.0),
                    f(y - 1.5),
                    f(x + 12.0),
                    f(y - 4.0),
                    f(x + 12.0),
                    f(y - 13.0)
                ));
            }
            "Text" => {
                // Sticky-note icon in the top-left corner of the rectangle.
                let c = style.color.unwrap_or([1.0, 0.85, 0.2]);
                let (x, y) = (rect[0], rect[3]);
                rect = [x, y - 20.0, x + 20.0, y];
                body.push_str(&rgb(c, "rg"));
                body.push_str("0.2 0.2 0.2 RG 1 w ");
                body.push_str(&format!("{} {} 18 14 re B ", f(x + 1.0), f(y - 15.0)));
                body.push_str(
                    &(poly(
                        &[(x + 4.0, y - 15.0), (x + 4.0, y - 19.5), (x + 9.0, y - 15.0)],
                        true,
                    ) + "B "),
                );
                body.push_str("0.25 0.25 0.25 RG 0.8 w ");
                for k in 0..3 {
                    let ly = y - 4.5 - k as f32 * 3.5;
                    body.push_str(&format!(
                        "{} {} m {} {} l S ",
                        f(x + 4.0),
                        f(ly),
                        f(x + 15.0),
                        f(ly)
                    ));
                }
            }
            "Stamp" => {
                let name = name_of(d, b"Name").unwrap_or_default();
                if name == crate::signmarks::SIGNATURE_STAMP {
                    // Handwritten signatures keep their drawing (it scales with the rectangle).
                    return Ok(false);
                }
                if name == "FolitectaImage" {
                    // Keep the picture of the existing appearance, change only the opacity.
                    let img = d
                        .get(b"AP")
                        .ok()
                        .map(|ap| deref(doc, ap))
                        .and_then(|ap| ap.as_dict().ok())
                        .and_then(|ap| ap.get(b"N").ok())
                        .map(|n| deref(doc, n))
                        .and_then(|n| n.as_stream().ok())
                        .and_then(|s| s.dict.get(b"Resources").ok())
                        .map(|r| deref(doc, r))
                        .and_then(|r| r.as_dict().ok())
                        .and_then(|r| r.get(b"XObject").ok())
                        .map(|x| deref(doc, x))
                        .and_then(|x| x.as_dict().ok())
                        .and_then(|x| x.get(b"Img").ok())
                        .cloned();
                    let Some(img) = img else { return Ok(false) };
                    resources.set("XObject", dictionary! { "Img" => img });
                    body.push_str(&format!(
                        "{} 0 0 {} {} {} cm /Img Do ",
                        f(rect[2] - rect[0]),
                        f(rect[3] - rect[1]),
                        f(rect[0]),
                        f(rect[1])
                    ));
                } else if name.starts_with("Folitecta") || !d.has(b"AP") || appearance_is_ours(doc, d) {
                    // Folitecta text stamps show their text; standard stamps without
                    // an appearance of their own show the stamp name.
                    let text = if name.starts_with("Folitecta") {
                        crate::util::dict_text(doc, d, b"Contents").unwrap_or_default()
                    } else {
                        stamp_label(&name)
                    };
                    let font = self.helvetica_font_id();
                    resources.set("Font", dictionary! { "Helv" => font });
                    let c = style.color.unwrap_or([0.8, 0.1, 0.1]);
                    let (bw, bh) = (rect[2] - rect[0], rect[3] - rect[1]);
                    let unit_w = helvetica_width(&text, 1.0).max(1.0);
                    let fs = (bh * 0.55).min((bw - 16.0).max(4.0) / unit_w).max(4.0);
                    let tw = helvetica_width(&text, fs);
                    body.push_str(&rgb(c, "RG"));
                    body.push_str(&rgb(c, "rg"));
                    if let Some(fc) = style.fill {
                        body.push_str(&rgb(fc, "rg"));
                        body.push_str(&format!(
                            "{} {} {} {} re f ",
                            f(rect[0] + 1.0),
                            f(rect[1] + 1.0),
                            f(bw - 2.0),
                            f(bh - 2.0)
                        ));
                        body.push_str(&rgb(c, "rg"));
                    }
                    body.push_str(&format!(
                        "{} w {} {} {} {} re S ",
                        f(w.max(0.5) * 2.0),
                        f(rect[0] + 1.0),
                        f(rect[1] + 1.0),
                        f(bw - 2.0),
                        f(bh - 2.0)
                    ));
                    body.push_str(&format!(
                        "BT /Helv {} Tf {} {} Td ",
                        f(fs),
                        f(rect[0] + (bw - tw) / 2.0),
                        f(rect[1] + (bh - fs * 0.72) / 2.0)
                    ));
                    body.push_str(&String::from_utf8_lossy(&pdf_literal(&text)));
                    body.push_str(" Tj ET ");
                } else {
                    return Ok(false);
                }
            }
            "Redact" => {
                let outline = style.color.unwrap_or([1.0, 0.0, 0.0]);
                body.push_str(&rgb(outline, "rg"));
                body.push_str("/GS1 gs ");
                body.push_str(&format!(
                    "{} {} {} {} re f ",
                    f(rect[0]),
                    f(rect[1]),
                    f(rect[2] - rect[0]),
                    f(rect[3] - rect[1])
                ));
                body.push_str("/GS0 gs ");
                body.push_str(&rgb(outline, "RG"));
                body.push_str(&format!(
                    "1.5 w {} {} {} {} re S ",
                    f(rect[0] + 0.75),
                    f(rect[1] + 0.75),
                    f(rect[2] - rect[0] - 1.5),
                    f(rect[3] - rect[1] - 1.5)
                ));
            }
            _ => return Ok(false),
        }
        let mut gs = dictionary! { "GS0" => dictionary! { "CA" => style.opacity, "ca" => style.opacity, "BM" => blend } };
        if subtype == "Redact" {
            gs.set(
                "GS1",
                dictionary! { "CA" => style.opacity, "ca" => 0.18 * style.opacity },
            );
        }
        resources.set("ExtGState", gs);
        let content = format!("q /GS0 gs {body}Q");
        let stream = Stream::new(
            dictionary! {
                "Type" => "XObject",
                "Subtype" => "Form",
                "BBox" => vec![rect[0].into(), rect[1].into(), rect[2].into(), rect[3].into()],
                "Resources" => resources,
                AP_MARKER => true,
            },
            content.into_bytes(),
        );
        let ap = self.inner.add_object(stream);
        d.set("AP", dictionary! { "N" => ap });
        d.set(
            "Rect",
            vec![rect[0].into(), rect[1].into(), rect[2].into(), rect[3].into()],
        );
        d.remove(b"AS");
        Ok(true)
    }
}

/// Key marking appearance streams generated by Folitecta.
const AP_MARKER: &str = "FolitectaAP";

/// Whether the normal appearance of `d` was generated by Folitecta.
fn appearance_is_ours(doc: &lopdf::Document, d: &Dictionary) -> bool {
    d.get(b"AP")
        .ok()
        .map(|ap| deref(doc, ap))
        .and_then(|ap| ap.as_dict().ok())
        .and_then(|ap| ap.get(b"N").ok())
        .map(|n| deref(doc, n))
        .and_then(|n| n.as_stream().ok())
        .map(|s| s.dict.has(AP_MARKER.as_bytes()))
        .unwrap_or(false)
}

/// Readable label of a standard stamp name (`NotApproved` → `NOT APPROVED`).
pub(crate) fn stamp_label(name: &str) -> String {
    let mut n = name.trim_start_matches('#');
    for prefix in ["SB", "SH", "D"] {
        if let Some(rest) = n.strip_prefix(prefix) {
            if rest.len() > 1
                && rest.starts_with(|c: char| c.is_ascii_uppercase())
                && rest.chars().nth(1).map(|c| c.is_lowercase()).unwrap_or(false)
            {
                n = rest;
                break;
            }
        }
    }
    let mut out = String::new();
    let chars: Vec<char> = n.chars().collect();
    for (i, c) in chars.iter().enumerate() {
        if i > 0 && c.is_uppercase() && chars[i - 1].is_lowercase() {
            out.push(' ');
        }
        if *c == '_' || *c == '-' {
            out.push(' ');
        } else {
            out.extend(c.to_uppercase());
        }
    }
    if out.trim().is_empty() {
        "STAMP".into()
    } else {
        out
    }
}

/// Write `style` into the dictionary entries of a comment of `subtype`.
pub(crate) fn apply_style(d: &mut Dictionary, subtype: &str, style: &AnnotStyle) {
    d.set("C", color_obj(style.color));
    match style.fill {
        Some(c) => d.set("IC", color_obj(Some(c))),
        None => {
            d.remove(b"IC");
        }
    }
    d.set("CA", style.opacity.clamp(0.0, 1.0));
    let mut bs = dictionary! { "W" => style.width.max(0.0), "S" => if style.dashed { "D" } else { "S" } };
    if style.dashed {
        let w = style.width.max(0.5);
        bs.set("D", vec![(w * 3.0).max(3.0).into(), (w * 2.0).max(2.0).into()]);
    }
    d.set("BS", bs);
    if matches!(subtype, "Line" | "PolyLine") {
        d.set(
            "LE",
            vec![
                Object::Name(style.line_start.as_bytes().to_vec()),
                Object::Name(style.line_end.as_bytes().to_vec()),
            ],
        );
    }
    if subtype == "FreeText" {
        let f = fmt_num;
        let c = style.text_color;
        d.set(
            "DA",
            Object::string_literal(format!(
                "/Helv {} Tf {} {} {} rg",
                f(style.font_size),
                f(c[0]),
                f(c[1]),
                f(c[2])
            )),
        );
        d.set("Q", style.align.min(2) as i64);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_appearance_strings_parse() {
        assert_eq!(parse_da("/Helv 14 Tf 1 0 0 rg"), (14.0, [1.0, 0.0, 0.0]));
        assert_eq!(parse_da("0.5 g /F1 9 Tf"), (9.0, [0.5, 0.5, 0.5]));
        assert_eq!(parse_da(""), (12.0, [0.0, 0.0, 0.0]));
    }

    #[test]
    fn standard_stamp_names_become_labels() {
        assert_eq!(stamp_label("NotApproved"), "NOT APPROVED");
        assert_eq!(stamp_label("SBForPublicRelease"), "FOR PUBLIC RELEASE");
        assert_eq!(stamp_label("#DApproved"), "APPROVED");
        assert_eq!(stamp_label("Draft"), "DRAFT");
        assert_eq!(stamp_label(""), "STAMP");
    }

    #[test]
    fn every_line_ending_draws_something() {
        for kind in LINE_ENDINGS.iter().filter(|k| **k != "None") {
            assert!(
                !line_ending(kind, (10.0, 10.0), (1.0, 0.0), 1.0, true).is_empty(),
                "{kind}"
            );
        }
        assert!(line_ending("None", (0.0, 0.0), (1.0, 0.0), 1.0, false).is_empty());
    }
}
