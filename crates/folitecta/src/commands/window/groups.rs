//! The commands named after a group of commands (Other Tools, Clipboard,
//! Panes, the tool sets of Tools › Tools…): each opens a small window that
//! lists the commands of its group, the ones that work today, to run one.
//! File and Form open their menus.

use dioxus::prelude::ReadableExt;

use crate::commands::{handlers, is_implemented, no_doc};
use crate::state::{AppState, Dialog};
use crate::ui::chrome::ribbon_layout::RIBBON;

/// The commands of a curated ribbon group (tab, group), in their order.
fn ribbon(tab: &str, group: &str) -> Vec<&'static str> {
    RIBBON
        .iter()
        .filter(|t| t.id == tab)
        .flat_map(|t| t.groups.iter())
        .filter(|g| g.id == group)
        .flat_map(|g| g.items.iter().copied())
        .collect()
}

/// The pane toggles of the View tab beyond the curated panes group.
fn other_panes() -> Vec<&'static str> {
    let main = ribbon("view", "panes");
    folitecta_core::registry()
        .ribbon_group("view", "window")
        .into_iter()
        .map(|c| c.id.as_str())
        .filter(|id| id.starts_with("view.") && !id.ends_with("Group") && *id != "view.otherPanes")
        .filter(|id| !main.contains(id))
        .collect()
}

/// The commands a group command lists, in order, duplicates removed.
pub fn group_commands(id: &str) -> Vec<&'static str> {
    let mut v: Vec<&'static str> = match id {
        "clipboardGroup" => {
            let mut v = ribbon("home", "edit");
            v.extend([
                "pasteSpecial",
                "pasteAsStamp",
                "document.insertPagesFromClipboard",
                "newDocClipboard",
            ]);
            v
        }
        "selectionGroup" => vec![
            "selectAll",
            "edit.deselect",
            "invertSelection",
            "tool.select",
            "tool.selectText",
            "tool.editAnnots",
            "tool.editContent",
            "selectPages",
            "thumbnailsView.select.with",
        ],
        "view.snapGroup" => vec![
            "view.snap",
            "view.snap.toGrid",
            "view.snap.toGuides",
            "view.snap.toObjects",
        ],
        "webLinksGroup" => vec![
            "createWebLinksFromText",
            "createWebLinksFromBarcodes",
            "removeAllWebLinks",
        ],
        "shareGroup" => {
            let mut v = ribbon("share", "sendTo");
            v.extend(["sendByEmailOpenAllZip", "emailMethods"]);
            v.extend(ribbon("share", "places"));
            v
        }
        "view.panesGroup" => {
            let mut v = ribbon("view", "panes");
            v.extend(other_panes());
            v
        }
        "view.otherPanes" => other_panes(),
        "view.otherToolsGroup" | "otherToolsGroup" | "comment.otherToolsGroup" => vec![
            "tool.snapshot",
            "tool.zoom",
            "tool.zoomDyn",
            "tool.loupe",
            "tool.panZoom",
            "tool.editContent",
            "tool.editText",
            "tool.addText",
            "tool.table",
            "tool.selBarcodePlace",
            "tool.selImagePlace",
            "tool.link",
            "tool.article",
            "tool.cropPage",
            "tool.calibrate",
            "tool.editFields",
        ],
        "view.pageLayoutTypesGroup" => vec![
            "view.layout.onePage",
            "view.layout.oneColumn",
            "view.layout.twoPages",
            "view.layout.twoColumns",
            "view.layout.showCover",
            "view.layout.showGaps",
            "view.layout.r2l",
        ],
        "view.rotateViewGroup" => vec!["view.rotate.CW", "view.rotate.CCW", "view.rotate.reset"],
        "view.docTabsGroup" => vec![
            "window.new",
            "window.newBlank",
            "window.activeDocToNewWindow",
            "window.activeDocToNewVertTabGroup",
            "window.activeDocToNewHorzTabGroup",
            "window.allDocsToOneTabGroup",
            "window.allDocsToActiveWindow",
            "window.distributeTabs",
            "window.moveTabsTo",
            "window.sortAllDocTabsAbc",
        ],
        "view.show" => vec![
            "view.showRulers",
            "view.showGrid",
            "view.showGuides",
            "view.showTranspGrid",
            "view.highlightLinks",
            "view.showPageMeasureInfo",
            "view.docInfoBar",
        ],
        "tools.basic" => ribbon("home", "tools"),
        "tools.markup" => {
            let mut v = ribbon("comment", "note");
            v.extend(ribbon("comment", "text"));
            v.extend(ribbon("comment", "textMarkup"));
            v.extend(ribbon("comment", "drawing"));
            v
        }
        "tools.editContent" => {
            let mut v = ribbon("home", "objects");
            v.extend(ribbon("home", "text"));
            v
        }
        "tools.editLinks" => {
            let mut v = vec!["tool.link"];
            v.extend(ribbon("home", "links"));
            v
        }
        "tools.measure" => ribbon("comment", "measure"),
        "comments.newGroup" => {
            let mut v = ribbon("comment", "note");
            v.extend(ribbon("comment", "text"));
            v.extend(ribbon("comment", "textMarkup"));
            v.extend(ribbon("comment", "drawing"));
            v
        }
        "comments.deleteGroup" => vec![
            "comments.deleteSel",
            "comments.deleteAllShown",
            "comments.deleteAllDoc",
        ],
        "document.pagesMore" => ribbon("organize", "pages"),
        _ => Vec::new(),
    };
    // Commands that run by themselves (not submenus), each once.
    let mut seen = std::collections::HashSet::new();
    v.retain(|c| *c != id && is_implemented(c) && seen.insert(*c));
    v
}

