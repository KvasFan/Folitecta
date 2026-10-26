//! Interaction step: Preflight PDF/A — a document set in the embedded font
//! with a script and device colours checked (the broken rules counted, all
//! fixable), then converted: a new tab with the copy, declared PDF/A-2b,
//! the original unchanged.

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
    let made = folitecta_core::Document::from_markdown(
        "# Отчёт\n\nТекст документа.\n",
        folitecta_core::TextLayout::default(),
        "Отчёт",
    )
    .and_then(|mut d| {
        d.set_document_scripts(&[("start".into(), "app.alert('hi');".into())])?;
        d.clear_history();
        Ok(d)
    });
    let Ok(doc) = made else {
        cx.record("preflight", false, "the document could not be made".into());
        return;
    };
    let original = doc.bytes();
    crate::files::open_document_tab(st, doc);
    let tab = st.read_active(|t| t.id).unwrap_or(0);
    let tabs = st.docs.read().len();
    pause(500).await;
    crate::commands::dispatch(st, "document.preflight");
    let mut summary = String::new();
    for _ in 0..40 {
        pause(150).await;
        summary =
            text("return document.querySelector('.dialog .preflight-summary')?.textContent || '';").await;
        if !summary.is_empty() {
            break;
        }
    }
    let broken = text(
        "return String(document.querySelectorAll('.dialog .preflight-rules .audit-row.broken').length);",
    )
    .await;
    let _ = eval("document.querySelector('.dialog-foot .tbtn.primary')?.click();").await;
    let mut claimed = false;
    for _ in 0..40 {
        pause(150).await;
        if st.docs.read().len() > tabs {
            claimed = st
                .read_active(|t| {
                    t.doc
                        .xmp_packet()
                        .unwrap_or_default()
                        .contains("<pdfaid:part>2</pdfaid:part>")
                })
                .unwrap_or(false);
            break;
        }
    }
    let copy = st.read_active(|t| t.id).unwrap_or(0);
    let unchanged = st
        .docs
        .read()
        .iter()
        .find(|t| t.id == tab)
        .is_some_and(|t| t.doc.bytes() == original);
    for id in [copy, tab] {
        crate::files::close_tab(st, id, true, crate::state::AfterUnsaved::Close);
        pause(200).await;
    }
    let broken_n: usize = broken.parse().unwrap_or(0);
    cx.record(
        "preflight PDF/A: the broken rules listed, a converted copy opened in a new tab declared PDF/A-2b, the original unchanged",
        !summary.is_empty() && broken_n >= 2 && claimed && unchanged && copy != tab,
        format!("summary {summary:?}, broken {broken}, claimed {claimed}, unchanged {unchanged}"),
    );
}
