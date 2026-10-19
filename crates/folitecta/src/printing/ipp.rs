//! The Internet Printing Protocol (RFC 8010, 8011) as far as printing to the
//! local print server needs it: messages encoded and decoded, the attributes
//! of a job made from a ticket, printers and their capabilities read from
//! the answers. Pure functions, tested byte for byte.

use folitecta_core::print::{Sides, media_size, standard_name};

use super::{JobTicket, Media, PrintError, PrinterCaps, PrinterInfo, PrinterState};

/// Operations.
pub const PRINT_JOB: u16 = 0x0002;
pub const VALIDATE_JOB: u16 = 0x0004;
pub const GET_PRINTER_ATTRIBUTES: u16 = 0x000B;
pub const CUPS_GET_DEFAULT: u16 = 0x4001;
pub const CUPS_GET_PRINTERS: u16 = 0x4002;

/// Attribute groups.
pub const OPERATION: u8 = 0x01;
pub const JOB: u8 = 0x02;
pub const END: u8 = 0x03;
pub const PRINTER: u8 = 0x04;
pub const UNSUPPORTED: u8 = 0x05;

/// Value tags.
pub const INTEGER: u8 = 0x21;
pub const BOOLEAN: u8 = 0x22;
pub const ENUM: u8 = 0x23;
pub const RESOLUTION: u8 = 0x32;
pub const RANGE: u8 = 0x33;
pub const BEG_COLLECTION: u8 = 0x34;
pub const END_COLLECTION: u8 = 0x37;
#[cfg(test)]
pub const TEXT: u8 = 0x41;
pub const NAME: u8 = 0x42;
pub const KEYWORD: u8 = 0x44;
pub const URI: u8 = 0x45;
pub const CHARSET: u8 = 0x47;
pub const LANGUAGE: u8 = 0x48;
pub const MIME: u8 = 0x49;
pub const MEMBER_NAME: u8 = 0x4A;

/// One value of an attribute.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Int(i32),
    Bool(bool),
    Enum(i32),
    Str(String),
    Range(i32, i32),
    Res(i32, i32, u8),
    Collection(Vec<Attr>),
    Bytes(Vec<u8>),
    /// No value: unknown, unsupported, no-value (tags 0x10 to 0x1F).
    OutOfBand(u8),
}

/// An attribute: its value tag, name and values.
#[derive(Debug, Clone, PartialEq)]
pub struct Attr {
    pub tag: u8,
    pub name: String,
    pub values: Vec<Value>,
}

impl Attr {
    pub fn new(tag: u8, name: &str, values: Vec<Value>) -> Self {
        Attr {
            tag,
            name: name.to_string(),
            values,
        }
    }

    pub fn text(tag: u8, name: &str, value: &str) -> Self {
        Attr::new(tag, name, vec![Value::Str(value.to_string())])
    }

    pub fn int(tag: u8, name: &str, value: i32) -> Self {
        Attr::new(
            tag,
            name,
            vec![if tag == ENUM {
                Value::Enum(value)
            } else {
                Value::Int(value)
            }],
        )
    }

    /// The text values.
    pub fn strings(&self) -> Vec<&str> {
        self.values
            .iter()
            .filter_map(|v| match v {
                Value::Str(s) => Some(s.as_str()),
                _ => None,
            })
            .collect()
    }

    pub fn first_str(&self) -> Option<&str> {
        self.strings().into_iter().next()
    }

    /// The first integer or enumeration value.
    pub fn first_int(&self) -> Option<i32> {
        self.values.iter().find_map(|v| match v {
            Value::Int(i) | Value::Enum(i) => Some(*i),
            _ => None,
        })
    }

    pub fn first_bool(&self) -> Option<bool> {
        self.values.iter().find_map(|v| match v {
            Value::Bool(b) => Some(*b),
            _ => None,
        })
    }
}

/// A group of attributes.
#[derive(Debug, Clone, PartialEq)]
pub struct Group {
    pub tag: u8,
    pub attrs: Vec<Attr>,
}

/// A request or a response.
#[derive(Debug, Clone, PartialEq)]
pub struct Message {
    /// The operation of a request, the status of a response.
    pub code: u16,
    pub request_id: u32,
    pub groups: Vec<Group>,
}

/// The attribute `name` among `attrs`.
pub fn find<'a>(attrs: &'a [Attr], name: &str) -> Option<&'a Attr> {
    attrs.iter().find(|a| a.name == name)
}

impl Message {
    /// The attribute `name` in the first group with tag `group`.
    pub fn attr(&self, group: u8, name: &str) -> Option<&Attr> {
        self.groups
            .iter()
            .filter(|g| g.tag == group)
            .find_map(|g| find(&g.attrs, name))
    }

