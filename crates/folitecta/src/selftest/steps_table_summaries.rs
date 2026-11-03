//! Interaction step: table_summaries — Fix of the rule Table summary asks a
//! summary for the table of the tags; written in its window, the rule passes.

use folitecta_core::accessibility::CheckStatus;

use super::*;

pub(super) async fn run(cx: &mut Ctx<'_>) {
    let st = cx.st;
    let made = folitecta_core::Document::new_blank(1, 400.0, 500.0).and_then(|mut d| {
        d.set_tagged(true)?;
        let table = d.add_struct_elem(None, None, "Table", "")?;
        let row = d.add_struct_elem(Some(table), None, "TR", "")?;
        d.add_struct_elem(Some(row), None, "TH", "")?;
        d.clear_history();
        Ok(d)
    });
    let Ok(doc) = made else {
        cx.record("table summaries", false, "the document could not be made".into());
        return;
    };
    crate::files::open_document_tab(st, doc);
    let tab = st.read_active(|t| t.id).unwrap_or(0);
    pause(400).await;
    let status = |st: AppState| {
        st.read_active(|t| {
            t.doc
                .accessibility_check()
                .into_iter()
                .find(|r| r.rule == "table_summary")
                .map(|r| r.status)
        })
        .flatten()
    };
    let before = status(st);
    crate::commands::accessibility::fix_rule(st, "table_summary");
    pause(500).await;
    let asked = matches!(*st.dialog.peek(), Dialog::TableSummaries);
    let _ = eval(
        "const a = document.querySelector('.dialog .table-summary-text'); if (a) { a.value = 'Sales by month'; a.dispatchEvent(new Event('input', { bubbles: true })); }",
    )
    .await;
    pause(200).await;
    let _ = eval("document.querySelector('.dialog-foot .tbtn.primary')?.click();").await;
    pause(500).await;
    let summary = st
        .read_active(|t| t.doc.table_summaries().first().map(|s| s.1.clone()))
        .flatten()
        .unwrap_or_default();
    let after = status(st);
    crate::files::close_tab(st, tab, true, crate::state::AfterUnsaved::Close);
    pause(300).await;
    cx.record(
        "accessibility: Fix of Table summary asks a summary for the table; written, the rule passes",
        before == Some(CheckStatus::Failed)
            && asked
            && summary == "Sales by month"
            && after == Some(CheckStatus::Passed),
        format!("before {before:?}, asked {asked}, summary {summary:?}, after {after:?}"),
    );
}
