//! Small helpers shared by the document modules.

use lopdf::{Dictionary, Object, ObjectId};
use time::OffsetDateTime;
use time::macros::format_description;

/// Write `data` to `path` without ever leaving a half-written file there: the
/// bytes go to a temporary file in the same folder, are flushed to the disk,
/// and the temporary file then replaces the target in one rename. When
/// anything fails the original file is untouched and the temporary file is
/// removed. A symbolic link is followed, so the file it points to is
/// replaced, not the link.
pub fn write_atomic(path: &std::path::Path, data: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNT: AtomicU64 = AtomicU64::new(0);
    let target = match std::fs::symlink_metadata(path) {
        Ok(m) if m.file_type().is_symlink() => std::fs::canonicalize(path)?,
        _ => path.to_path_buf(),
    };
    let dir = match target.parent() {
        Some(d) if !d.as_os_str().is_empty() => d.to_path_buf(),
        _ => std::path::PathBuf::from("."),
    };
    let name = target
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "file".into());
    let n = COUNT.fetch_add(1, Ordering::SeqCst);
    let tmp = dir.join(format!(".{name}.{}-{n}.tmp", std::process::id()));
    let written = (|| {
        let mut file = std::fs::File::create(&tmp)?;
        file.write_all(data)?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&tmp, &target)
    })();
    if written.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    written
}

/// Whether `data` looks like a whole PDF file: the header at the start and
/// the end-of-file marker near the end (checked before replacing a file).
pub fn looks_like_whole_pdf(data: &[u8]) -> bool {
    let tail = &data[data.len().saturating_sub(1024)..];
    data.starts_with(b"%PDF-") && tail.windows(5).any(|w| w == b"%%EOF")
}

/// Current UTC time as a PDF date string (`D:YYYYMMDDHHmmSSZ`).
pub fn pdf_date_now() -> String {
    let now = OffsetDateTime::now_utc();
    let fmt = format_description!("[year][month][day][hour][minute][second]");
    match now.format(&fmt) {
        Ok(s) => format!("D:{s}Z"),
        Err(_) => "D:19700101000000Z".to_string(),
    }
}

/// Current date for display (`YYYY-MM-DD`).
pub fn today_display() -> String {
    let now = OffsetDateTime::now_utc();
    let fmt = format_description!("[year]-[month]-[day]");
    now.format(&fmt).unwrap_or_default()
}

/// Seconds since 1970 as `YYYY-MM-DD HH:MM` (UTC); empty for 0.
pub fn unix_display(secs: u64) -> String {
    if secs == 0 {
        return String::new();
    }
    let fmt = format_description!("[year]-[month]-[day] [hour]:[minute]");
    OffsetDateTime::from_unix_timestamp(secs as i64)
        .ok()
        .and_then(|d| d.format(&fmt).ok())
        .unwrap_or_default()
}

/// Convert a PDF date string into a readable `YYYY-MM-DD HH:MM` form.
/// Returns the input unchanged when it does not look like a PDF date.
pub fn pdf_date_display(raw: &str) -> String {
    let s = raw.trim().trim_start_matches("D:");
    let digits: String = s.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.len() < 8 {
        return raw.to_string();
    }
    let y = &digits[0..4];
    let m = &digits[4..6];
    let d = &digits[6..8];
    let hh = digits.get(8..10).unwrap_or("00");
    let mm = digits.get(10..12).unwrap_or("00");
    format!("{y}-{m}-{d} {hh}:{mm}")
}

/// Unix time (UTC) of a PDF date `D:YYYY[MM[DD[HH[mm[SS]]]]][Z|+HH'mm'|-HH'mm']`
/// (ISO 32000-1 7.9.4): missing fields take their first value, the prefix
/// and the apostrophes are optional, a date without a time zone is read as
/// UTC. `None` when the text is not a date.
pub fn pdf_date_unix(raw: &str) -> Option<i64> {
    let s = raw.trim();
    let s = s.strip_prefix("D:").unwrap_or(s);
    let len = s.bytes().take_while(u8::is_ascii_digit).count();
    if !(4..=14).contains(&len) || len % 2 != 0 {
        return None;
    }
    let field = |from: usize, default: u32| -> Option<u32> {
        match s.get(from..from + 2) {
            Some(v) if from + 2 <= len => v.parse().ok(),
            _ => Some(default),
        }
    };
    let year: i32 = s[..4].parse().ok()?;
    let month = time::Month::try_from(field(4, 1)? as u8).ok()?;
    let date = time::Date::from_calendar_date(year, month, field(6, 1)? as u8).ok()?;
    let clock = time::Time::from_hms(field(8, 0)? as u8, field(10, 0)? as u8, field(12, 0)? as u8).ok()?;
    let local = time::PrimitiveDateTime::new(date, clock)
        .assume_utc()
        .unix_timestamp();
    let zone = &s[len..];
    let offset = match zone.chars().next() {
        None | Some('Z') => 0,
        Some(sign @ ('+' | '-')) => {
            let digits: String = zone[1..].chars().filter(char::is_ascii_digit).take(4).collect();
            let hours: i64 = digits.get(..2)?.parse().ok()?;
            let minutes: i64 = digits.get(2..4).map_or(Some(0), |m| m.parse().ok())?;
            let secs = hours * 3600 + minutes * 60;
            if sign == '+' { secs } else { -secs }
        }
        Some(_) => return None,
    };
    Some(local - offset)
}