    /// The names of the attributes the server did not support.
    pub fn unsupported(&self) -> Vec<String> {
        self.groups
            .iter()
            .filter(|g| g.tag == UNSUPPORTED)
            .flat_map(|g| g.attrs.iter().map(|a| a.name.clone()))
            .collect()
    }

    pub fn status_message(&self) -> String {
        self.attr(OPERATION, "status-message")
            .and_then(Attr::first_str)
            .unwrap_or_default()
            .to_string()
    }
}

fn put_str(out: &mut Vec<u8>, s: &[u8]) {
    out.extend_from_slice(&(s.len().min(u16::MAX as usize) as u16).to_be_bytes());
    out.extend_from_slice(&s[..s.len().min(u16::MAX as usize)]);
}

fn put_value(out: &mut Vec<u8>, tag: u8, name: &str, v: &Value) {
    if let Value::Collection(members) = v {
        out.push(BEG_COLLECTION);
        put_str(out, name.as_bytes());
        put_str(out, b"");
        for m in members {
            out.push(MEMBER_NAME);
            put_str(out, b"");
            put_str(out, m.name.as_bytes());
            for mv in &m.values {
                put_value(out, m.tag, "", mv);
            }
        }
        out.push(END_COLLECTION);
        put_str(out, b"");
        put_str(out, b"");
        return;
    }
    let (tag, data): (u8, Vec<u8>) = match v {
        Value::Int(i) => (tag, i.to_be_bytes().to_vec()),
        Value::Enum(i) => (tag, i.to_be_bytes().to_vec()),
        Value::Bool(b) => (tag, vec![u8::from(*b)]),
        Value::Str(s) => (tag, s.as_bytes().to_vec()),
        Value::Range(a, b) => (tag, [a.to_be_bytes(), b.to_be_bytes()].concat()),
        Value::Res(x, y, u) => {
            let mut d = [x.to_be_bytes(), y.to_be_bytes()].concat();
            d.push(*u);
            (tag, d)
        }
        Value::Bytes(b) => (tag, b.clone()),
        Value::OutOfBand(t) => (*t, Vec::new()),
        Value::Collection(_) => unreachable!("collections are written above"),
    };
    out.push(tag);
    put_str(out, name.as_bytes());
    put_str(out, &data);
}

/// A request: version 2.0, the operation, the request id, the groups.
pub fn encode(op: u16, id: u32, groups: &[Group]) -> Vec<u8> {
    let mut out = vec![2, 0];
    out.extend_from_slice(&op.to_be_bytes());
    out.extend_from_slice(&id.to_be_bytes());
    for g in groups {
        out.push(g.tag);
        for a in &g.attrs {
            for (i, v) in a.values.iter().enumerate() {
                put_value(&mut out, a.tag, if i == 0 { &a.name } else { "" }, v);
            }
        }
    }
    out.push(END);
    out
}

struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn u8(&mut self) -> Result<u8, String> {
        let b = *self
            .data
            .get(self.pos)
            .ok_or("the answer of the print server ends too early")?;
        self.pos += 1;
        Ok(b)
    }

    fn bytes(&mut self, n: usize) -> Result<&'a [u8], String> {
        let end = self.pos.checked_add(n).ok_or("bad length")?;
        let s = self
            .data
            .get(self.pos..end)
            .ok_or("the answer of the print server ends too early")?;
        self.pos = end;
        Ok(s)
    }

    fn u16(&mut self) -> Result<u16, String> {
        let b = self.bytes(2)?;
        Ok(u16::from_be_bytes([b[0], b[1]]))
    }

    fn sized(&mut self) -> Result<&'a [u8], String> {
        let n = self.u16()? as usize;
        self.bytes(n)
    }
}

fn int_of(v: &[u8]) -> Result<i32, String> {
    let b: [u8; 4] = v.try_into().map_err(|_| "an integer of the wrong size")?;
    Ok(i32::from_be_bytes(b))
}

fn decode_value(tag: u8, v: &[u8]) -> Result<Value, String> {
    Ok(match tag {
        0x10..=0x1F => Value::OutOfBand(tag),
        INTEGER => Value::Int(int_of(v)?),
        ENUM => Value::Enum(int_of(v)?),
        BOOLEAN => Value::Bool(v.first().copied().unwrap_or(0) != 0),
        RESOLUTION if v.len() == 9 => Value::Res(int_of(&v[..4])?, int_of(&v[4..8])?, v[8]),
        RANGE if v.len() == 8 => Value::Range(int_of(&v[..4])?, int_of(&v[4..])?),
        // Text or name with a language: the language, then the text.
        0x35 | 0x36 => {
            let mut r = Reader { data: v, pos: 0 };
            let _lang = r.sized()?;
            Value::Str(String::from_utf8_lossy(r.sized()?).into_owned())
        }
        0x41..=0x49 => Value::Str(String::from_utf8_lossy(v).into_owned()),
        _ => Value::Bytes(v.to_vec()),
    })
}

