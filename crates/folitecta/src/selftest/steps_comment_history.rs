//! Interaction step: comment_history — a note with a reply and a review
//! state: its history window lists when it was made, the reply and the
//! state, in that order.

use super::*;

pub(super) async fn run(cx: &mut Ctx<'_>) {
    let st = cx.st;
    let made = folitecta_core::Document::new_blank(1, 400.0, 400.0).and_then(|mut d| {
        d.add_note(0, 50.0, 50.0, "Check the figures", "Ada", [1.0, 1.0, 0.0])?;
        let note = d.annotations(0)?[0].index;
        d.add_reply(0, note, "They are right", "Ben")?;
        d.set_review_state(0, note, "Review", "Accepted", "Ben", "")?;
        d.clear_history();
        Ok((d, note))
    });
    let Ok((doc, note)) = made else {
        cx.record("comment history", false, "the document could not be made".into());
        return;
    };
    crate::files::open_document_tab(st, doc);
    let tab = st.read_active(|t| t.id).unwrap_or(0);
    pause(400).await;
    st.set_dialog(crate::state::Dialog::ReviewHistory { page: 0, index: note });
    pause(400).await;
    let rows = eval("return Array.from(document.querySelectorAll('.dialog .history-entry')).map(r => r.cells[1].textContent + ':' + r.cells[2].textContent).join('|');")
        .await
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default();
    st.set_dialog(crate::state::Dialog::None);
    crate::files::close_tab(st, tab, true, crate::state::AfterUnsaved::Close);
    pause(300).await;
    let parts: Vec<&str> = rows.split('|').collect();
    cx.record(
        "comment history: made, replied, state set, in that order with their authors",
        parts.len() == 3
            && parts[0].starts_with("Ada:")
            && parts[1].contains("They are right")
            && parts[2].starts_with("Ben:"),
        format!("rows {rows:?}"),
    );
}
