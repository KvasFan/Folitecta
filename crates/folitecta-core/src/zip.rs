//! ZIP archives: written with deflated (or stored) entries and read back,
//! for documents sent or kept together. No encryption and no ZIP64, so an
//! archive stays below 4 GB and 65 535 entries.

use std::io::{Read, Write};

use crate::error::{Error, Result};

const LOCAL: u32 = 0x0403_4b50;
const CENTRAL: u32 = 0x0201_4b50;
const END: u32 = 0x0605_4b50;
/// Bit 11 of the flags: the name is UTF-8.
const UTF8: u16 = 0x0800;
/// Version 2.0: deflate and folders.
const VERSION: u16 = 20;

/// One file of an archive.
#[derive(Debug, Clone, PartialEq)]
pub struct ZipEntry {
    /// Path inside the archive, folders separated by `/`.
    pub name: String,
    pub data: Vec<u8>,
}

impl ZipEntry {
    pub fn new(name: impl Into<String>, data: Vec<u8>) -> Self {
        ZipEntry {
            name: name.into(),
            data,
        }
    }
}

fn bad(what: impl std::fmt::Display) -> Error {
    Error::Invalid(format!("invalid ZIP archive: {what}"))
}

fn too_large() -> Error {
    Error::Invalid("the archive is too large for the ZIP format (4 GB, 65 535 files)".into())
}

fn put16(out: &mut Vec<u8>, v: u16) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn put32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn u16_at(data: &[u8], at: usize) -> Result<u16> {
    data.get(at..at + 2)
        .map(|b| u16::from_le_bytes([b[0], b[1]]))
        .ok_or_else(|| bad("it ends too early"))
}

fn u32_at(data: &[u8], at: usize) -> Result<u32> {
    data.get(at..at + 4)
        .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .ok_or_else(|| bad("it ends too early"))
}

/// The current time as MS-DOS time and date (UTC).
fn dos_time() -> (u16, u16) {
    let now = time::OffsetDateTime::now_utc();
    let time = (u16::from(now.hour()) << 11) | (u16::from(now.minute()) << 5) | (u16::from(now.second()) / 2);
    let year = (now.year() - 1980).clamp(0, 127) as u16;
    let date = (year << 9) | ((now.month() as u16) << 5) | u16::from(now.day());
    (time, date)
}

fn deflate(data: &[u8]) -> Result<Vec<u8>> {
    let mut enc = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
    enc.write_all(data)?;
    Ok(enc.finish()?)
}

/// A ZIP archive of `entries`, each one deflated unless that makes it larger
/// (PDF files are mostly compressed already).
pub fn write(entries: &[ZipEntry]) -> Result<Vec<u8>> {
    let count = u16::try_from(entries.len()).map_err(|_| too_large())?;
    let (time, date) = dos_time();
    let mut out = Vec::new();
    let mut central = Vec::new();
    for e in entries {
        let name = e.name.replace('\\', "/").trim_start_matches('/').to_string();
        if name.is_empty() {
            return Err(Error::Invalid("a file of the archive has no name".into()));
        }
        let name_len = u16::try_from(name.len()).map_err(|_| too_large())?;
        let crc = crc32fast::hash(&e.data);
        let deflated = deflate(&e.data)?;
        let (method, body): (u16, &[u8]) = if deflated.len() < e.data.len() {
            (8, &deflated)
        } else {
            (0, &e.data)
        };
        let offset = u32::try_from(out.len()).map_err(|_| too_large())?;
        let packed = u32::try_from(body.len()).map_err(|_| too_large())?;
        let size = u32::try_from(e.data.len()).map_err(|_| too_large())?;
        // The fields both headers share, from the version needed to the extra length.
        let mut common = Vec::with_capacity(26);
        put16(&mut common, VERSION);
        put16(&mut common, UTF8);
        put16(&mut common, method);
        put16(&mut common, time);
        put16(&mut common, date);
        put32(&mut common, crc);
        put32(&mut common, packed);
        put32(&mut common, size);
        put16(&mut common, name_len);
        put16(&mut common, 0);
        put32(&mut out, LOCAL);
        out.extend_from_slice(&common);
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(body);
        put32(&mut central, CENTRAL);
        put16(&mut central, VERSION);
        central.extend_from_slice(&common);
        // Comment length, disk, internal and external attributes, offset.
        put16(&mut central, 0);
        put16(&mut central, 0);
        put16(&mut central, 0);
        put32(&mut central, 0);
        put32(&mut central, offset);
        central.extend_from_slice(name.as_bytes());
    }
    let cd_offset = u32::try_from(out.len()).map_err(|_| too_large())?;
    let cd_size = u32::try_from(central.len()).map_err(|_| too_large())?;
    out.extend_from_slice(&central);
    put32(&mut out, END);
    put16(&mut out, 0);
    put16(&mut out, 0);
    put16(&mut out, count);
    put16(&mut out, count);
    put32(&mut out, cd_size);
    put32(&mut out, cd_offset);
    put16(&mut out, 0);
    Ok(out)
}

