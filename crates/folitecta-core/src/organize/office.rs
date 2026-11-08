//! Export to office formats (FOL-L009): the text of the pages as editable
//! documents — Word (DOCX), rich text (RTF) and web pages (HTML) — made of
//! the paragraphs and headings of the reading order, the tables found in
//! the text of the pages as tables (their text not repeated as paragraphs)
//! and the pictures of the pages where they stand among them, at the size
//! they have on the page, with a page break where each page began when
//! asked. What is not
//! exported (comments, the picture of a scanned page whose text is
//! exported, specks) is counted for the report; the original is never
//! written.

use std::ops::Range;

use base64::Engine;

use crate::document::Document;
use crate::error::{Error, Result};
use crate::pictures::Colors;
use crate::textlayer::list_mark;
use crate::xml::escape_text;
use crate::zip::{ZipEntry, write};

/// The formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OfficeFormat {
    Docx,
    Rtf,
    Html,
}

impl OfficeFormat {
    pub const ALL: [OfficeFormat; 3] = [OfficeFormat::Docx, OfficeFormat::Rtf, OfficeFormat::Html];

    pub fn id(self) -> &'static str {
        match self {
            OfficeFormat::Docx => "docx",
            OfficeFormat::Rtf => "rtf",
            OfficeFormat::Html => "html",
        }
    }

    pub fn from_id(id: &str) -> Option<OfficeFormat> {
        OfficeFormat::ALL.into_iter().find(|f| f.id() == id)
    }

    pub fn extension(self) -> &'static str {
        self.id()
    }
}

/// An export and what it holds.
#[derive(Debug, Clone, PartialEq)]
pub struct OfficeExport {
    pub bytes: Vec<u8>,
    pub paragraphs: usize,
    pub headings: usize,
    /// The tables exported as tables.
    pub tables: usize,
    /// The pictures exported.
    pub pictures: usize,
    /// What the export leaves out: `pictures`, `comments`; how many.
    pub left_out: Vec<(&'static str, usize)>,
}

/// A picture of a page as exported: its file data, the kind of it, its
/// size in pixels and on the page (points).
#[derive(Debug, Clone)]
struct Picture {
    data: Vec<u8>,
    jpeg: bool,
    pixels: (u32, u32),
    size: (f32, f32),
}

impl Picture {
    fn mime(&self) -> &'static str {
        if self.jpeg { "image/jpeg" } else { "image/png" }
    }

    fn extension(&self) -> &'static str {
        if self.jpeg { "jpeg" } else { "png" }
    }

    /// The size shown, no wider than `width` points (the width of the text).
    fn shown(&self, width: f32) -> (f32, f32) {
        let (w, h) = self.size;
        if w > width && w > 0.0 {
            (width, h * width / w)
        } else {
            (w, h)
        }
    }
}

/// One piece of the reading order: its page, heading level (0: a
/// paragraph), text; or a picture of the page.
enum Piece {
    Text { page: usize, level: u8, text: String },
    Picture { page: usize, picture: Picture },
    Table { page: usize, rows: Vec<Vec<String>> },
}

impl Piece {
    fn page(&self) -> usize {
        match self {
            Piece::Text { page, .. } | Piece::Picture { page, .. } | Piece::Table { page, .. } => *page,
        }
    }
}

/// A cell holds a number (either way of marking decimals).
fn numeric(cell: &str) -> bool {
    use crate::sheets::{CellValue, cell_value};
    !matches!(cell_value(cell, false), CellValue::Text(_))
        || !matches!(cell_value(cell, true), CellValue::Text(_))
}

/// The first row of a table heads it: all words over numbers.
fn has_head(rows: &[Vec<String>]) -> bool {
    rows.len() > 1 && rows[0].iter().all(|c| !numeric(c)) && rows[1..].iter().flatten().any(|c| numeric(c))
}

