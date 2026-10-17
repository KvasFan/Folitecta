//! Content streams: units of drawing outside tagged content, marked content wrapped around them or rewritten, and pages written back stream by stream.

use super::*;

// ----------------------------------------------------------------------
// Content streams
// ----------------------------------------------------------------------

/// The MCID of a `BDC` with an inline property list.
pub(super) fn mcid_of(data: &[u8], op: &Op<'_>) -> Option<i64> {
    if op.operator != b"BDC" || op.operands.len() != 2 {
        return None;
    }
    let (a, b) = op.operands[1];
    let props = data.get(a..b)?;
    let pos = props.windows(5).position(|w| w == b"/MCID")?;
    let rest = &props[pos + 5..];
    let digits: Vec<u8> = rest
        .iter()
        .skip_while(|c| c.is_ascii_whitespace())
        .take_while(|c| c.is_ascii_digit())
        .copied()
        .collect();
    std::str::from_utf8(&digits).ok()?.parse().ok()
}

pub(super) fn is_artifact(data: &[u8], op: &Op<'_>) -> bool {
    matches!(op.operator, b"BDC" | b"BMC")
        && op.operands.first().and_then(|r| name_of(data, *r)).as_deref() == Some(b"Artifact")
}

/// A name written for a content stream or a dictionary.
pub(super) fn pdf_name(tag: &str) -> String {
    let mut out = String::from("/");
    for b in tag.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.') {
            out.push(b as char);
        } else {
            out.push_str(&format!("#{b:02X}"));
        }
    }
    out
}