fn read_collection(r: &mut Reader<'_>, depth: u32) -> Result<Vec<Attr>, String> {
    if depth > 8 {
        return Err("collections nested too deeply".into());
    }
    let mut members: Vec<Attr> = Vec::new();
    loop {
        let tag = r.u8()?;
        let _name = r.sized()?;
        let value = r.sized()?;
        match tag {
            END_COLLECTION => return Ok(members),
            MEMBER_NAME => members.push(Attr::new(0, &String::from_utf8_lossy(value), Vec::new())),
            BEG_COLLECTION => {
                let sub = read_collection(r, depth + 1)?;
                let m = members
                    .last_mut()
                    .ok_or("a collection value without a member name")?;
                if m.tag == 0 {
                    m.tag = BEG_COLLECTION;
                }
                m.values.push(Value::Collection(sub));
            }
            t => {
                let v = decode_value(t, value)?;
                let m = members.last_mut().ok_or("a member value without a name")?;
                if m.tag == 0 {
                    m.tag = t;
                }
                m.values.push(v);
            }
        }
    }
}

/// A response (or a request, for the tests' server).
pub fn decode(bytes: &[u8]) -> Result<Message, String> {
    let mut r = Reader { data: bytes, pos: 0 };
    let _version = r.bytes(2)?;
    let code = r.u16()?;
    let id = r.bytes(4)?;
    let request_id = u32::from_be_bytes([id[0], id[1], id[2], id[3]]);
    let mut groups: Vec<Group> = Vec::new();
    loop {
        let tag = r.u8()?;
        if tag == END {
            break;
        }
        if tag < 0x10 {
            // A new group (unknown groups are kept and ignored by the readers).
            groups.push(Group {
                tag,
                attrs: Vec::new(),
            });
            continue;
        }
        let name = String::from_utf8_lossy(r.sized()?).into_owned();
        let value = r.sized()?;
        let v = if tag == BEG_COLLECTION {
            Value::Collection(read_collection(&mut r, 1)?)
        } else {
            decode_value(tag, value)?
        };
        let group = groups.last_mut().ok_or("an attribute outside of a group")?;
        if name.is_empty() {
            let a = group
                .attrs
                .last_mut()
                .ok_or("an additional value without an attribute")?;
            a.values.push(v);
        } else {
            group.attrs.push(Attr {
                tag,
                name,
                values: vec![v],
            });
        }
    }
    Ok(Message {
        code,
        request_id,
        groups,
    })
}

/// The error of an unsuccessful status (`None` for the successful ones,
/// 0x0000 to 0x00FF).
pub fn status_error(status: u16, message: &str) -> Option<PrintError> {
    match status {
        0x0000..=0x00FF => None,
        0x0401..=0x0403 => Some(PrintError::Auth),
        0x0406 => Some(PrintError::NotFound(message.to_string())),
        0x040A => Some(PrintError::FormatNotSupported),
        0x0506 => Some(PrintError::NotAccepting),
        s => Some(PrintError::Server {
            status: s,
            message: message.to_string(),
        }),
    }
}

/// A name cut to at most 255 bytes at a character boundary.
pub fn name_value(s: &str) -> String {
    let mut end = s.len().min(255);
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    s[..end].to_string()
}

/// The address of a queue of the local server (its name percent-encoded).
pub fn printer_uri(name: &str) -> String {
    let mut out = String::from("ipp://localhost/printers/");
    for b in name.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// The operation attributes every request starts with.
pub fn operation_attrs(uri: Option<&str>, user: &str, lang: &str) -> Vec<Attr> {
    let mut attrs = vec![
        Attr::text(CHARSET, "attributes-charset", "utf-8"),
        Attr::text(LANGUAGE, "attributes-natural-language", lang),
    ];
    if let Some(uri) = uri {
        attrs.push(Attr::text(URI, "printer-uri", uri));
    }
    attrs.push(Attr::text(NAME, "requesting-user-name", &name_value(user)));
    attrs
}

/// The keywords asked for in a request.
pub fn requested(names: &[&str]) -> Attr {
    Attr::new(
        KEYWORD,
        "requested-attributes",
        names.iter().map(|n| Value::Str((*n).to_string())).collect(),
    )
}

pub const PRINTER_LIST_ATTRS: &[&str] = &[
    "printer-name",
    "printer-uri-supported",
    "printer-info",
    "printer-location",
    "printer-make-and-model",
    "printer-state",
    "printer-state-message",
    "printer-state-reasons",
    "printer-is-accepting-jobs",
    "queued-job-count",
];

pub const CAPS_ATTRS: &[&str] = &[
    "media-supported",
    "media-default",
    "media-col-default",
    "media-col-database",
    "media-bottom-margin-supported",
    "media-left-margin-supported",
    "media-right-margin-supported",
    "media-top-margin-supported",
    "sides-supported",
    "sides-default",
    "copies-supported",
    "multiple-document-handling-supported",
    "print-color-mode-supported",
    "print-color-mode-default",
    "printer-resolution-supported",
    "document-format-supported",
    "print-scaling-supported",
];

pub fn sides_keyword(s: Sides) -> &'static str {
    match s {
        Sides::OneSided => "one-sided",
        Sides::LongEdge => "two-sided-long-edge",
        Sides::ShortEdge => "two-sided-short-edge",
    }
}

