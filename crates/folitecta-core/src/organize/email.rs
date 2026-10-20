//! E-mail messages (`.eml`, MIME): read and made into PDF documents (the
//! subject, the sender, the recipients and the date above the text of the
//! message, the attached files embedded in the document), and unsent drafts
//! written with files attached, for any mail program to open.

use base64::Engine;
use lopdf::{Object, dictionary};
use mail_parser::{Address, MessageParser, MimeHeaders};

use crate::document::Document;
use crate::error::{Error, Result};
use crate::fonts::TextWriter;
use crate::textdoc::TextLayout;
use crate::util::fmt_num;

/// A message read from a file.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct EmailMessage {
    pub subject: String,
    pub from: String,
    pub to: String,
    pub cc: String,
    /// When it was written, as the sender's program wrote it (`2026-10-18 09:30 +02:00`).
    pub date: String,
    /// The text of the message; an HTML message is made into text.
    pub body: String,
    /// The attached files: name and contents.
    pub attachments: Vec<(String, Vec<u8>)>,
}

/// The words in front of the header lines.
#[derive(Debug, Clone, PartialEq)]
pub struct EmailLabels {
    pub from: String,
    pub to: String,
    pub cc: String,
    pub date: String,
    pub attachments: String,
}

impl Default for EmailLabels {
    fn default() -> Self {
        EmailLabels {
            from: "From".into(),
            to: "To".into(),
            cc: "Cc".into(),
            date: "Date".into(),
            attachments: "Attachments".into(),
        }
    }
}

fn addresses(a: Option<&Address>) -> String {
    let Some(a) = a else { return String::new() };
    a.iter()
        .map(|x| match (x.name().map(str::trim), x.address().map(str::trim)) {
            (Some(n), Some(e)) if !n.is_empty() && n != e => format!("{n} <{e}>"),
            (_, Some(e)) => e.to_string(),
            (Some(n), None) => n.to_string(),
            (None, None) => String::new(),
        })
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(", ")
}

fn date_text(d: &mail_parser::DateTime) -> String {
    let sign = if d.tz_before_gmt { '-' } else { '+' };
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02} {sign}{:02}:{:02}",
        d.year, d.month, d.day, d.hour, d.minute, d.tz_hour, d.tz_minute
    )
}

/// The extension usual for a type of content.
fn extension_of(ctype: &str, subtype: Option<&str>) -> &'static str {
    match (ctype, subtype.unwrap_or("")) {
        ("message", "rfc822") => "eml",
        ("text", "html") => "html",
        ("text", "calendar") => "ics",
        ("text", _) => "txt",
        ("image", "jpeg" | "jpg") => "jpg",
        ("image", "png") => "png",
        ("image", "gif") => "gif",
        ("application", "pdf") => "pdf",
        ("application", "zip") => "zip",
        _ => "bin",
    }
}

/// Lines without trailing blanks, never more than two empty lines in a row.
fn clean_body(text: &str) -> String {
    let mut out: Vec<&str> = Vec::new();
    let mut empty = 0;
    for line in text.lines() {
        let line = line.trim_end();
        if line.is_empty() {
            empty += 1;
            if empty > 2 {
                continue;
            }
        } else {
            empty = 0;
        }
        out.push(line);
    }
    out.join("\n").trim_matches('\n').to_string()
}

