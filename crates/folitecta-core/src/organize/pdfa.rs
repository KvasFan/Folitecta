//! Preflight for PDF/A-2b (FOL-L043): a named profile with the rules it
//! covers, each finding with its page or object; conversion of a copy of
//! the document that fixes what can be fixed (the file identifier, XMP
//! metadata agreeing with the document information, an sRGB output intent,
//! forbidden actions, annotation flags and appearances, LZW streams, XFA,
//! interpolation, optional content configurations) and declares PDF/A-2b
//! in its metadata only when no rule is left broken. The metadata alone
//! never claims conformance; the original document is never changed.
//!
//! The rules are a subset of ISO 19005-2 level B, listed in [`RULES`];
//! what they do not cover (transparency, fonts beyond being embedded,
//! colour spaces inside ICC-based ones, ...) is not checked.

use std::collections::{BTreeSet, HashSet};

use lopdf::{Dictionary, Object, ObjectId, Stream, dictionary};

use crate::document::Document;
use crate::error::{Error, Result};
use crate::util::{deref, text_of};
use crate::xml::escape_text;

/// The profile: what it checks against.
pub const PROFILE: &str = "PDF/A-2b";
pub const PROFILE_STANDARD: &str = "ISO 19005-2:2011, level B";

/// The rules checked: id, clause of the standard, what it asks.
pub const RULES: &[(&str, &str, &str)] = &[
    ("encryption", "6.1.3", "The file is not encrypted"),
    ("file-id", "6.1.3", "The trailer has a file identifier"),
    ("metadata", "6.6.2", "The document has XMP metadata"),
    (
        "metadata-sync",
        "6.6.2.3",
        "The XMP metadata agrees with the document information",
    ),
    (
        "identification",
        "6.6.4",
        "The XMP metadata identifies PDF/A-2, level B",
    ),
    (
        "output-intent",
        "6.2.3",
        "Device colours have a PDF/A output intent",
    ),
    ("fonts", "6.2.11.4", "Every font is embedded"),
    (
        "actions",
        "6.5",
        "No JavaScript, launch, sound, movie or other forbidden action",
    ),
    (
        "annotation-type",
        "6.3.1",
        "No sound, movie, screen, 3D or rich media annotation",
    ),
    (
        "annotation-flags",
        "6.3.2",
        "Annotations are printed and not hidden",
    ),
    (
        "annotation-appearance",
        "6.3.3",
        "Annotations have a normal appearance only",
    ),
    (
        "embedded-files",
        "6.8",
        "No embedded file that is not known to be PDF/A",
    ),
    ("lzw", "6.1.7", "No LZW compression"),
    ("xfa", "6.4.2", "No XFA forms"),
    (
        "need-appearances",
        "6.4.1",
        "Form fields do not ask for appearances to be made",
    ),
    ("interpolate", "6.2.8", "Pictures are not interpolated"),
    (
        "optional-content",
        "6.9",
        "Optional content configurations are named and plain",
    ),
    ("postscript", "6.2.9", "No PostScript drawings"),
];

/// One rule broken.
#[derive(Debug, Clone, PartialEq)]
pub struct Finding {
    pub rule: &'static str,
    pub page: Option<usize>,
    pub object: Option<(u32, u16)>,
    pub detail: String,
    /// Conversion fixes it.
    pub fixable: bool,
}

/// The result of a preflight.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Preflight {
    pub findings: Vec<Finding>,
}

impl Preflight {
    /// No rule is broken.
    pub fn passed(&self) -> bool {
        self.findings.is_empty()
    }

    /// The rules broken, in the order of [`RULES`], with how many times.
    pub fn broken(&self) -> Vec<(&'static str, usize)> {
        RULES
            .iter()
            .filter_map(|(id, _, _)| {
                let n = self.findings.iter().filter(|f| f.rule == *id).count();
                (n > 0).then_some((*id, n))
            })
            .collect()
    }
}

/// What a conversion did.
#[derive(Debug, Clone, PartialEq)]
pub struct Conversion {
    pub before: Preflight,
    pub after: Preflight,
    /// The document declares PDF/A-2b (only when nothing was left).
    pub claimed: bool,
}

/// Actions PDF/A-2 forbids.
const FORBIDDEN_ACTIONS: &[&[u8]] = &[
    b"Launch",
    b"Sound",
    b"Movie",
    b"ResetForm",
    b"ImportData",
    b"Hide",
    b"SetOCGState",
    b"Rendition",
    b"Trans",
    b"GoTo3DView",
    b"JavaScript",
];

/// Annotation types PDF/A-2 forbids.
const FORBIDDEN_ANNOTATIONS: &[&[u8]] = &[b"Sound", b"Movie", b"Screen", b"3D", b"RichMedia"];

/// Annotation flags: invisible, hidden, print, no view, toggle no view.
const F_INVISIBLE: i64 = 1;
const F_HIDDEN: i64 = 2;
const F_PRINT: i64 = 4;
const F_NO_VIEW: i64 = 32;
const F_TOGGLE_NO_VIEW: i64 = 256;

fn name_of<'a>(d: &'a Dictionary, key: &[u8]) -> Option<&'a [u8]> {
    d.get(key).ok().and_then(|o| o.as_name().ok())
}

