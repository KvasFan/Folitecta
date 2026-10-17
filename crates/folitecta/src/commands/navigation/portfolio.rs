//! Portfolio commands: new portfolios, files and folders, layout, preview,
//! navigation, columns and properties.

use base64::Engine;
use dioxus::prelude::*;
use folitecta_core::Document;
use folitecta_core::portfolio::{Portfolio, SchemaField};

use crate::commands::{core_result, handlers, no_doc, option_on};
use crate::i18n::{t, tf};
use crate::state::{AppState, Dialog, PortfolioState, ToastKind};

/// The portfolio of the active document and the state of its files view.
pub fn current(st: AppState) -> Option<(Portfolio, PortfolioState)> {
    st.read_active(|t| t.doc.portfolio().map(|p| (p, t.portfolio.clone())))
        .flatten()
}

fn need_portfolio(st: AppState) -> Option<(Portfolio, PortfolioState)> {
    if no_doc(st) {
        return None;
    }
    let r = current(st);
    if r.is_none() {
        st.toast(ToastKind::Info, t(st.lang(), "toast.not_portfolio"), "");
    }
    r
}

/// "folders", "flat" or "compact".
pub fn layout(st: AppState) -> String {
    st.ui
        .peek()
        .value("portfolio.layout")
        .unwrap_or("folders")
        .to_string()
}

/// "right", "bottom", "off" or "auto".
pub fn preview_mode(st: AppState) -> String {
    st.ui
        .peek()
        .value("portfolio.preview")
        .unwrap_or("auto")
        .to_string()
}

fn new_portfolio(st: AppState) {
    spawn(async move {
        let Some(files) = rfd::AsyncFileDialog::new().pick_files().await else {
            return;
        };
        let mut items = Vec::new();
        for f in files {
            items.push((f.file_name(), f.read().await));
        }
        let title = t(st.lang(), "portfolio.cover_title");
        match Document::new_portfolio(items, &title) {
            Ok(mut doc) => {
                doc.set_name_hint(&format!("{title}.pdf"));
                crate::files::open_document_tab(st, doc);
            }
            Err(e) => st.error(e),
        }
    });
}

fn add_files(st: AppState) {
    let Some((_, ps)) = need_portfolio(st) else { return };
    spawn(async move {
        let Some(files) = rfd::AsyncFileDialog::new().pick_files().await else {
            return;
        };
        let mut n = 0;
        for f in files {
            let data = f.read().await;
            match st.with_active(|t| t.doc.portfolio_add_file(&f.file_name(), data, ps.folder)) {
                Some(Err(e)) => st.error(e),
                Some(Ok(())) => n += 1,
                None => {}
            }
        }
        crate::files::set_window_title(st);
        st.toast(
            ToastKind::Ok,
            tf(st.lang(), "toast.portfolio_added", &[("count", &n.to_string())]),
            "",
        );
    });
}

/// Add a folder from disk with its files and subfolders.
fn add_folder(st: AppState) {
    let Some((_, ps)) = need_portfolio(st) else { return };
    spawn(async move {
        let Some(dir) = rfd::AsyncFileDialog::new().pick_folder().await else {
            return;
        };
        let mut stack = vec![(dir.path().to_path_buf(), ps.folder)];
        let mut n = 0;
        let mut guard = 0;
        while let Some((path, parent)) = stack.pop() {
            guard += 1;
            if guard > 2000 {
                break;
            }
            let name = path
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "Folder".into());
            let folder = match st.with_active(|t| t.doc.portfolio_create_folder(&name, parent)) {
                Some(Ok(id)) => Some(id),
                Some(Err(e)) => {
                    st.error(e);
                    continue;
                }
                None => return,
            };
            let Ok(entries) = std::fs::read_dir(&path) else {
                continue;
            };
            for e in entries.flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push((p, folder));
                } else if let Ok(data) = std::fs::read(&p) {
                    let fname = p
                        .file_name()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_default();
                    if let Some(Ok(())) = st.with_active(|t| t.doc.portfolio_add_file(&fname, data, folder)) {
                        n += 1;
                    }
                }
            }
        }
        crate::files::set_window_title(st);
        st.toast(
            ToastKind::Ok,
            tf(st.lang(), "toast.portfolio_added", &[("count", &n.to_string())]),
            "",
        );
    });
}

