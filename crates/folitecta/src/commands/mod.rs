//! Command dispatch.
//!
//! Each area module exports a `HANDLERS` table mapping registry ids to functions.
//! Together they are the single source of truth for which registry commands are
//! implemented: menus, ribbon badges, the feature matrix and the roadmap notice
//! all derive from them.

use dioxus::prelude::ReadableExt;
mod comments;
mod convert;
mod edit;
mod file;
mod forms;
mod help;
mod pages;
mod panes;
mod protect;
mod view;

use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

use folitecta_core::registry;

use crate::files;
use crate::i18n::{t, tf};
use crate::state::{AppState, ConfirmAction, Dialog, Pane, ToastKind, Zoom};
use crate::util;

pub(crate) use comments::markup_selection;

pub type Handler = fn(AppState);

/// Declare a module's handler table.
macro_rules! handlers {
    ($( $id:literal => $f:expr ),* $(,)?) => {
        pub const HANDLERS: &[(&str, $crate::commands::Handler)] = &[ $( ($id, $f) ),* ];
    };
}
pub(crate) use handlers;

const TABLES: &[&[(&str, Handler)]] = &[
    file::HANDLERS,
    edit::HANDLERS,
    view::HANDLERS,
    pages::HANDLERS,
    comments::HANDLERS,
    forms::HANDLERS,
    protect::HANDLERS,
    convert::HANDLERS,
    panes::HANDLERS,
    help::HANDLERS,
];

static INDEX: LazyLock<HashMap<&'static str, Handler>> = LazyLock::new(|| {
    let mut map = HashMap::new();
    for table in TABLES {
        for (id, f) in table.iter() {
            map.insert(*id, *f);
        }
    }
    map
});

/// Commands that perform a reduced version of the full operation.
pub const PARTIAL: &[&str] = &[
    "comments.summarize",
    "comments.export",
    "commentsView.exportSelected",
    "forms.exportData",
    "sendByEmail",
    "copySrcFile",
];

/// View options toggled by command id; their state lives in the preferences.
pub const VIEW_OPTIONS: &[&str] = &[
    "view.darkPageMode",
    "view.overrideDocumentColors",
    "view.showGrid",
    "view.showRulers",
    "view.showGuides",
    "view.highlightLinks",
    "view.showTranspGrid",
    "view.docInfoBar",
    "view.showPageMeasureInfo",
    "view.pageMeasureInfo.WH",
    "view.pageMeasureInfo.XY",
    "view.pageMeasureInfo.units",
    "view.mathY",
    "view.snap",
    "view.snap.toGrid",
    "view.snap.toGuides",
    "view.layout.r2l",
    "view.layout.showCover",
    "view.layout.showGaps",
    "tool.keepSelected",
    "bookmarksView.showIcons",
    "bookmarksView.wrapTitles",
    "commentsView.show.authors",
    "commentsView.show.dates",
    "commentsView.show.colors",
    "commentsView.show.wrapLongTitles",
    "searchView.results.showPageNum",
    "searchView.results.wrapText",
    "thumbnailsView.logicalVisible",
];

/// Options that are on unless the user turned them off.
pub const DEFAULT_ON: &[&str] = &[
    "bookmarksView.showIcons",
    "commentsView.show.authors",
    "commentsView.show.dates",
    "commentsView.show.colors",
    "searchView.results.showPageNum",
    "view.pageMeasureInfo.WH",
    "view.pageMeasureInfo.XY",
    "view.pageMeasureInfo.units",
    "view.layout.showGaps",
];

/// Current value of a view option.
pub fn option_on(st: AppState, id: &str) -> bool {
    let ui = st.ui.read();
    match ui.options.get(id) {
        Some(v) => *v,
        None => DEFAULT_ON.contains(&id),
    }
}

/// `Some(state)` when `id` is a view option, for toggle buttons and menu checks.
pub fn toggle_state(st: AppState, id: &str) -> Option<bool> {
    VIEW_OPTIONS.contains(&id).then(|| option_on(st, id))
}

fn toggle_view_option(st: AppState, id: &str) {
    let next = !option_on(st, id);
    st.set_option(id, next);
}

// ---- Shared helpers for the area modules -----------------------------------

pub(crate) fn no_doc(st: AppState) -> bool {
    if st.has_doc() {
        false
    } else {
        st.toast(ToastKind::Warn, t(st.lang(), "toast.no_document"), "");
        true
    }
}

pub(crate) fn core_result(st: AppState, r: Option<folitecta_core::Result<()>>) {
    match r {
        Some(Err(e)) => st.error(e),
        _ => files::set_window_title(st),
    }
}

/// Run an operation on the active document and report the outcome.
pub(crate) fn doc_op(st: AppState, f: impl FnOnce(&mut crate::state::DocTab) -> folitecta_core::Result<()>) {
    if no_doc(st) {
        return;
    }
    let r = st.with_active(f);
    core_result(st, r);
}

/// Open a dialog that needs a document.
pub(crate) fn doc_dialog(st: AppState, d: Dialog) {
    if !no_doc(st) {
        st.set_dialog(d);
    }
}

pub(crate) fn set_zoom(st: AppState, zoom: Zoom) {
    st.with_active(|t| t.zoom = zoom);
}

pub(crate) fn toggle_pane(st: AppState, pane: Pane) {
    let id = pane.id();
    let mut ui = st.ui_mut();
    if ui.show_left_pane && ui.left_pane == id {
        ui.show_left_pane = false;
    } else {
        ui.show_left_pane = true;
        ui.left_pane = id;
    }
    drop(ui);
    st.save_prefs();
}

