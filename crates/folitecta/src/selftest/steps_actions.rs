//! Interaction step: Action Sequences — a watermark step added to the
//! sequence shown and the sequence run on the open document (one undo
//! step); then run on two files and a third whose output is there already:
//! the plan shows the collision, the run gives done, done, skipped.

use super::*;

async fn text(js: &str) -> String {
    eval(js)
        .await
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default()
}

pub(super) async fn run(cx: &mut Ctx<'_>) {
    let st = cx.st;
    let make = |n: usize| {
        let p = "Plain text of a page. ".repeat(40);
        let md = (0..n)
            .map(|i| format!("# Part {}\n\n{p}\n\n", i + 1))
            .collect::<String>();
        folitecta_core::Document::from_markdown(&md, folitecta_core::TextLayout::default(), "Report")
    };
    let Ok(doc) = make(2) else {
        cx.record("action sequences", false, "the document could not be made".into());
        return;
    };
    let original = doc.bytes();
    crate::files::open_document_tab(st, doc);
    let tab = st.read_active(|t| t.id).unwrap_or(0);
    pause(500).await;
    crate::commands::dispatch(st, "tools.actionSequences");
    pause(400).await;
    let _ = eval(
        "const s = document.querySelector('.dialog .actions-add-step'); s.value = 'watermark'; s.dispatchEvent(new Event('change', { bubbles: true }));",
    )
    .await;
    pause(200).await;
    let _ = eval("document.querySelector('.dialog .actions-add-btn')?.click();").await;
    pause(300).await;
    let steps = text("return String(document.querySelectorAll('.dialog .actions-step').length);").await;
    let _ = eval("document.querySelector('.dialog .actions-run-here')?.click();").await;
    pause(800).await;
    let marked = st
        .read_active(|t| t.doc.page_text(0).contains("DRAFT"))
        .unwrap_or(false);
    crate::commands::dispatch(st, "undo");
    pause(300).await;
    let undone = st.read_active(|t| t.doc.bytes() == original).unwrap_or(false);

    // On files: two new outputs and one already there.
    let dir = cx.dir.join("actions");
    let out = dir.join("out");
    let _ = std::fs::create_dir_all(&out);
    let mut inputs = Vec::new();
    for (i, n) in [1usize, 2, 1].into_iter().enumerate() {
        let path = dir.join(format!("file{i}.pdf"));
        if let Ok(mut d) = make(n) {
            let _ = d.save_as(&path);
        }
        inputs.push(path);
    }
    let _ = std::fs::write(out.join("file2-done.pdf"), "old");
    crate::commands::dispatch(st, "tools.actionSequences");
    pause(300).await;
    crate::commands::actions::set_inputs(
        st,
        inputs,
        folitecta_core::OutputRule {
            folder: Some(out.clone()),
            suffix: "-done".into(),
            overwrite: false,
        },
    );
    pause(400).await;
    let planned = text("return [...document.querySelectorAll('.dialog .actions-plan .audit-row')].map(r => r.children[2].textContent).join('|');").await;
    let _ = eval("document.querySelector('.dialog .actions-start')?.click();").await;
    let mut summary = String::new();
    for _ in 0..80 {
        pause(200).await;
        summary = text("return document.querySelector('.dialog .actions-summary')?.textContent || '';").await;
        if !summary.is_empty() {
            break;
        }
    }
    let statuses: Vec<&'static str> = crate::commands::actions::ACTIONS
        .peek()
        .batch
        .as_ref()
        .map(|b| b.results.iter().map(|r| r.status).collect())
        .unwrap_or_default();
    let written = out.join("file0-done.pdf").exists() && out.join("file1-done.pdf").exists();
    st.set_dialog(crate::state::Dialog::None);
    crate::files::close_tab(st, tab, true, crate::state::AfterUnsaved::Close);
    pause(300).await;
    cx.record(
        "action sequences: a step added and the sequence run on the document as one undo step; run on files with a collision: done, done, skipped",
        steps == "2" && marked && undone && planned.split('|').filter(|s| !s.is_empty()).count() == 1
            && statuses == ["done", "done", "skipped"] && written && !summary.is_empty(),
        format!("steps {steps}, marked {marked}, undone {undone}, planned {planned:?}, statuses {statuses:?}, written {written}, summary {summary:?}"),
    );
}