/// A PDF date (`D:20261026101500+01'00'`) as XMP (`2026-10-26T10:15:00+01:00`).
pub fn xmp_date(pdf: &str) -> Option<String> {
    let s = pdf.trim().trim_start_matches("D:");
    let digits: String = s.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.len() < 4 {
        return None;
    }
    let part = |a: usize, b: usize, d: &str| digits.get(a..b).unwrap_or(d).to_string();
    let (y, mo, da) = (part(0, 4, "0000"), part(4, 6, "01"), part(6, 8, "01"));
    let (h, mi, se) = (part(8, 10, "00"), part(10, 12, "00"), part(12, 14, "00"));
    let rest = &s[digits.len()..];
    let zone = match rest.chars().next() {
        Some('Z') | None => "Z".to_string(),
        Some(sign @ ('+' | '-')) => {
            let z: String = rest[1..].chars().filter(|c| c.is_ascii_digit()).collect();
            format!(
                "{sign}{}:{}",
                z.get(0..2).unwrap_or("00"),
                z.get(2..4).unwrap_or("00")
            )
        }
        _ => "Z".to_string(),
    };
    Some(format!("{y}-{mo}-{da}T{h}:{mi}:{se}{zone}"))
}

/// The document information that XMP has to repeat: key, value.
fn info_entries(doc: &lopdf::Document) -> Vec<(&'static str, String)> {
    let Some(info) = doc
        .trailer
        .get(b"Info")
        .ok()
        .map(|i| deref(doc, i))
        .and_then(|i| i.as_dict().ok())
    else {
        return Vec::new();
    };
    [
        "Title",
        "Author",
        "Subject",
        "Keywords",
        "Creator",
        "Producer",
        "CreationDate",
        "ModDate",
    ]
    .into_iter()
    .filter_map(|k| {
        let v = text_of(doc, info.get(k.as_bytes()).ok()?)?;
        (!v.trim().is_empty()).then_some((k, v))
    })
    .collect()
}

/// The XMP property that repeats an information entry.
fn xmp_name(key: &str) -> &'static str {
    match key {
        "Title" => "dc:title",
        "Author" => "dc:creator",
        "Subject" => "dc:description",
        "Keywords" => "pdf:Keywords",
        "Creator" => "xmp:CreatorTool",
        "Producer" => "pdf:Producer",
        "CreationDate" => "xmp:CreateDate",
        _ => "xmp:ModifyDate",
    }
}

