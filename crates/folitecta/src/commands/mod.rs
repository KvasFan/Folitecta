//! Command dispatch.
//!
//! Each area module exports a `HANDLERS` table mapping registry ids to functions.
//! Together they are the single source of truth for which registry commands are
//! implemented: menus, ribbon badges, the feature matrix and the roadmap notice
//! all derive from them.

use dioxus::prelude::ReadableExt;
pub(crate) mod accessibility;
pub(crate) mod attachments;
pub(crate) mod bookmarks;
pub(crate) mod calculator;
mod comments;
pub(crate) mod compare;
mod convert;
pub(crate) mod doctools;
mod edit;
pub(crate) mod explorer;
pub(crate) mod fields_pane;
mod file;
pub(crate) mod fileops;
mod format;
mod forms;
mod help;
pub(crate) mod history;
pub(crate) mod interface;
pub(crate) mod javascript;
pub(crate) mod layers;
pub(crate) mod macros;
pub(crate) mod media;
mod pages;
mod panes;
pub(crate) mod pictures;
pub(crate) mod portfolio;
mod protect;
pub(crate) mod review;
pub(crate) mod search;
pub(crate) mod sessions;
pub(crate) mod signatures;
pub(crate) mod signs;
pub(crate) mod sources;
pub(crate) mod split;
pub(crate) mod stamps;
pub(crate) mod tables;
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
    format::HANDLERS,
    protect::HANDLERS,
    review::HANDLERS,
    bookmarks::HANDLERS,
    interface::HANDLERS,
    signatures::HANDLERS,
    signs::HANDLERS,
    macros::HANDLERS,
    layers::HANDLERS,
    attachments::HANDLERS,
    fields_pane::HANDLERS,
    search::HANDLERS,
    portfolio::HANDLERS,
    history::HANDLERS,
    sessions::HANDLERS,
    stamps::HANDLERS,
    doctools::HANDLERS,
    compare::HANDLERS,
    tables::HANDLERS,
    accessibility::HANDLERS,
    calculator::HANDLERS,
    fileops::HANDLERS,
    javascript::HANDLERS,
    media::HANDLERS,
    explorer::HANDLERS,
    sources::HANDLERS,
    pictures::HANDLERS,
    split::HANDLERS,
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
    "fontName",
    "tool.editFields.mode.date",
    "sendByEmail",
    "copySrcFile",
    // From the locale environment variables only.
    "macro.region",
    // The first matching annotation is selected (the selection holds one).
    "layersView.selectCorresponding.comments",
    "layersView.selectCorresponding.fields",
    "layersView.selectCorresponding.links",
    // Opens the PDF files and prints every open document.
    "view.portfolio.print",
    // There are no tab groups: the files open in this window.
    "sessions.open.addToActiveGroup",
    "sessions.open.addToNewGroup",
    // The first stamp made with the design is selected.
    "stampCustom.selectAnnots",
    // They show the stamps pane.
    "stampCustom.menu",
    "stampCustom.recentsMenu",
    // The content outside the crop box is clipped, not deleted from the file.
    "document.removeCroppedContent",
    // The pictures of the current page (a single picture cannot be selected).
    "document.recompressImage",
    // There is no side-by-side view: it switches between the two documents.
    "cmpDocs.show.both",
    // The system decides where the browser opens a link.
    "tool.link.openInBrowser.newTab",
    "tool.link.openInBrowser.newWindow",
];

/// View options toggled by command id; their state lives in the preferences.
pub const VIEW_OPTIONS: &[&str] = &[
    "view.layout.ribbon",
    "view.panzoom",
    "view.showPageSizePopup",
    "view.spellCheck",
    "bookmarksView.autoExpandBookmarks",
    "bookmarksView.ensureVisibility",
    "bookmarksView.markInvalid",
    "bookmarksView.showContrastText",
    "bookmarksView.useCurrentPage",
    "bookmarksView.useInheritedZoom",
    "bookmarksView.useRegionForDest",
    "bookmarksView.useTextForTitle",
    "pdfNamedDestsView.ensureVisibility",
    "pdfNamedDestsView.useCurrentPage",
    "pdfNamedDestsView.useInheritedZoom",
    "pdfNamedDestsView.useRegionForDest",
    "pdfNamedDestsView.useTextForTitle",
    "pdfNamedDestsView.wrapTitles",
    "pdfNamedDestsView.hdr.details",
    "pdfNamedDestsView.hdr.name",
    "pdfNamedDestsView.hdr.page",
    "pdfNamedDestsView.hdr.type",
    "commentsView.show.subjects",
    "commentsView.show.states",
    "commentsView.show.text",
    "commentsView.show.compactItems",
    "commentsView.show.noTextPlaceholder",
    "commentsView.show.uniformText",
    "commentsView.show.dynamicIcons",
    "commentsView.showHiddenComments",
    "commentsView.showCommentsFromHiddenLayers",
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
    "attachmentsView.autoShowPane",
    "attachmentsView.hdr.name",
    "attachmentsView.hdr.description",
    "attachmentsView.hdr.modified",
    "attachmentsView.hdr.size",
    "attachmentsView.hdr.compressed",
    "attachmentsView.hdr.location",
    "attachmentsView.hdr.type",
    "attachmentsView.hdr.relationship",
    "fieldsView.showFullNames",
    "searchView.results.showCommentTypes",
    "searchView.results.showContextBookmarks",
    "searchView.results.showIcon",
    "searchView.lookInSubFolders",
    "searchView.estimateFolder",
    "document.find.coloredResults",
    "tool.loupe.trackMouse",
    "tagsview.highlightContent",
    "contentsView.displaySelected",
    "msp.hdr.tagName",
    "msp.hdr.value",
    "editImage.applyToAll",
];

