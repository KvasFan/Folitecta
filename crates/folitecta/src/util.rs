//! Small UI helpers.

use dioxus::prelude::*;

/// Parse a page range such as `1-3, 5, 8-` (1-based) into zero-based indices.
/// An empty string selects every page.
pub fn parse_page_range(input: &str, page_count: usize) -> Result<Vec<usize>, String> {
    let s = input.trim();
    if page_count == 0 {
        return Ok(Vec::new());
    }
    if s.is_empty() {
        return Ok((0..page_count).collect());
    }
    let mut out: Vec<usize> = Vec::new();
    for part in s.split([',', ';']) {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let (a, b) = match part.split_once('-') {
            Some((a, b)) => (a.trim(), b.trim()),
            None => (part, part),
        };
        let start: usize = if a.is_empty() {
            1
        } else {
            a.parse().map_err(|_| format!("'{part}' is not a page range"))?
        };
        let end: usize = if b.is_empty() {
            page_count
        } else {
            b.parse().map_err(|_| format!("'{part}' is not a page range"))?
        };
        if start == 0 || end == 0 || start > page_count || end > page_count {
            return Err(format!("page numbers must be between 1 and {page_count}"));
        }
        let (lo, hi) = if start <= end { (start, end) } else { (end, start) };
        for p in lo..=hi {
            if !out.contains(&(p - 1)) {
                out.push(p - 1);
            }
        }
    }
    Ok(out)
}

/// Why a print range cannot be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RangeError {
    /// This part is not a page or a range of pages.
    Bad(String),
    /// A page number past the last page (or 0).
    OutOfRange(usize),
}

/// Split `s` at the characters of `seps` outside double quotes.
fn split_unquoted<'a>(s: &'a str, seps: &[char]) -> Vec<&'a str> {
    let mut parts = Vec::new();
    let mut quoted = false;
    let mut start = 0;
    for (i, c) in s.char_indices() {
        if c == '"' {
            quoted = !quoted;
        } else if !quoted && seps.contains(&c) {
            parts.push(&s[start..i]);
            start = i + c.len_utf8();
        }
    }
    parts.push(&s[start..]);
    parts
}

/// The pages of a print range such as `1-3, 7, 10-` (zero-based) in the order
/// written: repeated pages print again and a range written backwards prints
/// backwards. With `use_labels` the ends are page labels (`iii-v`), quoted
/// when a label holds a dash or a comma (`"A-1"-"A-4"`); a number that is
/// not a label is a page number. An empty range is every page.
pub fn parse_print_range(
    input: &str,
    count: usize,
    labels: &[String],
    use_labels: bool,
) -> Result<Vec<usize>, RangeError> {
    if input.trim().is_empty() {
        return Ok((0..count).collect());
    }
    let page = |text: &str, part: &str| -> Result<usize, RangeError> {
        let t = text.trim();
        let unquoted = t.trim_matches('"');
        if use_labels {
            if let Some(i) = labels.iter().position(|l| l == unquoted) {
                return Ok(i);
            }
        }
        let n: usize = unquoted
            .parse()
            .map_err(|_| RangeError::Bad(part.trim().to_string()))?;
        if n == 0 || n > count {
            return Err(RangeError::OutOfRange(count));
        }
        Ok(n - 1)
    };
    let mut out = Vec::new();
    for part in split_unquoted(input, &[',', ';']) {
        if part.trim().is_empty() {
            continue;
        }
        let ends = split_unquoted(part, &['-']);
        let (a, b) = match ends.as_slice() {
            [one] => {
                let p = page(one, part)?;
                (p, p)
            }
            [a, b] => (
                if a.trim().is_empty() { 0 } else { page(a, part)? },
                if b.trim().is_empty() {
                    count.saturating_sub(1)
                } else {
                    page(b, part)?
                },
            ),
            _ => return Err(RangeError::Bad(part.trim().to_string())),
        };
        if count == 0 {
            return Err(RangeError::OutOfRange(0));
        }
        if a <= b {
            out.extend(a..=b);
        } else {
            out.extend((b..=a).rev());
        }
    }
    Ok(out)
}

