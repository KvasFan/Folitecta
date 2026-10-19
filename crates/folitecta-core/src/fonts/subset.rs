//! A TrueType subsetter that keeps the glyph numbers of the font.
//!
//! The glyphs asked for (with the components of composite glyphs, and glyph
//! 0) keep their records byte for byte, and their advances and side bearings;
//! the other glyphs up to the highest one kept become empty, without metrics.
//! Only the tables that index the glyphs are written again (`loca`, `hmtx`,
//! `head`, `hhea`, `maxp`, a `post` without glyph names and a small `cmap` of
//! the characters drawn); `OS/2`, `name` and the hinting tables are copied
//! whole. Tables that
//! depend on the number of glyphs and are not needed to draw (`hdmx`, `LTSH`,
//! `VDMX`, `vhea`, `vmtx`) and the layout and kerning tables are left out.

use std::collections::{BTreeMap, BTreeSet};

use skrifa::FontRef;

use super::source::{be_i16, be_u16, be_u32, table};

/// Tables copied whole when the font has them.
const COPIED: [&[u8; 4]; 6] = [b"OS/2", b"name", b"cvt ", b"fpgm", b"prep", b"gasp"];

/// The components of a composite glyph record.
fn components(record: &[u8]) -> Result<Vec<u16>, String> {
    let damaged = || "damaged composite glyph".to_string();
    let mut at = 10;
    let mut out = Vec::new();
    loop {
        let flags = be_u16(record, at).ok_or_else(damaged)?;
        out.push(be_u16(record, at + 2).ok_or_else(damaged)?);
        at += 4 + if flags & 0x0001 != 0 { 4 } else { 2 };
        at += if flags & 0x0008 != 0 {
            2
        } else if flags & 0x0040 != 0 {
            4
        } else if flags & 0x0080 != 0 {
            8
        } else {
            0
        };
        if flags & 0x0020 == 0 {
            return Ok(out);
        }
        if out.len() > 1024 || at > record.len() {
            return Err(damaged());
        }
    }
}

/// Sum of the big-endian 32-bit words of `data`, zero-padded.
pub(crate) fn checksum(data: &[u8]) -> u32 {
    data.chunks(4).fold(0u32, |sum, c| {
        let mut word = [0u8; 4];
        word[..c.len()].copy_from_slice(c);
        sum.wrapping_add(u32::from_be_bytes(word))
    })
}

/// A `cmap` with the characters drawn: format 4 for the Basic Multilingual
/// Plane (3, 1) and format 12 (3, 10) when there are characters beyond it.
fn cmap_table(chars: &[(char, u16)]) -> Vec<u8> {
    let mut all: Vec<(u32, u16)> = chars.iter().map(|(c, g)| (*c as u32, *g)).collect();
    all.sort_unstable();
    all.dedup_by_key(|(c, _)| *c);
    let bmp: Vec<(u16, u16)> = all
        .iter()
        .filter(|(c, _)| *c < 0xFFFF)
        .map(|(c, g)| (*c as u16, *g))
        .collect();
    let mut subtables: Vec<(u16, Vec<u8>)> = Vec::new();
    let seg_count = bmp.len() + 1;
    if 16 + 8 * seg_count <= 0xFFFF {
        let mut f4 = Vec::new();
        let p = 1usize << (usize::BITS - 1 - seg_count.leading_zeros());
        for v in [
            4u16,
            (16 + 8 * seg_count) as u16,
            0,
            (2 * seg_count) as u16,
            (2 * p) as u16,
            p.trailing_zeros() as u16,
            (2 * seg_count - 2 * p) as u16,
        ] {
            f4.extend(v.to_be_bytes());
        }
        let ends = bmp.iter().map(|(c, _)| *c).chain([0xFFFF]);
        ends.clone().for_each(|c| f4.extend(c.to_be_bytes()));
        f4.extend(0u16.to_be_bytes());
        ends.for_each(|c| f4.extend(c.to_be_bytes()));
        bmp.iter()
            .map(|(c, g)| g.wrapping_sub(*c))
            .chain([1])
            .for_each(|d| f4.extend(d.to_be_bytes()));
        (0..seg_count).for_each(|_| f4.extend(0u16.to_be_bytes()));
        subtables.push((1, f4));
    }
    if all.iter().any(|(c, _)| *c > 0xFFFF) || subtables.is_empty() {
        let mut f12 = Vec::new();
        f12.extend(12u16.to_be_bytes());
        f12.extend(0u16.to_be_bytes());
        f12.extend((16 + 12 * all.len() as u32).to_be_bytes());
        f12.extend(0u32.to_be_bytes());
        f12.extend((all.len() as u32).to_be_bytes());
        for (c, g) in &all {
            f12.extend(c.to_be_bytes());
            f12.extend(c.to_be_bytes());
            f12.extend((*g as u32).to_be_bytes());
        }
        subtables.push((10, f12));
    }
    let mut out = Vec::new();
    out.extend(0u16.to_be_bytes());
    out.extend((subtables.len() as u16).to_be_bytes());
    let mut offset = 4 + 8 * subtables.len() as u32;
    for (encoding, body) in &subtables {
        out.extend(3u16.to_be_bytes());
        out.extend(encoding.to_be_bytes());
        out.extend(offset.to_be_bytes());
        offset += body.len() as u32;
    }
    for (_, body) in subtables {
        out.extend(body);
    }
    out
}

