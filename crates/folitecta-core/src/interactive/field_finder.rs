//! Form fields found on a flat form (FOL-L014): the places a printed form
//! leaves to write in, proposed as fields to add: runs of underscores and
//! lines drawn with nothing written on them (a text field above each),
//! empty boxes (a text field in each, of several lines when tall) and small
//! squares (a check box); a row of three squares or more side by side is a
//! comb, a text field of one character a square. Each is named after the words beside it ("Name:"
//! before a line, "I agree" after a square, "Comments" above a box). Places
//! where a field already is are left out.

use std::ops::Range;

use crate::document::{Document, PageSize};
use crate::error::Result;
use crate::form_design::{FieldProps, NewField};
use crate::marks::{display_to_pdf, pdf_rect_to_display};
use crate::textlayer::PageText;

/// A field proposed on a page.
#[derive(Debug, Clone, PartialEq)]
pub struct FieldProposal {
    pub page: usize,
    /// [`NewField::Text`] or [`NewField::CheckBox`].
    pub kind: NewField,
    /// Several lines (a tall box).
    pub multiline: bool,
    /// Its rectangle (PDF user space of the page).
    pub rect: [f32; 4],
    /// The words beside it, the name it will have (empty: none found).
    pub label: String,
    /// A comb: its number of squares, one character each (0: none).
    pub comb: u32,
}

/// The height of a text field over a line, in points.
const LINE_FIELD: f32 = 16.0;
/// How far away words may be to name a field, in points.
const LABEL_REACH: f32 = 220.0;

fn overlap(a: [f32; 4], b: [f32; 4]) -> f32 {
    let w = (a[2].min(b[2]) - a[0].max(b[0])).max(0.0);
    let h = (a[3].min(b[3]) - a[1].max(b[1])).max(0.0);
    w * h
}

fn area(a: [f32; 4]) -> f32 {
    (a[2] - a[0]).max(0.0) * (a[3] - a[1]).max(0.0)
}

/// Words of the glyphs of `text` whose middles lie in the band `y0..y1`
/// (display space) and in `x0..x1`, joined left to right: of words far
/// apart, the last group when `keep_last` (before a field), else the first.
fn words_in(text: &PageText, band: (f32, f32), xs: (f32, f32), keep_last: bool) -> String {
    let mut glyphs: Vec<_> = text
        .glyphs
        .iter()
        .filter(|g| {
            let (mx, my) = ((g.bbox[0] + g.bbox[2]) / 2.0, (g.bbox[1] + g.bbox[3]) / 2.0);
            my >= band.0 && my <= band.1 && mx >= xs.0 && mx <= xs.1 && !g.text.contains('_')
        })
        .collect();
    glyphs.sort_by(|a, b| a.bbox[0].total_cmp(&b.bbox[0]));
    let mut out = String::new();
    let mut last: Option<(f32, f32)> = None;
    for g in glyphs {
        if let Some((x1, size)) = last {
            let gap = g.bbox[0] - x1;
            // Words far apart are not one label: keep the nearest group.
            if gap > 2.5 * size.max(g.size) {
                if !keep_last {
                    break;
                }
                out.clear();
            } else if gap > 0.22 * size.max(g.size) && !out.ends_with(' ') {
                out.push(' ');
            }
        }
        out.push_str(&g.text);
        last = Some((g.bbox[2], g.size));
    }
    out
}

/// A label made fit to name a field: its last words (before a line) or
/// first ones, without the colon and other marks at its ends.
fn tidy(label: &str, last: bool) -> String {
    let words: Vec<&str> = label.split_whitespace().collect();
    let take: Vec<&str> = if last {
        words[words.len().saturating_sub(5)..].to_vec()
    } else {
        words.into_iter().take(5).collect()
    };
    take.join(" ")
        .trim_matches(|c: char| !c.is_alphanumeric())
        .chars()
        .filter(|c| !matches!(c, '.' | '[' | ']'))
        .take(40)
        .collect::<String>()
        .trim()
        .to_string()
}

/// Any glyph inside `r` (display space).
fn has_text(text: &PageText, r: [f32; 4]) -> bool {
    text.glyphs.iter().any(|g| {
        let (mx, my) = ((g.bbox[0] + g.bbox[2]) / 2.0, (g.bbox[1] + g.bbox[3]) / 2.0);
        !g.text.trim().is_empty() && mx > r[0] && mx < r[2] && my > r[1] && my < r[3]
    })
}

