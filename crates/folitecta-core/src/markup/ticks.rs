//! Tick marks (FOL-L029): small marks placed with a click — a check, a
//! cross, a dot, or the next number or letter of a sequence, plain or in a
//! circle — in a colour and a size. Each is a stamp that keeps its mark as
//! data (`/FolitectaTick`) and is drawn again from it when it is resized.

use lopdf::{Object, ObjectId};
use serde::{Deserialize, Serialize};

use crate::document::Document;
use crate::error::{Error, Result};
use crate::fonts::TextWriter;
use crate::util::fmt_num;

/// The name of tick mark stamps.
pub const TICK_STAMP: &str = "FolitectaTick";

/// What a tick mark shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum TickShape {
    #[default]
    Check,
    Cross,
    Dot,
    /// Its text alone.
    Text,
    /// Its text in a circle.
    Circled,
}

impl TickShape {
    pub const ALL: [TickShape; 5] = [
        TickShape::Check,
        TickShape::Cross,
        TickShape::Dot,
        TickShape::Text,
        TickShape::Circled,
    ];

    pub fn id(self) -> &'static str {
        match self {
            TickShape::Check => "check",
            TickShape::Cross => "cross",
            TickShape::Dot => "dot",
            TickShape::Text => "text",
            TickShape::Circled => "circled",
        }
    }

    pub fn from_id(id: &str) -> TickShape {
        TickShape::ALL
            .into_iter()
            .find(|s| s.id() == id)
            .unwrap_or_default()
    }

    /// The shape shows a text (the next of a sequence).
    pub fn has_text(self) -> bool {
        matches!(self, TickShape::Text | TickShape::Circled)
    }
}

/// How the texts of a sequence of marks go on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Sequence {
    /// 1, 2, 3, …
    #[default]
    Numbers,
    /// A, B, … Z, AA, AB, …
    Upper,
    /// a, b, … z, aa, ab, …
    Lower,
    /// i, ii, iii, iv, …
    Roman,
}

impl Sequence {
    pub const ALL: [Sequence; 4] = [
        Sequence::Numbers,
        Sequence::Upper,
        Sequence::Lower,
        Sequence::Roman,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Sequence::Numbers => "numbers",
            Sequence::Upper => "upper",
            Sequence::Lower => "lower",
            Sequence::Roman => "roman",
        }
    }

    pub fn from_id(id: &str) -> Sequence {
        Sequence::ALL
            .into_iter()
            .find(|s| s.id() == id)
            .unwrap_or_default()
    }

    /// The `n`-th text of the sequence (`n` from 1).
    pub fn label(self, n: u32) -> String {
        let n = n.max(1);
        match self {
            Sequence::Numbers => n.to_string(),
            Sequence::Upper | Sequence::Lower => {
                // Like the columns of a spreadsheet.
                let base = if self == Sequence::Upper { b'A' } else { b'a' };
                let mut out = Vec::new();
                let mut k = n;
                while k > 0 {
                    k -= 1;
                    out.push((base + (k % 26) as u8) as char);
                    k /= 26;
                }
                out.iter().rev().collect()
            }
            Sequence::Roman => {
                const PARTS: [(u32, &str); 13] = [
                    (1000, "m"),
                    (900, "cm"),
                    (500, "d"),
                    (400, "cd"),
                    (100, "c"),
                    (90, "xc"),
                    (50, "l"),
                    (40, "xl"),
                    (10, "x"),
                    (9, "ix"),
                    (5, "v"),
                    (4, "iv"),
                    (1, "i"),
                ];
                let mut k = n.min(3999);
                let mut out = String::new();
                for (v, s) in PARTS {
                    while k >= v {
                        out.push_str(s);
                        k -= v;
                    }
                }
                out
            }
        }
    }
}

/// A tick mark: its shape, its text (for the shapes that show one), its size
/// (points across) and colour.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TickMark {
    pub shape: TickShape,
    #[serde(default)]
    pub text: String,
    pub size: f32,
    pub color: [f32; 3],
}

