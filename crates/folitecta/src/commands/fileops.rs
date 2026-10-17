//! The document file (rename, move, recycle bin, save to), metadata
//! templates and XMP, guides as files, the document size and the continuous
//! zoom tool.

use std::path::{Path, PathBuf};

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

use super::{handlers, no_doc, select_tool};
use crate::i18n::{t, tf};
use crate::state::{AppState, Dialog, Guide, ToastKind, Tool};

// ---- The document file -----------------------------------------------------------

/// Path of the active document, or a message when it has never been saved.
fn need_path(st: AppState) -> Option<PathBuf> {
    if no_doc(st) {
        return None;
    }
    let path = st.read_active(|t| t.doc.path().map(Path::to_path_buf)).flatten();
    if path.is_none() {
        st.toast(ToastKind::Info, t(st.lang(), "toast.save_first"), "");
    }
    path
}

/// Point the active document, the history and the window title at a new path.
fn moved(st: AppState, from: &Path, to: PathBuf) {
    st.with_active(|t| t.doc.set_path(Some(to.clone())));
    {
        let mut ui = st.ui_mut();
        for h in ui.history.iter_mut() {
            if h.path == from {
                h.path = to.clone();
            }
        }
    }
    st.save_prefs();
    crate::files::set_window_title(st);
}

/// Move a file, copying it when the target is on another drive.
fn move_file(from: &Path, to: &Path) -> std::io::Result<()> {
    if std::fs::rename(from, to).is_ok() {
        return Ok(());
    }
    std::fs::copy(from, to)?;
    std::fs::remove_file(from)
}

/// Give the document file a new name in its folder (`.pdf` added when missing).
pub fn rename_to(st: AppState, name: &str) -> Result<PathBuf, String> {
    let Some(from) = need_path(st) else {
        return Err(String::new());
    };
    let name = name.trim();
    if name.is_empty() || name.contains(['/', '\\', ':', '*', '?', '"', '<', '>', '|']) {
        return Err(t(st.lang(), "toast.bad_file_name"));
    }
    let name = if Path::new(name).extension().is_some() {
        name.to_string()
    } else {
        format!("{name}.pdf")
    };
    let to = from.with_file_name(&name);
    if to == from {
        return Ok(to);
    }
    if to.exists() {
        return Err(tf(st.lang(), "toast.file_exists", &[("name", &name)]));
    }
    move_file(&from, &to).map_err(|e| e.to_string())?;
    moved(st, &from, to.clone());
    Ok(to)
}

fn rename_doc(st: AppState) {
    let Some(path) = need_path(st) else { return };
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    st.set_dialog(Dialog::RenameDoc(name));
}

fn move_doc(st: AppState) {
    let Some(from) = need_path(st) else { return };
    spawn(async move {
        let Some(dir) = rfd::AsyncFileDialog::new().pick_folder().await else {
            return;
        };
        let Some(name) = from.file_name() else { return };
        let to = dir.path().join(name);
        if to == from {
            return;
        }
        if to.exists() {
            st.toast(
                ToastKind::Warn,
                tf(
                    st.lang(),
                    "toast.file_exists",
                    &[("name", &to.display().to_string())],
                ),
                "",
            );
            return;
        }
        match move_file(&from, &to) {
            Ok(()) => {
                moved(st, &from, to.clone());
                st.toast(
                    ToastKind::Ok,
                    tf(
                        st.lang(),
                        "toast.file_moved",
                        &[("name", &to.display().to_string())],
                    ),
                    "",
                );
            }
            Err(e) => st.error(e),
        }
    });
}

fn trash_doc(st: AppState) {
    if need_path(st).is_some() {
        super::confirm(
            st,
            "dialog.trash.title",
            "dialog.trash.body",
            crate::state::ConfirmAction::TrashDocument,
        );
    }
}

/// Move the active document's file to the recycle bin and close it.
pub fn trash_now(st: AppState) {
    let Some(path) = need_path(st) else { return };
    match crate::util::move_to_trash(&path) {
        Ok(()) => {
            if let Some(id) = st.read_active(|t| t.id) {
                crate::files::close_tab(st, id, true, crate::state::AfterUnsaved::Close);
            }
            st.ui_mut().history.retain(|h| h.path != path);
            st.save_prefs();
            st.toast(ToastKind::Ok, t(st.lang(), "toast.trashed"), "");
        }
        Err(e) => st.error(e),
    }
}

// ---- Metadata templates ---------------------------------------------------------