/// Follow references until a direct object is reached.
pub fn deref<'a>(doc: &'a lopdf::Document, obj: &'a Object) -> &'a Object {
    doc.dereference(obj).map(|(_, o)| o).unwrap_or(obj)
}

/// Characters of PDFDocEncoding codes 0x18..=0x1F.
const PDFDOC_18: [char; 8] = ['˘', 'ˇ', 'ˆ', '˙', '˝', '˛', '˚', '˜'];
/// Characters of PDFDocEncoding codes 0x80..=0xA0 (`\0` = undefined).
const PDFDOC_80: [char; 33] = [
    '•', '†', '‡', '…', '—', '–', 'ƒ', '⁄', '‹', '›', '−', '‰', '„', '“', '”', '‘', '’', '‚', '™', 'ﬁ', 'ﬂ',
    'Ł', 'Œ', 'Š', 'Ÿ', 'Ž', 'ı', 'ł', 'œ', 'š', 'ž', '\0', '€',
];

/// Decode the bytes of a PDF text string: UTF-16BE or UTF-8 with a byte order
/// mark, otherwise PDFDocEncoding (tabs and line breaks included).
pub fn decode_pdf_text(bytes: &[u8]) -> String {
    if let Some(rest) = bytes.strip_prefix(&[0xFE, 0xFF]) {
        let units: Vec<u16> = rest
            .chunks(2)
            .map(|c| u16::from_be_bytes([c[0], *c.get(1).unwrap_or(&0)]))
            .collect();
        let s: String = char::decode_utf16(units)
            .map(|r| r.unwrap_or('\u{FFFD}'))
            .collect();
        // Drop language escape sequences (ESC ... ESC).
        let mut out = String::with_capacity(s.len());
        let mut in_escape = false;
        for c in s.chars() {
            if c == '\u{1B}' {
                in_escape = !in_escape;
            } else if !in_escape {
                out.push(c);
            }
        }
        return out;
    }
    if let Some(rest) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        return String::from_utf8_lossy(rest).to_string();
    }
    bytes
        .iter()
        .filter_map(|&b| match b {
            0x09 | 0x0A | 0x0D | 0x20..=0x7E => Some(b as char),
            0x18..=0x1F => Some(PDFDOC_18[(b - 0x18) as usize]),
            0x80..=0xA0 => Some(PDFDOC_80[(b - 0x80) as usize]).filter(|c| *c != '\0'),
            0xA1..=0xFF if b != 0xAD => Some(b as char),
            _ => None,
        })
        .collect()
}

/// Decode a PDF text string (possibly behind a reference) into a Rust string.
pub fn text_of(doc: &lopdf::Document, obj: &Object) -> Option<String> {
    let obj = deref(doc, obj);
    match obj {
        Object::String(bytes, _) => Some(decode_pdf_text(bytes)),
        Object::Name(n) => Some(String::from_utf8_lossy(n).to_string()),
        Object::Integer(i) => Some(i.to_string()),
        Object::Real(r) => Some(fmt_num(*r)),
        Object::Boolean(b) => Some(b.to_string()),
        _ => None,
    }
}

/// Read a dictionary entry as text.
pub fn dict_text(doc: &lopdf::Document, dict: &Dictionary, key: &[u8]) -> Option<String> {
    dict.get(key).ok().and_then(|o| text_of(doc, o))
}

/// Read a number (integer or real).
pub fn num(obj: &Object) -> Option<f32> {
    match obj {
        Object::Integer(i) => Some(*i as f32),
        Object::Real(r) => Some(*r),
        _ => None,
    }
}