pub(super) const PAINT: &[&[u8]] = &[b"S", b"s", b"f", b"F", b"f*", b"B", b"B*", b"b", b"b*"];
pub(super) const PATH_BUILD: &[&[u8]] = &[b"m", b"l", b"c", b"v", b"y", b"h", b"re"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum UnitKind {
    Text,
    XObject,
    Image,
    Path,
    Shading,
}

/// Operations that draw something outside tagged content and artifacts: one
/// text showing operation, an XObject, an inline image, a painted path.
#[derive(Debug, Clone, Copy)]
pub(super) struct Unit {
    pub(super) first: usize,
    pub(super) last: usize,
    pub(super) kind: UnitKind,
    /// Text units of one segment can share a marked content sequence.
    pub(super) segment: usize,
    /// Position among the units.
    pub(super) seq: usize,
    /// Where the first operation starts in the joined content.
    pub(super) offset: usize,
}

pub(super) fn content_units(data: &[u8], ops: &[Op<'_>]) -> Vec<Unit> {
    let mut out = Vec::new();
    let mut stack: Vec<bool> = Vec::new();
    let mut segment = 0usize;
    let mut in_text = false;
    let mut path_start: Option<usize> = None;
    for (i, op) in ops.iter().enumerate() {
        let covered = stack.iter().any(|c| *c);
        let mut unit = |kind, first: usize| {
            let seq = out.len();
            out.push(Unit {
                first,
                last: i,
                kind,
                segment,
                seq,
                offset: ops[first].start,
            })
        };
        match op.operator {
            b"BDC" | b"BMC" => {
                stack.push(mcid_of(data, op).is_some() || is_artifact(data, op));
                segment += 1;
            }
            b"EMC" => {
                stack.pop();
                segment += 1;
            }
            b"BT" => {
                in_text = true;
                segment += 1;
            }
            b"ET" => {
                in_text = false;
                segment += 1;
            }
            b"Tj" | b"TJ" | b"'" | b"\"" if in_text => {
                if !covered {
                    unit(UnitKind::Text, i);
                }
            }
            _ if in_text => {}
            o if PATH_BUILD.contains(&o) => {
                if path_start.is_none() {
                    path_start = Some(i);
                }
            }
            o if PAINT.contains(&o) => {
                if let Some(s) = path_start.take() {
                    if !covered {
                        unit(UnitKind::Path, s);
                    }
                }
                segment += 1;
            }
            b"n" => {
                path_start = None;
            }
            b"Do" | b"BI" | b"sh" => {
                if !covered {
                    let kind = match op.operator {
                        b"Do" => UnitKind::XObject,
                        b"BI" => UnitKind::Image,
                        _ => UnitKind::Shading,
                    };
                    unit(kind, i);
                }
                segment += 1;
            }
            _ => {}
        }
    }
    out
}

/// What wraps a run of operations.
#[derive(Debug, Clone)]
pub(super) enum Wrap {
    Mcid(String, i64),
    Artifact,
}

/// Output of a rewrite: bytes that go where an operation of the source was
/// (the start of the operation in the joined content), and whether they
/// differ from the source.
pub(super) type Piece = (usize, Vec<u8>, bool);

#[cfg(test)]
pub(super) fn join(pieces: &[Piece]) -> Vec<u8> {
    pieces.iter().flat_map(|p| p.1.iter().copied()).collect()
}

/// Copy the operations with marked content sequences around some runs
/// (`(first op, last op, wrap)`, in order and without overlaps).
pub(super) fn write_wrapped(data: &[u8], ops: &[Op<'_>], wraps: &[(usize, usize, Wrap)]) -> Vec<Piece> {
    let opens: HashMap<usize, &Wrap> = wraps.iter().map(|(f, _, w)| (*f, w)).collect();
    let closes: HashMap<usize, usize> = wraps.iter().fold(HashMap::new(), |mut m, (_, l, _)| {
        *m.entry(*l).or_insert(0) += 1;
        m
    });
    let mut out = Vec::with_capacity(ops.len() + wraps.len() * 2);
    for (i, op) in ops.iter().enumerate() {
        if let Some(w) = opens.get(&i) {
            let open = match w {
                Wrap::Mcid(tag, n) => format!("{} <</MCID {n}>> BDC\n", pdf_name(tag)).into_bytes(),
                Wrap::Artifact => b"/Artifact BMC\n".to_vec(),
            };
            out.push((op.start, open, true));
        }
        let mut raw = data[op.start..op.end].to_vec();
        raw.push(b'\n');
        out.push((op.start, raw, false));
        for _ in 0..closes.get(&i).copied().unwrap_or(0) {
            out.push((op.start, b"EMC\n".to_vec(), true));
        }
    }
    out
}

/// What happens to a marked content sequence with an MCID.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum McChange {
    Keep,
    /// Becomes an artifact.
    Artifact,
    /// The marker goes away, the content stays.
    Strip,
}

/// Rewrite the markers of marked content with an MCID; None when nothing changed.
pub(super) fn rewrite_mcids(data: &[u8], f: &dyn Fn(i64) -> McChange) -> Option<Vec<Piece>> {
    let ops = operations(data);
    let mut out = Vec::with_capacity(ops.len());
    let mut stack: Vec<bool> = Vec::new();
    let mut changed = false;
    for op in &ops {
        let mut raw = data[op.start..op.end].to_vec();
        raw.push(b'\n');
        match op.operator {
            b"BDC" | b"BMC" => match mcid_of(data, op).map(f) {
                Some(McChange::Artifact) => {
                    out.push((op.start, b"/Artifact BMC\n".to_vec(), true));
                    stack.push(false);
                    changed = true;
                }
                Some(McChange::Strip) => {
                    out.push((op.start, Vec::new(), true));
                    stack.push(true);
                    changed = true;
                }
                _ => {
                    out.push((op.start, raw, false));
                    stack.push(false);
                }
            },
            b"EMC" => {
                if stack.pop() == Some(true) {
                    out.push((op.start, Vec::new(), true));
                } else {
                    out.push((op.start, raw, false));
                }
            }
            _ => out.push((op.start, raw, false)),
        }
    }
    changed.then_some(out)
}

pub(super) fn max_mcid(data: &[u8]) -> Option<i64> {
    operations(data).iter().filter_map(|op| mcid_of(data, op)).max()
}

/// The content streams of a page, decoded and joined, with where each starts.
#[derive(Debug, Clone, Default)]
pub(super) struct PageStreams {
    pub(super) ids: Vec<ObjectId>,
    pub(super) starts: Vec<usize>,
    /// Streams of running headers, footers, page numbers and watermarks.
    pub(super) pagination: Vec<bool>,
    pub(super) data: Vec<u8>,
}

impl PageStreams {
    pub(super) fn read(doc: &lopdf::Document, page_id: ObjectId) -> Self {
        let mut ps = PageStreams::default();
        for id in doc.get_page_contents(page_id) {
            ps.ids.push(id);
            ps.starts.push(ps.data.len());
            match doc.get_object(id).and_then(Object::as_stream) {
                Ok(s) => {
                    ps.data
                        .extend(s.decompressed_content().unwrap_or_else(|_| s.content.clone()));
                    let mark = s.dict.get(b"FolitectaMark").ok().and_then(|m| m.as_name().ok());
                    ps.pagination.push(matches!(
                        mark,
                        Some(b"Watermark" | b"HeaderFooter" | b"PageNumber")
                    ));
                }
                Err(_) => ps.pagination.push(false),
            }
            ps.data.push(b'\n');
        }
        ps
    }

    pub(super) fn stream_of(&self, offset: usize) -> usize {
        self.starts.partition_point(|s| *s <= offset).saturating_sub(1)
    }

    pub(super) fn is_pagination(&self, offset: usize) -> bool {
        self.pagination
            .get(self.stream_of(offset))
            .copied()
            .unwrap_or(false)
    }
}

/// Write the pieces of a rewrite into the streams they come from; streams
/// without a change stay as they are (marks added by Folitecta keep their
/// own streams).
pub(super) fn write_pieces(
    doc: &mut lopdf::Document,
    page_id: ObjectId,
    ps: &PageStreams,
    pieces: Vec<Piece>,
) -> Result<()> {
    let n = ps.ids.len();
    if n == 0 {
        let data: Vec<u8> = pieces.into_iter().flat_map(|p| p.1).collect();
        return set_page_content(doc, page_id, data);
    }
    let mut outs = vec![Vec::new(); n];
    let mut changed = vec![false; n];
    for (off, bytes, ch) in pieces {
        let k = ps.stream_of(off).min(n - 1);
        outs[k].extend_from_slice(&bytes);
        changed[k] |= ch;
    }
    for k in 0..n {
        if changed[k] {
            let id = ps.ids[k];
            let mut dict = doc.get_object(id)?.as_stream()?.dict.clone();
            for key in [b"Filter".as_slice(), b"DecodeParms", b"Length"] {
                dict.remove(key);
            }
            let mut s = Stream::new(dict, std::mem::take(&mut outs[k]));
            let _ = s.compress();
            doc.objects.insert(id, Object::Stream(s));
        }
    }
    Ok(())
}

/// Replace the content of a page with one new stream.
pub(super) fn set_page_content(doc: &mut lopdf::Document, page_id: ObjectId, data: Vec<u8>) -> Result<()> {
    let mut stream = Stream::new(Dictionary::new(), data);
    let _ = stream.compress();
    let sid = doc.add_object(stream);
    doc.get_dictionary_mut(page_id)?
        .set("Contents", Object::Reference(sid));
    Ok(())
}

/// A unit measured through a probe.
#[derive(Debug, Clone)]
pub(super) struct Probed {
    pub(super) unit: Unit,
    pub(super) item: Option<MarkedItem>,
}
