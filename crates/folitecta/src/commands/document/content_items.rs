//! Edit Content › All: the pictures and the shapes a page draws, shown
//! together (as Filter Image Items and Filter Shapes say), chosen with
//! clicks or all at once (Select Image Items, Select Shapes, Select All
//! Content Items), edited (Edit Object) or deleted (Delete) in one undo step.

use dioxus::prelude::ReadableExt;

use crate::commands::{handlers, no_doc, select_tool};
use crate::i18n::t;
use crate::state::{AppState, ToastKind, Tool};

/// Choose a picture or a path of `page`; `add` keeps the others chosen
/// (a second click on one takes it away).
pub fn choose(st: AppState, page: usize, image: Option<usize>, path: Option<usize>, add: bool) {
    st.with_active(|t| {
        let mut sel = match t.content_sel.take() {
            Some((p, im, pa)) if add && p == page => (p, im, pa),
            _ => (page, Vec::new(), Vec::new()),
        };
        let toggle = |list: &mut Vec<usize>, i: usize| match list.iter().position(|x| *x == i) {
            Some(k) if add => {
                list.remove(k);
            }
            Some(_) => {}
            None => list.push(i),
        };
        if let Some(i) = image {
            toggle(&mut sel.1, i);
        }
        if let Some(i) = path {
            toggle(&mut sel.2, i);
        }
        t.content_sel = (!sel.1.is_empty() || !sel.2.is_empty()).then_some(sel);
    });
}

/// Choose every picture and/or every path of the current page.
fn select(st: AppState, images: bool, paths: bool) {
    if no_doc(st) {
        return;
    }
    select_tool(st, Tool::EditAll);
    let page = st.current_page();
    let (im, pa) = st
        .read_active(|t| {
            let im: Vec<usize> = if images {
                t.doc
                    .page_images(page)
                    .map(|l| l.iter().map(|x| x.index).collect())
                    .unwrap_or_default()
            } else {
                Vec::new()
            };
            let pa: Vec<usize> = if paths {
                t.doc
                    .page_paths(page)
                    .map(|l| l.iter().map(|x| x.index).collect())
                    .unwrap_or_default()
            } else {
                Vec::new()
            };
            (im, pa)
        })
        .unwrap_or_default();
    if im.is_empty() && pa.is_empty() {
        st.toast(ToastKind::Info, t(st.lang(), "toast.no_content_items"), "");
    }
    st.with_active(|t| t.content_sel = (!im.is_empty() || !pa.is_empty()).then_some((page, im, pa)));
}

/// Edit › Delete with objects chosen: they leave the page's content.
pub fn delete_chosen(st: AppState) -> bool {
    if *st.tool.peek() != Tool::EditAll {
        return false;
    }
    let Some((page, im, pa)) = st.read_active(|t| t.content_sel.clone()).flatten() else {
        return false;
    };
    if !st.allowed(folitecta_core::Restricted::Modify) {
        return true;
    }
    let r = st.with_active(|t| {
        let r = t.doc.delete_content_items(page, &im, &pa);
        t.content_sel = None;
        r
    });
    match r {
        Some(Ok(())) => crate::files::set_window_title(st),
        Some(Err(e)) => st.error(st.core_error_text(&e)),
        None => {}
    }
    true
}

/// Edit Object: the one object chosen in its own editor (the picture editor,
/// or the nodes of the path).
fn edit_object(st: AppState) {
    if no_doc(st) {
        return;
    }
    let sel = st.read_active(|t| t.content_sel.clone()).flatten();
    match sel {
        Some((page, im, pa)) if im.len() == 1 && pa.is_empty() => {
            st.with_active(|t| t.image_sel = Some((page, im[0])));
            crate::commands::pictures::edit_selected(st);
        }
        Some((page, im, pa)) if pa.len() == 1 && im.is_empty() => {
            select_tool(st, Tool::EditPaths);
            crate::commands::paths::choose(st, page, pa[0]);
        }
        _ => st.toast(ToastKind::Info, t(st.lang(), "toast.choose_one_object"), ""),
    }
}

handlers! {
    "tool.editContent.all" => |st| select_tool(st, Tool::EditAll),
    "selectContentItems" => |st| select_tool(st, Tool::EditAll),
    "selectAllContentItems" => |st| select(st, true, true),
    "selectImageItems" => |st| select(st, true, false),
    "selectShapes" => |st| select(st, false, true),
    "editObject" => edit_object,
}
