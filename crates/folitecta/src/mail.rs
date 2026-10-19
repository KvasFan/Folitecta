//! The files of an e-mail message: written into a private folder and handed
//! to the system's mail program (Simple MAPI on Windows, Mail on macOS,
//! `xdg-email` on Linux), or saved with the message as an unsent draft
//! (`.eml`) where the user chooses. Nothing is ever sent by Folitecta, and a
//! mail program that is missing or cannot take the files is reported as such:
//! nothing else is opened in its place.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

/// How the message was handed over.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sent {
    /// The mail program confirmed it took the message with the files
    /// (Windows: the message was sent or saved from its window).
    Attached,
    /// A new message opened in the mail program; whether it holds the files
    /// cannot be checked from here.
    Opened,
    /// The message was closed without being sent (only Windows tells).
    Cancelled,
}

/// Why the mail program did not take the message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HandOverError {
    /// No mail program takes messages from other programs.
    NoProgram(String),
    /// The mail program (or what handles e-mail links) cannot take the files.
    Attachments(String),
    /// The mail program answered with an error.
    Refused(String),
}

/// Programs started by this module (mail programs, folders and drafts opened
/// with the system): the self-test checks it does not change.
static LAUNCHES: AtomicUsize = AtomicUsize::new(0);

pub fn launches() -> usize {
    LAUNCHES.load(Ordering::SeqCst)
}

/// A hand-over that stands for the mail program, so tests and the self-test
/// never start one.
pub type FakeHandOver = fn(&str, &str, &[PathBuf]) -> Result<Sent, HandOverError>;

static FAKE: Mutex<Option<FakeHandOver>> = Mutex::new(None);

/// Use `fake` instead of the mail program (and open nothing with the system)
/// until it is set back to `None`.
pub fn set_fake(fake: Option<FakeHandOver>) {
    *FAKE.lock().unwrap_or_else(|e| e.into_inner()) = fake;
}

fn fake() -> Option<FakeHandOver> {
    *FAKE.lock().unwrap_or_else(|e| e.into_inner())
}

/// Open a new message of the mail program with `files` attached. It may
/// wait until the message window closes (Simple MAPI), so call it away from
/// the interface thread.
pub fn hand_over(subject: &str, body: &str, files: &[PathBuf]) -> Result<Sent, HandOverError> {
    for f in files {
        if !f.is_file() || std::fs::File::open(f).is_err() {
            return Err(HandOverError::Attachments(f.display().to_string()));
        }
    }
    if let Some(fake) = fake() {
        return fake(subject, body, files);
    }
    LAUNCHES.fetch_add(1, Ordering::SeqCst);
    platform::compose(subject, body, files)
}

/// Open a file or a folder with the program the system has for it.
pub fn open_with_system(target: &Path) -> Result<(), String> {
    if fake().is_some() {
        return Ok(());
    }
    LAUNCHES.fetch_add(1, Ordering::SeqCst);
    crate::util::open_external(&target.to_string_lossy())
}

/// The name a file of the message gets in the folder handed to the mail
/// program: characters that some mail programs read as separators or quotes
/// in their list of attachments (`'`, `,`, `%`, `\`, `;`, `"`, line breaks)
/// are replaced. The names in drafts and archives stay as typed.
pub fn handover_name(name: &str) -> String {
    name.chars()
        .map(|c| match c {
            '\'' => '\u{2019}',
            ',' | '%' | '\\' | ';' | '"' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect()
}

/// A new folder for the files of one message in the temporary folder, made
/// for this message (never one that was there) and open to this user only.
/// Folders of earlier messages of this user older than a day are removed.
pub fn outbox() -> Result<PathBuf, String> {
    outbox_in(&std::env::temp_dir())
}

const OUTBOX_PREFIX: &str = "folitecta-mail-";

/// A number nobody else can guess (the random keys of the standard hasher).
fn random() -> u64 {
    use std::hash::{BuildHasher, Hasher};
    static COUNT: AtomicU64 = AtomicU64::new(0);
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u128(
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0),
    );
    h.write_u32(std::process::id());
    h.write_u64(COUNT.fetch_add(1, Ordering::SeqCst));
    h.finish()
}

fn make_private_dir(path: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        std::fs::DirBuilder::new().mode(0o700).create(path)
    }
    #[cfg(not(unix))]
    {
        std::fs::create_dir(path)
    }
}

