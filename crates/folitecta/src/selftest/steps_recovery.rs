//! Interaction step 46: document recovery — a copy of a document with
//! unsaved changes is kept, and a copy left by a window that ended is
//! offered and opened again.

use super::*;
use crate::recovery::Entry;

pub(super) async fn run(cx: &mut Ctx<'_>) {
    let st = cx.st;
    // 46. The active document has changes: checked, it gets its copy.
    crate::recovery::checkpoint(st);
    pause(1500).await;
    let Some(dir) = crate::state::app_data_dir().map(|d| d.join("recovery")) else {
        cx.record("document recovery", false, "no data folder".into());
        return;
    };
    let tab = st.read_active(|t| t.id).unwrap_or(0);
    let own = dir.join(format!("{}-{tab}.pdf", std::process::id()));
    let kept = own.exists();

    // A copy left by another window, ten minutes ago.
    let left = Entry {
        pid: 1,
        tab: 99,
        title: "Left behind.pdf".into(),
        path: None,
        pages: 1,
        saved: 0,
        alive: 0,
        revision: 1,
    };
    let blank = folitecta_core::Document::new_blank(1, 300.0, 400.0)
        .ok()
        .map(|d| d.bytes().as_ref().clone())
        .unwrap_or_default();
    let _ = std::fs::write(dir.join("1-99.pdf"), blank);
    let _ = std::fs::write(
        dir.join("1-99.json"),
        serde_json::to_vec(&left).unwrap_or_default(),
    );
    let offered = crate::recovery::recoverable().iter().any(|e| e.tab == 99);
    let tabs = st.docs.read().len();
    crate::commands::dispatch(st, "view.recovery");
    pause(500).await;
    let listed = document::eval("return document.querySelectorAll('.dialog .recovery-row').length;")
        .await
        .ok()
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let _ = capture_window(&cx.dir.join("interaction-recovery.png"));
    let _ = document::eval("document.querySelector('.dialog .recovery-open')?.click();").await;
    pause(800).await;
    let opened = st.docs.read().len() == tabs + 1
        && st
            .read_active(|t| t.doc.is_dirty() && t.doc.page_count() == 1)
            .unwrap_or(false);
    let gone = !dir.join("1-99.json").exists() && crate::recovery::recoverable().is_empty();
    st.set_dialog(Dialog::None);
    // Back to the document of the other steps (the recovered one is closed).
    if opened {
        if let Some(id) = st.read_active(|t| t.id) {
            crate::files::close_tab(st, id, true, crate::state::AfterUnsaved::Close);
        }
    }
    let n = st.docs.read().len();
    if n > 0 {
        st.set_active(n - 1);
    }
    pause(400).await;
    cx.record(
        "document recovery: a copy kept, a copy left behind offered and opened",
        kept && offered && listed == 1 && opened && gone,
        format!("kept {kept}, offered {offered}, listed {listed}, opened {opened}, gone {gone}"),
    );
}
