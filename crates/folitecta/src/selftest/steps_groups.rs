//! Interaction step: tab groups and windows — the active document moved to
//! a new group beside the others, each group with its tab strip; a press in
//! the other group's view and a click on its tab make its document the one
//! worked in; the tab moved back ends the groups; the tabs shared out one
//! group above the other and gathered again; a saved document moved to a
//! new window (noted, not started, in the self-test) and its tab closed.

use super::*;
use folitecta_core::{Document, Template, TemplateOptions};

async fn number(js: &str) -> i64 {
    eval(js).await.ok().and_then(|v| v.as_i64()).unwrap_or(-1)
}

fn paper(t: Template) -> Option<Document> {
    Document::from_template(
        t,
        &TemplateOptions {
            pages: 3,
            ..TemplateOptions::default()
        },
    )
    .ok()
}

pub(super) async fn run(cx: &mut Ctx<'_>) {
    let st = cx.st;
    let before = st.docs.read().len();
    let file = cx.dir.join("window.pdf");
    let (Some(a), Some(b), Some(())) = (
        paper(Template::Ruled),
        paper(Template::Squared),
        paper(Template::Dotted).and_then(|d| std::fs::write(&file, d.bytes().as_ref()).ok()),
    ) else {
        cx.record("tab groups", false, "the documents could not be made".into());
        return;
    };
    crate::files::open_document_tab(st, a);
    crate::files::open_document_tab(st, b);
    crate::files::open_path(st, file.clone(), None);
    pause(900).await;
    let ids: Vec<u64> = st.docs.read().iter().map(|t| t.id).collect();
    let (id_b, id_c) = (ids[ids.len() - 2], ids[ids.len() - 1]);

    // The file's tab to a new group beside the others.
    crate::commands::dispatch(st, "window.activeDocToNewVertTabGroup");
    pause(900).await;
    let groups =
        number("return document.querySelectorAll('.canvas-split.side.groups > .tab-group').length;").await;
    let strips = eval(
        "return [...document.querySelectorAll('.tab-group .doc-tabs')].map(s => s.querySelectorAll('.doc-tab:not(.add)').length).join(',');",
    )
    .await
    .ok()
    .and_then(|v| v.as_str().map(str::to_string))
    .unwrap_or_default();
    let first = st
        .groups
        .read()
        .clone()
        .map(|g| (g.focus, g.shown, st.read_active(|t| t.id)));

    // A press in the other group's view: the squared paper is worked in.
    let _ = eval(
        "const c = document.getElementById('canvas-other'); if (c) { const r = c.getBoundingClientRect(); c.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, clientX: r.left + 30, clientY: r.top + 30 })); }",
    )
    .await;
    pause(700).await;
    let pressed = (
        st.read_active(|t| t.id),
        st.groups.read().as_ref().map(|g| g.focus),
    );
    // A click on the file's tab in its group: it is worked in again.
    let _ = eval("document.querySelectorAll('.tab-group')[1]?.querySelector('.doc-tab:not(.add)')?.click();")
        .await;
    pause(700).await;
    let clicked = st.read_active(|t| t.id);
    // Back to the first group: one group again.
    crate::commands::dispatch(st, "window.moveTabTo");
    pause(800).await;
    let ended = (
        st.groups.read().is_none(),
        number("return document.querySelectorAll('.tab-group').length;").await,
        st.docs.read().iter().all(|t| t.split.is_none()),
    );
    // Shared out one above the other, then gathered.
    crate::commands::dispatch(st, "window.distributeDocsHorz");
    pause(800).await;
    let stacked =
        number("return document.querySelectorAll('.canvas-split.stacked.groups > .tab-group').length;").await;
    crate::commands::dispatch(st, "window.allDocsToOneTabGroup");
    pause(800).await;
    let gathered = st.groups.read().is_none()
        && number("return document.querySelectorAll('.tab-group').length;").await == 0;

    // The file to a new window: noted, its tab closed here.
    let i = st.docs.read().iter().position(|t| t.id == id_c).unwrap_or(0);
    st.set_active(i);
    pause(300).await;
    let noted_before = crate::commands::tab_groups::noted().len();
    crate::commands::dispatch(st, "window.activeDocToNewWindow");
    pause(600).await;
    let noted = crate::commands::tab_groups::noted();
    let moved = noted.len() == noted_before + 1
        && noted.last().is_some_and(|f| f.len() == 1 && f[0] == file)
        && !st.docs.read().iter().any(|t| t.id == id_c);
    let left: Vec<u64> = st.docs.read().iter().skip(before).map(|t| t.id).collect();
    for id in left {
        crate::files::close_tab(st, id, true, crate::state::AfterUnsaved::Close);
        pause(200).await;
    }
    cx.record(
        "tab groups side by side with their strips, the other group worked in by a press and by its tab, ended, stacked and gathered; a saved document moved to a new window",
        groups == 2
            && strips == format!("{},1", before + 2)
            && first.as_ref().is_some_and(|f| f.0 == 1 && f.1 == [id_b, id_c] && f.2 == Some(id_c))
            && pressed == (Some(id_b), Some(0))
            && clicked == Some(id_c)
            && ended == (true, 0, true)
            && stacked == 2
            && gathered
            && moved,
        format!(
            "groups {groups}, strips {strips}, first {first:?}, pressed {pressed:?}, clicked {clicked:?} (c {id_c}), ended {ended:?}, stacked {stacked}, gathered {gathered}, noted {noted:?}"
        ),
    );
}