/// A display rectangle in PDF user space.
fn to_pdf(size: &PageSize, r: [f32; 4]) -> [f32; 4] {
    let (ax, ay) = display_to_pdf(size, r[0], size.height - r[1]);
    let (bx, by) = display_to_pdf(size, r[2], size.height - r[3]);
    [ax.min(bx), ay.min(by), ax.max(bx), ay.max(by)]
}

/// Proposals of one page, in display space: kind, several lines, area,
/// label.
fn page_proposals(
    text: &PageText,
    paths: &[crate::paths::PagePath],
    size: &PageSize,
) -> Vec<(NewField, bool, [f32; 4], String, u32)> {
    let mut out = Vec::new();
    // Runs of underscores in the text.
    let chars: Vec<char> = text.text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] != '_' {
            i += 1;
            continue;
        }
        let start = i;
        while i < chars.len() && chars[i] == '_' {
            i += 1;
        }
        if i - start < 4 {
            continue;
        }
        let Some(r) = text.range_rects(start, i).into_iter().next() else {
            continue;
        };
        let h = (r[3] - r[1]).max(12.0);
        let field = [r[0], r[3] - h, r[2], r[3]];
        let band = (r[1], r[3]);
        let mut label = tidy(&words_in(text, band, (r[0] - LABEL_REACH, r[0]), true), true);
        if label.is_empty() {
            label = tidy(&words_in(text, band, (r[2], r[2] + LABEL_REACH), false), false);
        }
        out.push((NewField::Text, false, field, label, 0));
    }
    for p in paths.iter().filter(|p| p.stroked || p.filled) {
        let rects = p.rectangles();
        let shapes: Vec<[f32; 4]> = if rects.is_empty() {
            vec![p.bounds]
        } else {
            rects.clone()
        };
        for shape in shapes {
            let b = pdf_rect_to_display(size, shape);
            let (w, h) = (b[2] - b[0], b[3] - b[1]);
            // A page border or a frame around much of it is no field.
            if w > 0.8 * size.width && h > 0.4 * size.height {
                continue;
            }
            if h <= 2.5 && w >= 40.0 {
                // A line to write on: nothing written just above it.
                let field = [b[0], b[1] - LINE_FIELD, b[2], b[1]];
                if has_text(text, [field[0], field[1] + 2.0, field[2], field[3]]) {
                    continue;
                }
                let band = (field[1] - 2.0, field[3] + 1.0);
                let label = tidy(&words_in(text, band, (b[0] - LABEL_REACH, b[0]), true), true);
                out.push((NewField::Text, false, field, label, 0));
            } else if p.stroked && !rects.is_empty() && !has_text(text, b) {
                let square = (6.0..=24.0).contains(&w) && (w - h).abs() <= 0.25 * w;
                let band = (b[1] - 2.0, b[3] + 2.0);
                if square {
                    let mut label = tidy(&words_in(text, band, (b[2], b[2] + LABEL_REACH), false), false);
                    if label.is_empty() {
                        label = tidy(&words_in(text, band, (b[0] - LABEL_REACH, b[0]), true), true);
                    }
                    out.push((NewField::CheckBox, false, b, label, 0));
                } else if w >= 40.0 && (10.0..=300.0).contains(&h) {
                    let line_band = (b[1], (b[1] + 18.0).min(b[3]));
                    let mut label = tidy(&words_in(text, line_band, (b[0] - LABEL_REACH, b[0]), true), true);
                    if label.is_empty() {
                        // The words just above the box.
                        label = tidy(&words_in(text, (b[1] - 16.0, b[1]), (b[0], b[2]), false), false);
                    }
                    let inset = [b[0] + 1.0, b[1] + 1.0, b[2] - 1.0, b[3] - 1.0];
                    out.push((NewField::Text, h > 30.0, inset, label, 0));
                }
            }
        }
    }
    combs(text, out)
}

