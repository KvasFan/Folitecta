//! Edit Paths (FOL-L021): the tool of Edit Content › Shapes. A click chooses
//! a path the page draws; its nodes are dragged to move them; the bar over
//! it changes its colours and width or deletes it. Every change is one
//! undo step of the document.

use dioxus::prelude::*;
use folitecta_core::marks::display_to_pdf;

use crate::commands::{handlers, no_doc, select_tool};
use crate::state::{AppState, Tool};

/// A node being dragged: page, path, node and where it is now (display
/// points of the page, y down).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NodeDrag {
    pub page: usize,
    pub path: usize,
    pub node: usize,
    pub at: [f32; 2],
}

pub static NODE_DRAG: GlobalSignal<Option<NodeDrag>> = Signal::global(|| None);

/// The path chosen, as page and index, when the tool is Edit Paths.
pub fn selected(st: AppState) -> Option<(usize, usize)> {
    st.read_active(|t| t.path_sel)
        .flatten()
        .filter(|_| *st.tool.peek() == Tool::EditPaths)
}

pub fn choose(st: AppState, page: usize, index: usize) {
    if st.read_active(|t| t.path_sel) != Some(Some((page, index))) {
        st.with_active(|t| t.path_sel = Some((page, index)));
    }
}

pub fn clear(st: AppState) {
    if st.read_active(|t| t.path_sel.is_some()).unwrap_or(false) {
        st.with_active(|t| t.path_sel = None);
    }
}

/// The node dragged is dropped at its place: the content changes.
pub fn drop_node(st: AppState) {
    let Some(d) = *NODE_DRAG.peek() else { return };
    *NODE_DRAG.write() = None;
    let result = st.with_active(|t| {
        let size = t.doc.page_size(d.page)?;
        let (x, y) = display_to_pdf(&size, d.at[0], size.height - d.at[1]);
        t.doc.move_path_node(d.page, d.path, d.node, [x, y])
    });
    match result {
        Some(Ok(())) => crate::files::set_window_title(st),
        Some(Err(e)) => st.error(st.core_error_text(&e)),
        None => {}
    }
}

/// Change the colours or the width of the path chosen.
pub fn restyle(st: AppState, stroke: Option<[f32; 3]>, fill: Option<[f32; 3]>, width: Option<f32>) {
    let Some((page, index)) = selected(st) else { return };
    match st.with_active(|t| t.doc.set_path_style(page, index, stroke, fill, width)) {
        Some(Ok(())) => crate::files::set_window_title(st),
        Some(Err(e)) => st.error(st.core_error_text(&e)),
        None => {}
    }
}

/// Delete the path chosen.
pub fn delete(st: AppState) {
    let Some((page, index)) = selected(st) else { return };
    match st.with_active(|t| t.doc.delete_path(page, index)) {
        Some(Ok(())) => {
            st.with_active(|t| t.path_sel = None);
            crate::files::set_window_title(st);
        }
        Some(Err(e)) => st.error(st.core_error_text(&e)),
        None => {}
    }
}

/// A colour of a colour input (`#rrggbb`).
pub fn parse_colour(hex: &str) -> Option<[f32; 3]> {
    let h = hex.trim().trim_start_matches('#');
    if h.len() != 6 {
        return None;
    }
    let c = |i: usize| {
        u8::from_str_radix(&h[i..i + 2], 16)
            .ok()
            .map(|v| v as f32 / 255.0)
    };
    Some([c(0)?, c(2)?, c(4)?])
}

pub fn colour_hex(c: [f32; 3]) -> String {
    let b = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!("#{:02x}{:02x}{:02x}", b(c[0]), b(c[1]), b(c[2]))
}

/// Check state of the tool.
pub fn paths_state(st: AppState, id: &str) -> Option<bool> {
    (id == "tool.editContent.shapes").then(|| (st.tool)() == Tool::EditPaths)
}

fn tool(st: AppState) {
    if no_doc(st) {
        return;
    }
    select_tool(st, Tool::EditPaths);
}

handlers! {
    "tool.editContent.shapes" => tool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colours_of_colour_inputs() {
        assert_eq!(parse_colour("#ff0080"), Some([1.0, 0.0, 128.0 / 255.0]));
        assert_eq!(parse_colour("nope"), None);
        assert_eq!(colour_hex([1.0, 0.0, 0.5]), "#ff0080");
    }
}
