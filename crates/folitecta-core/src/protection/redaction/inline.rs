//! Inline pictures (`BI … ID … EI`) read into picture objects, so the ones
//! under redaction marks can be covered like any other picture.

use lopdf::{Dictionary, Object, Stream};

use super::scan::resource;
use crate::util::deref;

/// The full key of an abbreviated inline picture key (ISO 32000-1 Table 93).
fn full_key(k: &[u8]) -> &[u8] {
    match k {
        b"BPC" => b"BitsPerComponent",
        b"CS" => b"ColorSpace",
        b"D" => b"Decode",
        b"DP" => b"DecodeParms",
        b"F" => b"Filter",
        b"H" => b"Height",
        b"IM" => b"ImageMask",
        b"I" => b"Interpolate",
        b"W" => b"Width",
        b"L" => b"Length",
        other => other,
    }
}

/// The full name of an abbreviated colour space or filter (Table 94).
fn full_name(n: &[u8]) -> &[u8] {
    match n {
        b"G" => b"DeviceGray",
        b"RGB" => b"DeviceRGB",
        b"CMYK" => b"DeviceCMYK",
        b"I" => b"Indexed",
        b"AHx" => b"ASCIIHexDecode",
        b"A85" => b"ASCII85Decode",
        b"LZW" => b"LZWDecode",
        b"Fl" => b"FlateDecode",
        b"RL" => b"RunLengthDecode",
        b"CCF" => b"CCITTFaxDecode",
        b"DCT" => b"DCTDecode",
        other => other,
    }
}

fn expand(o: Object) -> Object {
    match o {
        Object::Name(n) => Object::Name(full_name(&n).to_vec()),
        Object::Array(a) => Object::Array(a.into_iter().map(expand).collect()),
        Object::Dictionary(d) => {
            let mut out = Dictionary::new();
            for (k, v) in d.into_iter() {
                out.set(full_key(&k).to_vec(), expand(v));
            }
            Object::Dictionary(out)
        }
        o => o,
    }
}

fn is_ws(c: u8) -> bool {
    matches!(c, 0 | 9 | 10 | 12 | 13 | 32)
}

