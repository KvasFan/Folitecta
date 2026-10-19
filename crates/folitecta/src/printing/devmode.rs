//! The settings of a Windows print job, as pure functions tested on every
//! system: the fields of the driver's DEVMODE (paper, portrait, two-sided,
//! copies, collation, colour) written into and read from its bytes, who makes
//! the copies, the order of the sides when Folitecta makes them, where each
//! band of pixels goes on the device and the bitmap header of a band.

use folitecta_core::print::Sides;

use super::{JobTicket, Media};

pub const DM_ORIENTATION: u32 = 0x1;
pub const DM_PAPERSIZE: u32 = 0x2;
pub const DM_PAPERLENGTH: u32 = 0x4;
pub const DM_PAPERWIDTH: u32 = 0x8;
pub const DM_COPIES: u32 = 0x100;
pub const DM_COLOR: u32 = 0x800;
pub const DM_DUPLEX: u32 = 0x1000;
pub const DM_COLLATE: u32 = 0x8000;

pub const DMORIENT_PORTRAIT: i16 = 1;
pub const DMDUP_SIMPLEX: i16 = 1;
/// Two-sided, turned on the long edge of an upright sheet.
pub const DMDUP_VERTICAL: i16 = 2;
/// Two-sided, turned on the short edge of an upright sheet.
pub const DMDUP_HORIZONTAL: i16 = 3;
pub const DMPAPER_USER: i16 = 256;
pub const DMCOLOR_MONOCHROME: i16 = 1;
pub const DMCOLOR_COLOR: i16 = 2;

/// Byte offsets of the fields in a DEVMODEW (the same on every Windows).
pub const OFF_FIELDS: usize = 72;
pub const OFF_ORIENTATION: usize = 76;
pub const OFF_PAPER: usize = 78;
pub const OFF_LENGTH: usize = 80;
pub const OFF_WIDTH: usize = 82;
pub const OFF_COPIES: usize = 86;
pub const OFF_COLOR: usize = 92;
pub const OFF_DUPLEX: usize = 94;
pub const OFF_COLLATE: usize = 100;
/// The public part of a DEVMODEW, before the driver's own bytes.
pub const PUBLIC_SIZE: usize = 220;

/// A paper of a driver.
#[derive(Debug, Clone, PartialEq)]
pub struct WinPaper {
    pub code: u16,
    pub name: String,
    /// Width and length in tenths of a millimetre (upright).
    pub size: (i32, i32),
}

/// What a driver reports it can do.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct WinCaps {
    pub papers: Vec<WinPaper>,
    pub duplex: bool,
    pub max_copies: u32,
    pub collate: bool,
    pub color: bool,
}

/// The fields Folitecta sets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DevFields {
    pub fields: u32,
    pub orientation: i16,
    pub paper: i16,
    /// Tenths of a millimetre, for a paper of the user's size.
    pub paper_w: i16,
    pub paper_l: i16,
    pub copies: i16,
    pub collate: i16,
    pub color: i16,
    pub duplex: i16,
}

/// Who makes the copies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CopyPlan {
    /// The driver (the job is sent once).
    Driver,
    /// Folitecta sends the sides as many times as there are copies.
    OwnLoop { collated: bool },
}

/// Points to tenths of a millimetre.
fn tenths(pt: f32) -> i32 {
    (pt / 72.0 * 254.0).round() as i32
}

/// The driver's paper of `width` x `height` points (within 1 mm, either way
/// round), or a paper of the user's size.
pub fn paper_code(width: f32, height: f32, papers: &[WinPaper], preferred: Option<u16>) -> (i16, i16, i16) {
    let (w, l) = (tenths(width.min(height)), tenths(width.max(height)));
    let fits = |p: &WinPaper| {
        let (a, b) = (p.size.0.min(p.size.1), p.size.0.max(p.size.1));
        (a - w).abs() <= 10 && (b - l).abs() <= 10
    };
    if let Some(code) = preferred {
        if papers.iter().any(|p| p.code == code && fits(p)) {
            return (code as i16, 0, 0);
        }
    }
    match papers.iter().find(|p| fits(p)) {
        Some(p) => (p.code as i16, 0, 0),
        None => (
            DMPAPER_USER,
            w.clamp(1, i16::MAX as i32) as i16,
            l.clamp(1, i16::MAX as i32) as i16,
        ),
    }
}

