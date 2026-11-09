//! Interaction step: Edit Paths — a click on a filled rectangle of the page
//! chooses it (its nodes shown with the bar of its colours); one of its
//! nodes dragged changes the page content; the fill made green; the path
//! deleted; each change undone. On the triangle, a double click on its
//! bottom edge adds a node there, a double click on that node removes it; a
//! click on its top node chooses it, Smooth on the bar makes the edge to it
//! a curve, Convert › Straight makes it straight again. Path Editing › 3
//! Point › Circle and three clicks draw a circle through them in the page
//! content, undone in one step.

use super::*;

/// A one-page PDF of 400 by 400 points drawing `content`.
fn drawing_pdf(content: &str) -> Vec<u8> {
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 400 400] /Contents 4 0 R /Resources << >> >>"
            .to_string(),
        format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()),
    ];
    let mut out = b"%PDF-1.7\n".to_vec();
    let mut offsets = Vec::new();
    for (i, o) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend(format!("{} 0 obj\n{o}\nendobj\n", i + 1).bytes());
    }
    let xref = out.len();
    out.extend(format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).bytes());
    for off in offsets {
        out.extend(format!("{off:010} 00000 n \n").bytes());
    }
    out.extend(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            objects.len() + 1
        )
        .bytes(),
    );
    out
}

async fn number(js: &str) -> f64 {
    eval(js).await.ok().and_then(|v| v.as_f64()).unwrap_or(-1.0)
}

