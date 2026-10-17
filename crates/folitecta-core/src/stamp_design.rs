//! Designed stamps: text, shapes, pictures and fields that are filled in when
//! the stamp is placed (date, name, e-mail, company, login, counter, custom
//! values and page-mark macros), drawn as the appearance of a stamp comment.

use std::collections::HashMap;

use base64::Engine;
use lopdf::{Object, ObjectId, dictionary};
use serde::{Deserialize, Serialize};

use crate::document::Document;
use crate::error::{Error, Result};
use crate::util::{fmt_num, helvetica_width, pdf_literal};

/// One part of a designed stamp. Coordinates are points of the design,
/// origin at the top-left corner, y downwards.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct StampElement {
    /// `text`, `rect`, `oval`, `line`, `polyline`, `polygon`, `curve`, `image` or `field`.
    pub kind: String,
    /// Box of text, fields, rectangles, ovals and pictures: x, y, width, height.
    pub rect: [f32; 4],
    /// Points of lines, polylines, polygons and curves.
    pub points: Vec<[f32; 2]>,
    /// Text, or the label / template of a field.
    pub text: String,
    /// Field kind: `date`, `name`, `email`, `corporation`, `login`, `counter`,
    /// `custom` or `macros`.
    pub field: String,
    /// Date format (`YYYY-MM-DD HH:mm`) or the digits of a counter.
    pub format: String,
    pub font_size: f32,
    pub bold: bool,
    /// `left`, `center` or `right`.
    pub align: String,
    pub color: [f32; 3],
    pub fill: Option<[f32; 3]>,
    pub line_width: f32,
    /// Corner radius of rectangles.
    pub radius: f32,
    pub opacity: f32,
    /// PNG or JPEG, base64.
    pub image: String,
}

impl Default for StampElement {
    fn default() -> Self {
        StampElement {
            kind: "text".into(),
            rect: [10.0, 10.0, 180.0, 30.0],
            points: Vec::new(),
            text: String::new(),
            field: String::new(),
            format: String::new(),
            font_size: 18.0,
            bold: true,
            align: "center".into(),
            color: [0.78, 0.12, 0.12],
            fill: None,
            line_width: 2.0,
            radius: 0.0,
            opacity: 1.0,
            image: String::new(),
        }
    }
}

impl StampElement {
    /// A new element of a kind, placed inside a design of the given size.
    pub fn new(kind: &str, w: f32, h: f32) -> StampElement {
        let base = StampElement {
            kind: kind.into(),
            rect: [w * 0.1, h * 0.2, w * 0.8, h * 0.6],
            ..Default::default()
        };
        match kind {
            "line" => StampElement {
                points: vec![[w * 0.1, h * 0.5], [w * 0.9, h * 0.5]],
                ..base
            },
            "polyline" | "curve" => StampElement {
                points: vec![
                    [w * 0.1, h * 0.7],
                    [w * 0.35, h * 0.3],
                    [w * 0.65, h * 0.7],
                    [w * 0.9, h * 0.3],
                ],
                ..base
            },
            "polygon" => StampElement {
                points: vec![[w * 0.5, h * 0.15], [w * 0.85, h * 0.85], [w * 0.15, h * 0.85]],
                ..base
            },
            "text" => StampElement {
                text: "TEXT".into(),
                ..base
            },
            "field" => StampElement {
                field: "date".into(),
                format: "YYYY-MM-DD".into(),
                font_size: 12.0,
                bold: false,
                ..base
            },
            _ => base,
        }
    }

    fn is_text(&self) -> bool {
        self.kind == "text" || self.kind == "field"
    }

    /// Bounds `[x0, y0, x1, y1]` of the element in the design.
    pub fn bounds(&self) -> [f32; 4] {
        if self.points.is_empty()
            || matches!(self.kind.as_str(), "text" | "field" | "rect" | "oval" | "image")
        {
            let r = self.rect;
            return [r[0], r[1], r[0] + r[2], r[1] + r[3]];
        }
        let mut b = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
        for p in &self.points {
            b[0] = b[0].min(p[0]);
            b[1] = b[1].min(p[1]);
            b[2] = b[2].max(p[0]);
            b[3] = b[3].max(p[1]);
        }
        let m = self.line_width / 2.0;
        [b[0] - m, b[1] - m, b[2] + m, b[3] + m]
    }