fn delete(st: AppState) {
    let Some((_, ps)) = need_portfolio(st) else { return };
    if ps.selected.is_empty() && ps.selected_folders.is_empty() {
        st.toast(ToastKind::Info, t(st.lang(), "toast.portfolio_select"), "");
        return;
    }
    let r = st.with_active(|t| {
        let r = t.doc.portfolio_delete(&ps.selected, &ps.selected_folders);
        t.portfolio.selected.clear();
        t.portfolio.selected_folders.clear();
        r
    });
    match r {
        Some(Ok(n)) => {
            crate::files::set_window_title(st);
            st.toast(
                ToastKind::Ok,
                tf(st.lang(), "toast.portfolio_deleted", &[("count", &n.to_string())]),
                "",
            );
        }
        Some(Err(e)) => st.error(e),
        None => {}
    }
}

fn select_all(st: AppState, all: bool) {
    let Some((p, ps)) = need_portfolio(st) else { return };
    st.with_active(|t| {
        if all {
            let flat = layout(st) == "flat";
            t.portfolio.selected = p
                .items
                .iter()
                .filter(|i| flat || i.folder == ps.folder.filter(|f| Some(*f) != p.root))
                .map(|i| i.key.clone())
                .collect();
            t.portfolio.selected_folders = if flat {
                Vec::new()
            } else {
                p.subfolders(ps.folder).iter().map(|f| f.id).collect()
            };
        } else {
            t.portfolio.selected.clear();
            t.portfolio.selected_folders.clear();
        }
    });
}

/// The selected files, or every file of the current folder.
fn chosen_files(p: &Portfolio, ps: &PortfolioState) -> Vec<folitecta_core::portfolio::PortfolioItem> {
    if ps.selected.is_empty() {
        p.files_in(ps.folder).into_iter().cloned().collect()
    } else {
        p.items
            .iter()
            .filter(|i| ps.selected.contains(&i.key))
            .cloned()
            .collect()
    }
}

fn extract(st: AppState) {
    let Some((p, ps)) = need_portfolio(st) else { return };
    let items = chosen_files(&p, &ps);
    if items.is_empty() {
        st.toast(ToastKind::Info, t(st.lang(), "toast.portfolio_select"), "");
        return;
    }
    spawn(async move {
        let Some(dir) = rfd::AsyncFileDialog::new().pick_folder().await else {
            return;
        };
        let mut n = 0;
        for it in items {
            let name = it.name.rsplit(['/', '\\']).next().unwrap_or("file").to_string();
            let target = dir.path().join(if name.trim().is_empty() {
                "file".into()
            } else {
                name
            });
            match st.read_active(|t| t.doc.save_attachment(&it.attachment, &target)) {
                Some(Ok(())) => n += 1,
                Some(Err(e)) => st.error(e),
                None => {}
            }
        }
        st.toast(
            ToastKind::Ok,
            tf(
                st.lang(),
                "toast.portfolio_extracted",
                &[("count", &n.to_string())],
            ),
            "",
        );
    });
}

fn item_info(st: AppState) {
    let Some((_, ps)) = need_portfolio(st) else { return };
    if let Some(k) = ps.selected.first() {
        st.set_dialog(Dialog::PortfolioItem(k.clone()));
    } else if let Some(f) = ps.selected_folders.first() {
        st.set_dialog(Dialog::PortfolioFolder { id: Some(*f) });
    } else {
        st.toast(ToastKind::Info, t(st.lang(), "toast.portfolio_select"), "");
    }
}

fn make_default(st: AppState) {
    let Some((_, ps)) = need_portfolio(st) else { return };
    let key = ps.selected.first().cloned();
    let r = st.with_active(|t| t.doc.set_portfolio_initial(key.as_deref()));
    core_result(st, r);
    let msg = if key.is_some() {
        "toast.portfolio_initial"
    } else {
        "toast.portfolio_initial_cover"
    };
    st.toast(ToastKind::Ok, t(st.lang(), msg), "");
}

