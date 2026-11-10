//! The actions of the chosen link (Document › Actions): listed in the
//! properties pane, a row chosen with a click; Add and Edit open a window
//! for one action (a page, a web address, a script, a page change of the
//! reader, the page of a bookmark), Up, Down and Remove act on the row
//! chosen. Every change is one undo step.

use dioxus::prelude::*;
use folitecta_core::{ButtonAction, LinkTarget};

use crate::commands::{handlers, no_doc};
use crate::i18n::t;
use crate::state::{AppState, Dialog, ToastKind};

/// The action chosen in the list: link (page, index) and its place.
pub static ACTION_SEL: GlobalSignal<Option<(usize, usize, usize)>> = Signal::global(|| None);

/// The link chosen (page, index), when the comment chosen is a link.
pub fn chosen_link(st: AppState) -> Option<(usize, usize)> {
    st.read_active(|t| {
        let (p, i) = t.selected_annot?;
        let a = t.doc.annotations(p).ok()?.into_iter().find(|a| a.index == i)?;
        (a.subtype == "Link").then_some((p, i))
    })
    .flatten()
}

/// The place of the action chosen in the chosen link's list.
fn chosen_action(st: AppState) -> Option<(usize, usize, usize)> {
    let (p, i) = chosen_link(st)?;
    ACTION_SEL.peek().filter(|(q, j, _)| *q == p && *j == i)
}

/// What an action does, for the list.
pub fn describe(lang: crate::i18n::Lang, a: &ButtonAction) -> String {
    match a {
        ButtonAction::Script(code) => {
            let line = code.lines().next().unwrap_or_default();
            format!(
                "{}: {}",
                t(lang, "link_actions.script"),
                line.chars().take(60).collect::<String>()
            )
        }
        ButtonAction::Target(target) => target.describe(),
    }
}

/// Say how the change went; the link stays chosen (a change of the
/// document lets go of the comment chosen when the tab refreshes).
fn report(st: AppState, link: (usize, usize), r: Option<folitecta_core::Result<()>>) {
    st.with_active(|t| t.selected_annot = Some(link));
    match r {
        Some(Ok(())) => crate::files::set_window_title(st),
        Some(Err(e)) => st.error(st.core_error_text(&e)),
        None => {}
    }
}

fn need_link(st: AppState) -> Option<(usize, usize)> {
    if no_doc(st) {
        return None;
    }
    let link = chosen_link(st);
    if link.is_none() {
        st.toast(ToastKind::Info, t(st.lang(), "link_actions.choose_link"), "");
    }
    link
}

fn need_action(st: AppState) -> Option<(usize, usize, usize)> {
    need_link(st)?;
    let at = chosen_action(st);
    if at.is_none() {
        st.toast(ToastKind::Info, t(st.lang(), "link_actions.choose_action"), "");
    }
    at
}

fn add(st: AppState, kind: &str) {
    if need_link(st).is_some() {
        st.set_dialog(Dialog::LinkAction {
            edit: None,
            kind: kind.to_string(),
        });
    }
}

fn edit(st: AppState) {
    let Some((p, i, at)) = need_action(st) else { return };
    let current = st
        .read_active(|t| t.doc.link_actions(p, i).get(at).cloned())
        .flatten();
    let kind = match current {
        Some(ButtonAction::Script(_)) => "script",
        Some(ButtonAction::Target(LinkTarget::Uri(_))) => "web",
        Some(ButtonAction::Target(LinkTarget::NamedAction(_))) => "reader",
        _ => "page",
    };
    st.set_dialog(Dialog::LinkAction {
        edit: Some(at),
        kind: kind.to_string(),
    });
}

fn shift(st: AppState, up: bool) {
    let Some((p, i, at)) = need_action(st) else { return };
    let len = st.read_active(|t| t.doc.link_actions(p, i).len()).unwrap_or(0);
    report(
        st,
        (p, i),
        st.with_active(|t| t.doc.move_link_action(p, i, at, up)),
    );
    let to = if up {
        at.saturating_sub(1)
    } else {
        (at + 1).min(len.saturating_sub(1))
    };
    *ACTION_SEL.write() = Some((p, i, to));
}

fn remove(st: AppState) {
    let Some((p, i, at)) = need_action(st) else { return };
    report(st, (p, i), st.with_active(|t| t.doc.remove_link_action(p, i, at)));
    *ACTION_SEL.write() = None;
}

/// Add `action` to the chosen link, or put it in place of action `edit`.
pub fn store(st: AppState, edit: Option<usize>, action: ButtonAction) -> bool {
    let Some((p, i)) = chosen_link(st) else {
        return false;
    };
    let r = st.with_active(|t| match edit {
        Some(at) => t.doc.replace_link_action(p, i, at, &action),
        None => t.doc.add_link_action(p, i, &action),
    });
    let ok = matches!(r, Some(Ok(())));
    report(st, (p, i), r);
    ok
}

handlers! {
    "actions.add" => |st| add(st, "page"),
    "actions.addFromBookmark" => |st| add(st, "bookmark"),
    "actions.edit" => edit,
    "actions.up" => |st| shift(st, true),
    "actions.down" => |st| shift(st, false),
    "actions.remove" => remove,
}