/// The owner of a file (the user id; 0 where the system has none).
fn owner(meta: &std::fs::Metadata) -> u32 {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        meta.uid()
    }
    #[cfg(not(unix))]
    {
        let _ = meta;
        0
    }
}

pub(crate) fn outbox_in(base: &Path) -> Result<PathBuf, String> {
    let mut made = None;
    for _ in 0..16 {
        let path = base.join(format!("{OUTBOX_PREFIX}{:016x}", random()));
        match make_private_dir(&path) {
            Ok(()) => {
                made = Some(path);
                break;
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(format!("{}: {e}", path.display())),
        }
    }
    let dir = made.ok_or_else(|| format!("{}: no new folder could be made", base.display()))?;
    let me = std::fs::symlink_metadata(&dir).map(|m| owner(&m)).ok();
    let day = Duration::from_secs(86_400);
    for entry in std::fs::read_dir(base).into_iter().flatten().flatten() {
        let path = entry.path();
        if path == dir || !entry.file_name().to_string_lossy().starts_with(OUTBOX_PREFIX) {
            continue;
        }
        let Ok(meta) = std::fs::symlink_metadata(&path) else {
            continue;
        };
        let old = meta
            .modified()
            .ok()
            .and_then(|t| SystemTime::now().duration_since(t).ok())
            .is_some_and(|age| age > day);
        if old && meta.is_dir() && Some(owner(&meta)) == me {
            let _ = std::fs::remove_dir_all(&path);
        }
    }
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

/// One file of a message.
#[derive(Debug, Clone, PartialEq)]
pub struct Outgoing {
    pub name: String,
    pub bytes: Arc<Vec<u8>>,
    /// A PDF made by Folitecta: checked to be whole before it is written.
    pub made_here: bool,
}

/// Write the files of a message into `dir` (atomically, each under its
/// hand-over name, numbered when two have one name).
pub fn write_files(dir: &Path, files: &[Outgoing]) -> Result<Vec<PathBuf>, String> {
    files
        .iter()
        .map(|f| {
            let path = unique_path(dir, &handover_name(&f.name));
            let written = if f.made_here {
                folitecta_core::document::write_pdf_file(&path, &f.bytes).map_err(|e| e.to_string())
            } else {
                folitecta_core::util::write_atomic(&path, &f.bytes)
                    .map_err(|e| format!("{}: {e}", path.display()))
            };
            written.map(|()| path)
        })
        .collect()
}

/// The file name of a draft: the subject made safe (60 characters at most),
/// or `message`.
pub fn draft_file_name(subject: &str) -> String {
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
    let stem = stem.trim().trim_end_matches('.').trim();
    format!("{}.eml", if stem.is_empty() { "message" } else { stem })
}

/// Save an unsent draft (`X-Unsent: 1`) with `files` attached at `path`,
/// replacing it only once the new file is complete.
pub fn write_draft(path: &Path, subject: &str, body: &str, files: &[Outgoing]) -> Result<(), String> {
    let parts: Vec<(String, &[u8])> = files
        .iter()
        .map(|f| (f.name.clone(), f.bytes.as_slice()))
        .collect();
    let eml = folitecta_core::email::write_eml_draft(subject, body, &parts);
    folitecta_core::util::write_atomic(path, &eml).map_err(|e| format!("{}: {e}", path.display()))
}

/// What a Simple MAPI answer means (the codes of `MAPISendMailW`).
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub(crate) fn mapi_result(code: u32) -> Result<Sent, HandOverError> {
    match code {
        0 => Ok(Sent::Attached),
        1 => Ok(Sent::Cancelled),
        // Too many files; an attachment not found, not opened, not written, too large.
        9 | 11 | 12 | 13 | 28 => Err(HandOverError::Attachments(format!("MAPI {code}"))),
        // Logon failure, not supported, no Unicode.
        3 | 26 | 27 => Err(HandOverError::NoProgram(format!("MAPI {code}"))),
        _ => Err(HandOverError::Refused(format!("MAPI {code}"))),
    }
}

/// What an exit code of `xdg-email` means (xdg-utils: 2 a file is missing,
/// 3 no program for the action, 5 a file cannot be read).
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(crate) fn xdg_result(code: Option<i32>) -> Result<Sent, HandOverError> {
    match code {
        Some(0) => Ok(Sent::Opened),
        Some(c @ (2 | 5)) => Err(HandOverError::Attachments(format!("xdg-email {c}"))),
        Some(3) => Err(HandOverError::NoProgram("xdg-email 3".into())),
        Some(c) => Err(HandOverError::Refused(format!("xdg-email {c}"))),
        None => Err(HandOverError::Refused("xdg-email".into())),
    }
}

/// The arguments of `xdg-email` for a message: UTF-8 text, the subject and
/// the message only when they are not empty (an empty one makes it fail),
/// and the files.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(crate) fn xdg_email_args(subject: &str, body: &str, files: &[PathBuf]) -> Vec<std::ffi::OsString> {
    let mut args: Vec<std::ffi::OsString> = vec!["--utf8".into()];
    for (flag, text) in [("--subject", subject), ("--body", body)] {
        if !text.is_empty() {
            args.push(flag.into());
            args.push(text.into());
        }
    }
    for f in files {
        args.push("--attach".into());
        args.push(f.into());
    }
    args
}

/// Whether the program that handles e-mail links (its desktop file id and
/// the categories of that file) is a web browser, which drops attachments.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(crate) fn is_web_browser(desktop_id: &str, categories: &str) -> bool {
    if categories.split(';').any(|c| c.trim() == "WebBrowser") {
        return true;
    }
    let id = desktop_id.to_ascii_lowercase();
    [
        "firefox",
        "chrom",
        "brave",
        "vivaldi",
        "opera",
        "edge",
        "epiphany",
        "falkon",
        "librewolf",
        "qutebrowser",
        "konqueror",
        "midori",
        "waterfox",
        "floorp",
    ]
    .iter()
    .any(|b| id.contains(b))
}