/// Data URL of a picture for an image file, or of the first page of a PDF.
pub fn preview_image(name: &str, data: &[u8]) -> Option<String> {
    let lower = name.to_lowercase();
    let b64 = |mime: &str, bytes: &[u8]| {
        format!(
            "data:{mime};base64,{}",
            base64::engine::general_purpose::STANDARD.encode(bytes)
        )
    };
    if lower.ends_with(".png") {
        return Some(b64("image/png", data));
    }
    if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
        return Some(b64("image/jpeg", data));
    }
    if lower.ends_with(".pdf") || data.starts_with(b"%PDF") {
        let doc = Document::from_bytes(data.to_vec(), None, None).ok()?;
        let size = doc.page_size(0).ok()?;
        let scale = (360.0 / size.width.max(size.height)).clamp(0.1, 2.0);
        let page = doc.render_page_png(0, scale).ok()?;
        return Some(b64("image/png", &page.png));
    }
    None
}

/// Open a file of the portfolio: PDFs in a new tab, other files with the
/// system's application.
pub fn open_item(st: AppState, item: folitecta_core::portfolio::PortfolioItem) {
    let is_pdf = item.name.to_lowercase().ends_with(".pdf") || item.attachment.mime == "application/pdf";
    if is_pdf {
        let data = st
            .read_active(|t| t.doc.attachment_data(&item.attachment).ok())
            .flatten();
        match data.map(|d| Document::from_bytes(d, None, None)) {
            Some(Ok(mut doc)) => {
                doc.set_name_hint(&item.name);
                crate::files::open_document_tab(st, doc);
            }
            Some(Err(e)) => st.error(e),
            None => {}
        }
    } else {
        crate::commands::attachments::open_attachment(st, item.attachment);
    }
}

fn preview_file(st: AppState) {
    let Some((p, ps)) = need_portfolio(st) else { return };
    match ps
        .selected
        .first()
        .and_then(|k| p.items.iter().find(|i| &i.key == k))
    {
        Some(it) => open_item(st, it.clone()),
        None => st.toast(ToastKind::Info, t(st.lang(), "toast.portfolio_select"), ""),
    }
}

/// Open the selected PDF files and print every open document.
fn print(st: AppState) {
    let Some((p, ps)) = need_portfolio(st) else { return };
    let pdfs: Vec<_> = chosen_files(&p, &ps)
        .into_iter()
        .filter(|i| i.name.to_lowercase().ends_with(".pdf"))
        .collect();
    if pdfs.is_empty() {
        st.toast(ToastKind::Info, t(st.lang(), "toast.portfolio_no_pdf"), "");
        return;
    }
    for it in pdfs {
        open_item(st, it);
    }
    st.set_dialog(Dialog::Print(true));
}

fn navigate(st: AppState, dir: &str) {
    let Some((p, _)) = need_portfolio(st) else { return };
    st.with_active(|t| {
        let ps = &mut t.portfolio;
        match dir {
            "up" => {
                let parent = ps
                    .folder
                    .and_then(|f| p.folders.iter().find(|x| x.id == f))
                    .and_then(|f| f.parent)
                    .filter(|f| Some(*f) != p.root);
                if ps.folder.is_some() {
                    ps.open_folder(parent);
                }
            }
            "back" => {
                if let Some(prev) = ps.back.pop() {
                    ps.forward.push(ps.folder);
                    ps.folder = prev;
                }
            }
            _ => {
                if let Some(next) = ps.forward.pop() {
                    ps.back.push(ps.folder);
                    ps.folder = next;
                }
            }
        }
    });
}

fn set_view(st: AppState, view: &str) {
    if need_portfolio(st).is_none() {
        return;
    }
    let r = st.with_active(|t| t.doc.set_portfolio_view(view));
    core_result(st, r);
}

fn toggle_cover(st: AppState) {
    if need_portfolio(st).is_none() {
        return;
    }
    st.with_active(|t| t.portfolio.cover = !t.portfolio.cover);
}

/// Change the columns: add, delete or move the selected one.
fn edit_columns(st: AppState, op: &str) {
    let Some((p, ps)) = need_portfolio(st) else { return };
    let mut schema = p.schema.clone();
    let sel = ps.column.filter(|i| *i < schema.len());
    let new_sel = match (op, sel) {
        ("add", _) => {
            let mut n = schema.len() + 1;
            while schema.iter().any(|f| f.key == format!("Column{n}")) {
                n += 1;
            }
            schema.push(SchemaField {
                key: format!("Column{n}"),
                name: tf(st.lang(), "portfolio.column_name", &[("n", &n.to_string())]),
                kind: "S".into(),
                visible: true,
                editable: true,
            });
            Some(schema.len() - 1)
        }
        ("delete", Some(i)) => {
            schema.remove(i);
            None
        }
        ("up", Some(i)) if i > 0 => {
            schema.swap(i, i - 1);
            Some(i - 1)
        }
        ("down", Some(i)) if i + 1 < schema.len() => {
            schema.swap(i, i + 1);
            Some(i + 1)
        }
        (_, None) => {
            st.toast(ToastKind::Info, t(st.lang(), "toast.portfolio_column"), "");
            return;
        }
        _ => return,
    };
    let r = st.with_active(|t| {
        t.portfolio.column = new_sel;
        t.doc.set_portfolio_schema(&schema)
    });
    core_result(st, r);
}

