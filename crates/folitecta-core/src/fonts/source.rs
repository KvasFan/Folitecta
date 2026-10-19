//! The Unicode font file: where it is looked for, how it is read and checked.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use sha2::{Digest, Sha256};
use skrifa::{FontRef, MetadataProvider, Tag};

use super::GlyphProblem;

/// File name of the font that draws text beyond WinAnsi.
pub const UNICODE_FONT_FILE: &str = "FreeSans.ttf";

/// The folders set by the program (`None`: the `fonts` folder of the source tree).
static DIRS: Mutex<Option<Vec<PathBuf>>> = Mutex::new(None);
/// The font read from the folders, or why it could not be.
type Loaded = Result<Arc<SourceFont>, GlyphProblem>;
/// The last answer, with the time it was found.
static LOADED: Mutex<Option<(Instant, Loaded)>> = Mutex::new(None);
/// A font that could not be found or read is looked for again after this time.
const RETRY_AFTER: Duration = Duration::from_secs(10);

/// Look for the Unicode font in exactly these folders, in this order (the
/// program sets them once at start; tests that change them run in their own
/// test program, the setting is shared by the whole process).
pub fn set_font_dirs(dirs: Vec<PathBuf>) {
    *DIRS.lock().unwrap_or_else(|e| e.into_inner()) = Some(dirs);
    *LOADED.lock().unwrap_or_else(|e| e.into_inner()) = None;
}

/// The folders the Unicode font is looked for in.
pub fn font_dirs() -> Vec<PathBuf> {
    DIRS.lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
        .unwrap_or_else(|| vec![Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fonts")])
}

/// Read the Unicode font now (for a background thread at start, so the first
/// text that needs it does not wait).
pub fn preload() {
    let _ = unicode_font();
}

/// The Unicode font: the first file of the folders that can be used. A file
/// that cannot be used is skipped for the next folder; its problem is the
/// answer only when no folder has a usable one.
pub(crate) fn unicode_font() -> Loaded {
    let mut loaded = LOADED.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((at, result)) = loaded.as_ref() {
        if result.is_ok() || at.elapsed() < RETRY_AFTER {
            return result.clone();
        }
    }
    let dirs = font_dirs();
    let mut first_problem = None;
    let mut found = None;
    for dir in &dirs {
        let path = dir.join(UNICODE_FONT_FILE);
        if !path.is_file() {
            continue;
        }
        match SourceFont::load(&path) {
            Ok(font) => {
                found = Some(Arc::new(font));
                break;
            }
            Err(problem) => {
                first_problem.get_or_insert(problem);
            }
        }
    }
    let result = match (found, first_problem) {
        (Some(font), _) => Ok(font),
        (None, Some(problem)) => Err(problem),
        (None, None) => Err(GlyphProblem::FontMissing {
            file: UNICODE_FONT_FILE.to_string(),
            searched: dirs,
        }),
    };
    *loaded = Some((Instant::now(), result.clone()));
    result
}

/// Values of the font descriptor (ISO 32000-1, 9.8), in 1/1000 em.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Descriptor {
    pub(crate) flags: i64,
    pub(crate) bbox: [i64; 4],
    pub(crate) italic_angle: f32,
    pub(crate) ascent: i64,
    pub(crate) descent: i64,
    pub(crate) cap_height: i64,
    pub(crate) stem_v: i64,
}

/// A TrueType font read for drawing generated text.
#[derive(Debug)]
pub(crate) struct SourceFont {
    pub(crate) path: PathBuf,
    /// PostScript name (letters, digits and hyphens only).
    pub(crate) name: String,
    pub(crate) descriptor: Descriptor,
    /// SHA-256 of the file: subsets of different files never share a name.
    pub(crate) digest: [u8; 32],
    data: Vec<u8>,
    cmap: HashMap<char, u16>,
    /// Advance of every glyph in 1/1000 em, rounded (the values of `/W`).
    advances: Vec<u16>,
}

pub(crate) fn be_u16(data: &[u8], at: usize) -> Option<u16> {
    data.get(at..at + 2).map(|b| u16::from_be_bytes([b[0], b[1]]))
}

pub(crate) fn be_i16(data: &[u8], at: usize) -> Option<i16> {
    be_u16(data, at).map(|v| v as i16)
}

pub(crate) fn be_u32(data: &[u8], at: usize) -> Option<u32> {
    data.get(at..at + 4)
        .map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
}

pub(crate) fn table<'a>(font: &FontRef<'a>, tag: &[u8; 4]) -> Option<&'a [u8]> {
    font.table_data(Tag::new(tag)).map(|d| d.as_bytes())
}

/// Letters, digits and hyphens of a PostScript name (ISO 32000-1, 9.6.2).
fn postscript_name(raw: &str) -> String {
    raw.chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
        .take(63)
        .collect()
}

