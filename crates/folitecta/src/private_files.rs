//! Files handed to other programs (attachments, pictures edited in another
//! application) are written to a folder of this window inside a folder of
//! the user's own — the runtime folder of the session, else the user's cache
//! folder — never to the shared temporary folder.
//!
//! Every folder Folitecta makes on the way is created readable by the user
//! only and checked to be a real folder of the user: a link, or a folder of
//! someone else, is refused. Files are created new, readable by the user
//! only. The folders of a window are removed on Exit, the one of protected
//! documents as soon as no protected document is open, and the folders of
//! windows that ended otherwise at the next start.
//!
//! Use [`private_dir`] for the folder of a kind of file and
//! [`write_private_file`] to write a file in it.

use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// Decrypted files of protected documents (attachments, pictures).
pub const PROTECTED: &str = "protected";
/// Attachments of documents that are not protected.
pub const ATTACHMENTS: &str = "attachments";
/// Pictures edited in another application.
pub const PICTURES: &str = "pictures";

const KINDS: [&str; 3] = [PROTECTED, ATTACHMENTS, PICTURES];

/// The folder of this window for files of `kind` (see the constants),
/// created when needed.
pub fn private_dir(kind: &str) -> io::Result<PathBuf> {
    dir_in(&root()?, kind, std::process::id())
}

/// Write `data` to the new file `path` (in a folder of [`private_dir`]),
/// readable by the user only; an earlier file of that name is replaced (a
/// link of that name is removed, never followed).
pub fn write_private_file(path: &Path, data: &[u8]) -> io::Result<()> {
    match std::fs::remove_file(path) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e),
        _ => {}
    }
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)?.write_all(data)
}

/// Remove this window's folder of `kind`.
pub fn forget(kind: &str) {
    if let Some(root) = root_path() {
        // A link is removed itself, never followed.
        let _ = std::fs::remove_dir_all(root.join(format!("{kind}-{}", std::process::id())));
    }
}

/// Remove every folder of this window (leaving with Exit).
pub fn forget_all() {
    for kind in KINDS {
        forget(kind);
    }
}

/// Remove the folders left by windows that ended without Exit (at start).
pub fn sweep() {
    if let Some(root) = root_path() {
        sweep_in(&root, std::process::id());
    }
}

/// The folder of the user the private folders live in.
fn base_dir() -> Option<PathBuf> {
    // The self-test (and a chosen settings folder) keeps everything there.
    if let Some(dir) = std::env::var_os("FOLITECTA_CONFIG_DIR") {
        return Some(PathBuf::from(dir));
    }
    #[cfg(target_os = "windows")]
    {
        std::env::var_os("LOCALAPPDATA").map(PathBuf::from)
    }
    #[cfg(target_os = "macos")]
    {
        std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library").join("Caches"))
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        let absolute = |v: std::ffi::OsString| Some(PathBuf::from(v)).filter(|p| p.is_absolute());
        std::env::var_os("XDG_RUNTIME_DIR")
            .and_then(absolute)
            .filter(|p| p.is_dir())
            .or_else(|| std::env::var_os("XDG_CACHE_HOME").and_then(absolute))
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))
    }
}

fn root_path() -> Option<PathBuf> {
    base_dir().map(|b| b.join("folitecta-files"))
}

/// The folder of the private folders, created and checked.
fn root() -> io::Result<PathBuf> {
    let base =
        base_dir().ok_or_else(|| io::Error::other("there is no folder of the user for private files"))?;
    std::fs::create_dir_all(&base)?;
    // The user's own folder (it may be a link the user made, so it is followed).
    check_owner(&std::fs::metadata(&base)?, &base)?;
    let root = base.join("folitecta-files");
    ensure_private_dir(&root)?;
    Ok(root)
}

/// The folder `kind-pid` of `root`, created and checked.
fn dir_in(root: &Path, kind: &str, pid: u32) -> io::Result<PathBuf> {
    if kind.is_empty() || !kind.bytes().all(|b| b.is_ascii_lowercase()) {
        return Err(io::Error::other("not a kind of private folder"));
    }
    ensure_private_dir(root)?;
    let dir = root.join(format!("{kind}-{pid}"));
    ensure_private_dir(&dir)?;
    Ok(dir)
}

#[cfg(unix)]
fn user_id() -> u32 {
    // SAFETY: geteuid has no preconditions and cannot fail.
    unsafe { libc::geteuid() }
}

/// `path` belongs to the user.
fn check_owner(meta: &std::fs::Metadata, path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if meta.uid() != user_id() {
            return Err(io::Error::other(format!(
                "{} belongs to another user",
                path.display()
            )));
        }
    }
    #[cfg(not(unix))]
    let _ = (meta, path);
    Ok(())
}

/// Create `path` (readable by the user only) when it is missing, then
/// check that it is a real folder of the user, readable by the user only.
fn ensure_private_dir(path: &Path) -> io::Result<()> {
    match std::fs::symlink_metadata(path) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            let mut builder = std::fs::DirBuilder::new();
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                builder.mode(0o700);
            }
            match builder.create(path) {
                Err(e) if e.kind() != io::ErrorKind::AlreadyExists => return Err(e),
                _ => {}
            }
        }
        Err(e) => return Err(e),
        Ok(_) => {}
    }
    let meta = std::fs::symlink_metadata(path)?;
    check_private_dir(&meta, path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        // A real folder of the user (checked above): others lose any access.
        if meta.permissions().mode() & 0o077 != 0 {
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))?;
        }
    }
    Ok(())
}

