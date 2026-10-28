//! Interaction step: relations — Delete Pages on a page that bookmarks, a
//! named destination and a link lead to asks first and names them; after OK
//! they are gone with the page, and Undo brings them back. Insert Pages from
//! a file names the form fields and destinations it will rename before OK;
//! after OK the renamed fields are in the form.

use super::*;

async fn text(js: &str) -> String {
    eval(js)
        .await
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default()
}

/// Wait until `js` gives some text (about 6 seconds at most).
async fn wait_text(js: &str) -> String {
    for _ in 0..40 {
        let t = text(js).await;
        if !t.trim().is_empty() {
            return t;
        }
        pause(150).await;
    }
    String::new()
}

async fn click_ok() {
    // OK of a dialog that changes the document, or of one that destroys.
    let _ = eval("document.querySelector('.dialog-foot .tbtn.primary, .dialog-foot .tbtn.danger')?.click();")
        .await;
    pause(500).await;
}

pub(super) async fn run(cx: &mut Ctx<'_>) {
    let st = cx.st;
    let Ok(doc) = folitecta_core::Document::from_bytes(folitecta_core::testdocs::relations_doc(), None, None)
    else {
        cx.record("relations", false, "the document could not be made".into());
        return;
    };
    crate::files::open_document_tab(st, doc);
    let tab = st.read_active(|t| t.id).unwrap_or(0);
    pause(800).await;

    // Delete Pages on page 3: "Part B", `third` and the link of page 1 lead there.
    st.with_active(|t| t.selected_pages = vec![2]);
    crate::commands::dispatch(st, "document.deletePages");
    pause(400).await;
    let first = text("return document.querySelector('.dialog .dialog-head')?.textContent || '';").await;
    click_ok().await;
    let asked =
        wait_text("return [...document.querySelectorAll('.dialog p')].map(p => p.textContent).join(' ');")
            .await;
    let still_four = st.page_count() == 4;
    click_ok().await;
    let titles = |st: AppState| {
        st.read_active(|t| t.doc.outline_flat().into_iter().map(|b| b.1).collect::<Vec<_>>())
            .unwrap_or_default()
    };
    let after = titles(st);
    let dests = st.read_active(|t| t.doc.named_destinations().len()).unwrap_or(0);
    let deleted = st.page_count() == 3 && !after.iter().any(|t| t == "Part B") && dests == 3;
    crate::commands::dispatch(st, "undo");
    pause(400).await;
    let back = st.page_count() == 4 && titles(st).len() == 5;
    let named = asked.contains('1') && asked.chars().filter(|c| *c == ',').count() >= 2;
    cx.record(
        "relations: deleting a page others lead to names what goes, asks, takes it away; undo brings it back",
        still_four && named && deleted && back,
        format!(
            "dialog {first:?}, asked {asked:?}, before OK four pages {still_four}, after {after:?} with {dests} destinations, back {back}"
        ),
    );

    // Insert Pages from a file: the same document again.
    let path = cx.dir.join("relations-source.pdf");
    let written = std::fs::write(&path, folitecta_core::testdocs::relations_doc()).is_ok();
    st.set_dialog(crate::state::Dialog::MergeFile(path.clone()));
    let preview = wait_text(
        "const c = document.querySelector('.dialog .merge-arrival'); return c ? c.textContent : '';",
    )
    .await;
    let before = st.read_active(|t| t.doc.form_fields().len()).unwrap_or(0);
    click_ok().await;
    pause(400).await;
    let names: Vec<String> = st
        .read_active(|t| t.doc.form_fields().into_iter().map(|f| f.name).collect())
        .unwrap_or_default();
    let renamed = names.iter().any(|n| n == "name_2") && names.len() == 2 * before;
    cx.record(
        "relations: inserting pages names the fields and destinations it renames, then brings them into the form",
        written && preview.contains("name_2") && preview.contains("first_2") && renamed,
        format!("preview {preview:?}, fields {before} -> {names:?}"),
    );
    crate::files::close_tab(st, tab, true, crate::state::AfterUnsaved::Close);
    pause(300).await;
}
