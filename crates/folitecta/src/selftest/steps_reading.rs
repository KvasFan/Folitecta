//! Interaction step: reading (FOL-L042). A report of several pages shown in
//! the reflow view: its headings and paragraphs page by page, a larger text,
//! the page at the top followed both ways. Then read aloud with a stand-in
//! voice that speaks nothing: the paragraphs in order from the current page,
//! the one read marked in the reflow view and on its page, a pause that
//! holds the reading, the next paragraph, stop.

use super::*;
use crate::commands::read_aloud::READ;

fn report() -> Option<folitecta_core::Document> {
    let p = "The quick brown fox jumps over the lazy dog and runs into the woods again. ".repeat(16);
    let md = format!(
        "# Annual report\n\n{p}\n\n{p}\n\n## Results\n\n{p}\n\n{p}\n\n{p}\n\n{p}\n\n{p}\n\n## Outlook\n\n{p}\n\n{p}\n\n{p}\n\n{p}\n\n{p}\n\n{p}\n"
    );
    let mut doc = folitecta_core::Document::from_markdown(&md, Default::default(), "").ok()?;
    doc.clear_history();
    Some(doc)
}

async fn number(js: &str) -> i64 {
    eval(js).await.ok().and_then(|v| v.as_i64()).unwrap_or(-1)
}

async fn text(js: &str) -> String {
    eval(js)
        .await
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default()
}

pub(super) async fn run(cx: &mut Ctx<'_>) {
    let st = cx.st;
    let Some(doc) = report() else {
        cx.record("reading", false, "the document could not be made".into());
        return;
    };
    crate::files::open_document_tab(st, doc);
    let tab = st.read_active(|t| t.id).unwrap_or(0);
    pause(600).await;
    let pages = st.read_active(|t| t.doc.page_count()).unwrap_or(0) as i64;

    // The reflow view.
    crate::commands::dispatch(st, "view.reflow");
    pause(700).await;
    let shown_pages = number("return document.querySelectorAll('#reflow .reflow-page').length;").await;
    let heading = text("return document.querySelector('#reflow h1')?.textContent || '';").await;
    let h2 = number("return document.querySelectorAll('#reflow h2').length;").await;
    let size_before = text("return document.getElementById('reflow')?.style.fontSize || '';").await;
    let _ = eval("document.querySelector('.reflow-larger')?.click();").await;
    pause(300).await;
    let size_after = text("return document.getElementById('reflow')?.style.fontSize || '';").await;
    // Scrolled to the last page: it becomes the current page.
    let _ = eval(&format!(
        "document.getElementById('rp-{}')?.scrollIntoView({{ block: 'start' }});",
        pages - 1
    ))
    .await;
    pause(700).await;
    let followed = st.current_page() as i64 == pages - 1;
    // A page gone to is brought to the top.
    crate::commands::dispatch(st, "view.goto.firstPage");
    pause(600).await;
    let back = number("const v = document.getElementById('reflow'); const p = document.getElementById('rp-0'); if (!v || !p) return -1; return Math.round(Math.abs(p.getBoundingClientRect().top - v.getBoundingClientRect().top));").await;
    let _ = capture_window(&cx.dir.join("interaction-reflow.png"));
    cx.record(
        "the reflow view shows the text page by page, larger on request, and follows the current page both ways",
        shown_pages == pages
            && heading == "Annual report"
            && h2 == 2
            && size_before != size_after
            && followed
            && (0..40).contains(&back),
        format!(
            "pages {shown_pages} of {pages}, h1 {heading:?}, h2 {h2}, size {size_before} -> {size_after}, followed {followed}, first page at {back}"
        ),
    );

    // Read aloud, with a stand-in voice.
    crate::speech::set_stand_in(true);
    crate::commands::dispatch(st, "view.readAloud");
    pause(400).await;
    let bar = number("return document.querySelectorAll('.read-bar').length;").await == 1;
    let _ = eval("document.querySelector('.read-play')?.click();").await;
    pause(250).await;
    let marked = number("return document.querySelectorAll('#reflow .reading').length;").await;
    // Paused: the reading holds.
    let _ = eval("document.querySelector('.read-play')?.click();").await;
    pause(100).await;
    let held_at = READ.peek().current;
    pause(400).await;
    let held = READ.peek().paused && READ.peek().current == held_at;
    let _ = eval("document.querySelector('.read-play')?.click();").await;
    pause(400).await;
    let went_on = READ.peek().current > held_at;
    let said = crate::speech::stood_in();
    let in_order = said.first().map(String::as_str) == Some("Annual report")
        && said.get(1).is_some_and(|s| s.starts_with("The quick brown fox"));
    // Held again and back to the pages: the paragraph read is marked on its page.
    let _ = eval("document.querySelector('.read-play')?.click();").await;
    pause(100).await;
    crate::commands::dispatch(st, "view.reflow");
    pause(700).await;
    let on_page = number("return document.querySelectorAll('#canvas .reading-mark').length;").await;
    // The next paragraph, by the bar.
    let before_next = READ.peek().current;
    let _ = eval("document.querySelector('.read-next')?.click();").await;
    pause(30).await;
    let next = READ.peek().current == before_next + 1;
    let _ = capture_window(&cx.dir.join("interaction-read-aloud.png"));
    let _ = eval("document.querySelector('.read-stop')?.click();").await;
    pause(300).await;
    let stopped = !READ.peek().running && st.read_active(|t| t.reading.is_none()).unwrap_or(false);
    crate::commands::dispatch(st, "view.readAloud");
    crate::speech::set_stand_in(false);
    pause(200).await;
    crate::files::close_tab(st, tab, true, crate::state::AfterUnsaved::Close);
    pause(300).await;
    cx.record(
        "read aloud: the paragraphs in order, the one read marked in the reflow view and on its page, a pause that holds, stop",
        bar && marked == 1 && held && went_on && in_order && on_page > 0 && next && stopped,
        format!(
            "bar {bar}, marked {marked}, held {held} at {held_at}, went on {went_on}, said {:?}, on page {on_page}, next {next}, stopped {stopped}",
            said.iter().take(3).map(|s| s.chars().take(30).collect::<String>()).collect::<Vec<_>>()
        ),
    );
}