/// Format zero-based pages as a 1-based range string.
pub fn format_pages(pages: &[usize]) -> String {
    let mut sorted = pages.to_vec();
    sorted.sort_unstable();
    sorted.dedup();
    let mut out = String::new();
    let mut i = 0;
    while i < sorted.len() {
        let start = sorted[i];
        let mut end = start;
        while i + 1 < sorted.len() && sorted[i + 1] == end + 1 {
            end = sorted[i + 1];
            i += 1;
        }
        if !out.is_empty() {
            out.push_str(", ");
        }
        if start == end {
            out.push_str(&(start + 1).to_string());
        } else {
            out.push_str(&format!("{}-{}", start + 1, end + 1));
        }
        i += 1;
    }
    out
}

/// Parse `#rrggbb` into components in `0..=1`.
pub fn parse_color(hex: &str) -> [f32; 3] {
    let h = hex.trim().trim_start_matches('#');
    if h.len() != 6 {
        return [0.0, 0.0, 0.0];
    }
    let c = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).unwrap_or(0) as f32 / 255.0;
    [c(0), c(2), c(4)]
}

pub fn color_hex(c: [f32; 3]) -> String {
    format!(
        "#{:02x}{:02x}{:02x}",
        (c[0] * 255.0).round() as u8,
        (c[1] * 255.0).round() as u8,
        (c[2] * 255.0).round() as u8
    )
}

pub fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut v = bytes as f64;
    let mut i = 0;
    while v >= 1024.0 && i < UNITS.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{bytes} B")
    } else {
        format!("{v:.1} {}", UNITS[i])
    }
}

/// Copy text to the system clipboard through the webview.
pub fn copy_to_clipboard(text: &str) {
    let json = serde_json::to_string(text).unwrap_or_else(|_| "\"\"".into());
    let js = format!("navigator.clipboard.writeText({json});");
    spawn(async move {
        let _ = document::eval(&js).await;
    });
}

/// Open a URL or file with the system default application.
pub fn open_external(target: &str) -> Result<(), String> {
    open::that_detached(target).map_err(|e| e.to_string())
}

