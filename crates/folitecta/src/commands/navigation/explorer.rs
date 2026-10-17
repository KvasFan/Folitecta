//! Explorer pane: folders and files of the computer, places, folder filters,
//! lists of files, and thumbnails of PDF files.

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::SystemTime;

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

use crate::commands::{handlers, option_on, show_pane};
use crate::i18n::{t, tf};
use crate::state::{AppState, ConfirmAction, Dialog, Pane, ToastKind};

/// Where the explorer is and what it selects.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ExplorerState {
    pub dir: Option<PathBuf>,
    pub back: Vec<PathBuf>,
    pub forward: Vec<PathBuf>,
    pub selected: BTreeSet<PathBuf>,
    /// Selected custom place (for moving it up or down).
    pub place: Option<PathBuf>,
    /// Bumped to read the folder again.
    pub generation: u64,
}

pub static EXPLORER: GlobalSignal<ExplorerState> = Signal::global(ExplorerState::default);

/// A filter of the files shown: name and extensions (`*` for every file).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FolderFilter {
    pub name: String,
    pub exts: Vec<String>,
}

/// One entry of a folder.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub path: PathBuf,
    pub name: String,
    pub dir: bool,
    pub size: u64,
    pub modified: Option<SystemTime>,
}

pub fn home() -> PathBuf {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

/// The folder shown: the chosen one, else the folder of the active document, else home.
pub fn current_dir(st: AppState) -> PathBuf {
    if let Some(d) = EXPLORER.read().dir.clone() {
        return d;
    }
    st.read_active(|t| t.doc.path().and_then(|p| p.parent()).map(Path::to_path_buf))
        .flatten()
        .unwrap_or_else(home)
}

/// Built-in places (label key, path) that exist on this computer.
pub fn builtin_places() -> Vec<(String, PathBuf)> {
    let h = home();
    let mut out = vec![("explorer.place.home".to_string(), h.clone())];
    for (key, sub) in [
        ("explorer.place.desktop", "Desktop"),
        ("explorer.place.documents", "Documents"),
        ("explorer.place.downloads", "Downloads"),
    ] {
        let p = h.join(sub);
        if p.is_dir() {
            out.push((key.to_string(), p));
        }
    }
    #[cfg(target_os = "windows")]
    for letter in b'A'..=b'Z' {
        let p = PathBuf::from(format!("{}:\\", letter as char));
        if p.exists() {
            out.push((String::new(), p));
        }
    }
    #[cfg(not(target_os = "windows"))]
    out.push((String::new(), PathBuf::from("/")));
    out
}

pub fn custom_places(st: AppState) -> Vec<PathBuf> {
    serde_json::from_str(&st.value("explorer.places")).unwrap_or_default()
}

fn save_places(st: AppState, list: &[PathBuf]) {
    st.set_value("explorer.places", serde_json::to_string(list).unwrap_or_default());
}

/// Built-in filters followed by the user's own.
pub fn filters(st: AppState) -> Vec<FolderFilter> {
    let mut out = vec![
        FolderFilter {
            name: t(st.lang(), "explorer.filter.pdf"),
            exts: vec!["pdf".into()],
        },
        FolderFilter {
            name: t(st.lang(), "explorer.filter.supported"),
            exts: folitecta_core::combine::source_exts()
                .into_iter()
                .map(str::to_string)
                .collect(),
        },
        FolderFilter {
            name: t(st.lang(), "explorer.filter.all"),
            exts: vec!["*".into()],
        },
    ];
    out.extend(custom_filters(st));
    out
}

pub fn custom_filters(st: AppState) -> Vec<FolderFilter> {
    serde_json::from_str(&st.value("explorer.filters")).unwrap_or_default()
}

pub fn save_custom_filters(st: AppState, list: &[FolderFilter]) {
    st.set_value(
        "explorer.filters",
        serde_json::to_string(list).unwrap_or_default(),
    );
}

/// Index of the filter in use.
pub fn filter_index(st: AppState) -> usize {
    st.value("explorer.filter").parse().unwrap_or(0)
}

/// Extensions typed by a person: `pdf, *.png; .txt` gives pdf, png, txt.
pub fn parse_exts(s: &str) -> Vec<String> {
    s.split([',', ';', ' '])
        .map(|e| {
            e.trim()
                .trim_start_matches('*')
                .trim_start_matches('.')
                .to_lowercase()
        })
        .filter(|e| !e.is_empty())
        .collect()
}

/// Entries of a folder that pass the filter: folders first, then files, by name.
pub fn entries(dir: &Path, filter: &FolderFilter) -> Vec<Entry> {
    let all = filter.exts.iter().any(|e| e == "*");
    let mut out: Vec<Entry> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let path = e.path();
            let name = e.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                return None;
            }
            let meta = e.metadata().ok()?;
            let is_dir = meta.is_dir();
            if !is_dir && !all {
                let ext = path
                    .extension()
                    .and_then(|x| x.to_str())
                    .unwrap_or("")
                    .to_lowercase();
                if !filter.exts.contains(&ext) {
                    return None;
                }
            }
            Some(Entry {
                path,
                name,
                dir: is_dir,
                size: if is_dir { 0 } else { meta.len() },
                modified: meta.modified().ok(),
            })
        })
        .collect();
    out.sort_by(|a, b| {
        b.dir
            .cmp(&a.dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    out
}

/// Entries of the folder shown, with the filter in use.
pub fn shown_entries(st: AppState) -> Vec<Entry> {
    let f = filters(st);
    let filter = f.get(filter_index(st)).cloned().unwrap_or_else(|| f[0].clone());
    entries(&current_dir(st), &filter)
}

/// Go to a folder (remembering where we were).
pub fn go(st: AppState, dir: PathBuf) {
    let cur = current_dir(st);
    let mut e = EXPLORER.write();
    if cur != dir {
        e.back.push(cur);
        e.forward.clear();
    }
    e.dir = Some(dir);
    e.selected.clear();
}

fn back(st: AppState) {
    let cur = current_dir(st);
    let mut e = EXPLORER.write();
    if let Some(p) = e.back.pop() {
        e.forward.push(cur);
        e.dir = Some(p);
        e.selected.clear();
    }
}

fn forward(st: AppState) {
    let cur = current_dir(st);
    let mut e = EXPLORER.write();
    if let Some(p) = e.forward.pop() {
        e.back.push(cur);
        e.dir = Some(p);
        e.selected.clear();
    }
}

fn up(st: AppState) {
    if let Some(parent) = current_dir(st).parent().map(Path::to_path_buf) {
        go(st, parent);
    }
}

/// The folder of the active document.
fn sync(st: AppState) {
    match st
        .read_active(|t| t.doc.path().and_then(|p| p.parent()).map(Path::to_path_buf))
        .flatten()
    {
        Some(d) => {
            go(st, d);
            show_pane(st, Pane::Explorer);
        }
        None => st.toast(ToastKind::Info, t(st.lang(), "toast.save_first"), ""),
    }
}

fn refresh() {
    EXPLORER.write().generation += 1;
}

/// Open a file, or enter a folder.
pub fn activate(st: AppState, entry: &Entry) {
    if entry.dir {
        go(st, entry.path.clone());
        return;
    }
    let is_pdf = entry
        .path
        .extension()
        .and_then(|x| x.to_str())
        .is_some_and(|x| x.eq_ignore_ascii_case("pdf"));
    if is_pdf {
        crate::files::open_path(st, entry.path.clone(), None);
    } else if let Err(e) = crate::util::open_external(&entry.path.display().to_string()) {
        st.error(e);
    }
}

fn add_place(st: AppState) {
    let dir = current_dir(st);
    let mut list = custom_places(st);
    if !list.contains(&dir) {
        list.push(dir.clone());
        save_places(st, &list);
    }
    EXPLORER.write().place = Some(dir);
    show_pane(st, Pane::Explorer);
}

/// Move the selected custom place up or down.
fn move_place(st: AppState, delta: isize) {
    let Some(p) = EXPLORER.peek().place.clone() else {
        st.toast(ToastKind::Info, t(st.lang(), "toast.select_place"), "");
        return;
    };
    let mut list = custom_places(st);
    let Some(i) = list.iter().position(|x| *x == p) else {
        return;
    };
    let j = i as isize + delta;
    if j < 0 || j as usize >= list.len() {
        return;
    }
    list.swap(i, j as usize);
    save_places(st, &list);
}

/// Remove a custom place.
pub fn remove_place(st: AppState, p: &Path) {
    let mut list = custom_places(st);
    list.retain(|x| x != p);
    save_places(st, &list);
}

fn delete(st: AppState) {
    if EXPLORER.peek().selected.is_empty() {
        st.toast(ToastKind::Info, t(st.lang(), "toast.select_files"), "");
        return;
    }
    let lang = st.lang();
    let n = EXPLORER.peek().selected.len();
    st.set_dialog(Dialog::Confirm {
        title: t(lang, "explorer.delete.title"),
        body: tf(lang, "explorer.delete.body", &[("count", &n.to_string())]),
        action: ConfirmAction::TrashFiles,
    });
}

/// Move the selected files and folders to the recycle bin.
pub fn trash_selected(st: AppState) {
    let sel: Vec<PathBuf> = EXPLORER.peek().selected.iter().cloned().collect();
    let mut done = 0;
    for p in &sel {
        match crate::util::move_to_trash(p) {
            Ok(()) => done += 1,
            Err(e) => st.error(format!("{}: {e}", p.display())),
        }
    }
    EXPLORER.write().selected.clear();
    refresh();
    st.toast(
        ToastKind::Ok,
        tf(st.lang(), "toast.files_trashed", &[("count", &done.to_string())]),
        "",
    );
}

/// Create a folder in the folder shown.
pub fn create_folder(st: AppState, name: &str) -> Result<PathBuf, String> {
    let name = name.trim();
    if name.is_empty() || name.contains(['/', '\\', ':', '*', '?', '"', '<', '>', '|']) {
        return Err(t(st.lang(), "toast.bad_file_name"));
    }
    let p = current_dir(st).join(name);
    if p.exists() {
        return Err(tf(st.lang(), "toast.file_exists", &[("name", name)]));
    }
    std::fs::create_dir(&p).map_err(|e| e.to_string())?;
    refresh();
    EXPLORER.write().selected = BTreeSet::from([p.clone()]);
    Ok(p)
}

fn save_list(st: AppState, selection: bool) {
    let paths: Vec<String> = if selection {
        EXPLORER
            .peek()
            .selected
            .iter()
            .map(|p| p.display().to_string())
            .collect()
    } else {
        shown_entries(st)
            .into_iter()
            .filter(|e| !e.dir)
            .map(|e| e.path.display().to_string())
            .collect()
    };
    if paths.is_empty() {
        st.toast(ToastKind::Info, t(st.lang(), "toast.select_files"), "");
        return;
    }
    crate::files::save_text_file(st, "files.txt", paths.join("\n") + "\n");
}

/// Paths listed in a text file (one per line), the ones that exist.
pub fn read_list(text: &str) -> Vec<PathBuf> {
    text.lines()
        .map(|l| l.trim().trim_matches('"'))
        .filter(|l| !l.is_empty())
        .map(PathBuf::from)
        .filter(|p| p.exists())
        .collect()
}

fn load_list(st: AppState) {
    spawn(async move {
        let Some(file) = rfd::AsyncFileDialog::new()
            .add_filter("TXT", &["txt", "lst"])
            .pick_file()
            .await
        else {
            return;
        };
        let text = String::from_utf8_lossy(&file.read().await).to_string();
        let pdfs: Vec<PathBuf> = read_list(&text)
            .into_iter()
            .filter(|p| {
                p.extension()
                    .and_then(|x| x.to_str())
                    .is_some_and(|x| x.eq_ignore_ascii_case("pdf"))
            })
            .collect();
        if pdfs.is_empty() {
            st.toast(ToastKind::Info, t(st.lang(), "toast.no_pdf_in_list"), "");
        }
        for p in pdfs {
            crate::files::open_path(st, p, None);
        }
    });
}

fn edit_custom(st: AppState) {
    let i = filter_index(st);
    if i < 3 {
        st.toast(ToastKind::Info, t(st.lang(), "toast.select_custom_filter"), "");
        return;
    }
    st.set_dialog(Dialog::FolderFilter(Some(i - 3)));
}

fn remove_custom(st: AppState) {
    let i = filter_index(st);
    if i < 3 {
        st.toast(ToastKind::Info, t(st.lang(), "toast.select_custom_filter"), "");
        return;
    }
    let mut list = custom_filters(st);
    if i - 3 < list.len() {
        list.remove(i - 3);
        save_custom_filters(st, &list);
        st.set_value("explorer.filter", "0");
    }
}

// ---- Thumbnails -----------------------------------------------------------------

type ThumbKey = (PathBuf, Option<SystemTime>);

static THUMBS: LazyLock<Mutex<HashMap<ThumbKey, Arc<Vec<u8>>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Path of a file written into a URL path segment (hex of its bytes).
pub fn thumb_url(path: &Path) -> String {
    let hex: String = path
        .to_string_lossy()
        .bytes()
        .map(|b| format!("{b:02x}"))
        .collect();
    format!("/thumb/{hex}.png")
}

fn path_of_url(url: &str) -> Option<PathBuf> {
    let hex = url
        .trim_start_matches('/')
        .strip_prefix("thumb/")?
        .strip_suffix(".png")?;
    let bytes: Option<Vec<u8>> = (0..hex.len())
        .step_by(2)
        .map(|i| hex.get(i..i + 2).and_then(|h| u8::from_str_radix(h, 16).ok()))
        .collect();
    Some(PathBuf::from(String::from_utf8(bytes?).ok()?))
}

/// The thumbnail of the first page of a PDF file (rendered once per change of the file).
pub fn thumbnail(url: &str) -> Option<Arc<Vec<u8>>> {
    let path = path_of_url(url)?;
    let modified = std::fs::metadata(&path).ok()?.modified().ok();
    let key = (path.clone(), modified);
    if let Some(png) = THUMBS.lock().ok()?.get(&key) {
        return Some(png.clone());
    }
    let data = std::fs::read(&path).ok()?;
    let renderer = folitecta_core::render::Renderer::new(Arc::new(data)).ok()?;
    let page = renderer.render_page(0, 0.25).ok()?;
    let png = Arc::new(page.png);
    if let Ok(mut cache) = THUMBS.lock() {
        if cache.len() > 400 {
            cache.clear();
        }
        cache.insert(key, png.clone());
    }
    Some(png)
}

/// Check states of the explorer commands.
pub fn explorer_state(st: AppState, id: &str) -> Option<bool> {
    match id {
        "explorer.thumb.loading" => Some(option_on(st, "explorer.thumbs")),
        _ => None,
    }
}

handlers! {
    "explorer" => |st| show_pane(st, Pane::Explorer),
    "explorer.refresh" => |_| refresh(),
    "explorer.delete" => delete,
    "explorer.folder.back" => back,
    "explorer.folder.forward" => forward,
    "explorer.folder.up" => up,
    "explorer.folder.sync" => sync,
    "explorer.folder.create" => |st| st.set_dialog(Dialog::NewFolder),
    "explorer.places.nav.up" => |st| move_place(st, -1),
    "explorer.places.nav.down" => |st| move_place(st, 1),
    "explorer.thumb.loading" => |st| { let on = option_on(st, "explorer.thumbs"); st.set_option("explorer.thumbs", !on); },
    "addPlace" => add_place,
    "folderExts.addCustom" => |st| st.set_dialog(Dialog::FolderFilter(None)),
    "folderExts.editCustom" => edit_custom,
    "folderExts.removeCustom" => remove_custom,
    "folderExts.fileList.saveAll" => |st| save_list(st, false),
    "folderExts.fileList.saveSel" => |st| save_list(st, true),
    "folderExts.fileList.loadAll" => load_list,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extensions_lists_and_urls() {
        assert_eq!(parse_exts("pdf, *.PNG; .txt  x"), vec!["pdf", "png", "txt", "x"]);
        let p = PathBuf::from("C:\\Docs\\a b é.pdf");
        assert_eq!(path_of_url(&thumb_url(&p)), Some(p));
        assert!(read_list("\n  \"/definitely/not/here.pdf\"\n").is_empty());
    }
}