pub(crate) fn show_pane(st: AppState, pane: Pane) {
    let mut ui = st.ui_mut();
    ui.show_left_pane = true;
    ui.left_pane = pane.id();
    drop(ui);
    st.save_prefs();
}

pub(crate) fn open_url(st: AppState, url: &str) {
    if let Err(e) = util::open_external(url) {
        st.error(e);
    }
}

pub(crate) fn confirm(st: AppState, title_key: &str, body_key: &str, action: ConfirmAction) {
    let lang = st.lang();
    st.set_dialog(Dialog::Confirm {
        title: t(lang, title_key),
        body: t(lang, body_key),
        action,
    });
}

/// Effective zoom factor of the active document (fit modes resolved).
pub fn current_zoom(st: AppState) -> f32 {
    let (cw, ch) = (st.canvas_size)();
    // Rulers take extra room on the left and at the top of the pages.
    let rulers = option_on(st, "view.showRulers");
    let (mx, my) = if rulers { (80.0, 88.0) } else { (56.0, 70.0) };
    st.read_active(|t| {
        let two = t.layout == crate::state::Layout::TwoPages;
        match t.zoom {
            Zoom::Fixed(z) => z,
            Zoom::FitWidth => {
                let widest = (0..t.doc.page_count())
                    .map(|p| t.display_size(p).0)
                    .fold(1.0, f32::max);
                // Two pages side by side in the two-page layout.
                let w = if two { widest * 2.0 + 18.0 } else { widest };
                ((cw as f32 - mx) / w).clamp(0.1, 8.0)
            }
            Zoom::FitPage => {
                let (w, h) = t.display_size(t.page);
                let w = if two { w * 2.0 + 18.0 } else { w };
                ((cw as f32 - mx) / w).min((ch as f32 - my) / h).clamp(0.1, 8.0)
            }
        }
    })
    .unwrap_or(1.0)
}

// ---- Public API ------------------------------------------------------------

fn all_ids() -> impl Iterator<Item = &'static str> {
    TABLES.iter().flat_map(|t| t.iter().map(|(id, _)| *id))
}

/// Set of implemented command ids.
pub fn implemented_ids() -> HashSet<&'static str> {
    all_ids()
        .filter(|id| !PARTIAL.contains(id))
        .chain(VIEW_OPTIONS.iter().copied())
        .collect()
}

/// Set of partially implemented command ids.
pub fn partial_ids() -> HashSet<&'static str> {
    all_ids().filter(|id| PARTIAL.contains(id)).collect()
}

pub fn is_implemented(id: &str) -> bool {
    INDEX.contains_key(id) || VIEW_OPTIONS.contains(&id)
}

pub fn is_partial(id: &str) -> bool {
    PARTIAL.contains(&id)
}

/// Run a command by registry id.
pub fn dispatch(st: AppState, id: &str) {
    st.close_menus();
    if let Some(handler) = INDEX.get(id) {
        handler(st);
        return;
    }
    if VIEW_OPTIONS.contains(&id) {
        toggle_view_option(st, id);
        return;
    }
    let reg = registry();
    match reg.get(id) {
        Some(cmd) if reg.has_children(id) => {
            // A pure container: open it in the palette so its children are reachable.
            st.set_palette(Some(cmd.label.trim_end_matches('…').to_string()));
        }
        Some(cmd) => {
            let label = crate::i18n::cmd_label(st.lang(), cmd);
            st.toast(
                ToastKind::Warn,
                tf(st.lang(), "toast.planned", &[("label", &label)]),
                "",
            );
            st.set_dialog(Dialog::NotImplemented(id.to_string()));
        }
        None => st.error(format!("unknown command {id}")),
    }
}

/// Resolve a keyboard shortcut to a command id.
pub fn shortcut_command(key: &str, ctrl: bool, shift: bool, alt: bool) -> Option<&'static str> {
    let mut parts = Vec::new();
    if ctrl {
        parts.push("Ctrl");
    }
    if shift {
        parts.push("Shift");
    }
    if alt {
        parts.push("Alt");
    }
    let key_name = match key {
        "+" | "=" => "+",
        "-" | "_" => "-",
        other => other,
    };
    parts.push(key_name);
    let combo = parts.join("+");
    registry().by_shortcut(&combo).map(|c| c.id.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handlers_reference_registry_commands() {
        let reg = registry();
        let mut seen = HashSet::new();
        for id in all_ids() {
            assert!(reg.get(id).is_some(), "handler for unknown command {id}");
            assert!(seen.insert(id), "duplicate handler {id}");
        }
        for id in VIEW_OPTIONS {
            assert!(
                reg.get(id).is_some(),
                "view option {id} is not a registry command"
            );
            assert!(!INDEX.contains_key(id), "view option {id} also has a handler");
        }
        for id in PARTIAL {
            assert!(INDEX.contains_key(id), "partial command {id} has no handler");
        }
        assert!(
            implemented_ids().len() >= 250,
            "implemented: {}",
            implemented_ids().len()
        );
    }

    #[test]
    fn shortcuts_resolve() {
        assert_eq!(shortcut_command("o", true, false, false), Some("open"));
        assert_eq!(shortcut_command("s", true, true, false), Some("saveAs"));
        assert_eq!(
            shortcut_command("Page Down", false, false, false),
            Some("view.goto.nextPage")
        );
        assert_eq!(shortcut_command("q", true, false, false), None);
    }
}
