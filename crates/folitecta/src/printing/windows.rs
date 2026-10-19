//! Printing on Windows through GDI: the printers and their drivers' papers
//! and capabilities, the driver's own settings window, and jobs whose sheets
//! are drawn by the renderer in bands of pixels at the printer's resolution
//! (at most 600 dots per inch) and sent with `StretchDIBits`. The pure parts
//! (DEVMODE fields, copies, band placement, bitmap headers) are in
//! [`super::devmode`] and tested on every system.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use folitecta_core::print::Sides;
use folitecta_core::render::band_rows;
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Gdi::{
    BITMAPINFO, CreateDCW, CreateICW, DEVMODEW, DIB_RGB_COLORS, DeleteDC, GetDeviceCaps, HDC, HORZRES,
    LOGPIXELSX, LOGPIXELSY, PHYSICALHEIGHT, PHYSICALOFFSETX, PHYSICALOFFSETY, PHYSICALWIDTH, RASTERCAPS,
    RC_STRETCHDIB, ResetDCW, SRCCOPY, StretchDIBits, VERTRES,
};
use windows::Win32::Graphics::Printing::{
    ClosePrinter, DocumentPropertiesW, EnumPrintersW, GetDefaultPrinterW, GetPrinterW, OpenPrinterW,
    PRINTER_ENUM_CONNECTIONS, PRINTER_ENUM_LOCAL, PRINTER_HANDLE, PRINTER_INFO_2W, PRINTER_INFO_4W,
    PRINTER_STATUS_ERROR, PRINTER_STATUS_OFFLINE, PRINTER_STATUS_PAUSED, PRINTER_STATUS_PRINTING,
};
use windows::Win32::Storage::Xps::{
    AbortDoc, DC_COLLATE, DC_COLORDEVICE, DC_COPIES, DC_DUPLEX, DC_ENUMRESOLUTIONS, DC_PAPERNAMES, DC_PAPERS,
    DC_PAPERSIZE, DOCINFOW, DeviceCapabilitiesW, EndDoc, EndPage, PRINTER_DEVICE_CAPABILITIES, StartDocW,
    StartPage,
};
use windows::core::{PCWSTR, PWSTR};

use super::devmode::{self, DevFields, WinCaps, WinPaper};
use super::{
    JobOutcome, JobProgress, JobTicket, Media, PrintBackend, PrintError, PrinterCaps, PrinterInfo,
    PrinterState,
};

// The byte offsets the pure functions use are those of the real structure.
const _: () = assert!(std::mem::offset_of!(DEVMODEW, dmFields) == devmode::OFF_FIELDS);
const _: () = assert!(std::mem::offset_of!(DEVMODEW, Anonymous1) == devmode::OFF_ORIENTATION);
const _: () = assert!(std::mem::offset_of!(DEVMODEW, dmColor) == devmode::OFF_COLOR);
const _: () = assert!(std::mem::offset_of!(DEVMODEW, dmDuplex) == devmode::OFF_DUPLEX);
const _: () = assert!(std::mem::offset_of!(DEVMODEW, dmCollate) == devmode::OFF_COLLATE);
const _: () = assert!(std::mem::size_of::<DEVMODEW>() == devmode::PUBLIC_SIZE);

/// Memory a band may use while it is drawn.
const BAND_BUDGET: usize = 96 * 1024 * 1024;
/// `DocumentPropertiesW` modes.
const DM_OUT_BUFFER: u32 = 2;
const DM_IN_PROMPT: u32 = 4;
const DM_IN_BUFFER: u32 = 8;
/// The answer of the driver's window when its settings are kept.
const IDOK: i32 = 1;

/// A null-terminated wide string.
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// The text of a wide string ending with a zero.
fn from_wide(p: PWSTR) -> String {
    if p.is_null() {
        return String::new();
    }
    // SAFETY: the string comes from the spooler and ends with a zero.
    unsafe { p.to_string().unwrap_or_default() }
}

/// The bytes of a DEVMODE, kept aligned for the structure.
struct DevBuf {
    words: Vec<u64>,
    len: usize,
}

impl DevBuf {
    fn new(len: usize) -> Self {
        DevBuf {
            words: vec![0; len.div_ceil(8).max(devmode::PUBLIC_SIZE.div_ceil(8))],
            len,
        }
    }