/// Move a file to the recycle bin (the trash on Linux and macOS).
#[cfg(target_os = "windows")]
pub fn move_to_trash(path: &std::path::Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows::Win32::UI::Shell::{
        FO_DELETE, FOF_ALLOWUNDO, FOF_NOCONFIRMATION, FOF_NOERRORUI, FOF_SILENT, SHFILEOPSTRUCTW,
        SHFileOperationW,
    };
    use windows::core::PCWSTR;
    let full = std::fs::canonicalize(path).map_err(|e| e.to_string())?;
    // The shell wants a plain path, not the extended `\\?\` form.
    let shown = full.to_string_lossy().trim_start_matches(r"\\?\").to_string();
    // A list of paths ending with two zeros.
    let from: Vec<u16> = std::ffi::OsStr::new(&shown).encode_wide().chain([0, 0]).collect();
    let mut op = SHFILEOPSTRUCTW {
        wFunc: FO_DELETE,
        pFrom: PCWSTR(from.as_ptr()),
        fFlags: (FOF_ALLOWUNDO.0 | FOF_NOCONFIRMATION.0 | FOF_SILENT.0 | FOF_NOERRORUI.0) as u16,
        ..Default::default()
    };
    let r = unsafe { SHFileOperationW(&mut op) };
    if r != 0 || op.fAnyOperationsAborted.as_bool() {
        return Err(format!(
            "the file could not be moved to the recycle bin (error {r})"
        ));
    }
    Ok(())
}

/// Move a file to the trash (freedesktop.org trash specification).
#[cfg(target_os = "linux")]
pub fn move_to_trash(path: &std::path::Path) -> Result<(), String> {
    let full = std::fs::canonicalize(path).map_err(|e| e.to_string())?;
    let data = std::env::var_os("XDG_DATA_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".local/share")))
        .ok_or("no home folder")?;
    let trash = data.join("Trash");
    let (files, info) = (trash.join("files"), trash.join("info"));
    std::fs::create_dir_all(&files).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&info).map_err(|e| e.to_string())?;
    let name = full
        .file_name()
        .ok_or("no file name")?
        .to_string_lossy()
        .to_string();
    // A name not used yet in the trash.
    let mut target = name.clone();
    let mut n = 1;
    while files.join(&target).exists() || info.join(format!("{target}.trashinfo")).exists() {
        n += 1;
        target = format!("{n} {name}");
    }
    let mut encoded = String::new();
    for b in full.to_string_lossy().bytes() {
        if b.is_ascii_alphanumeric() || b"/-_.~".contains(&b) {
            encoded.push(b as char);
        } else {
            encoded.push_str(&format!("%{b:02X}"));
        }
    }
    let date = folitecta_core::util::unix_display(crate::state::now_secs()).replace(' ', "T");
    std::fs::write(
        info.join(format!("{target}.trashinfo")),
        format!("[Trash Info]\nPath={encoded}\nDeletionDate={date}:00\n"),
    )
    .map_err(|e| e.to_string())?;
    let dest = files.join(&target);
    if std::fs::rename(&full, &dest).is_err() {
        std::fs::copy(&full, &dest).map_err(|e| e.to_string())?;
        std::fs::remove_file(&full).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Move a file to the trash through the Finder.
#[cfg(target_os = "macos")]
pub fn move_to_trash(path: &std::path::Path) -> Result<(), String> {
    let full = std::fs::canonicalize(path).map_err(|e| e.to_string())?;
    let p = full.to_string_lossy().replace('\\', "\\\\").replace('"', "\\\"");
    let status = std::process::Command::new("osascript")
        .args([
            "-e",
            &format!("tell application \"Finder\" to delete POSIX file \"{p}\""),
        ])
        .status()
        .map_err(|e| e.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err("the Finder could not move the file to the trash".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn print_ranges_by_number_and_by_label() {
        let labels: Vec<String> = ["i", "ii", "iii", "iv", "v", "1", "2", "A-1", "A-2"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let n = labels.len();
        assert_eq!(parse_print_range("", 3, &[], false), Ok(vec![0, 1, 2]));
        assert_eq!(
            parse_print_range("1-3, 2", n, &labels, false),
            Ok(vec![0, 1, 2, 1])
        );
        assert_eq!(parse_print_range("3-1", n, &labels, false), Ok(vec![2, 1, 0]));
        assert_eq!(parse_print_range("8-", n, &labels, false), Ok(vec![7, 8]));
        assert_eq!(parse_print_range("-2", n, &labels, false), Ok(vec![0, 1]));
        assert_eq!(parse_print_range("iii-v", n, &labels, true), Ok(vec![2, 3, 4]));
        // "1" is a label here (the sixth page).
        assert_eq!(parse_print_range("1", n, &labels, true), Ok(vec![5]));
        assert_eq!(
            parse_print_range("\"A-1\"-\"A-2\"", n, &labels, true),
            Ok(vec![7, 8])
        );
        assert_eq!(parse_print_range("9", n, &labels, true), Ok(vec![8]));
        assert_eq!(
            parse_print_range("x", n, &labels, false),
            Err(RangeError::Bad("x".into()))
        );
        assert_eq!(
            parse_print_range("1-2-3", n, &labels, false),
            Err(RangeError::Bad("1-2-3".into()))
        );
        assert_eq!(
            parse_print_range("0", n, &labels, false),
            Err(RangeError::OutOfRange(n))
        );
        assert_eq!(
            parse_print_range("12", n, &labels, false),
            Err(RangeError::OutOfRange(n))
        );
    }

    #[test]
    fn page_ranges() {
        assert_eq!(parse_page_range("", 3).unwrap(), vec![0, 1, 2]);
        assert_eq!(parse_page_range("1-2, 4", 5).unwrap(), vec![0, 1, 3]);
        assert_eq!(parse_page_range("3-", 4).unwrap(), vec![2, 3]);
        assert_eq!(parse_page_range("4-2", 5).unwrap(), vec![1, 2, 3]);
        assert!(parse_page_range("0", 3).is_err());
        assert!(parse_page_range("9", 3).is_err());
        assert!(parse_page_range("x", 3).is_err());
        assert_eq!(format_pages(&[0, 1, 2, 4]), "1-3, 5");
        assert_eq!(format_pages(&[]), "");
    }

    #[test]
    fn colors() {
        assert_eq!(parse_color("#ff0000"), [1.0, 0.0, 0.0]);
        assert_eq!(color_hex([0.0, 0.5, 1.0]), "#0080ff");
        assert_eq!(human_size(2048), "2.0 KB");
    }
}