/// Open the window of a group's commands.
pub fn open_group(st: AppState, id: &str) {
    st.set_dialog(Dialog::CommandGroup(id.to_string()));
}

handlers! {
    "clipboardGroup" => |st| open_group(st, "clipboardGroup"),
    "selectionGroup" => |st| open_group(st, "selectionGroup"),
    "view.snapGroup" => |st| open_group(st, "view.snapGroup"),
    "webLinksGroup" => |st| { if !no_doc(st) { open_group(st, "webLinksGroup") } },
    "shareGroup" => |st| open_group(st, "shareGroup"),
    "view.panesGroup" => |st| open_group(st, "view.panesGroup"),
    "view.otherPanes" => |st| open_group(st, "view.otherPanes"),
    "view.otherToolsGroup" => |st| open_group(st, "view.otherToolsGroup"),
    "otherToolsGroup" => |st| open_group(st, "otherToolsGroup"),
    "comment.otherToolsGroup" => |st| open_group(st, "comment.otherToolsGroup"),
    "view.pageLayoutTypesGroup" => |st| open_group(st, "view.pageLayoutTypesGroup"),
    "view.rotateViewGroup" => |st| open_group(st, "view.rotateViewGroup"),
    "view.docTabsGroup" => |st| open_group(st, "view.docTabsGroup"),
    "view.show" => |st| open_group(st, "view.show"),
    "tools.basic" => |st| open_group(st, "tools.basic"),
    "tools.markup" => |st| open_group(st, "tools.markup"),
    "tools.editContent" => |st| open_group(st, "tools.editContent"),
    "tools.editLinks" => |st| open_group(st, "tools.editLinks"),
    "tools.measure" => |st| open_group(st, "tools.measure"),
    "comments.newGroup" => |st| open_group(st, "comments.newGroup"),
    "comments.deleteGroup" => |st| { if !no_doc(st) { open_group(st, "comments.deleteGroup") } },
    "document.pagesMore" => |st| open_group(st, "document.pagesMore"),
    // The properties pane, also when nothing is chosen.
    "noProps" => |st| { if !st.ui.read().show_right_pane { st.ui_mut().show_right_pane = true; st.save_prefs(); } },
    "file" => |st| { st.set_open_dropdown(None); st.set_open_menu(Some("file".into())) },
    "form" => |st| { st.set_open_dropdown(None); st.set_open_menu(Some("form".into())) },
}
