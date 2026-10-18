//! Files sent by e-mail with the system's mail program: a new message with
//! the files attached (Simple MAPI on Windows, Mail on macOS, `xdg-email` on
//! Linux). When no program takes attachments, an unsent draft (`.eml`) with
//! the files attached is saved and opened with the program that reads e-mail
//! files; only when that fails too, a `mailto:` message opens and the folder
//! of the files is shown, so they can be dragged into it. Nothing is ever sent
//! by Folitecta itself.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// How the message was handed over.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Sent {
    /// The mail program has the message with the files attached.
    Attached,
    /// The message was closed without being sent (Windows tells).
    Cancelled,
    /// A draft with the files attached was saved there and opened.
    Draft(PathBuf),
    /// Only a `mailto:` message could be opened; the folder of the files is shown.
    Folder,
}

/// Open a new message with `files` attached. It may wait until the message
/// window closes (Simple MAPI), so call it away from the interface thread.
pub fn compose(subject: &str, body: &str, files: &[PathBuf]) -> Result<Sent, String> {
    if let Ok(sent) = platform::compose(subject, body, files) {
        return Ok(sent);
    }
    if let Ok(draft) = write_draft(subject, body, files) {
        if open::that_detached(&draft).is_ok() {
            return Ok(Sent::Draft(draft));
        }
    }
    let url = format!("mailto:?subject={}&body={}", encode(subject), encode(body));
    open::that_detached(&url).map_err(|e| e.to_string())?;
    if let Some(dir) = files.first().and_then(|f| f.parent()) {
        let _ = open::that_detached(dir);
    }
    Ok(Sent::Folder)
}

/// Save an unsent draft with `files` attached, in a folder of the outbox.
pub fn write_draft(subject: &str, body: &str, files: &[PathBuf]) -> Result<PathBuf, String> {
    let dir = outbox()?;
    let attached = files
        .iter()
        .map(|f| {
            let name = f
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            std::fs::read(f)
                .map(|data| (name, data))
                .map_err(|e| format!("{}: {e}", f.display()))
        })
        .collect::<Result<Vec<_>, String>>()?;
    let eml = folitecta_core::email::write_eml_draft(subject, body, &attached);
    let stem: String = subject
        .chars()
        .map(|c| {
            if "<>:\"/\\|?*".contains(c) || c.is_control() {
                '_'
            } else {
                c
            }
        })
        .take(60)
        .collect();
    let stem = if stem.trim().is_empty() {
        "message".to_string()
    } else {
        stem.trim().to_string()
    };
    let path = unique_path(&dir, &format!("{stem}.eml"));
    std::fs::write(&path, eml).map_err(|e| e.to_string())?;
    Ok(path)
}