/// A table of a Word document, as wide as the text, its columns even.
fn docx_table(rows: &[Vec<String>]) -> String {
    let columns = rows.first().map(Vec::len).unwrap_or(1).max(1);
    // Twips: 20 to the point.
    let total = (TEXT_WIDTH * 20.0) as usize;
    let width = total / columns;
    let head = has_head(rows);
    let grid = format!("<w:gridCol w:w=\"{width}\"/>").repeat(columns);
    let border =
        |side: &str| format!("<w:{side} w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"808080\"/>");
    let mut out = format!(
        "<w:tbl><w:tblPr><w:tblW w:w=\"{total}\" w:type=\"dxa\"/><w:tblBorders>{}{}{}{}{}{}</w:tblBorders>\
         <w:tblCellMar><w:left w:w=\"80\" w:type=\"dxa\"/><w:right w:w=\"80\" w:type=\"dxa\"/></w:tblCellMar>\
         <w:tblLook w:val=\"0020\" w:firstRow=\"1\"/></w:tblPr><w:tblGrid>{}</w:tblGrid>",
        border("top"),
        border("left"),
        border("bottom"),
        border("right"),
        border("insideH"),
        border("insideV"),
        grid
    );
    for (r, row) in rows.iter().enumerate() {
        let bold = head && r == 0;
        out.push_str("<w:tr>");
        if bold {
            out.push_str("<w:trPr><w:tblHeader/></w:trPr>");
        }
        for cell in row {
            let jc = if numeric(cell) {
                "<w:jc w:val=\"right\"/>"
            } else {
                ""
            };
            let b = if bold { "<w:rPr><w:b/></w:rPr>" } else { "" };
            out.push_str(&format!(
                "<w:tc><w:tcPr><w:tcW w:w=\"{width}\" w:type=\"dxa\"/></w:tcPr>\
                 <w:p><w:pPr><w:spacing w:after=\"0\"/>{jc}</w:pPr><w:r>{b}<w:t xml:space=\"preserve\">{}</w:t></w:r></w:p></w:tc>",
                escape_text(&clean(cell))
            ));
        }
        out.push_str("</w:tr>");
    }
    // A paragraph after it: Word wants one between a table and what follows.
    out.push_str("</w:tbl><w:p/>");
    out
}

/// Points of the text of a page of A4 with the margins of the exports
/// (DOCX: 2 cm; HTML: the column of the page).
const TEXT_WIDTH: f32 = 481.0;

/// Characters XML does not allow.
pub(crate) fn clean(text: &str) -> String {
    text.chars()
        .filter(|c| matches!(c, '\t' | '\n' | '\r') || (*c >= ' ' && !matches!(*c, '\u{fffe}' | '\u{ffff}')))
        .collect()
}

/// A picture of a Word document: inline in its own paragraph, `n` its
/// number among the pictures.
fn docx_picture(n: usize, picture: &Picture) -> String {
    let (w, h) = picture.shown(TEXT_WIDTH);
    // EMU: 12 700 to the point.
    let (cx, cy) = ((w * 12_700.0).round() as u64, (h * 12_700.0).round() as u64);
    format!(
        "<w:p><w:r><w:drawing><wp:inline distT=\"0\" distB=\"0\" distL=\"0\" distR=\"0\">\
         <wp:extent cx=\"{cx}\" cy=\"{cy}\"/><wp:docPr id=\"{n}\" name=\"Picture {n}\"/>\
         <a:graphic xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\">\
         <a:graphicData uri=\"http://schemas.openxmlformats.org/drawingml/2006/picture\">\
         <pic:pic xmlns:pic=\"http://schemas.openxmlformats.org/drawingml/2006/picture\">\
         <pic:nvPicPr><pic:cNvPr id=\"{n}\" name=\"image{n}.{ext}\"/><pic:cNvPicPr/></pic:nvPicPr>\
         <pic:blipFill><a:blip r:embed=\"rIdImg{n}\"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill>\
         <pic:spPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"{cx}\" cy=\"{cy}\"/></a:xfrm>\
         <a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></pic:spPr></pic:pic></a:graphicData></a:graphic>\
         </wp:inline></w:drawing></w:r></w:p>",
        ext = picture.extension()
    )
}