/// Rows of three squares or more side by side, alike, become one comb.
fn combs(
    text: &PageText,
    found: Vec<(NewField, bool, [f32; 4], String, u32)>,
) -> Vec<(NewField, bool, [f32; 4], String, u32)> {
    let (mut squares, mut out): (Vec<_>, Vec<_>) = found.into_iter().partition(|p| p.0 == NewField::CheckBox);
    squares.sort_by(|a, b| a.2[1].total_cmp(&b.2[1]).then(a.2[0].total_cmp(&b.2[0])));
    let mut i = 0;
    while i < squares.len() {
        let mut j = i + 1;
        while j < squares.len() {
            let (a, b) = (squares[j - 1].2, squares[j].2);
            let side = |r: [f32; 4]| (r[2] - r[0], r[3] - r[1]);
            let alike = (side(a).0 - side(b).0).abs() <= 1.0 && (side(a).1 - side(b).1).abs() <= 1.0;
            let beside = (a[1] - b[1]).abs() <= 1.5 && b[0] - a[2] <= 2.0 && b[0] - a[2] >= -1.0;
            if !(alike && beside) {
                break;
            }
            j += 1;
        }
        if j - i >= 3 {
            let (first, last) = (squares[i].2, squares[j - 1].2);
            let r = [first[0], first[1], last[2], last[3]];
            let band = (r[1] - 2.0, r[3] + 2.0);
            let mut label = tidy(&words_in(text, band, (r[0] - LABEL_REACH, r[0]), true), true);
            if label.is_empty() {
                label = tidy(&words_in(text, band, (r[2], r[2] + LABEL_REACH), false), false);
            }
            out.push((NewField::Text, false, r, label, (j - i) as u32));
        } else {
            out.extend(squares[i..j].iter().cloned());
        }
        i = j;
    }
    out
}

impl Document {
    /// The fields a flat form on `pages` leaves room for (see the module),
    /// page by page from the top; none where fields already are.
    pub fn find_form_fields(&self, pages: Range<usize>) -> Vec<FieldProposal> {
        let layers = self.text_layers();
        let mut out = Vec::new();
        for page in pages {
            let (Some(text), Ok(size)) = (layers.get(page), self.page_size(page)) else {
                continue;
            };
            let paths = self.page_paths(page).unwrap_or_default();
            let taken: Vec<[f32; 4]> = self
                .annotations(page)
                .map(|a| {
                    a.into_iter()
                        .filter(|a| a.subtype == "Widget")
                        .map(|a| pdf_rect_to_display(&size, a.rect))
                        .collect()
                })
                .unwrap_or_default();
            let mut kept: Vec<(NewField, bool, [f32; 4], String, u32)> = Vec::new();
            for p in page_proposals(text, &paths, &size) {
                let clash = |r: [f32; 4]| overlap(r, p.2) > 0.3 * area(p.2).min(area(r)).max(1.0);
                if taken.iter().any(|r| clash(*r)) || kept.iter().any(|k| clash(k.2)) {
                    continue;
                }
                kept.push(p);
            }
            kept.sort_by(|a, b| a.2[1].total_cmp(&b.2[1]).then(a.2[0].total_cmp(&b.2[0])));
            out.extend(
                kept.into_iter()
                    .map(|(kind, multiline, r, label, comb)| FieldProposal {
                        page,
                        kind,
                        multiline,
                        rect: to_pdf(&size, r),
                        label,
                        comb,
                    }),
            );
        }
        out
    }

    /// Add the fields `found` proposes, each named after its label (made
    /// unique) and described by it. One undo step; how many were added.
    pub fn add_found_fields(&mut self, found: &[FieldProposal]) -> Result<usize> {
        let mark = self.undo_mark();
        let mut made = 0;
        for f in found {
            let base = if f.label.is_empty() {
                match f.kind {
                    NewField::CheckBox => "Check",
                    _ => "Text",
                }
                .to_string()
            } else {
                f.label.clone()
            };
            let names: Vec<String> = self.form_fields().into_iter().map(|f| f.name).collect();
            let name = std::iter::once(base.clone())
                .chain((2..).map(|k| format!("{base} {k}")))
                .find(|n| !names.contains(n))
                .unwrap_or(base);
            let props = FieldProps {
                name,
                tooltip: f.label.clone(),
                multiline: f.multiline,
                comb: f.comb > 1,
                max_len: f.comb,
                ..FieldProps::default()
            };
            if let Err(e) = self.add_form_field(f.page, f.kind, f.rect, &props) {
                // None of them, as before.
                self.squash_undo(&mark);
                if made > 0 {
                    let _ = self.undo();
                }
                return Err(e);
            }
            made += 1;
        }
        self.squash_undo(&mark);
        Ok(made)
    }
}