    /// Move the element by `dx`, `dy`.
    pub fn translate(&mut self, dx: f32, dy: f32) {
        self.rect[0] += dx;
        self.rect[1] += dy;
        for p in &mut self.points {
            p[0] += dx;
            p[1] += dy;
        }
    }
}

/// A stamp made of elements on a background.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct StampDesign {
    pub width: f32,
    pub height: f32,
    pub background: Option<[f32; 3]>,
    pub elements: Vec<StampElement>,
}

impl Default for StampDesign {
    fn default() -> Self {
        StampDesign {
            width: 200.0,
            height: 70.0,
            background: None,
            elements: Vec::new(),
        }
    }
}

/// Values of the fields when a stamp is placed.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct StampValues {
    pub name: String,
    pub email: String,
    pub corporation: String,
    pub login: String,
    pub counter: i64,
    /// Values of custom fields by label.
    pub custom: HashMap<String, String>,
    /// Local time: year, month, day, hour, minute, second.
    pub now: (i32, u8, u8, u8, u8, u8),
}

/// Kinds of stamp fields.
pub const FIELD_KINDS: &[&str] = &[
    "date",
    "name",
    "email",
    "corporation",
    "login",
    "counter",
    "custom",
    "macros",
];

/// Date formats offered for date fields.
pub const DATE_FORMATS: &[&str] = &[
    "YYYY-MM-DD",
    "DD/MM/YYYY",
    "MM/DD/YYYY",
    "DD.MM.YYYY",
    "D MMM YYYY",
    "YYYY-MM-DD HH:mm",
    "DD/MM/YYYY HH:mm",
    "HH:mm",
];

/// Templates the designer can start from.
pub const TEMPLATES: &[&str] = &[
    "approved",
    "received",
    "paid",
    "draft",
    "confidential",
    "signature",
];

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// Format a date with `YYYY`, `YY`, `MMM`, `MM`, `M`, `DD`, `D`, `HH`, `mm` and `ss`.
pub fn format_date(fmt: &str, now: (i32, u8, u8, u8, u8, u8)) -> String {
    let (y, mo, d, h, mi, s) = now;
    let chars: Vec<char> = fmt.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    let run = |i: usize, c: char| chars[i..].iter().take_while(|x| **x == c).count();
    while i < chars.len() {
        let c = chars[i];
        let n = run(i, c);
        let piece = match (c, n) {
            ('Y', 4..) => Some(format!("{y:04}")),
            ('Y', 2) => Some(format!("{:02}", y.rem_euclid(100))),
            ('M', 3..) => Some(MONTHS[(mo.clamp(1, 12) - 1) as usize].to_string()),
            ('M', 2) => Some(format!("{mo:02}")),
            ('M', 1) => Some(mo.to_string()),
            ('D', 2..) => Some(format!("{d:02}")),
            ('D', 1) => Some(d.to_string()),
            ('H', 2..) => Some(format!("{h:02}")),
            ('H', 1) => Some(h.to_string()),
            ('m', 2..) => Some(format!("{mi:02}")),
            ('s', 2..) => Some(format!("{s:02}")),
            _ => None,
        };
        match piece {
            Some(p) => {
                out.push_str(&p);
                i += n;
            }
            None => {
                out.push(c);
                i += 1;
            }
        }
    }
    out
}

