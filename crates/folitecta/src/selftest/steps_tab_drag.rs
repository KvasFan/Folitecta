//! Interaction step: tab_drag — of three tabs in two groups, one dragged
//! onto the strip of the other group moves there; a tab dropped on another
//! goes before it.

use super::*;

/// Drag the tab `id` and drop it on what `target` (a selector) finds.
async fn drag(id: u64, target: &str) {
    let _ = eval(&format!(
        r#"const from = document.querySelector('.doc-tab[data-tab="{id}"]');
const to = document.querySelector({target});
if (from && to) {{
  const data = new DataTransfer();
  from.dispatchEvent(new DragEvent('dragstart', {{ bubbles: true, dataTransfer: data }}));
  to.dispatchEvent(new DragEvent('dragover', {{ bubbles: true, cancelable: true, dataTransfer: data }}));
  to.dispatchEvent(new DragEvent('drop', {{ bubbles: true, cancelable: true, dataTransfer: data }}));
  from.dispatchEvent(new DragEvent('dragend', {{ bubbles: true, dataTransfer: data }}));
}}"#,
        target = serde_json::to_string(target).unwrap_or_default()
    ))
    .await;
    pause(700).await;
}

fn groups(st: AppState) -> Option<[Vec<u64>; 2]> {
    st.groups.read().clone().map(|g| g.tabs)
}

pub(super) async fn run(cx: &mut Ctx<'_>) {
    let st = cx.st;
    let made: Option<Vec<folitecta_core::Document>> = (0..3)
        .map(|_| folitecta_core::Document::new_blank(1, 300.0, 400.0).ok())
        .collect();
    let Some(docs) = made else {
        cx.record("tabs dragged", false, "the documents could not be made".into());
        return;
    };
    let before = st.docs.read().len();
    for d in docs {
        crate::files::open_document_tab(st, d);
    }
    pause(600).await;
    let ids: Vec<u64> = st.docs.read().iter().map(|t| t.id).collect();
    let (a, b, c) = (ids[before], ids[before + 1], ids[before + 2]);

    // The last tab to a group of its own, then the second dragged to it.
    crate::commands::dispatch(st, "window.activeDocToNewVertTabGroup");
    pause(800).await;
    let start = groups(st);
    drag(b, ".tab-group[data-group=\"1\"] .doc-tabs").await;
    let moved = groups(st);
    // In that group, the third dropped on the second goes before it.
    drag(c, &format!(".doc-tab[data-tab=\"{b}\"]")).await;
    let order: Vec<u64> = st
        .docs
        .read()
        .iter()
        .map(|t| t.id)
        .filter(|id| [a, b, c].contains(id))
        .collect();

    crate::commands::dispatch(st, "window.allDocsToOneTabGroup");
    pause(500).await;
    for id in [a, b, c] {
        crate::files::close_tab(st, id, true, crate::state::AfterUnsaved::Close);
    }
    pause(400).await;
    let start_ok = start.as_ref().is_some_and(|g| g[1] == [c]);
    let moved_ok = moved
        .as_ref()
        .is_some_and(|g| g[0].contains(&a) && g[1].contains(&b) && g[1].contains(&c));
    cx.record(
        "tabs dragged: one onto the strip of the other group moves there; one dropped on another goes before it",
        start_ok && moved_ok && order == [a, c, b],
        format!("groups at first {start:?}, after the drag {moved:?}, order {order:?} (tabs {a}, {b}, {c})"),
    );
}