/// A font program with the glyphs `glyphs` (and what they need) of `data`,
/// under their own numbers; `chars` are the characters drawn and their glyphs.
pub(crate) fn subset(data: &[u8], glyphs: &BTreeSet<u16>, chars: &[(char, u16)]) -> Result<Vec<u8>, String> {
    let font = FontRef::new(data).map_err(|_| "not a TrueType font".to_string())?;
    let get =
        |tag: &[u8; 4]| table(&font, tag).ok_or_else(|| format!("no {} table", String::from_utf8_lossy(tag)));
    let (head, maxp, hhea, hmtx) = (get(b"head")?, get(b"maxp")?, get(b"hhea")?, get(b"hmtx")?);
    let (loca, glyf) = (get(b"loca")?, get(b"glyf")?);
    let damaged = |what: &str| format!("damaged font ({what})");
    let num_glyphs = be_u16(maxp, 4).ok_or_else(|| damaged("maxp"))? as usize;
    let h_metrics = (be_u16(hhea, 34).ok_or_else(|| damaged("hhea"))? as usize).min(num_glyphs);
    if h_metrics == 0 {
        return Err(damaged("hhea"));
    }
    let long_loca = be_i16(head, 50).ok_or_else(|| damaged("head"))? == 1;
    if head.len() < 54 || hhea.len() < 36 || maxp.len() < 6 {
        return Err(damaged("table sizes"));
    }
    let offset = |g: usize| -> Option<usize> {
        if long_loca {
            be_u32(loca, 4 * g).map(|v| v as usize)
        } else {
            be_u16(loca, 2 * g).map(|v| v as usize * 2)
        }
    };
    let record = |g: usize| -> Result<&[u8], String> {
        let (start, end) = (offset(g), offset(g + 1));
        match (start, end) {
            (Some(s), Some(e)) if s <= e && e <= glyf.len() => Ok(&glyf[s..e]),
            _ => Err(damaged("glyph offsets")),
        }
    };

    // Glyph 0, the glyphs asked for and the components of composite glyphs.
    let mut keep = BTreeSet::new();
    let mut work: Vec<u16> = glyphs.iter().copied().chain([0]).collect();
    while let Some(g) = work.pop() {
        if g as usize >= num_glyphs {
            return Err(damaged(&format!("glyph {g} does not exist")));
        }
        if !keep.insert(g) {
            continue;
        }
        let rec = record(g as usize)?;
        if rec.len() >= 10 && be_i16(rec, 0).unwrap_or(0) < 0 {
            work.extend(components(rec)?);
        }
    }
    let n = *keep.last().unwrap_or(&0) as usize + 1;

    let mut new_glyf = Vec::new();
    let mut new_loca = Vec::with_capacity(4 * (n + 1));
    for g in 0..n {
        new_loca.extend((new_glyf.len() as u32).to_be_bytes());
        if keep.contains(&(g as u16)) {
            new_glyf.extend_from_slice(record(g)?);
            while new_glyf.len() % 4 != 0 {
                new_glyf.push(0);
            }
        }
    }
    new_loca.extend((new_glyf.len() as u32).to_be_bytes());

    // hmtx: the original advance and side bearing of each kept glyph; the
    // glyphs left out have none, like their empty outlines.
    let mut new_hmtx = Vec::with_capacity(4 * n);
    for g in 0..n {
        if keep.contains(&(g as u16)) {
            let (advance, bearing) = if g < h_metrics {
                (be_u16(hmtx, 4 * g), be_u16(hmtx, 4 * g + 2))
            } else {
                (
                    be_u16(hmtx, 4 * (h_metrics - 1)),
                    be_u16(hmtx, 4 * h_metrics + 2 * (g - h_metrics)),
                )
            };
            new_hmtx.extend(advance.ok_or_else(|| damaged("hmtx"))?.to_be_bytes());
            new_hmtx.extend(bearing.ok_or_else(|| damaged("hmtx"))?.to_be_bytes());
        } else {
            new_hmtx.extend([0; 4]);
        }
    }

    let mut new_head = head.to_vec();
    new_head[8..12].copy_from_slice(&[0; 4]);
    new_head[50..52].copy_from_slice(&1i16.to_be_bytes());
    let mut new_hhea = hhea.to_vec();
    new_hhea[34..36].copy_from_slice(&(n as u16).to_be_bytes());
    let mut new_maxp = maxp.to_vec();
    new_maxp[4..6].copy_from_slice(&(n as u16).to_be_bytes());
    let mut new_post = vec![0u8; 32];
    if let Some(post) = table(&font, b"post") {
        let len = post.len().min(16);
        new_post[..len].copy_from_slice(&post[..len]);
    }
    new_post[0..4].copy_from_slice(&0x0003_0000u32.to_be_bytes());

    let mut tables: BTreeMap<[u8; 4], Vec<u8>> = BTreeMap::new();
    for tag in COPIED {
        if let Some(t) = table(&font, tag) {
            tables.insert(*tag, t.to_vec());
        }
    }
    tables.insert(*b"cmap", cmap_table(chars));
    tables.insert(*b"glyf", new_glyf);
    tables.insert(*b"head", new_head);
    tables.insert(*b"hhea", new_hhea);
    tables.insert(*b"hmtx", new_hmtx);
    tables.insert(*b"loca", new_loca);
    tables.insert(*b"maxp", new_maxp);
    tables.insert(*b"post", new_post);
    Ok(write_sfnt(&tables))
}

