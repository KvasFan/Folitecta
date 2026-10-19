//! Paper sizes: the common ones, the PWG media names printers use, and the
//! paper closest to a page.

use super::Paper;

const MM: f32 = 72.0 / 25.4;

/// A paper size known by name.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StandardPaper {
    /// The PWG media keyword.
    pub key: &'static str,
    /// The short name shown in lists.
    pub name: &'static str,
    /// Width and height in points (upright).
    pub width: f32,
    pub height: f32,
}

const fn mm(key: &'static str, name: &'static str, w: f32, h: f32) -> StandardPaper {
    StandardPaper {
        key,
        name,
        width: w * MM,
        height: h * MM,
    }
}

const fn inch(key: &'static str, name: &'static str, w: f32, h: f32) -> StandardPaper {
    StandardPaper {
        key,
        name,
        width: w * 72.0,
        height: h * 72.0,
    }
}

/// The papers offered when no printer lists its own (saving as PDF, the
/// system dialog).
pub const STANDARD_PAPERS: &[StandardPaper] = &[
    mm("iso_a3_297x420mm", "A3", 297.0, 420.0),
    mm("iso_a4_210x297mm", "A4", 210.0, 297.0),
    mm("iso_a5_148x210mm", "A5", 148.0, 210.0),
    mm("iso_a6_105x148mm", "A6", 105.0, 148.0),
    mm("iso_b4_250x353mm", "B4", 250.0, 353.0),
    mm("iso_b5_176x250mm", "B5", 176.0, 250.0),
    inch("na_letter_8.5x11in", "Letter", 8.5, 11.0),
    inch("na_legal_8.5x14in", "Legal", 8.5, 14.0),
    inch("na_ledger_11x17in", "Tabloid", 11.0, 17.0),
    inch("na_executive_7.25x10.5in", "Executive", 7.25, 10.5),
];

/// Width and height in points of a self-describing PWG media name
/// (`iso_a4_210x297mm`, `na_letter_8.5x11in`, `om_small-photo_100x150mm`).
/// Ranges of custom sizes and names without dimensions give `None`.
pub fn media_size(keyword: &str) -> Option<(f32, f32)> {
    let k = keyword.trim();
    if k.starts_with("custom_min_") || k.starts_with("custom_max_") {
        return None;
    }
    let dims = k.rsplit('_').next()?;
    let (unit, scale) = if let Some(d) = dims.strip_suffix("mm") {
        (d, MM)
    } else {
        (dims.strip_suffix("in")?, 72.0)
    };
    let (w, h) = unit.split_once('x')?;
    let (w, h): (f32, f32) = (w.parse().ok()?, h.parse().ok()?);
    if !(w.is_finite() && h.is_finite()) || w <= 0.0 || h <= 0.0 || w > 10_000.0 || h > 10_000.0 {
        return None;
    }
    Some((w * scale, h * scale))
}

/// The paper of a media keyword, without margins (upright).
pub fn paper_for_key(key: &str) -> Option<Paper> {
    let (w, h) = media_size(key)?;
    Some(Paper::new(key, w, h))
}

/// The standard paper of a keyword, by name or by size.
pub fn standard_name(key: &str) -> Option<&'static str> {
    if let Some(p) = STANDARD_PAPERS.iter().find(|p| p.key == key) {
        return Some(p.name);
    }
    let (w, h) = media_size(key)?;
    STANDARD_PAPERS
        .iter()
        .find(|p| same_size((p.width, p.height), (w, h), 1.5))
        .map(|p| p.name)
}

/// Two sizes are the same, whatever the orientation, within `tol` points.
pub fn same_size(a: (f32, f32), b: (f32, f32), tol: f32) -> bool {
    let (a0, a1) = (a.0.min(a.1), a.0.max(a.1));
    let (b0, b1) = (b.0.min(b.1), b.0.max(b.1));
    (a0 - b0).abs() <= tol && (a1 - b1).abs() <= tol
}

/// The paper among `papers` of the size of a page of `w` x `h` points, in
/// either orientation, within `tol` points (the closest when several are).
pub fn closest_paper(w: f32, h: f32, papers: &[Paper], tol: f32) -> Option<&Paper> {
    let (p0, p1) = (w.min(h), w.max(h));
    papers
        .iter()
        .filter(|p| same_size((p.width, p.height), (w, h), tol))
        .min_by(|a, b| {
            let da = (a.width - p0).abs() + (a.height - p1).abs();
            let db = (b.width - p0).abs() + (b.height - p1).abs();
            da.total_cmp(&db)
        })
}

/// The usual paper of a region: Letter in the United States, Canada,
/// Mexico and the Philippines, A4 elsewhere. `locale` is a POSIX locale
/// name such as `en_US.UTF-8` (empty: A4).
pub fn default_paper_key(locale: &str) -> &'static str {
    let region = locale
        .split(['.', '@'])
        .next()
        .and_then(|l| l.split(['_', '-']).nth(1))
        .unwrap_or("")
        .to_ascii_uppercase();
    if matches!(region.as_str(), "US" | "CA" | "MX" | "PH") {
        "na_letter_8.5x11in"
    } else {
        "iso_a4_210x297mm"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: (f32, f32), b: (f32, f32)) -> bool {
        (a.0 - b.0).abs() < 0.01 && (a.1 - b.1).abs() < 0.01
    }

    #[test]
    fn pwg_names_give_their_sizes() {
        assert!(close(media_size("iso_a4_210x297mm").unwrap(), (595.28, 841.89)));
        assert!(close(media_size("na_letter_8.5x11in").unwrap(), (612.0, 792.0)));
        assert!(close(media_size("jis_b5_182x257mm").unwrap(), (515.91, 728.50)));
        assert!(close(
            media_size("om_small-photo_100x150mm").unwrap(),
            (283.46, 425.20)
        ));
        assert_eq!(media_size("custom_min_25.4x25.4mm"), None);
        assert_eq!(media_size("custom_max_215.9x355.6mm"), None);
        assert_eq!(media_size("a4"), None);
        assert_eq!(media_size("iso_a4_0x297mm"), None);
        assert_eq!(media_size("na_letter_axbin"), None);
    }

    #[test]
    fn the_closest_paper_ignores_orientation_within_tolerance() {
        let papers: Vec<Paper> = STANDARD_PAPERS
            .iter()
            .map(|p| Paper::new(p.key, p.width, p.height))
            .collect();
        assert_eq!(
            closest_paper(842.0, 595.0, &papers, 3.0).unwrap().key,
            "iso_a4_210x297mm"
        );
        assert_eq!(
            closest_paper(612.0, 792.0, &papers, 3.0).unwrap().key,
            "na_letter_8.5x11in"
        );
        assert!(closest_paper(600.0, 800.0, &papers, 3.0).is_none());
        assert_eq!(standard_name("iso_a4_210x297mm"), Some("A4"));
        assert_eq!(standard_name("na_letter_8.5x11in"), Some("Letter"));
        assert_eq!(standard_name("om_x_210x297mm"), Some("A4"));
    }

    #[test]
    fn regions_choose_letter_or_a4() {
        assert_eq!(default_paper_key("en_US.UTF-8"), "na_letter_8.5x11in");
        assert_eq!(default_paper_key("fr_CA"), "na_letter_8.5x11in");
        assert_eq!(default_paper_key("it_IT.UTF-8@euro"), "iso_a4_210x297mm");
        assert_eq!(default_paper_key(""), "iso_a4_210x297mm");
        assert_eq!(default_paper_key("C"), "iso_a4_210x297mm");
    }
}
