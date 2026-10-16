//! Small helpers shared by the document modules.

use lopdf::{Dictionary, Object, ObjectId};
use time::OffsetDateTime;
use time::macros::format_description;

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

/// Follow references until a direct object is reached.
pub fn deref<'a>(doc: &'a lopdf::Document, obj: &'a Object) -> &'a Object {
    doc.dereference(obj).map(|(_, o)| o).unwrap_or(obj)
}

/// Decode a PDF text string (possibly behind a reference) into a Rust string.
pub fn text_of(doc: &lopdf::Document, obj: &Object) -> Option<String> {
    let obj = deref(doc, obj);
    match obj {
        Object::String(..) => lopdf::decode_text_string(obj).ok(),
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
    Some([v[0].min(v[2]), v[1].min(v[3]), v[0].max(v[2]), v[1].max(v[3])])
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
