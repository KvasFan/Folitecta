//! Tab groups and windows (FOL-L002): the tabs in two groups side by side
//! or one above the other, each with its own tab strip and its document
//! shown; a tab moved to the other group, the tabs shared out or gathered
//! again; a document moved to a new window (another Folitecta, opening its
//! file), a new empty window.
//!
//! The groups are drawn with the split window: the active tab (the group
//! worked in) keeps the other group's document as the other view of its
//! split (`SplitView::source`); when the other group is clicked, its tab
//! becomes the active one and the views change places.

use std::path::PathBuf;
use std::sync::Mutex;

use dioxus::prelude::{ReadableExt, WritableExt};

use crate::commands::{handlers, no_doc};
use crate::i18n::t;
use crate::state::{AfterUnsaved, AppState, SplitView, TabGroups, ToastKind};

/// Bring the views and the groups in line with the tabs: tabs closed leave
/// their group, new ones join the group worked in, a group left empty ends
/// the groups; the active tab shows the other group's document beside it.
pub fn sync(st: AppState) {
    let Some(mut g) = st.groups.peek().clone() else {
        return;
    };
    let ids: Vec<u64> = st.docs.peek().iter().map(|t| t.id).collect();
    for tabs in g.tabs.iter_mut() {
        tabs.retain(|id| ids.contains(id));
    }
    for id in &ids {
        if g.group_of(*id).is_none() {
            g.tabs[g.focus.min(1)].push(*id);
        }
    }
    if g.tabs.iter().any(Vec::is_empty) {
        end(st);
        return;
    }
    let active = ids[(*st.active.peek()).min(ids.len() - 1)];
    let mine = g.group_of(active).unwrap_or(0);
    let other_group = 1 - mine;
    g.focus = mine;
    g.shown[mine] = active;
    if !g.tabs[other_group].contains(&g.shown[other_group]) {
        g.shown[other_group] = g.tabs[other_group][0];
    }
    let other = g.shown[other_group];
    let size = *st.canvas_size.peek();
    {
        let mut docs = st.docs_mut();
        // The views of documents shown beside another tab go back to them.
        let mut back = Vec::new();
        for t in docs.iter_mut() {
            let keep = t.id == active && t.split.as_ref().is_some_and(|s| s.source == Some(other));
            if !keep && t.split.as_ref().is_some_and(|s| s.source.is_some()) {
                if let Some(s) = t.split.take() {
                    back.extend(s.source.map(|id| (id, s.other)));
                }
            }
        }
        for (id, view) in back {
            if let Some(t) = docs.iter_mut().find(|t| t.id == id) {
                t.set_view_state(view);
            }
        }
        let other_view = docs.iter().find(|t| t.id == other).map(|t| t.view_state());
        if let (Some(view), Some(t)) = (other_view, docs.iter_mut().find(|t| t.id == active)) {
            match t.split.as_mut().filter(|s| s.source == Some(other)) {
                Some(s) => {
                    s.side_by_side = g.side_by_side;
                    s.focus = mine as u8;
                }
                None => {
                    t.split = Some(SplitView {
                        side_by_side: g.side_by_side,
                        focus: mine as u8,
                        other: view,
                        other_size: size,
                        source: Some(other),
                    })
                }
            }
        }
    }
    if st.groups.peek().as_ref() != Some(&g) {
        *st.groups.write_unchecked() = Some(g);
    }
}

/// One group again: the document worked in stays, the other one's view goes
/// back to it.
pub fn end(st: AppState) {
    if st.groups.peek().is_none() {
        return;
    }
    *st.groups.write_unchecked() = None;
    let mut docs = st.docs_mut();
    let mut back = Vec::new();
    for t in docs.iter_mut() {
        if t.split.as_ref().is_some_and(|s| s.source.is_some()) {
            if let Some(s) = t.split.take() {
                back.extend(s.source.map(|id| (id, s.other)));
            }
        }
    }
    for (id, view) in back {
        if let Some(t) = docs.iter_mut().find(|t| t.id == id) {
            t.set_view_state(view);
        }
    }
}

/// The other group's view was clicked: its document becomes the active one.
/// False when there are no groups (the split window handles it).
pub fn focus_group(st: AppState, pane: usize) -> bool {
    let Some(g) = st.groups.peek().clone() else {
        return false;
    };
    if pane != g.focus {
        let id = g.shown[pane.min(1)];
        let i = st.docs.peek().iter().position(|t| t.id == id);
        if let Some(i) = i {
            st.set_active(i);
            crate::files::set_window_title(st);
            crate::commands::split_view::show_pages(st);
        }
    }
    true
}

/// The ids of the tabs and the active one.
fn tabs(st: AppState) -> (Vec<u64>, u64) {
    let ids: Vec<u64> = st.docs.peek().iter().map(|t| t.id).collect();
    let active = ids
        .get((*st.active.peek()).min(ids.len().saturating_sub(1)))
        .copied()
        .unwrap_or(0);
    (ids, active)
}