/// Read an e-mail message (RFC 5322 with MIME parts, any character set).
pub fn read_email(data: &[u8]) -> Result<EmailMessage> {
    let not_mail = || Error::Invalid("the file is not an e-mail message".into());
    let msg = MessageParser::default().parse(data).ok_or_else(not_mail)?;
    if msg.subject().is_none() && msg.from().is_none() && msg.to().is_none() && msg.date().is_none() {
        return Err(not_mail());
    }
    let body = (0..msg.text_body_count())
        .filter_map(|i| msg.body_text(i))
        .map(|t| clean_body(&t))
        .filter(|t| !t.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");
    let attachments = msg
        .attachments()
        .enumerate()
        .map(|(i, part)| {
            let name = part
                .attachment_name()
                .map(|n| n.trim().to_string())
                .filter(|n| !n.is_empty())
                .unwrap_or_else(|| {
                    let ext = part
                        .content_type()
                        .map(|ct| extension_of(&ct.c_type, ct.c_subtype.as_deref()))
                        .unwrap_or("bin");
                    format!("attachment-{}.{ext}", i + 1)
                });
            (name, part.contents().to_vec())
        })
        .collect();
    Ok(EmailMessage {
        subject: msg.subject().unwrap_or("").trim().to_string(),
        from: addresses(msg.from()),
        to: addresses(msg.to()),
        cc: addresses(msg.cc()),
        date: msg.date().map(date_text).unwrap_or_default(),
        body,
        attachments,
    })
}

/// Control characters of a header value become spaces.
fn header_clean(s: &str) -> String {
    s.chars().map(|c| if c.is_control() { ' ' } else { c }).collect()
}

/// RFC 2047 encoded words for `s`, each at most 75 characters long and cut
/// between characters (45 bytes of UTF-8 make 60 characters of base64).
fn encoded_words(s: &str) -> Vec<String> {
    let enc = |t: &str| {
        format!(
            "=?UTF-8?B?{}?=",
            base64::engine::general_purpose::STANDARD.encode(t.as_bytes())
        )
    };
    let mut words = Vec::new();
    let mut chunk = String::new();
    for c in s.chars() {
        if chunk.len() + c.len_utf8() > 45 {
            words.push(enc(&chunk));
            chunk.clear();
        }
        chunk.push(c);
    }
    if !chunk.is_empty() || words.is_empty() {
        words.push(enc(&chunk));
    }
    words
}

/// A header value after `Name: ` (`used` characters): plain ASCII folded at
/// its spaces, anything else as RFC 2047 encoded words, one per line, so no
/// line grows past 78 characters (RFC 5322, 2.1.1) unless one word does.
fn header_text(s: &str, used: usize) -> String {
    let clean = header_clean(s);
    if clean.is_ascii() && clean.split(' ').all(|w| w.len() <= 900) {
        let mut out = String::new();
        let mut line = used;
        for (i, word) in clean.split(' ').enumerate() {
            if i > 0 {
                if line + 1 + word.len() > 78 && line > 1 {
                    out.push_str("\r\n ");
                    line = 1;
                } else {
                    out.push(' ');
                    line += 1;
                }
            }
            out.push_str(word);
            line += word.len();
        }
        out
    } else {
        encoded_words(&clean).join("\r\n ")
    }
}

/// A quoted-string (RFC 5322, 3.2.4).
fn quoted(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

/// The `name` parameter of the content type: plain when it is ASCII, encoded
/// words otherwise (read by older mail programs).
fn name_param(name: &str) -> String {
    let clean = header_clean(name);
    if clean.is_ascii() {
        format!("name={}", quoted(&clean))
    } else {
        format!("name=\"{}\"", encoded_words(&clean).join(" "))
    }
}

/// The `filename` parameter of the content disposition: plain when it is
/// short ASCII, otherwise the RFC 2231 form (`filename*=UTF-8''...`) cut in
/// numbered parts of at most 60 characters, never inside a character (some
/// readers decode each part on its own).
fn filename_param(name: &str) -> String {
    let clean = header_clean(name);
    if clean.is_ascii() && clean.len() <= 200 {
        return format!("filename={}", quoted(&clean));
    }
    let mut parts: Vec<String> = vec![String::new()];
    for c in clean.chars() {
        let mut buf = [0u8; 4];
        let token: String = c
            .encode_utf8(&mut buf)
            .bytes()
            .map(|b| {
                if b.is_ascii_alphanumeric() || b"!#$&+-.^_`|~".contains(&b) {
                    (b as char).to_string()
                } else {
                    format!("%{b:02X}")
                }
            })
            .collect();
        if parts.last().is_some_and(|p| p.len() + token.len() > 60) {
            parts.push(String::new());
        }
        if let Some(p) = parts.last_mut() {
            p.push_str(&token);
        }
    }
    if parts.len() == 1 {
        return format!("filename*=UTF-8''{}", parts[0]);
    }
    parts
        .iter()
        .enumerate()
        .map(|(i, p)| {
            if i == 0 {
                format!("filename*0*=UTF-8''{p}")
            } else {
                format!("filename*{i}*={p}")
            }
        })
        .collect::<Vec<_>>()
        .join(";\r\n ")
}

/// Base64 in lines of 76 characters.
fn base64_lines(data: &[u8]) -> String {
    let text = base64::engine::general_purpose::STANDARD.encode(data);
    let mut out = String::with_capacity(text.len() + text.len() / 76 * 2 + 2);
    for chunk in text.as_bytes().chunks(76) {
        out.push_str(std::str::from_utf8(chunk).unwrap_or_default());
        out.push_str("\r\n");
    }
    out
}

/// The content type of an attachment, from its extension.
fn content_type(name: &str) -> &'static str {
    match name.rsplit('.').next().map(|e| e.to_ascii_lowercase()).as_deref() {
        Some("pdf") => "application/pdf",
        Some("zip") => "application/zip",
        Some("xfdf") => "application/vnd.adobe.xfdf",
        Some("fdf") => "application/vnd.fdf",
        Some("txt") => "text/plain",
        Some("csv") => "text/csv",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        _ => "application/octet-stream",
    }
}

/// An unsent e-mail draft (`X-Unsent: 1`) with `files` attached, as an
/// `.eml` file: mail programs open it as a new message ready to be sent.
/// Nothing is sent by writing it.
pub fn write_eml_draft<D: AsRef<[u8]>>(subject: &str, body: &str, files: &[(String, D)]) -> Vec<u8> {
    let boundary = format!(
        "=_folitecta_{:016x}",
        files.iter().fold(subject.len() as u64, |h, (n, d)| h
            .wrapping_mul(31)
            .wrapping_add((n.len() + d.as_ref().len()) as u64))
    );
    let date = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc2822)
        .unwrap_or_default();
    let mut out = String::new();
    out.push_str("X-Unsent: 1\r\n");
    out.push_str(&format!("Subject: {}\r\n", header_text(subject, 9)));
    if !date.is_empty() {
        out.push_str(&format!("Date: {date}\r\n"));
    }
    out.push_str("MIME-Version: 1.0\r\n");
    out.push_str(&format!(
        "Content-Type: multipart/mixed; boundary=\"{boundary}\"\r\n\r\n"
    ));
    out.push_str("This is a message in MIME format.\r\n");
    out.push_str(&format!("--{boundary}\r\n"));
    out.push_str("Content-Type: text/plain; charset=utf-8\r\nContent-Transfer-Encoding: base64\r\n\r\n");
    out.push_str(&base64_lines(body.as_bytes()));
    for (name, data) in files {
        out.push_str(&format!("--{boundary}\r\n"));
        out.push_str(&format!(
            "Content-Type: {};\r\n {}\r\nContent-Disposition: attachment;\r\n {}\r\nContent-Transfer-Encoding: base64\r\n\r\n",
            content_type(name),
            name_param(name),
            filename_param(name)
        ));
        out.push_str(&base64_lines(data.as_ref()));
    }
    out.push_str(&format!("--{boundary}--\r\n"));
    out.into_bytes()
}

