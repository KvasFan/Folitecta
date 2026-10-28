//! Comment styles and appearance streams generated from the annotation dictionary.
//!
//! Every comment Folitecta draws gets its normal appearance (`/AP /N`) from
//! [`Document::generate_appearance`], which reads the geometry and style entries
//! of the dictionary (`/Rect`, `/C`, `/IC`, `/CA`, `/BS`, `/LE`, `/L`,
//! `/Vertices`, `/InkList`, `/QuadPoints`, `/DA`, `/Q`, `/Contents`). Changing a
//! style, moving or resizing therefore always yields a crisp appearance.

use lopdf::{Dictionary, Object, ObjectId, dictionary};

use crate::document::Document;
use crate::error::{Error, Result};
use crate::util::{deref, fmt_num, num};

mod generate;
mod measure;

pub(crate) use generate::text_problem;
pub(crate) use measure::{ANGLE, RADIUS, VOLUME};
mod note_icons;
mod shapes;

pub use note_icons::NOTE_ICONS;

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

/// Folitecta's entry of an ink comment drawn with a pen that tells how hard
/// it presses: one array per path of `/InkList`, a number from 0 to 1 per
/// point. The appearance drawn from it is in `/AP` for every reader.
pub(crate) const INK_PRESSURE: &str = "FolitectaPressure";

/// The pressures of the paths of an ink comment, one list per path; empty
/// when its strokes were not drawn with a pen.
pub(crate) fn ink_pressures(doc: &lopdf::Document, d: &Dictionary) -> Vec<Vec<f32>> {
    d.get(INK_PRESSURE.as_bytes())
        .ok()
        .map(|l| deref(doc, l))
        .and_then(|l| l.as_array().ok())
        .map(|l| l.iter().map(|p| numbers(doc, Some(p))).collect())
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
        apply_style(&mut d, &subtype, style);
        // Text the new style cannot draw is refused before the edit.
        if let Some(problem) = text_problem(&self.inner, &d) {
            return Err(problem.into());
        }
        self.edit_step(|doc| {
            doc.generate_appearance(&mut d)?;
            d.set("M", Object::string_literal(crate::util::pdf_date_now()));
            *doc.inner.get_dictionary_mut(id)? = d;
            Ok(())
        })
    }

    /// The icon of a note (`/Name` of a `Text` annotation), `None` for other comments.
    pub fn note_icon_of(&self, page: usize, index: usize) -> Option<String> {
        let d = self.annotation_dict(page, index).ok()?;
        (name_of(&d, b"Subtype").as_deref() == Some("Text"))
            .then(|| name_of(&d, b"Name").unwrap_or_else(|| "Comment".into()))
    }

    /// Give a note one of the [`NOTE_ICONS`] and draw it again.
    pub fn set_note_icon(&mut self, page: usize, index: usize, icon: &str) -> Result<()> {
        if !NOTE_ICONS.contains(&icon) {
            return Err(Error::Invalid(format!("a note cannot have the icon \"{icon}\"")));
        }
        let id = self.annotation_object(page, index)?;
        let mut d = self.inner.get_dictionary(id)?.clone();
        if name_of(&d, b"Subtype").as_deref() != Some("Text") {
            return Err(Error::Invalid("only notes have an icon".into()));
        }
        d.set("Name", Object::Name(icon.as_bytes().to_vec()));
        self.edit_step(|doc| {
            doc.generate_appearance(&mut d)?;
            d.set("M", Object::string_literal(crate::util::pdf_date_now()));
            *doc.inner.get_dictionary_mut(id)? = d;
            Ok(())
        })
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
}