    fn from_bytes(bytes: &[u8]) -> Self {
        let mut b = DevBuf::new(bytes.len());
        b.bytes_mut()[..bytes.len()].copy_from_slice(bytes);
        b
    }

    fn bytes(&self) -> &[u8] {
        // SAFETY: the words hold at least `len` bytes; u8 has no alignment.
        unsafe { std::slice::from_raw_parts(self.words.as_ptr() as *const u8, self.len) }
    }

    fn bytes_mut(&mut self) -> &mut [u8] {
        // SAFETY: as above, borrowed mutably.
        unsafe { std::slice::from_raw_parts_mut(self.words.as_mut_ptr() as *mut u8, self.len) }
    }

    fn ptr(&self) -> *const DEVMODEW {
        self.words.as_ptr() as *const DEVMODEW
    }

    fn ptr_mut(&mut self) -> *mut DEVMODEW {
        self.words.as_mut_ptr() as *mut DEVMODEW
    }
}

/// An open printer, closed when dropped.
struct Printer(PRINTER_HANDLE);

impl Printer {
    fn open(name: &[u16]) -> Result<Self, PrintError> {
        let mut h = PRINTER_HANDLE::default();
        // SAFETY: `name` ends with a zero; `h` receives the handle.
        unsafe { OpenPrinterW(PCWSTR(name.as_ptr()), &mut h, None) }.map_err(|_| {
            PrintError::NotFound(String::from_utf16_lossy(&name[..name.len().saturating_sub(1)]))
        })?;
        Ok(Printer(h))
    }
}

impl Drop for Printer {
    fn drop(&mut self) {
        // SAFETY: the handle was opened by OpenPrinterW.
        let _ = unsafe { ClosePrinter(self.0) };
    }
}

/// A device context, deleted when dropped.
struct Dc(HDC);

impl Drop for Dc {
    fn drop(&mut self) {
        // SAFETY: the context was created by CreateDCW or CreateICW.
        let _ = unsafe { DeleteDC(self.0) };
    }
}

/// The driver's default settings of a printer (or `current`), merged and
/// checked by the driver.
fn devmode_for(printer: &Printer, name: &[u16], current: Option<&[u8]>) -> Result<DevBuf, PrintError> {
    // SAFETY: a size query (no buffers).
    let size = unsafe { DocumentPropertiesW(None, printer.0, PCWSTR(name.as_ptr()), None, None, 0) };
    if size <= 0 {
        return Err(PrintError::Driver("the printer driver gives no settings".into()));
    }
    let mut out = DevBuf::new(size as usize);
    let input = current
        .filter(|c| c.len() >= devmode::PUBLIC_SIZE)
        .map(DevBuf::from_bytes);
    let mode = if input.is_some() {
        DM_IN_BUFFER | DM_OUT_BUFFER
    } else {
        DM_OUT_BUFFER
    };
    // SAFETY: `out` holds `size` bytes as the driver asked; `input` is a DEVMODE of the same driver.
    let r = unsafe {
        DocumentPropertiesW(
            None,
            printer.0,
            PCWSTR(name.as_ptr()),
            Some(out.ptr_mut()),
            input.as_ref().map(|i| i.ptr()),
            mode,
        )
    };
    if r < 0 {
        return Err(PrintError::Driver(
            "the printer driver refused its settings".into(),
        ));
    }
    Ok(out)
}

/// Apply `fields` to `dm` and let the driver check them.
fn merged(printer: &Printer, name: &[u16], dm: &DevBuf, fields: &DevFields) -> Result<DevBuf, PrintError> {
    let mut input = DevBuf::from_bytes(dm.bytes());
    devmode::write_fields(input.bytes_mut(), fields);
    devmode_for(printer, name, Some(input.bytes()))
}

/// The port of a printer and its state (level 2 information).
fn printer_details(printer: &Printer) -> Option<(String, u32, u32, String)> {
    let mut needed = 0u32;
    // SAFETY: a size query, then a buffer of that size.
    unsafe {
        let _ = GetPrinterW(printer.0, 2, None, &mut needed);
        if needed == 0 {
            return None;
        }
        let mut buf = vec![0u64; (needed as usize).div_ceil(8)];
        let bytes = std::slice::from_raw_parts_mut(buf.as_mut_ptr() as *mut u8, needed as usize);
        GetPrinterW(printer.0, 2, Some(bytes), &mut needed).ok()?;
        let info = &*(buf.as_ptr() as *const PRINTER_INFO_2W);
        Some((
            from_wide(info.pPortName),
            info.Status,
            info.cJobs,
            from_wide(info.pLocation),
        ))
    }
}

