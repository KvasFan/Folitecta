//! Interaction step: complete — the commands that finish the feature set:
//! group commands open the list of their group's commands, and a row runs
//! its command.

use super::*;

/// The command ids of the rows of the open group window.
async fn rows() -> Vec<String> {
    eval("return [...document.querySelectorAll('.dialog .command-group-row')].map(b => b.dataset.cmd);")
        .await
        .ok()
        .and_then(|v| serde_json::from_value::<Vec<String>>(v).ok())
        .unwrap_or_default()
}

pub(super) async fn run(cx: &mut Ctx<'_>) {
    let st = cx.st;
    let Ok(doc) = folitecta_core::Document::from_bytes(folitecta_core::testdocs::text_doc(2), None, None)
    else {
        cx.record("complete", false, "the document could not be made".into());
        return;
    };
    crate::files::open_document_tab(st, doc);
    let tab = st.read_active(|t| t.id).unwrap_or(0);
    pause(500).await;

    // Other Tools lists the tools; a click on the loupe chooses it.
    st.set_tool(Tool::Hand);
    crate::commands::dispatch(st, "otherToolsGroup");
    pause(400).await;
    let tools = rows().await;
    let _ =
        eval("document.querySelector('.dialog .command-group-row[data-cmd=\"tool.loupe\"]')?.click();").await;
    pause(400).await;
    let loupe = (st.tool)() == Tool::Loupe;
    let closed = matches!(*st.dialog.peek(), Dialog::None);
    st.set_tool(Tool::Hand);
    // Every group command lists commands that work, none planned.
    let groups = [
        "clipboardGroup",
        "selectionGroup",
        "view.snapGroup",
        "webLinksGroup",
        "shareGroup",
        "view.panesGroup",
        "view.otherPanes",
        "view.otherToolsGroup",
        "comment.otherToolsGroup",
        "view.pageLayoutTypesGroup",
        "view.rotateViewGroup",
        "view.docTabsGroup",
        "view.show",
        "tools.basic",
        "tools.markup",
        "tools.editContent",
        "tools.editLinks",
        "tools.measure",
        "comments.newGroup",
        "comments.deleteGroup",
        "document.pagesMore",
    ];
    let mut empty = Vec::new();
    for g in groups {
        crate::commands::dispatch(st, g);
        pause(200).await;
        let r = rows().await;
        let badges = eval("return document.querySelectorAll('.dialog .command-group .badge').length;")
            .await
            .ok()
            .and_then(|v| v.as_u64())
            .unwrap_or(1);
        if r.is_empty() || badges > 0 {
            empty.push(g);
        }
        st.set_dialog(Dialog::None);
        pause(100).await;
    }
    // The rotate group turns the view.
    crate::commands::dispatch(st, "view.rotateViewGroup");
    pause(300).await;
    let _ =
        eval("document.querySelector('.dialog .command-group-row[data-cmd=\"view.rotate.CW\"]')?.click();")
            .await;
    pause(300).await;
    let turned = st.read_active(|t| t.view_rotation).unwrap_or(0);
    crate::commands::dispatch(st, "view.rotate.reset");
    cx.record(
        "group commands list their group's commands; a row runs its command and closes the window",
        tools.contains(&"tool.loupe".to_string()) && loupe && closed && empty.is_empty() && turned == 90,
        format!("other tools {tools:?}, loupe {loupe}, closed {closed}, groups empty or planned {empty:?}, view turned {turned}"),
    );

    crate::files::close_tab(st, tab, true, crate::state::AfterUnsaved::Close);
    pause(300).await;
}
