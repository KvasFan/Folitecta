//! Source files of a new combined document: the list, its order, the pages
//! taken from each file, and combining them.

use std::collections::BTreeSet;
use std::path::PathBuf;

use dioxus::prelude::*;
use folitecta_core::Document;
use folitecta_core::combine::{Source, SourceKind, source_exts, source_kind, source_pages};

use super::handlers;
use crate::i18n::{t, tf};
use crate::state::{AppState, Dialog, ToastKind};

#[derive(Debug, Clone, PartialEq)]
pub struct SourceItem {
    pub path: PathBuf,
    pub kind: SourceKind,
    /// Pages of the file (None when it cannot be read).
    pub pages: Option<usize>,
    /// Pages to take, as typed (`1-3, 5`); empty for all.
    pub range: String,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Sources {
    pub items: Vec<SourceItem>,
    pub selected: BTreeSet<usize>,
    /// Columns with the kind, pages and folder of each file.
    pub details: bool,
}

pub static SOURCES: GlobalSignal<Sources> = Signal::global(Sources::default);

/// Add files that can be combined (others are counted and left out).
pub fn add_paths(st: AppState, paths: Vec<PathBuf>) {
    let mut skipped = 0;
    let mut added = Vec::new();
    for p in paths {
        match source_kind(&p) {
            Some(kind) => added.push(SourceItem {
                pages: source_pages(&p).ok(),
                path: p,
                kind,
                range: String::new(),
            }),
            None => skipped += 1,
        }
    }
    SOURCES.write().items.extend(added);
    if skipped > 0 {
        st.toast(
            ToastKind::Info,
            tf(
                st.lang(),
                "toast.sources_skipped",
                &[("count", &skipped.to_string())],
            ),
            "",
        );
    }
}

fn add_files(st: AppState) {
    spawn(async move {
        let Some(files) = rfd::AsyncFileDialog::new()
            .add_filter(t(st.lang(), "sources.files"), &source_exts())
            .pick_files()
            .await
        else {
            return;
        };
        add_paths(st, files.into_iter().map(|f| f.path().to_path_buf()).collect());
    });
}

fn add_folder(st: AppState) {
    spawn(async move {
        let Some(dir) = rfd::AsyncFileDialog::new().pick_folder().await else {
            return;
        };
        let mut paths: Vec<PathBuf> = std::fs::read_dir(dir.path())
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_file() && source_kind(p).is_some())
            .collect();
        paths.sort_by_key(|p| p.file_name().map(|n| n.to_string_lossy().to_lowercase()));
        add_paths(st, paths);
    });
}

fn import_list(st: AppState) {
    spawn(async move {
        let Some(file) = rfd::AsyncFileDialog::new()
            .add_filter("TXT", &["txt", "lst"])
            .pick_file()
            .await
        else {
            return;
        };
        let text = String::from_utf8_lossy(&file.read().await).to_string();
        add_paths(st, crate::commands::explorer::read_list(&text));
    });
}

/// The items with the selected ones moved together: one place `up` or
/// `down`, to the `top` or the `bottom`; returns them and the new selection.
pub fn reorder<T>(items: Vec<T>, sel: &BTreeSet<usize>, how: &str) -> (Vec<T>, BTreeSet<usize>) {
    if sel.is_empty() || items.is_empty() {
        return (items, sel.clone());
    }
    let mut picked = Vec::new();
    let mut rest = Vec::new();
    for (i, it) in items.into_iter().enumerate() {
        if sel.contains(&i) {
            picked.push(it)
        } else {
            rest.push(it)
        }
    }
    let first = *sel.iter().next().unwrap_or(&0);
    // Unselected items before the first selected one.
    let before = (0..first).filter(|i| !sel.contains(i)).count();
    let at = match how {
        "top" => 0,
        "bottom" => rest.len(),
        "up" => before.saturating_sub(1),
        _ => (before + 1).min(rest.len()),
    };
    let k = picked.len();
    for (j, it) in picked.into_iter().enumerate() {
        rest.insert(at + j, it);
    }
    (rest, (at..at + k).collect())
}

