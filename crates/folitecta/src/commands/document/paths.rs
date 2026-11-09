//! Edit Paths (FOL-L021): the tool of Edit Content › Shapes. A click chooses
//! a path the page draws; its nodes are dragged to move them, a double
//! click on a node removes it, a double click on the path adds one on the
//! nearest segment; a click on a node chooses it, and Convert makes it a
//! corner, smooth or straight, Anchors › Add puts a node after it; the bar
//! over the path changes its colours and width or deletes it. Every change
//! is one undo step of the document.

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
    /// Where it was when the drag began.
    pub from: [f32; 2],
}

pub static NODE_DRAG: GlobalSignal<Option<NodeDrag>> = Signal::global(|| None);

/// The node chosen with a click: page, path and node.
pub static NODE_SEL: GlobalSignal<Option<(usize, usize, usize)>> = Signal::global(|| None);

/// The node chosen on the path chosen.
pub fn chosen_node(st: AppState) -> Option<(usize, usize, usize)> {
    let (page, path) = selected(st)?;
    NODE_SEL.peek().filter(|(p, q, _)| *p == page && *q == path)
}

/// The path chosen, as page and index, when the tool is Edit Paths.
pub fn selected(st: AppState) -> Option<(usize, usize)> {
    st.read_active(|t| t.path_sel)
        .flatten()
        .filter(|_| *st.tool.peek() == Tool::EditPaths)
}