fn docx(pieces: &[Piece], title: &str, breaks: bool) -> Result<Vec<u8>> {
    let mut body = String::new();
    let mut media: Vec<(String, Vec<u8>)> = Vec::new();
    let mut page = pieces.first().map(Piece::page).unwrap_or(0);
    for p in pieces {
        if breaks && p.page() != page {
            body.push_str("<w:p><w:r><w:br w:type=\"page\"/></w:r></w:p>");
            page = p.page();
        }
        match p {
            Piece::Text { level: 0, text, .. } if list_mark(text).is_some() => {
                // An item of a list: its mark, a tab, its text, hanging.
                let (_, mark, rest) = list_mark(text).unwrap_or_default();
                body.push_str(&format!(
                    "<w:p><w:pPr><w:ind w:left=\"360\" w:hanging=\"360\"/></w:pPr>\
                     <w:r><w:t xml:space=\"preserve\">{}</w:t></w:r><w:r><w:tab/></w:r>\
                     <w:r><w:t xml:space=\"preserve\">{}</w:t></w:r></w:p>",
                    escape_text(&clean(&mark)),
                    escape_text(&clean(&rest))
                ));
            }
            Piece::Text { level, text, .. } => {
                let style = match level {
                    0 => String::new(),
                    l => format!("<w:pPr><w:pStyle w:val=\"Heading{}\"/></w:pPr>", l.min(&3)),
                };
                body.push_str(&format!(
                    "<w:p>{style}<w:r><w:t xml:space=\"preserve\">{}</w:t></w:r></w:p>",
                    escape_text(&clean(text))
                ));
            }
            Piece::Table { rows, .. } => body.push_str(&docx_table(rows)),
            Piece::Picture { picture, .. } => {
                let n = media.len() + 1;
                body.push_str(&docx_picture(n, picture));
                media.push((format!("image{n}.{}", picture.extension()), picture.data.clone()));
            }
        }
    }
    let document = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n\
         <w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\" \
         xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" \
         xmlns:wp=\"http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing\">\
         <w:body>{body}<w:sectPr><w:pgSz w:w=\"11906\" w:h=\"16838\"/>\
         <w:pgMar w:top=\"1134\" w:right=\"1134\" w:bottom=\"1134\" w:left=\"1134\" w:header=\"709\" w:footer=\"709\" w:gutter=\"0\"/>\
         </w:sectPr></w:body></w:document>"
    );
    let heading = |n: u8, size: u8| {
        format!(
            "<w:style w:type=\"paragraph\" w:styleId=\"Heading{n}\"><w:name w:val=\"heading {n}\"/>\
             <w:basedOn w:val=\"Normal\"/><w:next w:val=\"Normal\"/><w:qFormat/>\
             <w:pPr><w:keepNext/><w:spacing w:before=\"240\" w:after=\"120\"/><w:outlineLvl w:val=\"{}\"/></w:pPr>\
             <w:rPr><w:b/><w:sz w:val=\"{size}\"/></w:rPr></w:style>",
            n - 1
        )
    };
    let styles = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n\
         <w:styles xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\">\
         <w:docDefaults><w:rPrDefault><w:rPr><w:sz w:val=\"22\"/></w:rPr></w:rPrDefault>\
         <w:pPrDefault><w:pPr><w:spacing w:after=\"120\"/></w:pPr></w:pPrDefault></w:docDefaults>\
         <w:style w:type=\"paragraph\" w:default=\"1\" w:styleId=\"Normal\"><w:name w:val=\"Normal\"/><w:qFormat/></w:style>\
         {}{}{}</w:styles>",
        heading(1, 36),
        heading(2, 30),
        heading(3, 26)
    );
    let core = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n\
         <cp:coreProperties xmlns:cp=\"http://schemas.openxmlformats.org/package/2006/metadata/core-properties\" \
         xmlns:dc=\"http://purl.org/dc/elements/1.1/\"><dc:title>{}</dc:title></cp:coreProperties>",
        escape_text(&clean(title))
    );
    let entries = vec![
        ZipEntry::new(
            "[Content_Types].xml",
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n\
             <Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\">\
             <Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/>\
             <Default Extension=\"xml\" ContentType=\"application/xml\"/>\
             <Default Extension=\"png\" ContentType=\"image/png\"/>\
             <Default Extension=\"jpeg\" ContentType=\"image/jpeg\"/>\
             <Override PartName=\"/word/document.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml\"/>\
             <Override PartName=\"/word/styles.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml\"/>\
             <Override PartName=\"/docProps/core.xml\" ContentType=\"application/vnd.openxmlformats-package.core-properties+xml\"/>\
             </Types>"
                .as_bytes()
                .to_vec(),
        ),
        ZipEntry::new(
            "_rels/.rels",
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n\
             <Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">\
             <Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"word/document.xml\"/>\
             <Relationship Id=\"rId2\" Type=\"http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties\" Target=\"docProps/core.xml\"/>\
             </Relationships>"
                .as_bytes()
                .to_vec(),
        ),
        ZipEntry::new(
            "word/_rels/document.xml.rels",
            format!(
                "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n\
                 <Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">\
                 <Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles\" Target=\"styles.xml\"/>\
                 {}</Relationships>",
                media
                    .iter()
                    .enumerate()
                    .map(|(i, (name, _))| format!(
                        "<Relationship Id=\"rIdImg{}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/image\" Target=\"media/{name}\"/>",
                        i + 1
                    ))
                    .collect::<String>()
            )
            .into_bytes(),
        ),
        ZipEntry::new("word/document.xml", document.into_bytes()),
        ZipEntry::new("word/styles.xml", styles.into_bytes()),
        ZipEntry::new("docProps/core.xml", core.into_bytes()),
    ];
    let mut entries = entries;
    for (name, data) in media {
        entries.push(ZipEntry::new(format!("word/media/{name}"), data));
    }
    write(&entries)
}