/// Options that are on unless the user turned them off.
pub const DEFAULT_ON: &[&str] = &[
    "view.spellCheck",
    "search.pageText",
    "portfolio.tools",
    "tool.loupe.trackMouse",
    "tagsview.highlightContent",
    "contentsView.displaySelected",
    "msp.hdr.tagName",
    "msp.hdr.value",
    "media.enabled",
    "explorer.thumbs",
    "stamps.favorites",
    "stamps.recents",
    "historyList.group.show",
    "historyList.options.showPath",
    "historyList.options.showPinned",
    "historyList.options.showThumbnail",
    "historyList.options.showPreviewPopUp",
    "searchView.results.showCommentTypes",
    "searchView.results.showIcon",
    "searchView.lookInSubFolders",
    "document.find.coloredResults",
    "attachmentsView.hdr.name",
    "attachmentsView.hdr.description",
    "attachmentsView.hdr.modified",
    "attachmentsView.hdr.size",
    "attachmentsView.hdr.location",
    "bookmarksView.autoExpandBookmarks",
    "bookmarksView.ensureVisibility",
    "bookmarksView.markInvalid",
    "bookmarksView.showContrastText",
    "bookmarksView.useCurrentPage",
    "bookmarksView.useInheritedZoom",
    "bookmarksView.useTextForTitle",
    "pdfNamedDestsView.ensureVisibility",
    "pdfNamedDestsView.useCurrentPage",
    "pdfNamedDestsView.useInheritedZoom",
    "pdfNamedDestsView.hdr.name",
    "pdfNamedDestsView.hdr.page",
    "pdfNamedDestsView.hdr.type",
    "bookmarksView.showIcons",
    "commentsView.show.states",
    "commentsView.show.text",
    "commentsView.show.dynamicIcons",
    "commentsView.showHiddenComments",
    "commentsView.showCommentsFromHiddenLayers",
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
    VIEW_OPTIONS
        .contains(&id)
        .then(|| option_on(st, id))
        .or_else(|| review::review_state(st, id))
        .or_else(|| bookmarks::bookmark_state(st, id))
        .or_else(|| interface::interface_state(st, id))
        .or_else(|| layers::layers_state(st, id))
        .or_else(|| fields_pane::fields_state(st, id))
        .or_else(|| search::search_state(st, id))
        .or_else(|| portfolio::portfolio_state(st, id))
        .or_else(|| history::history_state(st, id))
        .or_else(|| sessions::sessions_state(st, id))
        .or_else(|| stamps::stamps_state(st, id))
        .or_else(|| compare::compare_state(st, id))
        .or_else(|| tables::tables_state(st, id))
        .or_else(|| accessibility::acc_state(st, id))
        .or_else(|| calculator::calc_state(st, id))
        .or_else(|| fileops::fileops_state(st, id))
        .or_else(|| media::media_state(st, id))
        .or_else(|| explorer::explorer_state(st, id))
        .or_else(|| pictures::pictures_state(st, id))
        .or_else(|| split::split_state(st, id))
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
/// Activate a tool (drawing tools need a document).
pub(crate) fn select_tool(st: AppState, tool: crate::state::Tool) {
    if tool.is_drawing() && no_doc(st) {
        return;
    }
    st.set_drawing(None);
    st.set_radio_group(None);
    st.set_editing_field(None);
    st.set_tool(tool);
}

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

/// Status of a pure container (a submenu without its own handler), derived from its
/// children: 2 when every child works, 1 when some do, 0 when none does.
fn container_status(
    id: &'static str,
    done: &HashSet<&'static str>,
    partial: &HashSet<&'static str>,
    memo: &mut HashMap<&'static str, u8>,
    depth: usize,
) -> u8 {
    if done.contains(id) {
        return 2;
    }
    if partial.contains(id) {
        return 1;
    }
    if let Some(s) = memo.get(id) {
        return *s;
    }
    let reg = registry();
    let children = reg.children(id);
    if children.is_empty() || depth > 12 {
        return 0;
    }
    let statuses: Vec<u8> = children
        .iter()
        .map(|c| container_status(c.id.as_str(), done, partial, memo, depth + 1))
        .collect();
    let s = if statuses.iter().all(|s| *s == 2) {
        2
    } else if statuses.iter().any(|s| *s > 0) {
        1
    } else {
        0
    };
    memo.insert(id, s);
    s
}

/// Implemented and partial command ids, with containers classified by their children.
fn status_sets() -> (HashSet<&'static str>, HashSet<&'static str>) {
    let mut done: HashSet<&'static str> = all_ids()
        .filter(|id| !PARTIAL.contains(id))
        .chain(VIEW_OPTIONS.iter().copied())
        .collect();
    let mut partial: HashSet<&'static str> = all_ids().filter(|id| PARTIAL.contains(id)).collect();
    let mut memo = HashMap::new();
    let reg = registry();
    let containers: Vec<&'static str> = reg
        .all()
        .iter()
        .map(|c| c.id.as_str())
        .filter(|id| !done.contains(id) && !partial.contains(id) && reg.has_children(id))
        .collect();
    let derived: Vec<(&'static str, u8)> = containers
        .into_iter()
        .map(|id| (id, container_status(id, &done, &partial, &mut memo, 0)))
        .collect();
    for (id, s) in derived {
        match s {
            2 => {
                done.insert(id);
            }
            1 => {
                partial.insert(id);
            }
            _ => {}
        }
    }
    (done, partial)
}

/// Set of implemented command ids (a submenu counts when all of its commands work).
pub fn implemented_ids() -> HashSet<&'static str> {
    status_sets().0
}

/// Set of partially implemented command ids (a submenu counts when some of its
/// commands work).
pub fn partial_ids() -> HashSet<&'static str> {
    status_sets().1
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
