//! Windows Image Acquisition: the system's dialogs choose the scanner and
//! scan (its source, colours and resolution), writing one file per page
//! into a folder of this scan, read back here.

use folitecta_core::ScannedPicture;
use windows::Win32::Devices::ImageAcquisition::{IWiaDevMgr2, IWiaItem2, WiaDevMgr2};
use windows::Win32::Foundation::{HWND, S_FALSE};
use windows::Win32::System::Com::{
    CLSCTX_LOCAL_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree,
    CoUninitialize,
};
use windows::core::BSTR;

/// Scan through the system's dialogs (the parent window `window`); an empty
/// list when they were cancelled.
pub fn acquire(window: isize) -> Result<Vec<ScannedPicture>, String> {
    // A thread of its own: the dialogs want a single-threaded apartment.
    std::thread::spawn(move || {
        let dir = std::env::temp_dir().join(format!("folitecta-scan-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let files = unsafe { dialogs(window, &dir) };
        let pictures = files.map(|files| {
            files
                .iter()
                .filter_map(|f| std::fs::read(f).ok())
                .map(|data| ScannedPicture { data, dpi: 0.0 })
                .collect()
        });
        let _ = std::fs::remove_dir_all(&dir);
        pictures
    })
    .join()
    .map_err(|_| "the scanning dialog failed".to_string())?
}

unsafe fn dialogs(window: isize, dir: &std::path::Path) -> Result<Vec<std::path::PathBuf>, String> {
    unsafe {
        let init = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let result = (|| {
            let manager: IWiaDevMgr2 =
                CoCreateInstance(&WiaDevMgr2, None, CLSCTX_LOCAL_SERVER).map_err(|e| e.message())?;
            let parent = HWND(window as *mut core::ffi::c_void);
            let mut device = BSTR::new();
            // Device type 0: any scanner or camera; flags 0.
            manager
                .SelectDeviceDlgID(parent, 0, 0, &mut device)
                .map_err(|e| e.message())?;
            if device.is_empty() {
                return Ok(Vec::new());
            }
            let folder = BSTR::from(dir.to_string_lossy().as_ref());
            let name = BSTR::from("page");
            let mut count = 0i32;
            let mut paths: *mut BSTR = std::ptr::null_mut();
            let mut item: Option<IWiaItem2> = None;
            let done = manager.GetImageDlg(
                0, &device, parent, &folder, &name, &mut count, &mut paths, &mut item,
            );
            let mut files = Vec::new();
            if !paths.is_null() {
                for i in 0..count.max(0) as usize {
                    let p = std::ptr::read(paths.add(i));
                    files.push(std::path::PathBuf::from(p.to_string()));
                }
                CoTaskMemFree(Some(paths as *const core::ffi::c_void));
            }
            match done {
                Ok(()) => Ok(files),
                Err(e) if e.code() == S_FALSE => Ok(files),
                Err(e) => Err(e.message()),
            }
        })();
        if init.is_ok() {
            CoUninitialize();
        }
        result
    }
}