fn caps_query(
    name: &[u16],
    port: &[u16],
    cap: PRINTER_DEVICE_CAPABILITIES,
    out: Option<PWSTR>,
    dm: Option<&DevBuf>,
) -> i32 {
    // SAFETY: `out` is null (a count query) or holds the space the count asked for.
    unsafe {
        DeviceCapabilitiesW(
            PCWSTR(name.as_ptr()),
            PCWSTR(port.as_ptr()),
            cap,
            out,
            dm.map(|d| d.ptr()),
        )
    }
}

/// The papers and capabilities the driver reports.
fn win_caps(name: &[u16], port: &[u16], dm: &DevBuf) -> WinCaps {
    let count = caps_query(name, port, DC_PAPERS, None, Some(dm)).max(0) as usize;
    let mut codes = vec![0u16; count];
    let mut names = vec![0u16; count * 64];
    let mut sizes = vec![0i32; count * 2];
    if count > 0 {
        caps_query(name, port, DC_PAPERS, Some(PWSTR(codes.as_mut_ptr())), Some(dm));
        caps_query(
            name,
            port,
            DC_PAPERNAMES,
            Some(PWSTR(names.as_mut_ptr())),
            Some(dm),
        );
        caps_query(
            name,
            port,
            DC_PAPERSIZE,
            Some(PWSTR(sizes.as_mut_ptr() as *mut u16)),
            Some(dm),
        );
    }
    let papers = (0..count)
        .map(|i| {
            let raw = &names[i * 64..i * 64 + 64];
            let end = raw.iter().position(|c| *c == 0).unwrap_or(64);
            WinPaper {
                code: codes[i],
                name: String::from_utf16_lossy(&raw[..end]),
                size: (sizes[i * 2], sizes[i * 2 + 1]),
            }
        })
        .filter(|p| p.size.0 > 0 && p.size.1 > 0)
        .collect();
    WinCaps {
        papers,
        duplex: caps_query(name, port, DC_DUPLEX, None, Some(dm)) == 1,
        max_copies: caps_query(name, port, DC_COPIES, None, Some(dm)).max(1) as u32,
        collate: caps_query(name, port, DC_COLLATE, None, Some(dm)) == 1,
        color: caps_query(name, port, DC_COLORDEVICE, None, Some(dm)) == 1,
    }
}

/// Device measures: printable width and height, dots per inch, offset and
/// size of the paper, in device pixels.
#[derive(Debug, Clone, Copy)]
struct Metrics {
    horz: i32,
    vert: i32,
    dpi_x: i32,
    dpi_y: i32,
    off_x: i32,
    off_y: i32,
    phys_w: i32,
    phys_h: i32,
}

fn metrics(dc: HDC) -> Metrics {
    // SAFETY: a valid device context.
    let cap = |i| unsafe { GetDeviceCaps(Some(dc), i) };
    Metrics {
        horz: cap(HORZRES),
        vert: cap(VERTRES),
        dpi_x: cap(LOGPIXELSX).max(1),
        dpi_y: cap(LOGPIXELSY).max(1),
        off_x: cap(PHYSICALOFFSETX),
        off_y: cap(PHYSICALOFFSETY),
        phys_w: cap(PHYSICALWIDTH),
        phys_h: cap(PHYSICALHEIGHT),
    }
}

/// The margins (points: left, bottom, right, top) the default paper leaves.
fn margins(name: &[u16], dm: &DevBuf) -> ([f32; 4], (u32, u32)) {
    let driver = wide("WINSPOOL");
    // SAFETY: an information context of the printer with its settings.
    let ic = unsafe {
        CreateICW(
            PCWSTR(driver.as_ptr()),
            PCWSTR(name.as_ptr()),
            PCWSTR::null(),
            Some(dm.ptr()),
        )
    };
    if ic.is_invalid() {
        return ([0.0; 4], (600, 600));
    }
    let ic = Dc(ic);
    let m = metrics(ic.0);
    let pt = |v: i32, dpi: i32| (v.max(0) as f32) / dpi as f32 * 72.0;
    (
        [
            pt(m.off_x, m.dpi_x),
            pt(m.phys_h - m.vert - m.off_y, m.dpi_y),
            pt(m.phys_w - m.horz - m.off_x, m.dpi_x),
            pt(m.off_y, m.dpi_y),
        ],
        (m.dpi_x as u32, m.dpi_y as u32),
    )
}