/// Read a `[x0 y0 x1 y1]` rectangle, normalised so that x0 <= x1 and y0 <= y1.
pub fn rect_of(doc: &lopdf::Document, obj: &Object) -> Option<[f32; 4]> {
    let arr = deref(doc, obj).as_array().ok()?;
    if arr.len() < 4 {
        return None;
    }
    let v: Vec<f32> = arr
        .iter()
        .take(4)
        .map(|o| num(deref(doc, o)))
        .collect::<Option<Vec<_>>>()?;
    Some(crate::geom::normalize([v[0], v[1], v[2], v[3]]))
}

/// Convert a colour array (gray, RGB or CMYK) into RGB components in `0..=1`.
pub fn color_of(doc: &lopdf::Document, obj: &Object) -> Option<[f32; 3]> {
    let arr = deref(doc, obj).as_array().ok()?;
    let v: Vec<f32> = arr.iter().filter_map(|o| num(deref(doc, o))).collect();
    match v.len() {
        1 => Some([v[0], v[0], v[0]]),
        3 => Some([v[0], v[1], v[2]]),
        4 => {
            let k = v[3];
            Some([
                (1.0 - v[0]) * (1.0 - k),
                (1.0 - v[1]) * (1.0 - k),
                (1.0 - v[2]) * (1.0 - k),
            ])
        }
        _ => None,
    }
}

/// Walk a PDF name tree and collect `(key, value)` pairs in order.
pub fn walk_name_tree(
    doc: &lopdf::Document,
    node: &Dictionary,
    out: &mut Vec<(Vec<u8>, Object)>,
    depth: usize,
) {
    if depth > 64 {
        return;
    }
    if let Ok(kids) = node
        .get(b"Kids")
        .map(|k| deref(doc, k))
        .and_then(|o| o.as_array())
    {
        for kid in kids {
            if let Ok(kd) = deref(doc, kid).as_dict() {
                walk_name_tree(doc, kd, out, depth + 1);
            }
        }
    }
    if let Ok(names) = node
        .get(b"Names")
        .map(|n| deref(doc, n))
        .and_then(|o| o.as_array())
    {
        for pair in names.chunks(2) {
            if pair.len() == 2 {
                if let Ok(k) = deref(doc, &pair[0]).as_str() {
                    out.push((k.to_vec(), pair[1].clone()));
                }
            }
        }
    }
}

