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

#[cfg(test)]
mod tests {
    use super::*;

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