/// The printers of this computer.
pub struct Gdi;

fn state_of(status: u32) -> PrinterState {
    if status & PRINTER_STATUS_OFFLINE != 0 {
        PrinterState::Offline
    } else if status & (PRINTER_STATUS_PAUSED | PRINTER_STATUS_ERROR) != 0 {
        PrinterState::Stopped
    } else if status & PRINTER_STATUS_PRINTING != 0 {
        PrinterState::Printing
    } else {
        PrinterState::Idle
    }
}

fn default_printer() -> Option<String> {
    let mut len = 0u32;
    // SAFETY: a size query, then a buffer of that size.
    unsafe {
        let _ = GetDefaultPrinterW(None, &mut len);
        if len == 0 {
            return None;
        }
        let mut buf = vec![0u16; len as usize];
        if !GetDefaultPrinterW(Some(PWSTR(buf.as_mut_ptr())), &mut len).as_bool() {
            return None;
        }
        let end = buf.iter().position(|c| *c == 0).unwrap_or(buf.len());
        Some(String::from_utf16_lossy(&buf[..end]))
    }
}

impl PrintBackend for Gdi {
    fn printers(&self) -> Result<Vec<PrinterInfo>, PrintError> {
        let flags = PRINTER_ENUM_LOCAL | PRINTER_ENUM_CONNECTIONS;
        let (mut needed, mut count) = (0u32, 0u32);
        // SAFETY: a size query, then a buffer of that size.
        let names: Vec<String> = unsafe {
            let _ = EnumPrintersW(flags, PCWSTR::null(), 4, None, &mut needed, &mut count);
            if needed == 0 {
                return Ok(Vec::new());
            }
            let mut buf = vec![0u64; (needed as usize).div_ceil(8)];
            let bytes = std::slice::from_raw_parts_mut(buf.as_mut_ptr() as *mut u8, needed as usize);
            EnumPrintersW(flags, PCWSTR::null(), 4, Some(bytes), &mut needed, &mut count)
                .map_err(|e| PrintError::Driver(e.to_string()))?;
            let infos = std::slice::from_raw_parts(buf.as_ptr() as *const PRINTER_INFO_4W, count as usize);
            infos.iter().map(|i| from_wide(i.pPrinterName)).collect()
        };
        let default = default_printer();
        Ok(names
            .into_iter()
            .map(|name| {
                let details = Printer::open(&wide(&name)).ok().and_then(|p| printer_details(&p));
                let (status, jobs, location) =
                    details
                        .map(|(_, s, j, l)| (s, j, l))
                        .unwrap_or((0, 0, String::new()));
                PrinterInfo {
                    is_default: default.as_deref() == Some(name.as_str()),
                    label: name.clone(),
                    location,
                    model: String::new(),
                    state: state_of(status),
                    message: String::new(),
                    jobs,
                    accepting: status & PRINTER_STATUS_PAUSED == 0,
                    name,
                }
            })
            .collect())
    }