/// Text in RTF: braces and backslashes escaped, other characters beyond
/// ASCII as `\uN?`.
fn rtf_text(text: &str) -> String {
    let mut out = String::new();
    for c in clean(text).chars() {
        match c {
            '\\' | '{' | '}' => {
                out.push('\\');
                out.push(c);
            }
            '\n' | '\r' => out.push_str("\\line "),
            '\t' => out.push_str("\\tab "),
            c if (c as u32) < 128 => out.push(c),
            c => {
                let mut buf = [0u16; 2];
                for unit in c.encode_utf16(&mut buf) {
                    out.push_str(&format!("\\u{}?", *unit as i16));
                }
            }
        }
    }
    out
}

fn rtf(pieces: &[Piece], breaks: bool) -> Vec<u8> {
    let mut out = String::from(
        "{\\rtf1\\ansi\\ansicpg1252\\deff0\\uc1{\\fonttbl{\\f0\\fswiss Helvetica;}}\\f0\\fs22\n",
    );
    let mut page = pieces.first().map(Piece::page).unwrap_or(0);
    for p in pieces {
        if breaks && p.page() != page {
            out.push_str("\\page\n");
            page = p.page();
        }
        match p {
            Piece::Text { level: 0, text, .. } => match list_mark(text) {
                Some((_, mark, rest)) => out.push_str(&format!(
                    "\\pard\\sa60\\li360\\fi-360 {}\\tab {}\\par\n",
                    rtf_text(&mark),
                    rtf_text(&rest)
                )),
                None => out.push_str(&format!("\\pard\\sa120 {}\\par\n", rtf_text(text))),
            },
            Piece::Text { level, text, .. } => {
                let l = (*level).min(3);
                let size = [36, 30, 26][(l - 1) as usize];
                out.push_str(&format!(
                    "\\pard\\keepn\\sb240\\sa120\\outlinelevel{} \\b\\fs{size} {}\\b0\\fs22\\par\n",
                    l - 1,
                    rtf_text(text)
                ));
            }
            Piece::Table { rows, .. } => {
                let columns = rows.first().map(Vec::len).unwrap_or(1).max(1);
                let width = (TEXT_WIDTH * 20.0) as usize / columns;
                let head = has_head(rows);
                let border = "\\clbrdrt\\brdrs\\brdrw10\\clbrdrl\\brdrs\\brdrw10\\clbrdrb\\brdrs\\brdrw10\\clbrdrr\\brdrs\\brdrw10";
                for (r, row) in rows.iter().enumerate() {
                    out.push_str("\\trowd\\trgaph80");
                    if head && r == 0 {
                        out.push_str("\\trhdr");
                    }
                    for k in 0..columns {
                        out.push_str(&format!("{border}\\cellx{}", (k + 1) * width));
                    }
                    out.push('\n');
                    for cell in row {
                        let align = if numeric(cell) { "\\qr" } else { "\\ql" };
                        let (b, b0) = if head && r == 0 {
                            ("\\b ", "\\b0")
                        } else {
                            ("", "")
                        };
                        out.push_str(&format!("\\pard\\intbl{align} {b}{}{b0}\\cell\n", rtf_text(cell)));
                    }
                    out.push_str("\\row\n");
                }
                out.push_str("\\pard\\sa120\\par\n");
            }
            Piece::Picture { picture, .. } => {
                // Twips: 20 to the point.
                let (w, h) = picture.shown(TEXT_WIDTH);
                let kind = if picture.jpeg { "\\jpegblip" } else { "\\pngblip" };
                let hex: String = picture.data.iter().map(|b| format!("{b:02x}")).collect();
                out.push_str(&format!(
                    "\\pard\\sa120 {{\\pict{kind}\\picw{}\\pich{}\\picwgoal{}\\pichgoal{}\n{hex}}}\\par\n",
                    picture.pixels.0,
                    picture.pixels.1,
                    (w * 20.0).round() as u32,
                    (h * 20.0).round() as u32
                ));
            }
        }
    }
    out.push('}');
    out.into_bytes()
}

