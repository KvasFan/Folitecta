//! Interaction step: doubtful_words — on a page whose text recognition read
//! "nunber", "2O26" and "~" with doubt, the search with Only Words Read with
//! Doubt finds "nunber" but not "Invoice"; the spell check lists the three,
//! says they were read with doubt, and Change and Ignore check them.

use folitecta_core::{DOUBTFUL_BELOW, OcrLine, OcrWord, PageOcr};

use super::*;

fn page() -> Option<folitecta_core::Document> {
    let mut doc = folitecta_core::Document::new_blank(1, 595.0, 842.0).ok()?;
    let word = |text: &str, x: f32, confidence: f32| OcrWord {
        text: text.into(),
        quad: [[x, 100.0], [x + 60.0, 100.0], [x + 60.0, 124.0], [x, 124.0]],
        confidence,
    };
    let line = OcrLine {
        text: "Invoice nunber 2O26 ~".into(),
        quad: [[100.0, 100.0], [400.0, 100.0], [400.0, 124.0], [100.0, 124.0]],
        words: vec![
            word("Invoice", 100.0, 0.98),
            word("nunber", 180.0, 0.55),
            word("2O26", 260.0, 0.62),
            word("~", 340.0, 0.3),
        ],
        confidence: 0.7,
    };
    doc.add_ocr_text(&[PageOcr {
        page: 0,
        scale: 2.0,
        lines: vec![line],
        doubt_below: DOUBTFUL_BELOW,
    }])
    .ok()?;
    doc.clear_history();
    Some(doc)
}

fn doubtful(st: AppState) -> Vec<String> {
    st.read_active(|t| t.doc.doubtful_words(0).into_iter().map(|w| w.text).collect())
        .unwrap_or_default()
}

async fn listed(st: AppState) -> Vec<(String, bool)> {
    for _ in 0..100 {
        let list = st
            .read_active(|t| {
                let run = t.spell.as_ref()?;
                run.made_for?;
                Some(
                    run.list
                        .iter()
                        .map(|m| (m.word.clone(), m.doubtful))
                        .collect::<Vec<_>>(),
                )
            })
            .flatten();
        if let Some(list) = list {
            return list;
        }
        pause(100).await;
    }
    Vec::new()
}

pub(super) async fn run(cx: &mut Ctx<'_>) {
    let st = cx.st;
    let Some(doc) = page() else {
        cx.record(
            "words read with doubt",
            false,
            "the document could not be made".into(),
        );
        return;
    };
    let saved = (st.value("search.doubtful"), st.value("spell.languages"));
    crate::files::open_document_tab(st, doc);
    let tab = st.read_active(|t| t.id).unwrap_or(0);
    pause(500).await;

    // The search keeps the results on words read with doubt.
    st.set_value("search.doubtful", "1");
    let on_doubt = crate::ui::panes::refresh_hits(st, "nunber").map(|h| h.0);
    let off_doubt = crate::ui::panes::refresh_hits(st, "Invoice").map(|h| h.0);
    st.set_value("search.doubtful", "0");
    let all = crate::ui::panes::refresh_hits(st, "Invoice").map(|h| h.0);

    // The spell check lists them, and says so.
    st.set_value("spell.languages", "en_US");
    crate::commands::dispatch(st, "document.spellCheckAndRepl");
    let found = listed(st).await;
    pause(300).await;
    let said = eval("return document.querySelectorAll('.dialog .spell-doubtful').length;")
        .await
        .ok()
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let _ = eval(
        "const i = document.querySelector('.dialog .spell-to'); if (i) { i.value = 'number'; i.dispatchEvent(new Event('input', { bubbles: true })); }",
    )
    .await;
    pause(200).await;
    let _ = eval("document.querySelector('.dialog .spell-change')?.click();").await;
    pause(600).await;
    let after_change = doubtful(st);
    let _ = eval("document.querySelector('.dialog .spell-ignore')?.click();").await;
    pause(600).await;
    let after_ignore = doubtful(st);
    let text = st.read_active(|t| t.doc.page_text(0)).unwrap_or_default();
    st.set_dialog(Dialog::None);
    pause(200).await;

    st.set_value("search.doubtful", saved.0);
    st.set_value("spell.languages", saved.1);
    crate::files::close_tab(st, tab, true, crate::state::AfterUnsaved::Close);
    pause(300).await;
    let doubtful_listed = found
        .iter()
        .filter(|(_, d)| *d)
        .map(|(w, _)| w.as_str())
        .collect::<Vec<_>>();
    cx.record(
        "words read with doubt: found alone by the search, listed by the spell check and checked by Change and Ignore",
        on_doubt == Some(1)
            && off_doubt == Some(0)
            && all == Some(1)
            && doubtful_listed == ["nunber", "2O26", "~"]
            && said == 1
            && after_change == ["2O26", "~"]
            && after_ignore == ["~"]
            && text.contains("Invoice number 2O26"),
        format!(
            "search {on_doubt:?}/{off_doubt:?}/{all:?}, listed {found:?}, said {said}, after Change {after_change:?}, after Ignore {after_ignore:?}, text {text:?}"
        ),
    );
}
