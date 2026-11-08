//! The objects of the file as they are written (FOL-L004): from the trailer
//! or any object, the entries of a dictionary or array, each with what it
//! holds in a few words and where it leads (inside it, or the object a
//! reference names); a stream with its dictionary and its data, as text
//! when it reads as text. Nothing is changed.

use lopdf::{Object, ObjectId};

use crate::document::Document;
use crate::error::{Error, Result};
use crate::util::decode_pdf_text;

/// Where an inspected value is: the trailer (no object) or an object, then
/// the keys of dictionaries and the indices of arrays (`[2]`) inside it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InspectPath {
    pub object: Option<ObjectId>,
    pub steps: Vec<String>,
}

impl InspectPath {
    pub fn trailer() -> Self {
        InspectPath::default()
    }

    pub fn object(id: ObjectId) -> Self {
        InspectPath {
            object: Some(id),
            steps: Vec::new(),
        }
    }

    fn then(&self, step: String) -> Self {
        let mut p = self.clone();
        p.steps.push(step);
        p
    }

    /// Its name: `trailer` or `12 0 obj`, then the steps.
    pub fn title(&self) -> String {
        let mut s = match self.object {
            None => "trailer".to_string(),
            Some((n, g)) => format!("{n} {g} obj"),
        };
        for step in &self.steps {
            s.push_str(" › ");
            s.push_str(step);
        }
        s
    }
}

/// One entry of what is inspected.
#[derive(Debug, Clone, PartialEq)]
pub struct InspectEntry {
    /// Its key (`/Type`) or index (`[0]`).
    pub key: String,
    /// `dictionary`, `array`, `stream`, `reference`, `number`, `string`,
    /// `name`, `boolean` or `null`.
    pub kind: &'static str,
    /// What it holds, in a few words.
    pub summary: String,
    /// Where it leads, when it holds more.
    pub open: Option<InspectPath>,
}

/// The data of a stream.
#[derive(Debug, Clone, PartialEq)]
pub struct StreamView {
    /// Bytes as written and once decoded (None when it cannot be decoded).
    pub length: usize,
    pub decoded: Option<usize>,
    /// The data as text (when it reads as text), else its first bytes in
    /// hexadecimal; at most [`STREAM_SHOWN`] bytes of it.
    pub text: String,
    pub is_text: bool,
    pub truncated: bool,
}

/// What an inspected place holds.
#[derive(Debug, Clone, PartialEq)]
pub struct Inspected {
    pub path: InspectPath,
    /// What it is, in a few words.
    pub summary: String,
    pub entries: Vec<InspectEntry>,
    pub stream: Option<StreamView>,
}

/// The most bytes of a stream shown.
pub const STREAM_SHOWN: usize = 64 * 1024;

fn kind(o: &Object) -> &'static str {
    match o {
        Object::Null => "null",
        Object::Boolean(_) => "boolean",
        Object::Integer(_) | Object::Real(_) => "number",
        Object::Name(_) => "name",
        Object::String(..) => "string",
        Object::Array(_) => "array",
        Object::Dictionary(_) => "dictionary",
        Object::Stream(_) => "stream",
        Object::Reference(_) => "reference",
    }
}

fn name(n: &[u8]) -> String {
    format!("/{}", String::from_utf8_lossy(n))
}

fn shorten(s: String, most: usize) -> String {
    if s.chars().count() > most {
        format!("{}…", s.chars().take(most).collect::<String>())
    } else {
        s
    }
}

/// `/Type` and `/Subtype` of a dictionary, as words.
fn typed(d: &lopdf::Dictionary) -> String {
    [b"Type".as_slice(), b"Subtype", b"S", b"FT"]
        .iter()
        .filter_map(|k| d.get(k).ok().and_then(|v| v.as_name().ok()).map(name))
        .collect::<Vec<_>>()
        .join(" ")
}

/// What `o` holds, in a few words; `doc` names what references lead to.
fn summary(doc: &lopdf::Document, o: &Object) -> String {
    match o {
        Object::Null => "null".into(),
        Object::Boolean(b) => b.to_string(),
        Object::Integer(i) => i.to_string(),
        Object::Real(r) => r.to_string(),
        Object::Name(n) => name(n),
        Object::String(s, _) => format!("({})", shorten(decode_pdf_text(s), 80)),
        Object::Array(a) => {
            let small = a.len() <= 6
                && a.iter()
                    .all(|x| matches!(x, Object::Integer(_) | Object::Real(_) | Object::Name(_)));
            if small {
                format!(
                    "[{}]",
                    a.iter().map(|x| summary(doc, x)).collect::<Vec<_>>().join(" ")
                )
            } else {
                format!("array of {}", a.len())
            }
        }
        Object::Dictionary(d) => format!("dictionary of {} {}", d.len(), typed(d))
            .trim()
            .to_string(),
        Object::Stream(s) => {
            let filters = s
                .filters()
                .map(|f| f.iter().map(|n| name(n)).collect::<Vec<_>>().join(" "))
                .unwrap_or_default();
            format!(
                "stream of {} bytes {} {}",
                s.content.len(),
                filters,
                typed(&s.dict)
            )
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
        }
        Object::Reference((n, g)) => match doc.objects.get(&(*n, *g)) {
            Some(target) => format!("{n} {g} R → {}", summary_short(target)),
            None => format!("{n} {g} R → missing"),
        },
    }
}

