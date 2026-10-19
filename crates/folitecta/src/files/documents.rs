//! Documents: open, new, save, save as, close, revert, the window title, and protected copies.

use super::*;

/// Open a document from disk and show it in a new tab.
pub fn open_path(st: AppState, path: PathBuf, password: Option<&str>) {
    // An e-mail message opens as a new document made from it.
    if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("eml")) {
        open_email(st, &path);
        return;
    }
    // Already open? Just activate it.
    let existing = st
        .docs
        .read()
        .iter()
        .position(|t| t.doc.path() == Some(path.as_path()));
    if let Some(idx) = existing {
        st.set_active(idx);
        return;
    }
    let result = match password {
        Some(pw) => Document::open_with_password(&path, pw),
        None => Document::open(&path),
    };
    match result {
        Ok(doc) => {
            let name = doc.display_name();
            let (title, pages) = (doc.info().title, doc.page_count());
            let id = st.alloc_id();
            st.docs_mut().push(DocTab::new(id, doc));
            let idx = st.docs.read().len() - 1;
            st.set_active(idx);
            st.ui_mut().push_recent(&path);
            crate::commands::history::record_open(st, &path, &title, pages);
            crate::commands::attachments::auto_show(st);
            st.save_prefs();
            set_window_title(st);
            st.toast(
                ToastKind::Ok,
                tf(st.lang(), "toast.opened", &[("name", &name)]),
                "",
            );
        }
        Err(CoreError::PasswordRequired) => st.set_dialog(Dialog::Password(path)),
        Err(e) => st.error(format!("{}: {e}", path.display())),
    }
}

/// Show the open dialog and open every selected file.
pub fn open_dialog(st: AppState) {
    spawn(async move {
        if let Some(files) = open_file_dialog().set_title("Open PDF").pick_files().await {
            for f in files {
                open_path(st, f.path().to_path_buf(), None);
            }
        }
    });
}

/// Create a blank document tab.
pub fn new_blank(st: AppState, pages: usize, width: f32, height: f32) {
    match Document::new_blank(pages, width, height) {
        Ok(mut doc) => {
            doc.clear_history();
            let id = st.alloc_id();
            st.docs_mut().push(DocTab::new(id, doc));
            let idx = st.docs.read().len() - 1;
            st.set_active(idx);
            set_window_title(st);
        }
        Err(e) => st.error(e),
    }
}

pub fn set_window_title(st: AppState) {
    let title = match st.read_active(|t| (t.title(), t.doc.is_dirty())) {
        Some((name, dirty)) => format!("{}{} - Folitecta", name, if dirty { " •" } else { "" }),
        None => "Folitecta".to_string(),
    };
    dioxus::desktop::window().set_title(&title);
}

/// Save the active document; falls back to Save As when it has no path.
pub fn save(st: AppState) {
    let has_path = st.read_active(|t| t.doc.path().is_some()).unwrap_or(false);
    if !has_path {
        save_as(st);
        return;
    }
    // A redaction not checked yet: the bytes are checked before they are written.
    if let Some(tab) = crate::commands::protect::unchecked_redaction(st) {
        if let Some(path) = st
            .read_active(|t| t.doc.path().map(|p| p.to_path_buf()))
            .flatten()
        {
            st.toast(
                ToastKind::Info,
                t(st.lang(), "toast.redaction_unchecked_save"),
                "",
            );
            dioxus::core::spawn_forever(async move {
                let _ = crate::commands::protect::save_redacted_copy_to(st, tab, path).await;
            });
            return;
        }
    }
    let result = st.with_active(|t| t.doc.save());
    match result {
        Some(Ok(())) => {
            let name = st.read_active(|t| t.title()).unwrap_or_default();
            st.toast(
                ToastKind::Ok,
                tf(st.lang(), "toast.saved", &[("name", &name)]),
                "",
            );
            set_window_title(st);
            // Saved: its recovery copy goes.
            crate::recovery::checkpoint(st);
        }
        Some(Err(e)) => st.error(e),
        None => st.toast(ToastKind::Warn, t(st.lang(), "toast.no_document"), ""),
    }
}

pub(crate) fn suggested_name(st: AppState, suffix: &str) -> String {
    st.read_active(|t| {
        let stem = t
            .doc
            .path()
            .and_then(|p| p.file_stem())
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "document".into());
        format!("{stem}{suffix}.pdf")
    })
    .unwrap_or_else(|| format!("document{suffix}.pdf"))
}