/// An sfnt file of `tables` (sorted by tag), with checksums.
fn write_sfnt(tables: &BTreeMap<[u8; 4], Vec<u8>>) -> Vec<u8> {
    let count = tables.len();
    let p = 1usize << (usize::BITS - 1 - count.max(1).leading_zeros());
    let mut out = Vec::new();
    out.extend(0x0001_0000u32.to_be_bytes());
    for v in [count, 16 * p, p.trailing_zeros() as usize, 16 * count - 16 * p] {
        out.extend((v as u16).to_be_bytes());
    }
    let mut offset = 12 + 16 * count;
    let mut head_at = None;
    for (tag, body) in tables {
        if tag == b"head" {
            head_at = Some(offset);
        }
        out.extend(tag);
        out.extend(checksum(body).to_be_bytes());
        out.extend((offset as u32).to_be_bytes());
        out.extend((body.len() as u32).to_be_bytes());
        offset += body.len().div_ceil(4) * 4;
    }
    for body in tables.values() {
        out.extend(body);
        while out.len() % 4 != 0 {
            out.push(0);
        }
    }
    if let Some(at) = head_at {
        let adjustment = 0xB1B0_AFBAu32.wrapping_sub(checksum(&out));
        out[at + 8..at + 12].copy_from_slice(&adjustment.to_be_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fonts::source::tests::bundled;
    use skrifa::MetadataProvider;
    use skrifa::outline::{DrawSettings, pen::SvgPen};
    use skrifa::prelude::{LocationRef, Size};

    fn subset_of(text: &str) -> (Vec<u8>, Vec<(char, u16)>) {
        let font = bundled();
        let chars: Vec<(char, u16)> = text.chars().map(|c| (c, font.glyph(c).unwrap())).collect();
        let glyphs = chars.iter().map(|(_, g)| *g).collect();
        (subset(font.data(), &glyphs, &chars).unwrap(), chars)
    }

    fn tags(data: &[u8]) -> Vec<String> {
        let font = FontRef::new(data).unwrap();
        font.table_directory()
            .table_records()
            .iter()
            .map(|r| r.tag().to_string())
            .collect()
    }

    fn glyph_record(data: &[u8], g: usize) -> Vec<u8> {
        let font = FontRef::new(data).unwrap();
        let (head, loca, glyf) = (
            table(&font, b"head").unwrap(),
            table(&font, b"loca").unwrap(),
            table(&font, b"glyf").unwrap(),
        );
        let off = |i: usize| {
            if be_i16(head, 50).unwrap() == 1 {
                be_u32(loca, 4 * i).unwrap() as usize
            } else {
                be_u16(loca, 2 * i).unwrap() as usize * 2
            }
        };
        glyf[off(g)..off(g + 1)].to_vec()
    }

    /// The subset record is the original one, padded with zeros to four bytes.
    fn same_record(subset: &[u8], original: &[u8]) -> bool {
        subset.starts_with(original)
            && subset.len() - original.len() < 4
            && subset[original.len()..].iter().all(|b| *b == 0)
    }

    fn outline(data: &[u8], g: u16) -> String {
        let font = FontRef::new(data).unwrap();
        let glyph = font.outline_glyphs().get(skrifa::GlyphId::new(g as u32)).unwrap();
        let mut pen = SvgPen::default();
        glyph
            .draw(
                DrawSettings::unhinted(Size::unscaled(), LocationRef::default()),
                &mut pen,
            )
            .unwrap();
        pen.to_string()
    }

    #[test]
    fn subset_keeps_ids_and_glyph_bytes() {
        let (data, chars) = subset_of("AŁλЖ");
        let source = bundled();
        let mut found = tags(&data);
        found.sort();
        let mut expected: Vec<String> = [
            "OS/2", "cmap", "glyf", "head", "hhea", "hmtx", "loca", "maxp", "name", "post",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let src = FontRef::new(source.data()).unwrap();
        for t in ["cvt ", "fpgm", "prep", "gasp"] {
            if table(&src, t.as_bytes().try_into().unwrap()).is_some() {
                expected.push(t.to_string());
            }
        }
        expected.sort();
        assert_eq!(found, expected);
        let font = FontRef::new(&data).unwrap();
        let max = chars.iter().map(|(_, g)| *g).max().unwrap();
        assert_eq!(be_u16(table(&font, b"maxp").unwrap(), 4).unwrap(), max + 1);
        assert_eq!(be_i16(table(&font, b"head").unwrap(), 50).unwrap(), 1);
        assert_eq!(be_u32(table(&font, b"post").unwrap(), 0).unwrap(), 0x0003_0000);
        for (c, g) in &chars {
            let original = glyph_record(source.data(), *g as usize);
            assert!(!original.is_empty(), "{c}");
            assert!(same_record(&glyph_record(&data, *g as usize), &original), "{c}");
            assert_eq!(outline(&data, *g), outline(source.data(), *g), "{c}");
            // The small cmap maps the characters to their glyphs.
            assert_eq!(font.charmap().map(*c).map(|g| g.to_u32()), Some(*g as u32), "{c}");
        }
        let unkept = (1..max)
            .find(|g| {
                !chars.iter().any(|(_, k)| k == g) && !glyph_record(source.data(), *g as usize).is_empty()
            })
            .unwrap();
        assert!(glyph_record(&data, unkept as usize).is_empty());
        // The advances of the kept glyphs are the original ones.
        let metrics = font.glyph_metrics(Size::unscaled(), LocationRef::default());
        let src_metrics = src.glyph_metrics(Size::unscaled(), LocationRef::default());
        for (_, g) in &chars {
            let id = skrifa::GlyphId::new(*g as u32);
            assert_eq!(metrics.advance_width(id), src_metrics.advance_width(id));
            assert_eq!(metrics.left_side_bearing(id), src_metrics.left_side_bearing(id));
        }
        assert_eq!(
            metrics.advance_width(skrifa::GlyphId::new(unkept as u32)),
            Some(0.0)
        );
    }

    #[test]
    fn composite_glyphs_bring_their_components() {
        let source = bundled();
        let composite = ('À'..='ž')
            .filter_map(|c| source.glyph(c).map(|g| (c, g)))
            .find(|(_, g)| {
                let rec = glyph_record(source.data(), *g as usize);
                rec.len() >= 10 && be_i16(&rec, 0).unwrap() < 0
            })
            .expect("a composite glyph among the accented letters");
        let (data, _) = subset_of(&composite.0.to_string());
        let parts = components(&glyph_record(source.data(), composite.1 as usize)).unwrap();
        assert!(!parts.is_empty());
        for part in parts {
            assert!(!glyph_record(&data, part as usize).is_empty(), "component {part}");
        }
        assert_eq!(outline(&data, composite.1), outline(source.data(), composite.1));
    }

    #[test]
    fn font_checksums_are_valid() {
        let (data, _) = subset_of("Łódź Ελληνικά");
        let font = FontRef::new(&data).unwrap();
        for rec in font.table_directory().table_records() {
            let start = rec.offset() as usize;
            let mut body = data[start..start + rec.length() as usize].to_vec();
            if rec.tag() == skrifa::Tag::new(b"head") {
                // The checksum of head is computed with checkSumAdjustment at zero.
                body[8..12].copy_from_slice(&[0; 4]);
            }
            assert_eq!(checksum(&body), rec.checksum(), "{}", rec.tag());
        }
        assert_eq!(checksum(&data), 0xB1B0_AFBA);
    }

    #[test]
    fn damaged_fonts_are_errors_not_panics() {
        let source = bundled();
        let glyphs: BTreeSet<u16> = [60000].into();
        assert!(subset(source.data(), &glyphs, &[]).is_err());
        assert!(subset(b"not a font at all", &BTreeSet::new(), &[]).is_err());
        // A composite glyph whose component points back at itself.
        let mut rec = vec![0xFF, 0xFF, 0, 0, 0, 0, 0, 0, 0, 0];
        rec.extend([0x00, 0x20, 0x00, 0x05, 0, 0, 0x00, 0x00, 0x00, 0x05, 0, 0]);
        assert_eq!(components(&rec).unwrap(), vec![5, 5]);
        assert!(components(&rec[..14]).is_err());
    }
}
