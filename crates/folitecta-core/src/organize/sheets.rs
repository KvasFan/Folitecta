//! Tables of the pages as spreadsheets (FOL-L009, FOL-L030): the tables
//! found in the text of the pages ([`crate::page_tables`]) written as a
//! workbook (XLSX: a sheet for each table, numbers and percentages as
//! numbers, the first row in bold when it is all words) or as CSV (the
//! tables one after the other, an empty line between them). Whether a comma
//! or a point marks the decimals is told by the numbers of each table, or
//! by the language of the document when they do not say.

use crate::document::Document;
use crate::error::{Error, Result};
use crate::office::clean;
use crate::page_tables::FoundTable;
use crate::xml::{escape_attr, escape_text};
use crate::zip::{ZipEntry, write};

/// The formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TableFormat {
    Xlsx,
    Csv,
}

impl TableFormat {
    pub const ALL: [TableFormat; 2] = [TableFormat::Xlsx, TableFormat::Csv];

    pub fn id(self) -> &'static str {
        match self {
            TableFormat::Xlsx => "xlsx",
            TableFormat::Csv => "csv",
        }
    }

    pub fn from_id(id: &str) -> Option<TableFormat> {
        TableFormat::ALL.into_iter().find(|f| f.id() == id)
    }

    pub fn extension(self) -> &'static str {
        self.id()
    }
}

/// An export of tables and what it holds.
#[derive(Debug, Clone, PartialEq)]
pub struct TablesExport {
    pub bytes: Vec<u8>,
    pub tables: usize,
    pub rows: usize,
    /// The cells written as numbers (XLSX).
    pub numbers: usize,
}

/// The value of a cell.
#[derive(Debug, Clone, PartialEq)]
pub enum CellValue {
    Text(String),
    Number(f64),
    /// A share (0.25 for 25%), with whether it was written with decimals.
    Percent(f64, bool),
}

/// The decimals of `text` (a number with its thousands) are marked by a
/// comma: `Some(true)`; by a point: `Some(false)`; it does not tell: None.
fn decimal_mark(text: &str) -> Option<bool> {
    let t = text.trim().trim_end_matches('%').trim();
    let (comma, point) = (t.rfind(','), t.rfind('.'));
    match (comma, point) {
        (Some(c), Some(p)) => Some(c > p),
        (Some(c), None) => (t.matches(',').count() == 1 && t[c + 1..].chars().count() != 3).then_some(true),
        (None, Some(p)) => (t.matches('.').count() == 1 && t[p + 1..].chars().count() != 3).then_some(false),
        (None, None) => None,
    }
}

/// Languages that mark decimals with a comma.
fn comma_language(lang: &str) -> bool {
    let code = lang.split(['-', '_']).next().unwrap_or_default().to_lowercase();
    !matches!(code.as_str(), "" | "en" | "ja" | "zh" | "ko" | "he" | "th" | "hi")
}

/// The digits of `t` with `thousands` between groups of three and
/// `decimal` before the decimals, as a number.
fn number(t: &str, thousands: char, decimal: char) -> Option<f64> {
    let (int, frac) = match t.split_once(decimal) {
        Some((i, f)) => (i, Some(f)),
        None => (t, None),
    };
    if let Some(f) = frac {
        if f.is_empty() || !f.chars().all(|c| c.is_ascii_digit()) {
            return None;
        }
    }
    let int: String = int
        .chars()
        .filter(|c| !matches!(c, '\u{a0}' | '\u{202f}' | ' '))
        .collect();
    let groups: Vec<&str> = int.split(thousands).collect();
    let digits = |g: &str| !g.is_empty() && g.chars().all(|c| c.is_ascii_digit());
    let fine = if groups.len() == 1 {
        (groups[0].is_empty() && frac.is_some()) || digits(groups[0])
    } else {
        digits(groups[0]) && groups[0].len() <= 3 && groups[1..].iter().all(|g| digits(g) && g.len() == 3)
    };
    // A code with a nought in front (a postcode, a number of an article).
    let code = frac.is_none() && int.len() > 1 && int.starts_with('0');
    if !fine || code {
        return None;
    }
    let plain = format!("{}.{}", groups.concat(), frac.unwrap_or("0"));
    plain.parse().ok()
}