/// The dictionary and data of an inline picture operation (`BI` … `EI`), as
/// a picture object; colour spaces named in `res` are resolved. None when
/// it cannot be read.
pub(crate) fn inline_picture(doc: &lopdf::Document, op: &[u8], res: &Dictionary) -> Option<Stream> {
    let body = op.strip_prefix(b"BI")?;
    // The dictionary ends with the ID operator, on its own.
    let mut id_at = None;
    let mut i = 0;
    while i + 2 <= body.len() {
        if &body[i..i + 2] == b"ID"
            && (i == 0 || is_ws(body[i - 1]))
            && body.get(i + 2).is_none_or(|c| is_ws(*c))
        {
            id_at = Some(i);
            break;
        }
        i += 1;
    }
    let id_at = id_at?;
    let decoded = lopdf::content::Content::decode(
        format!("<< {} >> FoliInline", String::from_utf8_lossy(&body[..id_at])).as_bytes(),
    )
    .ok()?;
    let dict = match decoded.operations.first()?.operands.first()? {
        Object::Dictionary(d) => d.clone(),
        _ => return None,
    };
    let Object::Dictionary(mut dict) = expand(Object::Dictionary(dict)) else {
        return None;
    };
    // The data: after one white-space byte following ID, up to the white
    // space before EI (or as long as /L says).
    let data_start = id_at + 3;
    let tail = body.strip_suffix(b"EI")?;
    let mut data_end = tail.len();
    if data_end > data_start && is_ws(tail[data_end - 1]) {
        data_end -= 1;
    }
    if let Some(len) = dict.get(b"Length").ok().and_then(|l| l.as_i64().ok()) {
        let len = usize::try_from(len).ok()?;
        if data_start + len <= tail.len() {
            data_end = data_start + len;
        }
    }
    if data_end < data_start {
        return None;
    }
    let data = body.get(data_start..data_end)?.to_vec();
    dict.remove(b"Length");
    // Colour spaces of the resources.
    if let Ok(Object::Name(n)) = dict.get(b"ColorSpace") {
        let n = n.clone();
        if !matches!(
            n.as_slice(),
            b"DeviceGray" | b"DeviceRGB" | b"DeviceCMYK" | b"Indexed" | b"Pattern"
        ) {
            let cs = resource(doc, res, b"ColorSpace", &n)?.clone();
            dict.set("ColorSpace", cs);
        }
    }
    if let Ok(Object::Array(a)) = dict.get(b"ColorSpace") {
        // An indexed space naming its base in the resources.
        let mut a = a.clone();
        if let Some(Object::Name(base)) = a.get(1).cloned() {
            if !matches!(base.as_slice(), b"DeviceGray" | b"DeviceRGB" | b"DeviceCMYK") {
                if let Some(cs) = resource(doc, res, b"ColorSpace", &base) {
                    a[1] = deref(doc, cs).clone();
                }
            }
        }
        dict.set("ColorSpace", Object::Array(a));
    }
    let w = dict.get(b"Width").ok()?.as_i64().ok()?;
    let h = dict.get(b"Height").ok()?.as_i64().ok()?;
    if w <= 0 || h <= 0 {
        return None;
    }
    // Unencoded data must be whole: a shorter one means EI was found inside it.
    if !dict.has(b"Filter") {
        let mask = dict.get(b"ImageMask").ok().and_then(|v| v.as_bool().ok()) == Some(true);
        let bpc = if mask {
            1
        } else {
            dict.get(b"BitsPerComponent")
                .ok()
                .and_then(|v| v.as_i64().ok())
                .unwrap_or(8)
        };
        let comps = match dict.get(b"ColorSpace").ok() {
            _ if mask => 1,
            Some(Object::Name(n)) if n == b"DeviceRGB" => 3,
            Some(Object::Name(n)) if n == b"DeviceCMYK" => 4,
            Some(Object::Array(a)) if a.first().and_then(|x| x.as_name().ok()) == Some(b"ICCBased") => a
                .get(1)
                .map(|s| deref(doc, s))
                .and_then(|s| s.as_stream().ok())
                .and_then(|s| s.dict.get(b"N").ok()?.as_i64().ok())
                .unwrap_or(3),
            _ => 1,
        };
        let need = ((w * comps * bpc) as usize).div_ceil(8) * h as usize;
        if data.len() < need {
            return None;
        }
    }
    dict.set("Type", "XObject");
    dict.set("Subtype", "Image");
    let mut s = Stream::new(dict, data);
    s.allows_compression = false;
    Some(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn abbreviations_are_expanded() {
        let doc = lopdf::Document::with_version("1.7");
        let s = inline_picture(
            &doc,
            b"BI /W 2 /H 2 /CS /G /BPC 8 /F /AHx ID FFFF0000> EI",
            &Dictionary::new(),
        )
        .unwrap();
        assert_eq!(
            s.dict.get(b"ColorSpace").unwrap().as_name().unwrap(),
            b"DeviceGray"
        );
        assert_eq!(
            s.dict.get(b"Filter").unwrap().as_name().unwrap(),
            b"ASCIIHexDecode"
        );
        assert_eq!(s.dict.get(b"Width").unwrap().as_i64().unwrap(), 2);
        // The data leaves out the white space around it.
        assert_eq!(s.content, b"FFFF0000>");
        let s = inline_picture(
            &doc,
            b"BI /W 1 /H 1 /CS [/I /RGB 1 <FF000000FF00>] /BPC 8 ID \x01 EI",
            &Dictionary::new(),
        )
        .unwrap();
        let cs = s.dict.get(b"ColorSpace").unwrap().as_array().unwrap();
        assert_eq!(cs[0].as_name().unwrap(), b"Indexed");
        assert_eq!(cs[1].as_name().unwrap(), b"DeviceRGB");
        assert_eq!(s.content, vec![1]);
    }

    #[test]
    fn short_unencoded_data_is_refused() {
        let doc = lopdf::Document::with_version("1.7");
        // 4 bytes needed, 2 found: EI was inside the data.
        assert!(inline_picture(&doc, b"BI /W 2 /H 2 /CS /G /BPC 8 ID ab EI", &Dictionary::new()).is_none());
        // Stencil masks are read too.
        let s = inline_picture(&doc, b"BI /W 8 /H 1 /IM true ID \x0f EI", &Dictionary::new()).unwrap();
        assert!(s.dict.get(b"ImageMask").unwrap().as_bool().unwrap());
    }
}
