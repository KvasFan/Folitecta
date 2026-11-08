//! Where the settings are kept when Folitecta travels with them: a folder
//! chosen with `--settings <folder>`, or, when a file or folder named
//! `portable` lies beside the program, the folder `settings` beside it.
//! Settings, digital IDs, trusted certificates, recovery copies and private
//! working files then all stay there (FOL-L050).

use std::path::{Path, PathBuf};

/// The name of the marker beside the program.
pub const MARKER: &str = "portable";

/// The settings folder `args` and the place of the program `exe` ask for.
pub fn settings_folder(args: &[String], exe: Option<&Path>) -> Option<PathBuf> {
    if let Some(i) = args.iter().position(|a| a == "--settings") {
        return args.get(i + 1).map(PathBuf::from);
    }
    let beside = exe?.parent()?;
    beside.join(MARKER).exists().then(|| beside.join("settings"))
}

/// Whether the settings are kept in a folder of their own (see the module).
pub fn active() -> bool {
    std::env::var_os("FOLITECTA_PORTABLE").is_some()
}

/// Keep the settings where `args` and the program ask (before any other
/// thread starts): the folder is made, and the program's folders use it.
pub fn apply(args: &[String]) {
    if std::env::var_os("FOLITECTA_CONFIG_DIR").is_some() {
        return;
    }
    let exe = std::env::current_exe().ok();
    let Some(dir) = settings_folder(args, exe.as_deref()) else {
        return;
    };
    if std::fs::create_dir_all(&dir).is_err() {
        eprintln!(
            "the settings folder {} cannot be made: the usual one is used",
            dir.display()
        );
        return;
    }
    let dir = dir.canonicalize().unwrap_or(dir);
    // SAFETY: called at the start of main, before any other thread exists.
    unsafe {
        std::env::set_var("FOLITECTA_CONFIG_DIR", &dir);
        std::env::set_var("FOLITECTA_PORTABLE", "1");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_settings_folder_is_chosen_or_beside_a_portable_program() {
        let dir = std::env::temp_dir().join(format!("folitecta-portable-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let exe = dir.join("folitecta");
        let args = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(settings_folder(&args(&[]), Some(&exe)), None);
        std::fs::write(dir.join(MARKER), b"").unwrap();
        assert_eq!(
            settings_folder(&args(&["a.pdf"]), Some(&exe)),
            Some(dir.join("settings"))
        );
        // Chosen on the command line: that folder, marker or not.
        assert_eq!(
            settings_folder(&args(&["--settings", "/tmp/elsewhere", "a.pdf"]), Some(&exe)),
            Some(PathBuf::from("/tmp/elsewhere"))
        );
        assert_eq!(settings_folder(&args(&["--settings"]), Some(&exe)), None);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
