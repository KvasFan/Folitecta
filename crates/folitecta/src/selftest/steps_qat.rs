//! Interaction step 57: the Quick Access Toolbar — a right click on a tool
//! of the ribbon puts it on the toolbar, where a click runs it; Customize
//! Toolbars finds and adds a command and a separator, moves the toolbar
//! below the ribbon; a right click takes the tool away and Reset brings
//! back the first toolbar.

use crate::commands::qat::items;

use super::*;

async fn count(selector: &str) -> u64 {
    eval(&format!(
        "return document.querySelectorAll({}).length;",
        serde_json::to_string(selector).unwrap_or_default()
    ))
    .await
    .ok()
    .and_then(|v| v.as_u64())
    .unwrap_or(0)
}

async fn click(selector: &str) {
    let _ = eval(&format!(
        "document.querySelector({})?.click();",
        serde_json::to_string(selector).unwrap_or_default()
    ))
    .await;
    pause(300).await;
}

async fn right_click(selector: &str) {
    let _ = eval(&format!(
        "document.querySelector({})?.dispatchEvent(new MouseEvent('contextmenu', {{ bubbles: true, cancelable: true, button: 2 }}));",
        serde_json::to_string(selector).unwrap_or_default()
    ))
    .await;
    pause(300).await;
}

pub(super) async fn run(cx: &mut Ctx<'_>) {
    let st = cx.st;
    // 57. The first toolbar, then the snapshot tool added with a right click.
    let saved = (st.value("qat.items"), st.value("qat.position"));
    st.ui_mut().values.remove("qat.items");
    st.ui_mut().values.remove("qat.position");
    st.set_ribbon_tab("home".into());
    pause(400).await;
    let first = count(".ribbon-right .qat-btn").await;
    right_click(".ribbon-body [data-cmd=\"tool.snapshot\"]").await;
    let added = count(".ribbon-right .qat-btn[data-cmd=\"tool.snapshot\"]").await;
    click(".ribbon-right .qat-btn[data-cmd=\"tool.snapshot\"]").await;
    let runs = (st.tool)() == Tool::Snapshot;
    st.set_tool(Tool::Hand);

    // Customize: a command found and added, a separator after the first entry.
    crate::commands::dispatch(st, "customizeToolbars");
    pause(400).await;
    let _ = eval(
        "const i = document.querySelector('.dialog .qat-search'); if (i) { i.value = 'rotate'; i.dispatchEvent(new Event('input', { bubbles: true })); }",
    )
    .await;
    pause(300).await;
    let found = count(".dialog .qat-add").await;
    click(".dialog .qat-add:not([disabled])").await;
    click(".dialog .qat-current .qat-entry").await;
    click(".dialog .qat-sep-btn").await;
    let separators = count(".ribbon-right .qat-sep").await;
    let entries = items(st).len();
    let _ = capture_window(&cx.dir.join("interaction-qat-customize.png"));
    click(".dialog .qat-below").await;
    pause(300).await;
    let row = count(".ribbon .qat-row").await == 1 && count(".ribbon-right").await == 0;
    st.set_dialog(Dialog::None);
    pause(300).await;
    let _ = capture_window(&cx.dir.join("interaction-qat-below.png"));
    // Taken away with a right click, then everything as at first.
    right_click(".qat-row .qat-btn[data-cmd=\"tool.snapshot\"]").await;
    let removed = !items(st).iter().any(|i| i.id == "tool.snapshot");
    crate::commands::dispatch(st, "customizeToolbars");
    pause(300).await;
    click(".dialog .qat-reset").await;
    crate::commands::dispatch(st, "QABarAbove");
    st.set_dialog(Dialog::None);
    pause(300).await;
    let back = count(".ribbon-right .qat-btn").await;
    for (key, value) in [("qat.items", saved.0), ("qat.position", saved.1)] {
        if value.is_empty() {
            st.ui_mut().values.remove(key);
        } else {
            st.set_value(key, value);
        }
    }
    cx.record(
        "quick access toolbar: a tool added with a right click and run from it, a command found and added with a separator, the toolbar moved below the ribbon, the tool taken away and the toolbar reset",
        first == 4 && added == 1 && runs && found > 0 && separators == 1 && entries == 7 && row && removed && back == 4,
        format!(
            "first {first}, added {added}, runs {runs}, found {found}, separators {separators}, entries {entries}, row {row}, removed {removed}, back {back}"
        ),
    );
}
