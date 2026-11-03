//! Interaction step: page_label_ranges — in a document numbered i, ii, 1, 2
//! the range "ii-1" of Rotate Pages turns the second and third pages, "1"
//! in the page box goes to the page labelled 1; with Use Page Labels off,
//! "1-2" is the first two pages again.

use super::*;

/// Choose a range in the open page dialog and press OK.
async fn rotate_range(range: &str) {
    let _ = eval(
        "[...document.querySelectorAll('.dialog .radio-row label')].find(l => l.textContent.includes('1-3, 7'))?.querySelector('input')?.click();",
    )
    .await;
    pause(250).await;
    let _ = eval(&format!(
        r#"const i = document.querySelector('.dialog input[placeholder="1-3, 7"]');
if (i) {{ i.value = {range:?}; i.dispatchEvent(new Event('input', {{ bubbles: true }})); }}"#
    ))
    .await;
    pause(200).await;
    let _ = eval("document.querySelector('.dialog-foot .tbtn.primary')?.click();").await;
    pause(500).await;
}

/// Which pages are turned sideways.
fn sideways(st: AppState) -> Vec<usize> {
    st.read_active(|t| {
        t.doc
            .page_sizes()
            .iter()
            .enumerate()
            .filter(|(_, s)| s.width > s.height)
            .map(|(i, _)| i)
            .collect()
    })
    .unwrap_or_default()
}

pub(super) async fn run(cx: &mut Ctx<'_>) {
    let st = cx.st;
    let made = folitecta_core::Document::new_blank(4, 595.0, 842.0).and_then(|mut d| {
        d.set_page_labels(&[(0, "r", "", 1), (2, "D", "", 1)])?;
        d.clear_history();
        Ok(d)
    });
    let Ok(doc) = made else {
        cx.record(
            "page label ranges",
            false,
            "the document could not be made".into(),
        );
        return;
    };
    let was_on = crate::commands::option_on(st, "pages.useLabels");
    st.set_option("pages.useLabels", true);
    crate::files::open_document_tab(st, doc);
    let tab = st.read_active(|t| t.id).unwrap_or(0);
    pause(500).await;

    crate::commands::dispatch(st, "document.rotatePages");
    pause(400).await;
    rotate_range("ii-1").await;
    let by_label = sideways(st);

    // "1" in the page box is the page labelled 1, the third.
    let _ = eval(
        "const i = document.querySelector('.page-input'); if (i) { i.focus(); i.value = '1'; i.dispatchEvent(new Event('input', { bubbles: true })); i.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true })); }",
    )
    .await;
    pause(400).await;
    let page = st.current_page();

    crate::commands::dispatch(st, "undo");
    pause(300).await;
    st.set_option("pages.useLabels", false);
    crate::commands::dispatch(st, "document.rotatePages");
    pause(400).await;
    rotate_range("1-2").await;
    let by_number = sideways(st);

    st.set_option("pages.useLabels", was_on);
    crate::files::close_tab(st, tab, true, crate::state::AfterUnsaved::Close);
    pause(300).await;
    cx.record(
        "page ranges and the page box read page labels (ii-1 is the second and third page), numbers again with Use Page Labels off",
        by_label == [1, 2] && page == 2 && by_number == [0, 1],
        format!("by label {by_label:?}, page box went to {page}, by number {by_number:?}"),
    );
}