/// A real folder (not a link) of the user.
fn check_private_dir(meta: &std::fs::Metadata, path: &Path) -> io::Result<()> {
    if meta.file_type().is_symlink() || !meta.is_dir() {
        return Err(io::Error::other(format!(
            "{} is not a folder of its own",
            path.display()
        )));
    }
    check_owner(meta, path)
}

/// Whether the window of process `pid` has ended: its process is gone where
/// processes can be looked at (Linux), else its folder is older than half a
/// day.
fn ended(pid: u32, folder: &Path) -> bool {
    let proc = Path::new("/proc");
    if proc.join("self").exists() {
        return !proc.join(pid.to_string()).exists();
    }
    std::fs::symlink_metadata(folder)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|m| m.elapsed().ok())
        .is_some_and(|age| age > std::time::Duration::from_secs(12 * 3600))
}

fn sweep_in(root: &Path, me: u32) {
    for e in std::fs::read_dir(root).into_iter().flatten().flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        let Some(pid) = name
            .rsplit_once('-')
            .and_then(|(kind, pid)| KINDS.contains(&kind).then_some(pid))
            .and_then(|p| p.parse::<u32>().ok())
        else {
            continue;
        };
        if pid == me || !ended(pid, &e.path()) {
            continue;
        }
        // A link is removed itself, never followed.
        let _ = std::fs::remove_dir_all(e.path()).or_else(|_| std::fs::remove_file(e.path()));
    }
}

#[cfg(all(test, unix))]
mod tests {
    use std::os::unix::fs::PermissionsExt;

    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("folitecta-private-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn mode(p: &Path) -> u32 {
        std::fs::symlink_metadata(p).unwrap().permissions().mode() & 0o777
    }

    #[test]
    fn folders_and_files_are_readable_by_the_user_only() {
        let base = scratch("modes");
        let root = base.join("folitecta-files");
        let dir = dir_in(&root, PROTECTED, 77).unwrap();
        assert_eq!(mode(&root), 0o700);
        assert_eq!(mode(&dir), 0o700);
        let file = dir.join("note.txt");
        write_private_file(&file, b"one").unwrap();
        assert_eq!(mode(&file), 0o600);
        // Written again: replaced, still private.
        write_private_file(&file, b"two").unwrap();
        assert_eq!(std::fs::read(&file).unwrap(), b"two");
        assert_eq!(mode(&file), 0o600);
        // A folder made by an older version with wider access is narrowed.
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
        dir_in(&root, PROTECTED, 77).unwrap();
        assert_eq!(mode(&dir), 0o700);
        assert!(dir_in(&root, "../x", 77).is_err());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn links_and_folders_of_others_are_refused() {
        let base = scratch("links");
        let elsewhere = base.join("elsewhere");
        std::fs::create_dir_all(&elsewhere).unwrap();
        // The folder of the window planted as a link: refused, and nothing
        // is written through it; removing it removes the link only.
        let root = base.join("folitecta-files");
        dir_in(&root, ATTACHMENTS, 1).unwrap();
        std::os::unix::fs::symlink(&elsewhere, root.join("protected-88")).unwrap();
        assert!(dir_in(&root, PROTECTED, 88).is_err());
        // The folder of the private folders as a link: refused.
        let linked = base.join("linked-root");
        std::os::unix::fs::symlink(&elsewhere, &linked).unwrap();
        assert!(dir_in(&linked, PROTECTED, 89).is_err());
        // A file planted as a link is replaced, not followed.
        let dir = dir_in(&root, PICTURES, 90).unwrap();
        let target = elsewhere.join("victim.txt");
        std::fs::write(&target, b"keep").unwrap();
        std::os::unix::fs::symlink(&target, dir.join("picture.png")).unwrap();
        write_private_file(&dir.join("picture.png"), b"new").unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), b"keep");
        assert!(
            !std::fs::symlink_metadata(dir.join("picture.png"))
                .unwrap()
                .file_type()
                .is_symlink()
        );
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn folders_of_other_users_are_refused() {
        // The root folder of the system belongs to another user.
        let meta = std::fs::symlink_metadata("/").unwrap();
        if user_id() != 0 {
            assert!(check_private_dir(&meta, Path::new("/")).is_err());
        }
    }

    #[test]
    fn folders_of_ended_windows_are_swept() {
        let base = scratch("sweep");
        let root = base.join("folitecta-files");
        let me = std::process::id();
        let dead = u32::MAX - 1;
        for (kind, pid) in [(PROTECTED, dead), (ATTACHMENTS, 1), (PICTURES, me)] {
            dir_in(&root, kind, pid).unwrap();
        }
        // Old, but its process (1) runs: kept where processes can be seen.
        let live = root.join(format!("{ATTACHMENTS}-1"));
        let old = std::time::SystemTime::now() - std::time::Duration::from_secs(48 * 3600);
        std::fs::File::open(&live).unwrap().set_modified(old).unwrap();
        // A link named like a folder of an ended window: the link goes, its
        // target stays.
        let elsewhere = base.join("elsewhere");
        std::fs::create_dir_all(&elsewhere).unwrap();
        std::fs::write(elsewhere.join("keep.txt"), b"keep").unwrap();
        std::os::unix::fs::symlink(&elsewhere, root.join(format!("{PICTURES}-{dead}"))).unwrap();
        sweep_in(&root, me);
        assert!(!root.join(format!("{PROTECTED}-{dead}")).exists());
        assert!(std::fs::symlink_metadata(root.join(format!("{PICTURES}-{dead}"))).is_err());
        assert!(elsewhere.join("keep.txt").exists());
        assert!(root.join(format!("{PICTURES}-{me}")).exists());
        if Path::new("/proc/self").exists() {
            assert!(live.exists(), "the folder of a running window was removed");
        }
        let _ = std::fs::remove_dir_all(&base);
    }
}
