//! Interaction step: snap_objects — with Snap and Snap › To Objects on, a
//! point near a corner of a box the page draws lands on the corner;
//! far from everything it stays where it is; with To Objects off it is not
//! moved to the corner. A point near the middle of a line, or of a side of
//! the box, lands on that middle.

use super::*;

pub(super) async fn run(cx: &mut Ctx<'_>) {
    let st = cx.st;
    // The flat form draws a box from (50, 560) to (350, 640).
    let Ok(doc) = folitecta_core::Document::from_bytes(folitecta_core::testdocs::flat_form_doc(), None, None)
    else {
        cx.record("snap to objects", false, "the document could not be made".into());
        return;
    };
    crate::files::open_document_tab(st, doc);
    let tab = st.read_active(|t| t.id).unwrap_or(0);
    pause(400).await;
    let before = (
        crate::commands::option_on(st, "view.snap"),
        crate::commands::option_on(st, "view.snap.toObjects"),
    );
    st.set_option("view.snap", true);
    st.set_option("view.snap.toObjects", true);
    // The corner at (50, 560) of the page is (50, 282) on the page as shown.
    let near = crate::ui::canvas::geometry::snap(st, 0, 53.0, 280.0, 1.0);
    let far = crate::ui::canvas::geometry::snap(st, 0, 450.0, 450.0, 1.0);
    // The middle of the line from (100, 720) to (300, 720): (200, 122) as
    // shown; of the bottom side of the box: (200, 282).
    let line_middle = crate::ui::canvas::geometry::snap(st, 0, 203.0, 125.0, 1.0);
    let side_middle = crate::ui::canvas::geometry::snap(st, 0, 198.0, 284.0, 1.0);
    st.set_option("view.snap.toObjects", false);
    let off = crate::ui::canvas::geometry::snap(st, 0, 53.0, 280.0, 1.0);
    st.set_option("view.snap", before.0);
    st.set_option("view.snap.toObjects", before.1);
    crate::files::close_tab(st, tab, true, crate::state::AfterUnsaved::Close);
    pause(300).await;
    let at = |p: (f32, f32), x: f32, y: f32| (p.0 - x).abs() < 0.01 && (p.1 - y).abs() < 0.01;
    cx.record(
        "snap to objects: a point near a corner lands on it, one far from everything stays, and not with To Objects off",
        at(near, 50.0, 282.0) && at(far, 450.0, 450.0) && !at(off, 50.0, 282.0),
        format!("near {near:?}, far {far:?}, off {off:?}"),
    );
    cx.record(
        "snap to objects: a point near the middle of a line or of a side of a box lands on that middle",
        at(line_middle, 200.0, 122.0) && at(side_middle, 200.0, 282.0),
        format!("line {line_middle:?}, side {side_middle:?}"),
    );
}