/// Format a number for a content stream (no trailing zeros, no exponent).
pub fn fmt_num(v: f32) -> String {
    if (v - v.round()).abs() < 1e-4 {
        format!("{}", v.round() as i64)
    } else {
        let s = format!("{v:.3}");
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

/// WinAnsiEncoding code of a character (`?` when the encoding lacks it).
pub fn winansi_byte(ch: char) -> u8 {
    let code = ch as u32;
    if code < 0x80 || (0xA0..=0xFF).contains(&code) {
        return code as u8;
    }
    match ch {
        '€' => 0x80,
        '‚' => 0x82,
        'ƒ' => 0x83,
        '„' => 0x84,
        '…' => 0x85,
        '†' => 0x86,
        '‡' => 0x87,
        'ˆ' => 0x88,
        '‰' => 0x89,
        'Š' => 0x8A,
        '‹' => 0x8B,
        'Œ' => 0x8C,
        'Ž' => 0x8E,
        '‘' => 0x91,
        '’' => 0x92,
        '“' => 0x93,
        '”' => 0x94,
        '•' => 0x95,
        '–' => 0x96,
        '—' => 0x97,
        '˜' => 0x98,
        '™' => 0x99,
        'š' => 0x9A,
        '›' => 0x9B,
        'œ' => 0x9C,
        'ž' => 0x9E,
        'Ÿ' => 0x9F,
        '\u{2212}' => b'-',
        '\u{00A0}' => 0xA0,
        _ => b'?',
    }
}

/// Helvetica advance widths (1/1000 em) for WinAnsi codes 32..=126.
const HELV_ASCII: [u16; 95] = [
    278, 278, 355, 556, 556, 889, 667, 191, 333, 333, 389, 584, 278, 333, 278, 278, // space .. /
    556, 556, 556, 556, 556, 556, 556, 556, 556, 556, // 0-9
    278, 278, 584, 584, 584, 556, 1015, // : ; < = > ? @
    667, 667, 722, 722, 667, 611, 778, 722, 278, 500, 667, 556, 833, 722, 778, 667, 778, 722, 667, 611, 722,
    667, 944, 667, 667, 611, // A-Z
    278, 278, 278, 469, 556, 333, // [ \ ] ^ _ `
    556, 556, 500, 556, 556, 278, 556, 556, 222, 222, 500, 222, 833, 556, 556, 556, 556, 333, 500, 278, 556,
    500, 722, 500, 500, 500, // a-z
    334, 260, 334, 584, // { | } ~
];

/// Helvetica advance widths for WinAnsi codes 0x80..=0xFF.
const HELV_HIGH: [u16; 128] = [
    556, 278, 222, 556, 333, 1000, 556, 556, 333, 1000, 667, 333, 1000, 278, 611, 278, // 0x80
    278, 222, 222, 333, 333, 350, 556, 1000, 333, 1000, 500, 333, 944, 278, 500, 667, // 0x90
    278, 333, 556, 556, 556, 556, 260, 556, 333, 737, 370, 556, 584, 333, 737, 333, // 0xA0
    400, 584, 333, 333, 333, 556, 537, 278, 333, 333, 365, 556, 834, 834, 834, 611, // 0xB0
    667, 667, 667, 667, 667, 667, 1000, 722, 667, 667, 667, 667, 278, 278, 278, 278, // 0xC0
    722, 722, 778, 778, 778, 778, 778, 584, 778, 722, 722, 722, 722, 667, 667, 611, // 0xD0
    556, 556, 556, 556, 556, 556, 889, 500, 556, 556, 556, 556, 278, 278, 278, 278, // 0xE0
    556, 556, 556, 556, 556, 556, 556, 584, 611, 556, 556, 556, 556, 500, 556, 500, // 0xF0
];

/// Advance width of one WinAnsi code in Helvetica (1/1000 em).
pub fn helvetica_advance(code: u8) -> u16 {
    match code {
        32..=126 => HELV_ASCII[(code - 32) as usize],
        0x80..=0xFF => HELV_HIGH[(code - 0x80) as usize],
        _ => 0,
    }
}

/// Width of `text` set in Helvetica at `font_size` points.
pub fn helvetica_width(text: &str, font_size: f32) -> f32 {
    let units: u32 = text
        .chars()
        .map(|c| helvetica_advance(winansi_byte(c)) as u32)
        .sum();
    units as f32 * font_size / 1000.0
}

/// Escape a string for use inside a PDF literal string `( ... )` in
/// WinAnsiEncoding (characters the encoding lacks become `?`).
pub fn pdf_literal(text: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len() + 2);
    out.push(b'(');
    for ch in text.chars() {
        let byte = winansi_byte(ch);
        match byte {
            b'(' | b')' | b'\\' => {
                out.push(b'\\');
                out.push(byte);
            }
            b'\n' => out.extend_from_slice(b"\\n"),
            b'\r' => out.extend_from_slice(b"\\r"),
            b'\t' => out.extend_from_slice(b"\\t"),
            0x20..=0x7e => out.push(byte),
            _ => out.extend_from_slice(format!("\\{byte:03o}").as_bytes()),
        }
    }
    out.push(b')');
    out
}

/// Text width for the built-in Helvetica font used by generated content.
pub fn approx_text_width(text: &str, font_size: f32) -> f32 {
    helvetica_width(text, font_size)
}

/// Build the `Reference` object for an id.
pub fn r(id: ObjectId) -> Object {
    Object::Reference(id)
}

#[cfg(test)]
mod tests {

    #[test]
    fn pdf_dates_become_unix_times() {
        const SIGN: i64 = 1_792_324_800; // 2026-10-18T12:00Z
        assert_eq!(pdf_date_unix("D:20261018120000Z"), Some(SIGN));
        assert_eq!(pdf_date_unix("D:20261018140000+02'00'"), Some(SIGN));
        assert_eq!(pdf_date_unix("D:20261018093000-02'30"), Some(SIGN));
        assert_eq!(pdf_date_unix("D:2026"), Some(1_767_225_600));
        assert_eq!(pdf_date_unix("20261018"), pdf_date_unix("D:20261018"));
        assert_eq!(pdf_date_unix("garbage"), None);
        assert_eq!(pdf_date_unix("D:20261318"), None);
    }

    use super::*;

    #[test]
    fn text_strings_keep_line_breaks_and_special_characters() {
        assert_eq!(decode_pdf_text(b"a\rb\nc\td"), "a\rb\nc\td");
        assert_eq!(decode_pdf_text(&[0x80, 0x92, 0xA0, 0xE9]), "•™€é");
        assert_eq!(decode_pdf_text(&[0xFE, 0xFF, 0x00, 0x41, 0x20, 0xAC]), "A€");
        assert_eq!(decode_pdf_text(&[0xEF, 0xBB, 0xBF, 0xC3, 0xA8]), "è");
        let s = lopdf::text_string("one\ntwo è");
        let mut doc = lopdf::Document::new();
        let id = doc.add_object(s);
        assert_eq!(text_of(&doc, &Object::Reference(id)).unwrap(), "one\ntwo è");
    }
}
