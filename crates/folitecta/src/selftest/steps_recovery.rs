//! Interaction step 46: document recovery — a copy of a document with
//! unsaved changes is kept, and a copy left by a window that ended is
//! offered, opened again and kept for the new document.

use super::*;
use crate::recovery::{Entry, folder, session};

pub(super) async fn run(cx: &mut Ctx<'_>) {
    let st = cx.st;
    // 46. The active document has changes: checked, it gets its copy.
    crate::recovery::checkpoint(st);
    pause(1500).await;
    let Some(dir) = folder() else {
        cx.record("document recovery", false, "no data folder".into());
        return;
    };
    let tab = st.read_active(|t| t.id).unwrap_or(0);
    let kept = dir.join(format!("{}-{tab}.pdf", session())).exists();

    // A copy left by a window that ended (no lock held for its session).
    let left = Entry {
        session: "1-1".into(),
        tab: 99,
        title: "Left behind.pdf".into(),
        path: None,
        pages: 1,
        saved: 0,
    };
    let blank = folitecta_core::Document::new_blank(1, 300.0, 400.0)
        .ok()
        .map(|d| d.bytes().as_ref().clone())
        .unwrap_or_default();
    let _ = std::fs::write(dir.join("1-1-99.pdf"), blank);
    let _ = std::fs::write(
        dir.join("1-1-99.json"),
        serde_json::to_vec(&left).unwrap_or_default(),
    );
    let offered = crate::recovery::recoverable().iter().any(|e| e.tab == 99);
    let tabs = st.docs.read().len();
    crate::commands::dispatch(st, "view.recovery");
    pause(500).await;
    let listed = document::eval(
        "return [...document.querySelectorAll('.dialog .recovery-row')].filter(r => r.textContent.includes('Left behind.pdf')).length;",
    )
    .await
    .ok()
    .and_then(|v| v.as_u64())
    .unwrap_or(0);
    let _ = capture_window(&cx.dir.join("interaction-recovery.png"));
    let _ = document::eval(
        "[...document.querySelectorAll('.dialog .recovery-row')].find(r => r.textContent.includes('Left behind.pdf'))?.querySelector('.recovery-open')?.click();",
    )
    .await;
    pause(800).await;
    let new_tab = st.read_active(|t| t.id).unwrap_or(0);
    let opened = st.docs.read().len() == tabs + 1
        && st
            .read_active(|t| t.doc.is_dirty() && t.doc.page_count() == 1 && t.title() == "Left behind.pdf")
            .unwrap_or(false);
    // The copy now belongs to the new document; the old note is gone.
    let adopted = dir.join(format!("{}-{new_tab}.pdf", session())).exists()
        && dir.join(format!("{}-{new_tab}.json", session())).exists()
        && !dir.join("1-1-99.json").exists()
        && !crate::recovery::recoverable().iter().any(|e| e.tab == 99);
    st.set_dialog(Dialog::None);
    // Back to the document of the other steps; the closed one's copy goes.
    if opened {
        crate::files::close_tab(st, new_tab, true, crate::state::AfterUnsaved::Close);
    }
    let n = st.docs.read().len();
    if n > 0 {
        st.set_active(n - 1);
    }
    pause(1200).await;
    let cleaned = !dir.join(format!("{}-{new_tab}.json", session())).exists();
    cx.record(
        "document recovery: a copy kept, a copy left behind offered, opened and kept",
        kept && offered && listed == 1 && opened && adopted && cleaned,
        format!("kept {kept}, offered {offered}, listed {listed}, opened {opened}, adopted {adopted}, cleaned {cleaned}"),
    );
}
