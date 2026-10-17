//! Accounting calculator comments: a tape of amounts and operations, with
//! running totals, drawn on the page and kept in the comment so it can be
//! edited again.

use lopdf::{Object, ObjectId};
use serde::{Deserialize, Serialize};

use crate::document::Document;
use crate::error::{Error, Result};
use crate::util::{fmt_num, helvetica_width, pdf_literal};

/// `/Name` of the stamp comments that hold a tape.
pub const CALC_STAMP: &str = "FolitectaCalculator";

/// Operations of a tape line: add, subtract, multiply, divide, total.
pub const OPERATIONS: &[char] = &['+', '-', '*', '/', '='];

/// One line of the tape. A total line (`=`) shows the running total.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TapeLine {
    pub op: char,
    pub amount: f64,
    #[serde(default)]
    pub comment: String,
}

impl TapeLine {
    pub fn new(op: char, amount: f64) -> Self {
        TapeLine {
            op,
            amount,
            comment: String::new(),
        }
    }
}

/// How results are rounded to the decimals of the tape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Rounding {
    /// Half away from zero.
    #[default]
    Default,
    /// Away from zero.
    Up,
    /// Toward zero.
    Down,
}

/// How the tape looks on the page.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TapeStyle {
    pub background: Option<[f32; 3]>,
    pub border_color: [f32; 3],
    pub border_width: f32,
    #[serde(default)]
    pub border_dashed: bool,
    pub font_size: f32,
    pub text_color: [f32; 3],
}

