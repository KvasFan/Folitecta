//! CSV to PDF (FOL-L005): a table of the rows of a CSV file, its separator
//! guessed (comma, semicolon as where the comma is the decimal mark, tab or
//! bar), the header row repeated on every page, numbers aligned right, the
//! pages turned sideways when the table is wider than an upright page.

use crate::document::Document;
use crate::error::{Error, Result};
use crate::textdoc::TextLayout;
use crate::typeset::{Align, Fonts, Sheet, Span, Typeset, column_widths};

/// The separators a CSV file may use, in the order they are preferred.
pub const SEPARATORS: [char; 4] = [',', ';', '\t', '|'];

/// The records of `text` with `separator` between the fields: fields in
/// double quotes may hold separators, line breaks and doubled quotes.
pub fn parse_csv(text: &str, separator: char) -> Vec<Vec<String>> {
    let text = text.trim_start_matches('\u{feff}');
    let mut rows = Vec::new();
    let mut row: Vec<String> = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut chars = text.chars().peekable();
    let mut any = false;
    while let Some(c) = chars.next() {
        any = true;
        if quoted {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    field.push('"');
                } else {
                    quoted = false;
                }
            } else {
                field.push(c);
            }
        } else if c == '"' && field.trim().is_empty() {
            field.clear();
            quoted = true;
        } else if c == separator {
            row.push(std::mem::take(&mut field));
        } else if c == '\r' || c == '\n' {
            if c == '\r' && chars.peek() == Some(&'\n') {
                chars.next();
            }
            row.push(std::mem::take(&mut field));
            rows.push(std::mem::take(&mut row));
            any = false;
        } else {
            field.push(c);
        }
    }
    if any {
        row.push(field);
        rows.push(row);
    }
    // Empty lines are no records.
    rows.retain(|r| !(r.len() == 1 && r[0].trim().is_empty()));
    rows
}

/// The separator of `text`: the one that splits its first records into the
/// same number of fields (more than one), the first such in [`SEPARATORS`].
pub fn guess_separator(text: &str) -> char {
    let sample: String = text.lines().take(30).collect::<Vec<_>>().join("\n");
    let mut best = (',', 0usize, 0usize);
    for sep in SEPARATORS {
        let rows = parse_csv(&sample, sep);
        let Some(first) = rows.first() else { continue };
        let n = first.len();
        if n < 2 {
            continue;
        }
        let same = rows.iter().filter(|r| r.len() == n).count();
        // Records that agree, then fields.
        if (same, n) > (best.1, best.2) {
            best = (sep, same, n);
        }
    }
    best.0
}

/// A number as spreadsheets write it: a sign, digits with group and decimal
/// marks, a currency or a percent sign.
fn number_like(s: &str) -> bool {
    let s = s.trim();
    let s = s.trim_start_matches(['+', '-', '−', '(']).trim_end_matches(')');
    let s = s.trim_matches(|c: char| matches!(c, '€' | '$' | '£' | '¥' | '%' | ' ' | '\u{a0}'));
    !s.is_empty()
        && s.chars().any(|c| c.is_ascii_digit())
        && s.chars()
            .all(|c| c.is_ascii_digit() || matches!(c, '.' | ',' | '\'' | ' ' | '\u{a0}' | '\u{202f}'))
}

/// Columns most of whose cells are numbers.
fn numeric_columns(rows: &[Vec<String>], n: usize) -> Vec<bool> {
    (0..n)
        .map(|c| {
            let cells: Vec<&str> = rows
                .iter()
                .filter_map(|r| r.get(c))
                .map(|s| s.as_str())
                .filter(|s| !s.trim().is_empty())
                .collect();
            !cells.is_empty() && cells.iter().filter(|s| number_like(s)).count() * 5 >= cells.len() * 4
        })
        .collect()
}

