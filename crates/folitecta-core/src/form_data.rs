//! Form data exchange (XFDF, FDF, CSV, tab-separated text) and the widgets of a
//! page for filling forms on the page.

use lopdf::ObjectId;

use crate::document::Document;
use crate::error::{Error, Result};
use crate::forms::FieldKind;
use crate::util::{deref, rect_of};

/// Data formats for form values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormDataFormat {
    Xfdf,
    Fdf,
    Csv,
    Text,
}

impl FormDataFormat {
    pub fn extension(&self) -> &'static str {
        match self {
            FormDataFormat::Xfdf => "xfdf",
            FormDataFormat::Fdf => "fdf",
            FormDataFormat::Csv => "csv",
            FormDataFormat::Text => "txt",
        }
    }

    /// Guess the format of a file from its extension.
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext.to_ascii_lowercase().as_str() {
            "xfdf" | "xml" => Some(FormDataFormat::Xfdf),
            "fdf" => Some(FormDataFormat::Fdf),
            "csv" => Some(FormDataFormat::Csv),
            "txt" | "tsv" => Some(FormDataFormat::Text),
            _ => None,
        }
    }
}

/// One widget of a page with what is needed to fill it on the page.
#[derive(Debug, Clone, PartialEq)]
pub struct PageWidget {
    /// Position in the page's `/Annots`.
    pub index: usize,
    pub widget: ObjectId,
    pub field: ObjectId,
    pub name: String,
    pub kind: FieldKind,
    /// PDF rectangle.
    pub rect: [f32; 4],
    pub value: String,
    pub options: Vec<String>,
    pub multiline: bool,
    pub read_only: bool,
    /// "On" state of a check box or radio button widget.
    pub on_state: String,
    pub max_len: u32,
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn xml_unescape(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&#13;", "\r")
        .replace("&#10;", "\n")
        .replace("&amp;", "&")
}

fn fdf_string(s: &str) -> String {
    let mut out = String::from("(");
    for ch in s.chars() {
        match ch {
            '(' | ')' | '\\' => {
                out.push('\\');
                out.push(ch);
            }
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            c => out.push(c),
        }
    }
    out.push(')');
    out
}

fn csv_cell(s: &str) -> String {
    if s.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// Split one CSV record (RFC 4180 quoting).
fn csv_split(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match (c, quoted) {
            ('"', true) if chars.peek() == Some(&'"') => {
                cur.push('"');
                chars.next();
            }
            ('"', _) => quoted = !quoted,
            (',', false) => out.push(std::mem::take(&mut cur)),
            (c, _) => cur.push(c),
        }
    }
    out.push(cur);
    out
}

/// Field values of an XFDF document (nested `<field>` names joined with dots).
pub fn parse_xfdf(xml: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut stack: Vec<String> = Vec::new();
    let mut rest = xml;
    while let Some(start) = rest.find('<') {
        let after = &rest[start + 1..];
        let Some(end) = after.find('>') else { break };
        let tag = &after[..end];
        rest = &after[end + 1..];
        if let Some(name_part) = tag.strip_prefix("field") {
            if name_part.starts_with(char::is_whitespace) || name_part.starts_with('/') {
                let name = name_part
                    .split("name=")
                    .nth(1)
                    .and_then(|v| {
                        let q = v.chars().next()?;
                        let v = &v[1..];
                        v.find(q).map(|e| xml_unescape(&v[..e]))
                    })
                    .unwrap_or_default();
                if tag.trim_end().ends_with('/') {
                    continue;
                }
                stack.push(name);
            }
        } else if tag == "/field" {
            stack.pop();
        } else if tag == "value" || tag.starts_with("value ") {
            if let Some(close) = rest.find("</value>") {
                let value = xml_unescape(&rest[..close]);
                let name = stack
                    .iter()
                    .filter(|s| !s.is_empty())
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(".");
                if !name.is_empty() {
                    match out.iter_mut().find(|(n, _): &&mut (String, String)| *n == name) {
                        // Several <value> elements: a multi-selection.
                        Some(existing) => {
                            existing.1.push('\n');
                            existing.1.push_str(&value);
                        }
                        None => out.push((name, value)),
                    }
                }
                rest = &rest[close + "</value>".len()..];
            }
        }
    }
    out
}

/// Field values of an FDF file (`/T (name) /V (value)` or `/V /Name`).
pub fn parse_fdf(data: &[u8]) -> Vec<(String, String)> {
    let text = String::from_utf8_lossy(data);
    let mut out = Vec::new();
    let read_string = |s: &str| -> Option<(String, usize)> {
        let s = s.trim_start();
        let offset_ws = 0;
        if let Some(body) = s.strip_prefix('(') {
            let mut depth = 1;
            let mut value = String::new();
            let mut chars = body.char_indices();
            while let Some((i, c)) = chars.next() {
                match c {
                    '\\' => {
                        if let Some((_, n)) = chars.next() {
                            value.push(match n {
                                'n' => '\n',
                                'r' => '\r',
                                't' => '\t',
                                other => other,
                            });
                        }
                    }
                    '(' => {
                        depth += 1;
                        value.push(c);
                    }
                    ')' => {
                        depth -= 1;
                        if depth == 0 {
                            return Some((value, offset_ws + i + 2));
                        }
                        value.push(c);
                    }
                    other => value.push(other),
                }
            }
            None
        } else if let Some(body) = s.strip_prefix('/') {
            let end = body
                .find(|c: char| c.is_whitespace() || "/<>[]()".contains(c))
                .unwrap_or(body.len());
            Some((body[..end].to_string(), end + 1))
        } else {
            None
        }
    };
    let mut pos = 0;
    while let Some(t) = text[pos..].find("/T") {
        let at = pos + t + 2;
        if text[at..].starts_with(|c: char| c.is_ascii_alphabetic()) {
            pos = at;
            continue;
        }
        let Some((name, used)) = read_string(&text[at..]) else {
            pos = at;
            continue;
        };
        let after_name = at + (text[at..].len() - text[at..].trim_start().len()) + used;
        // The value follows within the same field dictionary.
        let window_end = text[after_name..]
            .find("/T")
            .map(|i| after_name + i)
            .unwrap_or(text.len());
        let window = &text[after_name..window_end];
        if let Some(v) = window.find("/V") {
            if let Some((value, _)) = read_string(&window[v + 2..]) {
                out.push((name, value));
            }
        }
        pos = after_name;
    }
    out
}

impl Document {
    /// Widgets on a page (form fields), in `/Annots` order.
    pub fn page_widgets(&self, page: usize) -> Vec<PageWidget> {
        let fields = self.form_fields();
        let Ok(annots) = self.annotations(page) else {
            return Vec::new();
        };
        let doc = &self.inner;
        let mut out = Vec::new();
        for a in annots.into_iter().filter(|a| a.subtype == "Widget") {
            let Some(wid) = a.id else { continue };
            let field = fields.iter().find(|f| self.field_widgets(f.id).contains(&wid));
            let Some(field) = field else { continue };
            let wd = doc.get_dictionary(wid).ok();
            let ff = {
                let mut cur = Some(wid);
                let mut ff = None;
                let mut guard = 0;
                while let (Some(id), None) = (cur, ff) {
                    let d = doc.get_dictionary(id).ok();
                    ff = d.and_then(|d| d.get(b"Ff").ok()).and_then(|o| o.as_i64().ok());
                    cur = d
                        .and_then(|d| d.get(b"Parent").ok())
                        .and_then(|p| p.as_reference().ok());
                    guard += 1;
                    if guard > 32 {
                        break;
                    }
                }
                ff.unwrap_or(0)
            };
            let max_len = doc
                .get_dictionary(field.id)
                .ok()
                .and_then(|d| d.get(b"MaxLen").ok())
                .and_then(|o| o.as_i64().ok())
                .unwrap_or(0)
                .max(0) as u32;
            out.push(PageWidget {
                index: a.index,
                widget: wid,
                field: field.id,
                name: field.name.clone(),
                kind: field.kind.clone(),
                rect: wd
                    .and_then(|d| d.get(b"Rect").ok())
                    .and_then(|r| rect_of(doc, deref(doc, r)))
                    .unwrap_or(a.rect),
                value: field.value.clone(),
                options: field.options.clone(),
                multiline: ff & (1 << 12) != 0,
                read_only: field.read_only,
                on_state: self.widget_on_state(wid),
                max_len,
            });
        }
        out
    }

