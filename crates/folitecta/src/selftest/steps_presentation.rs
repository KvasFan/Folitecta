//! Interaction step: presentation — three pages presented on the whole
//! screen: the right arrow brings the second with its fade, which goes on by
//! itself to the third after the second it sets; Escape ends on that page.

use crate::ui::features::presentation::SHOWING;

use super::*;

async fn shown() -> (Option<i64>, String) {
    let page =
        eval("const p = document.querySelector('.presentation'); return p ? Number(p.dataset.page) : null;")
            .await
            .ok()
            .and_then(|v| v.as_i64());
    let class = eval("return document.querySelector('.presentation-page')?.className ?? '';")
        .await
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default();
    (page, class)
}

async fn key(name: &str) {
    let _ = eval(&format!(
        "document.querySelector('.presentation')?.dispatchEvent(new KeyboardEvent('keydown', {{ key: {}, bubbles: true }}));",
        serde_json::to_string(name).unwrap_or_default()
    ))
    .await;
    pause(300).await;
}

pub(super) async fn run(cx: &mut Ctx<'_>) {
    let st = cx.st;
    let made = folitecta_core::Document::new_blank(3, 595.0, 842.0).and_then(|mut d| {
        d.set_page_transition(&[1], Some("Fade"), 0.4, Some(1.0))?;
        d.clear_history();
        Ok(d)
    });
    let Ok(doc) = made else {
        cx.record("presentation", false, "the document could not be made".into());
        return;
    };
    crate::files::open_document_tab(st, doc);
    let tab = st.read_active(|t| t.id).unwrap_or(0);
    pause(400).await;

    crate::commands::dispatch(st, "view.presentation");
    pause(600).await;
    let first = shown().await;
    let full = *st.fullscreen.peek();
    key("ArrowRight").await;
    let second = shown().await;
    let _ = capture_window(&cx.dir.join("interaction-presentation.png"));
    // The second page shows for a second, then the third comes by itself.
    pause(1400).await;
    let third = shown().await;
    key("Escape").await;
    pause(900).await;
    let ended = SHOWING.peek().is_none() && shown().await.0.is_none();
    let page = st.current_page();
    let full_after = *st.fullscreen.peek();
    crate::files::close_tab(st, tab, true, crate::state::AfterUnsaved::Close);
    pause(300).await;
    cx.record(
        "presentation: the next page with its fade, the third by itself after its time, Escape ends on it",
        first.0 == Some(0)
            && full
            && second.0 == Some(1)
            && second.1.contains("present-fade")
            && third.0 == Some(2)
            && ended
            && page == 2
            && !full_after,
        format!("first {first:?}, full {full}, second {second:?}, third {third:?}, ended {ended}, page {page}, full after {full_after}"),
    );
}
