//! Interaction step: replace_fonts — Replace Fonts on a document whose fonts
//! are not embedded: Helvetica chosen, FreeSans bold asked; the font listed
//! afterwards is FreeSansBold, embedded, and the text is the same. Show
//! Problem Details finds nothing wrong in it; Document Actions › Clear
//! removes the scripts of its events after asking.

use super::*;

pub(super) async fn run(cx: &mut Ctx<'_>) {
    let st = cx.st;
    let Ok(doc) =
        folitecta_core::Document::from_bytes(folitecta_core::testdocs::embeddable_fonts_doc(), None, None)
    else {
        cx.record("replace fonts", false, "the document could not be made".into());
        return;
    };
    let text = doc.page_text(0);
    crate::files::open_document_tab(st, doc);
    let tab = st.read_active(|t| t.id).unwrap_or(0);
    pause(500).await;
    crate::commands::dispatch(st, "document.replaceFonts");
    pause(500).await;
    let open = matches!((st.dialog)(), crate::state::Dialog::ReplaceFonts);
    let set = |selector: &str, value: &str| {
        format!(
            "const c = document.querySelector('.dialog {selector}'); if (c) {{ c.value = '{value}'; c.dispatchEvent(new Event('change', {{ bubbles: true }})); }}"
        )
    };
    let _ = eval(&set(".replace-fonts-font", "Helvetica")).await;
    pause(150).await;
    let _ = eval(&set(".replace-fonts-family", "sans")).await;
    pause(150).await;
    let _ = eval("document.querySelector('.dialog .replace-fonts-bold')?.click();").await;
    pause(150).await;
    let _ = eval("document.querySelector('.dialog-foot .tbtn.primary')?.click();").await;
    pause(700).await;
    let (fonts, same) = st
        .read_active(|t| {
            let names: Vec<(String, bool)> = t
                .doc
                .used_fonts()
                .into_iter()
                .map(|f| (f.name, f.embedded))
                .collect();
            (names, t.doc.page_text(0) == text)
        })
        .unwrap_or_default();
    let replaced =
        fonts.iter().any(|(n, e)| n == "FreeSansBold" && *e) && !fonts.iter().any(|(n, _)| n == "Helvetica");
    // Show Problem Details: a sound file has none.
    crate::commands::dispatch(st, "document.showProblems");
    pause(500).await;
    let problems_open = matches!((st.dialog)(), crate::state::Dialog::Problems);
    let none = eval("return !!document.querySelector('.dialog .problems-none');")
        .await
        .ok()
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    st.set_dialog(crate::state::Dialog::None);
    cx.record(
        "Show Problem Details opens and finds nothing wrong in a sound file",
        problems_open && none,
        format!("dialog {problems_open}, none said {none}"),
    );
    // Document Actions › Clear: asked first, then the scripts of the events go.
    st.with_active(|t| {
        let _ = t.doc.set_document_action("WP", "app.alert('printing');");
        let _ = t.doc.set_document_action("WC", "app.alert('closing');");
    });
    crate::commands::dispatch(st, "docActions.clear");
    pause(400).await;
    let asked = matches!((st.dialog)(), crate::state::Dialog::Confirm { .. });
    let _ = eval("document.querySelector('.dialog-foot .tbtn.danger')?.click();").await;
    pause(500).await;
    let left = st.read_active(|t| t.doc.document_actions().len()).unwrap_or(99);
    cx.record(
        "Document Actions › Clear asks, then removes the scripts of the document's events",
        asked && left == 0,
        format!("asked {asked}, left {left}"),
    );
    crate::files::close_tab(st, tab, true, crate::state::AfterUnsaved::Close);
    pause(300).await;
    cx.record(
        "replace fonts: Helvetica drawn with FreeSans bold, embedded, the text unchanged",
        open && replaced && same,
        format!("dialog {open}, fonts {fonts:?}, same text {same}"),
    );
}
