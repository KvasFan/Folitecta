//! Bates numbering of several files: the list of files in the order they
//! are numbered, the pages of each, the columns shown (title, location,
//! type, size, modified, number of pages, pages numbered, page size), and
//! numbering them into copies named `<name>-bates.pdf`. The List commands
//! (Add, Edit, Delete, Up, Down, Clear) act on the files chosen.

use std::collections::BTreeSet;
use std::path::PathBuf;

use dioxus::prelude::*;
use folitecta_core::Document;

use crate::commands::handlers;
use crate::commands::sources::reorder;
use crate::i18n::{t, tf};
use crate::state::{AppState, Dialog, ToastKind};

#[derive(Debug, Clone, PartialEq)]
pub struct BatesFile {
    pub path: PathBuf,
    /// The title of the document information (empty when none).
    pub title: String,
    pub size: u64,
    /// Seconds since 1970 of the last change of the file.
    pub modified: Option<u64>,
    /// Pages of the file (None when it cannot be opened).
    pub pages: Option<usize>,
    /// The size of the first page, in points.
    pub page_size: Option<(f32, f32)>,
    /// Pages numbered, as typed (`1-3, 5`); empty for all.
    pub range: String,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct BatesFiles {
    pub items: Vec<BatesFile>,
    pub selected: BTreeSet<usize>,
}

pub static BATES_FILES: GlobalSignal<BatesFiles> = Signal::global(BatesFiles::default);

/// The columns of the list, by command id; the name of the file is always shown.
pub const COLUMNS: &[&str] = &[
    "batesNumberingFiles.hdr.title",
    "batesNumberingFiles.hdr.location",
    "batesNumberingFiles.hdr.type",
    "batesNumberingFiles.hdr.size",
    "batesNumberingFiles.hdr.modified",
    "batesNumberingFiles.hdr.pagesCount",
    "batesNumberingFiles.hdr.pagesRange",
    "batesNumberingFiles.hdr.pagesSize",
];

/// What the list knows of a file, read from it.
pub fn read_file(path: PathBuf) -> BatesFile {
    let meta = std::fs::metadata(&path).ok();
    let modified = meta
        .as_ref()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs());
    let doc = Document::open(&path).ok();
    BatesFile {
        title: doc.as_ref().map(|d| d.title()).unwrap_or_default(),
        size: meta.map(|m| m.len()).unwrap_or(0),
        modified,
        pages: doc.as_ref().map(|d| d.page_count()),
        page_size: doc
            .as_ref()
            .and_then(|d| d.page_size(0).ok())
            .map(|s| (s.width, s.height)),
        range: String::new(),
        path,
    }
}

/// Add PDF files to the list (read on a worker thread).
pub fn add_paths(paths: Vec<PathBuf>) {
    spawn(async move {
        let read = tokio::task::spawn_blocking(move || paths.into_iter().map(read_file).collect::<Vec<_>>())
            .await
            .unwrap_or_default();
        BATES_FILES.write().items.extend(read);
    });
}

fn add_files(st: AppState) {
    spawn(async move {
        let Some(files) = rfd::AsyncFileDialog::new()
            .add_filter("PDF", &["pdf", "PDF"])
            .set_title(t(st.lang(), "bates_files.choose"))
            .pick_files()
            .await
        else {
            return;
        };
        add_paths(files.into_iter().map(|f| f.path().to_path_buf()).collect());
    });
}

fn need_choice(st: AppState) -> bool {
    let none = BATES_FILES.peek().selected.is_empty();
    if none {
        st.toast(ToastKind::Info, t(st.lang(), "bates_files.choose_file"), "");
    }
    !none
}

fn remove(st: AppState) {
    if !need_choice(st) {
        return;
    }
    let mut w = BATES_FILES.write();
    let sel = std::mem::take(&mut w.selected);
    let items = std::mem::take(&mut w.items);
    w.items = items
        .into_iter()
        .enumerate()
        .filter(|(i, _)| !sel.contains(i))
        .map(|(_, f)| f)
        .collect();
}

fn move_chosen(st: AppState, how: &str) {
    if !need_choice(st) {
        return;
    }
    let mut w = BATES_FILES.write();
    let items = std::mem::take(&mut w.items);
    let (items, sel) = reorder(items, &w.selected, how);
    w.items = items;
    w.selected = sel;
}

/// Edit: the pages field of the first file chosen, ready to type.
fn edit(st: AppState) {
    if !need_choice(st) {
        return;
    }
    let first = BATES_FILES.peek().selected.iter().next().copied().unwrap_or(0);
    let _ = dioxus::document::eval(&format!(
        "const i = document.getElementById('bates-range-{first}'); if (i) {{ i.focus(); i.select(); }}"
    ));
}

