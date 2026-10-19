//! More comment tools: clouds, measurements, callouts, text insertion and
//! replacement marks, files attached to a page and the ink eraser.

use lopdf::{Dictionary, Object, ObjectId, Stream, dictionary};

use crate::appearance::{AnnotStyle, apply_style};
use crate::document::Document;
use crate::error::{Error, Result};
use crate::util::{self, deref, num};

/// Unit and scale of measurement comments.
#[derive(Debug, Clone, PartialEq)]
pub struct MeasureScale {
    /// Real-world units per PDF point.
    pub factor: f32,
    /// Unit shown after values ("mm", "cm", "m", "in", "ft", "pt").
    pub unit: String,
    /// Decimal places.
    pub decimals: u8,
    /// Human-readable scale stored in the document, e.g. "1 cm = 1 m".
    pub ratio: String,
    /// Separator of the decimals ("." or ",").
    pub decimal_sep: String,
    /// Separator of the thousands (empty for none).
    pub group_sep: String,
}

impl Default for MeasureScale {
    fn default() -> Self {
        MeasureScale {
            factor: 25.4 / 72.0,
            unit: "mm".into(),
            decimals: 1,
            ratio: "1 mm = 1 mm".into(),
            decimal_sep: ".".into(),
            group_sep: String::new(),
        }
    }
}

/// A number with `decimals` places and the separators of a measurement.
pub fn format_measure(v: f32, decimals: usize, decimal_sep: &str, group_sep: &str) -> String {
    let text = format!("{:.*}", decimals, v.abs());
    let (int, frac) = text.split_once('.').unwrap_or((&text, ""));
    let mut grouped = String::new();
    for (i, c) in int.chars().enumerate() {
        if i > 0 && (int.len() - i) % 3 == 0 {
            grouped.push_str(group_sep);
        }
        grouped.push(c);
    }
    let sign = if v < 0.0 && text.chars().any(|c| c.is_ascii_digit() && c != '0') {
        "-"
    } else {
        ""
    };
    if frac.is_empty() {
        format!("{sign}{grouped}")
    } else {
        format!("{sign}{grouped}{decimal_sep}{frac}")
    }
}

impl MeasureScale {
    /// Points per unit of a paper measurement unit.
    pub fn points_per(unit: &str) -> f32 {
        match unit {
            "pt" => 1.0,
            "in" => 72.0,
            "ft" => 864.0,
            "cm" => 72.0 / 2.54,
            "m" => 7200.0 / 2.54,
            "km" => 7_200_000.0 / 2.54,
            _ => 72.0 / 25.4,
        }
    }

    /// A scale where `paper` units on the page correspond to `real` units in reality
    /// (for example 1 cm = 2.5 m).
    pub fn from_ratio(
        paper: f32,
        paper_unit: &str,
        real: f32,
        real_unit: &str,
        decimals: u8,
    ) -> MeasureScale {
        let paper_pts = (paper * Self::points_per(paper_unit)).max(0.0001);
        let real_in_unit = real.max(0.0);
        MeasureScale {
            factor: real_in_unit / paper_pts,
            unit: real_unit.to_string(),
            decimals: decimals.min(6),
            ratio: format!(
                "{} {} = {} {}",
                util::fmt_num(paper),
                paper_unit,
                util::fmt_num(real),
                real_unit
            ),
            ..MeasureScale::default()
        }
    }

    /// The same scale with these separators of the decimals and thousands.
    pub fn with_separators(mut self, decimal_sep: &str, group_sep: &str) -> MeasureScale {
        self.decimal_sep = if decimal_sep.is_empty() {
            ".".into()
        } else {
            decimal_sep.to_string()
        };
        self.group_sep = group_sep.to_string();
        self
    }
}

/// An ink comment changed by the eraser: position, object and remaining strokes.
type InkEdit = (usize, ObjectId, Vec<Vec<(f32, f32)>>);

/// Kinds of measurement comments.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MeasureKind {
    Distance,
    Perimeter,
    Area,
}

