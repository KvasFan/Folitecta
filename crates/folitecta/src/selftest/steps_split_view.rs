//! Interaction step 49: Window › Split — two views of the document, each
//! scrolled on its own; a press in the other view focuses it (its page
//! becomes the current page) and the split ends with the focused view.

use super::*;

async fn number(js: &str) -> i64 {
    eval(js).await.ok().and_then(|v| v.as_i64()).unwrap_or(-1)
}

pub(super) async fn run(cx: &mut Ctx<'_>) {
    let st = cx.st;
    // 49. Side by side.
    st.set_tool(Tool::Hand);
    st.go_to_page(0);
    pause(400).await;
    crate::commands::dispatch(st, "window.split.vert");
    pause(900).await;
    let canvases = number("return document.querySelectorAll('.canvas-split.side > .canvas').length;").await;
    let ids =
        eval("return [...document.querySelectorAll('.canvas-split > .canvas')].map(c => c.id).join(',');")
            .await
            .ok()
            .and_then(|v| v.as_str().map(str::to_string))
            .unwrap_or_default();
    // The other view scrolled to the fourth page on its own.
    let _ = eval(
        "const c = document.getElementById('canvas-other'), p = document.getElementById('spg-3'); if (c && p) c.scrollTop = p.offsetTop - 20;",
    )
    .await;
    pause(700).await;
    let other_page = st
        .read_active(|t| t.split.as_ref().map(|s| s.other.page))
        .flatten();
    let first_page = st.current_page();
    let _ = capture_window(&cx.dir.join("interaction-split-view.png"));
    // A press in it focuses it: the ids follow the focus.
    let _ = eval(
        "const c = document.getElementById('canvas-other'); if (c) { const r = c.getBoundingClientRect(); c.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, clientX: r.left + 30, clientY: r.top + 30 })); }",
    )
    .await;
    pause(600).await;
    let focused = st
        .read_active(|t| t.split.as_ref().map(|s| (s.focus, t.page, s.other.page)))
        .flatten();
    let ids_after =
        eval("return [...document.querySelectorAll('.canvas-split > .canvas')].map(c => c.id).join(',');")
            .await
            .ok()
            .and_then(|v| v.as_str().map(str::to_string))
            .unwrap_or_default();
    // One above the other, then one view again.
    crate::commands::dispatch(st, "window.split.horz");
    pause(500).await;
    let stacked = number("return document.querySelectorAll('.canvas-split.stacked > .canvas').length;").await;
    crate::commands::dispatch(st, "window.removeSplit");
    pause(1200).await;
    let single = number("return document.querySelectorAll('.canvas').length;").await;
    let page_after = st.current_page();
    cx.record(
        "split window: two views scrolled on their own, the other one focused by a press, then one view",
        canvases == 2
            && ids == "canvas,canvas-other"
            && other_page == Some(3)
            && first_page == 0
            && focused == Some((1, 3, 0))
            && ids_after == "canvas-other,canvas"
            && stacked == 2
            && single == 1
            && page_after == 3,
        format!(
            "canvases {canvases} ({ids}), other page {other_page:?}, first {first_page}, focused {focused:?} ({ids_after}), stacked {stacked}, single {single}, page {page_after}"
        ),
    );
    st.go_to_page(0);
    pause(300).await;
}