impl Document {
    /// A new document with the records of CSV `text` as a table (the
    /// separator guessed when `separator` is None). The first record is the
    /// header unless it looks like data (numbers where the column has them).
    pub fn from_csv(
        text: &str,
        separator: Option<char>,
        layout: TextLayout,
        title: &str,
    ) -> Result<Document> {
        let sep = separator.unwrap_or_else(|| guess_separator(text));
        let mut rows = parse_csv(text, sep);
        if rows.is_empty() {
            return Err(Error::Invalid("the file has no records".into()));
        }
        let n = rows.iter().map(|r| r.len()).max().unwrap_or(1);
        for r in &mut rows {
            r.resize(n, String::new());
        }
        let numeric = numeric_columns(&rows[1..], n);
        let first_is_data = rows.len() > 1
            && numeric
                .iter()
                .zip(&rows[0])
                .any(|(num, cell)| *num && number_like(cell));
        let head = (!first_is_data).then(|| rows.remove(0));
        let fonts = Fonts::new(rows.iter().chain(head.iter()).flatten().map(String::as_str))?;
        let size = (layout.font_size * 0.82).clamp(5.0, 20.0);
        let pad = size * 0.4;
        let margin = layout
            .margin
            .clamp(18.0, layout.page_width.min(layout.page_height) / 4.0)
            * 0.7;
        let spans = |r: &Vec<String>| r.iter().map(|c| vec![Span::plain(c.trim())]).collect::<Vec<_>>();
        let head_cells = head.as_ref().map(spans);
        let body: Vec<Vec<Vec<Span>>> = rows.iter().map(spans).collect();
        let aligns: Vec<Align> = numeric
            .iter()
            .map(|num| if *num { Align::Right } else { Align::Left })
            .collect();
        // Upright, unless the table needs the width of the page turned.
        let (short, long) = (
            layout.page_width.min(layout.page_height),
            layout.page_width.max(layout.page_height),
        );
        let upright = Sheet {
            width: short,
            height: long,
            margin,
        };
        let probe = Typeset::new(upright, fonts);
        let natural: f32 = column_widths(
            &probe,
            head_cells.as_deref().unwrap_or(&[]),
            &body,
            size,
            f32::MAX,
            pad,
        )
        .iter()
        .sum();
        let sideways = natural > short - 2.0 * margin;
        let sheet = if sideways {
            Sheet {
                width: long,
                height: short,
                margin,
            }
        } else {
            upright
        };
        let mut ts = Typeset::new(sheet, probe.fonts);
        let width = ts.right() - ts.left();
        let widths = column_widths(&ts, head_cells.as_deref().unwrap_or(&[]), &body, size, width, pad);
        let x0 = ts.left();
        ts.table(
            head_cells.as_deref(),
            &body,
            &widths,
            &aligns,
            size,
            size * 1.3,
            x0,
            pad,
        )?;
        ts.finish(title)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quoted_fields_hold_separators_breaks_and_quotes() {
        let rows = parse_csv("a,\"b, c\",\"say \"\"hi\"\"\"\r\n1,\"two\nlines\",3\n\n", ',');
        assert_eq!(
            rows,
            [vec!["a", "b, c", "say \"hi\""], vec!["1", "two\nlines", "3"]]
        );
        assert_eq!(parse_csv("x;y", ';'), [vec!["x", "y"]]);
        assert!(parse_csv("", ',').is_empty());
    }

    #[test]
    fn separators_are_guessed() {
        assert_eq!(
            guess_separator("nome;importo\nRossi;1.234,50\nBianchi;20,00\n"),
            ';'
        );
        assert_eq!(
            guess_separator("name,amount\nSmith,\"1,234.50\"\nJones,20.00\n"),
            ','
        );
        assert_eq!(guess_separator("a\tb\tc\n1\t2\t3\n"), '\t');
        assert_eq!(guess_separator("only one column\nhere\n"), ',');
    }

    #[test]
    fn numbers_in_local_forms() {
        for s in [
            "1.234,50",
            "1,234.50",
            "-20",
            "€ 12,00",
            "45%",
            "(1 200)",
            "1'000",
        ] {
            assert!(number_like(s), "{s}");
        }
        for s in ["Rossi", "2026-10-21", "", "n/a", "12 abc"] {
            assert!(!number_like(s), "{s}");
        }
    }
}