fn number_format(unit: &str, factor: f32, scale: &MeasureScale) -> Object {
    let mut d = dictionary! {
        "Type" => "NumberFormat",
        "U" => lopdf::text_string(unit),
        "C" => factor,
        "D" => 10i64.pow(scale.decimals.min(6) as u32),
        "F" => "D",
    };
    // Written only when they differ from what readers assume without them.
    if scale.decimal_sep != "." {
        d.set("RD", lopdf::text_string(&scale.decimal_sep));
    }
    if !scale.group_sep.is_empty() {
        d.set("RT", lopdf::text_string(&scale.group_sep));
    }
    Object::Array(vec![Object::Dictionary(d)])
}

fn flat(points: &[(f32, f32)]) -> Object {
    Object::Array(
        points
            .iter()
            .flat_map(|(x, y)| [(*x).into(), (*y).into()])
            .collect(),
    )
}

fn bbox(points: &[(f32, f32)]) -> [f32; 4] {
    points
        .iter()
        .fold([f32::MAX, f32::MAX, f32::MIN, f32::MIN], |r, p| {
            [r[0].min(p.0), r[1].min(p.1), r[2].max(p.0), r[3].max(p.1)]
        })
}

/// Distance from `p` to the segment `a`-`b`.
fn seg_distance(p: (f32, f32), a: (f32, f32), b: (f32, f32)) -> f32 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len2 = dx * dx + dy * dy;
    let t = if len2 == 0.0 {
        0.0
    } else {
        (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / len2).clamp(0.0, 1.0)
    };
    let (cx, cy) = (a.0 + dx * t, a.1 + dy * t);
    ((p.0 - cx).powi(2) + (p.1 - cy).powi(2)).sqrt()
}

impl Document {
    /// Add a cloud: a polygon drawn with a scalloped border.
    pub fn add_cloud(
        &mut self,
        page: usize,
        points: &[(f32, f32)],
        style: &AnnotStyle,
        author: &str,
    ) -> Result<ObjectId> {
        if points.len() < 3 {
            return Err(Error::Invalid("a cloud needs at least three points".into()));
        }
        let page_id = self.page_id(page)?;
        self.begin_edit();
        let mut annot = self.base_annotation(
            page_id,
            "Polygon",
            bbox(points),
            author,
            "",
            style.color.unwrap_or([0.0; 3]),
        );
        annot.set("Vertices", flat(points));
        annot.set("BE", dictionary! { "S" => "C", "I" => 1i64 });
        apply_style(&mut annot, "Polygon", style);
        self.finish_annotation(page_id, annot)
    }

    /// Add a measurement: a distance (two points), a perimeter (open path) or an
    /// area (closed polygon), labelled with its value in the scale's unit.
    pub fn add_measurement(
        &mut self,
        page: usize,
        kind: MeasureKind,
        points: &[(f32, f32)],
        style: &AnnotStyle,
        scale: &MeasureScale,
        author: &str,
    ) -> Result<ObjectId> {
        let (subtype, intent, min) = match kind {
            MeasureKind::Distance => ("Line", "LineDimension", 2),
            MeasureKind::Perimeter => ("PolyLine", "PolyLineDimension", 2),
            MeasureKind::Area => ("Polygon", "PolygonDimension", 3),
        };
        if points.len() < min {
            return Err(Error::Invalid("not enough points for the measurement".into()));
        }
        let points = if kind == MeasureKind::Distance {
            &points[..2]
        } else {
            points
        };
        let page_id = self.page_id(page)?;
        self.begin_edit();
        let mut annot = self.base_annotation(
            page_id,
            subtype,
            bbox(points),
            author,
            "",
            style.color.unwrap_or([0.0; 3]),
        );
        annot.set(if subtype == "Line" { "L" } else { "Vertices" }, flat(points));
        annot.set("IT", Object::Name(intent.as_bytes().to_vec()));
        let area_unit = format!("{}\u{b2}", scale.unit);
        annot.set(
            "Measure",
            dictionary! {
                "Type" => "Measure",
                "Subtype" => "RL",
                "R" => lopdf::text_string(&scale.ratio),
                "X" => number_format(&scale.unit, scale.factor, scale),
                "D" => number_format(&scale.unit, scale.factor, scale),
                "A" => number_format(&area_unit, scale.factor * scale.factor, scale),
            },
        );
        apply_style(&mut annot, subtype, style);
        let f = util::fmt_num;
        let c = style.color.unwrap_or([0.0; 3]);
        annot.set(
            "DA",
            Object::string_literal(format!(
                "/Helv {} Tf {} {} {} rg",
                f(style.font_size),
                f(c[0]),
                f(c[1]),
                f(c[2])
            )),
        );
        self.finish_annotation(page_id, annot)
    }

