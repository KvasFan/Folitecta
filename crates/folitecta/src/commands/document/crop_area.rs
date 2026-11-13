//! Crop › Apply and Crop Page › Apply: the area to keep is drawn on a page
//! (with the snapshot tool, asked for by the first Apply), shown with the
//! rest dimmed, and the second Apply crops to it: the page it was drawn on,
//! or every page (the pages chosen in the thumbnails when there are).
//! Show / Hide Origin Anchor mark where the page's coordinates start.

use crate::commands::{handlers, no_doc, select_tool};
use crate::i18n::t;
use crate::state::{AppState, ToastKind, Tool};

fn apply(st: AppState, all: bool) {
    if no_doc(st) {
        return;
    }
    let Some((page, r)) = st.read_active(|t| t.crop_area).flatten() else {
        // Draw the area first.
        st.set_value("snapshot.target", "crop");
        select_tool(st, Tool::Snapshot);
        st.toast(ToastKind::Info, t(st.lang(), "toast.crop_draw"), "");
        return;
    };
    let pages: Vec<usize> = if all {
        let chosen = st.read_active(|t| t.selected_pages.clone()).unwrap_or_default();
        if chosen.is_empty() {
            (0..st.page_count()).collect()
        } else {
            chosen
        }
    } else {
        vec![page]
    };
    let rect = st
        .read_active(|t| {
            t.page_sizes
                .get(page)
                .map(|s| folitecta_core::textops::display_rect_to_pdf(s, r))
        })
        .flatten();
    let Some(rect) = rect else { return };
    let r = st.with_active(|t| {
        let r = t.doc.crop_pages(&pages, rect);
        t.crop_area = None;
        r
    });
    match r {
        Some(Ok(())) => crate::files::set_window_title(st),
        Some(Err(e)) => st.error(st.core_error_text(&e)),
        None => {}
    }
}

handlers! {
    "cropPage.apply" => |st| apply(st, false),
    "crop.apply" => |st| apply(st, true),
    "cropPage.showOriginAnchor" => |st| st.set_option("view.originAnchor", true),
    "cropPage.hideOriginAnchor" => |st| st.set_option("view.originAnchor", false),
}