/// The value of the text of a cell, decimals marked by a comma when `comma`.
pub fn cell_value(text: &str, comma: bool) -> CellValue {
    let t = text.trim();
    let as_text = || CellValue::Text(t.to_string());
    let (mut body, mut negative) = (t, false);
    if let Some(inner) = body.strip_prefix('(').and_then(|b| b.strip_suffix(')')) {
        (body, negative) = (inner.trim(), true);
    }
    if let Some(rest) = body.strip_prefix(['-', '−']) {
        (body, negative) = (rest.trim_start(), !negative);
    } else if let Some(rest) = body.strip_prefix('+') {
        body = rest.trim_start();
    }
    let (body, percent) = match body.strip_suffix('%') {
        Some(b) => (b.trim_end(), true),
        None => (body, false),
    };
    let (thousands, decimal) = if comma { ('.', ',') } else { (',', '.') };
    let Some(v) = number(body, thousands, decimal) else {
        return as_text();
    };
    let v = if negative { -v } else { v };
    if percent {
        CellValue::Percent(v / 100.0, body.contains(decimal))
    } else {
        CellValue::Number(v)
    }
}

/// Whether the decimals of `table` are marked by a comma: as most of its
/// numbers that tell say, or as the language does.
fn table_comma(table: &FoundTable, lang: &str) -> bool {
    let (mut comma, mut point) = (0, 0);
    for cell in table.rows.iter().flatten() {
        // Only numbers tell: "Rome, Italy" says nothing.
        match decimal_mark(cell) {
            Some(m) if matches!(cell_value(cell, m), CellValue::Text(_)) => {}
            Some(true) => comma += 1,
            Some(false) => point += 1,
            None => {}
        }
    }
    if comma == point {
        comma_language(lang)
    } else {
        comma > point
    }
}

/// The name of column `i` (A, B, ..., Z, AA, ...).
fn column_name(mut i: usize) -> String {
    let mut out = Vec::new();
    loop {
        out.push(b'A' + (i % 26) as u8);
        if i < 26 {
            break;
        }
        i = i / 26 - 1;
    }
    out.reverse();
    String::from_utf8(out).unwrap_or_default()
}

/// A name a sheet may have: at most 31 characters, none of `[]:*?/\`,
/// unlike those in `taken`.
fn sheet_name(wanted: &str, taken: &[String]) -> String {
    let base: String = clean(wanted)
        .chars()
        .map(|c| if "[]:*?/\\".contains(c) { ' ' } else { c })
        .collect::<String>()
        .trim()
        .trim_matches('\'')
        .chars()
        .take(31)
        .collect();
    let base = if base.is_empty() {
        "Table".to_string()
    } else {
        base
    };
    let mut name = base.clone();
    let mut k = 2;
    while taken.iter().any(|t| t.to_lowercase() == name.to_lowercase()) {
        let tail = format!(" ({k})");
        name = format!(
            "{}{tail}",
            base.chars().take(31 - tail.chars().count()).collect::<String>()
        );
        k += 1;
    }
    name
}

/// Styles of the cells: 1 bold, 2 a percentage, 3 a percentage with decimals.
const STYLES: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n\
<styleSheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\">\
<fonts count=\"2\"><font><sz val=\"11\"/><name val=\"Calibri\"/></font><font><b/><sz val=\"11\"/><name val=\"Calibri\"/></font></fonts>\
<fills count=\"2\"><fill><patternFill patternType=\"none\"/></fill><fill><patternFill patternType=\"gray125\"/></fill></fills>\
<borders count=\"1\"><border><left/><right/><top/><bottom/><diagonal/></border></borders>\
<cellStyleXfs count=\"1\"><xf numFmtId=\"0\" fontId=\"0\" fillId=\"0\" borderId=\"0\"/></cellStyleXfs>\
<cellXfs count=\"4\"><xf numFmtId=\"0\" fontId=\"0\" fillId=\"0\" borderId=\"0\" xfId=\"0\"/>\
<xf numFmtId=\"0\" fontId=\"1\" fillId=\"0\" borderId=\"0\" xfId=\"0\" applyFont=\"1\"/>\
<xf numFmtId=\"9\" fontId=\"0\" fillId=\"0\" borderId=\"0\" xfId=\"0\" applyNumberFormat=\"1\"/>\
<xf numFmtId=\"10\" fontId=\"0\" fillId=\"0\" borderId=\"0\" xfId=\"0\" applyNumberFormat=\"1\"/></cellXfs>\
<cellStyles count=\"1\"><cellStyle name=\"Normal\" xfId=\"0\" builtinId=\"0\"/></cellStyles>\
</styleSheet>";