    fn capabilities(&self, printer: &str) -> Result<PrinterCaps, PrintError> {
        let name = wide(printer);
        let p = Printer::open(&name)?;
        let port = wide(&printer_details(&p).map(|d| d.0).unwrap_or_default());
        let dm = devmode_for(&p, &name, None)?;
        let wc = win_caps(&name, &port, &dm);
        let (margins, dpi) = margins(&name, &dm);
        let current = devmode::read_fields(dm.bytes());
        let res_count = caps_query(&name, &port, DC_ENUMRESOLUTIONS, None, Some(&dm)).max(0) as usize;
        let mut res = vec![0i32; res_count * 2];
        if res_count > 0 {
            caps_query(
                &name,
                &port,
                DC_ENUMRESOLUTIONS,
                Some(PWSTR(res.as_mut_ptr() as *mut u16)),
                Some(&dm),
            );
        }
        let mut resolutions: Vec<(u32, u32)> = res
            .chunks(2)
            .filter(|r| r[0] > 0 && r[1] > 0)
            .map(|r| (r[0] as u32, r[1] as u32))
            .collect();
        if resolutions.is_empty() {
            resolutions.push(dpi);
        }
        let media: Vec<Media> = wc
            .papers
            .iter()
            .map(|p| {
                let pt = |t: i32| t as f32 / 254.0 * 72.0;
                let (w, h) = (pt(p.size.0), pt(p.size.1));
                Media {
                    key: format!("dm:{}", p.code),
                    name: p.name.clone(),
                    width: w.min(h),
                    height: w.max(h),
                    margins,
                }
            })
            .collect();
        let default_media = media
            .iter()
            .find(|m| devmode::media_code(m) == Some(current.paper as u16))
            .map(|m| m.key.clone());
        let mut sides = vec![Sides::OneSided];
        if wc.duplex {
            sides.extend([Sides::LongEdge, Sides::ShortEdge]);
        }
        Ok(PrinterCaps {
            media,
            default_media,
            duplex: wc.duplex,
            default_sides: match current.duplex {
                devmode::DMDUP_VERTICAL if wc.duplex => Sides::LongEdge,
                devmode::DMDUP_HORIZONTAL if wc.duplex => Sides::ShortEdge,
                _ => Sides::OneSided,
            },
            sides,
            max_copies: wc.max_copies.max(999),
            collate: true,
            color: wc.color,
            default_color: wc.color && current.color != devmode::DMCOLOR_MONOCHROME,
            color_modes: Vec::new(),
            resolutions,
            accepts_pdf: true,
            scaling_none: true,
        })
    }