    /// Measurement comments of the whole document as `(page, kind, value text)`.
    pub fn measurements(&self) -> Vec<(usize, String, String)> {
        let mut out = Vec::new();
        for p in 0..self.page_count() {
            for a in self.annotations(p).unwrap_or_default() {
                let Some(id) = a.id else { continue };
                let Ok(d) = self.inner.get_dictionary(id) else {
                    continue;
                };
                let it = d
                    .get(b"IT")
                    .ok()
                    .and_then(|n| n.as_name().ok())
                    .map(|n| String::from_utf8_lossy(n).to_string())
                    .unwrap_or_default();
                let kind = match it.as_str() {
                    "LineDimension" => "Distance",
                    "PolyLineDimension" => "Perimeter",
                    "PolygonDimension" => "Area",
                    _ => continue,
                };
                out.push((p, kind.to_string(), a.contents.clone()));
            }
        }
        out
    }

    /// Add a callout: a text box with a leader line pointing at `tip`.
    pub fn add_callout(
        &mut self,
        page: usize,
        tip: (f32, f32),
        bx: [f32; 4],
        text: &str,
        style: &AnnotStyle,
        author: &str,
    ) -> Result<ObjectId> {
        let page_id = self.page_id(page)?;
        let bx = crate::geom::normalize(bx);
        // Attach the leader to the middle of the box side facing the tip, with a short knee.
        let (cx, cy) = ((bx[0] + bx[2]) / 2.0, (bx[1] + bx[3]) / 2.0);
        let (attach, knee) = if tip.0 < bx[0] {
            ((bx[0], cy), (bx[0] - 12.0, cy))
        } else if tip.0 > bx[2] {
            ((bx[2], cy), (bx[2] + 12.0, cy))
        } else if tip.1 < bx[1] {
            ((cx, bx[1]), (cx, bx[1] - 12.0))
        } else {
            ((cx, bx[3]), (cx, bx[3] + 12.0))
        };
        // Text that cannot be drawn is refused before anything changes.
        crate::fonts::check_text(&crate::util::clean_text(text))?;
        self.begin_edit();
        let mut annot = self.base_annotation(
            page_id,
            "FreeText",
            bx,
            author,
            text,
            style.color.unwrap_or([0.0; 3]),
        );
        annot.set("IT", Object::Name(b"FreeTextCallout".to_vec()));
        annot.set("CL", flat(&[tip, knee, attach]));
        apply_style(&mut annot, "FreeText", style);
        annot.set("LE", Object::Name(style.line_end.as_bytes().to_vec()));
        self.finish_annotation(page_id, annot)
    }

    /// Add an insert-text mark (caret) at `at` (baseline point) for text of height `h`.
    pub fn add_caret(
        &mut self,
        page: usize,
        at: (f32, f32),
        h: f32,
        contents: &str,
        color: [f32; 3],
        author: &str,
    ) -> Result<ObjectId> {
        let page_id = self.page_id(page)?;
        self.begin_edit();
        let annot = self.caret_dict(page_id, at, h, contents, color, author);
        self.finish_annotation(page_id, annot)
    }