/// The driver's paper code of a media key `dm:<code>`.
pub fn media_code(m: &Media) -> Option<u16> {
    m.key.strip_prefix("dm:")?.parse().ok()
}

/// The fields of a job printing a sheet of `paper` (width, height in points).
pub fn job_fields(t: &JobTicket, paper: (f32, f32), caps: &WinCaps) -> (DevFields, CopyPlan) {
    let copies = t.copies.max(1);
    let plan = if copies == 1 || (caps.max_copies >= copies && (!t.collate || caps.collate)) {
        CopyPlan::Driver
    } else {
        CopyPlan::OwnLoop { collated: t.collate }
    };
    let preferred = t.media.as_ref().and_then(media_code);
    let (code, w, l) = paper_code(paper.0, paper.1, &caps.papers, preferred);
    let mut f = DevFields {
        fields: DM_ORIENTATION | DM_PAPERSIZE,
        orientation: DMORIENT_PORTRAIT,
        paper: code,
        ..Default::default()
    };
    if code == DMPAPER_USER {
        f.fields |= DM_PAPERWIDTH | DM_PAPERLENGTH;
        f.paper_w = w;
        f.paper_l = l;
    }
    f.fields |= DM_COPIES;
    f.copies = match plan {
        CopyPlan::Driver => copies.min(i16::MAX as u32) as i16,
        CopyPlan::OwnLoop { .. } => 1,
    };
    if caps.collate && plan == CopyPlan::Driver && copies > 1 {
        f.fields |= DM_COLLATE;
        f.collate = i16::from(t.collate);
    }
    if caps.duplex {
        f.fields |= DM_DUPLEX;
        f.duplex = match t.sides {
            Sides::OneSided => DMDUP_SIMPLEX,
            Sides::LongEdge => DMDUP_VERTICAL,
            Sides::ShortEdge => DMDUP_HORIZONTAL,
        };
    }
    if caps.color {
        f.fields |= DM_COLOR;
        f.color = if t.gray { DMCOLOR_MONOCHROME } else { DMCOLOR_COLOR };
    }
    (f, plan)
}

fn put16(b: &mut [u8], at: usize, v: i16) {
    if let Some(s) = b.get_mut(at..at + 2) {
        s.copy_from_slice(&v.to_le_bytes());
    }
}

fn get16(b: &[u8], at: usize) -> i16 {
    b.get(at..at + 2)
        .map(|s| i16::from_le_bytes([s[0], s[1]]))
        .unwrap_or(0)
}

/// Write `f` into the bytes of a DEVMODEW (its other fields kept).
pub fn write_fields(dm: &mut [u8], f: &DevFields) {
    if dm.len() < PUBLIC_SIZE.min(OFF_COLLATE + 2) {
        return;
    }
    let old = u32::from_le_bytes([
        dm[OFF_FIELDS],
        dm[OFF_FIELDS + 1],
        dm[OFF_FIELDS + 2],
        dm[OFF_FIELDS + 3],
    ]);
    let mut fields = old | f.fields;
    put16(dm, OFF_ORIENTATION, f.orientation);
    put16(dm, OFF_PAPER, f.paper);
    if f.fields & DM_PAPERWIDTH != 0 {
        put16(dm, OFF_WIDTH, f.paper_w);
        put16(dm, OFF_LENGTH, f.paper_l);
    } else {
        // A standard paper: its size comes from its code.
        fields &= !(DM_PAPERWIDTH | DM_PAPERLENGTH);
    }
    if f.fields & DM_COPIES != 0 {
        put16(dm, OFF_COPIES, f.copies);
    }
    if f.fields & DM_COLOR != 0 {
        put16(dm, OFF_COLOR, f.color);
    }
    if f.fields & DM_DUPLEX != 0 {
        put16(dm, OFF_DUPLEX, f.duplex);
    }
    if f.fields & DM_COLLATE != 0 {
        put16(dm, OFF_COLLATE, f.collate);
    }
    dm[OFF_FIELDS..OFF_FIELDS + 4].copy_from_slice(&fields.to_le_bytes());
}