fn css(c: [f32; 3]) -> String {
    format!(
        "rgb({},{},{})",
        (c[0] * 255.0).round() as u8,
        (c[1] * 255.0).round() as u8,
        (c[2] * 255.0).round() as u8
    )
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Bezier control points of a smooth curve through points (Catmull-Rom).
fn curve_segments(p: &[[f32; 2]]) -> Vec<([f32; 2], [f32; 2], [f32; 2])> {
    let mut out = Vec::new();
    for i in 0..p.len().saturating_sub(1) {
        let p0 = if i == 0 { p[0] } else { p[i - 1] };
        let (p1, p2) = (p[i], p[i + 1]);
        let p3 = if i + 2 < p.len() { p[i + 2] } else { p[i + 1] };
        let c1 = [p1[0] + (p2[0] - p0[0]) / 6.0, p1[1] + (p2[1] - p0[1]) / 6.0];
        let c2 = [p2[0] - (p3[0] - p1[0]) / 6.0, p2[1] - (p3[1] - p1[1]) / 6.0];
        out.push((c1, c2, p2));
    }
    out
}

impl StampDesign {
    /// A design to start from.
    pub fn template(name: &str) -> Option<StampDesign> {
        let red = [0.78, 0.12, 0.12];
        let green = [0.08, 0.5, 0.2];
        let blue = [0.1, 0.25, 0.65];
        let frame = |c: [f32; 3], w: f32, h: f32| StampElement {
            kind: "rect".into(),
            rect: [3.0, 3.0, w - 6.0, h - 6.0],
            color: c,
            line_width: 3.0,
            radius: 8.0,
            ..Default::default()
        };
        let title = |t: &str, c: [f32; 3], rect: [f32; 4], size: f32| StampElement {
            kind: "text".into(),
            text: t.into(),
            rect,
            color: c,
            font_size: size,
            ..Default::default()
        };
        let field = |f: &str, fmt: &str, c: [f32; 3], rect: [f32; 4], label: &str| StampElement {
            kind: "field".into(),
            field: f.into(),
            format: fmt.into(),
            text: label.into(),
            rect,
            color: c,
            font_size: 10.0,
            bold: false,
            ..Default::default()
        };
        let (w, h) = (220.0, 80.0);
        let d = match name {
            "approved" => vec![
                frame(green, w, h),
                title("APPROVED", green, [10.0, 10.0, 200.0, 34.0], 26.0),
                field("name", "", green, [10.0, 46.0, 200.0, 12.0], "by "),
                field("date", "D MMM YYYY HH:mm", green, [10.0, 60.0, 200.0, 12.0], ""),
            ],
            "received" => vec![
                frame(blue, w, h),
                title("RECEIVED", blue, [10.0, 10.0, 200.0, 34.0], 26.0),
                field("date", "D MMM YYYY", blue, [10.0, 48.0, 200.0, 14.0], ""),
            ],
            "paid" => vec![
                frame(green, w, h),
                title("PAID", green, [10.0, 8.0, 200.0, 40.0], 32.0),
                field("date", "YYYY-MM-DD", green, [10.0, 52.0, 200.0, 14.0], ""),
            ],
            "draft" => vec![
                frame(red, w, 60.0),
                title("DRAFT", red, [10.0, 10.0, 200.0, 40.0], 32.0),
            ],
            "confidential" => vec![
                frame(red, w, 60.0),
                title("CONFIDENTIAL", red, [10.0, 12.0, 200.0, 36.0], 24.0),
            ],
            "signature" => vec![
                StampElement {
                    kind: "line".into(),
                    points: vec![[10.0, 50.0], [210.0, 50.0]],
                    color: [0.1, 0.1, 0.1],
                    line_width: 1.0,
                    ..Default::default()
                },
                field("name", "", [0.1, 0.1, 0.1], [10.0, 54.0, 200.0, 12.0], ""),
                field(
                    "date",
                    "YYYY-MM-DD",
                    [0.1, 0.1, 0.1],
                    [10.0, 66.0, 200.0, 12.0],
                    "",
                ),
            ],
            _ => return None,
        };
        let height = if matches!(name, "draft" | "confidential") {
            60.0
        } else {
            h
        };
        Some(StampDesign {
            width: w,
            height,
            background: None,
            elements: d,
        })
    }

    /// Text shown by a text element or field.
    pub fn element_text(el: &StampElement, values: &StampValues, macros: &dyn Fn(&str) -> String) -> String {
        if el.kind != "field" {
            return el.text.clone();
        }
        let value = match el.field.as_str() {
            "date" => format_date(
                if el.format.is_empty() {
                    "YYYY-MM-DD"
                } else {
                    &el.format
                },
                values.now,
            ),
            "name" => values.name.clone(),
            "email" => values.email.clone(),
            "corporation" => values.corporation.clone(),
            "login" => values.login.clone(),
            "counter" => {
                let digits = el.format.trim().parse::<usize>().unwrap_or(1);
                format!("{:0digits$}", values.counter)
            }
            "custom" => values.custom.get(&el.text).cloned().unwrap_or_default(),
            "macros" => return macros(&el.text),
            _ => String::new(),
        };
        // The text of other fields is a label put before the value.
        if el.field == "custom" {
            value
        } else {
            format!("{}{value}", el.text)
        }
    }

    /// Labels of the custom fields (values asked when placing the stamp).
    pub fn custom_fields(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for e in &self.elements {
            if e.kind == "field" && e.field == "custom" && !out.contains(&e.text) {
                out.push(e.text.clone());
            }
        }
        out
    }

    /// Whether placing the stamp advances a counter.
    pub fn has_counter(&self) -> bool {
        self.elements
            .iter()
            .any(|e| e.kind == "field" && e.field == "counter")
    }

    /// Shrink the design to its elements with a small margin.
    pub fn crop_to_content(&mut self) {
        if self.elements.is_empty() {
            return;
        }
        let mut b = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
        for e in &self.elements {
            let r = e.bounds();
            b[0] = b[0].min(r[0]);
            b[1] = b[1].min(r[1]);
            b[2] = b[2].max(r[2]);
            b[3] = b[3].max(r[3]);
        }
        let m = 4.0;
        let (dx, dy) = (m - b[0], m - b[1]);
        for e in &mut self.elements {
            e.translate(dx, dy);
        }
        self.width = (b[2] - b[0] + 2.0 * m).max(10.0);
        self.height = (b[3] - b[1] + 2.0 * m).max(10.0);
    }

    /// An SVG picture of the design (for previews).
    pub fn svg(&self, values: &StampValues) -> String {
        let (w, h) = (self.width.max(1.0), self.height.max(1.0));
        let mut s = format!(
            "<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 {w} {h}' width='{w}' height='{h}'>"
        );
        if let Some(bg) = self.background {
            s.push_str(&format!("<rect width='{w}' height='{h}' fill='{}'/>", css(bg)));
        }
        let identity = |t: &str| t.to_string();
        for e in &self.elements {
            let stroke = css(e.color);
            let fill = e.fill.map(css).unwrap_or_else(|| "none".into());
            let common = format!(
                "stroke='{stroke}' stroke-width='{}' fill='{fill}' opacity='{}' stroke-linejoin='round' stroke-linecap='round'",
                e.line_width,
                e.opacity.clamp(0.0, 1.0)
            );
            let r = e.rect;
            let pts = |close: bool| {
                let mut d = String::new();
                for (i, p) in e.points.iter().enumerate() {
                    d.push_str(&format!("{}{} {} ", if i == 0 { "M" } else { "L" }, p[0], p[1]));
                }
                if close {
                    d.push('Z');
                }
                d
            };
            match e.kind.as_str() {
                "rect" => s.push_str(&format!(
                    "<rect x='{}' y='{}' width='{}' height='{}' rx='{}' {common}/>",
                    r[0], r[1], r[2], r[3], e.radius
                )),
                "oval" => s.push_str(&format!(
                    "<ellipse cx='{}' cy='{}' rx='{}' ry='{}' {common}/>",
                    r[0] + r[2] / 2.0,
                    r[1] + r[3] / 2.0,
                    r[2] / 2.0,
                    r[3] / 2.0
                )),
                "line" | "polyline" => s.push_str(&format!(
                    "<path d='{}' {}/>",
                    pts(false),
                    common.replace(&format!("fill='{fill}'"), "fill='none'")
                )),
                "polygon" => s.push_str(&format!("<path d='{}' {common}/>", pts(true))),
                "curve" => {
                    if let Some(p0) = e.points.first() {
                        let mut d = format!("M{} {} ", p0[0], p0[1]);
                        for (c1, c2, p) in curve_segments(&e.points) {
                            d.push_str(&format!(
                                "C{} {} {} {} {} {} ",
                                c1[0], c1[1], c2[0], c2[1], p[0], p[1]
                            ));
                        }
                        s.push_str(&format!(
                            "<path d='{d}' {}/>",
                            common.replace(&format!("fill='{fill}'"), "fill='none'")
                        ));
                    }
                }
                "image" if !e.image.is_empty() => {
                    let mime = if e.image.starts_with("/9j/") {
                        "image/jpeg"
                    } else {
                        "image/png"
                    };
                    s.push_str(&format!(
                        "<image x='{}' y='{}' width='{}' height='{}' href='data:{mime};base64,{}' opacity='{}'/>",
                        r[0], r[1], r[2], r[3], e.image, e.opacity
                    ));
                }
                "text" | "field" => {
                    let text = Self::element_text(e, values, &identity);
                    let (anchor, x) = match e.align.as_str() {
                        "left" => ("start", r[0]),
                        "right" => ("end", r[0] + r[2]),
                        _ => ("middle", r[0] + r[2] / 2.0),
                    };
                    for (i, line) in text.lines().enumerate() {
                        let y = r[1] + e.font_size * (0.85 + 1.2 * i as f32);
                        s.push_str(&format!(
                            "<text x='{x}' y='{y}' font-family='Helvetica, Arial, sans-serif' font-size='{}' font-weight='{}' text-anchor='{anchor}' fill='{stroke}' opacity='{}'>{}</text>",
                            e.font_size,
                            if e.bold { "bold" } else { "normal" },
                            e.opacity,
                            xml_escape(line)
                        ));
                    }
                }
                _ => {}
            }
        }
        s.push_str("</svg>");
        s
    }
}

impl Document {
    /// Place a designed stamp inside `rect` (PDF space). `name` becomes the
    /// stamp's name. Fields are filled with `values`; macros expand for the page.
    pub fn add_designed_stamp(
        &mut self,
        page: usize,
        rect: [f32; 4],
        design: &StampDesign,
        values: &StampValues,
        author: &str,
        name: &str,
    ) -> Result<ObjectId> {
        if design.elements.is_empty() && design.background.is_none() {
            return Err(Error::Invalid("the stamp is empty".into()));
        }
        let page_id = self.page_id(page)?;
        let r = [
            rect[0].min(rect[2]),
            rect[1].min(rect[3]),
            rect[0].max(rect[2]),
            rect[1].max(rect[3]),
        ];
        let (sx, sy) = (
            (r[2] - r[0]) / design.width.max(1.0),
            (r[3] - r[1]) / design.height.max(1.0),
        );
        // Texts first: macros read the document before it changes.
        let texts: Vec<String> = design
            .elements
            .iter()
            .map(|e| {
                if e.is_text() {
                    StampDesign::element_text(e, values, &|t: &str| self.expand_macros(t, page, 1))
                } else {
                    String::new()
                }
            })
            .collect();
        let mut images = Vec::new();
        for e in &design.elements {
            if e.kind == "image" && !e.image.is_empty() {
                let data = base64::engine::general_purpose::STANDARD
                    .decode(&e.image)
                    .map_err(|e| Error::Invalid(format!("stamp picture: {e}")))?;
                images.push(Some(crate::images::image_object(data)?));
            } else {
                images.push(None);
            }
        }
        self.begin_edit();
        let f = fmt_num;
        let font = self.inner.add_object(dictionary! {
            "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica", "Encoding" => "WinAnsiEncoding",
        });
        let bold = self.inner.add_object(dictionary! {
            "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica-Bold", "Encoding" => "WinAnsiEncoding",
        });
        let mut xobjects = lopdf::Dictionary::new();
        let mut gstates = lopdf::Dictionary::new();
        // Design coordinates: y downwards from the top-left corner of the rectangle.
        let mut c = format!("q {} 0 0 {} {} {} cm\n", f(sx), f(-sy), f(r[0]), f(r[3]));
        if let Some(bg) = design.background {
            c.push_str(&format!(
                "{} {} {} rg 0 0 {} {} re f\n",
                f(bg[0]),
                f(bg[1]),
                f(bg[2]),
                f(design.width),
                f(design.height)
            ));
        }
        for (i, (e, img)) in design.elements.iter().zip(images).enumerate() {
            c.push_str("q ");
            if e.opacity < 0.999 {
                let gs = format!("GS{i}");
                let a = e.opacity.clamp(0.0, 1.0);
                gstates.set(
                    gs.as_bytes().to_vec(),
                    Object::Dictionary(dictionary! { "CA" => a, "ca" => a }),
                );
                c.push_str(&format!("/{gs} gs "));
            }
            let col = e.color;
            c.push_str(&format!(
                "{} {} {} RG {} {} {} rg {} w 1 j 1 J ",
                f(col[0]),
                f(col[1]),
                f(col[2]),
                f(col[0]),
                f(col[1]),
                f(col[2]),
                f(e.line_width.max(0.1))
            ));
            if let Some(fl) = e.fill {
                c.push_str(&format!("{} {} {} rg ", f(fl[0]), f(fl[1]), f(fl[2])));
            }
            let paint = |closed: bool| match (e.fill.is_some() && closed, e.line_width > 0.0) {
                (true, true) => "B",
                (true, false) => "f",
                _ => "S",
            };
            let rr = e.rect;
            match e.kind.as_str() {
                "rect" => {
                    let rad = e.radius.min(rr[2] / 2.0).min(rr[3] / 2.0).max(0.0);
                    if rad > 0.0 {
                        let k = 0.5523 * rad;
                        let (x0, y0, x1, y1) = (rr[0], rr[1], rr[0] + rr[2], rr[1] + rr[3]);
                        c.push_str(&format!(
                            "{} {} m {} {} l {} {} {} {} {} {} c {} {} l {} {} {} {} {} {} c {} {} l {} {} {} {} {} {} c {} {} l {} {} {} {} {} {} c h {}",
                            f(x0 + rad), f(y0),
                            f(x1 - rad), f(y0),
                            f(x1 - rad + k), f(y0), f(x1), f(y0 + rad - k), f(x1), f(y0 + rad),
                            f(x1), f(y1 - rad),
                            f(x1), f(y1 - rad + k), f(x1 - rad + k), f(y1), f(x1 - rad), f(y1),
                            f(x0 + rad), f(y1),
                            f(x0 + rad - k), f(y1), f(x0), f(y1 - rad + k), f(x0), f(y1 - rad),
                            f(x0), f(y0 + rad),
                            f(x0), f(y0 + rad - k), f(x0 + rad - k), f(y0), f(x0 + rad), f(y0),
                            paint(true)
                        ));
                    } else {
                        c.push_str(&format!(
                            "{} {} {} {} re {}",
                            f(rr[0]),
                            f(rr[1]),
                            f(rr[2]),
                            f(rr[3]),
                            paint(true)
                        ));
                    }
                }
                "oval" => {
                    let (cx, cy, rx, ry) =
                        (rr[0] + rr[2] / 2.0, rr[1] + rr[3] / 2.0, rr[2] / 2.0, rr[3] / 2.0);
                    let (kx, ky) = (0.5523 * rx, 0.5523 * ry);
                    c.push_str(&format!(
                        "{} {} m {} {} {} {} {} {} c {} {} {} {} {} {} c {} {} {} {} {} {} c {} {} {} {} {} {} c h {}",
                        f(cx + rx), f(cy),
                        f(cx + rx), f(cy + ky), f(cx + kx), f(cy + ry), f(cx), f(cy + ry),
                        f(cx - kx), f(cy + ry), f(cx - rx), f(cy + ky), f(cx - rx), f(cy),
                        f(cx - rx), f(cy - ky), f(cx - kx), f(cy - ry), f(cx), f(cy - ry),
                        f(cx + kx), f(cy - ry), f(cx + rx), f(cy - ky), f(cx + rx), f(cy),
                        paint(true)
                    ));
                }
                "line" | "polyline" | "polygon" => {
                    for (k, p) in e.points.iter().enumerate() {
                        c.push_str(&format!(
                            "{} {} {} ",
                            f(p[0]),
                            f(p[1]),
                            if k == 0 { "m" } else { "l" }
                        ));
                    }
                    if e.kind == "polygon" {
                        c.push_str(&format!("h {}", paint(true)));
                    } else {
                        c.push('S');
                    }
                }
                "curve" => {
                    if let Some(p0) = e.points.first() {
                        c.push_str(&format!("{} {} m ", f(p0[0]), f(p0[1])));
                        for (c1, c2, p) in curve_segments(&e.points) {
                            c.push_str(&format!(
                                "{} {} {} {} {} {} c ",
                                f(c1[0]),
                                f(c1[1]),
                                f(c2[0]),
                                f(c2[1]),
                                f(p[0]),
                                f(p[1])
                            ));
                        }
                        c.push('S');
                    }
                }
                "image" => {
                    if let Some(img) = img {
                        let id = self.add_image_object(img);
                        let nm = format!("Im{i}");
                        xobjects.set(nm.as_bytes().to_vec(), Object::Reference(id));
                        // Pictures are drawn upright: flip back inside the box.
                        c.push_str(&format!(
                            "{} 0 0 {} {} {} cm /{nm} Do",
                            f(rr[2]),
                            f(-rr[3]),
                            f(rr[0]),
                            f(rr[1] + rr[3])
                        ));
                    }
                }
                "text" | "field" => {
                    let text = &texts[i];
                    let size = e.font_size.max(2.0);
                    let fname = if e.bold { "HB" } else { "H" };
                    let factor = if e.bold { 1.06 } else { 1.0 };
                    for (k, line) in text.lines().enumerate() {
                        let w = helvetica_width(line, size) * factor;
                        let x = match e.align.as_str() {
                            "left" => rr[0],
                            "right" => rr[0] + rr[2] - w,
                            _ => rr[0] + (rr[2] - w) / 2.0,
                        };
                        let y = rr[1] + size * (0.85 + 1.2 * k as f32);
                        c.push_str(&format!(
                            "BT /{fname} {} Tf 1 0 0 -1 {} {} Tm {} Tj ET ",
                            f(size),
                            f(x),
                            f(y),
                            String::from_utf8_lossy(&pdf_literal(line))
                        ));
                    }
                }
                _ => {}
            }
            c.push_str(" Q\n");
        }
        c.push_str("Q\n");
        let mut resources = dictionary! { "Font" => dictionary! { "H" => font, "HB" => bold } };
        if !xobjects.is_empty() {
            resources.set("XObject", Object::Dictionary(xobjects));
        }
        if !gstates.is_empty() {
            resources.set("ExtGState", Object::Dictionary(gstates));
        }
        let contents = texts
            .iter()
            .filter(|t| !t.is_empty())
            .cloned()
            .collect::<Vec<_>>()
            .join(" ");
        let color = design
            .elements
            .first()
            .map(|e| e.color)
            .unwrap_or([0.0, 0.0, 0.0]);
        let mut annot = self.base_annotation(page_id, "Stamp", r, author, &contents, color);
        let clean: String = name.chars().filter(|c| c.is_ascii_alphanumeric()).collect();
        annot.set(
            "Name",
            Object::Name(format!("FolitectaDesign{clean}").into_bytes()),
        );
        self.add_appearance(&mut annot, r, c, resources);
        let id = self.push_annotation(page_id, annot)?;
        self.commit()?;
        Ok(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_follow_their_format() {
        let now = (2026, 3, 7, 9, 5, 2);
        assert_eq!(format_date("YYYY-MM-DD HH:mm", now), "2026-03-07 09:05");
        assert_eq!(format_date("D MMM YY", now), "7 Mar 26");
        assert_eq!(format_date("DD/MM/YYYY", now), "07/03/2026");
        assert_eq!(format_date("at HH:mm:ss", now), "at 09:05:02");
    }
}
