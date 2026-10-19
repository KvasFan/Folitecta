//! Search options shared by the search pane and the Find menu, words near
//! each other, result groups, and searching the PDF files of a folder.

use std::path::{Path, PathBuf};

use dioxus::prelude::*;
use folitecta_core::search::{FoundIn, OtherHit, Proximity, SearchScopes};
use folitecta_core::{Document, SearchOptions};

use crate::commands::{handlers, no_doc, option_on};
use crate::i18n::{t, tf};
use crate::state::{AppState, Dialog, FolderHit, FolderSearch, Pane, ToastKind};

/// Where a search looks: command ids (pane and Find menu) and the option key.
const SCOPES: &[(&str, &str, &str)] = &[
    (
        "searchView.includePageText",
        "document.find.includePageText",
        "search.pageText",
    ),
    (
        "searchView.includeAnnotations",
        "document.find.includeAnnotations",
        "search.comments",
    ),
    (
        "searchView.includeBookmarks",
        "document.find.includeBookmarks",
        "search.bookmarks",
    ),
    (
        "searchView.includeFormFields",
        "document.find.includeFormFields",
        "search.fields",
    ),
    (
        "searchView.includeExtLinks",
        "document.find.includeExtLinks",
        "search.links",
    ),
    ("searchView.includeDocInfo", "", "search.docInfo"),
    ("searchView.includeAttachments", "", "search.attachments"),
    (
        "searchView.ignoreDiacritics",
        "document.find.ignoreDiacritics",
        "search.diacritics",
    ),
];

/// Option key of a scope or diacritics command.
fn scope_key(id: &str) -> Option<&'static str> {
    SCOPES
        .iter()
        .find(|(a, b, _)| *a == id || (!b.is_empty() && *b == id))
        .map(|(_, _, k)| *k)
}

/// Words near each other: command id and value.
const PROXIMITY: &[(&str, &str)] = &[
    ("searchView.proximity.adjacent", "adjacent"),
    ("searchView.proximity.samePara", "paragraph"),
    ("searchView.proximity.samePage", "page"),
    ("searchView.proximity.sameDoc", "document"),
];

pub fn proximity(st: AppState) -> Proximity {
    match st.ui.peek().value("search.proximity").unwrap_or("") {
        "adjacent" => Proximity::Adjacent,
        "paragraph" => Proximity::Paragraph,
        "page" => Proximity::Page,
        "document" => Proximity::Document,
        _ => Proximity::Phrase,
    }
}

/// Options, word mode and places of the next search.
pub fn search_settings(st: AppState) -> (SearchOptions, Proximity, SearchScopes) {
    let (case_sensitive, whole_words) = st
        .read_active(|t| (t.search.case_sensitive, t.search.whole_words))
        .unwrap_or((false, false));
    let on = |k: &str| option_on(st, k);
    let opts = SearchOptions {
        case_sensitive,
        whole_words,
        ignore_diacritics: on("search.diacritics"),
    };
    let scopes = SearchScopes {
        page_text: on("search.pageText"),
        comments: on("search.comments"),
        bookmarks: on("search.bookmarks"),
        fields: on("search.fields"),
        doc_info: on("search.docInfo"),
        attachments: on("search.attachments"),
        links: on("search.links"),
    };
    (opts, proximity(st), scopes)
}

/// Search again with the current query (after an option changed).
pub fn rerun(st: AppState) {
    let q = st.read_active(|t| t.search.query.clone()).unwrap_or_default();
    if !q.trim().is_empty() {
        crate::ui::panes::run_search(st, &q);
    }
}

fn toggle_scope(st: AppState, id: &str) {
    if let Some(k) = scope_key(id) {
        let next = !option_on(st, k);
        st.set_option(k, next);
        rerun(st);
    }
}

fn set_proximity(st: AppState, value: &str) {
    let cur = st.ui.peek().value("search.proximity").unwrap_or("").to_string();
    st.set_value("search.proximity", if cur == value { "" } else { value });
    rerun(st);
}

fn toggle_case(st: AppState) {
    st.with_active(|t| t.search.case_sensitive = !t.search.case_sensitive);
    rerun(st);
}

fn toggle_whole(st: AppState) {
    st.with_active(|t| t.search.whole_words = !t.search.whole_words);
    rerun(st);
}

/// Keys of the result groups: "p{page}" for pages, the kind for other places,
/// the file path for folder results.
pub fn group_keys(st: AppState) -> Vec<String> {
    st.read_active(|t| {
        let mut keys: Vec<String> = t.search.hits.iter().map(|h| format!("p{}", h.page)).collect();
        keys.extend(
            t.search
                .other
                .iter()
                .map(|o| crate::ui::panes::other_group(&o.found_in).to_string()),
        );
        if let Some(f) = &t.search.folder {
            keys.extend(f.hits.iter().map(|h| h.path.display().to_string()));
        }
        keys.sort();
        keys.dedup();
        keys
    })
    .unwrap_or_default()
}

fn collapse_all(st: AppState, collapse: bool) {
    let keys = group_keys(st);
    st.with_active(|t| {
        t.search.collapsed = if collapse {
            keys.into_iter().collect()
        } else {
            Default::default()
        };
    });
}

