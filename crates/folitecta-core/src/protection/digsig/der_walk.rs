//! DER elements read from the bytes as they were received (signed parts are
//! verified over those exact bytes, never over a re-encoding) and written for
//! the small local authorities the tests and the self-test use.

/// One DER element at the start of some bytes.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Element<'a> {
    pub(crate) tag: u8,
    /// The element with its tag and length.
    pub(crate) whole: &'a [u8],
    pub(crate) value: &'a [u8],
    /// The bytes after the element.
    pub(crate) rest: &'a [u8],
}

/// The element at the start of `data` (single-byte tags, definite lengths of
/// at most four bytes).
pub(crate) fn element(data: &[u8]) -> Option<Element<'_>> {
    let tag = *data.first()?;
    if tag & 0x1F == 0x1F {
        return None;
    }
    let first = *data.get(1)?;
    let (len, header) = if first < 0x80 {
        (first as usize, 2)
    } else {
        let n = (first & 0x7F) as usize;
        if n == 0 || n > 4 {
            return None;
        }
        let bytes = data.get(2..2 + n)?;
        (bytes.iter().fold(0usize, |a, b| (a << 8) | *b as usize), 2 + n)
    };
    let end = header.checked_add(len)?;
    Some(Element {
        tag,
        whole: data.get(..end)?,
        value: data.get(header..end)?,
        rest: &data[end..],
    })
}

/// The element at the start of `data` when its tag is `tag`.
pub(crate) fn expect(data: &[u8], tag: u8) -> Option<Element<'_>> {
    element(data).filter(|e| e.tag == tag)
}

/// Every element of `data` one after the other; `None` when the bytes do not
/// end with a whole element.
pub(crate) fn elements(mut data: &[u8]) -> Option<Vec<Element<'_>>> {
    let mut out = Vec::new();
    while !data.is_empty() {
        let e = element(data)?;
        data = e.rest;
        out.push(e);
    }
    Some(out)
}

fn unix_of(year: i32, digits: &str) -> Option<i64> {
    let n = |r: std::ops::Range<usize>| digits.get(r)?.parse::<u32>().ok();
    let month = time::Month::try_from(n(0..2)? as u8).ok()?;
    let date = time::Date::from_calendar_date(year, month, n(2..4)? as u8).ok()?;
    let clock = time::Time::from_hms(n(4..6)? as u8, n(6..8)? as u8, n(8..10)? as u8).ok()?;
    Some(
        time::PrimitiveDateTime::new(date, clock)
            .assume_utc()
            .unix_timestamp(),
    )
}

/// Unix time of a GeneralizedTime value `YYYYMMDDHHMMSS[.fff]Z` (the
/// fraction of a second is allowed and dropped).
pub(crate) fn generalized_time(value: &[u8]) -> Option<i64> {
    let text = std::str::from_utf8(value).ok()?;
    let text = text.strip_suffix('Z')?;
    let whole = text.split('.').next()?;
    if whole.len() != 14 || !whole.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    unix_of(whole[..4].parse().ok()?, &whole[4..])
}

/// Append a DER length.
pub(crate) fn push_len(out: &mut Vec<u8>, len: usize) {
    if len < 0x80 {
        out.push(len as u8);
    } else {
        let bytes: Vec<u8> = len
            .to_be_bytes()
            .iter()
            .copied()
            .skip_while(|b| *b == 0)
            .collect();
        out.push(0x80 | bytes.len() as u8);
        out.extend(bytes);
    }
}

/// An element with `tag` holding the concatenation of `parts`.
pub(crate) fn tlv(tag: u8, parts: &[&[u8]]) -> Vec<u8> {
    let len: usize = parts.iter().map(|p| p.len()).sum();
    let mut out = Vec::with_capacity(len + 6);
    out.push(tag);
    push_len(&mut out, len);
    for p in parts {
        out.extend_from_slice(p);
    }
    out
}

/// A GeneralizedTime element of a Unix time (whole seconds).
pub(crate) fn generalized(secs: i64) -> Vec<u8> {
    let text = time::OffsetDateTime::from_unix_timestamp(secs)
        .ok()
        .and_then(|t| {
            t.format(&time::macros::format_description!(
                "[year][month][day][hour][minute][second]"
            ))
            .ok()
        })
        .unwrap_or_else(|| "19700101000000".into());
    tlv(0x18, &[text.as_bytes(), b"Z"])
}

/// The raw bytes of the first element of a SEQUENCE (the part a signature
/// of a certificate, a revocation list or an OCSP answer covers).
pub(crate) fn signed_part(der: &[u8]) -> Option<&[u8]> {
    let outer = expect(der, 0x30)?;
    Some(element(outer.value)?.whole)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn times_are_read() {
        assert_eq!(generalized_time(b"19700101000001Z"), Some(1));
        assert_eq!(generalized_time(b"20261018120000.123Z"), Some(1_792_324_800));
        assert_eq!(generalized_time(b"20261018120000"), None);
        assert_eq!(generalized_time(b"2026101812000Z"), None);
        let g = generalized(1_792_324_800);
        assert_eq!(generalized_time(element(&g).unwrap().value), Some(1_792_324_800));
    }

    #[test]
    fn elements_are_split_and_written() {
        let data = [0x30, 0x03, 0x02, 0x01, 0x07, 0x05, 0x00];
        let e = element(&data).unwrap();
        assert_eq!(
            (e.tag, e.whole.len(), e.value, e.rest),
            (0x30, 5, &data[2..5], &data[5..])
        );
        assert!(expect(&data, 0x31).is_none());
        assert_eq!(expect(e.value, 0x02).unwrap().value, [0x07]);
        assert!(element(&[0x30, 0x05, 0x00]).is_none());
        assert_eq!(elements(&data).unwrap().len(), 2);
        assert!(elements(&data[..6]).is_none());
        assert_eq!(tlv(0x30, &[&[0x02, 0x01, 0x07]]), data[..5]);
        let long = tlv(0x04, &[&[0u8; 300]]);
        assert_eq!(&long[..4], &[0x04, 0x82, 0x01, 0x2C]);
        assert_eq!(signed_part(&data[..5]), Some(&[0x02, 0x01, 0x07][..]));
    }
}