fn need_two(st: AppState) -> bool {
    if st.docs.peek().len() < 2 {
        st.toast(ToastKind::Warn, t(st.lang(), "toast.groups_need_two"), "");
        return false;
    }
    true
}

fn set(st: AppState, g: TabGroups) {
    *st.groups.write_unchecked() = Some(g);
    sync(st);
    crate::commands::split_view::show_pages(st);
}

/// The active tab to a new group beside (or below) the others; with groups
/// already, to the other group (a group left empty ends them).
fn to_other_group(st: AppState, side_by_side: bool) {
    if no_doc(st) || !need_two(st) {
        return;
    }
    let (ids, active) = tabs(st);
    let mut g = st.groups.peek().clone().unwrap_or_else(|| TabGroups {
        side_by_side,
        tabs: [ids.clone(), Vec::new()],
        shown: [active, active],
        focus: 0,
    });
    let from = g.group_of(active).unwrap_or(0);
    let to = 1 - from;
    g.tabs[from].retain(|id| *id != active);
    if g.tabs[from].is_empty() {
        end(st);
        crate::commands::split_view::show_pages(st);
        return;
    }
    g.tabs[to].push(active);
    g.side_by_side = side_by_side;
    g.focus = to;
    g.shown[to] = active;
    if !g.tabs[from].contains(&g.shown[from]) {
        // The tab before the one that went, else the first.
        let pos = ids.iter().position(|id| *id == active).unwrap_or(0);
        g.shown[from] = ids[..pos]
            .iter()
            .rev()
            .find(|id| g.tabs[from].contains(id))
            .copied()
            .unwrap_or(g.tabs[from][0]);
    }
    set(st, g);
}

/// Move the active tab to the other group, the groups placed as they are.
fn move_tab(st: AppState) {
    let side = st.groups.peek().as_ref().is_none_or(|g| g.side_by_side);
    to_other_group(st, side);
}

/// The tabs shared out between two groups, the first half and the second.
fn distribute(st: AppState, side_by_side: bool) {
    if no_doc(st) || !need_two(st) {
        return;
    }
    let (ids, active) = tabs(st);
    let half = ids.len().div_ceil(2);
    let groups = [ids[..half].to_vec(), ids[half..].to_vec()];
    let mine = usize::from(!groups[0].contains(&active));
    let mut shown = [groups[0][0], groups[1][0]];
    shown[mine] = active;
    set(
        st,
        TabGroups {
            side_by_side,
            tabs: groups,
            shown,
            focus: mine,
        },
    );
}

fn merge(st: AppState) {
    end(st);
    crate::commands::split_view::show_pages(st);
}

/// Windows the self-test starts are noted, not started.
static NOTED: Mutex<Option<Vec<Vec<PathBuf>>>> = Mutex::new(None);

/// Note windows instead of starting them (the self-test), or start them again.
pub fn set_noting(on: bool) {
    *NOTED.lock().unwrap_or_else(|e| e.into_inner()) = on.then(Vec::new);
}

/// The windows noted: the files each would open.
pub fn noted() -> Vec<Vec<PathBuf>> {
    NOTED
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
        .unwrap_or_default()
}

/// Start another Folitecta window opening `files`.
pub fn start_window(files: &[PathBuf]) -> Result<(), String> {
    if let Some(noted) = NOTED.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
        noted.push(files.to_vec());
        return Ok(());
    }
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    std::process::Command::new(exe)
        .args(files)
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// The active document to a new window: its file opens there and its tab
/// closes here. A document with unsaved changes (or never saved) is saved
/// first.
fn to_new_window(st: AppState) {
    if no_doc(st) {
        return;
    }
    let Some((id, path, dirty)) =
        st.read_active(|t| (t.id, t.doc.path().map(|p| p.to_path_buf()), t.doc.is_dirty()))
    else {
        return;
    };
    let Some(path) = path.filter(|_| !dirty) else {
        st.toast(ToastKind::Warn, t(st.lang(), "toast.save_before_window"), "");
        return;
    };
    match start_window(&[path]) {
        Ok(()) => crate::files::close_tab(st, id, true, AfterUnsaved::Close),
        Err(e) => st.error(e),
    }
}

/// A new empty window.
pub fn new_window(st: AppState) {
    if let Err(e) = start_window(&[]) {
        st.error(e);
    }
}

handlers! {
    "window.activeDocToNewVertTabGroup" => |st| to_other_group(st, true),
    "window.activeDocToNewHorzTabGroup" => |st| to_other_group(st, false),
    "window.moveTabTo" => move_tab,
    "window.distributeDocsVert" => |st| distribute(st, true),
    "window.distributeDocsHorz" => |st| distribute(st, false),
    "window.allDocsToOneTabGroup" => merge,
    "window.activeDocToNewWindow" => to_new_window,
    "window.newBlank" => new_window,
}
