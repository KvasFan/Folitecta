//! Changes to the content streams of a page: the streams read as one piece
//! of data, ranges of it replaced, and each changed stream written back
//! (a stream shared with other pages is copied for this page first).

use lopdf::{Object, ObjectId, Stream};

use crate::error::{Error, Result};

/// The content streams of a page joined, with where each starts.
pub(crate) struct Joined {
    pub(crate) ids: Vec<ObjectId>,
    pub(crate) starts: Vec<usize>,
    pub(crate) data: Vec<u8>,
}

/// Read the content streams of a page (decoded), joined by newlines.
pub(crate) fn joined(doc: &lopdf::Document, page_id: ObjectId) -> Joined {
    let mut j = Joined {
        ids: Vec::new(),
        starts: Vec::new(),
        data: Vec::new(),
    };
    for id in doc.get_page_contents(page_id) {
        j.ids.push(id);
        j.starts.push(j.data.len());
        if let Ok(s) = doc.get_object(id).and_then(Object::as_stream) {
            j.data
                .extend(s.decompressed_content().unwrap_or_else(|_| s.content.clone()));
        }
        j.data.push(b'\n');
    }
    j
}

/// A change of the joined content: bytes `start..end` become `with`.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Splice {
    pub(crate) start: usize,
    pub(crate) end: usize,
    pub(crate) with: Vec<u8>,
}

/// How many times an object is referred to.
pub(crate) fn count_refs(doc: &lopdf::Document, target: ObjectId) -> usize {
    fn walk(o: &Object, t: ObjectId) -> usize {
        match o {
            Object::Reference(r) => usize::from(*r == t),
            Object::Array(a) => a.iter().map(|x| walk(x, t)).sum(),
            Object::Dictionary(d) => d.iter().map(|(_, x)| walk(x, t)).sum(),
            Object::Stream(s) => s.dict.iter().map(|(_, x)| walk(x, t)).sum(),
            _ => 0,
        }
    }
    doc.objects.values().map(|o| walk(o, target)).sum::<usize>()
        + doc.trailer.iter().map(|(_, x)| walk(x, target)).sum::<usize>()
}

/// Point a page at `new` instead of the content stream `old`.
fn replace_content_ref(
    doc: &mut lopdf::Document,
    page_id: ObjectId,
    old: ObjectId,
    new: ObjectId,
) -> Result<()> {
    let contents = doc.get_dictionary(page_id)?.get(b"Contents")?.clone();
    let swap = |arr: Vec<Object>| -> Vec<Object> {
        arr.into_iter()
            .map(|o| match o {
                Object::Reference(r) if r == old => Object::Reference(new),
                o => o,
            })
            .collect()
    };
    let replaced = match contents {
        Object::Reference(r) if r == old => Object::Reference(new),
        Object::Reference(r) => Object::Array(swap(doc.get_object(r)?.as_array()?.clone())),
        Object::Array(a) => Object::Array(swap(a)),
        other => other,
    };
    doc.get_dictionary_mut(page_id)?.set("Contents", replaced);
    Ok(())
}

/// The content stream holding bytes `start..end` of the joined content:
/// its position and where it starts and ends.
pub(crate) fn stream_span(j: &Joined, start: usize, end: usize) -> Result<(usize, usize, usize)> {
    let k = j.starts.partition_point(|s| *s <= start).saturating_sub(1);
    if k >= j.ids.len() {
        return Err(Error::Invalid("the page has no content".into()));
    }
    let base = j.starts[k];
    // Each stream is followed by the newline that joins it to the next.
    let stream_end = j.starts.get(k + 1).copied().unwrap_or(j.data.len()) - 1;
    if end > stream_end {
        return Err(Error::Invalid("the change crosses two content streams".into()));
    }
    Ok((k, base, stream_end))
}

/// Check that every change stays inside one content stream.
pub(crate) fn check_splices(j: &Joined, splices: &[Splice]) -> Result<()> {
    for s in splices {
        stream_span(j, s.start, s.end)?;
    }
    Ok(())
}

/// Apply changes to the joined content of a page, stream by stream.
pub(crate) fn rewrite(
    doc: &mut lopdf::Document,
    page_id: ObjectId,
    j: &Joined,
    splices: &[Splice],
) -> Result<()> {
    let mut by_stream: std::collections::BTreeMap<usize, Vec<&Splice>> = Default::default();
    for s in splices {
        let (k, _, _) = stream_span(j, s.start, s.end)?;
        by_stream.entry(k).or_default().push(s);
    }
    // Streams reached through an array other pages share are shared too.
    let array_shared = match doc
        .get_dictionary(page_id)
        .ok()
        .and_then(|d| d.get(b"Contents").ok())
    {
        Some(Object::Reference(r)) => {
            doc.get_object(*r).map(|o| o.as_array().is_ok()).unwrap_or(false) && count_refs(doc, *r) > 1
        }
        _ => false,
    };
    for (k, mut list) in by_stream {
        let (_, base, stream_end) = stream_span(j, j.starts[k], j.starts[k])?;
        let sid = j.ids[k];
        let mut data = j.data[base..stream_end].to_vec();
        // From the end, so earlier ranges keep their offsets.
        list.sort_by_key(|s| std::cmp::Reverse(s.start));
        for s in list {
            data.splice(s.start - base..s.end - base, s.with.iter().copied());
        }
        let mut dict = doc.get_object(sid)?.as_stream()?.dict.clone();
        for key in [b"Filter".as_slice(), b"DecodeParms", b"Length"] {
            dict.remove(key);
        }
        let mut s = Stream::new(dict, data);
        let _ = s.compress();
        if array_shared || count_refs(doc, sid) > 1 {
            // A stream shared with other pages: this page gets a copy of its own.
            let nid = doc.add_object(s);
            replace_content_ref(doc, page_id, sid, nid)?;
        } else {
            doc.objects.insert(sid, Object::Stream(s));
        }
    }
    Ok(())
}

/// A name written for a content stream (`/Name` with `#xx` escapes).
pub(crate) fn name_token(name: &[u8]) -> String {
    let mut out = String::from("/");
    for &b in name {
        let regular = b.is_ascii_graphic()
            && !matches!(
                b,
                b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b'/' | b'%' | b'#'
            );
        if regular {
            out.push(b as char);
        } else {
            out.push_str(&format!("#{b:02X}"));
        }
    }
    out
}

/// Bytes written as a hexadecimal string.
pub(crate) fn hex_string(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2 + 2);
    out.push('<');
    for b in bytes {
        out.push_str(&format!("{b:02X}"));
    }
    out.push('>');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens() {
        assert_eq!(name_token(b"F1"), "/F1");
        assert_eq!(name_token(b"A B#"), "/A#20B#23");
        assert_eq!(hex_string(&[0, 0x41, 0xff]), "<0041FF>");
    }
}