#[cfg(target_os = "windows")]
mod platform {
    use std::path::PathBuf;

    use windows::Win32::Foundation::FreeLibrary;
    use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
    use windows::Win32::System::Mapi::{
        LPMAPISENDMAILW, MAPI_DIALOG, MAPI_LOGON_UI, MapiFileDescW, MapiMessageW,
    };
    use windows::core::{PWSTR, s, w};

    use super::{HandOverError, Sent, mapi_result};

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain([0]).collect()
    }

    /// A message of the default mail program through Simple MAPI.
    pub(super) fn compose(subject: &str, body: &str, files: &[PathBuf]) -> Result<Sent, HandOverError> {
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
            let lib = LoadLibraryW(w!("MAPI32.DLL")).map_err(|e| HandOverError::NoProgram(e.to_string()))?;
            let send: LPMAPISENDMAILW = std::mem::transmute(GetProcAddress(lib, s!("MAPISendMailW")));
            let result = match send {
                Some(send) => mapi_result(send(0, 0, &message, MAPI_DIALOG | MAPI_LOGON_UI, 0)),
                None => Err(HandOverError::NoProgram("MAPISendMailW".into())),
            };
            let _ = FreeLibrary(lib);
            result
        }
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use std::path::PathBuf;

    use super::{HandOverError, Sent};

    /// A message of Mail with the files attached (Mail takes neither the
    /// subject nor the message this way).
    pub(super) fn compose(_subject: &str, _body: &str, files: &[PathBuf]) -> Result<Sent, HandOverError> {
        let status = std::process::Command::new("open")
            .args(["-a", "Mail"])
            .args(files)
            .status()
            .map_err(|e| HandOverError::NoProgram(format!("open: {e}")))?;
        if status.success() {
            Ok(Sent::Opened)
        } else {
            Err(HandOverError::NoProgram("Mail".into()))
        }
    }
}

#[cfg(target_os = "linux")]
mod platform {
    use std::path::{Path, PathBuf};
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    use super::{HandOverError, Sent, is_web_browser, xdg_email_args, xdg_result};

    /// The categories of the desktop file `id` of an installed program.
    fn categories(id: &str) -> String {
        let mut dirs: Vec<PathBuf> = Vec::new();
        match std::env::var_os("XDG_DATA_HOME") {
            Some(d) => dirs.push(PathBuf::from(d)),
            None => dirs.extend(std::env::var_os("HOME").map(|h| Path::new(&h).join(".local/share"))),
        }
        let system = std::env::var("XDG_DATA_DIRS").unwrap_or_else(|_| "/usr/local/share:/usr/share".into());
        dirs.extend(system.split(':').filter(|d| !d.is_empty()).map(PathBuf::from));
        for d in dirs {
            if let Ok(text) = std::fs::read_to_string(d.join("applications").join(id)) {
                return text
                    .lines()
                    .find_map(|l| l.strip_prefix("Categories="))
                    .unwrap_or_default()
                    .to_string();
            }
        }
        String::new()
    }