impl TickMark {
    /// The rectangle of the mark centred on `at` (PDF user space): a square,
    /// wider for a long text.
    pub fn rect(&self, at: (f32, f32)) -> [f32; 4] {
        let s = self.size.clamp(4.0, 200.0);
        let w = if self.shape.has_text() {
            let writer = TextWriter::new(&self.text).ok();
            let fs = self.font_size(s);
            let tw = writer.map(|w| w.width(&w.piece(&self.text), fs)).unwrap_or(0.0);
            s.max(
                tw + s * if self.shape == TickShape::Circled {
                    0.45
                } else {
                    0.2
                },
            )
        } else {
            s
        };
        [at.0 - w / 2.0, at.1 - s / 2.0, at.0 + w / 2.0, at.1 + s / 2.0]
    }

    fn font_size(&self, side: f32) -> f32 {
        if self.shape == TickShape::Circled {
            side * 0.55
        } else {
            side * 0.8
        }
    }

    /// What the mark says in the list of comments.
    pub fn description(&self) -> String {
        match self.shape {
            TickShape::Check => "\u{2713}".into(),
            TickShape::Cross => "\u{2717}".into(),
            TickShape::Dot => "\u{2022}".into(),
            TickShape::Text | TickShape::Circled => self.text.clone(),
        }
    }

    /// The drawing of the mark filling `rect` (PDF user space), and the
    /// writer of its text when it has one.
    pub(crate) fn drawing(&self, rect: [f32; 4]) -> Result<(String, Option<TextWriter>)> {
        let f = fmt_num;
        let [x0, y0, x1, y1] = crate::geom::normalize(rect);
        let (w, h) = ((x1 - x0).max(1.0), (y1 - y0).max(1.0));
        let s = w.min(h);
        let (cx, cy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
        let c = self.color;
        let rgb = |op: &str| format!("{} {} {} {op} ", f(c[0]), f(c[1]), f(c[2]));
        let at = |u: f32, v: f32| format!("{} {}", f(cx + (u - 0.5) * s), f(cy + (v - 0.5) * s));
        let mut out = String::from("q ");
        let mut writer = None;
        match self.shape {
            TickShape::Check => {
                out.push_str(&rgb("RG"));
                out.push_str(&format!(
                    "{} w 1 J 1 j {} m {} l {} l S ",
                    f(s * 0.13),
                    at(0.14, 0.52),
                    at(0.4, 0.22),
                    at(0.88, 0.8)
                ));
            }
            TickShape::Cross => {
                out.push_str(&rgb("RG"));
                out.push_str(&format!(
                    "{} w 1 J {} m {} l S {} m {} l S ",
                    f(s * 0.13),
                    at(0.2, 0.2),
                    at(0.8, 0.8),
                    at(0.2, 0.8),
                    at(0.8, 0.2)
                ));
            }
            TickShape::Dot => {
                out.push_str(&rgb("rg"));
                out.push_str(&circle(cx, cy, s * 0.3));
                out.push_str("f ");
            }
            TickShape::Text | TickShape::Circled => {
                let mut wr = TextWriter::new(&self.text)?;
                let text = wr.piece(&self.text);
                if text.trim().is_empty() {
                    return Err(Error::Invalid("the tick mark has no text".into()));
                }
                if self.shape == TickShape::Circled {
                    out.push_str(&rgb("RG"));
                    // The circle round the text; wider texts in an oval.
                    out.push_str(&format!("{} w ", f(s * 0.07)));
                    out.push_str(&oval(cx, cy, w / 2.0 - s * 0.05, h / 2.0 - s * 0.05));
                    out.push_str("S ");
                }
                let fs = self.font_size(s);
                let tw = wr.width(&text, fs);
                let place = format!("1 0 0 1 {} {} Tm", f(cx - tw / 2.0), f(cy - fs * 0.35));
                out.push_str(&wr.text_object(&text, fs, Some(c), &place, true, ("TR", "TB"))?);
                out.push(' ');
                writer = Some(wr);
            }
        }
        out.push('Q');
        Ok((out, writer))
    }
}

/// A circle as four Bézier curves.
fn circle(cx: f32, cy: f32, r: f32) -> String {
    oval(cx, cy, r, r)
}

fn oval(cx: f32, cy: f32, rx: f32, ry: f32) -> String {
    let f = fmt_num;
    let (kx, ky) = (rx * 0.5523, ry * 0.5523);
    format!(
        "{} {} m {} {} {} {} {} {} c {} {} {} {} {} {} c {} {} {} {} {} {} c {} {} {} {} {} {} c ",
        f(cx + rx),
        f(cy),
        f(cx + rx),
        f(cy + ky),
        f(cx + kx),
        f(cy + ry),
        f(cx),
        f(cy + ry),
        f(cx - kx),
        f(cy + ry),
        f(cx - rx),
        f(cy + ky),
        f(cx - rx),
        f(cy),
        f(cx - rx),
        f(cy - ky),
        f(cx - kx),
        f(cy - ry),
        f(cx),
        f(cy - ry),
        f(cx + kx),
        f(cy - ry),
        f(cx + rx),
        f(cy - ky),
        f(cx + rx),
        f(cy)
    )
}

impl Document {
    /// Add `mark` centred on `at` (PDF user space) of `page`. Text that cannot
    /// be drawn is refused before anything changes.
    pub fn add_tick_mark(
        &mut self,
        page: usize,
        at: (f32, f32),
        mark: &TickMark,
        author: &str,
    ) -> Result<ObjectId> {
        let page_id = self.page_id(page)?;
        let rect = mark.rect(at);
        let json = serde_json::to_string(mark).map_err(|e| Error::Invalid(e.to_string()))?;
        let (content, writer) = mark.drawing(rect)?;
        self.begin_edit();
        let mut annot = self.base_annotation(page_id, "Stamp", rect, author, &mark.description(), mark.color);
        annot.set("Name", Object::Name(TICK_STAMP.as_bytes().to_vec()));
        annot.set("Subj", lopdf::text_string("Tick Mark"));
        annot.set("FolitectaTick", Object::string_literal(json));
        let res = match writer.map(|w| self.drawing_resources(w, None)).transpose() {
            Ok(res) => res.unwrap_or_default(),
            Err(e) => {
                self.abandon_edit();
                return Err(e);
            }
        };
        self.add_appearance(&mut annot, rect, content, res);
        let id = self.push_drawn_annotation(page_id, annot)?;
        self.commit()?;
        Ok(id)
    }