pub fn choose(st: AppState, page: usize, index: usize) {
    if st.read_active(|t| t.path_sel) != Some(Some((page, index))) {
        st.with_active(|t| t.path_sel = Some((page, index)));
        *NODE_SEL.write() = None;
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
    // A click without a move (the first of a double click) chooses the node.
    if (d.at[0] - d.from[0]).abs() < 0.01 && (d.at[1] - d.from[1]).abs() < 0.01 {
        *NODE_SEL.write() = Some((d.page, d.path, d.node));
        return;
    }
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

fn report(st: AppState, result: Option<folitecta_core::Result<()>>) {
    match result {
        Some(Ok(())) => crate::files::set_window_title(st),
        Some(Err(e)) => st.error(st.core_error_text(&e)),
        None => {}
    }
}

/// The node of `nodes` (display points) after which a node at `at` goes:
/// the start of the segment nearest to it.
pub fn segment_before(nodes: &[[f32; 2]], at: [f32; 2]) -> usize {
    let dist = |a: [f32; 2], b: [f32; 2]| {
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let len = dx * dx + dy * dy;
        let t = if len > 0.0 {
            (((at[0] - a[0]) * dx + (at[1] - a[1]) * dy) / len).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let (px, py) = (a[0] + t * dx - at[0], a[1] + t * dy - at[1]);
        px * px + py * py
    };
    (0..nodes.len().saturating_sub(1))
        .min_by(|i, j| dist(nodes[*i], nodes[i + 1]).total_cmp(&dist(nodes[*j], nodes[j + 1])))
        .unwrap_or(0)
}

/// Add a node to the path chosen at `at` (display points of the page),
/// on the segment nearest to it.
pub fn add_node(st: AppState, page: usize, path: usize, nodes: &[[f32; 2]], at: [f32; 2]) {
    let after = segment_before(nodes, at);
    let result = st.with_active(|t| {
        let size = t.doc.page_size(page)?;
        let (x, y) = display_to_pdf(&size, at[0], size.height - at[1]);
        t.doc.add_path_node(page, path, after, [x, y])
    });
    report(st, result);
}

/// Remove node `node` of the path chosen.
pub fn remove_node(st: AppState, page: usize, path: usize, node: usize) {
    *NODE_SEL.write() = None;
    report(st, st.with_active(|t| t.doc.remove_path_node(page, path, node)));
}

/// The node chosen made `kind`; without one, a note says how to choose it.
fn convert(st: AppState, kind: folitecta_core::NodeKind) {
    let Some((page, path, node)) = chosen_node(st) else {
        st.toast(
            crate::state::ToastKind::Info,
            crate::i18n::t(st.lang(), "paths.choose_node"),
            "",
        );
        return;
    };
    report(
        st,
        st.with_active(|t| t.doc.convert_path_node(page, path, node, kind)),
    );
}

fn corner(st: AppState) {
    convert(st, folitecta_core::NodeKind::Corner);
}

fn smooth(st: AppState) {
    convert(st, folitecta_core::NodeKind::Smooth);
}

fn straight(st: AppState) {
    convert(st, folitecta_core::NodeKind::Straight);
}

/// A node halfway along the segment after the node chosen.
fn add_after(st: AppState) {
    let Some((page, path, node)) = chosen_node(st) else {
        st.toast(
            crate::state::ToastKind::Info,
            crate::i18n::t(st.lang(), "paths.choose_node"),
            "",
        );
        return;
    };
    let result = st.with_active(|t| {
        let paths = t.doc.page_paths(page)?;
        let nodes = &paths
            .get(path)
            .ok_or_else(|| folitecta_core::Error::Invalid("no such path".into()))?
            .nodes;
        let (a, b) = match (nodes.get(node), nodes.get(node + 1)) {
            (Some(a), Some(b)) => (a.at, b.at),
            _ => {
                return Err(folitecta_core::Error::Invalid(
                    "the last node has no segment after it".into(),
                ));
            }
        };
        t.doc
            .add_path_node(page, path, node, [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0])
    });
    report(st, result);
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
    let tool = (st.tool)();
    match id {
        "tool.editContent.shapes" => Some(tool == Tool::EditPaths),
        "pathedit.3point.arc" => Some(tool == Tool::ThreePoint(folitecta_core::ThreePoint::Arc)),
        "pathedit.3point.circle" => Some(tool == Tool::ThreePoint(folitecta_core::ThreePoint::Circle)),
        "pathedit.3point.rect" => Some(tool == Tool::ThreePoint(folitecta_core::ThreePoint::Rect)),
        _ => None,
    }
}

fn tool(st: AppState) {
    if no_doc(st) {
        return;
    }
    select_tool(st, Tool::EditPaths);
}

/// Draw an arc, a circle or a rectangle with three clicks.
fn three_point(st: AppState, shape: folitecta_core::ThreePoint) {
    if no_doc(st) {
        return;
    }
    select_tool(st, Tool::ThreePoint(shape));
}

fn arc(st: AppState) {
    three_point(st, folitecta_core::ThreePoint::Arc);
}

fn circle(st: AppState) {
    three_point(st, folitecta_core::ThreePoint::Circle);
}

fn rect(st: AppState) {
    three_point(st, folitecta_core::ThreePoint::Rect);
}

handlers! {
    "tool.editContent.shapes" => tool,
    "pathedit.3point.arc" => arc,
    "pathedit.3point.circle" => circle,
    "pathedit.3point.rect" => rect,
    "pathedit.convert.corner" => corner,
    "pathedit.convert.smooth" => smooth,
    "pathedit.convert.straight" => straight,
    "pathedit.anchors.add" => add_after,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_node_goes_on_the_nearest_segment() {
        let nodes = [[0.0, 0.0], [100.0, 0.0], [100.0, 100.0]];
        assert_eq!(segment_before(&nodes, [50.0, 3.0]), 0);
        assert_eq!(segment_before(&nodes, [97.0, 60.0]), 1);
        assert_eq!(segment_before(&[[1.0, 1.0]], [5.0, 5.0]), 0);
    }

    #[test]
    fn colours_of_colour_inputs() {
        assert_eq!(parse_colour("#ff0080"), Some([1.0, 0.0, 128.0 / 255.0]));
        assert_eq!(parse_colour("nope"), None);
        assert_eq!(colour_hex([1.0, 0.0, 0.5]), "#ff0080");
    }
}