/// Document information kept to be given to other documents.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MetaTemplate {
    pub name: String,
    pub title: String,
    pub author: String,
    pub subject: String,
    pub keywords: String,
}

pub fn templates(st: AppState) -> Vec<MetaTemplate> {
    serde_json::from_str(&st.value("meta.templates")).unwrap_or_default()
}

pub fn save_templates(st: AppState, list: &[MetaTemplate]) {
    st.set_value("meta.templates", serde_json::to_string(list).unwrap_or_default());
}

/// Keep the information of the active document as a template.
pub fn save_template(st: AppState, name: &str) -> bool {
    let name = name.trim();
    if name.is_empty() {
        return false;
    }
    let Some(info) = st.read_active(|t| t.doc.info()) else {
        return false;
    };
    let mut list = templates(st);
    list.retain(|x| x.name != name);
    list.push(MetaTemplate {
        name: name.to_string(),
        title: info.title,
        author: info.author,
        subject: info.subject,
        keywords: info.keywords,
    });
    list.sort_by_key(|x| x.name.to_lowercase());
    save_templates(st, &list);
    true
}

/// Give the active document the information of a template (empty fields keep theirs).
pub fn apply_template(st: AppState, tpl: &MetaTemplate) {
    let r = st.with_active(|t| {
        let i = t.doc.info();
        let pick = |a: &str, b: String| if a.trim().is_empty() { b } else { a.to_string() };
        t.doc.set_info(
            &pick(&tpl.title, i.title),
            &pick(&tpl.author, i.author),
            &pick(&tpl.subject, i.subject),
            &pick(&tpl.keywords, i.keywords),
        )
    });
    super::core_result(st, r);
}

/// Save the XMP packet of the active document (one made from its
/// information when it has none).
pub fn export_xmp(st: AppState) {
    if no_doc(st) {
        return;
    }
    let (packet, stem) = st
        .read_active(|t| {
            let name = t.doc.display_name();
            let stem = Path::new(&name)
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or(name);
            (t.doc.xmp_or_generated(), stem)
        })
        .unwrap_or_default();
    crate::files::save_text_file(st, &format!("{stem}.xmp"), packet);
}

// ---- Guides ------------------------------------------------------------------------

#[derive(Serialize, Deserialize)]
struct GuideFile {
    page: Option<usize>,
    horizontal: bool,
    pos: f32,
}

fn export_guides(st: AppState) {
    if no_doc(st) {
        return;
    }
    let guides: Vec<GuideFile> = st
        .read_active(|t| {
            t.guides
                .iter()
                .map(|g| GuideFile {
                    page: g.page,
                    horizontal: g.horizontal,
                    pos: g.pos,
                })
                .collect()
        })
        .unwrap_or_default();
    if guides.is_empty() {
        st.toast(ToastKind::Info, t(st.lang(), "toast.no_guides"), "");
        return;
    }
    crate::files::save_text_file(
        st,
        "guides.json",
        serde_json::to_string_pretty(&guides).unwrap_or_default(),
    );
}

/// Add the guides of a file written by "Export guides".
pub fn import_guides_from(st: AppState, text: &str) -> Result<usize, String> {
    let list: Vec<GuideFile> = serde_json::from_str(text).map_err(|e| e.to_string())?;
    let pages = st.page_count();
    let added: Vec<Guide> = list
        .into_iter()
        .filter(|g| g.pos.is_finite() && g.page.is_none_or(|p| p < pages))
        .map(|g| Guide {
            page: g.page,
            horizontal: g.horizontal,
            pos: g.pos,
        })
        .collect();
    let n = added.len();
    st.with_active(|t| {
        for g in added {
            if !t.guides.contains(&g) {
                t.guides.push(g);
            }
        }
    });
    Ok(n)
}

fn import_guides(st: AppState) {
    if no_doc(st) {
        return;
    }
    spawn(async move {
        let Some(file) = rfd::AsyncFileDialog::new()
            .add_filter("JSON", &["json"])
            .pick_file()
            .await
        else {
            return;
        };
        let text = String::from_utf8_lossy(&file.read().await).to_string();
        match import_guides_from(st, &text) {
            Ok(n) => st.toast(
                ToastKind::Ok,
                tf(st.lang(), "toast.guides_imported", &[("count", &n.to_string())]),
                "",
            ),
            Err(e) => st.error(e),
        }
    });
}