/// The sheet of a table, and how many of its cells are numbers.
fn sheet(table: &FoundTable, comma: bool) -> (String, usize) {
    let values: Vec<Vec<CellValue>> = table
        .rows
        .iter()
        .map(|r| r.iter().map(|c| cell_value(c, comma)).collect())
        .collect();
    let bold_head = values.len() > 1
        && values[0].iter().all(|v| matches!(v, CellValue::Text(_)))
        && values[1..]
            .iter()
            .flatten()
            .any(|v| !matches!(v, CellValue::Text(_)));
    let columns = table.columns().max(1);
    let widths: Vec<usize> = (0..columns)
        .map(|c| {
            table
                .rows
                .iter()
                .filter_map(|r| r.get(c))
                .map(|s| s.chars().count())
                .max()
                .unwrap_or(0)
        })
        .collect();
    let cols: String = widths
        .iter()
        .enumerate()
        .map(|(i, w)| {
            format!(
                "<col min=\"{n}\" max=\"{n}\" width=\"{}\" customWidth=\"1\"/>",
                (*w as f32 * 1.1 + 2.0).clamp(6.0, 60.0),
                n = i + 1
            )
        })
        .collect();
    let mut numbers = 0;
    let mut data = String::new();
    for (r, row) in values.iter().enumerate() {
        data.push_str(&format!("<row r=\"{}\">", r + 1));
        for (c, v) in row.iter().enumerate() {
            let at = format!("{}{}", column_name(c), r + 1);
            match v {
                CellValue::Text(t) if t.is_empty() => {}
                CellValue::Text(t) => {
                    let style = if r == 0 && bold_head { " s=\"1\"" } else { "" };
                    data.push_str(&format!(
                        "<c r=\"{at}\" t=\"inlineStr\"{style}><is><t xml:space=\"preserve\">{}</t></is></c>",
                        escape_text(&clean(t))
                    ));
                }
                CellValue::Number(n) => {
                    numbers += 1;
                    data.push_str(&format!("<c r=\"{at}\"><v>{n}</v></c>"));
                }
                CellValue::Percent(n, decimals) => {
                    numbers += 1;
                    let style = if *decimals { 3 } else { 2 };
                    data.push_str(&format!("<c r=\"{at}\" s=\"{style}\"><v>{n}</v></c>"));
                }
            }
        }
        data.push_str("</row>");
    }
    let last = format!("{}{}", column_name(columns - 1), values.len().max(1));
    (
        format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n\
             <worksheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\">\
             <dimension ref=\"A1:{last}\"/><cols>{cols}</cols><sheetData>{data}</sheetData></worksheet>"
        ),
        numbers,
    )
}