pub(super) async fn run(cx: &mut Ctx<'_>) {
    let st = cx.st;
    let content = "q 1 0 0 RG 3 w 40 40 m 160 40 l 100 140 l h S Q 0 0 1 rg 200 200 100 80 re f";
    let Ok(doc) = folitecta_core::Document::from_bytes(drawing_pdf(content), None, None) else {
        cx.record("edit paths", false, "the document could not be made".into());
        return;
    };
    crate::files::open_document_tab(st, doc);
    let tab = st.read_active(|t| t.id).unwrap_or(0);
    pause(500).await;
    crate::commands::dispatch(st, "tool.editContent.shapes");
    pause(400).await;
    let boxes = number("return document.querySelectorAll('#canvas .path-box').length;").await;
    // A press on the rectangle (the second path) chooses it.
    let _ = eval(
        "const b = document.querySelectorAll('#canvas .path-box')[1]; if (b) { const r = b.getBoundingClientRect(); b.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, clientX: r.left + r.width / 2, clientY: r.top + r.height / 2 })); }",
    )
    .await;
    pause(400).await;
    let nodes = number("return document.querySelectorAll('#canvas .path-node').length;").await;
    let bar = number("return document.querySelectorAll('#canvas .path-toolbar').length;").await;
    // The far corner dragged 40 points right.
    let _ = eval(
        r#"
const n = document.querySelectorAll('#canvas .path-node')[1];
const r = n.getBoundingClientRect();
n.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, clientX: r.left + 5, clientY: r.top + 5 }));
"#,
    )
    .await;
    pause(300).await;
    let _ = eval(
        r#"
const layer = document.querySelector('#canvas .path-drag-layer');
const n = document.querySelectorAll('#canvas .path-node')[1];
const r = n.getBoundingClientRect();
const z = layer.getBoundingClientRect().width / 400;
layer.dispatchEvent(new MouseEvent('mousemove', { bubbles: true, clientX: r.left + 5 + 40 * z, clientY: r.top + 5 }));
"#,
    )
    .await;
    pause(300).await;
    let _ = eval(
        "const layer = document.querySelector('#canvas .path-drag-layer'); if (layer) layer.dispatchEvent(new MouseEvent('mouseup', { bubbles: true }));",
    )
    .await;
    pause(500).await;
    let bounds = st
        .read_active(|t| t.doc.page_paths(0).ok().and_then(|p| p.get(1).map(|x| x.bounds)))
        .flatten()
        .unwrap_or_default();
    // Green fill, then delete.
    crate::commands::paths::restyle(st, None, Some([0.0, 0.6, 0.0]), None);
    pause(300).await;
    let green = st
        .read_active(|t| {
            t.doc
                .page_paths(0)
                .ok()
                .and_then(|p| p.get(1).and_then(|x| x.fill))
        })
        .flatten();
    crate::commands::paths::delete(st);
    pause(300).await;
    let left = st
        .read_active(|t| t.doc.page_paths(0).map(|p| p.len()).unwrap_or(0))
        .unwrap_or(0);
    for _ in 0..3 {
        crate::commands::dispatch(st, "undo");
        pause(200).await;
    }
    let back = st
        .read_active(|t| t.doc.page_paths(0).ok().and_then(|p| p.get(1).map(|x| x.bounds)))
        .flatten()
        .unwrap_or_default();
    // The triangle: a node added on its bottom edge, then removed.
    let triangle_nodes = |st: AppState| {
        st.read_active(|t| {
            t.doc
                .page_paths(0)
                .ok()
                .and_then(|p| p.first().map(|x| x.nodes.len()))
        })
        .flatten()
        .unwrap_or(0)
    };
    let _ = eval(
        "const b = document.querySelectorAll('#canvas .path-box')[0]; if (b) { const r = b.getBoundingClientRect(); b.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, clientX: r.left + r.width / 2, clientY: r.bottom - 3 })); }",
    )
    .await;
    pause(400).await;
    let _ = eval(
        "const b = document.querySelector('#canvas .path-box.selected'); if (b) { const r = b.getBoundingClientRect(); b.dispatchEvent(new MouseEvent('dblclick', { bubbles: true, clientX: r.left + r.width / 2, clientY: r.bottom - 3 })); }",
    )
    .await;
    pause(400).await;
    let added = triangle_nodes(st);
    let new_node = st
        .read_active(|t| {
            t.doc
                .page_paths(0)
                .ok()
                .and_then(|p| p.first().map(|x| x.nodes[1].at))
        })
        .flatten()
        .unwrap_or_default();
    let _ = eval(
        "const n = document.querySelectorAll('#canvas .path-node')[1]; if (n) { const r = n.getBoundingClientRect(); n.dispatchEvent(new MouseEvent('dblclick', { bubbles: true, clientX: r.left + 5, clientY: r.top + 5 })); }",
    )
    .await;
    pause(400).await;
    let removed = triangle_nodes(st);
    cx.record(
        "edit paths: a double click on a path adds a node on its nearest edge, one on a node removes it",
        added == 4 && (new_node[0] - 100.0).abs() < 4.0 && (new_node[1] - 40.0).abs() < 4.0 && removed == 3,
        format!("nodes after adding {added} (the new one at {new_node:?}), after removing {removed}"),
    );
    // The top node of the triangle chosen with a click (a press and a
    // release without a move), then converted.
    let _ = eval(
        "const n = document.querySelectorAll('#canvas .path-node')[2]; if (n) { const r = n.getBoundingClientRect(); n.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, clientX: r.left + 5, clientY: r.top + 5 })); }",
    )
    .await;
    pause(300).await;
    let _ = eval(
        "const layer = document.querySelector('#canvas .path-drag-layer'); if (layer) layer.dispatchEvent(new MouseEvent('mouseup', { bubbles: true }));",
    )
    .await;
    pause(400).await;
    let chosen = number("return document.querySelectorAll('#canvas .path-node.chosen').length;").await;
    let buttons = number("return document.querySelectorAll('#canvas .path-node-cmd').length;").await;
    let _ = eval(
        "const b = document.querySelector('#canvas .path-node-cmd[data-cmd=\"pathedit.convert.smooth\"]'); if (b) b.click();",
    )
    .await;
    pause(400).await;
    let curved = triangle_nodes(st);
    crate::commands::dispatch(st, "pathedit.convert.straight");
    pause(400).await;
    let straight = triangle_nodes(st);
    cx.record(
        "edit paths: a click chooses a node; Smooth on the bar makes the edge to it a curve, Convert › Straight a line again",
        chosen == 1.0 && buttons == 4.0 && curved == 5 && straight == 3,
        format!("chosen {chosen}, buttons {buttons}, nodes when smooth {curved}, when straight {straight}"),
    );
    // A circle through three clicks (display points, y down): centre 300,250
    // and radius 50 on the page.
    let count = |st: AppState| {
        st.read_active(|t| t.doc.page_paths(0).map(|p| p.len()).unwrap_or(0))
            .unwrap_or(0)
    };
    let before = count(st);
    crate::commands::dispatch(st, "pathedit.3point.circle");
    pause(300).await;
    let z = crate::commands::current_zoom(st);
    for p in [(250.0, 150.0), (300.0, 100.0), (350.0, 150.0)] {
        let _ = pointer_path(0, z, &[p], false).await;
        pause(250).await;
    }
    pause(300).await;
    let circle = st
        .read_active(|t| t.doc.page_paths(0).ok().and_then(|p| p.last().map(|x| x.bounds)))
        .flatten()
        .unwrap_or_default();
    let drawn = count(st);
    crate::commands::dispatch(st, "undo");
    pause(300).await;
    let undone = count(st);
    let near = circle
        .iter()
        .zip([250.0, 200.0, 350.0, 300.0])
        .all(|(a, b)| (a - b).abs() < 3.0);
    cx.record(
        "edit paths: 3 Point › Circle and three clicks draw a circle through them in the page content, undone in one step",
        drawn == before + 1 && near && undone == before,
        format!("paths {before} → {drawn} → {undone} after undo, the circle within {circle:?}"),
    );
    st.set_tool(Tool::Hand);
    crate::files::close_tab(st, tab, true, crate::state::AfterUnsaved::Close);
    pause(300).await;
    let moved = (bounds[2] - 340.0).abs() < 2.0 && (bounds[0] - 200.0).abs() < 0.5;
    cx.record(
        "edit paths: a rectangle chosen with its nodes and bar; a node dragged in the page content; filled green; deleted; undone",
        boxes == 2.0 && nodes == 2.0 && bar == 1.0 && moved && green == Some([0.0, 0.6, 0.0]) && left == 1 && back == [200.0, 200.0, 300.0, 280.0],
        format!("boxes {boxes}, nodes {nodes}, bar {bar}, bounds {bounds:?}, green {green:?}, left {left}, back {back:?}"),
    );
}
