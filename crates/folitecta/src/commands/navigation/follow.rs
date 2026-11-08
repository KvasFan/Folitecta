//! Following the links and buttons of a document. What stays inside it —
//! a page, a destination, a named action, clearing the form — runs at once;
//! what reaches outside — a web address, a file, another document — is
//! named and asked first, unless the document was trusted for the session
//! or the web address is on a site the user chose to open without asking
//! (kept in the settings, forgotten from the Preferences).
//! Programs are never started from a document, and form data is never sent.

use std::path::PathBuf;

use folitecta_core::LinkTarget;

use crate::i18n::tf;
use crate::state::{AppState, ConfirmAction, Dialog, ToastKind};

/// Extensions of files that run as programs when opened.
const PROGRAMS: &[&str] = &[
    "exe", "com", "bat", "cmd", "msi", "scr", "ps1", "vbs", "vbe", "js", "jse", "wsf", "wsh", "hta", "cpl",
    "jar", "sh", "bash", "zsh", "csh", "command", "app", "desktop", "appimage", "run", "bin", "py", "pl",
    "rb", "lnk", "reg",
];

/// A file named by a document, relative to the document's folder.
fn file_of(st: AppState, name: &str) -> PathBuf {
    let path = PathBuf::from(name.trim_start_matches("file://"));
    if path.is_absolute() {
        return path;
    }
    st.read_active(|t| t.doc.path().and_then(|p| p.parent()).map(|d| d.join(&path)))
        .flatten()
        .unwrap_or(path)
}

fn is_program(path: &std::path::Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| PROGRAMS.contains(&e.to_ascii_lowercase().as_str()))
}

/// The settings value of the sites whose links open without asking.
const SITES: &str = "links.allowedSites";

/// The site of a web address (`https://Example.org:8080/a` →
/// `https://example.org:8080`); none for other addresses.
pub fn web_site(url: &str) -> Option<String> {
    let (scheme, rest) = url.trim().split_once("://")?;
    let scheme = scheme.to_ascii_lowercase();
    if scheme != "http" && scheme != "https" {
        return None;
    }
    let host = rest.split(['/', '?', '#']).next()?;
    // Never a site with a user name in it (`https://a@b`).
    if host.is_empty() || host.contains('@') {
        return None;
    }
    Some(format!("{scheme}://{}", host.to_ascii_lowercase()))
}

/// The sites whose links open without asking.
pub fn allowed_sites(st: AppState) -> Vec<String> {
    st.value(SITES)
        .lines()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

/// Open the links to `site` without asking from now on.
pub fn allow_site(st: AppState, site: &str) {
    let mut sites = allowed_sites(st);
    if !sites.iter().any(|s| s == site) {
        sites.push(site.to_string());
        st.set_value(SITES, sites.join("\n"));
    }
}

/// Ask again for every site.
pub fn forget_sites(st: AppState) {
    st.set_value(SITES, "");
}

/// Whether `target` of document `tab` is asked before it opens.
pub fn needs_asking(st: AppState, target: &LinkTarget) -> bool {
    if st.read_active(|t| t.trust_links).unwrap_or(false) {
        return false;
    }
    match target {
        LinkTarget::Uri(u) => web_site(u).is_none_or(|s| !allowed_sites(st).contains(&s)),
        _ => true,
    }
}

/// Follow `target`, a link or button of the active document.
pub fn follow(st: AppState, target: LinkTarget) {
    let lang = st.lang();
    match &target {
        LinkTarget::Page(p) => st.go_to_page(*p),
        LinkTarget::Named(name) => {
            let page = st
                .read_active(|t| {
                    t.doc
                        .named_destinations()
                        .into_iter()
                        .find(|d| d.name == *name)
                        .and_then(|d| d.page)
                })
                .flatten();
            match page {
                Some(p) => st.go_to_page(p),
                None => st.toast(
                    ToastKind::Warn,
                    tf(lang, "toast.link_nowhere", &[("name", name)]),
                    "",
                ),
            }
        }
        LinkTarget::NamedAction(action) => {
            let (page, count) = (st.current_page(), st.page_count());
            match action.as_str() {
                "NextPage" => st.go_to_page((page + 1).min(count.saturating_sub(1))),
                "PrevPage" => st.go_to_page(page.saturating_sub(1)),
                "FirstPage" => st.go_to_page(0),
                "LastPage" => st.go_to_page(count.saturating_sub(1)),
                "Print" => crate::commands::dispatch(st, "print"),
                _ => st.toast(
                    ToastKind::Info,
                    tf(lang, "toast.link_action", &[("what", &target.describe())]),
                    "",
                ),
            }
        }
        LinkTarget::Reset => crate::commands::confirm(
            st,
            "cmd.forms.reset",
            "dialog.confirm.reset_form",
            ConfirmAction::ResetForm,
        ),
        LinkTarget::Submit(to) => st.toast(ToastKind::Info, tf(lang, "toast.link_submit", &[("to", to)]), ""),
        LinkTarget::Uri(_) | LinkTarget::Launch(_) | LinkTarget::Remote(_) => {
            if let LinkTarget::Launch(name) = &target {
                if is_program(&file_of(st, name)) {
                    st.toast(
                        ToastKind::Warn,
                        tf(lang, "toast.link_program", &[("file", name)]),
                        "",
                    );
                    return;
                }
            }
            match st.read_active(|t| t.id) {
                Some(tab) if needs_asking(st, &target) => st.set_dialog(Dialog::OpenTarget { tab, target }),
                _ => open(st, &target),
            }
        }
        LinkTarget::Other(_) => st.toast(
            ToastKind::Info,
            tf(lang, "toast.link_action", &[("what", &target.describe())]),
            "",
        ),
    }
}

/// Do the `actions` of a link or a button, in order: a script waits for the
/// one before it.
pub fn run_actions(st: AppState, actions: Vec<folitecta_core::ButtonAction>, target: Option<String>) {
    dioxus::core::spawn_forever(async move {
        for action in actions {
            match action {
                folitecta_core::ButtonAction::Target(target) => follow(st, target),
                folitecta_core::ButtonAction::Script(code) => {
                    crate::commands::javascript::run_of_document(st, code, target.clone()).await
                }
            }
        }
    });
}

/// Open what an external target names (asked already).
pub fn open(st: AppState, target: &LinkTarget) {
    match target {
        LinkTarget::Uri(u) => crate::commands::open_url(st, u),
        LinkTarget::Launch(name) => {
            let path = file_of(st, name);
            if is_program(&path) {
                return;
            }
            if let Err(e) = crate::util::open_external(&path.display().to_string()) {
                st.error(e);
            }
        }
        LinkTarget::Remote(name) => {
            let path = file_of(st, name);
            if path.exists() {
                crate::files::open_path(st, path, None);
            } else {
                st.toast(
                    ToastKind::Warn,
                    tf(
                        st.lang(),
                        "toast.link_missing_file",
                        &[("file", &path.display().to_string())],
                    ),
                    "",
                );
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_site_of_a_web_address() {
        assert_eq!(
            web_site("https://Example.org/a/b?c#d").as_deref(),
            Some("https://example.org")
        );
        assert_eq!(
            web_site("HTTP://example.org:8080").as_deref(),
            Some("http://example.org:8080")
        );
        assert_eq!(web_site("mailto:a@example.org"), None);
        assert_eq!(web_site("ftp://example.org/x"), None);
        assert_eq!(web_site("https://user@example.org/"), None);
        assert_eq!(web_site("https:///x"), None);
    }
}