fn sides_of(keyword: &str) -> Option<Sides> {
    match keyword {
        "one-sided" => Some(Sides::OneSided),
        "two-sided-long-edge" => Some(Sides::LongEdge),
        "two-sided-short-edge" => Some(Sides::ShortEdge),
        _ => None,
    }
}

/// The job attributes of a ticket for a printer with `caps`, leaving out
/// what the printer does not support and the names in `skip`.
pub fn job_attributes(t: &JobTicket, caps: &PrinterCaps, skip: &[String]) -> Vec<Attr> {
    let mut out = Vec::new();
    let copies = t.copies.clamp(1, caps.max_copies.max(1));
    if copies > 1 {
        out.push(Attr::int(INTEGER, "copies", copies as i32));
        if caps.collate {
            out.push(Attr::text(
                KEYWORD,
                "multiple-document-handling",
                if t.collate {
                    "separate-documents-collated-copies"
                } else {
                    "separate-documents-uncollated-copies"
                },
            ));
        }
    }
    if caps.sides.contains(&t.sides) {
        out.push(Attr::text(KEYWORD, "sides", sides_keyword(t.sides)));
    }
    if let Some(m) = &t.media {
        if !m.key.is_empty() {
            out.push(Attr::text(KEYWORD, "media", &m.key));
        }
    }
    let mode = if t.gray { "monochrome" } else { "color" };
    if caps.color_modes.iter().any(|m| m == mode) {
        out.push(Attr::text(KEYWORD, "print-color-mode", mode));
    }
    out.push(Attr::int(ENUM, "orientation-requested", 3));
    if caps.scaling_none {
        out.push(Attr::text(KEYWORD, "print-scaling", "none"));
    }
    out.retain(|a| !skip.contains(&a.name));
    out
}

/// The printers of a CUPS-Get-Printers answer; `default` is the name of the
/// default printer.
pub fn printers_from(m: &Message, default: Option<&str>) -> Vec<PrinterInfo> {
    m.groups
        .iter()
        .filter(|g| g.tag == PRINTER)
        .filter_map(|g| {
            let s = |n: &str| {
                find(&g.attrs, n)
                    .and_then(Attr::first_str)
                    .unwrap_or_default()
                    .to_string()
            };
            let name = s("printer-name");
            if name.is_empty() {
                return None;
            }
            let reasons: Vec<String> = find(&g.attrs, "printer-state-reasons")
                .map(|a| a.strings().into_iter().map(str::to_string).collect())
                .unwrap_or_default();
            let accepting = find(&g.attrs, "printer-is-accepting-jobs")
                .and_then(Attr::first_bool)
                .unwrap_or(true);
            let state = if reasons.iter().any(|r| r.starts_with("offline")) {
                PrinterState::Offline
            } else {
                match find(&g.attrs, "printer-state").and_then(Attr::first_int) {
                    Some(3) => PrinterState::Idle,
                    Some(4) => PrinterState::Printing,
                    Some(5) => PrinterState::Stopped,
                    _ => PrinterState::Unknown,
                }
            };
            let info = s("printer-info");
            Some(PrinterInfo {
                is_default: default == Some(name.as_str()),
                label: if info.is_empty() { name.clone() } else { info },
                location: s("printer-location"),
                model: s("printer-make-and-model"),
                message: s("printer-state-message"),
                jobs: find(&g.attrs, "queued-job-count")
                    .and_then(Attr::first_int)
                    .unwrap_or(0)
                    .max(0) as u32,
                state,
                accepting,
                name,
            })
        })
        .collect()
}

/// Hundredths of millimetres to points.
fn hmm(v: i32) -> f32 {
    v as f32 / 100.0 / 25.4 * 72.0
}

/// The size (hundredths of mm) and margins (points: left, bottom, right,
/// top) of a `media-col` collection.
fn media_col(members: &[Attr]) -> Option<((i32, i32), [f32; 4])> {
    let size = find(members, "media-size")?.values.iter().find_map(|v| match v {
        Value::Collection(c) => Some(c),
        _ => None,
    })?;
    let x = find(size, "x-dimension")?.first_int()?;
    let y = find(size, "y-dimension")?.first_int()?;
    let margin = |n: &str| find(members, n).and_then(Attr::first_int).map(hmm).unwrap_or(0.0);
    Some((
        (x, y),
        [
            margin("media-left-margin"),
            margin("media-bottom-margin"),
            margin("media-right-margin"),
            margin("media-top-margin"),
        ],
    ))
}