/// What an object a reference names is, without following it further.
fn summary_short(o: &Object) -> String {
    match o {
        Object::Dictionary(d) => format!("dictionary {}", typed(d)).trim().to_string(),
        Object::Stream(s) => format!("stream {}", typed(&s.dict)).trim().to_string(),
        Object::Array(a) => format!("array of {}", a.len()),
        other => kind(other).to_string(),
    }
}

/// The entries of `o` found at `path`.
fn entries(doc: &lopdf::Document, o: &Object, path: &InspectPath) -> Vec<InspectEntry> {
    let entry = |key: String, v: &Object| InspectEntry {
        key: key.clone(),
        kind: kind(v),
        summary: summary(doc, v),
        open: match v {
            Object::Reference(r) => doc.objects.contains_key(r).then(|| InspectPath::object(*r)),
            Object::Array(_) | Object::Dictionary(_) | Object::Stream(_) => Some(path.then(key)),
            _ => None,
        },
    };
    match o {
        Object::Dictionary(d) => d.iter().map(|(k, v)| entry(name(k), v)).collect(),
        Object::Stream(s) => s.dict.iter().map(|(k, v)| entry(name(k), v)).collect(),
        Object::Array(a) => a
            .iter()
            .enumerate()
            .map(|(i, v)| entry(format!("[{i}]"), v))
            .collect(),
        _ => Vec::new(),
    }
}

/// The data of stream `s`.
fn stream_view(s: &lopdf::Stream) -> StreamView {
    let decoded = if s.filters().map(|f| f.is_empty()).unwrap_or(true) {
        Some(s.content.clone())
    } else {
        s.decompressed_content().ok()
    };
    let data = decoded.as_deref().unwrap_or(&s.content);
    let sample = &data[..data.len().min(STREAM_SHOWN)];
    let readable = sample
        .iter()
        .filter(|b| matches!(b, b'\n' | b'\r' | b'\t') || (0x20..0x7f).contains(*b))
        .count();
    let is_text = readable * 100 >= sample.len() * 95;
    let (text, truncated) = if is_text {
        (
            String::from_utf8_lossy(sample).to_string(),
            data.len() > STREAM_SHOWN,
        )
    } else {
        let head = &data[..data.len().min(512)];
        let hex = head
            .chunks(32)
            .map(|c| c.iter().map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(" "))
            .collect::<Vec<_>>()
            .join("\n");
        (hex, data.len() > 512)
    };
    StreamView {
        length: s.content.len(),
        decoded: decoded.as_ref().map(Vec::len),
        text,
        is_text,
        truncated,
    }
}

impl Document {
    /// What the file holds at `path` (see the module). Refused when the
    /// security settings forbid copying, as it shows the text of the file.
    pub fn inspect(&self, path: &InspectPath) -> Result<Inspected> {
        self.require(crate::security::Restricted::Copy)?;
        let doc = &self.inner;
        let trailer = Object::Dictionary(doc.trailer.clone());
        let mut at: &Object = match path.object {
            None => &trailer,
            Some(id) => doc
                .objects
                .get(&id)
                .ok_or_else(|| Error::Invalid(format!("there is no object {} {}", id.0, id.1)))?,
        };
        for step in &path.steps {
            let next = match at {
                Object::Dictionary(d) => d.get(step.trim_start_matches('/').as_bytes()).ok(),
                Object::Stream(s) => s.dict.get(step.trim_start_matches('/').as_bytes()).ok(),
                Object::Array(a) => step
                    .trim_start_matches('[')
                    .trim_end_matches(']')
                    .parse::<usize>()
                    .ok()
                    .and_then(|i| a.get(i)),
                _ => None,
            };
            at = next.ok_or_else(|| Error::Invalid(format!("{} holds no {step}", path.title())))?;
        }
        Ok(Inspected {
            path: path.clone(),
            summary: summary(doc, at),
            entries: entries(doc, at, path),
            stream: match at {
                Object::Stream(s) => Some(stream_view(s)),
                _ => None,
            },
        })
    }

    /// How many objects the file has.
    pub fn object_count(&self) -> usize {
        self.inner.objects.len()
    }
}
