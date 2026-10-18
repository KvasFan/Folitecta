//! Interaction step 53: spell checking — Spell Check and Replace finds the
//! words of the page text and of a comment that the English and Italian
//! dictionaries do not know; one is changed to the word typed, one passed
//! over, one added to the user's words and the comment corrected with Enter;
//! then the Edit Text tool marks the words left on the page.

use crate::commands::spelling::{self as sp, SUGGESTIONS};

use super::*;

/// The words of the list of the check, once made (at most 10 s).
async fn words(st: AppState) -> Vec<String> {
    for _ in 0..100 {
        let list = st
            .read_active(|t| {
                let run = t.spell.as_ref()?;
                run.made_for?;
                Some(run.list.iter().map(|m| m.word.clone()).collect::<Vec<_>>())
            })
            .flatten();
        if let Some(list) = list {
            return list;
        }
        pause(100).await;
    }
    Vec::new()
}

async fn click(selector: &str) {
    let _ = document::eval(&format!(
        "document.querySelector({})?.click();",
        serde_json::to_string(selector).unwrap_or_default()
    ))
    .await;
    pause(400).await;
}

async fn count(selector: &str) -> u64 {
    document::eval(&format!(
        "return document.querySelectorAll({}).length;",
        serde_json::to_string(selector).unwrap_or_default()
    ))
    .await
    .ok()
    .and_then(|v| v.as_u64())
    .unwrap_or(0)
}

/// Type `text` in "Change to".
async fn type_change_to(text: &str, enter: bool) {
    let _ = document::eval(&format!(
        "const i = document.querySelector('.dialog .spell-to'); if (i) {{ i.value = {}; i.dispatchEvent(new Event('input', {{ bubbles: true }})); }}",
        serde_json::to_string(text).unwrap_or_default()
    ))
    .await;
    pause(200).await;
    if enter {
        let _ = document::eval(
            "document.querySelector('.dialog .spell-to')?.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));",
        )
        .await;
        pause(500).await;
    }
}

fn current_word(st: AppState) -> Option<String> {
    st.read_active(|t| sp::current(t).map(|m| m.word.clone()))
        .flatten()
}

pub(super) async fn run(cx: &mut Ctx<'_>) {
    let st = cx.st;
    // 53. A document of its own, checked in English and Italian.
    let made = folitecta_core::Document::from_text(
        "Teh quick brown fox jumps over the lazzy dog.\nUn esempio sbagliatto di testo italiano.",
        Default::default(),
    )
    .and_then(|mut d| {
        d.add_note(
            0,
            400.0,
            700.0,
            "A note with a mistaek",
            "Reviewer",
            [1.0, 0.85, 0.2],
        )?;
        d.clear_history();
        Ok(d)
    });
    let Ok(doc) = made else {
        cx.record("spell check", false, "the document could not be made".into());
        return;
    };
    let saved_langs = st.value("spell.languages");
    st.set_value("spell.languages", "en_US,it_IT");
    crate::files::open_document_tab(st, doc);
    let tab = st.read_active(|t| t.id).unwrap_or(0);
    pause(600).await;
    crate::commands::dispatch(st, "document.spellCheckAndRepl");
    let found = words(st).await;
    // The suggestions of the first word, found in the background.
    let mut suggested = 0;
    for _ in 0..50 {
        pause(100).await;
        suggested = count(".dialog .spell-sugg").await;
        if suggested > 0 {
            break;
        }
    }
    let marked = count("#canvas .spell-current").await + count("#canvas .spell-word").await;
    let _ = capture_window(&cx.dir.join("interaction-spell-check.png"));

    // Changed to the word typed, passed over, added to the user's words.
    type_change_to("The", false).await;
    click(".dialog .spell-change").await;
    let changed = st
        .read_active(|t| t.doc.page_text(0).starts_with("The quick"))
        .unwrap_or(false);
    let after_change = current_word(st);
    click(".dialog .spell-ignore").await;
    let after_ignore = current_word(st);
    click(".dialog .spell-add").await;
    let after_add = current_word(st);
    // The comment's word: marked on the page, corrected with Enter.
    let comment_marked = count("#canvas .spell-current").await;
    type_change_to("mistake", true).await;
    let note = st
        .read_active(|t| {
            t.doc
                .annotations(0)
                .ok()?
                .into_iter()
                .find(|a| a.subtype == "Text")
                .map(|a| a.contents)
        })
        .flatten()
        .unwrap_or_default();
    let done = count(".dialog .spell-status.ok").await;
    let _ = capture_window(&cx.dir.join("interaction-spell-done.png"));
    st.set_dialog(Dialog::None);

    // The Edit Text tool marks the words left: the one passed over (only
    // passed over by the dialog) and the one taken out of the user's words.
    sp::remove_word(st, "sbagliatto");
    st.set_tool(Tool::EditText);
    let mut edit_marks = 0;
    for _ in 0..20 {
        pause(150).await;
        edit_marks = count("#canvas .spell-word").await;
        if edit_marks >= 2 {
            break;
        }
    }
    let _ = capture_window(&cx.dir.join("interaction-spell-edit.png"));
    st.set_tool(Tool::Hand);
    *SUGGESTIONS.write() = None;
    crate::files::close_tab(st, tab, true, crate::state::AfterUnsaved::Close);
    pause(300).await;
    if saved_langs.is_empty() {
        st.ui_mut().values.remove("spell.languages");
    } else {
        st.set_value("spell.languages", saved_langs);
    }

    cx.record(
        "spell check: words of the page text and of a comment found in two languages, changed, passed over, added, corrected with Enter; Edit Text marks the words left",
        found == ["Teh", "lazzy", "sbagliatto", "mistaek"]
            && suggested > 0
            && marked >= 3
            && changed
            && after_change.as_deref() == Some("lazzy")
            && after_ignore.as_deref() == Some("sbagliatto")
            && after_add.as_deref() == Some("mistaek")
            && comment_marked == 1
            && note == "A note with a mistake"
            && done == 1
            && edit_marks >= 2,
        format!(
            "found {found:?}, suggestions {suggested}, marked {marked}, changed {changed}, then {after_change:?} / {after_ignore:?} / {after_add:?}, comment marked {comment_marked}, note {note:?}, done {done}, edit marks {edit_marks}"
        ),
    );
}