/// The capabilities of a printer from its Get-Printer-Attributes answer.
pub fn caps_from(m: &Message) -> PrinterCaps {
    let attrs: &[Attr] = m
        .groups
        .iter()
        .find(|g| g.tag == PRINTER)
        .map(|g| g.attrs.as_slice())
        .unwrap_or(&[]);
    let strings = |n: &str| -> Vec<String> {
        find(attrs, n)
            .map(|a| a.strings().into_iter().map(str::to_string).collect())
            .unwrap_or_default()
    };
    let collections = |n: &str| -> Vec<&Vec<Attr>> {
        find(attrs, n)
            .map(|a| {
                a.values
                    .iter()
                    .filter_map(|v| match v {
                        Value::Collection(c) => Some(c),
                        _ => None,
                    })
                    .collect()
            })
            .unwrap_or_default()
    };
    let default_col = collections("media-col-default")
        .first()
        .and_then(|c| media_col(c));
    let database: Vec<((i32, i32), [f32; 4])> = collections("media-col-database")
        .into_iter()
        .filter_map(|c| media_col(c))
        .collect();
    let widest = |n: &str| -> f32 {
        find(attrs, n)
            .map(|a| {
                a.values
                    .iter()
                    .filter_map(|v| match v {
                        Value::Int(i) => Some(*i),
                        _ => None,
                    })
                    .max()
                    .unwrap_or(0)
            })
            .map(hmm)
            .unwrap_or(0.0)
    };
    let fallback = [
        widest("media-left-margin-supported"),
        widest("media-bottom-margin-supported"),
        widest("media-right-margin-supported"),
        widest("media-top-margin-supported"),
    ];
    let mut media = Vec::new();
    for key in strings("media-supported") {
        let Some((w, h)) = media_size(&key) else {
            continue;
        };
        let same = |(x, y): (i32, i32)| folitecta_core::print::same_size((hmm(x), hmm(y)), (w, h), 1.5);
        let margins = default_col
            .filter(|(size, _)| same(*size))
            .map(|(_, m)| m)
            .or_else(|| {
                database
                    .iter()
                    .find(|(size, m)| same(*size) && m.iter().any(|v| *v > 0.0))
                    .map(|(_, m)| *m)
            })
            .or_else(|| database.iter().find(|(size, _)| same(*size)).map(|(_, m)| *m))
            .unwrap_or(fallback);
        media.push(Media {
            name: standard_name(&key).unwrap_or_default().to_string(),
            width: w.min(h),
            height: w.max(h),
            margins,
            key,
        });
    }
    let sides: Vec<Sides> = strings("sides-supported")
        .iter()
        .filter_map(|s| sides_of(s))
        .collect();
    let default_sides = strings("sides-default")
        .first()
        .and_then(|s| sides_of(s))
        .unwrap_or(Sides::OneSided);
    let max_copies = find(attrs, "copies-supported")
        .and_then(|a| {
            a.values.iter().find_map(|v| match v {
                Value::Range(_, hi) => Some(*hi),
                Value::Int(i) => Some(*i),
                _ => None,
            })
        })
        .unwrap_or(1)
        .clamp(1, 9999) as u32;
    let color_modes = strings("print-color-mode-supported");
    let color = color_modes.iter().any(|m| m == "color" || m == "auto");
    let default_color = strings("print-color-mode-default")
        .first()
        .map(|m| m != "monochrome")
        .unwrap_or(color);
    let resolutions = find(attrs, "printer-resolution-supported")
        .map(|a| {
            a.values
                .iter()
                .filter_map(|v| match v {
                    // Unit 3: dots per inch.
                    Value::Res(x, y, 3) => Some((*x as u32, *y as u32)),
                    Value::Res(x, y, 4) => Some(((*x as f32 * 2.54) as u32, (*y as f32 * 2.54) as u32)),
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default();
    PrinterCaps {
        default_media: strings("media-default").into_iter().next(),
        media,
        duplex: sides.iter().any(|s| s.duplex()),
        sides,
        default_sides,
        max_copies,
        collate: strings("multiple-document-handling-supported")
            .iter()
            .any(|s| s == "separate-documents-collated-copies"),
        color,
        default_color,
        color_modes,
        resolutions,
        accepts_pdf: strings("document-format-supported")
            .iter()
            .any(|f| f == "application/pdf" || f == "application/octet-stream"),
        scaling_none: strings("print-scaling-supported").iter().any(|s| s == "none"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ticket() -> JobTicket {
        JobTicket {
            printer: "Office".into(),
            job_name: "Report".into(),
            copies: 2,
            collate: true,
            sides: Sides::LongEdge,
            media: Some(Media {
                key: "iso_a4_210x297mm".into(),
                name: "A4".into(),
                width: 595.28,
                height: 841.89,
                margins: [0.0; 4],
            }),
            gray: false,
            dpi: 600,
            devmode: None,
        }
    }

    fn caps() -> PrinterCaps {
        PrinterCaps {
            sides: vec![Sides::OneSided, Sides::LongEdge, Sides::ShortEdge],
            duplex: true,
            max_copies: 99,
            collate: true,
            color: true,
            color_modes: vec!["color".into(), "monochrome".into()],
            scaling_none: true,
            accepts_pdf: true,
            ..Default::default()
        }
    }

    #[test]
    fn a_print_job_is_encoded_byte_for_byte() {
        let groups = [
            Group {
                tag: OPERATION,
                attrs: vec![
                    Attr::text(CHARSET, "attributes-charset", "utf-8"),
                    Attr::text(MIME, "document-format", "application/pdf"),
                ],
            },
            Group {
                tag: JOB,
                attrs: vec![
                    Attr::int(INTEGER, "copies", 2),
                    Attr::int(ENUM, "orientation-requested", 3),
                    Attr::new(
                        KEYWORD,
                        "sides",
                        vec![Value::Str("one-sided".into()), Value::Str("two".into())],
                    ),
                ],
            },
        ];
        let bytes = encode(PRINT_JOB, 7, &groups);
        let mut want: Vec<u8> = vec![2, 0, 0, 2, 0, 0, 0, 7, 1];
        want.extend_from_slice(&[0x47, 0, 18]);
        want.extend_from_slice(b"attributes-charset");
        want.extend_from_slice(&[0, 5]);
        want.extend_from_slice(b"utf-8");
        want.extend_from_slice(&[0x49, 0, 15]);
        want.extend_from_slice(b"document-format");
        want.extend_from_slice(&[0, 15]);
        want.extend_from_slice(b"application/pdf");
        want.push(2);
        want.extend_from_slice(&[0x21, 0, 6]);
        want.extend_from_slice(b"copies");
        want.extend_from_slice(&[0, 4, 0, 0, 0, 2]);
        want.extend_from_slice(&[0x23, 0, 21]);
        want.extend_from_slice(b"orientation-requested");
        want.extend_from_slice(&[0, 4, 0, 0, 0, 3]);
        want.extend_from_slice(&[0x44, 0, 5]);
        want.extend_from_slice(b"sides");
        want.extend_from_slice(&[0, 9]);
        want.extend_from_slice(b"one-sided");
        want.extend_from_slice(&[0x44, 0, 0, 0, 3]);
        want.extend_from_slice(b"two");
        want.push(3);
        assert_eq!(bytes, want);
        // And back.
        let m = decode(&bytes).unwrap();
        assert_eq!((m.code, m.request_id), (PRINT_JOB, 7));
        assert_eq!(m.groups.len(), 2);
        assert_eq!(m.attr(JOB, "sides").unwrap().strings(), ["one-sided", "two"]);
        assert_eq!(m.attr(JOB, "orientation-requested").unwrap().first_int(), Some(3));
    }

    /// A CUPS-Get-Printers answer with two printers and a printer
    /// attributes answer with a media collection, written by hand.
    fn printers_answer() -> Vec<u8> {
        let printer = |name: &str, state: i32, reasons: &[&str], accepting: bool| Group {
            tag: PRINTER,
            attrs: vec![
                Attr::text(NAME, "printer-name", name),
                Attr::text(TEXT, "printer-info", &format!("{name} on the desk")),
                Attr::text(TEXT, "printer-location", "Office"),
                Attr::int(ENUM, "printer-state", state),
                Attr::new(
                    KEYWORD,
                    "printer-state-reasons",
                    reasons.iter().map(|r| Value::Str((*r).into())).collect(),
                ),
                Attr::new(BOOLEAN, "printer-is-accepting-jobs", vec![Value::Bool(accepting)]),
                Attr::int(INTEGER, "queued-job-count", 2),
            ],
        };
        let mut groups = vec![Group {
            tag: OPERATION,
            attrs: vec![Attr::text(CHARSET, "attributes-charset", "utf-8")],
        }];
        groups.push(printer("Laser", 3, &["none"], true));
        groups.push(printer("Photo", 5, &["offline-report"], false));
        // An unknown group is skipped by the readers.
        groups.push(Group {
            tag: 0x07,
            attrs: vec![Attr::new(0x15, "x", vec![Value::OutOfBand(0x15)])],
        });
        encode(0x0000, 1, &groups)
    }

    #[test]
    fn printers_are_read_from_an_answer() {
        let m = decode(&printers_answer()).unwrap();
        assert_eq!(status_error(m.code, ""), None);
        let list = printers_from(&m, Some("Photo"));
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].name, "Laser");
        assert_eq!(list[0].label, "Laser on the desk");
        assert_eq!(list[0].state, PrinterState::Idle);
        assert_eq!(list[0].jobs, 2);
        assert!(!list[0].is_default);
        assert_eq!(list[1].state, PrinterState::Offline);
        assert!(list[1].is_default && !list[1].accepting);
        assert_eq!(m.groups[3].attrs[0].values, [Value::OutOfBand(0x15)]);
    }

    fn col(members: Vec<Attr>) -> Value {
        Value::Collection(members)
    }

    fn media_col_value(x: i32, y: i32, margins: [i32; 4]) -> Value {
        col(vec![
            Attr::new(
                BEG_COLLECTION,
                "media-size",
                vec![col(vec![
                    Attr::int(INTEGER, "x-dimension", x),
                    Attr::int(INTEGER, "y-dimension", y),
                ])],
            ),
            Attr::int(INTEGER, "media-left-margin", margins[0]),
            Attr::int(INTEGER, "media-bottom-margin", margins[1]),
            Attr::int(INTEGER, "media-right-margin", margins[2]),
            Attr::int(INTEGER, "media-top-margin", margins[3]),
        ])
    }

    #[test]
    fn capabilities_are_read_from_the_printer_attributes() {
        let groups = [
            Group {
                tag: OPERATION,
                attrs: vec![Attr::text(CHARSET, "attributes-charset", "utf-8")],
            },
            Group {
                tag: PRINTER,
                attrs: vec![
                    Attr::new(
                        KEYWORD,
                        "media-supported",
                        [
                            "iso_a4_210x297mm",
                            "na_letter_8.5x11in",
                            "custom_min_10x10mm",
                            "iso_a5_148x210mm",
                        ]
                        .iter()
                        .map(|s| Value::Str((*s).into()))
                        .collect(),
                    ),
                    Attr::text(KEYWORD, "media-default", "iso_a4_210x297mm"),
                    Attr::new(
                        BEG_COLLECTION,
                        "media-col-default",
                        vec![media_col_value(21000, 29700, [300, 500, 300, 300])],
                    ),
                    Attr::new(
                        BEG_COLLECTION,
                        "media-col-database",
                        vec![
                            media_col_value(21590, 27940, [0, 0, 0, 0]),
                            media_col_value(21590, 27940, [635, 635, 635, 635]),
                        ],
                    ),
                    Attr::new(
                        INTEGER,
                        "media-bottom-margin-supported",
                        vec![Value::Int(0), Value::Int(423)],
                    ),
                    Attr::new(
                        KEYWORD,
                        "sides-supported",
                        vec![
                            Value::Str("one-sided".into()),
                            Value::Str("two-sided-long-edge".into()),
                        ],
                    ),
                    Attr::text(KEYWORD, "sides-default", "two-sided-long-edge"),
                    Attr::new(RANGE, "copies-supported", vec![Value::Range(1, 999)]),
                    Attr::new(
                        KEYWORD,
                        "multiple-document-handling-supported",
                        vec![
                            Value::Str("separate-documents-uncollated-copies".into()),
                            Value::Str("separate-documents-collated-copies".into()),
                        ],
                    ),
                    Attr::new(
                        KEYWORD,
                        "print-color-mode-supported",
                        vec![Value::Str("monochrome".into()), Value::Str("color".into())],
                    ),
                    Attr::text(KEYWORD, "print-color-mode-default", "monochrome"),
                    Attr::new(
                        RESOLUTION,
                        "printer-resolution-supported",
                        vec![Value::Res(600, 600, 3)],
                    ),
                    Attr::new(
                        MIME,
                        "document-format-supported",
                        vec![Value::Str("application/pdf".into())],
                    ),
                    Attr::text(KEYWORD, "print-scaling-supported", "none"),
                ],
            },
        ];
        let m = decode(&encode(0, 3, &groups)).unwrap();
        let c = caps_from(&m);
        assert_eq!(c.media.len(), 3);
        let a4 = &c.media[0];
        assert_eq!((a4.key.as_str(), a4.name.as_str()), ("iso_a4_210x297mm", "A4"));
        let mm3 = 3.0 / 25.4 * 72.0;
        assert!((a4.margins[0] - mm3).abs() < 0.01 && (a4.margins[1] - 5.0 / 25.4 * 72.0).abs() < 0.01);
        let letter = &c.media[1];
        assert!(
            (letter.margins[0] - 6.35 / 25.4 * 72.0).abs() < 0.01,
            "{:?}",
            letter.margins
        );
        // No collection for A5: the largest margins the printer lists.
        assert!((c.media[2].margins[1] - 4.23 / 25.4 * 72.0).abs() < 0.01);
        assert_eq!(c.default_media.as_deref(), Some("iso_a4_210x297mm"));
        assert!(c.duplex && c.sides == [Sides::OneSided, Sides::LongEdge]);
        assert_eq!(c.default_sides, Sides::LongEdge);
        assert_eq!(c.max_copies, 999);
        assert!(c.collate && c.color && !c.default_color);
        assert_eq!(c.resolutions, [(600, 600)]);
        assert!(c.accepts_pdf && c.scaling_none);
    }

    #[test]
    fn statuses_map_to_errors() {
        assert_eq!(status_error(0x0000, ""), None);
        assert_eq!(status_error(0x0001, ""), None);
        assert_eq!(status_error(0x0002, ""), None);
        assert_eq!(status_error(0x0401, ""), Some(PrintError::Auth));
        assert_eq!(status_error(0x0403, ""), Some(PrintError::Auth));
        assert_eq!(status_error(0x0406, "x"), Some(PrintError::NotFound("x".into())));
        assert_eq!(status_error(0x040A, ""), Some(PrintError::FormatNotSupported));
        assert_eq!(status_error(0x0506, ""), Some(PrintError::NotAccepting));
        assert_eq!(
            status_error(0x040B, "bad media"),
            Some(PrintError::Server {
                status: 0x040B,
                message: "bad media".into()
            })
        );
    }

    #[test]
    fn job_attributes_follow_the_printer() {
        let t = ticket();
        let names = |attrs: &[Attr]| attrs.iter().map(|a| a.name.clone()).collect::<Vec<_>>();
        let all = job_attributes(&t, &caps(), &[]);
        assert_eq!(
            names(&all),
            [
                "copies",
                "multiple-document-handling",
                "sides",
                "media",
                "print-color-mode",
                "orientation-requested",
                "print-scaling"
            ]
        );
        assert_eq!(all[1].first_str(), Some("separate-documents-collated-copies"));
        // A printer without duplex, colour modes or scaling, paper by page, one copy.
        let bare = PrinterCaps {
            sides: vec![Sides::OneSided],
            max_copies: 99,
            ..Default::default()
        };
        let one = JobTicket {
            copies: 1,
            media: None,
            ..ticket()
        };
        assert_eq!(
            names(&job_attributes(&one, &bare, &[])),
            ["orientation-requested"]
        );
        // Uncollated copies, and attributes the server refused left out.
        let unc = JobTicket {
            collate: false,
            ..ticket()
        };
        let a = job_attributes(&unc, &caps(), &["print-scaling".to_string(), "media".to_string()]);
        assert_eq!(a[1].first_str(), Some("separate-documents-uncollated-copies"));
        assert!(
            !names(&a).contains(&"print-scaling".to_string()) && !names(&a).contains(&"media".to_string())
        );
    }

    #[test]
    fn margins_are_asked_by_their_own_names() {
        // IPP has no wildcards in requested attributes.
        for side in ["bottom", "left", "right", "top"] {
            let name = format!("media-{side}-margin-supported");
            assert!(CAPS_ATTRS.contains(&name.as_str()), "{name}");
        }
        assert!(CAPS_ATTRS.iter().all(|a| !a.contains('*')));
    }

    #[test]
    fn names_and_addresses_are_safe() {
        let long = "é".repeat(200);
        let cut = name_value(&long);
        assert!(cut.len() <= 255 && cut.chars().all(|c| c == 'é'));
        assert_eq!(
            printer_uri("HP LaserJet/2"),
            "ipp://localhost/printers/HP%20LaserJet%2F2"
        );
        let ops = operation_attrs(Some("ipp://localhost/printers/x"), "anna", "it");
        assert_eq!(ops[1].first_str(), Some("it"));
        assert_eq!(ops.len(), 4);
    }

    #[test]
    fn damaged_answers_are_errors_not_panics() {
        let good = printers_answer();
        for cut in [0, 3, 9, 20, good.len() - 1] {
            assert!(decode(&good[..cut]).is_err(), "{cut}");
        }
        // An attribute before any group.
        assert!(decode(&[2, 0, 0, 0, 0, 0, 0, 1, 0x44, 0, 1, b'a', 0, 0, 3]).is_err());
        // A text with a language.
        let mut b = vec![
            2, 0, 0, 0, 0, 0, 0, 1, 1, 0x35, 0, 1, b't', 0, 9, 0, 2, b'e', b'n', 0, 3, b'a', b'b', b'c', 3,
        ];
        let m = decode(&b).unwrap();
        assert_eq!(m.attr(1, "t").unwrap().first_str(), Some("abc"));
        b[16] = 9;
        assert!(decode(&b).is_err());
    }
}
