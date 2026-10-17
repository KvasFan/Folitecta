//! A small lexer for content streams that keeps the byte ranges of every
//! operation, so streams can be rewritten operation by operation while the
//! operations that are kept stay byte for byte as they were (inline images
//! included).

/// One operation: its operands and operator, as ranges of the stream.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Op<'a> {
    /// Start of the first operand (or of the operator).
    pub start: usize,
    /// End of the operator (after `EI` for inline images).
    pub end: usize,
    pub operator: &'a [u8],
    /// Top-level operands.
    pub operands: Vec<(usize, usize)>,
}

fn is_ws(c: u8) -> bool {
    matches!(c, 0 | 9 | 10 | 12 | 13 | 32)
}

fn is_delim(c: u8) -> bool {
    matches!(
        c,
        b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b'/' | b'%'
    )
}

fn is_operand_word(w: &[u8]) -> bool {
    w == b"true"
        || w == b"false"
        || w == b"null"
        || (!w.is_empty() && w.iter().all(|c| matches!(c, b'0'..=b'9' | b'+' | b'-' | b'.')))
}

/// Split a content stream into operations.
pub(crate) fn operations(data: &[u8]) -> Vec<Op<'_>> {
    let n = data.len();
    let mut ops = Vec::new();
    let mut operands: Vec<(usize, usize)> = Vec::new();
    let mut op_start: Option<usize> = None;
    let mut depth = 0usize;
    let mut nested_start = 0usize;
    let mut i = 0usize;
    while i < n {
        let c = data[i];
        if is_ws(c) {
            i += 1;
            continue;
        }
        if c == b'%' {
            while i < n && data[i] != b'\n' && data[i] != b'\r' {
                i += 1;
            }
            continue;
        }
        let tok = i;
        if op_start.is_none() {
            op_start = Some(tok);
        }
        match c {
            b'(' => {
                let mut level = 0usize;
                while i < n {
                    match data[i] {
                        b'\\' => i += 1,
                        b'(' => level += 1,
                        b')' => {
                            level -= 1;
                            if level == 0 {
                                i += 1;
                                break;
                            }
                        }
                        _ => {}
                    }
                    i += 1;
                }
            }
            b'<' if data.get(i + 1) == Some(&b'<') => {
                i += 2;
                if depth == 0 {
                    nested_start = tok;
                }
                depth += 1;
                continue;
            }
            b'>' if data.get(i + 1) == Some(&b'>') => {
                i += 2;
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    operands.push((nested_start, i));
                }
                continue;
            }
            b'<' => {
                while i < n && data[i] != b'>' {
                    i += 1;
                }
                i = (i + 1).min(n);
            }
            b'[' | b'{' => {
                i += 1;
                if depth == 0 {
                    nested_start = tok;
                }
                depth += 1;
                continue;
            }
            b']' | b'}' => {
                i += 1;
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    operands.push((nested_start, i));
                }
                continue;
            }
            b'/' => {
                i += 1;
                while i < n && !is_ws(data[i]) && !is_delim(data[i]) {
                    i += 1;
                }
            }
            b')' | b'>' => {
                // Stray delimiter: skip it.
                i += 1;
                continue;
            }
            _ => {
                while i < n && !is_ws(data[i]) && !is_delim(data[i]) {
                    i += 1;
                }
                let word = &data[tok..i];
                if depth == 0 && !is_operand_word(word) {
                    if word == b"BI" {
                        i = inline_image_end(data, i);
                    }
                    ops.push(Op {
                        start: op_start.take().unwrap_or(tok),
                        end: i,
                        operator: word,
                        operands: std::mem::take(&mut operands),
                    });
                    continue;
                }
            }
        }
        if depth == 0 {
            operands.push((tok, i));
        }
    }
    ops
}

/// End of an inline image that starts after `BI` at `from` (after its `EI`).
fn inline_image_end(data: &[u8], from: usize) -> usize {
    let n = data.len();
    // The image dictionary ends with the ID operator.
    let mut i = from;
    let mut id_end = None;
    while i + 2 <= n {
        if data[i] == b'I'
            && data[i + 1] == b'D'
            && i > 0
            && is_ws(data[i - 1])
            && (i + 2 == n || is_ws(data[i + 2]))
        {
            id_end = Some(i + 2);
            break;
        }
        i += 1;
    }
    let Some(start) = id_end else {
        return n;
    };
    let mut j = start + 1;
    while j + 2 <= n {
        if data[j] == b'E'
            && data[j + 1] == b'I'
            && is_ws(data[j - 1])
            && (j + 2 == n || is_ws(data[j + 2]) || is_delim(data[j + 2]))
        {
            return j + 2;
        }
        j += 1;
    }
    n
}