pub fn save_as(st: AppState) {
    if !st.has_doc() {
        st.toast(ToastKind::Warn, t(st.lang(), "toast.no_document"), "");
        return;
    }
    let name = suggested_name(st, "");
    spawn(async move {
        let Some(file) = pdf_dialog()
            .set_title("Save As")
            .set_file_name(&name)
            .save_file()
            .await
        else {
            return;
        };
        let path = file.path().to_path_buf();
        if let Some(tab) = crate::commands::protect::unchecked_redaction(st) {
            let _ = crate::commands::protect::save_redacted_copy_to(st, tab, path).await;
            return;
        }
        match st.with_active(|t| t.doc.save_as(&path)) {
            Some(Ok(())) => {
                st.ui_mut().push_recent(&path);
                st.save_prefs();
                set_window_title(st);
                // Saved: its recovery copy goes.
                crate::recovery::checkpoint(st);
                st.toast(
                    ToastKind::Ok,
                    tf(
                        st.lang(),
                        "toast.saved",
                        &[(
                            "name",
                            &path
                                .file_name()
                                .map(|n| n.to_string_lossy().to_string())
                                .unwrap_or_default(),
                        )],
                    ),
                    "",
                );
            }
            Some(Err(e)) => st.error(e),
            None => {}
        }
    });
}

pub fn save_copy(st: AppState) {
    save_copy_with(st, false);
}

/// Save a copy without the encryption of the file the document came from
/// (Save Unrestricted Copy: the user removes the protection on purpose).
pub fn save_unprotected_copy(st: AppState) {
    save_copy_with(st, true);
}

fn save_copy_with(st: AppState, unprotected: bool) {
    if !st.has_doc() {
        st.toast(ToastKind::Warn, t(st.lang(), "toast.no_document"), "");
        return;
    }
    let name = suggested_name(st, if unprotected { "-unrestricted" } else { "-copy" });
    spawn(async move {
        let Some(file) = pdf_dialog()
            .set_title(if unprotected {
                "Save an Unrestricted Copy"
            } else {
                "Save a Copy"
            })
            .set_file_name(&name)
            .save_file()
            .await
        else {
            return;
        };
        let path = file.path().to_path_buf();
        let saved = st.with_active(|t| {
            if unprotected {
                t.doc.save_unprotected_copy(&path)
            } else {
                t.doc.save_copy(&path)
            }
        });
        match saved {
            Some(Ok(())) => st.toast(
                ToastKind::Ok,
                tf(st.lang(), "toast.saved", &[("name", &path.display().to_string())]),
                "",
            ),
            Some(Err(e)) => st.error(e),
            None => {}
        }
    });
}

pub fn save_all(st: AppState) {
    let n = st.docs.read().len();
    let mut errors = Vec::new();
    for i in 0..n {
        let mut docs = st.docs_mut();
        let tab = &mut docs[i];
        if tab.doc.path().is_some() && tab.doc.is_dirty() {
            if let Err(e) = tab.doc.save() {
                errors.push(e.to_string());
            }
        }
    }
    for e in errors {
        st.error(e);
    }
    set_window_title(st);
    crate::recovery::checkpoint(st);
}

/// Close a tab by id; asks about unsaved changes unless `force`.
pub fn close_tab(st: AppState, id: u64, force: bool, then: AfterUnsaved) {
    let info = st.docs.read().iter().position(|t| t.id == id).map(|idx| {
        (
            idx,
            st.docs.read()[idx].doc.is_dirty(),
            st.docs.read()[idx].doc.path().map(Path::to_path_buf),
        )
    });
    let Some((idx, dirty, path)) = info else { return };
    if dirty && !force {
        st.set_dialog(Dialog::Unsaved { tab: id, then });
        return;
    }
    let tab = st.docs_mut().remove(idx);
    if let Some(p) = path {
        crate::commands::history::record_close(st, &p, tab.page, &tab.doc);
        st.closed_mut().push(p);
    }
    // The recovery copy of a closed document goes.
    crate::recovery::checkpoint(st);
    let n = st.docs.read().len();
    let active = (st.active)();
    if n == 0 {
        st.set_active(0);
    } else if active >= n {
        st.set_active(n - 1);
    } else if active > idx {
        st.set_active(active - 1);
    }
    set_window_title(st);
}