    fn caret_dict(
        &self,
        page_id: ObjectId,
        at: (f32, f32),
        h: f32,
        contents: &str,
        color: [f32; 3],
        author: &str,
    ) -> Dictionary {
        let h = h.clamp(4.0, 72.0);
        let rect = [at.0 - h * 0.3, at.1 - h * 0.25, at.0 + h * 0.3, at.1 + h * 0.3];
        let mut annot = self.base_annotation(page_id, "Caret", rect, author, contents, color);
        annot.set("Sy", Object::Name(b"None".to_vec()));
        annot
    }

    /// Mark text for replacement: strike out `quads` and attach `replacement`
    /// as an insert mark at the end of the text (one undo step).
    pub fn add_replacement(
        &mut self,
        page: usize,
        quads: &[[f32; 8]],
        replacement: &str,
        color: [f32; 3],
        author: &str,
    ) -> Result<()> {
        let last = *quads
            .last()
            .ok_or_else(|| Error::Invalid("select the text to replace first".into()))?;
        let page_id = self.page_id(page)?;
        self.begin_edit();
        let rect = bbox(
            &quads
                .iter()
                .flat_map(|q| [(q[0], q[1]), (q[2], q[3]), (q[4], q[5]), (q[6], q[7])])
                .collect::<Vec<_>>(),
        );
        let mut strike = self.base_annotation(page_id, "StrikeOut", rect, author, "", color);
        strike.set(
            "QuadPoints",
            Object::Array(quads.iter().flatten().map(|v| (*v).into()).collect()),
        );
        self.generate_appearance(&mut strike)?;
        let strike_id = self.push_annotation(page_id, strike)?;
        // Upper-right and lower-right corners of the last quad: insert after the text.
        let (ur, lr) = ((last[2], last[3]), (last[6], last[7]));
        let h = ((ur.0 - lr.0).powi(2) + (ur.1 - lr.1).powi(2)).sqrt();
        let mut caret = self.caret_dict(page_id, lr, h, replacement, color, author);
        caret.set("IRT", Object::Reference(strike_id));
        caret.set("RT", Object::Name(b"Group".to_vec()));
        caret.set("Subj", lopdf::text_string("Replace Text"));
        self.generate_appearance(&mut caret)?;
        self.push_annotation(page_id, caret)?;
        self.commit()
    }

    /// Attach a file to a page as a paper-clip comment at `at` (top-left corner).
    pub fn add_file_attachment(
        &mut self,
        page: usize,
        at: (f32, f32),
        name: &str,
        data: Vec<u8>,
        description: &str,
        author: &str,
    ) -> Result<ObjectId> {
        if name.trim().is_empty() {
            return Err(Error::Invalid("attachment name is empty".into()));
        }
        let page_id = self.page_id(page)?;
        self.begin_edit();
        let size = data.len() as i64;
        let stream_id = self.inner.add_object(Stream::new(
            dictionary! {
                "Type" => "EmbeddedFile",
                "Params" => dictionary! { "Size" => size, "ModDate" => Object::string_literal(util::pdf_date_now()) },
            },
            data,
        ));
        let mut spec = dictionary! {
            "Type" => "Filespec",
            "F" => Object::string_literal(name.to_string()),
            "UF" => lopdf::text_string(name),
            "EF" => dictionary! { "F" => stream_id, "UF" => stream_id },
        };
        if !description.is_empty() {
            spec.set("Desc", lopdf::text_string(description));
        }
        let spec_id = self.inner.add_object(spec);
        let rect = [at.0, at.1 - 20.0, at.0 + 14.0, at.1];
        let contents = if description.is_empty() { name } else { description };
        let mut annot =
            self.base_annotation(page_id, "FileAttachment", rect, author, contents, [0.2, 0.4, 0.8]);
        annot.set("FS", Object::Reference(spec_id));
        annot.set("Name", Object::Name(b"Paperclip".to_vec()));
        self.finish_annotation(page_id, annot)
    }

