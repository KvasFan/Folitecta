//! Interaction step: flatten_read — Flatten Comments with the notes of one
//! author left out flattens only the others'; with the reading bar open, a
//! double click on the second paragraph of a page reads from it.

use crate::state::Zoom;

use super::*;

pub(super) async fn run(cx: &mut Ctx<'_>) {
    let st = cx.st;
    // Flatten, Bruno's note left out.
    let made = folitecta_core::Document::new_blank(1, 400.0, 500.0).and_then(|mut d| {
        d.add_note(0, 50.0, 450.0, "First", "Anna", [1.0, 0.9, 0.2])?;
        d.add_note(0, 90.0, 450.0, "Second", "Bruno", [1.0, 0.9, 0.2])?;
        d.clear_history();
        Ok(d)
    });
    let Ok(doc) = made else {
        cx.record("flatten and read", false, "the document could not be made".into());
        return;
    };
    crate::files::open_document_tab(st, doc);
    let tab = st.read_active(|t| t.id).unwrap_or(0);
    pause(400).await;
    crate::commands::dispatch(st, "comments.flatten");
    pause(400).await;
    let _ = eval("document.querySelector('.dialog input[data-author=\"Bruno\"]')?.click();").await;
    pause(200).await;
    let _ = eval("document.querySelector('.dialog-foot .tbtn.primary')?.click();").await;
    pause(500).await;
    let left: Vec<String> = st
        .read_active(|t| {
            t.doc
                .annotations(0)
                .unwrap_or_default()
                .into_iter()
                .filter(|a| a.subtype == "Text")
                .map(|a| a.author)
                .collect()
        })
        .unwrap_or_default();
    crate::files::close_tab(st, tab, true, crate::state::AfterUnsaved::Close);
    pause(300).await;

    // Read from the second paragraph, double-clicked on the page.
    let Ok(doc) = folitecta_core::Document::from_text(
        "The first paragraph of the page, read only when asked.\n\nThe second paragraph, where the reading begins.",
        Default::default(),
    ) else {
        cx.record(
            "flatten and read",
            false,
            "the text document could not be made".into(),
        );
        return;
    };
    let second = doc
        .reading_blocks(0..1)
        .into_iter()
        .find(|b| b.text.starts_with("The second"))
        .map(|b| b.start);
    let at = doc
        .search("second paragraph", Default::default())
        .first()
        .map(|h| h.rects[0]);
    crate::files::open_document_tab(st, doc);
    let tab = st.read_active(|t| t.id).unwrap_or(0);
    st.with_active(|t| t.zoom = Zoom::Fixed(1.0));
    pause(600).await;
    crate::speech::set_stand_in(true);
    crate::commands::dispatch(st, "view.readAloud");
    pause(400).await;
    if let Some(r) = at {
        let (x, y) = ((r[0] + r[2]) / 2.0, (r[1] + r[3]) / 2.0);
        let _ = eval(&format!(
            "const el = document.querySelector('#pg-0 .page-inner'); if (el) {{ const b = el.getBoundingClientRect(); el.dispatchEvent(new MouseEvent('dblclick', {{ bubbles: true, clientX: b.left + {x}, clientY: b.top + {y} }})); }}"
        ))
        .await;
    }
    // While it reads, the word read is marked on the page.
    let mut word_marked = false;
    for _ in 0..80 {
        pause(10).await;
        if st.read_active(|t| t.reading_word.is_some()).unwrap_or(false) {
            word_marked = true;
            break;
        }
    }
    pause(200).await;
    let said = crate::speech::stood_in();
    let _ = eval("document.querySelector('.read-stop')?.click();").await;
    pause(300).await;
    crate::commands::dispatch(st, "view.readAloud");
    crate::speech::set_stand_in(false);
    pause(200).await;
    crate::files::close_tab(st, tab, true, crate::state::AfterUnsaved::Close);
    pause(300).await;
    cx.record(
        "Flatten Comments with an author left out keeps that author's notes; a double click on a paragraph reads from it, the word read marked",
        // The stand-in voice says the paragraphs at once: the first said is
        // where the reading began.
        left == ["Bruno"]
            && second.is_some()
            && said.first().is_some_and(|s| s.starts_with("The second"))
            && word_marked,
        format!("notes left {left:?}, second paragraph at {second:?}, said {said:?}, word marked {word_marked}"),
    );
}