/// Open the current result: show it, or open the file of a folder result.
fn open_result(st: AppState) {
    if no_doc(st) {
        return;
    }
    let folder_hit = st
        .read_active(|t| t.search.folder.as_ref().and_then(|f| f.hits.first().cloned()))
        .flatten();
    let n = st.read_active(|t| t.search.hits.len()).unwrap_or(0);
    if n > 0 {
        let cur = st.read_active(|t| t.search.current).unwrap_or(0);
        crate::ui::panes::reveal_hit(st, cur);
    } else if let Some(h) = folder_hit {
        open_folder_hit(st, h);
    } else {
        st.toast(ToastKind::Info, t(st.lang(), "toast.not_found"), "");
    }
}

/// Show a match found outside the page text.
pub fn open_other(st: AppState, hit: OtherHit) {
    match hit.found_in {
        FoundIn::Comment { page, index, .. } => {
            st.with_active(|t| t.selected_annot = Some((page, index)));
            st.navigate(page, true);
        }
        FoundIn::Bookmark { path, page } => {
            st.with_active(|t| t.selected_bookmark = Some(path));
            if let Some(p) = page {
                st.navigate(p, true);
            }
        }
        FoundIn::Field { page, .. } => {
            if let Some(p) = page {
                st.navigate(p, true);
            }
        }
        FoundIn::DocInfo { .. } => st.set_dialog(Dialog::DocProps),
        FoundIn::Attachment { name } => {
            st.with_active(|t| {
                t.selected_attachment = t.doc.attachments().iter().position(|a| a.name == name);
            });
            crate::commands::show_pane(st, Pane::Attachments);
        }
        FoundIn::AttachmentPage { name, page } => {
            let data = st
                .read_active(|t| {
                    let a = t.doc.attachments().into_iter().find(|a| a.name == name)?;
                    Some((t.doc.attachment_data(&a).ok()?, t.doc.protection().cloned()))
                })
                .flatten();
            match data.map(|(d, p)| Document::from_bytes(d, None, None).map(|doc| (doc, p))) {
                Some(Ok((mut doc, protection))) => {
                    // An attachment of a protected document is protected like it.
                    if let Some(p) = &protection {
                        doc.inherit_protection(p);
                    }
                    doc.set_name_hint(&name);
                    crate::files::open_document_tab(st, doc);
                    st.navigate(page, true);
                }
                Some(Err(e)) => st.error(e),
                None => {}
            }
        }
        FoundIn::Link { page, .. } => st.navigate(page, true),
    }
}

/// Open the file of a folder result at its page.
pub fn open_folder_hit(st: AppState, h: FolderHit) {
    crate::files::open_path(st, h.path.clone(), None);
    let open = st
        .read_active(|t| t.doc.path() == Some(h.path.as_path()))
        .unwrap_or(false);
    if open {
        st.navigate(h.page, true);
    }
}

/// PDF files of a folder (and of its subfolders when `deep`).
pub fn pdf_files(root: &Path, deep: bool) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut dirs = vec![root.to_path_buf()];
    while let Some(dir) = dirs.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        let mut entries: Vec<_> = entries.flatten().collect();
        entries.sort_by_key(|e| e.file_name());
        for e in entries {
            let p = e.path();
            let Ok(ft) = e.file_type() else { continue };
            if ft.is_dir() {
                if deep {
                    dirs.push(p);
                }
            } else if p
                .extension()
                .is_some_and(|x| x.to_string_lossy().eq_ignore_ascii_case("pdf"))
            {
                out.push(p);
            }
        }
    }
    out
}

/// Search the PDF files of a folder for the current query (page text).
pub fn search_folder(st: AppState) {
    if no_doc(st) {
        return;
    }
    let query = st.read_active(|t| t.search.query.clone()).unwrap_or_default();
    if query.trim().is_empty() {
        st.toast(ToastKind::Info, t(st.lang(), "toast.search_query_first"), "");
        return;
    }
    let deep = option_on(st, "searchView.lookInSubFolders");
    let estimate = option_on(st, "searchView.estimateFolder");
    let (opts, prox, _) = search_settings(st);
    spawn(async move {
        let Some(dir) = rfd::AsyncFileDialog::new().pick_folder().await else {
            return;
        };
        let root = dir.path().to_path_buf();
        let files = {
            let r = root.clone();
            tokio::task::spawn_blocking(move || pdf_files(&r, deep))
                .await
                .unwrap_or_default()
        };
        if estimate {
            let bytes: u64 = files
                .iter()
                .filter_map(|f| std::fs::metadata(f).ok())
                .map(|m| m.len())
                .sum();
            st.toast(
                ToastKind::Info,
                tf(
                    st.lang(),
                    "toast.folder_estimate",
                    &[
                        ("count", &files.len().to_string()),
                        ("size", &crate::util::human_size(bytes)),
                    ],
                ),
                "",
            );
        }
        st.with_active(|t| {
            t.search.folder = Some(FolderSearch {
                root: root.clone(),
                query: query.clone(),
                files: files.len(),
                searched: 0,
                running: true,
                hits: Vec::new(),
            })
        });
        crate::commands::show_pane(st, Pane::Search);
        for (i, f) in files.into_iter().enumerate() {
            let q = query.clone();
            let path = f.clone();
            let found: Vec<FolderHit> = tokio::task::spawn_blocking(move || {
                Document::open(&path)
                    .map(|d| {
                        d.search_words(&q, opts, prox)
                            .into_iter()
                            .map(|h| FolderHit {
                                path: path.clone(),
                                page: h.page,
                                snippet: h.snippet,
                            })
                            .collect()
                    })
                    .unwrap_or_default()
            })
            .await
            .unwrap_or_default();
            let still = st
                .with_active(|t| match t.search.folder.as_mut() {
                    Some(fs) if fs.root == root && fs.running => {
                        fs.searched = i + 1;
                        fs.hits.extend(found);
                        true
                    }
                    _ => false,
                })
                .unwrap_or(false);
            if !still {
                return;
            }
        }
        let n = st
            .with_active(|t| {
                t.search.folder.as_mut().map(|fs| {
                    fs.running = false;
                    fs.hits.len()
                })
            })
            .flatten()
            .unwrap_or(0);
        st.toast(
            ToastKind::Ok,
            tf(st.lang(), "toast.folder_done", &[("count", &n.to_string())]),
            "",
        );
    });
}

