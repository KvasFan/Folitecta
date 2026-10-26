//! Export to office formats (FOL-L009): the text of the pages as editable
//! documents — Word (DOCX), rich text (RTF) and web pages (HTML) — made of
//! the paragraphs and headings of the reading order, with a page break
//! where each page began when asked. What is not exported (pictures,
//! comments) is counted for the report; the original is never written.

use std::ops::Range;

use crate::document::Document;
use crate::error::{Error, Result};
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
    /// What the export leaves out: `pictures`, `comments`; how many.
    pub left_out: Vec<(&'static str, usize)>,
}

/// One piece of the reading order: its page, heading level (0: a
/// paragraph), text.
struct Piece {
    page: usize,
    level: u8,
    text: String,
}

/// Characters XML does not allow.
fn clean(text: &str) -> String {
    text.chars()
        .filter(|c| matches!(c, '\t' | '\n' | '\r') || (*c >= ' ' && !matches!(*c, '\u{fffe}' | '\u{ffff}')))
        .collect()
}

fn docx(pieces: &[Piece], title: &str, breaks: bool) -> Result<Vec<u8>> {
    let mut body = String::new();
    let mut page = pieces.first().map(|p| p.page).unwrap_or(0);
    for p in pieces {
        if breaks && p.page != page {
            body.push_str("<w:p><w:r><w:br w:type=\"page\"/></w:r></w:p>");
            page = p.page;
        }
        let style = match p.level {
            0 => String::new(),
            l => format!("<w:pPr><w:pStyle w:val=\"Heading{}\"/></w:pPr>", l.min(3)),
        };
        body.push_str(&format!(
            "<w:p>{style}<w:r><w:t xml:space=\"preserve\">{}</w:t></w:r></w:p>",
            escape_text(&clean(&p.text))
        ));
    }
    let document = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n\
         <w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\">\
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
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n\
             <Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">\
             <Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles\" Target=\"styles.xml\"/>\
             </Relationships>"
                .as_bytes()
                .to_vec(),
        ),
        ZipEntry::new("word/document.xml", document.into_bytes()),
        ZipEntry::new("word/styles.xml", styles.into_bytes()),
        ZipEntry::new("docProps/core.xml", core.into_bytes()),
    ];
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
    let mut page = pieces.first().map(|p| p.page).unwrap_or(0);
    for p in pieces {
        if breaks && p.page != page {
            out.push_str("\\page\n");
            page = p.page;
        }
        match p.level {
            0 => out.push_str(&format!("\\pard\\sa120 {}\\par\n", rtf_text(&p.text))),
            l => {
                let size = [36, 30, 26][(l.min(3) - 1) as usize];
                out.push_str(&format!(
                    "\\pard\\keepn\\sb240\\sa120\\outlinelevel{} \\b\\fs{size} {}\\b0\\fs22\\par\n",
                    l.min(3) - 1,
                    rtf_text(&p.text)
                ));
            }
        }
    }
    out.push('}');
    out.into_bytes()
}

fn html(pieces: &[Piece], title: &str, lang: &str, breaks: bool) -> Vec<u8> {
    let mut body = String::new();
    let mut page = pieces.first().map(|p| p.page).unwrap_or(0);
    for p in pieces {
        if breaks && p.page != page {
            body.push_str("<hr class=\"page\">\n");
            page = p.page;
        }
        let text = escape_text(&clean(&p.text));
        match p.level {
            0 => body.push_str(&format!("<p>{text}</p>\n")),
            l => body.push_str(&format!("<h{n}>{text}</h{n}>\n", n = l.min(3))),
        }
    }
    let lang = if lang.is_empty() { "en" } else { lang };
    format!(
        "<!DOCTYPE html>\n<html lang=\"{}\">\n<head>\n<meta charset=\"utf-8\">\n<title>{}</title>\n\
         <style>body{{font-family:sans-serif;max-width:46em;margin:2em auto;line-height:1.5;padding:0 1em}}\
         hr.page{{border:none;break-before:page}}</style>\n</head>\n<body>\n{body}</body>\n</html>\n",
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
        let n = self.page_count();
        if pages.start >= pages.end || pages.end > n {
            return Err(Error::InvalidPage(pages.end.saturating_sub(1)));
        }
        let pieces: Vec<Piece> = self
            .reading_blocks(pages.clone())
            .into_iter()
            .filter(|b| !b.text.trim().is_empty())
            .map(|b| Piece {
                page: b.page,
                level: b.level,
                text: b.text,
            })
            .collect();
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
        let pictures: usize = pages.clone().map(|p| self.image_count(p)).sum();
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
        let left_out = [("pictures", pictures), ("comments", comments)]
            .into_iter()
            .filter(|(_, k)| *k > 0)
            .collect();
        Ok(OfficeExport {
            bytes,
            paragraphs: pieces.iter().filter(|p| p.level == 0).count(),
            headings: pieces.iter().filter(|p| p.level > 0).count(),
            left_out,
        })
    }
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