impl SourceFont {
    pub(crate) fn load(path: &Path) -> Result<SourceFont, GlyphProblem> {
        let unreadable = |reason: &str| GlyphProblem::FontUnreadable {
            path: path.to_path_buf(),
            reason: reason.to_string(),
        };
        let data = std::fs::read(path).map_err(|e| unreadable(&e.to_string()))?;
        let font = FontRef::new(&data).map_err(|_| unreadable("not a TrueType or OpenType font"))?;
        if table(&font, b"glyf").is_none() || table(&font, b"loca").is_none() {
            return Err(unreadable("no TrueType outlines (glyf)"));
        }
        let damaged = || unreadable("damaged font tables");
        let head = table(&font, b"head").ok_or_else(damaged)?;
        let maxp = table(&font, b"maxp").ok_or_else(damaged)?;
        let hhea = table(&font, b"hhea").ok_or_else(damaged)?;
        let hmtx = table(&font, b"hmtx").ok_or_else(damaged)?;
        let upem = be_u16(head, 18)
            .filter(|u| (16..=16384).contains(u))
            .ok_or_else(damaged)? as f64;
        let num_glyphs = be_u16(maxp, 4).filter(|n| *n > 0).ok_or_else(damaged)? as usize;
        let h_metrics = be_u16(hhea, 34).filter(|n| *n > 0).ok_or_else(damaged)? as usize;
        let scale = |v: f64| (v * 1000.0 / upem).round() as i64;

        let mut file_name = path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        file_name = postscript_name(&file_name);
        let os2 = table(&font, b"OS/2");
        let fs_type = os2.and_then(|t| be_u16(t, 8)).unwrap_or(0);
        let name = font
            .localized_strings(skrifa::string::StringId::POSTSCRIPT_NAME)
            .english_or_first()
            .map(|s| postscript_name(&s.to_string()))
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| {
                if file_name.is_empty() {
                    "Font".into()
                } else {
                    file_name
                }
            });
        // Usage permissions of OS/2 fsType: when several are set the least
        // restrictive applies. Preview & Print (4) and Editable (8) allow
        // embedding; Restricted (2) alone does not, nor do "no subsetting"
        // (0x100) and "bitmap only" (0x200).
        let restricted = fs_type & 0x0002 != 0 && fs_type & 0x000C == 0;
        if restricted || fs_type & 0x0300 != 0 {
            return Err(GlyphProblem::NotEmbeddable { font: name });
        }

        let mut advances = Vec::with_capacity(num_glyphs);
        let mut last = 0u16;
        for g in 0..num_glyphs {
            if g < h_metrics {
                last = be_u16(hmtx, 4 * g).ok_or_else(damaged)?;
            }
            advances.push(scale(last as f64).clamp(0, u16::MAX as i64) as u16);
        }
        let mut cmap = HashMap::new();
        for (code, gid) in font.charmap().mappings() {
            let gid = gid.to_u32();
            if gid == 0 || gid as usize >= num_glyphs {
                continue;
            }
            if let Some(c) = char::from_u32(code) {
                cmap.insert(c, gid as u16);
            }
        }
        if cmap.is_empty() {
            return Err(unreadable("no Unicode character map"));
        }