    /// A message of the desktop's mail program (xdg-utils). The program that
    /// handles e-mail links is checked first: none, or a web browser (which
    /// would drop the files), is reported instead of being opened.
    pub(super) fn compose(subject: &str, body: &str, files: &[PathBuf]) -> Result<Sent, HandOverError> {
        compose_with(None, Duration::from_secs(8), subject, body, files)
    }

    /// [`compose`] with the programs looked for in `path` (the folders of
    /// the system when `None`) and waiting at most `wait` for `xdg-email`.
    pub(super) fn compose_with(
        path: Option<&std::ffi::OsStr>,
        wait: Duration,
        subject: &str,
        body: &str,
        files: &[PathBuf],
    ) -> Result<Sent, HandOverError> {
        let command = |name: &str| {
            let mut c = Command::new(name);
            if let Some(p) = path {
                c.env("PATH", p);
            }
            c
        };
        if let Ok(out) = command("xdg-mime")
            .args(["query", "default", "x-scheme-handler/mailto"])
            .stderr(Stdio::null())
            .output()
        {
            if out.status.success() {
                let id = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if id.is_empty() {
                    return Err(HandOverError::NoProgram("x-scheme-handler/mailto".into()));
                }
                if is_web_browser(&id, &categories(&id)) {
                    return Err(HandOverError::Attachments(id));
                }
            }
        }
        let mut child = command("xdg-email")
            .args(xdg_email_args(subject, body, files))
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| HandOverError::NoProgram(format!("xdg-email: {e}")))?;
        // Some mail programs keep xdg-email waiting until they are closed:
        // after a few seconds the message counts as opened.
        let started = Instant::now();
        loop {
            match child.try_wait() {
                Ok(Some(status)) => return xdg_result(status.code()),
                Ok(None) if started.elapsed() < wait => {
                    std::thread::sleep(Duration::from_millis(100));
                }
                Ok(None) => {
                    std::thread::spawn(move || {
                        let _ = child.wait();
                    });
                    return Ok(Sent::Opened);
                }
                Err(e) => return Err(HandOverError::Refused(format!("xdg-email: {e}"))),
            }
        }
    }
}

#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
mod platform {
    use std::path::PathBuf;

    use super::{HandOverError, Sent};