pub fn close_active(st: AppState) {
    if let Some(id) = st.read_active(|t| t.id) {
        close_tab(st, id, false, AfterUnsaved::Close);
    }
}

pub fn close_all(st: AppState) {
    let ids: Vec<(u64, bool)> = st.docs.read().iter().map(|t| (t.id, t.doc.is_dirty())).collect();
    if let Some((id, _)) = ids.iter().find(|(_, dirty)| *dirty) {
        st.set_dialog(Dialog::Unsaved {
            tab: *id,
            then: AfterUnsaved::CloseAll,
        });
        return;
    }
    for (id, _) in ids {
        close_tab(st, id, true, AfterUnsaved::Close);
    }
}

/// Continue after the unsaved-changes dialog.
pub fn resolve_unsaved(st: AppState, tab: u64, then: AfterUnsaved, save_first: bool) {
    if save_first {
        let idx = st.docs.read().iter().position(|t| t.id == tab);
        if let Some(idx) = idx {
            st.set_active(idx);
            let has_path = st.read_active(|t| t.doc.path().is_some()).unwrap_or(false);
            if !has_path {
                // Save As is asynchronous; the user can close again afterwards.
                save_as(st);
                return;
            }
            if let Some(Err(e)) = st.with_active(|t| t.doc.save()) {
                st.error(e);
                return;
            }
        }
    }
    close_tab(st, tab, true, then.clone());
    match then {
        AfterUnsaved::Close => {}
        AfterUnsaved::CloseAll => close_all(st),
        AfterUnsaved::Exit => exit_app(st),
    }
}

pub fn exit_app(st: AppState) {
    let dirty: Option<u64> = st.docs.read().iter().find(|t| t.doc.is_dirty()).map(|t| t.id);
    match dirty {
        Some(id) => st.set_dialog(Dialog::Unsaved {
            tab: id,
            then: AfterUnsaved::Exit,
        }),
        None => {
            // Left on purpose: nothing to recover next time.
            crate::recovery::forget_session();
            dioxus::desktop::window().close()
        }
    }
}

pub fn revert(st: AppState) {
    let path = st.read_active(|t| t.doc.path().map(Path::to_path_buf)).flatten();
    let Some(path) = path else {
        st.toast(ToastKind::Warn, t(st.lang(), "toast.no_document"), "");
        return;
    };
    match Document::open(&path) {
        Ok(doc) => {
            st.with_active(|t| {
                t.doc = doc;
                t.revision = u64::MAX; // force refresh
            });
            set_window_title(st);
            // Back to the file: no changes left to recover.
            crate::recovery::checkpoint(st);
        }
        Err(e) => st.error(e),
    }
}

pub fn reopen_closed(st: AppState) {
    let path = st.closed_mut().pop();
    match path {
        Some(p) => open_path(st, p, None),
        None => st.toast(ToastKind::Info, t(st.lang(), "label.none"), ""),
    }
}

/// Replace the active tab with the file at path (used after signing, when the
/// saved file is the document to continue with).
pub fn replace_active_with(st: AppState, path: PathBuf) {
    if let Some(i) = st.active_index() {
        st.docs_mut().remove(i);
        let n = st.docs.read().len();
        if n > 0 {
            st.set_active(i.min(n - 1));
        }
    }
    open_path(st, path, None);
}

/// Open a document created in memory (untitled) in a new tab.
pub fn open_document_tab(st: AppState, mut doc: Document) {
    doc.clear_history();
    let id = st.alloc_id();
    st.docs_mut().push(DocTab::new(id, doc));
    let n = st.docs.read().len();
    st.set_active(n - 1);
    set_window_title(st);
}

/// Write an encrypted copy of the active document.
pub fn save_encrypted(st: AppState, user: String, owner: String, perms: folitecta_core::SecurityPermissions) {
    let name = suggested_name(st, "-protected");
    spawn(async move {
        let Some(file) = pdf_dialog()
            .set_title("Save protected copy")
            .set_file_name(&name)
            .save_file()
            .await
        else {
            return;
        };
        let path = file.path().to_path_buf();
        match st.read_active(|t| t.doc.save_encrypted(&path, &user, &owner, perms)) {
            Some(Ok(())) => st.toast(
                ToastKind::Ok,
                tf(st.lang(), "toast.saved", &[("name", &path.display().to_string())]),
                t(st.lang(), "toast.protected_note"),
            ),
            Some(Err(e)) => st.error(e),
            None => {}
        }
    });
}