    /// Export the form values in a data format.
    pub fn export_form_data(&self, format: FormDataFormat) -> Vec<u8> {
        let fields: Vec<crate::forms::FormField> = self
            .form_fields()
            .into_iter()
            .filter(|f| f.kind != FieldKind::PushButton && f.kind != FieldKind::Signature)
            .collect();
        let values: Vec<(String, String)> =
            fields.iter().map(|f| (f.name.clone(), f.value.clone())).collect();
        let file = self
            .path()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        match format {
            FormDataFormat::Xfdf => {
                let mut s = String::from(
                    "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<xfdf xmlns=\"http://ns.adobe.com/xfdf/\" xml:space=\"preserve\">\n  <fields>\n",
                );
                for f in &fields {
                    s.push_str(&format!("    <field name=\"{}\">\n", xml_escape(&f.name)));
                    // Multiple selections of list boxes become several values.
                    let parts: Vec<&str> = if f.kind == FieldKind::ListBox {
                        f.value.split(", ").collect()
                    } else {
                        vec![f.value.as_str()]
                    };
                    for v in parts {
                        s.push_str(&format!("      <value>{}</value>\n", xml_escape(v)));
                    }
                    s.push_str("    </field>\n");
                }
                s.push_str("  </fields>\n");
                if !file.is_empty() {
                    s.push_str(&format!("  <f href=\"{}\"/>\n", xml_escape(&file)));
                }
                s.push_str("</xfdf>\n");
                s.into_bytes()
            }
            FormDataFormat::Fdf => {
                let mut s = String::from("%FDF-1.2\n1 0 obj\n<< /FDF << /Fields [\n");
                for (name, value) in &values {
                    s.push_str(&format!(
                        "<< /T {} /V {} >>\n",
                        fdf_string(name),
                        fdf_string(value)
                    ));
                }
                s.push(']');
                if !file.is_empty() {
                    s.push_str(&format!(" /F {}", fdf_string(&file)));
                }
                s.push_str(" >> >>\nendobj\ntrailer\n<< /Root 1 0 R >>\n%%EOF\n");
                s.into_bytes()
            }
            FormDataFormat::Csv => {
                let header: Vec<String> = values.iter().map(|(n, _)| csv_cell(n)).collect();
                let row: Vec<String> = values.iter().map(|(_, v)| csv_cell(v)).collect();
                format!("{}\r\n{}\r\n", header.join(","), row.join(",")).into_bytes()
            }
            FormDataFormat::Text => {
                let header: Vec<String> = values.iter().map(|(n, _)| n.replace(['\t', '\n'], " ")).collect();
                let row: Vec<String> = values.iter().map(|(_, v)| v.replace(['\t', '\n'], " ")).collect();
                format!("{}\n{}\n", header.join("\t"), row.join("\t")).into_bytes()
            }
        }
    }