    pub(super) fn compose(_subject: &str, _body: &str, _files: &[PathBuf]) -> Result<Sent, HandOverError> {
        Err(HandOverError::NoProgram(std::env::consts::OS.into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn outgoing(name: &str, bytes: &[u8], made_here: bool) -> Outgoing {
        Outgoing {
            name: name.into(),
            bytes: Arc::new(bytes.to_vec()),
            made_here,
        }
    }

    fn base(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("folitecta-mailtest-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[cfg(unix)]
    #[test]
    fn the_outbox_is_private_and_new() {
        use std::os::unix::fs::PermissionsExt;
        let root = base("private");
        // A folder of the older shared kind that cannot be written does not matter.
        let shared = root.join("folitecta-mail");
        std::fs::create_dir(&shared).unwrap();
        std::fs::set_permissions(&shared, std::fs::Permissions::from_mode(0o500)).unwrap();
        let a = outbox_in(&root).unwrap();
        let b = outbox_in(&root).unwrap();
        assert_ne!(a, b);
        assert_eq!(a.parent(), Some(root.as_path()));
        let mode = std::fs::metadata(&a).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o700);
        // Old folders of this user go, other folders stay.
        let old = root.join(format!("{OUTBOX_PREFIX}old"));
        std::fs::create_dir(&old).unwrap();
        let other = root.join("someone-else");
        std::fs::create_dir(&other).unwrap();
        let long_ago = SystemTime::now() - Duration::from_secs(3 * 86_400);
        for d in [&old, &other] {
            std::fs::File::open(d).unwrap().set_modified(long_ago).unwrap();
        }
        outbox_in(&root).unwrap();
        assert!(!old.exists());
        assert!(other.exists() && a.exists() && b.exists());
        std::fs::set_permissions(&shared, std::fs::Permissions::from_mode(0o700)).unwrap();
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn files_are_written_whole_and_numbered() {
        let dir = outbox_in(&base("files")).unwrap();
        let pdf = b"%PDF-1.7\n1 0 obj\n<<>>\nendobj\ntrailer\n<<>>\n%%EOF\n";
        let tail = [b"%PDF-1.4\n%%EOF\n".as_slice(), &[b'x'; 2048]].concat();
        let files = [
            outgoing("a.pdf", pdf, true),
            // A saved file goes as it is, even with data after its end.
            outgoing("a.pdf", &tail, false),
            outgoing("dell'ufficio, 50%.pdf", b"x", false),
        ];
        let paths = write_files(&dir, &files).unwrap();
        let names: Vec<String> = paths
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
            .collect();
        assert_eq!(names, ["a.pdf", "a (2).pdf", "dell\u{2019}ufficio_ 50_.pdf"]);
        assert_eq!(std::fs::read(&paths[0]).unwrap(), pdf);
        assert_eq!(std::fs::read(&paths[1]).unwrap(), tail);
        let leftovers = std::fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().ends_with(".tmp"))
            .count();
        assert_eq!(leftovers, 0);
        // A copy made here that is not a whole PDF is refused.
        assert!(write_files(&dir, &[outgoing("b.pdf", b"not a pdf", true)]).is_err());
        let _ = std::fs::remove_dir_all(dir.parent().unwrap());
    }

    #[test]
    fn hand_over_names_have_no_separators() {
        let safe = handover_name("it's, 100% \"done\"; a\\b\n.pdf");
        assert!(!safe.contains(['\'', ',', '%', '\\', '\n', ';', '"']), "{safe}");
        assert_eq!(handover_name("plain name.pdf"), "plain name.pdf");
    }

    #[test]
    fn drafts_are_named_after_the_subject() {
        assert_eq!(draft_file_name("Report: October?"), "Report_ October_.eml");
        assert_eq!(draft_file_name(""), "message.eml");
        assert_eq!(draft_file_name("  ...  "), "message.eml");
    }

    #[test]
    fn drafts_hold_the_files() {
        let dir = base("draft");
        let path = dir.join("draft.eml");
        std::fs::write(&path, b"older draft").unwrap();
        let files = [outgoing("report.pdf", b"%PDF-1.7 report", true)];
        write_draft(&path, "Report: October?", "See the file.", &files).unwrap();
        let msg = folitecta_core::email::read_email(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(msg.subject, "Report: October?");
        assert_eq!(
            msg.attachments,
            vec![("report.pdf".to_string(), b"%PDF-1.7 report".to_vec())]
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn names_are_numbered_in_the_outbox() {
        let dir = outbox_in(&base("numbered")).unwrap();
        let first = unique_path(&dir, "a.pdf");
        std::fs::write(&first, b"x").unwrap();
        assert_eq!(unique_path(&dir, "a.pdf").file_name().unwrap(), "a (2).pdf");
        let _ = std::fs::remove_dir_all(dir.parent().unwrap());
    }

    #[test]
    fn xdg_email_answers_are_explained() {
        assert_eq!(xdg_result(Some(0)), Ok(Sent::Opened));
        assert!(matches!(xdg_result(Some(2)), Err(HandOverError::Attachments(_))));
        assert!(matches!(xdg_result(Some(5)), Err(HandOverError::Attachments(_))));
        assert!(matches!(xdg_result(Some(3)), Err(HandOverError::NoProgram(_))));
        assert!(matches!(xdg_result(Some(4)), Err(HandOverError::Refused(_))));
        assert!(matches!(xdg_result(None), Err(HandOverError::Refused(_))));
        assert!(is_web_browser("firefox.desktop", ""));
        assert!(is_web_browser(
            "org.example.Viewer.desktop",
            "Network;WebBrowser;"
        ));
        assert!(!is_web_browser("org.kde.kmail2.desktop", "Office;Email;"));
        assert!(!is_web_browser("thunderbird.desktop", "Network;Email;"));
    }

    #[test]
    fn empty_subjects_and_messages_are_left_out_of_xdg_email() {
        let files = [PathBuf::from("/tmp/a.pdf")];
        let args = |s: &str, b: &str| -> Vec<String> {
            xdg_email_args(s, b, &files)
                .into_iter()
                .map(|a| a.to_string_lossy().to_string())
                .collect()
        };
        assert_eq!(
            args("a.pdf", ""),
            ["--utf8", "--subject", "a.pdf", "--attach", "/tmp/a.pdf"]
        );
        assert_eq!(args("", ""), ["--utf8", "--attach", "/tmp/a.pdf"]);
        assert_eq!(
            args("a.pdf", "Hello"),
            [
                "--utf8",
                "--subject",
                "a.pdf",
                "--body",
                "Hello",
                "--attach",
                "/tmp/a.pdf"
            ]
        );
    }

    /// The hand-over on Linux with stand-ins of `xdg-mime` and `xdg-email`
    /// (the only programs on the path given to them: the real ones are never
    /// reached). `xdg-email` fails like the real one on an empty subject or
    /// message, and records its arguments.
    #[cfg(target_os = "linux")]
    #[test]
    fn linux_hand_over_with_stand_in_programs() {
        use std::os::unix::fs::PermissionsExt;
        let run = |name: &str, handler: &str, code: i32, subject: &str, body: &str| {
            let dir = base(&format!("stubs-{name}"));
            let log = dir.join("args.log");
            let stub = |file: &str, text: String| {
                let p = dir.join(file);
                std::fs::write(&p, text).unwrap();
                std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
            };
            stub(
                "xdg-mime",
                r#"#!/bin/sh
printf '%s\n' 'HANDLER'
"#
                .replace("HANDLER", handler),
            );
            stub(
                "xdg-email",
                r#"#!/bin/sh
for a in "$@"; do printf '%s\n' "$a" >> 'LOG'; done
prev=
for a in "$@"; do
  case "$prev" in --subject|--body) [ -z "$a" ] && exit 1;; esac
  prev=$a
done
exit CODE
"#
                .replace("LOG", &log.display().to_string())
                .replace("CODE", &code.to_string()),
            );
            let file = dir.join("a.pdf");
            std::fs::write(&file, b"%PDF").unwrap();
            let sent = platform::compose_with(
                Some(dir.as_os_str()),
                Duration::from_secs(5),
                subject,
                body,
                std::slice::from_ref(&file),
            );
            let args = std::fs::read_to_string(&log).ok();
            let _ = std::fs::remove_dir_all(&dir);
            (sent, args)
        };
        // The subject made from the file name and no message: it opens.
        let (sent, args) = run("ok", "org.example.Mail.desktop", 0, "a.pdf", "");
        assert_eq!(sent, Ok(Sent::Opened));
        let args = args.unwrap();
        assert!(args.starts_with("--utf8\n--subject\na.pdf\n--attach\n"), "{args}");
        assert!(!args.contains("--body"));
        // Exit codes of xdg-email.
        let (sent, _) = run("none", "org.example.Mail.desktop", 3, "a.pdf", "text");
        assert!(matches!(sent, Err(HandOverError::NoProgram(_))));
        // No program for e-mail links, or a web browser: xdg-email is not run.
        let (sent, args) = run("unset", "", 0, "a.pdf", "");
        assert!(matches!(sent, Err(HandOverError::NoProgram(_))));
        assert!(args.is_none());
        let (sent, args) = run("browser", "firefox.desktop", 0, "a.pdf", "");
        assert!(matches!(sent, Err(HandOverError::Attachments(_))));
        assert!(args.is_none());
    }

    #[test]
    fn mapi_answers_are_explained() {
        assert_eq!(mapi_result(0), Ok(Sent::Attached));
        assert_eq!(mapi_result(1), Ok(Sent::Cancelled));
        assert!(matches!(mapi_result(11), Err(HandOverError::Attachments(_))));
        assert!(matches!(mapi_result(26), Err(HandOverError::NoProgram(_))));
        assert!(matches!(mapi_result(2), Err(HandOverError::Refused(_))));
    }

    #[test]
    fn a_fake_hand_over_launches_nothing() {
        fn fake(_: &str, _: &str, files: &[PathBuf]) -> Result<Sent, HandOverError> {
            Err(HandOverError::Attachments(files.len().to_string()))
        }
        let dir = outbox_in(&base("fake")).unwrap();
        let paths = write_files(&dir, &[outgoing("a.txt", b"a", false)]).unwrap();
        let before = launches();
        set_fake(Some(fake));
        assert_eq!(
            hand_over("s", "b", &paths),
            Err(HandOverError::Attachments("1".into()))
        );
        // A file that is not there is reported before anything is asked.
        assert!(matches!(
            hand_over("s", "b", &[dir.join("missing.pdf")]),
            Err(HandOverError::Attachments(_))
        ));
        assert_eq!(open_with_system(&dir), Ok(()));
        set_fake(None);
        assert_eq!(launches(), before);
        let _ = std::fs::remove_dir_all(dir.parent().unwrap());
    }
}