/// An XMP packet of the document information, declaring PDF/A-2b when `claim`.
fn pdfa_xmp(entries: &[(&'static str, String)], claim: bool) -> String {
    let mut props = String::new();
    for (key, value) in entries {
        let v = escape_text(value);
        let p = match *key {
            "Title" => {
                format!("<dc:title><rdf:Alt><rdf:li xml:lang=\"x-default\">{v}</rdf:li></rdf:Alt></dc:title>")
            }
            "Author" => format!("<dc:creator><rdf:Seq><rdf:li>{v}</rdf:li></rdf:Seq></dc:creator>"),
            "Subject" => format!(
                "<dc:description><rdf:Alt><rdf:li xml:lang=\"x-default\">{v}</rdf:li></rdf:Alt></dc:description>"
            ),
            "Keywords" => format!("<pdf:Keywords>{v}</pdf:Keywords>"),
            "Creator" => format!("<xmp:CreatorTool>{v}</xmp:CreatorTool>"),
            "Producer" => format!("<pdf:Producer>{v}</pdf:Producer>"),
            k @ ("CreationDate" | "ModDate") => match xmp_date(value) {
                Some(d) => format!("<{n}>{d}</{n}>", n = xmp_name(k)),
                None => continue,
            },
            _ => continue,
        };
        props.push_str(&p);
    }
    props.push_str("<dc:format>application/pdf</dc:format>");
    if claim {
        props.push_str("<pdfaid:part>2</pdfaid:part><pdfaid:conformance>B</pdfaid:conformance>");
    }
    let ns = if claim {
        " xmlns:pdfaid=\"http://www.aiim.org/pdfa/ns/id/\""
    } else {
        ""
    };
    format!(
        "<?xpacket begin=\"\u{feff}\" id=\"W5M0MpCehiHzreSzNTczkc9d\"?>\n\
         <x:xmpmeta xmlns:x=\"adobe:ns:meta/\">\n\
         <rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\">\n\
         <rdf:Description rdf:about=\"\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\" \
         xmlns:pdf=\"http://ns.adobe.com/pdf/1.3/\" xmlns:xmp=\"http://ns.adobe.com/xap/1.0/\"{ns}>{props}</rdf:Description>\n\
         </rdf:RDF>\n\
         </x:xmpmeta>\n\
         <?xpacket end=\"w\"?>"
    )
}

/// An sRGB colour profile (ICC version 2.1, a matrix and a curve of 1024
/// points): the output intent of converted documents.
pub fn srgb_icc() -> Vec<u8> {
    fn s15(v: f64) -> [u8; 4] {
        ((v * 65536.0).round() as i32).to_be_bytes()
    }
    fn xyz(x: f64, y: f64, z: f64) -> Vec<u8> {
        let mut t = b"XYZ \0\0\0\0".to_vec();
        for v in [x, y, z] {
            t.extend(s15(v));
        }
        t
    }
    let desc = {
        let name = b"sRGB IEC61966-2.1\0";
        let mut t = b"desc\0\0\0\0".to_vec();
        t.extend((name.len() as u32).to_be_bytes());
        t.extend(name);
        t.extend([0u8; 4]); // Unicode language
        t.extend([0u8; 4]); // Unicode count
        t.extend([0u8; 2]); // ScriptCode code
        t.push(0); // ScriptCode count
        t.extend([0u8; 67]);
        t
    };
    let cprt = {
        let mut t = b"text\0\0\0\0".to_vec();
        t.extend(b"No copyright, use freely\0");
        t
    };
    let curve = {
        let mut t = b"curv\0\0\0\0".to_vec();
        t.extend(1024u32.to_be_bytes());
        for i in 0..1024 {
            let c = i as f64 / 1023.0;
            let l = if c <= 0.04045 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            };
            t.extend(((l * 65535.0).round() as u16).to_be_bytes());
        }
        t
    };
    let tags: Vec<(&[u8; 4], Vec<u8>)> = vec![
        (b"desc", desc),
        (b"cprt", cprt),
        (b"wtpt", xyz(0.9505, 1.0, 1.0891)),
        (b"rXYZ", xyz(0.4361, 0.2225, 0.0139)),
        (b"gXYZ", xyz(0.3851, 0.7169, 0.0971)),
        (b"bXYZ", xyz(0.1431, 0.0606, 0.7141)),
        (b"rTRC", curve),
    ];
    // The tag table: the three curves share one element.
    let count = tags.len() + 2;
    let mut offset = 128 + 4 + 12 * count;
    let mut table = Vec::new();
    let mut data = Vec::new();
    let mut curve_at = (0, 0);
    for (sig, body) in &tags {
        let at = offset;
        table.extend(*sig);
        table.extend((at as u32).to_be_bytes());
        table.extend((body.len() as u32).to_be_bytes());
        if *sig == b"rTRC" {
            curve_at = (at, body.len());
        }
        data.extend(body);
        offset += body.len();
        while !offset.is_multiple_of(4) {
            data.push(0);
            offset += 1;
        }
    }
    for sig in [b"gTRC", b"bTRC"] {
        table.extend(sig);
        table.extend((curve_at.0 as u32).to_be_bytes());
        table.extend((curve_at.1 as u32).to_be_bytes());
    }
    let size = offset;
    let mut h = Vec::with_capacity(size);
    h.extend((size as u32).to_be_bytes());
    h.extend([0u8; 4]); // CMM
    h.extend([0x02, 0x10, 0, 0]); // version 2.1
    h.extend(b"mntr");
    h.extend(b"RGB ");
    h.extend(b"XYZ ");
    for v in [2026u16, 1, 1, 0, 0, 0] {
        h.extend(v.to_be_bytes());
    }
    h.extend(b"acsp");
    h.extend([0u8; 4]); // platform
    h.extend([0u8; 4]); // flags
    h.extend([0u8; 4]); // manufacturer
    h.extend([0u8; 4]); // model
    h.extend([0u8; 8]); // attributes
    h.extend([0u8; 4]); // perceptual
    h.extend(s15(0.9642));
    h.extend(s15(1.0));
    h.extend(s15(0.8249));
    h.extend([0u8; 4]); // creator
    h.extend([0u8; 44]);
    debug_assert_eq!(h.len(), 128);
    h.extend((count as u32).to_be_bytes());
    h.extend(table);
    h.extend(data);
    h
}

/// Device colours a content stream uses: grey, RGB, CMYK.
#[derive(Debug, Default, Clone, Copy)]
struct DeviceColours {
    gray: bool,
    rgb: bool,
    cmyk: bool,
}

impl DeviceColours {
    fn any(&self) -> bool {
        self.gray || self.rgb || self.cmyk
    }

    fn name(&mut self, n: &[u8]) {
        match n {
            b"DeviceGray" | b"G" => self.gray = true,
            b"DeviceRGB" | b"RGB" => self.rgb = true,
            b"DeviceCMYK" | b"CMYK" => self.cmyk = true,
            _ => {}
        }
    }

    fn content(&mut self, data: &[u8]) {
        let Ok(content) = lopdf::content::Content::decode(data) else {
            return;
        };
        for op in content.operations {
            match op.operator.as_str() {
                "g" | "G" => self.gray = true,
                "rg" | "RG" => self.rgb = true,
                "k" | "K" => self.cmyk = true,
                "cs" | "CS" => {
                    if let Some(Ok(n)) = op.operands.first().map(|o| o.as_name()) {
                        self.name(n);
                    }
                }
                _ => {}
            }
        }
    }
}

impl Document {
    /// Check the document against the PDF/A-2b rules of [`RULES`].
    pub fn preflight_pdfa(&self) -> Preflight {
        let doc = &self.inner;
        let mut out: Vec<Finding> = Vec::new();
        let mut add = |rule, page, object: Option<ObjectId>, detail: String, fixable| {
            out.push(Finding {
                rule,
                page,
                object,
                detail,
                fixable,
            });
        };
        if self.is_protected() || doc.trailer.has(b"Encrypt") {
            add(
                "encryption",
                None,
                None,
                "the document is encrypted".into(),
                false,
            );
        }
        if !doc.trailer.has(b"ID") {
            add("file-id", None, None, "the trailer has no ID".into(), true);
        }
        // Metadata, its agreement with the information, the identification.
        let catalog = doc.catalog().ok().cloned().unwrap_or_default();
        let props = self.xmp_properties();
        let prop = |name: &str| {
            props
                .iter()
                .find(|(n, _)| n == name)
                .map(|(_, v)| v.trim().to_string())
        };
        if self.xmp_packet().is_none() {
            add("metadata", None, None, "no XMP metadata".into(), true);
        } else {
            for (key, value) in info_entries(doc) {
                let want = match key {
                    "CreationDate" | "ModDate" => xmp_date(&value),
                    _ => Some(value.trim().to_string()),
                };
                let have = prop(xmp_name(key));
                let same = match (&want, &have) {
                    (Some(w), Some(h)) if key.ends_with("Date") => w.get(..19) == h.get(..19),
                    (Some(w), Some(h)) => w == h,
                    _ => false,
                };
                if !same {
                    add(
                        "metadata-sync",
                        None,
                        None,
                        format!(
                            "{key} is {:?} in the information, {:?} in XMP",
                            value,
                            have.unwrap_or_default()
                        ),
                        true,
                    );
                }
            }
        }
        let part = prop("pdfaid:part");
        let conformance = prop("pdfaid:conformance");
        if part.as_deref() != Some("2") || !matches!(conformance.as_deref(), Some("B" | "b")) {
            add(
                "identification",
                None,
                None,
                format!(
                    "part {:?}, conformance {:?}",
                    part.unwrap_or_default(),
                    conformance.unwrap_or_default()
                ),
                true,
            );
        }
        // Colours.
        let mut colours = DeviceColours::default();
        let pages = doc.get_pages();
        for pid in pages.values() {
            colours.content(&doc.get_page_content(*pid));
        }
        for obj in doc.objects.values() {
            let Object::Stream(s) = obj else { continue };
            match name_of(&s.dict, b"Subtype") {
                Some(b"Form") => {
                    if let Ok(data) = s
                        .decompressed_content()
                        .or_else(|_| Ok::<_, lopdf::Error>(s.content.clone()))
                    {
                        colours.content(&data);
                    }
                }
                Some(b"Image") => {
                    if let Ok(cs) = s.dict.get(b"ColorSpace") {
                        match deref(doc, cs) {
                            Object::Name(n) => colours.name(n),
                            Object::Array(a)
                                if a.first().and_then(|x| x.as_name().ok()) == Some(b"Indexed") =>
                            {
                                if let Some(Ok(n)) = a.get(1).map(|b| deref(doc, b).as_name()) {
                                    colours.name(n);
                                }
                            }
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
        }
        let intent = catalog
            .get(b"OutputIntents")
            .ok()
            .map(|o| deref(doc, o))
            .and_then(|o| o.as_array().ok())
            .and_then(|a| {
                a.iter()
                    .filter_map(|i| deref(doc, i).as_dict().ok())
                    .find(|d| name_of(d, b"S") == Some(b"GTS_PDFA1"))
                    .cloned()
            });
        if colours.any() {
            let n = intent
                .as_ref()
                .and_then(|d| d.get(b"DestOutputProfile").ok())
                .and_then(|p| deref(doc, p).as_stream().ok())
                .and_then(|s| s.dict.get(b"N").ok().and_then(|n| n.as_i64().ok()));
            match n {
                None => add(
                    "output-intent",
                    None,
                    None,
                    "device colours without a PDF/A output intent".into(),
                    !colours.cmyk,
                ),
                Some(3) if colours.cmyk => add(
                    "output-intent",
                    None,
                    None,
                    "CMYK colours with an RGB output intent".into(),
                    false,
                ),
                Some(4) if colours.rgb => add(
                    "output-intent",
                    None,
                    None,
                    "RGB colours with a CMYK output intent".into(),
                    false,
                ),
                _ => {}
            }
        }
        // Objects one by one.
        let page_of: std::collections::HashMap<ObjectId, usize> = pages
            .iter()
            .enumerate()
            .flat_map(|(i, (_, pid))| {
                doc.get_dictionary(*pid)
                    .ok()
                    .and_then(|d| d.get(b"Annots").ok())
                    .map(|a| deref(doc, a))
                    .and_then(|a| a.as_array().ok())
                    .map(|a| {
                        a.iter()
                            .filter_map(|r| r.as_reference().ok())
                            .map(|r| (r, i))
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default()
            })
            .collect();
        let mut fonts_seen: BTreeSet<String> = BTreeSet::new();
        for (id, obj) in &doc.objects {
            match obj {
                Object::Stream(s) => {
                    let d = &s.dict;
                    let filters: Vec<&[u8]> = match d.get(b"Filter") {
                        Ok(Object::Name(n)) => vec![n.as_slice()],
                        Ok(Object::Array(a)) => a.iter().filter_map(|x| x.as_name().ok()).collect(),
                        _ => Vec::new(),
                    };
                    if filters.iter().any(|f| *f == b"LZWDecode" || *f == b"LZW") {
                        add(
                            "lzw",
                            None,
                            Some(*id),
                            "a stream compressed with LZW".into(),
                            true,
                        );
                    }
                    if d.get(b"Interpolate").ok().and_then(|b| b.as_bool().ok()) == Some(true) {
                        add(
                            "interpolate",
                            None,
                            Some(*id),
                            "a picture asks to be interpolated".into(),
                            true,
                        );
                    }
                    if name_of(d, b"Subtype") == Some(b"PS") || name_of(d, b"Subtype2") == Some(b"PS") {
                        add(
                            "postscript",
                            None,
                            Some(*id),
                            "a PostScript drawing".into(),
                            false,
                        );
                    }
                }
                Object::Dictionary(d) => {
                    let ty = name_of(d, b"Type");
                    if ty == Some(b"Font") {
                        let subtype = name_of(d, b"Subtype").unwrap_or_default();
                        if subtype == b"Type3" {
                            continue;
                        }
                        let descriptor = if subtype == b"Type0" {
                            d.get(b"DescendantFonts")
                                .ok()
                                .map(|x| deref(doc, x))
                                .and_then(|x| x.as_array().ok())
                                .and_then(|a| a.first())
                                .and_then(|f| deref(doc, f).as_dict().ok())
                                .and_then(|f| f.get(b"FontDescriptor").ok())
                        } else {
                            d.get(b"FontDescriptor").ok()
                        }
                        .and_then(|fd| deref(doc, fd).as_dict().ok());
                        let embedded = descriptor.is_some_and(|fd| {
                            fd.has(b"FontFile") || fd.has(b"FontFile2") || fd.has(b"FontFile3")
                        });
                        if !embedded {
                            let base = name_of(d, b"BaseFont")
                                .map(|b| String::from_utf8_lossy(b).to_string())
                                .unwrap_or_default();
                            if fonts_seen.insert(base.clone()) {
                                add("fonts", None, Some(*id), format!("{base} is not embedded"), false);
                            }
                        }
                    }
                    if ty == Some(b"Annot") || d.has(b"Subtype") && d.has(b"Rect") {
                        let page = page_of.get(id).copied();
                        let subtype = name_of(d, b"Subtype").unwrap_or_default();
                        let sub = String::from_utf8_lossy(subtype).to_string();
                        if FORBIDDEN_ANNOTATIONS.contains(&subtype) {
                            add(
                                "annotation-type",
                                page,
                                Some(*id),
                                format!("a {sub} annotation"),
                                false,
                            );
                        }
                        if subtype == b"FileAttachment" {
                            add(
                                "embedded-files",
                                page,
                                Some(*id),
                                "a file attachment annotation".into(),
                                false,
                            );
                        }
                        if subtype != b"Popup" {
                            let f = d.get(b"F").ok().and_then(|f| f.as_i64().ok()).unwrap_or(0);
                            if f & F_PRINT == 0
                                || f & (F_INVISIBLE | F_HIDDEN | F_NO_VIEW | F_TOGGLE_NO_VIEW) != 0
                            {
                                add(
                                    "annotation-flags",
                                    page,
                                    Some(*id),
                                    format!("a {sub} annotation is not printed or is hidden"),
                                    true,
                                );
                            }
                        }
                        let rect_empty = d
                            .get(b"Rect")
                            .ok()
                            .and_then(|r| r.as_array().ok())
                            .map(|r| {
                                let v: Vec<f32> = r.iter().filter_map(|x| x.as_float().ok()).collect();
                                v.len() == 4 && ((v[2] - v[0]).abs() < 1e-3 || (v[3] - v[1]).abs() < 1e-3)
                            })
                            .unwrap_or(false);
                        if subtype != b"Popup" && subtype != b"Link" && !rect_empty {
                            match d
                                .get(b"AP")
                                .ok()
                                .map(|a| deref(doc, a))
                                .and_then(|a| a.as_dict().ok())
                            {
                                None => add(
                                    "annotation-appearance",
                                    page,
                                    Some(*id),
                                    format!("a {sub} annotation has no appearance"),
                                    false,
                                ),
                                Some(ap) if ap.has(b"R") || ap.has(b"D") => add(
                                    "annotation-appearance",
                                    page,
                                    Some(*id),
                                    format!("a {sub} annotation has rollover or down appearances"),
                                    true,
                                ),
                                _ => {}
                            }
                        }
                        if d.has(b"AA") {
                            add(
                                "actions",
                                page,
                                Some(*id),
                                "additional actions of an annotation".into(),
                                true,
                            );
                        }
                        if let Some(a) = d
                            .get(b"A")
                            .ok()
                            .map(|a| deref(doc, a))
                            .and_then(|a| a.as_dict().ok())
                        {
                            if let Some(s) = name_of(a, b"S").filter(|s| FORBIDDEN_ACTIONS.contains(s)) {
                                add(
                                    "actions",
                                    page,
                                    Some(*id),
                                    format!("a {} action", String::from_utf8_lossy(s)),
                                    true,
                                );
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        // Catalog: actions, JavaScript, forms, optional content, attachments.
        if catalog.has(b"AA") {
            add(
                "actions",
                None,
                None,
                "additional actions of the document".into(),
                true,
            );
        }
        if let Some(open) = catalog
            .get(b"OpenAction")
            .ok()
            .map(|o| deref(doc, o))
            .and_then(|o| o.as_dict().ok())
        {
            if let Some(s) = name_of(open, b"S").filter(|s| FORBIDDEN_ACTIONS.contains(s)) {
                add(
                    "actions",
                    None,
                    None,
                    format!("an opening {} action", String::from_utf8_lossy(s)),
                    true,
                );
            }
        }
        for (i, (_, pid)) in pages.iter().enumerate() {
            if doc.get_dictionary(*pid).is_ok_and(|p| p.has(b"AA")) {
                add(
                    "actions",
                    Some(i),
                    Some(*pid),
                    "additional actions of a page".into(),
                    true,
                );
            }
        }
        let names = catalog
            .get(b"Names")
            .ok()
            .map(|n| deref(doc, n))
            .and_then(|n| n.as_dict().ok());
        if names.is_some_and(|n| n.has(b"JavaScript")) {
            add("actions", None, None, "document JavaScript".into(), true);
        }
        if names.is_some_and(|n| n.has(b"EmbeddedFiles")) {
            add("embedded-files", None, None, "embedded files".into(), false);
        }
        // Outline actions.
        let mut outline_actions = 0;
        for obj in doc.objects.values() {
            if let Object::Dictionary(d) = obj {
                if d.has(b"Title") && (d.has(b"Parent") || d.has(b"First")) {
                    if let Some(a) = d
                        .get(b"A")
                        .ok()
                        .map(|a| deref(doc, a))
                        .and_then(|a| a.as_dict().ok())
                    {
                        if name_of(a, b"S").is_some_and(|s| FORBIDDEN_ACTIONS.contains(&s)) {
                            outline_actions += 1;
                        }
                    }
                }
            }
        }
        if outline_actions > 0 {
            add(
                "actions",
                None,
                None,
                format!("{outline_actions} bookmarks with forbidden actions"),
                true,
            );
        }
        if let Some(form) = catalog
            .get(b"AcroForm")
            .ok()
            .map(|f| deref(doc, f))
            .and_then(|f| f.as_dict().ok())
        {
            if form.has(b"XFA") {
                add("xfa", None, None, "an XFA form".into(), true);
            }
            if form.get(b"NeedAppearances").ok().and_then(|b| b.as_bool().ok()) == Some(true) {
                add(
                    "need-appearances",
                    None,
                    None,
                    "NeedAppearances is true".into(),
                    true,
                );
            }
        }
        if let Some(oc) = catalog
            .get(b"OCProperties")
            .ok()
            .map(|o| deref(doc, o))
            .and_then(|o| o.as_dict().ok())
        {
            let mut configs: Vec<&Dictionary> = Vec::new();
            if let Some(d) = oc
                .get(b"D")
                .ok()
                .map(|d| deref(doc, d))
                .and_then(|d| d.as_dict().ok())
            {
                configs.push(d);
            }
            if let Some(a) = oc
                .get(b"Configs")
                .ok()
                .map(|c| deref(doc, c))
                .and_then(|c| c.as_array().ok())
            {
                configs.extend(a.iter().filter_map(|c| deref(doc, c).as_dict().ok()));
            }
            for c in configs {
                if !c.has(b"Name") || c.has(b"AS") {
                    add(
                        "optional-content",
                        None,
                        None,
                        "a layer configuration without a name or with automatic states".into(),
                        true,
                    );
                }
            }
        }
        Preflight { findings: out }
    }

    /// A copy of the document converted to PDF/A-2b: what can be fixed is
    /// fixed, and PDF/A-2b is declared only when no rule is left broken.
    pub fn convert_to_pdfa(&self) -> Result<(Document, Conversion)> {
        let before = self.preflight_pdfa();
        if before.findings.iter().any(|f| f.rule == "encryption") {
            return Err(Error::Invalid(
                "a protected document cannot become PDF/A: save an unrestricted copy first".into(),
            ));
        }
        let mut copy = self.duplicate()?;
        copy.fix_pdfa(&before)?;
        // The metadata of the document information, then the rules again:
        // PDF/A-2b is declared only when nothing else is left.
        let entries = info_entries(&copy.inner);
        copy.write_xmp_raw(&pdfa_xmp(&entries, false))?;
        copy.commit()?;
        let checked = copy.preflight_pdfa();
        if checked.findings.iter().all(|f| f.rule == "identification") {
            copy.write_xmp_raw(&pdfa_xmp(&entries, true))?;
            copy.commit()?;
        }
        let after = copy.preflight_pdfa();
        let claimed = after.passed();
        copy.set_path(None);
        copy.clear_history();
        Ok((
            copy,
            Conversion {
                before,
                after,
                claimed,
            },
        ))
    }

    fn write_xmp_raw(&mut self, packet: &str) -> Result<()> {
        let stream = Stream::new(
            dictionary! { "Type" => "Metadata", "Subtype" => "XML" },
            packet.as_bytes().to_vec(),
        );
        let id = self.inner.add_object(stream);
        let cat = self.inner.trailer.get(b"Root")?.as_reference()?;
        self.inner
            .get_dictionary_mut(cat)?
            .set("Metadata", Object::Reference(id));
        Ok(())
    }

    /// Fix what the findings say can be fixed (without a commit).
    fn fix_pdfa(&mut self, found: &Preflight) -> Result<()> {
        let rules: HashSet<&str> = found
            .findings
            .iter()
            .filter(|f| f.fixable)
            .map(|f| f.rule)
            .collect();
        let doc = &mut self.inner;
        if rules.contains("file-id") {
            use std::hash::{Hash, Hasher};
            let mut h = std::collections::hash_map::DefaultHasher::new();
            doc.objects.len().hash(&mut h);
            format!("{:?}", doc.trailer).hash(&mut h);
            std::time::SystemTime::now().hash(&mut h);
            let a = h.finish().to_be_bytes();
            let id: Vec<u8> = a.iter().chain(a.iter().rev()).copied().collect();
            doc.trailer.set(
                "ID",
                vec![
                    Object::String(id.clone(), lopdf::StringFormat::Hexadecimal),
                    Object::String(id, lopdf::StringFormat::Hexadecimal),
                ],
            );
        }
        let cat_id = doc.trailer.get(b"Root")?.as_reference()?;
        if rules.contains("output-intent") {
            let mut icc = Stream::new(dictionary! { "N" => 3 }, srgb_icc());
            let _ = icc.compress();
            let icc_id = doc.add_object(icc);
            let intent = doc.add_object(dictionary! {
                "Type" => "OutputIntent",
                "S" => "GTS_PDFA1",
                "OutputConditionIdentifier" => Object::string_literal("sRGB IEC61966-2.1"),
                "RegistryName" => Object::string_literal("http://www.color.org"),
                "Info" => Object::string_literal("sRGB IEC61966-2.1"),
                "DestOutputProfile" => icc_id,
            });
            doc.get_dictionary_mut(cat_id)?
                .set("OutputIntents", vec![Object::Reference(intent)]);
        }
        let forbidden = |d: &Dictionary| name_of(d, b"S").is_some_and(|s| FORBIDDEN_ACTIONS.contains(&s));
        if rules.contains("actions") {
            let cat = doc.get_dictionary_mut(cat_id)?;
            cat.remove(b"AA");
            let open_bad = match cat.get(b"OpenAction") {
                Ok(Object::Dictionary(d)) => forbidden(d),
                _ => false,
            };
            if open_bad {
                cat.remove(b"OpenAction");
            }
            let names = cat.get(b"Names").ok().cloned();
            match names {
                Some(Object::Reference(r)) => {
                    if let Ok(n) = doc.get_dictionary_mut(r) {
                        n.remove(b"JavaScript");
                    }
                }
                Some(Object::Dictionary(_)) => {
                    if let Ok(Object::Dictionary(n)) = doc.get_dictionary_mut(cat_id)?.get_mut(b"Names") {
                        n.remove(b"JavaScript");
                    }
                }
                _ => {}
            }
            if let Ok(Object::Reference(r)) = doc.get_dictionary(cat_id)?.get(b"OpenAction") {
                let r = *r;
                if doc.get_dictionary(r).is_ok_and(forbidden) {
                    doc.get_dictionary_mut(cat_id)?.remove(b"OpenAction");
                }
            }
            let bad_actions: HashSet<ObjectId> = doc
                .objects
                .iter()
                .filter_map(|(id, o)| o.as_dict().ok().filter(|d| forbidden(d)).map(|_| *id))
                .collect();
            for obj in doc.objects.values_mut() {
                if let Object::Dictionary(d) = obj {
                    if d.has(b"Type") && name_of(d, b"Type") == Some(b"Page")
                        || d.has(b"Rect")
                        || d.has(b"FT")
                    {
                        d.remove(b"AA");
                    }
                    let a_bad = match d.get(b"A") {
                        Ok(Object::Dictionary(a)) => forbidden(a),
                        Ok(Object::Reference(r)) => bad_actions.contains(r),
                        _ => false,
                    };
                    if a_bad {
                        d.remove(b"A");
                    }
                }
            }
        }
        for obj in doc.objects.values_mut() {
            match obj {
                Object::Dictionary(d) if d.has(b"Rect") && d.has(b"Subtype") => {
                    let subtype = name_of(d, b"Subtype").unwrap_or_default().to_vec();
                    if rules.contains("annotation-flags") && subtype != b"Popup" {
                        let f = d.get(b"F").ok().and_then(|f| f.as_i64().ok()).unwrap_or(0);
                        d.set(
                            "F",
                            (f | F_PRINT) & !(F_INVISIBLE | F_HIDDEN | F_NO_VIEW | F_TOGGLE_NO_VIEW),
                        );
                    }
                    if rules.contains("annotation-appearance") {
                        if let Ok(Object::Dictionary(ap)) = d.get_mut(b"AP") {
                            ap.remove(b"R");
                            ap.remove(b"D");
                        }
                    }
                }
                Object::Stream(s) => {
                    if rules.contains("interpolate") && s.dict.has(b"Interpolate") {
                        s.dict.set("Interpolate", false);
                    }
                    let lzw = match s.dict.get(b"Filter") {
                        Ok(Object::Name(n)) => n == b"LZWDecode" || n == b"LZW",
                        Ok(Object::Array(a)) => a
                            .iter()
                            .any(|x| x.as_name().ok().is_some_and(|n| n == b"LZWDecode" || n == b"LZW")),
                        _ => false,
                    };
                    if rules.contains("lzw") && lzw {
                        if let Ok(plain) = s.decompressed_content() {
                            s.dict.remove(b"Filter");
                            s.dict.remove(b"DecodeParms");
                            s.set_plain_content(plain);
                            let _ = s.compress();
                        }
                    }
                }
                _ => {}
            }
        }
        // Appearance dictionaries kept as references.
        if rules.contains("annotation-appearance") {
            let ap_refs: Vec<ObjectId> = doc
                .objects
                .values()
                .filter_map(|o| o.as_dict().ok())
                .filter(|d| d.has(b"Rect"))
                .filter_map(|d| d.get(b"AP").ok().and_then(|a| a.as_reference().ok()))
                .collect();
            for r in ap_refs {
                if let Ok(ap) = doc.get_dictionary_mut(r) {
                    ap.remove(b"R");
                    ap.remove(b"D");
                }
            }
        }
        let cat = doc.get_dictionary(cat_id)?.clone();
        if rules.contains("xfa") || rules.contains("need-appearances") {
            match cat.get(b"AcroForm") {
                Ok(Object::Reference(r)) => {
                    if let Ok(f) = doc.get_dictionary_mut(*r) {
                        f.remove(b"XFA");
                        f.remove(b"NeedAppearances");
                    }
                }
                Ok(Object::Dictionary(_)) => {
                    if let Ok(Object::Dictionary(f)) = doc.get_dictionary_mut(cat_id)?.get_mut(b"AcroForm") {
                        f.remove(b"XFA");
                        f.remove(b"NeedAppearances");
                    }
                }
                _ => {}
            }
        }
        if rules.contains("optional-content") {
            let oc_ref = cat.get(b"OCProperties").ok().and_then(|o| o.as_reference().ok());
            let mut configs: Vec<ObjectId> = Vec::new();
            let oc = match oc_ref {
                Some(r) => doc.get_dictionary(r).ok().cloned(),
                None => cat
                    .get(b"OCProperties")
                    .ok()
                    .and_then(|o| o.as_dict().ok())
                    .cloned(),
            };
            if let Some(oc) = &oc {
                if let Ok(Object::Reference(r)) = oc.get(b"D") {
                    configs.push(*r);
                }
                if let Ok(Object::Array(a)) = oc.get(b"Configs") {
                    configs.extend(a.iter().filter_map(|c| c.as_reference().ok()));
                }
            }
            let fix = |c: &mut Dictionary, n: usize| {
                c.remove(b"AS");
                if !c.has(b"Name") {
                    c.set("Name", Object::string_literal(format!("Configuration {}", n + 1)));
                }
            };
            for (n, r) in configs.iter().enumerate() {
                if let Ok(c) = doc.get_dictionary_mut(*r) {
                    fix(c, n);
                }
            }
            // A configuration written inside the properties.
            let target = match oc_ref {
                Some(r) => doc.get_dictionary_mut(r).ok(),
                None => match doc.get_dictionary_mut(cat_id)?.get_mut(b"OCProperties") {
                    Ok(Object::Dictionary(d)) => Some(d),
                    _ => None,
                },
            };
            if let Some(oc) = target {
                if let Ok(Object::Dictionary(d)) = oc.get_mut(b"D") {
                    fix(d, 0);
                }
                if let Ok(Object::Array(a)) = oc.get_mut(b"Configs") {
                    for (n, c) in a.iter_mut().enumerate() {
                        if let Object::Dictionary(d) = c {
                            fix(d, n + 1);
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pdf_dates_become_xmp_dates() {
        assert_eq!(
            xmp_date("D:20261026101500+01'00'").as_deref(),
            Some("2026-10-26T10:15:00+01:00")
        );
        assert_eq!(xmp_date("D:20260101").as_deref(), Some("2026-01-01T00:00:00Z"));
        assert_eq!(
            xmp_date("D:20261026101500Z").as_deref(),
            Some("2026-10-26T10:15:00Z")
        );
        assert_eq!(xmp_date("nonsense"), None);
    }

    #[test]
    fn the_srgb_profile_is_well_formed() {
        let p = srgb_icc();
        let u32_at = |i: usize| u32::from_be_bytes(p[i..i + 4].try_into().unwrap()) as usize;
        assert_eq!(u32_at(0), p.len());
        assert_eq!(&p[36..40], b"acsp");
        assert_eq!(&p[12..24], b"mntrRGB XYZ ");
        let count = u32_at(128);
        assert_eq!(count, 9);
        let mut sigs = Vec::new();
        for t in 0..count {
            let at = 132 + 12 * t;
            let (offset, size) = (u32_at(at + 4), u32_at(at + 8));
            assert_eq!(offset % 4, 0);
            assert!(offset + size <= p.len());
            sigs.push(String::from_utf8_lossy(&p[at..at + 4]).to_string());
        }
        for s in [
            "desc", "cprt", "wtpt", "rXYZ", "gXYZ", "bXYZ", "rTRC", "gTRC", "bTRC",
        ] {
            assert!(sigs.iter().any(|x| x == s), "{s}");
        }
    }
}