    /// Import form values from data in a format (one undo step). Returns the
    /// number of fields filled.
    pub fn import_form_data(&mut self, format: FormDataFormat, data: &[u8]) -> Result<usize> {
        let values: Vec<(String, String)> = match format {
            FormDataFormat::Xfdf => parse_xfdf(&String::from_utf8_lossy(data)),
            FormDataFormat::Fdf => parse_fdf(data),
            FormDataFormat::Csv | FormDataFormat::Text => {
                let text = String::from_utf8_lossy(data);
                let text = text.trim_start_matches('\u{feff}');
                let mut lines = text.lines().filter(|l| !l.trim().is_empty());
                let (Some(h), Some(r)) = (lines.next(), lines.next()) else {
                    return Err(Error::Invalid(
                        "the file needs a row of field names and a row of values".into(),
                    ));
                };
                let (header, row) = if format == FormDataFormat::Csv {
                    (csv_split(h), csv_split(r))
                } else {
                    (
                        h.split('\t').map(str::to_string).collect(),
                        r.split('\t').map(str::to_string).collect(),
                    )
                };
                header.into_iter().zip(row).collect()
            }
        };
        if values.is_empty() {
            return Err(Error::Invalid("no field values found in the file".into()));
        }
        self.set_field_values(&values)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xfdf_parsing_handles_nesting_and_escapes() {
        let xml = r#"<?xml version="1.0"?><xfdf><fields><field name="a"><field name="b"><value>x &amp; y</value></field></field><field name='c'><value>1</value><value>2</value></field><field name="empty"/></fields></xfdf>"#;
        assert_eq!(
            parse_xfdf(xml),
            vec![
                ("a.b".to_string(), "x & y".to_string()),
                ("c".to_string(), "1\n2".to_string())
            ]
        );
    }

    #[test]
    fn fdf_parsing_reads_strings_and_names() {
        let fdf = b"%FDF-1.2\n1 0 obj << /FDF << /Fields [ << /T (Name) /V (Jane \\(J\\)) >> << /V /Yes /T (Agree) >> << /T (Nr)/V (3) >> ] >> >> endobj";
        let got = parse_fdf(fdf);
        assert!(
            got.contains(&("Name".to_string(), "Jane (J)".to_string())),
            "{got:?}"
        );
        assert!(got.contains(&("Nr".to_string(), "3".to_string())), "{got:?}");
    }

    #[test]
    fn csv_cells_round_trip() {
        let line = format!(
            "{},{},{}",
            csv_cell("plain"),
            csv_cell("a,b"),
            csv_cell("say \"hi\"")
        );
        assert_eq!(csv_split(&line), vec!["plain", "a,b", "say \"hi\""]);
    }
}
