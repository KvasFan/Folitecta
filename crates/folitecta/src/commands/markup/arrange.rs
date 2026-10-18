//! Arranging the chosen comments of a page (the selected one and those
//! added with Shift or Ctrl): aligning, distributing, same size, the
//! order they are stacked in, and groups. The first comment chosen is the
//! one the others are aligned on or sized like.

use dioxus::prelude::ReadableExt;
use folitecta_core::Document;
use folitecta_core::arrange::{AlignTo, Axis, Edge, SameSize, SpreadBy, Stack};

use crate::commands::handlers;
use crate::files;
use crate::i18n::t;
use crate::state::{AppState, ToastKind};

/// What the chosen comments are aligned with (Align To): `selection`,
/// `page`, `group` (the page, moved together) or `first`.
fn align_to(st: AppState) -> AlignTo {
    match st.ui.peek().value("arrange.alignTo") {
        Some("page") => AlignTo::Page,
        Some("group") => AlignTo::PageAsGroup,
        Some("first") => AlignTo::First,
        _ => AlignTo::Selection,
    }
}

/// Whether comments are distributed over the page rather than between the
/// first and the last of them.
fn over_page(st: AppState) -> bool {
    st.ui.peek().value("arrange.distributeTo") == Some("page")
}

/// Run an arrangement on the chosen comments and keep them chosen.
fn arrange(
    st: AppState,
    op: impl FnOnce(&mut Document, usize, &[usize]) -> folitecta_core::Result<Vec<usize>>,
) {
    let Some((page, chosen)) = st.read_active(|t| t.chosen_annots()).flatten() else {
        st.toast(ToastKind::Info, t(st.lang(), "toast.choose_comments"), "");
        return;
    };
    match st.with_active(|t| op(&mut t.doc, page, &chosen)) {
        Some(Ok(kept)) => {
            st.with_active(|t| t.set_chosen(page, &kept));
            files::set_window_title(st);
        }
        Some(Err(e)) => st.error(e),
        None => {}
    }
}

fn align(st: AppState, edge: Edge) {
    let to = align_to(st);
    arrange(st, move |doc, page, chosen| {
        doc.align_annotations(page, chosen, edge, to)
            .map(|_| chosen.to_vec())
    });
}

fn distribute(st: AppState, axis: Axis, by: SpreadBy) {
    let on_page = over_page(st);
    arrange(st, move |doc, page, chosen| {
        doc.distribute_annotations(page, chosen, axis, by, on_page)
            .map(|_| chosen.to_vec())
    });
}

fn same(st: AppState, what: SameSize) {
    arrange(st, move |doc, page, chosen| {
        doc.size_annotations(page, chosen, what).map(|_| chosen.to_vec())
    });
}

fn stack(st: AppState, how: Stack) {
    arrange(st, move |doc, page, chosen| {
        doc.stack_annotations(page, chosen, how)
    });
}

/// Make one group of the chosen comments (and of the groups they are in):
/// they are then chosen, moved and deleted together.
fn group(st: AppState) {
    let few = st
        .read_active(|t| t.chosen_annots().is_none_or(|(_, l)| l.len() < 2))
        .unwrap_or(true);
    if few {
        st.toast(ToastKind::Info, t(st.lang(), "toast.choose_two_comments"), "");
        return;
    }
    arrange(st, |doc, page, chosen| doc.group_annotations(page, chosen));
}

/// Break up the groups of the chosen comments; they stay chosen.
fn ungroup(st: AppState) {
    arrange(st, |doc, page, chosen| {
        doc.ungroup_annotations(page, chosen).map(|_| chosen.to_vec())
    });
}

/// Show the Arrange tab of the chosen comments.
fn show_tab(st: AppState) {
    if st.read_active(|t| t.chosen_annots().is_some()).unwrap_or(false) {
        st.set_ribbon_tab(crate::ui::chrome::ribbon_layout::ARRANGE_TAB.to_string());
    } else {
        st.toast(ToastKind::Info, t(st.lang(), "toast.choose_comments"), "");
    }
}

/// Check states of Align To and Distribute To.
pub fn arrange_state(st: AppState, id: &str) -> Option<bool> {
    let align = st
        .ui
        .read()
        .value("arrange.alignTo")
        .unwrap_or("selection")
        .to_string();
    let over = over_page(st);
    match id {
        "alignto.selection" => Some(align == "selection"),
        "alignto.page" => Some(align == "page"),
        "alignto.pageAsGroup" => Some(align == "group"),
        "alignto.keyObject" => Some(align == "first"),
        "distributeTo.selection" => Some(!over),
        "distributeTo.page" => Some(over),
        _ => None,
    }
}

handlers! {
    "align.left" => |st| align(st, Edge::Left),
    "align.center" => |st| align(st, Edge::Center),
    "align.right" => |st| align(st, Edge::Right),
    "align.top" => |st| align(st, Edge::Top),
    "align.middle" => |st| align(st, Edge::Middle),
    "align.bottom" => |st| align(st, Edge::Bottom),
    "alignto.selection" => |st| st.set_value("arrange.alignTo", "selection"),
    "alignto.page" => |st| st.set_value("arrange.alignTo", "page"),
    "alignto.pageAsGroup" => |st| st.set_value("arrange.alignTo", "group"),
    "alignto.keyObject" => |st| st.set_value("arrange.alignTo", "first"),
    "distribHorz.left" => |st| distribute(st, Axis::Across, SpreadBy::Start),
    "distribHorz.middle" => |st| distribute(st, Axis::Across, SpreadBy::Middle),
    "distribHorz.right" => |st| distribute(st, Axis::Across, SpreadBy::End),
    "distribHorz.space" => |st| distribute(st, Axis::Across, SpreadBy::Space),
    "distribVert.top" => |st| distribute(st, Axis::Down, SpreadBy::Start),
    "distribVert.center" => |st| distribute(st, Axis::Down, SpreadBy::Middle),
    "distribVert.bottom" => |st| distribute(st, Axis::Down, SpreadBy::End),
    "distribVert.space" => |st| distribute(st, Axis::Down, SpreadBy::Space),
    "distributeTo.selection" => |st| st.set_value("arrange.distributeTo", "selection"),
    "distributeTo.page" => |st| st.set_value("arrange.distributeTo", "page"),
    "makeSameWidth" => |st| same(st, SameSize::Width),
    "makeSameHeight" => |st| same(st, SameSize::Height),
    "makeSameSize" => |st| same(st, SameSize::Both),
    "order.foreground" => |st| stack(st, Stack::Front),
    "order.background" => |st| stack(st, Stack::Back),
    "order.moveForward" => |st| stack(st, Stack::Forward),
    "order.moveBackward" => |st| stack(st, Stack::Backward),
    "group" => group,
    "ungroup" => ungroup,
    "arrange" => show_tab,
}
