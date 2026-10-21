//! Window › Split: a second view of the document, side by side with the
//! first or below it, each with its own page, zoom and layout.

use dioxus::prelude::*;

use crate::commands::{handlers, no_doc};
use crate::state::{AppState, SplitView};

/// Scroll the views to their pages once they are drawn again: again after
/// the canvases took their new size, which changes the zoom of a view that
/// fits the width or the page.
fn show_pages(st: AppState) {
    let page = st.current_page();
    let other = st
        .read_active(|t| t.split.as_ref().map(|s| s.other.page))
        .flatten();
    let mut js = format!("document.getElementById('pg-{page}')?.scrollIntoView({{ block: 'start' }});");
    if let Some(p) = other {
        js.push_str(&format!(
            "document.getElementById('spg-{p}')?.scrollIntoView({{ block: 'start' }});"
        ));
    }
    spawn(async move {
        for wait in [200, 450] {
            tokio::time::sleep(std::time::Duration::from_millis(wait)).await;
            let _ = document::eval(&js).await;
        }
    });
}

/// Split the window (side by side, or one view above the other); the same
/// split again ends it.
fn split(st: AppState, side_by_side: bool) {
    if no_doc(st) {
        return;
    }
    let size = *st.canvas_size.peek();
    st.with_active(|t| match t.split.as_mut() {
        // A comparison side by side ends: a split of the document starts.
        Some(s) if s.source.is_some() => {
            t.split = Some(SplitView {
                side_by_side,
                focus: 0,
                other: t.view_state(),
                other_size: size,
                source: None,
            })
        }
        Some(s) if s.side_by_side == side_by_side => t.split = None,
        Some(s) => s.side_by_side = side_by_side,
        None => {
            t.split = Some(SplitView {
                side_by_side,
                focus: 0,
                other: t.view_state(),
                other_size: size,
                source: None,
            })
        }
    });
    show_pages(st);
}

/// End the split: the focused view stays.
fn remove(st: AppState) {
    if st.read_active(|t| t.split.is_some()).unwrap_or(false) {
        st.with_active(|t| t.split = None);
        show_pages(st);
    }
}

/// Check states of the split commands.
pub fn split_state(st: AppState, id: &str) -> Option<bool> {
    // A comparison side by side is not a split of the document.
    let side = |s: &SplitView| s.source.is_none() && s.side_by_side;
    match id {
        "window.split.vert" => st.read_active(|t| t.split.as_ref().is_some_and(side)),
        "window.split.horz" => st.read_active(|t| {
            t.split
                .as_ref()
                .is_some_and(|s| s.source.is_none() && !s.side_by_side)
        }),
        _ => None,
    }
}

handlers! {
    "window.split.vert" => |st| split(st, true),
    "window.split.horz" => |st| split(st, false),
    "window.removeSplit" => remove,
}