/// The files of a ZIP archive (stored and deflated entries; folders are
/// skipped), each checked against its CRC.
pub fn read(data: &[u8]) -> Result<Vec<ZipEntry>> {
    // The end record is the last 22 bytes, unless a comment (up to 64 KB) follows it.
    let last = data.len().checked_sub(22).ok_or_else(|| bad("it is too short"))?;
    let end = (last.saturating_sub(65_535)..=last)
        .rev()
        .find(|&i| u32_at(data, i).ok() == Some(END))
        .ok_or_else(|| bad("its directory is missing"))?;
    let count = u16_at(data, end + 10)?;
    let mut pos = u32_at(data, end + 16)? as usize;
    let mut out = Vec::with_capacity(count as usize);
    for _ in 0..count {
        if u32_at(data, pos)? != CENTRAL {
            return Err(bad("its directory is damaged"));
        }
        let flags = u16_at(data, pos + 8)?;
        let method = u16_at(data, pos + 10)?;
        let crc = u32_at(data, pos + 16)?;
        let packed = u32_at(data, pos + 20)? as usize;
        let size = u32_at(data, pos + 24)? as usize;
        let name_len = u16_at(data, pos + 28)? as usize;
        let extra_len = u16_at(data, pos + 30)? as usize;
        let comment_len = u16_at(data, pos + 32)? as usize;
        let offset = u32_at(data, pos + 42)? as usize;
        let raw_name = data
            .get(pos + 46..pos + 46 + name_len)
            .ok_or_else(|| bad("a name is cut"))?;
        // Names without the UTF-8 flag are in the old DOS code page; their
        // ASCII part reads the same.
        let name = if flags & UTF8 != 0 {
            String::from_utf8_lossy(raw_name).to_string()
        } else {
            raw_name.iter().map(|&b| b as char).collect()
        };
        pos += 46 + name_len + extra_len + comment_len;
        if name.ends_with('/') {
            continue;
        }
        if flags & 1 != 0 {
            return Err(Error::Invalid(format!("“{name}” is encrypted in the archive")));
        }
        if u32_at(data, offset)? != LOCAL {
            return Err(bad("a file header is damaged"));
        }
        let start = offset + 30 + u16_at(data, offset + 26)? as usize + u16_at(data, offset + 28)? as usize;
        let body = data
            .get(start..start + packed)
            .ok_or_else(|| bad("a file is cut"))?;
        let content = match method {
            0 => body.to_vec(),
            8 => {
                let mut v = Vec::with_capacity(size);
                // Never more than the size the directory announces.
                flate2::read::DeflateDecoder::new(body)
                    .take(size as u64 + 1)
                    .read_to_end(&mut v)?;
                v
            }
            m => {
                return Err(Error::Invalid(format!(
                    "“{name}” uses a compression method Folitecta does not read ({m})"
                )));
            }
        };
        if content.len() != size || crc32fast::hash(&content) != crc {
            return Err(bad(format!("“{name}” is damaged")));
        }
        out.push(ZipEntry { name, data: content });
    }
    Ok(out)
}