    fn submit(
        &self,
        t: &JobTicket,
        pdf: Arc<Vec<u8>>,
        progress: &dyn Fn(JobProgress),
        cancel: &AtomicBool,
    ) -> Result<JobOutcome, PrintError> {
        let renderer = folitecta_core::Renderer::new(pdf).map_err(|e| PrintError::Io(e.to_string()))?;
        let sizes = renderer.page_sizes();
        if sizes.is_empty() {
            return Err(PrintError::Io("the print file has no sheets".into()));
        }
        let name = wide(&t.printer);
        let p = Printer::open(&name)?;
        let port = wide(&printer_details(&p).map(|d| d.0).unwrap_or_default());
        let base = devmode_for(&p, &name, t.devmode.as_deref())?;
        let wc = win_caps(&name, &port, &base);
        let (fields, plan) = devmode::job_fields(t, sizes[0], &wc);
        let mut dm = merged(&p, &name, &base, &fields)?;
        let driver = wide("WINSPOOL");
        // SAFETY: a device context of the printer with the job's settings.
        let dc = unsafe {
            CreateDCW(
                PCWSTR(driver.as_ptr()),
                PCWSTR(name.as_ptr()),
                PCWSTR::null(),
                Some(dm.ptr()),
            )
        };
        if dc.is_invalid() {
            return Err(PrintError::Driver(
                "the printer cannot be opened for printing".into(),
            ));
        }
        let dc = Dc(dc);
        // SAFETY: a valid device context.
        if unsafe { GetDeviceCaps(Some(dc.0), RASTERCAPS) } as u32 & RC_STRETCHDIB == 0 {
            return Err(PrintError::Driver("the printer cannot receive pictures".into()));
        }
        let doc_name = wide(&t.job_name);
        let info = DOCINFOW {
            cbSize: std::mem::size_of::<DOCINFOW>() as i32,
            lpszDocName: PCWSTR(doc_name.as_ptr()),
            ..Default::default()
        };
        // SAFETY: `info` and the name it points to live through the call.
        let job = unsafe { StartDocW(dc.0, &info) };
        if job <= 0 {
            return Err(PrintError::Driver("the print job was not started".into()));
        }
        let abort = |e: PrintError| {
            // SAFETY: the document was started on this context.
            unsafe { AbortDoc(dc.0) };
            e
        };
        let order = devmode::side_order(sizes.len(), t.copies.max(1), plan, t.sides.duplex());
        let total = order.len();
        let mut paper = sizes[0];
        let per_page = t.media.is_none();
        for (k, &side) in order.iter().enumerate() {
            if cancel.load(Ordering::Relaxed) {
                return Err(abort(PrintError::Cancelled));
            }
            let size = sizes[side];
            if per_page && ((size.0 - paper.0).abs() > 0.5 || (size.1 - paper.1).abs() > 0.5) {
                let (f, _) = devmode::job_fields(t, size, &wc);
                dm = merged(&p, &name, &dm, &f).map_err(abort)?;
                // SAFETY: the new settings for the next page of the same job.
                if unsafe { ResetDCW(dc.0, dm.ptr()) }.is_invalid() {
                    return Err(abort(PrintError::Driver("the paper could not be changed".into())));
                }
                paper = size;
            }
            // SAFETY: a page of the started document.
            if unsafe { StartPage(dc.0) } <= 0 {
                return Err(abort(PrintError::Driver("the page was not started".into())));
            }
            let m = metrics(dc.0);
            let want = if t.dpi == 0 { 600 } else { t.dpi.min(600) };
            let r = (m.dpi_x.min(m.dpi_y) as u32).min(want).max(72);
            let width = devmode::source_pixels(m.horz.max(1) as u32, m.dpi_x as u32, r);
            let height = devmode::source_pixels(m.vert.max(1) as u32, m.dpi_y as u32, r);
            let origin = (
                m.off_x as f32 * r as f32 / m.dpi_x as f32,
                m.off_y as f32 * r as f32 / m.dpi_y as f32,
            );
            let step = band_rows(width, height, BAND_BUDGET);
            let mut top = 0;
            while top < height {
                if cancel.load(Ordering::Relaxed) {
                    return Err(abort(PrintError::Cancelled));
                }
                let rows = step.min(height - top);
                let band = renderer
                    .render_band(side, (r as f32, r as f32), origin, width, top, rows, t.gray)
                    .map_err(|e| abort(PrintError::Io(e.to_string())))?;
                let bits = devmode::bottom_up(&band.data, band.stride);
                let header = devmode::dib_info(band.width, band.rows, band.bits);
                let (y0, h) = devmode::band_dest(top, rows, m.dpi_y as u32, r);
                // SAFETY: `bits` holds `rows` rows of `stride` bytes as `header` says.
                let done = unsafe {
                    StretchDIBits(
                        dc.0,
                        0,
                        y0,
                        m.horz,
                        h,
                        0,
                        0,
                        band.width as i32,
                        band.rows as i32,
                        Some(bits.as_ptr() as *const core::ffi::c_void),
                        header.as_ptr() as *const BITMAPINFO,
                        DIB_RGB_COLORS,
                        SRCCOPY,
                    )
                };
                if done <= 0 {
                    return Err(abort(PrintError::Driver(
                        "the printer did not take a picture".into(),
                    )));
                }
                top += rows;
            }
            // SAFETY: the page started above.
            if unsafe { EndPage(dc.0) } <= 0 {
                return Err(abort(PrintError::Driver("the page was not finished".into())));
            }
            progress(JobProgress::Rendering { sheet: k + 1, total });
        }
        // SAFETY: the document started above.
        if unsafe { EndDoc(dc.0) } <= 0 {
            return Err(PrintError::Driver("the print job was not finished".into()));
        }
        Ok(JobOutcome {
            job_id: Some(job as u32),
            ignored: Vec::new(),
        })
    }

    fn raster(&self) -> bool {
        true
    }
}

/// The driver's own settings window for `printer` (modal, owned by `hwnd`),
/// starting from `current`; the settings chosen, or `None` when cancelled.
pub fn properties(hwnd: isize, printer: &str, current: Option<&[u8]>) -> Result<Option<Vec<u8>>, PrintError> {
    let name = wide(printer);
    let p = Printer::open(&name)?;
    let input = devmode_for(&p, &name, current)?;
    let mut out = DevBuf::new(input.len);
    let owner = HWND(hwnd as *mut core::ffi::c_void);
    // SAFETY: both buffers have the driver's size; the window is modal.
    let r = unsafe {
        DocumentPropertiesW(
            Some(owner),
            p.0,
            PCWSTR(name.as_ptr()),
            Some(out.ptr_mut()),
            Some(input.ptr()),
            DM_IN_BUFFER | DM_OUT_BUFFER | DM_IN_PROMPT,
        )
    };
    Ok((r == IDOK).then(|| out.bytes().to_vec()))
}