fn toggle_tools(st: AppState) {
    let on = !option_on(st, "portfolio.tools");
    st.set_option("portfolio.tools", on);
}

/// Check states of the portfolio commands.
pub fn portfolio_state(st: AppState, id: &str) -> Option<bool> {
    let preview = |m: &str| Some(preview_mode(st) == m);
    let lay = |m: &str| Some(layout(st) == m);
    match id {
        "view.portfolio.preview.auto" => preview("auto"),
        "view.portfolio.preview.bottom" => preview("bottom"),
        "view.portfolio.preview.off" => preview("off"),
        "view.portfolio.preview.right" => preview("right"),
        "view.portfolio.view.compact" => lay("compact"),
        "view.portfolio.view.flat" => lay("flat"),
        "view.portfolio.view.folders" => lay("folders"),
        "view.portfolio.cover" => st.read_active(|t| t.portfolio.cover),
        "view.portfolioTools" => Some(option_on(st, "portfolio.tools")),
        _ => None,
    }
}

handlers! {
    "newPortfolio" => new_portfolio,
    "view.portfolio.addFiles" => add_files,
    "view.portfolio.addFolder" => add_folder,
    "view.portfolio.createFolder" => |st| { if need_portfolio(st).is_some() { st.set_dialog(Dialog::PortfolioFolder { id: None }) } },
    "view.portfolio.delete" => delete,
    "view.portfolio.selectAll" => |st| select_all(st, true),
    "view.portfolio.deselectAll" => |st| select_all(st, false),
    "view.portfolio.extractFiles" => extract,
    "view.portfolio.itemInfo" => item_info,
    "view.portfolio.makeDefault" => make_default,
    "view.portfolio.navDetails" => |st| set_view(st, "D"),
    "view.portfolio.navLayout" => |st| set_view(st, "T"),
    "view.portfolio.preview.auto" => |st| st.set_value("portfolio.preview", "auto"),
    "view.portfolio.preview.bottom" => |st| st.set_value("portfolio.preview", "bottom"),
    "view.portfolio.preview.off" => |st| st.set_value("portfolio.preview", "off"),
    "view.portfolio.preview.right" => |st| st.set_value("portfolio.preview", "right"),
    "view.portfolio.previewFile" => preview_file,
    "view.portfolio.print" => print,
    "view.portfolio.properties" => |st| { if need_portfolio(st).is_some() { st.set_dialog(Dialog::PortfolioProps) } },
    "view.portfolio.schemaView" => |st| { if need_portfolio(st).is_some() { st.set_dialog(Dialog::PortfolioProps) } },
    "view.portfolio.sortBy" => |st| { if need_portfolio(st).is_some() { st.set_dialog(Dialog::PortfolioProps) } },
    "view.portfolio.viewFields" => |st| { if need_portfolio(st).is_some() { st.set_dialog(Dialog::PortfolioProps) } },
    "view.portfolio.view.compact" => |st| st.set_value("portfolio.layout", "compact"),
    "view.portfolio.view.flat" => |st| st.set_value("portfolio.layout", "flat"),
    "view.portfolio.view.folders" => |st| st.set_value("portfolio.layout", "folders"),
    "view.portfolio.view.nav.up" => |st| navigate(st, "up"),
    "view.portfolio.view.nav.prev" => |st| navigate(st, "back"),
    "view.portfolio.view.nav.next" => |st| navigate(st, "forward"),
    "view.portfolio.cover" => toggle_cover,
    "view.portfolioTools" => toggle_tools,
    "portfolioSchemaList.add" => |st| edit_columns(st, "add"),
    "portfolioSchemaList.delete" => |st| edit_columns(st, "delete"),
    "portfolioSchemaList.up" => |st| edit_columns(st, "up"),
    "portfolioSchemaList.down" => |st| edit_columns(st, "down"),
}