/// Number the files in their order: each gets the Bates numbers of the
/// Bates settings on the pages chosen, the numbers going on from one file
/// to the next, in a copy next to it.
pub fn number(st: AppState) {
    let list = BATES_FILES.peek().items.clone();
    if list.is_empty() {
        st.toast(ToastKind::Info, t(st.lang(), "bates_files.empty"), "");
        return;
    }
    // Every range is checked before anything is written.
    let mut plan = Vec::new();
    for f in &list {
        let Some(n) = f.pages else {
            st.error(tf(
                st.lang(),
                "bates_files.unreadable",
                &[("file", &f.path.display().to_string())],
            ));
            return;
        };
        let pages = if f.range.trim().is_empty() {
            (0..n).collect()
        } else {
            match crate::util::parse_pages(&f.range, n) {
                Ok(p) if !p.is_empty() => p,
                _ => {
                    st.error(tf(
                        st.lang(),
                        "bates_files.bad_range",
                        &[("file", &f.path.display().to_string()), ("range", &f.range)],
                    ));
                    return;
                }
            }
        };
        plan.push((f.path.clone(), pages));
    }
    let format = crate::commands::macros::bates_format(st);
    let first = crate::commands::macros::bates_next(st);
    st.set_dialog(Dialog::None);
    dioxus::core::spawn_forever(async move {
        let done = tokio::task::spawn_blocking(move || {
            let mut next = first;
            let mut written = 0usize;
            let mut errors = Vec::new();
            for (docnumber, (path, pages)) in plan.into_iter().enumerate() {
                let template = format.replace("{docnumber}", &(docnumber + 1).to_string());
                let r = Document::open(&path).and_then(|mut doc| {
                    doc.add_page_numbers(
                        &pages,
                        &template,
                        next,
                        folitecta_core::MarkPosition::BottomRight,
                        9.0,
                        24.0,
                        [0.0, 0.0, 0.0],
                    )?;
                    let stem = path
                        .file_stem()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_else(|| "document".into());
                    doc.save_as(path.with_file_name(format!("{stem}-bates.pdf")))
                });
                match r {
                    Ok(()) => {
                        written += 1;
                        next += pages.len() as i64;
                    }
                    Err(e) => errors.push(format!("{}: {e}", path.display())),
                }
            }
            (written, next, errors)
        })
        .await;
        let Ok((written, next, errors)) = done else { return };
        for e in errors {
            st.error(e);
        }
        if next != first {
            st.set_value("bates.next", next.to_string());
        }
        st.toast(
            ToastKind::Ok,
            tf(
                st.lang(),
                "toast.bates_done",
                &[("count", &written.to_string()), ("last", &(next - 1).to_string())],
            ),
            "",
        );
    });
}

/// Check states of the columns.
pub fn bates_files_state(st: AppState, id: &str) -> Option<bool> {
    COLUMNS.contains(&id).then(|| column_on(st, id))
}

/// A column is shown: the title, the number of pages and the pages numbered
/// unless turned off, the others when turned on.
pub fn column_on(st: AppState, id: &str) -> bool {
    match st.ui.peek().options.get(id) {
        Some(v) => *v,
        None => matches!(
            id,
            "batesNumberingFiles.hdr.title"
                | "batesNumberingFiles.hdr.pagesCount"
                | "batesNumberingFiles.hdr.pagesRange"
        ),
    }
}

fn toggle_column(st: AppState, id: &str) {
    let on = column_on(st, id);
    st.set_option(id, !on);
}

handlers! {
    "document.batesNumbering.addMulti" => |st| st.set_dialog(Dialog::BatesFiles),
    "list.add" => add_files,
    "list.edit" => edit,
    "list.delete" => remove,
    "list.up" => |st| move_chosen(st, "up"),
    "list.down" => |st| move_chosen(st, "down"),
    "list.clear" => |_| { *BATES_FILES.write() = BatesFiles::default(); },
    "batesNumberingFiles.hdr.title" => |st| toggle_column(st, "batesNumberingFiles.hdr.title"),
    "batesNumberingFiles.hdr.location" => |st| toggle_column(st, "batesNumberingFiles.hdr.location"),
    "batesNumberingFiles.hdr.type" => |st| toggle_column(st, "batesNumberingFiles.hdr.type"),
    "batesNumberingFiles.hdr.size" => |st| toggle_column(st, "batesNumberingFiles.hdr.size"),
    "batesNumberingFiles.hdr.modified" => |st| toggle_column(st, "batesNumberingFiles.hdr.modified"),
    "batesNumberingFiles.hdr.pagesCount" => |st| toggle_column(st, "batesNumberingFiles.hdr.pagesCount"),
    "batesNumberingFiles.hdr.pagesRange" => |st| toggle_column(st, "batesNumberingFiles.hdr.pagesRange"),
    "batesNumberingFiles.hdr.pagesSize" => |st| toggle_column(st, "batesNumberingFiles.hdr.pagesSize"),
}