fn html(pieces: &[Piece], title: &str, lang: &str, breaks: bool) -> Vec<u8> {
    let mut body = String::new();
    let mut page = pieces.first().map(Piece::page).unwrap_or(0);
    // The list open: numbered or not.
    let mut list: Option<bool> = None;
    let close = |body: &mut String, list: &mut Option<bool>| {
        if let Some(numbered) = list.take() {
            body.push_str(if numbered { "</ol>\n" } else { "</ul>\n" });
        }
    };
    for p in pieces {
        let item = match p {
            Piece::Text { level: 0, text, .. } => list_mark(text),
            _ => None,
        };
        if item.as_ref().map(|i| i.0) != list {
            close(&mut body, &mut list);
        }
        if breaks && p.page() != page {
            close(&mut body, &mut list);
            body.push_str("<hr class=\"page\">\n");
            page = p.page();
        }
        if let Some((numbered, _, rest)) = item {
            if list.is_none() {
                body.push_str(if numbered { "<ol>\n" } else { "<ul>\n" });
                list = Some(numbered);
            }
            body.push_str(&format!("<li>{}</li>\n", escape_text(&clean(&rest))));
            continue;
        }
        match p {
            Piece::Text { level, text, .. } => {
                let text = escape_text(&clean(text));
                match level {
                    0 => body.push_str(&format!("<p>{text}</p>\n")),
                    l => body.push_str(&format!("<h{n}>{text}</h{n}>\n", n = l.min(&3))),
                }
            }
            Piece::Table { rows, .. } => {
                let head = has_head(rows);
                body.push_str("<table>\n");
                for (r, row) in rows.iter().enumerate() {
                    body.push_str("<tr>");
                    for cell in row {
                        let text = escape_text(&clean(cell));
                        if head && r == 0 {
                            body.push_str(&format!("<th>{text}</th>"));
                        } else if numeric(cell) {
                            body.push_str(&format!("<td class=\"n\">{text}</td>"));
                        } else {
                            body.push_str(&format!("<td>{text}</td>"));
                        }
                    }
                    body.push_str("</tr>\n");
                }
                body.push_str("</table>\n");
            }
            Piece::Picture { picture, .. } => {
                // In the page itself, so the file is whole on its own.
                let (w, h) = picture.size;
                body.push_str(&format!(
                    "<p class=\"picture\"><img src=\"data:{};base64,{}\" width=\"{}\" height=\"{}\" alt=\"\"></p>\n",
                    picture.mime(),
                    base64::engine::general_purpose::STANDARD.encode(&picture.data),
                    (w * 4.0 / 3.0).round(),
                    (h * 4.0 / 3.0).round()
                ));
            }
        }
    }
    close(&mut body, &mut list);
    let lang = if lang.is_empty() { "en" } else { lang };
    format!(
        "<!DOCTYPE html>\n<html lang=\"{}\">\n<head>\n<meta charset=\"utf-8\">\n<title>{}</title>\n\
         <style>body{{font-family:sans-serif;max-width:46em;margin:2em auto;line-height:1.5;padding:0 1em}}\
         hr.page{{border:none;break-before:page}}img{{max-width:100%;height:auto}}\
         table{{border-collapse:collapse;margin:1em 0}}th,td{{border:1px solid #999;padding:.2em .5em;text-align:left}}td.n{{text-align:right}}</style>\n</head>\n<body>\n{body}</body>\n</html>\n",
        escape_text(lang),
        escape_text(&clean(title))
    )
    .into_bytes()
}