/// Check states of the search options.
pub fn search_state(st: AppState, id: &str) -> Option<bool> {
    if let Some(k) = scope_key(id) {
        return Some(option_on(st, k));
    }
    if let Some((_, v)) = PROXIMITY.iter().find(|(c, _)| *c == id) {
        return Some(st.ui.peek().value("search.proximity") == Some(*v));
    }
    match id {
        "searchView.caseSensitive" | "document.find.caseSensitive" => {
            st.read_active(|t| t.search.case_sensitive)
        }
        "searchView.wholeWords" | "document.find.wholeWords" => st.read_active(|t| t.search.whole_words),
        _ => None,
    }
}

handlers! {
    "searchView.caseSensitive" => toggle_case,
    "searchView.wholeWords" => toggle_whole,
    "document.find.caseSensitive" => toggle_case,
    "document.find.wholeWords" => toggle_whole,
    "searchView.ignoreDiacritics" => |st| toggle_scope(st, "searchView.ignoreDiacritics"),
    "document.find.ignoreDiacritics" => |st| toggle_scope(st, "document.find.ignoreDiacritics"),
    "searchView.includePageText" => |st| toggle_scope(st, "searchView.includePageText"),
    "document.find.includePageText" => |st| toggle_scope(st, "document.find.includePageText"),
    "searchView.includeAnnotations" => |st| toggle_scope(st, "searchView.includeAnnotations"),
    "document.find.includeAnnotations" => |st| toggle_scope(st, "document.find.includeAnnotations"),
    "searchView.includeBookmarks" => |st| toggle_scope(st, "searchView.includeBookmarks"),
    "document.find.includeBookmarks" => |st| toggle_scope(st, "document.find.includeBookmarks"),
    "searchView.includeFormFields" => |st| toggle_scope(st, "searchView.includeFormFields"),
    "document.find.includeFormFields" => |st| toggle_scope(st, "document.find.includeFormFields"),
    "searchView.includeExtLinks" => |st| toggle_scope(st, "searchView.includeExtLinks"),
    "document.find.includeExtLinks" => |st| toggle_scope(st, "document.find.includeExtLinks"),
    "searchView.includeDocInfo" => |st| toggle_scope(st, "searchView.includeDocInfo"),
    "searchView.includeAttachments" => |st| toggle_scope(st, "searchView.includeAttachments"),
    "searchView.proximity.adjacent" => |st| set_proximity(st, "adjacent"),
    "searchView.proximity.samePara" => |st| set_proximity(st, "paragraph"),
    "searchView.proximity.samePage" => |st| set_proximity(st, "page"),
    "searchView.proximity.sameDoc" => |st| set_proximity(st, "document"),
    "searchView.results.expandAll" => |st| collapse_all(st, false),
    "searchView.results.collapseAll" => |st| collapse_all(st, true),
    "searchView.results.open" => open_result,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pane_and_menu_share_options() {
        assert_eq!(
            scope_key("searchView.includeAnnotations"),
            scope_key("document.find.includeAnnotations")
        );
        assert_eq!(scope_key("searchView.includeDocInfo"), Some("search.docInfo"));
        assert_eq!(scope_key(""), None);
    }

    #[test]
    fn folders_list_their_pdf_files() {
        let root = std::env::temp_dir().join(format!("foli-folder-{}", std::process::id()));
        let sub = root.join("sub");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(root.join("a.pdf"), b"x").unwrap();
        std::fs::write(root.join("b.txt"), b"x").unwrap();
        std::fs::write(sub.join("c.PDF"), b"x").unwrap();
        assert_eq!(pdf_files(&root, false).len(), 1);
        assert_eq!(pdf_files(&root, true).len(), 2);
        let _ = std::fs::remove_dir_all(root);
    }
}