/// Copy the guides of the current page to every other page.
pub fn duplicate_guides(st: AppState) -> usize {
    if no_doc(st) {
        return 0;
    }
    let page = st.current_page();
    let pages = st.page_count();
    let n = st
        .with_active(|t| {
            let own: Vec<Guide> = t
                .guides
                .iter()
                .filter(|g| g.page == Some(page))
                .copied()
                .collect();
            let mut added = 0;
            for p in (0..pages).filter(|p| *p != page) {
                for g in &own {
                    let copy = Guide { page: Some(p), ..*g };
                    if !t.guides.contains(&copy) {
                        t.guides.push(copy);
                        added += 1;
                    }
                }
            }
            added
        })
        .unwrap_or(0);
    if n == 0 {
        st.toast(ToastKind::Info, t(st.lang(), "toast.no_page_guides"), "");
    } else {
        st.toast(
            ToastKind::Ok,
            tf(st.lang(), "toast.guides_copied", &[("count", &n.to_string())]),
            "",
        );
    }
    n
}

// ---- Links, size, zoom ---------------------------------------------------------------

fn export_links(st: AppState) {
    if no_doc(st) {
        return;
    }
    let lang = st.lang();
    let text = st
        .read_active(|t| {
            let mut s = format!("{}\t{}\n", t_page(lang), crate::i18n::t(lang, "links.target"));
            for l in t.doc.all_links() {
                s.push_str(&format!("{}\t{}\n", l.page + 1, l.target.describe()));
            }
            s
        })
        .unwrap_or_default();
    crate::files::save_text_file(st, "links.txt", text);
}

fn t_page(lang: crate::i18n::Lang) -> String {
    crate::i18n::t(lang, "status.page")
}

/// Size of the current page and of the file.
fn page_size(st: AppState) {
    if no_doc(st) {
        return;
    }
    let page = st.current_page();
    let msg = st
        .read_active(|t| {
            let s = t.page_sizes.get(page)?;
            let mm = |v: f32| v / 72.0 * 25.4;
            Some(tf(
                st.lang(),
                "toast.page_size",
                &[
                    ("page", &(page + 1).to_string()),
                    ("w", &format!("{:.0}", mm(s.width))),
                    ("h", &format!("{:.0}", mm(s.height))),
                    ("wpt", &format!("{:.0}", s.width)),
                    ("hpt", &format!("{:.0}", s.height)),
                    ("size", &crate::util::human_size(t.doc.bytes().len() as u64)),
                ],
            ))
        })
        .flatten();
    if let Some(m) = msg {
        st.toast(ToastKind::Info, m, "");
    }
}

/// Zoom of the continuous zoom tool while the mouse button is held: the
/// pointer's starting height and the zoom it started from.
pub static ZOOM_DRAG: GlobalSignal<Option<(f64, f32)>> = Signal::global(|| None);

/// Zoom for a vertical drag of `dy` pixels (up zooms in).
pub fn dyn_zoom(start: f32, dy: f64) -> f32 {
    (start * 1.01f32.powf((-dy) as f32)).clamp(0.1, 8.0)
}

/// Check states of these commands.
pub fn fileops_state(st: AppState, id: &str) -> Option<bool> {
    match id {
        "tool.zoomDyn" => Some((st.tool)() == Tool::ZoomDyn),
        _ => None,
    }
}

handlers! {
    "renameDoc" => rename_doc,
    "rename" => rename_doc,
    "moveDoc" => move_doc,
    "moveToTrash" => trash_doc,
    "saveTo" => crate::files::save_as,
    "xmp.show.templates" => |st| if !no_doc(st) { st.set_dialog(Dialog::MetaTemplates) },
    "xmp.export.template" => |st| if !no_doc(st) { st.set_dialog(Dialog::MetaTemplateSave) },
    "rulers.exportGuides" => export_guides,
    "rulers.importGuides" => import_guides,
    "rulers.duplicateGuides" => |st| { duplicate_guides(st); },
    "linksView.export" => export_links,
    "dsd.pageSize" => page_size,
    "tool.zoomDyn" => |st| select_tool(st, Tool::ZoomDyn),
}

#[cfg(test)]
mod tests {
    use super::dyn_zoom;

    #[test]
    fn dragging_up_zooms_in() {
        assert!(dyn_zoom(1.0, -50.0) > 1.6);
        assert!(dyn_zoom(1.0, 50.0) < 0.7);
        assert_eq!(dyn_zoom(1.0, 0.0), 1.0);
        assert_eq!(dyn_zoom(1.0, -10_000.0), 8.0);
    }
}
