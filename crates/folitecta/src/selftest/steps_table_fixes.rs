//! Interaction step: table_fixes — a tagged table without header cells and
//! with a short row fails two rules of the accessibility check; Fix gives it
//! header cells, then a span, and the check passes both.

use folitecta_core::accessibility::CheckStatus;

use super::*;

fn table() -> Option<folitecta_core::Document> {
    let mut doc = folitecta_core::Document::new_blank(1, 400.0, 500.0).ok()?;
    doc.set_tagged(true).ok()?;
    let table = doc.add_struct_elem(None, None, "Table", "").ok()?;
    for count in [3, 3, 2] {
        let row = doc.add_struct_elem(Some(table), None, "TR", "").ok()?;
        for _ in 0..count {
            doc.add_struct_elem(Some(row), None, "TD", "").ok()?;
        }
    }
    doc.clear_history();
    Some(doc)
}

fn statuses(st: AppState) -> (Option<CheckStatus>, Option<CheckStatus>) {
    st.read_active(|t| {
        let r = t.doc.accessibility_check();
        let of = |id: &str| r.iter().find(|x| x.rule == id).map(|x| x.status);
        (of("table_headers"), of("table_regular"))
    })
    .unwrap_or((None, None))
}

pub(super) async fn run(cx: &mut Ctx<'_>) {
    let st = cx.st;
    let Some(doc) = table() else {
        cx.record("table fixes", false, "the document could not be made".into());
        return;
    };
    crate::files::open_document_tab(st, doc);
    let tab = st.read_active(|t| t.id).unwrap_or(0);
    pause(400).await;
    let before = statuses(st);
    crate::commands::accessibility::fix_rule(st, "table_headers");
    pause(300).await;
    crate::commands::accessibility::fix_rule(st, "table_regular");
    pause(300).await;
    let after = statuses(st);
    let steps = st.read_active(|t| t.doc.undo_steps().len()).unwrap_or(0);
    crate::files::close_tab(st, tab, true, crate::state::AfterUnsaved::Close);
    pause(300).await;
    let failed = Some(CheckStatus::Failed);
    let passed = Some(CheckStatus::Passed);
    cx.record(
        "accessibility: a table without headers and with a short row fixed in two steps, both rules then pass",
        before == (failed, failed) && after == (passed, passed) && steps == 2,
        format!("before {before:?}, after {after:?}, {steps} undo steps"),
    );
}
