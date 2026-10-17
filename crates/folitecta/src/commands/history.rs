//! History of opened documents: recording, pins, sorting, groups, thumbnails
//! and the history pane commands.

use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

use dioxus::prelude::*;

use super::{handlers, option_on};
use crate::i18n::{t, tf};
use crate::state::{AppState, HistoryEntry, Pane, ToastKind, app_data_dir, now_secs};

/// Most documents kept in the history (pinned ones are always kept).
const LIMIT: usize = 200;

/// Where the thumbnail of a document of the history is kept.
pub fn thumb_path(path: &Path) -> Option<PathBuf> {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    path.hash(&mut h);
    app_data_dir().map(|d| d.join("thumbs").join(format!("{:016x}.png", h.finish())))
}

/// Note that a document was opened.
pub fn record_open(st: AppState, path: &Path, title: &str, pages: usize) {
    let mut ui = st.ui_mut();
    let pinned = ui.history.iter().any(|h| h.path == path && h.pinned);
    let page = ui
        .history
        .iter()
        .find(|h| h.path == path)
        .map(|h| h.page)
        .unwrap_or(0);
    ui.history.retain(|h| h.path != path);
    ui.history.insert(
        0,
        HistoryEntry {
            path: path.to_path_buf(),
            title: title.to_string(),
            pages,
            opened: now_secs(),
            closed: 0,
            page,
            pinned,
        },
    );
    // Keep the newest and every pinned document.
    let mut kept = 0;
    ui.history.retain(|h| {
        kept += 1;
        h.pinned || kept <= LIMIT
    });
}

/// Note that a document was closed at `page`, and keep a thumbnail of it.
pub fn record_close(st: AppState, path: &Path, page: usize, doc: &folitecta_core::Document) {
    {
        let mut ui = st.ui_mut();
        if let Some(h) = ui.history.iter_mut().find(|h| h.path == path) {
            h.closed = now_secs();
            h.page = page;
        }
    }
    st.save_prefs();
    if let Some(target) = thumb_path(path) {
        let scale = doc
            .page_size(0)
            .map(|s| (120.0 / s.width.max(s.height)).clamp(0.05, 1.0))
            .unwrap_or(0.15);
        if let Ok(img) = doc.render_page_png(0, scale) {
            if let Some(dir) = target.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            let _ = std::fs::write(target, img.png);
        }
    }
}

/// "opentime", "closetime", "modtime" or "alphabetical".
pub fn sort_mode(st: AppState) -> String {
    st.ui
        .peek()
        .value("history.sort")
        .unwrap_or("opentime")
        .to_string()
}

/// Modification time of a file (seconds since 1970).
pub fn modified(path: &Path) -> u64 {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// The history in the chosen order.
pub fn sorted_entries(st: AppState) -> Vec<HistoryEntry> {
    let mut list = st.ui.peek().history.clone();
    match sort_mode(st).as_str() {
        "alphabetical" => list.sort_by_key(|h| file_name(&h.path).to_lowercase()),
        "closetime" => list.sort_by_key(|h| std::cmp::Reverse(h.closed.max(h.opened))),
        "modtime" => list.sort_by_key(|h| std::cmp::Reverse(modified(&h.path))),
        _ => list.sort_by_key(|h| std::cmp::Reverse(h.opened)),
    }
    list
}

pub fn file_name(p: &Path) -> String {
    p.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| p.display().to_string())
}

/// Group of a time: "today", "yesterday", "week" or "older".
pub fn age_group(secs: u64, now: u64) -> &'static str {
    let day = 86_400;
    let start_today = now - now % day;
    if secs >= start_today {
        "today"
    } else if secs >= start_today - day {
        "yesterday"
    } else if secs >= start_today - 6 * day {
        "week"
    } else {
        "older"
    }
}

/// The selected documents, or none.
fn selection(st: AppState) -> Vec<PathBuf> {
    st.history_sel.peek().clone()
}

fn need_selection(st: AppState) -> Option<Vec<PathBuf>> {
    let sel = selection(st);
    if sel.is_empty() {
        st.toast(ToastKind::Info, t(st.lang(), "toast.history_select"), "");
        None
    } else {
        Some(sel)
    }
}

/// Open documents of the history, at the page where they were closed.
pub fn open_paths(st: AppState, paths: Vec<PathBuf>) {
    for p in paths {
        if !p.exists() {
            st.toast(
                ToastKind::Warn,
                tf(
                    st.lang(),
                    "toast.history_missing",
                    &[("file", &p.display().to_string())],
                ),
                "",
            );
            continue;
        }
        let page = st
            .ui
            .peek()
            .history
            .iter()
            .find(|h| h.path == p)
            .map(|h| h.page)
            .unwrap_or(0);
        crate::files::open_path(st, p.clone(), None);
        if page > 0
            && st
                .read_active(|t| t.doc.path() == Some(p.as_path()))
                .unwrap_or(false)
        {
            st.navigate(page, true);
        }
    }
}

fn open(st: AppState) {
    if let Some(sel) = need_selection(st) {
        open_paths(st, sel);
    }
}

fn remove(st: AppState) {
    let Some(sel) = need_selection(st) else { return };
    {
        let mut ui = st.ui_mut();
        ui.history.retain(|h| !sel.contains(&h.path));
        ui.recent.retain(|p| !sel.contains(p));
    }
    for p in &sel {
        if let Some(t) = thumb_path(p) {
            let _ = std::fs::remove_file(t);
        }
    }
    st.history_sel.write_unchecked().clear();
    st.save_prefs();
}