impl Document {
    /// The text of `pages` as an editable document in `format`; with
    /// `page_breaks` each page begins on a page of its own.
    pub fn export_office(
        &self,
        format: OfficeFormat,
        pages: Range<usize>,
        page_breaks: bool,
    ) -> Result<OfficeExport> {
        self.require(crate::security::Restricted::Copy)?;
        let n = self.page_count();
        if pages.start >= pages.end || pages.end > n {
            return Err(Error::InvalidPage(pages.end.saturating_sub(1)));
        }
        let blocks: Vec<_> = self
            .reading_blocks(pages.clone())
            .into_iter()
            .filter(|b| !b.text.trim().is_empty())
            .collect();
        // Page by page, each picture and table before the first paragraph
        // below its top; the paragraphs inside a table are its cells.
        let mut pieces: Vec<Piece> = Vec::new();
        let mut skipped = 0;
        let layers = self.text_layers();
        for page in pages.clone() {
            let texts: Vec<_> = blocks.iter().filter(|b| b.page == page).collect();
            let (pictures, left) = self.office_pictures(page, !texts.is_empty());
            skipped += left;
            let tables = layers
                .get(page)
                .map(crate::page_tables::find_tables)
                .unwrap_or_default();
            let within = |r: &[f32; 4]| {
                let (x, y) = ((r[0] + r[2]) / 2.0, (r[1] + r[3]) / 2.0);
                tables
                    .iter()
                    .any(|(b, _)| x >= b[0] - 2.0 && x <= b[2] + 2.0 && y >= b[1] - 2.0 && y <= b[3] + 2.0)
            };
            let mut placed: Vec<(f32, Piece)> = pictures
                .into_iter()
                .map(|(top, picture)| (top, Piece::Picture { page, picture }))
                .chain(tables.iter().map(|(b, rows)| {
                    (
                        b[1],
                        Piece::Table {
                            page,
                            rows: rows.clone(),
                        },
                    )
                }))
                .collect();
            placed.sort_by(|a, b| a.0.total_cmp(&b.0));
            let mut placed = placed.into_iter().peekable();
            for b in texts {
                let rects = layers
                    .get(page)
                    .map(|l| l.range_rects(b.start, b.end))
                    .unwrap_or_default();
                let top = rects.first().map(|r| r[1]).unwrap_or(0.0);
                while let Some((_, piece)) = placed.next_if(|(t, _)| *t <= top) {
                    pieces.push(piece);
                }
                if !rects.is_empty() && rects.iter().filter(|r| within(r)).count() * 2 > rects.len() {
                    continue;
                }
                pieces.push(Piece::Text {
                    page,
                    level: b.level,
                    text: b.text.clone(),
                });
            }
            pieces.extend(placed.map(|(_, piece)| piece));
        }
        if pieces.is_empty() {
            return Err(Error::Invalid(
                "the pages have no text to export (pictures of text need text recognition first)".into(),
            ));
        }
        let info = self.info();
        let title = if info.title.trim().is_empty() {
            self.path()
                .and_then(|p| p.file_stem())
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default()
        } else {
            info.title.clone()
        };
        let bytes = match format {
            OfficeFormat::Docx => docx(&pieces, &title, page_breaks)?,
            OfficeFormat::Rtf => rtf(&pieces, page_breaks),
            OfficeFormat::Html => html(&pieces, &title, &self.language(), page_breaks),
        };
        let comments: usize = pages
            .map(|p| {
                self.annotations(p)
                    .map(|a| {
                        a.iter()
                            .filter(|x| x.subtype != "Link" && x.subtype != "Popup")
                            .count()
                    })
                    .unwrap_or(0)
            })
            .sum();
        let left_out = [("pictures", skipped), ("comments", comments)]
            .into_iter()
            .filter(|(_, k)| *k > 0)
            .collect();
        Ok(OfficeExport {
            bytes,
            paragraphs: pieces
                .iter()
                .filter(|p| matches!(p, Piece::Text { level: 0, .. }))
                .count(),
            headings: pieces
                .iter()
                .filter(|p| matches!(p, Piece::Text { level, .. } if *level > 0))
                .count(),
            tables: pieces.iter().filter(|p| matches!(p, Piece::Table { .. })).count(),
            pictures: pieces
                .iter()
                .filter(|p| matches!(p, Piece::Picture { .. }))
                .count(),
            left_out,
        })
    }