    /// Erase the parts of freehand (ink) comments of a page within `radius`
    /// points of the eraser `path`. Strokes are split where they are erased and
    /// empty comments are removed. Returns the number of comments changed.
    pub fn erase_ink(&mut self, page: usize, path: &[(f32, f32)], radius: f32) -> Result<usize> {
        if path.is_empty() {
            return Ok(0);
        }
        let near = |p: (f32, f32)| -> bool {
            if path.len() == 1 {
                return ((p.0 - path[0].0).powi(2) + (p.1 - path[0].1).powi(2)).sqrt() <= radius;
            }
            path.windows(2).any(|w| seg_distance(p, w[0], w[1]) <= radius)
        };
        let annots = self.annotations(page)?;
        let mut changes: Vec<InkEdit> = Vec::new();
        for a in annots.iter().filter(|a| a.subtype == "Ink") {
            let Some(id) = a.id else { continue };
            let d = self.inner.get_dictionary(id)?;
            let strokes: Vec<Vec<(f32, f32)>> = d
                .get(b"InkList")
                .ok()
                .map(|l| deref(&self.inner, l))
                .and_then(|l| l.as_array().ok())
                .map(|paths| {
                    paths
                        .iter()
                        .map(|p| {
                            let v: Vec<f32> = deref(&self.inner, p)
                                .as_array()
                                .map(|a| a.iter().filter_map(num).collect())
                                .unwrap_or_default();
                            v.as_chunks::<2>().0.iter().map(|c| (c[0], c[1])).collect()
                        })
                        .collect()
                })
                .unwrap_or_default();
            let mut kept: Vec<Vec<(f32, f32)>> = Vec::new();
            let mut touched = false;
            for stroke in strokes {
                let mut run: Vec<(f32, f32)> = Vec::new();
                for p in stroke {
                    if near(p) {
                        touched = true;
                        if run.len() >= 2 {
                            kept.push(std::mem::take(&mut run));
                        } else {
                            run.clear();
                        }
                    } else {
                        run.push(p);
                    }
                }
                if run.len() >= 2 {
                    kept.push(run);
                }
            }
            if touched {
                changes.push((a.index, id, kept));
            }
        }
        if changes.is_empty() {
            return Ok(0);
        }
        self.begin_edit();
        let count = changes.len();
        // Remove emptied comments from the end so earlier positions stay valid.
        let mut removed: Vec<ObjectId> = Vec::new();
        for (_, id, kept) in &changes {
            if kept.is_empty() {
                removed.push(*id);
                continue;
            }
            let list: Vec<Object> = kept.iter().map(|s| flat(s)).collect();
            self.inner
                .get_dictionary_mut(*id)?
                .set("InkList", Object::Array(list));
            self.refresh_appearance(*id)?;
        }
        if !removed.is_empty() {
            let page_id = self.page_id(page)?;
            if let Ok(Object::Array(arr)) = self.inner.get_dictionary_mut(page_id)?.get_mut(b"Annots") {
                arr.retain(|o| o.as_reference().map(|r| !removed.contains(&r)).unwrap_or(true));
            }
            for id in removed {
                self.inner.delete_object(id);
            }
        }
        self.commit()?;
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scales_convert_paper_to_real_units() {
        let s = MeasureScale::from_ratio(1.0, "cm", 2.5, "m", 2);
        // 1 cm of paper (28.35 pt) is 2.5 m.
        assert!((s.factor * 72.0 / 2.54 - 2.5).abs() < 1e-4, "{}", s.factor);
        assert_eq!(s.ratio, "1 cm = 2.5 m");
        assert!((MeasureScale::default().factor * 72.0 - 25.4).abs() < 1e-4);
    }

    #[test]
    fn segment_distance() {
        assert_eq!(seg_distance((0.0, 1.0), (-1.0, 0.0), (1.0, 0.0)), 1.0);
        assert_eq!(seg_distance((3.0, 0.0), (-1.0, 0.0), (1.0, 0.0)), 2.0);
    }
}
