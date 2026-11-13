//! System clipboard: text, PNG images and files.
//!
//! Windows uses the Win32 clipboard, Linux the GTK clipboard (both crates are
//! already part of the desktop stack) and macOS the `pbcopy`/`pbpaste`/`osascript`
//! system tools.

#[cfg(target_os = "windows")]
mod imp {
    use windows::Win32::Foundation::{HANDLE, HGLOBAL};
    use windows::Win32::System::DataExchange::{
        CloseClipboard, EmptyClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard,
        RegisterClipboardFormatW, SetClipboardData,
    };
    use windows::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock};
    use windows::Win32::System::Ole::{CF_DIB, CF_UNICODETEXT};
    use windows::core::w;

    struct Open;
    impl Open {
        fn new() -> Option<Open> {
            for _ in 0..10 {
                if unsafe { OpenClipboard(None) }.is_ok() {
                    return Some(Open);
                }
                std::thread::sleep(std::time::Duration::from_millis(15));
            }
            None
        }
    }
    impl Drop for Open {
        fn drop(&mut self) {
            let _ = unsafe { CloseClipboard() };
        }
    }

    fn png_format() -> u32 {
        unsafe { RegisterClipboardFormatW(w!("PNG")) }
    }

    fn read_bytes(format: u32) -> Option<Vec<u8>> {
        unsafe {
            IsClipboardFormatAvailable(format).ok()?;
            let h = GetClipboardData(format).ok()?;
            let g = HGLOBAL(h.0);
            let size = GlobalSize(g);
            let ptr = GlobalLock(g) as *const u8;
            if ptr.is_null() || size == 0 {
                return None;
            }
            let data = std::slice::from_raw_parts(ptr, size).to_vec();
            let _ = GlobalUnlock(g);
            Some(data)
        }
    }

    fn write_bytes(format: u32, data: &[u8]) -> Result<(), String> {
        unsafe {
            let g = GlobalAlloc(GMEM_MOVEABLE, data.len().max(1)).map_err(|e| e.to_string())?;
            let ptr = GlobalLock(g) as *mut u8;
            if ptr.is_null() {
                return Err("cannot lock clipboard memory".into());
            }
            std::ptr::copy_nonoverlapping(data.as_ptr(), ptr, data.len());
            let _ = GlobalUnlock(g);
            SetClipboardData(format, Some(HANDLE(g.0))).map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    pub fn get_text() -> Option<String> {
        let _open = Open::new()?;
        let bytes = read_bytes(CF_UNICODETEXT.0 as u32)?;
        let (pairs, _) = bytes.as_chunks::<2>();
        let wide: Vec<u16> = pairs
            .iter()
            .map(|c| u16::from_le_bytes(*c))
            .take_while(|c| *c != 0)
            .collect();
        Some(String::from_utf16_lossy(&wide))
    }

    pub fn set_text(text: &str) -> Result<(), String> {
        let _open = Open::new().ok_or("the clipboard is busy")?;
        unsafe { EmptyClipboard() }.map_err(|e| e.to_string())?;
        let mut wide: Vec<u8> = text.encode_utf16().flat_map(|c| c.to_le_bytes()).collect();
        wide.extend_from_slice(&[0, 0]);
        write_bytes(CF_UNICODETEXT.0 as u32, &wide)
    }

    pub fn get_png() -> Option<Vec<u8>> {
        let _open = Open::new()?;
        if let Some(png) = read_bytes(png_format()) {
            if png.starts_with(&[0x89, b'P', b'N', b'G']) {
                return Some(png);
            }
        }
        // CF_DIB: prepend a BITMAPFILEHEADER and decode as BMP.
        let dib = read_bytes(CF_DIB.0 as u32)?;
        if dib.len() < 40 {
            return None;
        }
        let header_size = u32::from_le_bytes([dib[0], dib[1], dib[2], dib[3]]) as usize;
        let bit_count = u16::from_le_bytes([dib[14], dib[15]]);
        let compression = u32::from_le_bytes([dib[16], dib[17], dib[18], dib[19]]);
        let colors_used = u32::from_le_bytes([dib[32], dib[33], dib[34], dib[35]]) as usize;
        let palette = if bit_count <= 8 {
            (if colors_used == 0 {
                1usize << bit_count
            } else {
                colors_used
            }) * 4
        } else {
            0
        };
        let masks = if compression == 3 && header_size == 40 {
            12
        } else {
            0
        };
        let offset = 14 + header_size + masks + palette;
        let mut bmp = Vec::with_capacity(dib.len() + 14);
        bmp.extend_from_slice(b"BM");
        bmp.extend_from_slice(&((dib.len() + 14) as u32).to_le_bytes());
        bmp.extend_from_slice(&[0, 0, 0, 0]);
        bmp.extend_from_slice(&(offset as u32).to_le_bytes());
        bmp.extend_from_slice(&dib);
        let img = image::load_from_memory_with_format(&bmp, image::ImageFormat::Bmp).ok()?;
        let mut out = std::io::Cursor::new(Vec::new());
        img.write_to(&mut out, image::ImageFormat::Png).ok()?;
        Some(out.into_inner())
    }

    /// `CF_HDROP`: a `DROPFILES` header (20 bytes, wide names) and the
    /// paths, each ended by a zero, the list by another.
    const CF_HDROP: u32 = 15;

    pub fn set_files(paths: &[std::path::PathBuf]) -> Result<(), String> {
        let mut data = Vec::new();
        data.extend_from_slice(&20u32.to_le_bytes());
        data.extend_from_slice(&[0u8; 12]);
        data.extend_from_slice(&1u32.to_le_bytes());
        for p in paths {
            for c in p.to_string_lossy().encode_utf16() {
                data.extend_from_slice(&c.to_le_bytes());
            }
            data.extend_from_slice(&[0, 0]);
        }
        data.extend_from_slice(&[0, 0]);
        let _open = Open::new().ok_or("the clipboard is busy")?;
        unsafe { EmptyClipboard() }.map_err(|e| e.to_string())?;
        write_bytes(CF_HDROP, &data)
    }

    pub fn get_files() -> Vec<std::path::PathBuf> {
        let Some(_open) = Open::new() else {
            return Vec::new();
        };
        let Some(data) = read_bytes(CF_HDROP) else {
            return Vec::new();
        };
        if data.len() < 20 {
            return Vec::new();
        }
        let at = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;
        let wide = u32::from_le_bytes([data[16], data[17], data[18], data[19]]) != 0;
        let rest = data.get(at..).unwrap_or_default();
        let names: Vec<String> = if wide {
            let (pairs, _) = rest.as_chunks::<2>();
            let units: Vec<u16> = pairs.iter().map(|c| u16::from_le_bytes(*c)).collect();
            units
                .split(|c| *c == 0)
                .take_while(|n| !n.is_empty())
                .map(String::from_utf16_lossy)
                .collect()
        } else {
            rest.split(|c| *c == 0)
                .take_while(|n| !n.is_empty())
                .map(|n| String::from_utf8_lossy(n).to_string())
                .collect()
        };
        names.into_iter().map(std::path::PathBuf::from).collect()
    }

    pub fn set_png(png: &[u8]) -> Result<(), String> {
        let img = image::load_from_memory(png)
            .map_err(|e| e.to_string())?
            .to_rgba8();
        let (w, h) = (img.width(), img.height());
        // 32-bit bottom-up DIB (BGRA) for applications that do not read PNG.
        let mut dib = Vec::with_capacity(40 + (w * h * 4) as usize);
        dib.extend_from_slice(&40u32.to_le_bytes());
        dib.extend_from_slice(&(w as i32).to_le_bytes());
        dib.extend_from_slice(&(h as i32).to_le_bytes());
        dib.extend_from_slice(&1u16.to_le_bytes());
        dib.extend_from_slice(&32u16.to_le_bytes());
        dib.extend_from_slice(&0u32.to_le_bytes());
        dib.extend_from_slice(&(w * h * 4).to_le_bytes());
        dib.extend_from_slice(&[0u8; 16]);
        for y in (0..h).rev() {
            for x in 0..w {
                let p = img.get_pixel(x, y);
                dib.extend_from_slice(&[p[2], p[1], p[0], p[3]]);
            }
        }
        let _open = Open::new().ok_or("the clipboard is busy")?;
        unsafe { EmptyClipboard() }.map_err(|e| e.to_string())?;
        write_bytes(png_format(), png)?;
        write_bytes(CF_DIB.0 as u32, &dib)
    }
}

#[cfg(target_os = "linux")]
mod imp {
    use gtk::gdk;
    use gtk::gdk_pixbuf::{Pixbuf, PixbufLoader};
    use gtk::prelude::*;

    fn clipboard() -> gtk::Clipboard {
        gtk::Clipboard::get(&gdk::SELECTION_CLIPBOARD)
    }

    pub fn get_text() -> Option<String> {
        clipboard().wait_for_text().map(|s| s.to_string())
    }

    pub fn set_text(text: &str) -> Result<(), String> {
        let c = clipboard();
        c.set_text(text);
        c.store();
        Ok(())
    }

    pub fn get_png() -> Option<Vec<u8>> {
        let pixbuf: Pixbuf = clipboard().wait_for_image()?;
        pixbuf.save_to_bufferv("png", &[]).ok()
    }

    /// The files as `text/uri-list`, as the file managers' own list
    /// (copied, not cut) and as their paths in text.
    pub fn set_files(paths: &[std::path::PathBuf]) -> Result<(), String> {
        let uris: Vec<String> = paths
            .iter()
            .filter_map(|p| gtk::glib::filename_to_uri(p, None).ok().map(|u| u.to_string()))
            .collect();
        if uris.is_empty() {
            return Err("no file to copy".into());
        }
        let text = paths
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join("\n");
        let targets = [
            gtk::TargetEntry::new("text/uri-list", gtk::TargetFlags::empty(), 0),
            gtk::TargetEntry::new("x-special/gnome-copied-files", gtk::TargetFlags::empty(), 1),
            gtk::TargetEntry::new("UTF8_STRING", gtk::TargetFlags::empty(), 2),
            gtk::TargetEntry::new("text/plain;charset=utf-8", gtk::TargetFlags::empty(), 2),
        ];
        let c = clipboard();
        let ok = c.set_with_data(&targets, move |_, sel, info| match info {
            0 => {
                let refs: Vec<&str> = uris.iter().map(String::as_str).collect();
                sel.set_uris(&refs);
            }
            1 => {
                let body = format!("copy\n{}", uris.join("\n"));
                sel.set(&sel.target(), 8, body.as_bytes());
            }
            _ => {
                sel.set_text(&text);
            }
        });
        if !ok {
            return Err("the clipboard did not take the files".into());
        }
        c.store();
        Ok(())
    }

    pub fn get_files() -> Vec<std::path::PathBuf> {
        clipboard()
            .wait_for_uris()
            .into_iter()
            .filter_map(|u| gtk::glib::filename_from_uri(&u).ok().map(|(p, _)| p))
            .collect()
    }

    pub fn set_png(png: &[u8]) -> Result<(), String> {
        let loader = PixbufLoader::new();
        loader.write(png).map_err(|e| e.to_string())?;
        loader.close().map_err(|e| e.to_string())?;
        let pixbuf = loader.pixbuf().ok_or("cannot decode image")?;
        let c = clipboard();
        c.set_image(&pixbuf);
        c.store();
        Ok(())
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use std::io::Write;
    use std::process::{Command, Stdio};

    pub fn get_text() -> Option<String> {
        let out = Command::new("pbpaste").output().ok()?;
        out.status
            .success()
            .then(|| String::from_utf8_lossy(&out.stdout).to_string())
    }

    pub fn set_text(text: &str) -> Result<(), String> {
        let mut child = Command::new("pbcopy")
            .stdin(Stdio::piped())
            .spawn()
            .map_err(|e| e.to_string())?;
        child
            .stdin
            .as_mut()
            .ok_or("pbcopy has no input")?
            .write_all(text.as_bytes())
            .map_err(|e| e.to_string())?;
        child.wait().map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn get_png() -> Option<Vec<u8>> {
        let out = Command::new("osascript")
            .args(["-e", "the clipboard as «class PNGf»"])
            .output()
            .ok()?;
        if !out.status.success() {
            return None;
        }
        // Output looks like «data PNGf89504E47...»
        let s = String::from_utf8_lossy(&out.stdout);
        let hex: String = s
            .split("PNGf")
            .nth(1)?
            .chars()
            .take_while(|c| c.is_ascii_hexdigit())
            .collect();
        let bytes: Option<Vec<u8>> = (0..hex.len() / 2)
            .map(|i| u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).ok())
            .collect();
        bytes.filter(|b| b.starts_with(&[0x89, b'P', b'N', b'G']))
    }

    pub fn set_files(paths: &[std::path::PathBuf]) -> Result<(), String> {
        let files: Vec<String> = paths
            .iter()
            .map(|p| {
                format!(
                    "POSIX file \"{}\"",
                    p.display().to_string().replace('\\', "\\\\").replace('"', "\\\"")
                )
            })
            .collect();
        let script = if files.len() == 1 {
            format!("set the clipboard to ({})", files[0])
        } else {
            format!("set the clipboard to {{{}}}", files.join(", "))
        };
        let status = Command::new("osascript")
            .args(["-e", &script])
            .status()
            .map_err(|e| e.to_string())?;
        if status.success() {
            Ok(())
        } else {
            Err("osascript could not set the clipboard".into())
        }
    }

    pub fn get_files() -> Vec<std::path::PathBuf> {
        let Ok(out) = Command::new("osascript")
            .args(["-e", "POSIX path of (the clipboard as «class furl»)"])
            .output()
        else {
            return Vec::new();
        };
        let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if out.status.success() && !path.is_empty() {
            vec![std::path::PathBuf::from(path)]
        } else {
            Vec::new()
        }
    }

    pub fn set_png(png: &[u8]) -> Result<(), String> {
        let path = std::env::temp_dir().join(format!("folitecta-clip-{}.png", std::process::id()));
        std::fs::write(&path, png).map_err(|e| e.to_string())?;
        let script = format!(
            "set the clipboard to (read (POSIX file \"{}\") as «class PNGf»)",
            path.display()
        );
        let status = Command::new("osascript")
            .args(["-e", &script])
            .status()
            .map_err(|e| e.to_string())?;
        let _ = std::fs::remove_file(&path);
        if status.success() {
            Ok(())
        } else {
            Err("osascript could not set the clipboard".into())
        }
    }
}

#[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
mod imp {
    pub fn get_text() -> Option<String> {
        None
    }
    pub fn set_text(_: &str) -> Result<(), String> {
        Err("clipboard not supported on this platform".into())
    }
    pub fn get_png() -> Option<Vec<u8>> {
        None
    }
    pub fn set_png(_: &[u8]) -> Result<(), String> {
        Err("clipboard not supported on this platform".into())
    }
    pub fn set_files(_: &[std::path::PathBuf]) -> Result<(), String> {
        Err("clipboard not supported on this platform".into())
    }
    pub fn get_files() -> Vec<std::path::PathBuf> {
        Vec::new()
    }
}

/// Clipboard text, if any.
pub fn get_text() -> Option<String> {
    imp::get_text()
}

/// Put text on the clipboard (errors are ignored by most callers).
pub fn set_text(text: &str) {
    let _ = imp::set_text(text);
}

/// Clipboard image as PNG bytes, if any.
pub fn get_png() -> Option<Vec<u8>> {
    imp::get_png()
}

/// Put a PNG image on the clipboard.
pub fn set_png(png: &[u8]) -> Result<(), String> {
    imp::set_png(png)
}

/// Put files on the clipboard (as the file managers copy them).
pub fn set_files(paths: &[std::path::PathBuf]) -> Result<(), String> {
    imp::set_files(paths)
}

/// The files on the clipboard (copied in a file manager), if any.
pub fn get_files() -> Vec<std::path::PathBuf> {
    imp::get_files()
}