/// Percent-encoding for the parts of a `mailto:` address.
fn encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// A new folder for the files of one message, in the temporary folder.
/// Folders of messages older than a day are removed.
pub fn outbox() -> Result<PathBuf, String> {
    let base = std::env::temp_dir().join("folitecta-mail");
    let day = Duration::from_secs(86_400);
    for entry in std::fs::read_dir(&base).into_iter().flatten().flatten() {
        let old = entry
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| SystemTime::now().duration_since(t).ok())
            .is_some_and(|age| age > day);
        if old {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
    // A folder of its own even for two messages made in the same millisecond.
    static COUNT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let stamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let n = COUNT.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let dir = base.join(format!("{stamp}-{}-{n}", std::process::id()));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

/// `name` in `dir`, numbered when a file of that name is there already.
pub fn unique_path(dir: &Path, name: &str) -> PathBuf {
    let path = dir.join(name);
    if !path.exists() {
        return path;
    }
    let (stem, ext) = match name.rsplit_once('.') {
        Some((s, e)) => (s.to_string(), format!(".{e}")),
        None => (name.to_string(), String::new()),
    };
    (2..)
        .map(|n| dir.join(format!("{stem} ({n}){ext}")))
        .find(|p| !p.exists())
        .unwrap_or(path)
}

#[cfg(target_os = "windows")]
mod platform {
    use std::path::PathBuf;

    use windows::Win32::Foundation::FreeLibrary;
    use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
    use windows::Win32::System::Mapi::{
        LPMAPISENDMAILW, MAPI_DIALOG, MAPI_E_USER_ABORT, MAPI_LOGON_UI, MapiFileDescW, MapiMessageW,
        SUCCESS_SUCCESS,
    };
    use windows::core::{PWSTR, s, w};

    use super::Sent;

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain([0]).collect()
    }

    /// A message of the default mail program through Simple MAPI.
    pub(super) fn compose(subject: &str, body: &str, files: &[PathBuf]) -> Result<Sent, String> {
        let mut subject = wide(subject);
        let mut body = wide(body);
        let mut paths: Vec<Vec<u16>> = files.iter().map(|f| wide(&f.to_string_lossy())).collect();
        let mut names: Vec<Vec<u16>> = files
            .iter()
            .map(|f| wide(&f.file_name().map(|n| n.to_string_lossy()).unwrap_or_default()))
            .collect();
        let mut descs: Vec<MapiFileDescW> = paths
            .iter_mut()
            .zip(names.iter_mut())
            .map(|(p, n)| MapiFileDescW {
                // Not placed in the text of the message.
                nPosition: u32::MAX,
                lpszPathName: PWSTR(p.as_mut_ptr()),
                lpszFileName: PWSTR(n.as_mut_ptr()),
                ..Default::default()
            })
            .collect();
        let message = MapiMessageW {
            lpszSubject: PWSTR(subject.as_mut_ptr()),
            lpszNoteText: PWSTR(body.as_mut_ptr()),
            nFileCount: descs.len() as u32,
            lpFiles: if descs.is_empty() {
                std::ptr::null_mut()
            } else {
                descs.as_mut_ptr()
            },
            ..Default::default()
        };
        // SAFETY: the message and the strings it points to live until the call
        // returns; the function comes from the system's MAPI library.
        unsafe {
            let lib = LoadLibraryW(w!("MAPI32.DLL")).map_err(|e| e.to_string())?;
            let send: LPMAPISENDMAILW = std::mem::transmute(GetProcAddress(lib, s!("MAPISendMailW")));
            let result = match send {
                Some(send) => match send(0, 0, &message, MAPI_DIALOG | MAPI_LOGON_UI, 0) {
                    SUCCESS_SUCCESS => Ok(Sent::Attached),
                    MAPI_E_USER_ABORT => Ok(Sent::Cancelled),
                    code => Err(format!("the mail program answered {code}")),
                },
                None => Err("no mail program takes messages from other programs".into()),
            };
            let _ = FreeLibrary(lib);
            result
        }
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use std::path::PathBuf;

    use super::Sent;

    /// A message of Mail with the files attached.
    pub(super) fn compose(_subject: &str, _body: &str, files: &[PathBuf]) -> Result<Sent, String> {
        let status = std::process::Command::new("open")
            .args(["-a", "Mail"])
            .args(files)
            .status()
            .map_err(|e| e.to_string())?;
        if status.success() {
            Ok(Sent::Attached)
        } else {
            Err("Mail could not open the files".into())
        }
    }
}

#[cfg(target_os = "linux")]
mod platform {
    use std::path::PathBuf;

    use super::Sent;

    /// A message of the desktop's mail program (xdg-utils).
    pub(super) fn compose(subject: &str, body: &str, files: &[PathBuf]) -> Result<Sent, String> {
        let mut cmd = std::process::Command::new("xdg-email");
        cmd.args(["--subject", subject, "--body", body]);
        for f in files {
            cmd.arg("--attach").arg(f);
        }
        let status = cmd.status().map_err(|e| e.to_string())?;
        if status.success() {
            Ok(Sent::Attached)
        } else {
            Err("xdg-email could not open a message".into())
        }
    }
}

#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
mod platform {
    use std::path::PathBuf;

    use super::Sent;

    pub(super) fn compose(_subject: &str, _body: &str, _files: &[PathBuf]) -> Result<Sent, String> {
        Err("no mail program".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mailto_parts_are_encoded() {
        assert_eq!(encode("Report 3 è.pdf"), "Report%203%20%C3%A8.pdf");
        assert_eq!(encode("a&b=c"), "a%26b%3Dc");
    }

    #[test]
    fn drafts_hold_the_files() {
        let dir = outbox().unwrap();
        let file = dir.join("report.pdf");
        std::fs::write(&file, b"%PDF-1.7 report").unwrap();
        let draft = write_draft("Report: October?", "See the file.", std::slice::from_ref(&file)).unwrap();
        assert_eq!(draft.file_name().unwrap(), "Report_ October_.eml");
        let msg = folitecta_core::email::read_email(&std::fs::read(&draft).unwrap()).unwrap();
        assert_eq!(msg.subject, "Report: October?");
        assert_eq!(
            msg.attachments,
            vec![("report.pdf".to_string(), b"%PDF-1.7 report".to_vec())]
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn names_are_numbered_in_the_outbox() {
        let dir = outbox().unwrap();
        let first = unique_path(&dir, "a.pdf");
        std::fs::write(&first, b"x").unwrap();
        assert_eq!(unique_path(&dir, "a.pdf").file_name().unwrap(), "a (2).pdf");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
