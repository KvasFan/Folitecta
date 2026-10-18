//! Documents sent by e-mail: the active document or every open one, as PDF
//! files or in a ZIP archive, and the data of a form.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use folitecta_core::zip::ZipEntry;

use crate::commands::{handlers, no_doc};
use crate::i18n::t;
use crate::mail::{self, Sent};
use crate::state::{AppState, ToastKind};

/// A name every file system takes.
fn file_safe(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| {
            if "<>:\"/\\|?*".contains(c) || c.is_control() {
                '_'
            } else {
                c
            }
        })
        .collect();
    let s = s.trim().trim_end_matches('.').to_string();
    if s.is_empty() { "document".into() } else { s }
}

fn file_name(p: &Path) -> String {
    p.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default()
}

/// The documents as files: their own file when it holds every change, a copy
/// in `dir` otherwise.
fn document_files(st: AppState, all: bool, dir: &Path) -> Result<Vec<PathBuf>, String> {
    let active = st.active_index();
    let mut out = Vec::new();
    for (i, tab) in st.docs_mut().iter_mut().enumerate() {
        if !all && Some(i) != active {
            continue;
        }
        match tab.doc.path() {
            Some(p) if !tab.doc.is_dirty() && p.is_file() => out.push(p.to_path_buf()),
            _ => {
                let mut name = file_safe(&tab.title());
                if !name.to_lowercase().ends_with(".pdf") {
                    name.push_str(".pdf");
                }
                let path = mail::unique_path(dir, &name);
                tab.doc.save_copy(&path).map_err(|e| e.to_string())?;
                out.push(path);
            }
        }
    }
    Ok(out)
}

/// One ZIP archive `name` in `dir` with `files` (numbered when two have one name).
fn zip_files(files: &[PathBuf], dir: &Path, name: &str) -> Result<PathBuf, String> {
    let mut seen = HashSet::new();
    let mut entries = Vec::new();
    for f in files {
        let data = std::fs::read(f).map_err(|e| format!("{}: {e}", f.display()))?;
        let base = file_name(f);
        let mut entry = base.clone();
        let mut n = 1;
        while !seen.insert(entry.to_lowercase()) {
            n += 1;
            entry = match base.rsplit_once('.') {
                Some((stem, ext)) => format!("{stem} ({n}).{ext}"),
                None => format!("{base} ({n})"),
            };
        }
        entries.push(ZipEntry::new(entry, data));
    }
    let zip = folitecta_core::zip::write(&entries).map_err(|e| e.to_string())?;
    let path = mail::unique_path(dir, name);
    std::fs::write(&path, zip).map_err(|e| e.to_string())?;
    Ok(path)
}

/// Hand a message to the mail program, away from the interface thread.
fn compose(st: AppState, subject: String, files: Vec<PathBuf>) {
    let lang = st.lang();
    // On the root scope: the message window of Windows may stay open a long time.
    dioxus::core::spawn_forever(async move {
        let sent = tokio::task::spawn_blocking(move || mail::compose(&subject, "", &files)).await;
        match sent {
            Ok(Ok(Sent::Attached)) => st.toast(ToastKind::Ok, t(lang, "toast.mail_ready"), ""),
            Ok(Ok(Sent::Cancelled)) => {}
            Ok(Ok(Sent::Draft(path))) => st.toast(
                ToastKind::Info,
                t(lang, "toast.mail_draft"),
                path.display().to_string(),
            ),
            Ok(Ok(Sent::Folder)) => st.toast(
                ToastKind::Info,
                t(lang, "toast.mail_folder"),
                t(lang, "toast.mail_folder_hint"),
            ),
            Ok(Err(e)) => st.error(e),
            Err(e) => st.error(e),
        }
    });
}

/// E-mail the active document (`all`: every open one), as files or in one ZIP archive.
fn send(st: AppState, all: bool, zipped: bool) {
    if no_doc(st) {
        return;
    }
    let lang = st.lang();
    let prepared = mail::outbox().and_then(|dir| {
        let files = document_files(st, all, &dir)?;
        let names: Vec<String> = files.iter().map(|f| file_name(f)).collect();
        let subject = names.join(", ");
        if !zipped {
            return Ok((subject, files));
        }
        let base = match names.as_slice() {
            [one] => one
                .rsplit_once('.')
                .map(|(s, _)| s.to_string())
                .unwrap_or(one.clone()),
            _ => t(lang, "label.documents"),
        };
        let zip = zip_files(&files, &dir, &format!("{}.zip", file_safe(&base)))?;
        Ok((subject, vec![zip]))
    });
    match prepared {
        Ok((subject, files)) => compose(st, subject, files),
        Err(e) => st.error(e),
    }
}

/// E-mail the values of the form as XFDF.
fn send_form_data(st: AppState) {
    if no_doc(st) {
        return;
    }
    let Some((name, data)) = st.read_active(|tab| {
        let title = tab.title();
        let stem = title
            .rsplit_once('.')
            .map(|(s, _)| s.to_string())
            .unwrap_or(title);
        (
            format!("{}-data.xfdf", file_safe(&stem)),
            tab.doc.export_form_data(folitecta_core::FormDataFormat::Xfdf),
        )
    }) else {
        return;
    };
    let prepared = mail::outbox().and_then(|dir| {
        let path = mail::unique_path(&dir, &name);
        std::fs::write(&path, data).map_err(|e| e.to_string())?;
        Ok(path)
    });
    match prepared {
        Ok(path) => compose(st, name, vec![path]),
        Err(e) => st.error(e),
    }
}

handlers! {
    "sendByEmail" => |st| send(st, false, false),
    "sendByEmailZip" => |st| send(st, false, true),
    "sendByEmailOpenAll" => |st| send(st, true, false),
    "sendByEmailOpenAllZip" => |st| send(st, true, true),
    "forms.emailData" => send_form_data,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_made_safe() {
        assert_eq!(file_safe("a/b:c?.pdf"), "a_b_c_.pdf");
        assert_eq!(file_safe("  "), "document");
        assert_eq!(file_safe("report."), "report");
    }

    #[test]
    fn archives_number_files_of_the_same_name() {
        let dir = mail::outbox().unwrap();
        let (a, b) = (dir.join("one"), dir.join("two"));
        std::fs::create_dir_all(&a).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        std::fs::write(a.join("x.pdf"), b"first").unwrap();
        std::fs::write(b.join("x.pdf"), b"second").unwrap();
        let zip = zip_files(&[a.join("x.pdf"), b.join("x.pdf")], &dir, "both.zip").unwrap();
        let entries = folitecta_core::zip::read(&std::fs::read(&zip).unwrap()).unwrap();
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["x.pdf", "x (2).pdf"]);
        assert_eq!(entries[1].data, b"second");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