/// A name operand without its slash and with `#xx` escapes decoded.
pub(crate) fn name_of(data: &[u8], range: (usize, usize)) -> Option<Vec<u8>> {
    let raw = data.get(range.0..range.1)?;
    let raw = raw.strip_prefix(b"/")?;
    let mut out = Vec::with_capacity(raw.len());
    let mut i = 0;
    while i < raw.len() {
        if raw[i] == b'#' {
            if let Some(v) = raw
                .get(i + 1..i + 3)
                .and_then(|h| std::str::from_utf8(h).ok())
                .and_then(|h| u8::from_str_radix(h, 16).ok())
            {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(raw[i]);
        i += 1;
    }
    Some(out)
}

/// What happens to a marked-content sequence or an XObject of a layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Treat {
    /// Left as it is.
    Leave,
    /// The content stays and stops being optional.
    Keep,
    /// The content goes away.
    Remove,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Frame {
    /// A marker copied to the output.
    Copy,
    /// A layer marker dropped, its content kept.
    Strip,
    /// Content being removed.
    Skip,
}

/// Rewriting state carried from one content stream of a page to the next.
#[derive(Debug, Default)]
pub(crate) struct Rewriter {
    stack: Vec<Frame>,
}

/// Decisions of a rewrite.
pub(crate) trait Decide {
    /// Treatment of `BDC` sequences tagged `/OC` with the property `name`.
    fn marked(&mut self, name: &[u8]) -> Treat;
    /// Treatment of the XObject `name` drawn with `Do` (its own `/OC` entry);
    /// `visible_context` tells whether it is drawn inside kept layer content.
    fn xobject(&mut self, name: &[u8], visible_context: bool) -> Treat;
    /// Whether painting outside kept layer content is removed.
    fn drop_unlayered(&self) -> bool {
        false
    }
}

const PAINT_PATH: &[&[u8]] = &[b"S", b"s", b"f", b"F", b"f*", b"B", b"B*", b"b", b"b*"];

impl Rewriter {
    pub fn new() -> Self {
        Self::default()
    }

    /// A rewriter for a form drawn inside kept layer content (or not).
    pub fn inside_kept_layer(kept: bool) -> Self {
        Self {
            stack: if kept { vec![Frame::Strip] } else { Vec::new() },
        }
    }

    /// Inside kept (stripped) layer content.
    fn in_kept_layer(&self) -> bool {
        self.stack.contains(&Frame::Strip)
    }

    /// Rewrite one stream; `None` when nothing changed.
    pub fn rewrite(&mut self, data: &[u8], decide: &mut dyn Decide) -> Option<Vec<u8>> {
        let ops = operations(data);
        let mut out = Vec::with_capacity(data.len());
        let mut changed = false;
        let drop_unlayered = decide.drop_unlayered();
        for op in &ops {
            let raw = &data[op.start..op.end];
            let skipping = self.stack.contains(&Frame::Skip);
            match op.operator {
                b"BDC" | b"BMC" => {
                    if skipping {
                        self.stack.push(Frame::Skip);
                        changed = true;
                        continue;
                    }
                    let oc_name = (op.operator == b"BDC"
                        && op.operands.len() == 2
                        && name_of(data, op.operands[0]).as_deref() == Some(b"OC"))
                    .then(|| name_of(data, op.operands[1]))
                    .flatten();
                    let frame = match oc_name.map(|n| decide.marked(&n)) {
                        Some(Treat::Keep) => Frame::Strip,
                        Some(Treat::Remove) => Frame::Skip,
                        _ => Frame::Copy,
                    };
                    self.stack.push(frame);
                    if frame == Frame::Copy {
                        out.extend_from_slice(raw);
                        out.push(b'\n');
                    } else {
                        changed = true;
                    }
                    continue;
                }
                b"EMC" => {
                    match self.stack.pop() {
                        Some(Frame::Copy) | None => {
                            out.extend_from_slice(raw);
                            out.push(b'\n');
                        }
                        _ => changed = true,
                    }
                    continue;
                }
                _ => {}
            }
            if skipping {
                changed = true;
                continue;
            }
            let unlayered = drop_unlayered && !self.in_kept_layer();
            if op.operator == b"Do" {
                let name = op.operands.first().and_then(|r| name_of(data, *r));
                let treat = name
                    .map(|n| decide.xobject(&n, self.in_kept_layer()))
                    .unwrap_or(Treat::Leave);
                let drop = match treat {
                    Treat::Remove => true,
                    Treat::Keep => false,
                    Treat::Leave => unlayered,
                };
                if drop {
                    changed = true;
                    continue;
                }
            } else if unlayered {
                let replacement: Option<Vec<u8>> = match op.operator {
                    o if PAINT_PATH.contains(&o) => Some(b"n".to_vec()),
                    b"Tj" | b"TJ" | b"sh" | b"BI" => Some(Vec::new()),
                    b"'" => Some(b"T*".to_vec()),
                    b"\"" if op.operands.len() == 3 => {
                        let (aw, ac) = (op.operands[0], op.operands[1]);
                        let mut r = data[aw.0..aw.1].to_vec();
                        r.extend_from_slice(b" Tw ");
                        r.extend_from_slice(&data[ac.0..ac.1]);
                        r.extend_from_slice(b" Tc T*");
                        Some(r)
                    }
                    _ => None,
                };
                if let Some(r) = replacement {
                    changed = true;
                    if !r.is_empty() {
                        out.extend_from_slice(&r);
                        out.push(b'\n');
                    }
                    continue;
                }
            }
            out.extend_from_slice(raw);
            out.push(b'\n');
        }
        changed.then_some(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Hide;
    impl Decide for Hide {
        fn marked(&mut self, name: &[u8]) -> Treat {
            match name {
                b"L1" => Treat::Remove,
                b"L2" => Treat::Keep,
                _ => Treat::Leave,
            }
        }
        fn xobject(&mut self, name: &[u8], _: bool) -> Treat {
            if name == b"Im1" {
                Treat::Remove
            } else {
                Treat::Leave
            }
        }
    }

    #[test]
    fn operations_keep_their_bytes() {
        let data = b"q 1 0 0 1 (a\\)b(c)) Tj [(x) 2 (y)] TJ /P <</MCID 3>> BDC EMC\n%c\nBI /W 1 /H 1 /CS /G /BPC 8 /F /AHx ID ff> EI Q";
        let ops = operations(data);
        let names: Vec<&[u8]> = ops.iter().map(|o| o.operator).collect();
        assert_eq!(names, vec![&b"q"[..], b"Tj", b"TJ", b"BDC", b"EMC", b"BI", b"Q"]);
        assert_eq!(&data[ops[1].start..ops[1].end], b"1 0 0 1 (a\\)b(c)) Tj");
        assert_eq!(ops[1].operands.len(), 5);
        assert_eq!(ops[3].operands.len(), 2);
        assert!(data[ops[5].start..ops[5].end].ends_with(b"ff> EI"));
        assert_eq!(name_of(b"/A#20B", (0, 6)).unwrap(), b"A B");
    }

    #[test]
    fn layers_are_removed_or_kept() {
        let data = b"/OC /L1 BDC 0 0 m 1 1 l S /Tag BMC (x) Tj EMC EMC /OC /L2 BDC (kept) Tj EMC /Im1 Do /Im2 Do /OC /L3 BDC (left) Tj EMC";
        let out = Rewriter::new().rewrite(data, &mut Hide).unwrap();
        let s = String::from_utf8(out).unwrap();
        assert_eq!(s, "(kept) Tj\n/Im2 Do\n/OC /L3 BDC\n(left) Tj\nEMC\n");
    }

    #[test]
    fn unlayered_painting_is_neutralized() {
        struct Only;
        impl Decide for Only {
            fn marked(&mut self, _: &[u8]) -> Treat {
                Treat::Keep
            }
            fn xobject(&mut self, _: &[u8], _: bool) -> Treat {
                Treat::Leave
            }
            fn drop_unlayered(&self) -> bool {
                true
            }
        }
        let data = b"0 0 m 5 5 l f (a) Tj 1 2 (b) \" /X Do /OC /L BDC 1 1 m 2 2 l S /Y Do EMC";
        let out = Rewriter::new().rewrite(data, &mut Only).unwrap();
        assert_eq!(
            String::from_utf8(out).unwrap(),
            "0 0 m\n5 5 l\nn\n1 Tw 2 Tc T*\n1 1 m\n2 2 l\nS\n/Y Do\n"
        );
    }
}