/// The fields of the bytes of a DEVMODEW (after the driver's own window).
pub fn read_fields(dm: &[u8]) -> DevFields {
    let fields = dm
        .get(OFF_FIELDS..OFF_FIELDS + 4)
        .map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
        .unwrap_or(0);
    DevFields {
        fields,
        orientation: get16(dm, OFF_ORIENTATION),
        paper: get16(dm, OFF_PAPER),
        paper_w: get16(dm, OFF_WIDTH),
        paper_l: get16(dm, OFF_LENGTH),
        copies: get16(dm, OFF_COPIES),
        collate: get16(dm, OFF_COLLATE),
        color: get16(dm, OFF_COLOR),
        duplex: get16(dm, OFF_DUPLEX),
    }
}

/// What the driver's own settings window chose, for the print window.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Chosen {
    pub sides: Option<Sides>,
    pub gray: Option<bool>,
    pub copies: Option<u32>,
    /// The media key of the paper (`dm:<code>`).
    pub paper: Option<String>,
}

/// The settings of the bytes of a DEVMODE that the print window shows.
pub fn chosen(dm: &[u8]) -> Chosen {
    let f = read_fields(dm);
    Chosen {
        sides: (f.fields & DM_DUPLEX != 0).then_some(match f.duplex {
            DMDUP_VERTICAL => Sides::LongEdge,
            DMDUP_HORIZONTAL => Sides::ShortEdge,
            _ => Sides::OneSided,
        }),
        gray: (f.fields & DM_COLOR != 0).then_some(f.color == DMCOLOR_MONOCHROME),
        copies: (f.fields & DM_COPIES != 0 && f.copies > 0).then_some(f.copies as u32),
        paper: (f.fields & DM_PAPERSIZE != 0 && f.paper > 0 && f.paper != DMPAPER_USER)
            .then(|| format!("dm:{}", f.paper)),
    }
}

/// The sides in sending order when Folitecta makes `copies` copies of `n`
/// sides: whole copies one after the other (collated), or each sheet
/// repeated (two sides at a time when two-sided).
pub fn side_order(n: usize, copies: u32, plan: CopyPlan, duplex: bool) -> Vec<usize> {
    match plan {
        CopyPlan::Driver => (0..n).collect(),
        CopyPlan::OwnLoop { collated: true } => (0..copies).flat_map(|_| 0..n).collect(),
        CopyPlan::OwnLoop { collated: false } => {
            let group = if duplex { 2 } else { 1 };
            (0..n)
                .collect::<Vec<_>>()
                .chunks(group)
                .flat_map(|g| std::iter::repeat_n(g.to_vec(), copies as usize).flatten())
                .collect()
        }
    }
}

/// Where a band of `rows` rows from `top`, drawn at `r` dots per inch, goes on
/// a device of `dev` dots per inch: its first row and its height in device
/// pixels. Neighbouring bands meet without a gap or an overlap.
pub fn band_dest(top: u32, rows: u32, dev: u32, r: u32) -> (i32, i32) {
    let to_dev = |v: u32| ((u64::from(v) * u64::from(dev) + u64::from(r) / 2) / u64::from(r.max(1))) as i32;
    let (y0, y1) = (to_dev(top), to_dev(top + rows));
    (y0, y1 - y0)
}

/// The pixels a drawing at `r` dots per inch needs to cover `device`
/// pixels of a device of `dev` dots per inch.
pub fn source_pixels(device: u32, dev: u32, r: u32) -> u32 {
    ((u64::from(device) * u64::from(r)).div_ceil(u64::from(dev.max(1)))) as u32
}

/// The BITMAPINFO of a band, bottom-up (the rows from the last), 24-bit, or
/// 8-bit with a grey palette: little-endian 32-bit words.
pub fn dib_info(width: u32, rows: u32, bits: u16) -> Vec<u32> {
    let mut words = vec![
        40,
        width,
        rows,
        1 | (u32::from(bits) << 16),
        0, // BI_RGB
        0,
        2835,
        2835,
        if bits == 8 { 256 } else { 0 },
        0,
    ];
    if bits == 8 {
        words.extend((0..256u32).map(|g| g | (g << 8) | (g << 16)));
    }
    words
}