/// Move the selected files: by one place, or to the top or the bottom.
pub fn move_selected(how: &str) {
    let mut s = SOURCES.write();
    let sel = s.selected.clone();
    let (items, selected) = reorder(std::mem::take(&mut s.items), &sel, how);
    s.items = items;
    s.selected = selected;
}
fn remove_selected() {
    let mut s = SOURCES.write();
    let sel = std::mem::take(&mut s.selected);
    let mut i = 0;
    s.items.retain(|_| {
        let keep = !sel.contains(&i);
        i += 1;
        keep
    });
}

fn focus_range(st: AppState) {
    let first = SOURCES.peek().selected.iter().next().copied();
    match first {
        Some(i) => {
            let js = format!("document.getElementById('source-range-{i}')?.focus();");
            spawn(async move {
                let _ = document::eval(&js).await;
            });
        }
        None => st.toast(ToastKind::Info, t(st.lang(), "toast.select_files"), ""),
    }
}

/// Open the list of source files (the file picker first when it is empty).
pub fn open(st: AppState) {
    st.set_dialog(Dialog::Combine);
    if SOURCES.peek().items.is_empty() {
        add_files(st);
    }
}

/// Combine the listed files into a new document tab.
pub fn combine(st: AppState) {
    let items = SOURCES.peek().items.clone();
    if items.is_empty() {
        st.toast(ToastKind::Info, t(st.lang(), "toast.no_sources"), "");
        return;
    }
    let mut sources = Vec::new();
    for it in &items {
        let pages = if it.range.trim().is_empty() || it.kind != SourceKind::Pdf {
            None
        } else {
            match crate::util::parse_page_range(&it.range, it.pages.unwrap_or(0)) {
                Ok(p) => Some(p),
                Err(e) => {
                    let name = it
                        .path
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_default();
                    st.toast(ToastKind::Warn, format!("{name}: {e}"), "");
                    return;
                }
            }
        };
        sources.push(Source {
            path: it.path.clone(),
            pages,
        });
    }
    match Document::combine(&sources) {
        Ok((doc, problems)) => {
            for (p, e) in problems {
                st.error(format!("{}: {e}", p.display()));
            }
            crate::files::open_document_tab(st, doc);
            st.set_dialog(Dialog::None);
        }
        Err(e) => st.error(e),
    }
}

handlers! {
    "newDocCombine" => open,
    "newDocFromFiles" => open,
    "newDocumentList.addFile" => add_files,
    "newDocumentList.addFolder" => add_folder,
    "newDocumentList.import" => import_list,
    "newDocumentList.remove" => |_| remove_selected(),
    "newDocumentList.clear" => |_| *SOURCES.write() = Sources { details: SOURCES.peek().details, ..Sources::default() },
    "newDocumentList.up" => |_| move_selected("up"),
    "newDocumentList.down" => |_| move_selected("down"),
    "newDocumentList.top" => |_| move_selected("top"),
    "newDocumentList.bottom" => |_| move_selected("bottom"),
    "newDocumentList.range" => focus_range,
    "newDocumentList.edit" => focus_range,
    "newDocumentList.viewMode" => |_| { let d = SOURCES.peek().details; SOURCES.write().details = !d; },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selected_files_move_together() {
        let items = vec!['a', 'b', 'c', 'd', 'e'];
        let sel = BTreeSet::from([1, 2]);
        assert_eq!(
            reorder(items.clone(), &sel, "up"),
            (vec!['b', 'c', 'a', 'd', 'e'], BTreeSet::from([0, 1]))
        );
        assert_eq!(
            reorder(items.clone(), &sel, "down"),
            (vec!['a', 'd', 'b', 'c', 'e'], BTreeSet::from([2, 3]))
        );
        assert_eq!(
            reorder(items.clone(), &sel, "top").0,
            vec!['b', 'c', 'a', 'd', 'e']
        );
        assert_eq!(
            reorder(items.clone(), &sel, "bottom"),
            (vec!['a', 'd', 'e', 'b', 'c'], BTreeSet::from([3, 4]))
        );
        // Already at the edge: nothing moves.
        let first = BTreeSet::from([0]);
        assert_eq!(reorder(items.clone(), &first, "up").0, items);
        let last = BTreeSet::from([4]);
        assert_eq!(reorder(items.clone(), &last, "down").0, items);
    }
}