impl Default for TapeStyle {
    fn default() -> Self {
        TapeStyle {
            background: Some([1.0, 1.0, 0.94]),
            border_color: [0.45, 0.45, 0.45],
            border_width: 1.0,
            border_dashed: false,
            font_size: 10.0,
            text_color: [0.0, 0.0, 0.0],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Tape {
    pub lines: Vec<TapeLine>,
    /// Decimals shown; `None` for a floating point.
    pub decimals: Option<u8>,
    #[serde(default)]
    pub rounding: Rounding,
    #[serde(default)]
    pub show_date: bool,
    /// Date and time written at the top when shown.
    #[serde(default)]
    pub date: String,
    #[serde(default)]
    pub style: TapeStyle,
}

impl Default for Tape {
    fn default() -> Self {
        Tape {
            lines: Vec::new(),
            decimals: Some(2),
            rounding: Rounding::Default,
            show_date: false,
            date: String::new(),
            style: TapeStyle::default(),
        }
    }
}

impl Tape {
    /// Round a value to the decimals of the tape.
    pub fn round(&self, v: f64) -> f64 {
        let Some(d) = self.decimals else { return v };
        let m = 10f64.powi(d.min(10) as i32);
        let x = v * m;
        // Binary fractions are a little off (2.675 is stored as 2.67499...).
        let eps = 1e-9 * x.abs().max(1.0);
        let r = match self.rounding {
            Rounding::Default => (x + eps * x.signum()).round(),
            Rounding::Up => {
                if x >= 0.0 {
                    (x - eps).ceil()
                } else {
                    (x + eps).floor()
                }
            }
            Rounding::Down => (x + eps * x.signum()).trunc(),
        };
        r / m
    }

    /// Running total after each line (results rounded to the decimals).
    pub fn totals(&self) -> Vec<f64> {
        let mut total = 0.0;
        self.lines
            .iter()
            .map(|l| {
                total = match l.op {
                    '+' => total + l.amount,
                    '-' => total - l.amount,
                    '*' => total * l.amount,
                    '/' if l.amount != 0.0 => total / l.amount,
                    _ => total,
                };
                total = self.round(total);
                total
            })
            .collect()
    }

    pub fn total(&self) -> f64 {
        self.totals().last().copied().unwrap_or(0.0)
    }

    /// A value as the tape shows it, with thousands separators.
    pub fn format(&self, v: f64) -> String {
        let v = self.round(v);
        let s = match self.decimals {
            Some(d) => format!("{:.*}", d.min(10) as usize, v.abs()),
            None => {
                let s = format!("{:.8}", v.abs());
                s.trim_end_matches('0').trim_end_matches('.').to_string()
            }
        };
        let (int, frac) = match s.split_once('.') {
            Some((i, f)) => (i.to_string(), Some(f.to_string())),
            None => (s.clone(), None),
        };
        let mut grouped = String::new();
        for (i, c) in int.chars().enumerate() {
            if i > 0 && (int.len() - i) % 3 == 0 {
                grouped.push(',');
            }
            grouped.push(c);
        }
        let sign = if v < 0.0 { "-" } else { "" };
        match frac {
            Some(f) => format!("{sign}{grouped}.{f}"),
            None => format!("{sign}{grouped}"),
        }
    }

    /// Rows as drawn: comment, value, operation, whether it is a total.
    pub fn rows(&self) -> Vec<(String, String, char, bool)> {
        let totals = self.totals();
        self.lines
            .iter()
            .zip(totals)
            .map(|(l, t)| {
                if l.op == '=' {
                    (l.comment.clone(), self.format(t), '=', true)
                } else {
                    (l.comment.clone(), self.format(l.amount), l.op, false)
                }
            })
            .collect()
    }

    /// The tape as plain text (for the comment and the clipboard).
    pub fn text(&self) -> String {
        let mut out = Vec::new();
        if self.show_date && !self.date.is_empty() {
            out.push(self.date.clone());
        }
        for (comment, value, op, _) in self.rows() {
            let line = format!("{value} {op}");
            out.push(if comment.is_empty() {
                line
            } else {
                format!("{line}  {comment}")
            });
        }
        out.push(format!("{} T", self.format(self.total())));
        out.join("\n")
    }

    /// Height the tape needs (points).
    pub fn height(&self) -> f32 {
        let size = self.style.font_size.max(4.0);
        let lines = self.lines.len() + 1 + usize::from(self.show_date && !self.date.is_empty());
        size * 1.35 * lines as f32 + size * 1.2
    }

    /// Content stream drawing the tape inside `rect` (PDF space).
    pub fn content(&self, rect: [f32; 4]) -> String {
        let f = fmt_num;
        let s = &self.style;
        let (x0, y0, x1, y1) = (rect[0], rect[1], rect[2], rect[3]);
        let mut c = String::new();
        if let Some(bg) = s.background {
            c.push_str(&format!(
                "{} {} {} rg {} {} {} {} re f\n",
                f(bg[0]),
                f(bg[1]),
                f(bg[2]),
                f(x0),
                f(y0),
                f(x1 - x0),
                f(y1 - y0)
            ));
        }
        let size = s.font_size.max(4.0);
        let lead = size * 1.35;
        let pad = size * 0.6;
        let tc = s.text_color;
        let mut y = y1 - pad - size * 0.85;
        let mut text = |font: &str, x: f32, y: f32, t: &str| {
            c.push_str(&format!(
                "BT /{font} {} Tf {} {} {} rg {} {} Td {} Tj ET\n",
                f(size),
                f(tc[0]),
                f(tc[1]),
                f(tc[2]),
                f(x),
                f(y),
                String::from_utf8_lossy(&pdf_literal(t))
            ));
        };
        let op_w = helvetica_width("*", size) * 1.6;
        let right = x1 - pad - op_w;
        if self.show_date && !self.date.is_empty() {
            text("TR", x0 + pad, y, &self.date);
            y -= lead;
        }
        for (comment, value, op, total) in self.rows() {
            let font = if total { "TB" } else { "TR" };
            let factor = if total { 1.06 } else { 1.0 };
            let vw = helvetica_width(&value, size) * factor;
            text(font, right - vw, y, &value);
            let opt = if op == '*' {
                "×".to_string()
            } else if op == '/' {
                "÷".to_string()
            } else {
                op.to_string()
            };
            text(font, right + op_w * 0.3, y, &opt);
            if !comment.is_empty() {
                // The comment fills the room left of the value.
                let room = (right - vw - x0 - 2.0 * pad).max(0.0);
                let mut shown = comment.clone();
                while !shown.is_empty() && helvetica_width(&shown, size) > room {
                    shown.pop();
                }
                if !shown.is_empty() {
                    text("TR", x0 + pad, y, &shown);
                }
            }
            y -= lead;
        }
        // The grand total under a rule.
        c.push_str(&format!(
            "{} {} {} RG 0.6 w {} {} m {} {} l S\n",
            f(tc[0]),
            f(tc[1]),
            f(tc[2]),
            f(x0 + pad),
            f(y + size * 1.05),
            f(x1 - pad),
            f(y + size * 1.05)
        ));
        let total = self.format(self.total());
        let tw = helvetica_width(&total, size) * 1.06;
        let mut text2 = |font: &str, x: f32, t: &str| {
            c.push_str(&format!(
                "BT /{font} {} Tf {} {} {} rg {} {} Td {} Tj ET\n",
                f(size),
                f(tc[0]),
                f(tc[1]),
                f(tc[2]),
                f(x),
                f(y),
                String::from_utf8_lossy(&pdf_literal(t))
            ));
        };
        text2("TB", right - tw, &total);
        text2("TB", right + op_w * 0.3, "T");
        if s.border_width > 0.0 {
            let b = s.border_color;
            let hw = s.border_width / 2.0;
            c.push_str(&format!(
                "{} {} {} RG {} w {} {} {} {} {} re S\n",
                f(b[0]),
                f(b[1]),
                f(b[2]),
                f(s.border_width),
                if s.border_dashed { "[3 2] 0 d" } else { "[] 0 d" },
                f(x0 + hw),
                f(y0 + hw),
                f(x1 - x0 - s.border_width),
                f(y1 - y0 - s.border_width)
            ));
        }
        c
    }
}

impl Document {
    /// Add a calculator tape at `rect` (PDF space; its height follows the tape).
    pub fn add_calculator(
        &mut self,
        page: usize,
        rect: [f32; 4],
        tape: &Tape,
        author: &str,
    ) -> Result<ObjectId> {
        let page_id = self.page_id(page)?;
        let x0 = rect[0].min(rect[2]);
        let x1 = rect[0].max(rect[2]).max(x0 + 120.0);
        let top = rect[1].max(rect[3]);
        let r = [x0, top - tape.height(), x1, top];
        let json = serde_json::to_string(tape).map_err(|e| Error::Invalid(e.to_string()))?;
        self.begin_edit();
        let mut annot =
            self.base_annotation(page_id, "Stamp", r, author, &tape.text(), tape.style.border_color);
        annot.set("Name", Object::Name(CALC_STAMP.as_bytes().to_vec()));
        annot.set("Subj", lopdf::text_string("Accounting Calculator"));
        annot.set("FolitectaCalc", Object::string_literal(json));
        let res = self.table_resources();
        self.add_appearance(&mut annot, r, tape.content(r), res);
        let id = self.push_annotation(page_id, annot)?;
        self.commit()?;
        Ok(id)
    }

    /// The tape of a comment, when it is a calculator.
    pub fn calculator_at(&self, page: usize, index: usize) -> Option<Tape> {
        let id = self.annotation_object(page, index).ok()?;
        let d = self.inner.get_dictionary(id).ok()?;
        match d.get(b"FolitectaCalc").ok()? {
            Object::String(s, _) => serde_json::from_slice(s).ok(),
            _ => None,
        }
    }

    /// Replace the tape of a calculator comment and redraw it (its height
    /// follows the number of lines, its top stays).
    pub fn set_calculator(&mut self, page: usize, index: usize, tape: &Tape) -> Result<()> {
        if self.calculator_at(page, index).is_none() {
            return Err(Error::Invalid("the comment is not a calculator".into()));
        }
        let id = self.annotation_object(page, index)?;
        let old = self
            .annotations(page)?
            .get(index)
            .map(|a| a.rect)
            .ok_or_else(|| Error::Invalid("calculator not found".into()))?;
        let r = [old[0], old[3] - tape.height(), old[2], old[3]];
        let json = serde_json::to_string(tape).map_err(|e| Error::Invalid(e.to_string()))?;
        self.begin_edit();
        let res = self.table_resources();
        let mut annot = self.inner.get_dictionary(id)?.clone();
        annot.set("FolitectaCalc", Object::string_literal(json));
        annot.set("Contents", lopdf::text_string(&tape.text()));
        annot.set(
            "Rect",
            Object::Array(r.iter().map(|v| Object::Real(*v)).collect()),
        );
        self.add_appearance(&mut annot, r, tape.content(r), res);
        *self.inner.get_dictionary_mut(id)? = annot;
        self.commit()
    }

    /// Calculators of the document: `(page, index)`.
    pub fn calculators(&self) -> Vec<(usize, usize)> {
        let mut out = Vec::new();
        for p in 0..self.page_count() {
            for a in self.annotations(p).unwrap_or_default() {
                if a.subtype == "Stamp" && self.calculator_at(p, a.index).is_some() {
                    out.push((p, a.index));
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tape(lines: &[(char, f64)]) -> Tape {
        Tape {
            lines: lines.iter().map(|(o, a)| TapeLine::new(*o, *a)).collect(),
            ..Tape::default()
        }
    }

    #[test]
    fn running_totals_and_rounding() {
        let mut t = tape(&[
            ('+', 10.0),
            ('+', 5.25),
            ('-', 2.0),
            ('=', 0.0),
            ('*', 3.0),
            ('/', 4.0),
        ]);
        assert_eq!(t.totals(), vec![10.0, 15.25, 13.25, 13.25, 39.75, 9.94]);
        assert_eq!(t.format(1234567.891), "1,234,567.89");
        assert_eq!(t.format(-0.5), "-0.50");
        t.decimals = Some(0);
        assert_eq!(t.format(2.5), "3");
        t.rounding = Rounding::Down;
        assert_eq!(t.format(2.9), "2");
        t.rounding = Rounding::Up;
        assert_eq!(t.format(2.1), "3");
        assert_eq!(t.format(-2.1), "-3");
        t.decimals = None;
        assert_eq!(t.format(1.0 / 3.0), "0.33333333");
        assert_eq!(t.format(2.5), "2.5");
        // Division by zero leaves the total.
        let z = tape(&[('+', 4.0), ('/', 0.0)]);
        assert_eq!(z.total(), 4.0);
        let r = tape(&[('+', 2.675)]);
        assert_eq!(r.format(r.total()), "2.68");
    }

    #[test]
    fn text_and_rows() {
        let mut t = tape(&[('+', 100.0), ('-', 25.5), ('=', 0.0)]);
        t.lines[0].comment = "Rent".into();
        let rows = t.rows();
        assert_eq!(rows[2], (String::new(), "74.50".to_string(), '=', true));
        assert_eq!(t.text(), "100.00 +  Rent\n25.50 -\n74.50 =\n74.50 T");
        let c = t.content([0.0, 0.0, 150.0, t.height()]);
        assert!(c.contains("(Rent) Tj") && c.contains("(74.50) Tj"));
    }
}