/// The rows of a band from the last to the first (for a bottom-up bitmap).
pub fn bottom_up(data: &[u8], stride: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len());
    for row in data.chunks(stride.max(1)).rev() {
        out.extend_from_slice(row);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn papers() -> Vec<WinPaper> {
        vec![
            WinPaper {
                code: 1,
                name: "Letter".into(),
                size: (2159, 2794),
            },
            WinPaper {
                code: 9,
                name: "A4".into(),
                size: (2100, 2970),
            },
        ]
    }

    fn caps() -> WinCaps {
        WinCaps {
            papers: papers(),
            duplex: true,
            max_copies: 99,
            collate: true,
            color: true,
        }
    }

    fn ticket() -> JobTicket {
        JobTicket {
            printer: "P".into(),
            job_name: "J".into(),
            copies: 1,
            collate: true,
            sides: Sides::OneSided,
            media: None,
            gray: false,
            dpi: 300,
            devmode: None,
        }
    }

    const A4: (f32, f32) = (595.28, 841.89);

    #[test]
    fn papers_are_found_by_size_or_given_by_size() {
        assert_eq!(paper_code(A4.0, A4.1, &papers(), None), (9, 0, 0));
        assert_eq!(paper_code(A4.1, A4.0, &papers(), None), (9, 0, 0));
        assert_eq!(paper_code(612.0, 792.0, &papers(), None).0, 1);
        assert_eq!(
            paper_code(300.0, 400.0, &papers(), None),
            (DMPAPER_USER, 1058, 1411)
        );
        // A code chosen in the list wins when it has the size.
        let mut list = papers();
        list.push(WinPaper {
            code: 77,
            name: "A4 (tray 2)".into(),
            size: (2100, 2970),
        });
        assert_eq!(paper_code(A4.0, A4.1, &list, Some(77)).0, 77);
        assert_eq!(paper_code(A4.0, A4.1, &list, Some(1)).0, 9);
    }

    #[test]
    fn fields_of_a_job() {
        let (f, plan) = job_fields(&ticket(), A4, &caps());
        assert_eq!(plan, CopyPlan::Driver);
        assert_eq!(f.orientation, DMORIENT_PORTRAIT);
        assert_eq!(f.paper, 9);
        assert_eq!(f.duplex, DMDUP_SIMPLEX);
        assert_eq!(f.color, DMCOLOR_COLOR);
        assert_eq!(f.fields & (DM_PAPERWIDTH | DM_COLLATE), 0);
        let long = JobTicket {
            sides: Sides::LongEdge,
            gray: true,
            copies: 3,
            ..ticket()
        };
        let (f, plan) = job_fields(&long, A4, &caps());
        assert_eq!(
            (f.duplex, f.color, f.copies, f.collate),
            (DMDUP_VERTICAL, DMCOLOR_MONOCHROME, 3, 1)
        );
        assert_eq!(plan, CopyPlan::Driver);
        assert_ne!(f.fields & DM_COLLATE, 0);
        let short = JobTicket {
            sides: Sides::ShortEdge,
            ..ticket()
        };
        assert_eq!(job_fields(&short, A4, &caps()).0.duplex, DMDUP_HORIZONTAL);
        // A user-sized paper.
        let (f, _) = job_fields(&ticket(), (300.0, 400.0), &caps());
        assert_eq!((f.paper, f.paper_w, f.paper_l), (DMPAPER_USER, 1058, 1411));
        assert_ne!(f.fields & DM_PAPERWIDTH, 0);
    }

    #[test]
    fn copies_the_driver_cannot_make_are_made_here() {
        let many = JobTicket {
            copies: 150,
            ..ticket()
        };
        let (f, plan) = job_fields(&many, A4, &caps());
        assert_eq!(plan, CopyPlan::OwnLoop { collated: true });
        assert_eq!(f.copies, 1);
        let no_collate = WinCaps {
            collate: false,
            ..caps()
        };
        let three = JobTicket {
            copies: 3,
            ..ticket()
        };
        assert_eq!(
            job_fields(&three, A4, &no_collate).1,
            CopyPlan::OwnLoop { collated: true }
        );
        let uncollated = JobTicket {
            copies: 3,
            collate: false,
            ..ticket()
        };
        assert_eq!(job_fields(&uncollated, A4, &no_collate).1, CopyPlan::Driver);
        assert_eq!(side_order(3, 2, CopyPlan::Driver, false), [0, 1, 2]);
        assert_eq!(
            side_order(3, 2, CopyPlan::OwnLoop { collated: true }, false),
            [0, 1, 2, 0, 1, 2]
        );
        assert_eq!(
            side_order(3, 2, CopyPlan::OwnLoop { collated: false }, false),
            [0, 0, 1, 1, 2, 2]
        );
        // Two-sided: each sheet (front and back) repeated; the plan pads to whole sheets.
        assert_eq!(
            side_order(4, 2, CopyPlan::OwnLoop { collated: false }, true),
            [0, 1, 0, 1, 2, 3, 2, 3]
        );
    }

    #[test]
    fn fields_are_written_into_the_bytes_and_read_back() {
        let mut dm = vec![0u8; 220 + 64];
        dm[OFF_FIELDS] = 0x40; // a field of the driver's, kept
        let long = JobTicket {
            sides: Sides::LongEdge,
            copies: 2,
            ..ticket()
        };
        let (f, _) = job_fields(&long, A4, &caps());
        write_fields(&mut dm, &f);
        let back = read_fields(&dm);
        assert_eq!(back.fields, f.fields | 0x40);
        assert_eq!(
            (
                back.orientation,
                back.paper,
                back.copies,
                back.duplex,
                back.color,
                back.collate
            ),
            (1, 9, 2, DMDUP_VERTICAL, DMCOLOR_COLOR, 1)
        );
        assert_eq!(&dm[OFF_PAPER..OFF_PAPER + 2], &[9, 0]);
        // A short buffer is left alone.
        let mut short = vec![0u8; 10];
        write_fields(&mut short, &f);
        assert!(short.iter().all(|b| *b == 0));
    }

    #[test]
    fn the_drivers_choices_come_back_to_the_window() {
        let mut dm = vec![0u8; PUBLIC_SIZE];
        assert_eq!(chosen(&dm), Chosen::default());
        let short = JobTicket {
            sides: Sides::ShortEdge,
            gray: true,
            copies: 3,
            ..ticket()
        };
        let (f, _) = job_fields(&short, A4, &caps());
        write_fields(&mut dm, &f);
        assert_eq!(
            chosen(&dm),
            Chosen {
                sides: Some(Sides::ShortEdge),
                gray: Some(true),
                copies: Some(3),
                paper: Some("dm:9".into()),
            }
        );
        // A paper of the user's size has no code to choose.
        let (f, _) = job_fields(&ticket(), (300.0, 400.0), &caps());
        write_fields(&mut dm, &f);
        assert_eq!(chosen(&dm).paper, None);
    }

    #[test]
    fn bands_meet_without_gaps_or_overlaps() {
        for (dev, r) in [
            (600, 600),
            (720, 600),
            (1200, 600),
            (600, 150),
            (300, 600),
            (1200, 300),
        ] {
            let height = source_pixels(7016, dev, r);
            let mut next = 0;
            let mut top = 0;
            while top < height {
                let rows = 97.min(height - top);
                let (y0, h) = band_dest(top, rows, dev, r);
                assert_eq!(y0, next, "{dev}/{r} at {top}");
                assert!(h >= 0);
                next = y0 + h;
                top += rows;
            }
            assert!(
                (next - 7016).abs() <= (dev / r.min(dev)) as i32 + 1,
                "{dev}/{r}: {next}"
            );
        }
        assert_eq!(source_pixels(4800, 600, 300), 2400);
        assert_eq!(source_pixels(4801, 600, 300), 2401);
    }

    #[test]
    fn band_bitmaps() {
        let rgb = dib_info(100, 20, 24);
        assert_eq!(rgb.len(), 10);
        assert_eq!((rgb[0], rgb[1], rgb[2], rgb[3]), (40, 100, 20, 1 | (24 << 16)));
        let grey = dib_info(100, 20, 8);
        assert_eq!(grey.len(), 10 + 256);
        assert_eq!(grey[8], 256);
        assert_eq!(grey[10 + 128], 0x0080_8080);
        let data = [1u8, 2, 3, 4, 5, 6, 7, 8];
        assert_eq!(bottom_up(&data, 4), [5, 6, 7, 8, 1, 2, 3, 4]);
    }
}