    /// The mark of a comment, when it is a tick mark.
    pub fn tick_mark_at(&self, page: usize, index: usize) -> Option<TickMark> {
        let id = self.annotation_object(page, index).ok()?;
        match self.inner.get_dictionary(id).ok()?.get(b"FolitectaTick").ok()? {
            Object::String(s, _) => serde_json::from_slice(s).ok(),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sequences_go_on() {
        let labels = |s: Sequence, ns: &[u32]| ns.iter().map(|n| s.label(*n)).collect::<Vec<_>>();
        assert_eq!(labels(Sequence::Numbers, &[1, 2, 10]), ["1", "2", "10"]);
        assert_eq!(
            labels(Sequence::Upper, &[1, 26, 27, 28, 52, 53, 702, 703]),
            ["A", "Z", "AA", "AB", "AZ", "BA", "ZZ", "AAA"]
        );
        assert_eq!(labels(Sequence::Lower, &[1, 3]), ["a", "c"]);
        assert_eq!(
            labels(Sequence::Roman, &[1, 4, 9, 14, 40, 1994]),
            ["i", "iv", "ix", "xiv", "xl", "mcmxciv"]
        );
        assert_eq!(Sequence::from_id("roman"), Sequence::Roman);
        assert_eq!(TickShape::from_id("nothing"), TickShape::Check);
    }

    #[test]
    fn marks_fit_their_text() {
        let mark = |shape, text: &str| TickMark {
            shape,
            text: text.into(),
            size: 20.0,
            color: [0.0; 3],
        };
        let r = mark(TickShape::Check, "").rect((100.0, 100.0));
        assert_eq!(r, [90.0, 90.0, 110.0, 110.0]);
        let wide = mark(TickShape::Circled, "123").rect((100.0, 100.0));
        assert!(wide[2] - wide[0] > 20.0 && wide[3] - wide[1] == 20.0, "{wide:?}");
        assert!(
            mark(TickShape::Text, " ")
                .drawing([0.0, 0.0, 20.0, 20.0])
                .is_err()
        );
    }
}