/// A workbook of `tables`, the sheets named by `names`.
fn xlsx(tables: &[FoundTable], names: &[String], lang: &str) -> Result<(Vec<u8>, usize)> {
    let mut entries = Vec::new();
    let mut sheets = String::new();
    let mut rels = String::new();
    let mut types = String::new();
    let mut taken: Vec<String> = Vec::new();
    let mut numbers = 0;
    for (i, table) in tables.iter().enumerate() {
        let n = i + 1;
        let name = sheet_name(names.get(i).map(String::as_str).unwrap_or("Table"), &taken);
        sheets.push_str(&format!(
            "<sheet name=\"{}\" sheetId=\"{n}\" r:id=\"rId{n}\"/>",
            escape_attr(&name)
        ));
        taken.push(name);
        rels.push_str(&format!(
            "<Relationship Id=\"rId{n}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet\" Target=\"worksheets/sheet{n}.xml\"/>"
        ));
        types.push_str(&format!(
            "<Override PartName=\"/xl/worksheets/sheet{n}.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml\"/>"
        ));
        let (xml, k) = sheet(table, table_comma(table, lang));
        numbers += k;
        entries.push(ZipEntry::new(
            format!("xl/worksheets/sheet{n}.xml"),
            xml.into_bytes(),
        ));
    }
    let mut all = vec![
        ZipEntry::new(
            "[Content_Types].xml",
            format!(
                "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n\
                 <Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\">\
                 <Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/>\
                 <Default Extension=\"xml\" ContentType=\"application/xml\"/>\
                 <Override PartName=\"/xl/workbook.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml\"/>\
                 <Override PartName=\"/xl/styles.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml\"/>\
                 {types}</Types>"
            )
            .into_bytes(),
        ),
        ZipEntry::new(
            "_rels/.rels",
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n\
             <Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">\
             <Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"xl/workbook.xml\"/>\
             </Relationships>"
                .as_bytes()
                .to_vec(),
        ),
        ZipEntry::new(
            "xl/workbook.xml",
            format!(
                "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n\
                 <workbook xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" \
                 xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\">\
                 <sheets>{sheets}</sheets></workbook>"
            )
            .into_bytes(),
        ),
        ZipEntry::new(
            "xl/_rels/workbook.xml.rels",
            format!(
                "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n\
                 <Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">\
                 {rels}<Relationship Id=\"rIdStyles\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles\" Target=\"styles.xml\"/>\
                 </Relationships>"
            )
            .into_bytes(),
        ),
        ZipEntry::new("xl/styles.xml", STYLES.as_bytes().to_vec()),
    ];
    all.append(&mut entries);
    Ok((write(&all)?, numbers))
}

impl Document {
    /// `tables` (found in this document) as a spreadsheet in `format`, the
    /// sheets of a workbook named `names` (one for each table).
    pub fn export_tables(
        &self,
        tables: &[FoundTable],
        format: TableFormat,
        names: &[String],
    ) -> Result<TablesExport> {
        self.require(crate::security::Restricted::Copy)?;
        if tables.is_empty() {
            return Err(Error::Invalid("there are no tables to export".into()));
        }
        let (bytes, numbers) = match format {
            TableFormat::Xlsx => xlsx(tables, names, &self.language())?,
            TableFormat::Csv => (
                tables
                    .iter()
                    .map(|t| crate::util::csv_table(&t.rows))
                    .collect::<Vec<_>>()
                    .join("\n")
                    .into_bytes(),
                0,
            ),
        };
        Ok(TablesExport {
            bytes,
            tables: tables.len(),
            rows: tables.iter().map(|t| t.rows.len()).sum(),
            numbers,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_are_told_from_words() {
        assert_eq!(cell_value("1,234.5", false), CellValue::Number(1234.5));
        assert_eq!(cell_value("1.234,5", true), CellValue::Number(1234.5));
        assert_eq!(cell_value("-12", false), CellValue::Number(-12.0));
        assert_eq!(cell_value("(7.25)", false), CellValue::Number(-7.25));
        assert_eq!(cell_value("12 %", false), CellValue::Percent(0.12, false));
        assert_eq!(cell_value("2,5%", true), CellValue::Percent(0.025, true));
        assert_eq!(cell_value("1 234 567", true), CellValue::Number(1_234_567.0));
        for text in ["00123", "1.2.3", "12,34,5", "Apples", "3 kg", "", "-", "1,5"] {
            assert!(matches!(cell_value(text, false), CellValue::Text(_)), "{text}");
        }
        assert_eq!(decimal_mark("1,5"), Some(true));
        assert_eq!(decimal_mark("1,500"), None);
        assert_eq!(decimal_mark("1.500,25"), Some(true));
        assert!(comma_language("it-IT") && !comma_language("en-GB") && !comma_language(""));
    }

    #[test]
    fn columns_and_sheets_are_named() {
        assert_eq!(column_name(0), "A");
        assert_eq!(column_name(25), "Z");
        assert_eq!(column_name(26), "AA");
        assert_eq!(column_name(701), "ZZ");
        assert_eq!(column_name(702), "AAA");
        let taken = vec!["Page 1".to_string()];
        assert_eq!(sheet_name("Page 1", &taken), "Page 1 (2)");
        assert_eq!(sheet_name("a/b:c", &[]), "a b c");
        assert_eq!(sheet_name(&"x".repeat(40), &[]).chars().count(), 31);
    }
}