/// One line of the typeset message.
enum Line {
    /// Text at `x` (from the left margin) in a font, size and grey level.
    Text {
        text: String,
        x: f32,
        bold: bool,
        size: f32,
        grey: f32,
    },
    /// A rule across the page.
    Rule,
    /// Room before the next line.
    Gap(f32),
}

impl Line {
    fn height(&self) -> f32 {
        match self {
            Line::Text { size, .. } => size * 1.3,
            Line::Rule => 10.0,
            Line::Gap(h) => *h,
        }
    }
}

/// Helvetica Bold is about this much wider than Helvetica.
const BOLD: f32 = 1.08;

impl Document {
    /// A new document showing `msg`: the subject, the header lines, then the
    /// text, on pages of `layout`; the attached files are embedded.
    pub fn from_email(msg: &EmailMessage, labels: &EmailLabels, layout: TextLayout) -> Result<Document> {
        let (pw, ph) = (layout.page_width, layout.page_height);
        let margin = layout.margin.clamp(18.0, pw.min(ph) / 3.0);
        let width = pw - 2.0 * margin;
        let fs = layout.font_size.clamp(6.0, 24.0);
        let mut lines = Vec::new();
        let subject = if msg.subject.is_empty() {
            "—"
        } else {
            msg.subject.as_str()
        };
        let subject_size = (fs * 1.45).round();
        let names: Vec<&str> = msg.attachments.iter().map(|(n, _)| n.as_str()).collect();
        let attached = names.join(", ");
        let header: Vec<(&str, &str)> = [
            (labels.from.as_str(), msg.from.as_str()),
            (labels.to.as_str(), msg.to.as_str()),
            (labels.cc.as_str(), msg.cc.as_str()),
            (labels.date.as_str(), msg.date.as_str()),
            (labels.attachments.as_str(), attached.as_str()),
        ]
        .into_iter()
        .filter(|(_, v)| !v.trim().is_empty())
        .collect();
        // One font for all the text of the message: text that cannot be drawn
        // is refused before anything is made.
        let mut writer = TextWriter::for_pieces(
            [subject, msg.body.as_str()]
                .into_iter()
                .chain(header.iter().flat_map(|(k, v)| [*k, *v])),
        )?;
        for l in writer.wrap_piece(&writer.piece(subject), subject_size * BOLD, width) {
            lines.push(Line::Text {
                text: l,
                x: 0.0,
                bold: true,
                size: subject_size,
                grey: 0.0,
            });
        }
        lines.push(Line::Gap(fs * 0.5));
        let hs = (fs * 0.9).max(6.0);
        // The values start where the longest label ends.
        let column = header
            .iter()
            .map(|(k, _)| writer.width(&format!("{}:", writer.piece(k)), hs) * BOLD)
            .fold(0.0, f32::max)
            + hs;
        for (label, value) in &header {
            let value = writer.piece(value);
            for (i, l) in writer
                .wrap_piece(&value, hs, width - column)
                .into_iter()
                .enumerate()
            {
                if i == 0 {
                    lines.push(Line::Text {
                        text: format!("{}:", writer.piece(label)),
                        x: 0.0,
                        bold: true,
                        size: hs,
                        grey: 0.35,
                    });
                    // The value goes on the same line as its label.
                    lines.push(Line::Gap(-hs * 1.3));
                }
                lines.push(Line::Text {
                    text: l,
                    x: column,
                    bold: false,
                    size: hs,
                    grey: 0.1,
                });
            }
        }
        lines.push(Line::Rule);
        for l in writer.wrap_piece(&writer.piece(&msg.body), fs, width) {
            lines.push(Line::Text {
                text: l,
                x: 0.0,
                bold: false,
                size: fs,
                grey: 0.0,
            });
        }
        // Lines into pages.
        let mut pages: Vec<Vec<(f32, &Line)>> = vec![Vec::new()];
        let mut y = ph - margin;
        for line in &lines {
            let h = line.height();
            if h > 0.0 && y - h < margin && !pages.last().is_some_and(|p| p.is_empty()) {
                pages.push(Vec::new());
                y = ph - margin;
                if matches!(line, Line::Gap(_) | Line::Rule) {
                    continue;
                }
            }
            y -= h;
            if let Some(page) = pages.last_mut() {
                page.push((y, line));
            }
        }
        let mut doc = Document::new_blank(pages.len(), pw, ph)?;
        // The standard Helvetica for WinAnsi text, as before; else the
        // Unicode font of the writer, added once every page is written.
        let standard = writer.is_standard().then(|| {
            let regular = doc.helvetica_font_id();
            let bold = doc.inner.add_object(dictionary! {
                "Type" => "Font",
                "Subtype" => "Type1",
                "BaseFont" => "Helvetica-Bold",
                "Encoding" => "WinAnsiEncoding",
            });
            (regular, bold)
        });
        let f = fmt_num;
        let mut written = Vec::new();
        for (page, page_id) in pages.iter().zip(doc.page_ids()) {
            let mut content = Vec::new();
            for (y, line) in page {
                match line {
                    Line::Text {
                        text,
                        x,
                        bold,
                        size,
                        grey,
                    } if !text.is_empty() => {
                        let place = format!("1 0 0 1 {} {} Tm", f(margin + x), f(y + size * 0.3));
                        // The grey as written before for the standard font; the
                        // Unicode font strokes its bold letters in it too.
                        let (color, at) = if writer.is_standard() {
                            (None, format!("{} g {place}", f(*grey)))
                        } else {
                            (Some([*grey; 3]), place)
                        };
                        let shown =
                            writer.text_object(text, *size, color, &at, *bold, ("FoliHelv", "FoliHelvB"))?;
                        content.extend_from_slice(shown.as_bytes());
                        content.extend_from_slice(b"\n");
                    }
                    Line::Rule => content.extend_from_slice(
                        format!(
                            "0.75 G 0.6 w {} {} m {} {} l S\n",
                            f(margin),
                            f(y + 5.0),
                            f(pw - margin),
                            f(y + 5.0)
                        )
                        .as_bytes(),
                    ),
                    _ => {}
                }
            }
            if let Some((regular, bold)) = standard {
                doc.add_page_resource(page_id, "Font", "FoliHelv", Object::Reference(regular))?;
                doc.add_page_resource(page_id, "Font", "FoliHelvB", Object::Reference(bold))?;
            }
            doc.append_page_content(page_id, content, None)?;
            written.push(page_id);
        }
        if standard.is_none() {
            let name = writer.resource_name();
            let font = writer.finish(&mut doc, None)?;
            for page_id in written {
                doc.add_page_resource(page_id, "Font", &name, Object::Reference(font))?;
            }
        }
        for (name, data) in &msg.attachments {
            doc.insert_attachment(name.as_bytes().to_vec(), name, data.clone(), "", None)?;
        }
        doc.set_info(&msg.subject, &msg.from, "", "")?;
        doc.commit()?;
        doc.clear_history();
        Ok(doc)
    }
}