    /// The pictures of `page` to export, with the top of each (display
    /// points), and how many are left out: specks (less than 8 points or 16
    /// pixels a side) and, when the page has text, the picture of a scanned
    /// page (its text is what is exported).
    fn office_pictures(&self, page: usize, has_text: bool) -> (Vec<(f32, Picture)>, usize) {
        let Ok(images) = self.page_images(page) else {
            return (Vec::new(), 0);
        };
        let scan = self.scan_picture(page);
        let Ok(size) = self.page_size(page) else {
            return (Vec::new(), 0);
        };
        let mut out = Vec::new();
        let mut left = 0;
        for im in images.iter() {
            let (w, h) = ((im.rect[2] - im.rect[0]).abs(), (im.rect[3] - im.rect[1]).abs());
            if w < 8.0 || h < 8.0 || im.width < 16 || im.height < 16 || (has_text && scan == Some(im.index)) {
                left += 1;
                continue;
            }
            let shown = crate::marks::pdf_rect_to_display(&size, im.rect);
            match self.office_picture(page, im, shown, (w, h)) {
                Some(p) => out.push((shown[1].min(shown[3]), p)),
                None => left += 1,
            }
        }
        (out, left)
    }

    /// One picture as a file: the JPEG data of the file when it is one
    /// drawn upright, else its pixels as PNG, or the page drawn over its
    /// area when it is turned or mirrored.
    fn office_picture(
        &self,
        page: usize,
        im: &crate::pictures::PageImage,
        shown: [f32; 4],
        size: (f32, f32),
    ) -> Option<Picture> {
        let m = im.matrix;
        let upright = m[0] > 0.0
            && m[3] > 0.0
            && m[1].abs() < 1e-6
            && m[2].abs() < 1e-6
            && size_of_rotation(self, page) == 0;
        if !upright {
            let png = self.render_region_png(page, shown, 2.0).ok()?;
            let pixels = image::load_from_memory(&png)
                .ok()
                .map(|i| (i.width(), i.height()))
                .unwrap_or((0, 0));
            return Some(Picture {
                data: png,
                jpeg: false,
                pixels,
                size,
            });
        }
        if im.filter == "DCTDecode" && matches!(im.colors, Colors::Color | Colors::Gray) {
            let stream = self
                .inner
                .get_object(im.object)
                .ok()
                .and_then(|o| o.as_stream().ok())
                .filter(|s| !s.dict.has(b"SMask") && !s.dict.has(b"Mask") && !s.dict.has(b"Decode"));
            if let Some(s) = stream {
                // The data as it is, when JPEG is its only filter.
                let only = match s.dict.get(b"Filter") {
                    Ok(lopdf::Object::Name(n)) => n == b"DCTDecode",
                    Ok(lopdf::Object::Array(a)) => a.len() == 1,
                    _ => false,
                };
                if only {
                    return Some(Picture {
                        data: s.content.clone(),
                        jpeg: true,
                        pixels: (im.width, im.height),
                        size,
                    });
                }
            }
        }
        let px = self.image_pixels(page, im.index).ok()?;
        let mut png = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(px.clone())
            .write_to(&mut png, image::ImageFormat::Png)
            .ok()?;
        Some(Picture {
            data: png.into_inner(),
            jpeg: false,
            pixels: px.dimensions(),
            size,
        })
    }
}

/// The rotation of page `page` (degrees clockwise).
fn size_of_rotation(doc: &Document, page: usize) -> i32 {
    doc.page_size(page).map(|s| s.rotation).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rtf_escapes_its_specials_and_unicode() {
        assert_eq!(rtf_text("a{b}\\c"), "a\\{b\\}\\\\c");
        assert_eq!(rtf_text("ё"), "\\u1105?");
        assert_eq!(rtf_text("😀"), "\\u-10179?\\u-8704?");
        assert_eq!(clean("a\u{1}b\u{fffe}"), "ab");
    }
}
