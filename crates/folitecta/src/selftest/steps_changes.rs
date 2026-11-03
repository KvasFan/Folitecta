//! Interaction step: changes — two pages turned with Rotate Pages are two
//! steps named after it in the History of Changes pane; a click on its first
//! row goes back before both, a click on the last forward again.

use super::*;

/// The rows of the pane: name and class.
async fn rows() -> Vec<(String, String)> {
    eval("return [...document.querySelectorAll('.changes-row')].map(r => [r.textContent.trim(), r.className]);")
        .await
        .ok()
        .and_then(|v| serde_json::from_value::<Vec<(String, String)>>(v).ok())
        .unwrap_or_default()
}

fn rotations(st: AppState) -> Vec<i32> {
    st.read_active(|t| t.doc.page_sizes().iter().map(|s| s.rotation).collect())
        .unwrap_or_default()
}

async fn rotate_current(st: AppState) {
    crate::commands::dispatch(st, "document.rotatePages");
    pause(400).await;
    let _ = eval("document.querySelector('.dialog-foot .tbtn.primary')?.click();").await;
    pause(500).await;
}

pub(super) async fn run(cx: &mut Ctx<'_>) {
    let st = cx.st;
    let Ok(mut doc) = folitecta_core::Document::new_blank(2, 595.0, 842.0) else {
        cx.record(
            "history of changes",
            false,
            "the document could not be made".into(),
        );
        return;
    };
    doc.clear_history();
    crate::files::open_document_tab(st, doc);
    let tab = st.read_active(|t| t.id).unwrap_or(0);
    pause(500).await;
    let was_open = st.ui.peek().show_left_pane;
    let was_pane = st.ui.peek().left_pane.clone();

    rotate_current(st).await;
    rotate_current(st).await;
    crate::commands::dispatch(st, "view.changes");
    pause(500).await;
    let listed = rows().await;
    let named = listed.len() == 3
        && listed[1].0.contains("Rotate Pages")
        && listed[2].0.contains("Rotate Pages")
        && listed[2].1.contains("current");
    let _ = capture_window(&cx.dir.join("interaction-changes.png"));

    // Back before both, then forward to the last.
    let _ = eval("document.querySelectorAll('.changes-row')[0]?.click();").await;
    pause(600).await;
    let back = rotations(st);
    let ahead = rows().await.iter().filter(|r| r.1.contains("ahead")).count();
    let _ = eval("const r = document.querySelectorAll('.changes-row'); r[r.length - 1]?.click();").await;
    pause(600).await;
    let forward = rotations(st);

    {
        let mut ui = st.ui_mut();
        ui.show_left_pane = was_open;
        ui.left_pane = was_pane;
    }
    crate::files::close_tab(st, tab, true, crate::state::AfterUnsaved::Close);
    pause(300).await;
    cx.record(
        "history of changes: two Rotate Pages steps named and listed; back before both and forward again with a click",
        named && back == [0, 0] && ahead == 2 && forward == [180, 0],
        format!("rows {listed:?}, back {back:?} with {ahead} ahead, forward {forward:?}"),
    );
}