        let bbox = [
            scale(be_i16(head, 36).ok_or_else(damaged)? as f64),
            scale(be_i16(head, 38).ok_or_else(damaged)? as f64),
            scale(be_i16(head, 40).ok_or_else(damaged)? as f64),
            scale(be_i16(head, 42).ok_or_else(damaged)? as f64),
        ];
        let ascent = scale(be_i16(hhea, 4).ok_or_else(damaged)? as f64);
        let descent = scale(be_i16(hhea, 6).ok_or_else(damaged)? as f64);
        let os2_version = os2.and_then(|t| be_u16(t, 0)).unwrap_or(0);
        let cap_height = os2
            .filter(|_| os2_version >= 2)
            .and_then(|t| be_i16(t, 88))
            .filter(|c| *c > 0)
            .map(|c| scale(c as f64))
            .unwrap_or((ascent as f64 * 0.7).round() as i64);
        let weight = os2.and_then(|t| be_u16(t, 4)).unwrap_or(400).clamp(100, 900) as f64;
        let post = table(&font, b"post");
        let italic_angle = post
            .and_then(|t| be_u32(t, 4))
            .map(|v| v as i32 as f32 / 65536.0)
            .unwrap_or(0.0);
        let fixed_pitch = post.and_then(|t| be_u32(t, 12)).unwrap_or(0) != 0;
        let mut flags = 4; // Symbolic: the glyphs are reached through CIDToGIDMap.
        if fixed_pitch {
            flags |= 1;
        }
        if italic_angle != 0.0 {
            flags |= 64;
        }
        let descriptor = Descriptor {
            flags,
            bbox,
            italic_angle,
            ascent,
            descent,
            cap_height,
            stem_v: (10.0 + 0.244 * (weight - 50.0)).round() as i64,
        };
        let digest: [u8; 32] = Sha256::digest(&data).into();
        Ok(SourceFont {
            path: path.to_path_buf(),
            name,
            descriptor,
            digest,
            data,
            cmap,
            advances,
        })
    }

    /// The glyph of a character (never the missing glyph 0).
    pub(crate) fn glyph(&self, c: char) -> Option<u16> {
        self.cmap.get(&c).copied()
    }

    /// Advance of a glyph in 1/1000 em, rounded (as written in `/W`).
    pub(crate) fn advance(&self, gid: u16) -> u16 {
        self.advances.get(gid as usize).copied().unwrap_or(0)
    }

    pub(crate) fn data(&self) -> &[u8] {
        &self.data
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// SHA-256 of fonts/FreeSans.ttf, as recorded in fonts/README.md.
    pub(crate) const FREESANS_SHA256: &str =
        "c80858440d8fb618e0ac5ff6f16251dbfa6b3316f00f3cdd17d477297dd87b04";

    pub(crate) fn bundled() -> SourceFont {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fonts")
            .join(UNICODE_FONT_FILE);
        SourceFont::load(&path).expect("fonts/FreeSans.ttf loads")
    }

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    #[test]
    fn bundled_font_is_the_recorded_file() {
        let font = bundled();
        assert_eq!(hex(&font.digest), FREESANS_SHA256);
        let readme =
            std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fonts/README.md"))
                .unwrap();
        assert!(readme.contains(FREESANS_SHA256));
    }

    #[test]
    fn bundled_font_loads_with_its_metrics() {
        let font = bundled();
        assert_eq!(font.name, "FreeSans");
        for c in ['ł', 'λ', 'Ж', 'A', 'Α', 'А', '€', '\u{2212}', ' '] {
            assert!(font.glyph(c).is_some(), "{c}");
        }
        assert!(font.glyph('漢').is_none());
        assert!(font.advance(font.glyph(' ').unwrap()) > 0);
        let d = &font.descriptor;
        assert_eq!(d.flags, 4);
        assert!(d.bbox[2] > d.bbox[0] && d.bbox[3] > d.bbox[1]);
        assert!(d.ascent > 0 && d.descent < 0, "{d:?}");
        assert!(d.cap_height > 0);
        // The file is a TrueType font that may be embedded and subset, with a
        // post table version the renderer accepts.
        let data = font.data();
        let f = FontRef::new(data).unwrap();
        assert!(table(&f, b"glyf").is_some());
        let fs_type = be_u16(table(&f, b"OS/2").unwrap(), 8).unwrap();
        assert_eq!(fs_type & 0x030E, 0, "fsType {fs_type:#x}");
        let post = be_u32(table(&f, b"post").unwrap(), 0).unwrap();
        assert!(
            [0x0001_0000, 0x0002_0000, 0x0003_0000].contains(&post),
            "{post:#x}"
        );
    }

    #[test]
    fn freesans_widths_are_close_to_helvetica_for_letters_and_digits() {
        // FreeSans is not metric-compatible with Helvetica: digits have the
        // same widths, letters differ by at most 34/1000 em (E), lowercase
        // letters are a little narrower; punctuation differs more (the
        // space is 250 against 278, the ellipsis 680 against 1000). A text box
        // that gains a letter beyond WinAnsi is drawn about 2 % narrower.
        let font = bundled();
        let width = |c: char| font.advance(font.glyph(c).unwrap()) as i32;
        let helvetica =
            |c: char| crate::util::helvetica_advance(crate::util::winansi_code(c).unwrap()) as i32;
        for c in ('0'..='9').chain('A'..='Z').chain('a'..='z') {
            let diff = (width(c) - helvetica(c)).abs();
            assert!(
                diff <= 34,
                "{c}: FreeSans {} Helvetica {}",
                width(c),
                helvetica(c)
            );
        }
        assert!(('0'..='9').all(|c| width(c) == helvetica(c)));
        assert_eq!((width(' '), helvetica(' ')), (250, 278));
        assert_eq!((width('…'), helvetica('…')), (680, 1000));
        let text = "The quick brown fox jumps over the lazy dog 0123456789";
        let free: i32 = text.chars().map(width).sum();
        let helv: i32 = text.chars().map(helvetica).sum();
        let ratio = free as f32 / helv as f32;
        assert!((0.96..1.0).contains(&ratio), "{ratio}");
    }
}