fn pin(st: AppState, pinned: bool) {
    let Some(sel) = need_selection(st) else { return };
    for h in st.ui_mut().history.iter_mut().filter(|h| sel.contains(&h.path)) {
        h.pinned = pinned;
    }
    st.save_prefs();
}

fn copy_path(st: AppState) {
    let Some(sel) = need_selection(st) else { return };
    let text = sel
        .iter()
        .map(|p| p.display().to_string())
        .collect::<Vec<_>>()
        .join("\n");
    crate::util::copy_to_clipboard(&text);
    st.toast(ToastKind::Ok, t(st.lang(), "toast.path_copied"), "");
}

fn open_folder(st: AppState) {
    let Some(sel) = need_selection(st) else { return };
    if let Some(dir) = sel[0].parent() {
        if let Err(e) = crate::util::open_external(&dir.display().to_string()) {
            st.error(e);
        }
    }
}

fn select_all(st: AppState) {
    let all: Vec<PathBuf> = st.ui.peek().history.iter().map(|h| h.path.clone()).collect();
    *st.history_sel.write_unchecked() = all;
}

fn collapse_all(st: AppState, collapse: bool) {
    let mut c = st.history_collapsed.write_unchecked();
    c.clear();
    if collapse {
        c.extend(["pinned", "today", "yesterday", "week", "older"].map(String::from));
    }
}

/// Forget every document that is not pinned.
pub fn clear(st: AppState) {
    {
        let mut ui = st.ui_mut();
        let gone: Vec<PathBuf> = ui
            .history
            .iter()
            .filter(|h| !h.pinned)
            .map(|h| h.path.clone())
            .collect();
        for p in &gone {
            if let Some(t) = thumb_path(p) {
                let _ = std::fs::remove_file(t);
            }
        }
        ui.history.retain(|h| h.pinned);
        ui.recent.clear();
    }
    st.history_sel.write_unchecked().clear();
    st.save_prefs();
    st.toast(ToastKind::Ok, t(st.lang(), "toast.history_cleared"), "");
}

fn set_sort(st: AppState, mode: &str) {
    st.set_value("history.sort", mode);
}

/// Next sort order.
fn cycle_sort(st: AppState) {
    let modes = ["opentime", "closetime", "modtime", "alphabetical"];
    let cur = sort_mode(st);
    let i = modes.iter().position(|m| *m == cur).unwrap_or(0);
    let next = modes[(i + 1) % modes.len()];
    set_sort(st, next);
    let id = match next {
        "closetime" => "historyList.sort.closetime",
        "modtime" => "historyList.sort.modtime",
        "alphabetical" => "historyList.sort.alphabetical",
        _ => "historyList.sort.opentime",
    };
    let label = folitecta_core::registry()
        .get(id)
        .map(|c| crate::i18n::cmd_label(st.lang(), c))
        .unwrap_or_default();
    st.toast(
        ToastKind::Info,
        tf(st.lang(), "toast.sorted_by", &[("order", &label)]),
        "",
    );
}

fn toggle(st: AppState, key: &str) {
    let on = !option_on(st, key);
    st.set_option(key, on);
}

/// Check states of the history options.
pub fn history_state(st: AppState, id: &str) -> Option<bool> {
    match id {
        "historyList.group.show"
        | "historyList.options.showCaptions"
        | "historyList.options.showPath"
        | "historyList.options.showPinned"
        | "historyList.options.showPreviewPopUp"
        | "historyList.options.showThumbnail" => Some(option_on(st, id)),
        "historyList.sort.alphabetical" => Some(sort_mode(st) == "alphabetical"),
        "historyList.sort.closetime" => Some(sort_mode(st) == "closetime"),
        "historyList.sort.modtime" => Some(sort_mode(st) == "modtime"),
        "historyList.sort.opentime" => Some(sort_mode(st) == "opentime"),
        _ => None,
    }
}

handlers! {
    "historyList.open" => open,
    "historyList.remove" => remove,
    "historyList.pin" => |st| pin(st, true),
    "historyList.unpin" => |st| pin(st, false),
    "historyList.copyPath" => copy_path,
    "historyList.folder" => open_folder,
    "historyList.selectAll" => select_all,
    "historyList.expandAll" => |st| collapse_all(st, false),
    "historyList.collapseAll" => |st| collapse_all(st, true),
    "historyList.group.show" => |st| toggle(st, "historyList.group.show"),
    "historyList.options.showCaptions" => |st| toggle(st, "historyList.options.showCaptions"),
    "historyList.options.showPath" => |st| toggle(st, "historyList.options.showPath"),
    "historyList.options.showPinned" => |st| toggle(st, "historyList.options.showPinned"),
    "historyList.options.showPreviewPopUp" => |st| toggle(st, "historyList.options.showPreviewPopUp"),
    "historyList.options.showThumbnail" => |st| toggle(st, "historyList.options.showThumbnail"),
    "historyList.sort.alphabetical" => |st| set_sort(st, "alphabetical"),
    "historyList.sort.closetime" => |st| set_sort(st, "closetime"),
    "historyList.sort.modtime" => |st| set_sort(st, "modtime"),
    "historyList.sort.opentime" => |st| set_sort(st, "opentime"),
    "historyList.sortBy" => cycle_sort,
    "view.history" => |st| crate::commands::show_pane(st, Pane::History),
}

#[cfg(test)]
mod tests {
    use super::age_group;

    #[test]
    fn times_are_grouped_by_age() {
        let now = 1_700_000_000u64;
        let today = now - now % 86_400;
        assert_eq!(age_group(now, now), "today");
        assert_eq!(age_group(today - 10, now), "yesterday");
        assert_eq!(age_group(today - 3 * 86_400, now), "week");
        assert_eq!(age_group(today - 30 * 86_400, now), "older");
    }
}
