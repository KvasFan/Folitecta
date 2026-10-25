//! The reflow view (FOL-L042): the text of the document as it is read, its
//! paragraphs and headings filling the width of the window at a size of
//! one's choosing, each page's part marked with its number. The page at the
//! top of the view becomes the current page, a page gone to is scrolled to,
//! and the paragraph read aloud is marked and kept in view.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

use dioxus::prelude::*;
use folitecta_core::ReadBlock;

use crate::i18n::{t, tf};
use crate::state::AppState;

/// Text sizes of the view (pixels), smallest to largest.
pub const SIZES: [u32; 9] = [13, 15, 17, 19, 21, 24, 28, 32, 38];

/// The text size chosen.
pub fn text_size(st: AppState) -> u32 {
    st.value("reflow.size").trim().parse().unwrap_or(19)
}

/// A larger (`up`) or smaller text.
pub fn change_size(st: AppState, up: bool) {
    let now = text_size(st);
    let next = if up {
        SIZES
            .iter()
            .copied()
            .find(|s| *s > now)
            .unwrap_or(SIZES[SIZES.len() - 1])
    } else {
        SIZES.iter().rev().copied().find(|s| *s < now).unwrap_or(SIZES[0])
    };
    st.set_value("reflow.size", next.to_string());
}

/// Bring the part of page `page` to the top of the view.
pub fn show_page(page: usize) {
    let js = format!(
        "const e = document.getElementById('rp-{page}'); if (e) e.scrollIntoView({{ block: 'start' }});"
    );
    spawn(async move {
        let _ = document::eval(&js).await;
    });
}

/// Report the page at the top of the view while it scrolls (at most every
/// 120 ms), as long as the view lives.
const FOLLOW_JS: &str = r#"
for (let i = 0; i < 50 && !document.getElementById('reflow'); i++) await new Promise(r => setTimeout(r, 40));
const view = document.getElementById('reflow');
if (!view) return;
let waiting = null;
const report = () => {
  const pages = view.getElementsByClassName('reflow-page');
  const top = view.getBoundingClientRect().top + 40;
  let lo = 0, hi = pages.length;
  while (lo < hi) { const m = (lo + hi) >> 1; if (pages[m].getBoundingClientRect().bottom <= top) lo = m + 1; else hi = m; }
  if (lo < pages.length) dioxus.send(+pages[lo].dataset.page);
};
view.addEventListener('scroll', () => {
  if (waiting) return;
  waiting = setTimeout(() => { waiting = null; report(); }, 120);
}, { passive: true });
await new Promise(() => {});
"#;

/// The blocks of the active document, made again when it changes.
type Made = Option<((u64, u64), Arc<Vec<ReadBlock>>)>;

#[component]
pub fn ReflowView() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let made: Rc<RefCell<Made>> = use_hook(|| Rc::new(RefCell::new(None)));
    // The page the view shows at its top, as last reported or scrolled to.
    let shown: Rc<Cell<Option<usize>>> = use_hook(|| Rc::new(Cell::new(None)));
    let Some((key, page)) = st.read_active(|t| ((t.id, t.doc.revision()), t.page)) else {
        return rsx! {};
    };
    let blocks = {
        let mut slot = made.borrow_mut();
        match slot.as_ref() {
            Some((k, b)) if *k == key => b.clone(),
            _ => {
                let b = Arc::new(
                    st.peek_active(|t| t.doc.reading_blocks(0..t.doc.page_count()))
                        .unwrap_or_default(),
                );
                *slot = Some((key, b.clone()));
                b
            }
        }
    };
    {
        let shown = shown.clone();
        use_future(move || {
            let shown = shown.clone();
            async move {
                let mut ev = document::eval(FOLLOW_JS);
                while let Ok(p) = ev.recv::<usize>().await {
                    shown.set(Some(p));
                    if st.peek_active(|t| t.page) != Some(p) {
                        st.with_active(|t| t.page = p);
                    }
                }
            }
        });
    }
    // A page gone to from elsewhere (and the current page at first) is
    // brought into view.
    if shown.get() != Some(page) {
        shown.set(Some(page));
        show_page(page);
    }
    let reading = crate::commands::read_aloud::current_block(st);
    // The paragraph read aloud stays in view.
    {
        let last: Rc<Cell<Option<(usize, usize)>>> = use_hook(|| Rc::new(Cell::new(None)));
        if reading != last.get() {
            last.set(reading);
            if let Some((p, s)) = reading {
                let js = format!(
                    "const e = document.getElementById('rb-{p}-{s}'); if (e) e.scrollIntoView({{ block: 'center', behavior: 'smooth' }});"
                );
                spawn(async move {
                    let _ = document::eval(&js).await;
                });
            }
        }
    }
    let size = text_size(st);
    // The blocks page by page (every page has its part, even without text).
    let pages = st.peek_active(|t| t.doc.page_count()).unwrap_or(0);
    let mut by_page: Vec<Vec<&ReadBlock>> = vec![Vec::new(); pages];
    for b in blocks.iter() {
        if let Some(v) = by_page.get_mut(b.page) {
            v.push(b);
        }
    }
    let no_text = blocks.is_empty();
    rsx! {
        div { class: "reflow-wrap",
            div { class: "reflow-bar",
                span { class: "muted", "{t(lang, \"reflow.title\")}" }
                span { class: "grow" }
                button { class: "tbtn reflow-smaller", title: "{t(lang, \"reflow.smaller\")}", onclick: move |_| change_size(st, false), "A\u{2212}" }
                span { class: "muted reflow-size", "{size}px" }
                button { class: "tbtn reflow-larger", title: "{t(lang, \"reflow.larger\")}", onclick: move |_| change_size(st, true), "A+" }
                button { class: "tbtn reflow-close", onclick: move |_| crate::commands::dispatch(st, "view.reflow"), "{t(lang, \"reflow.pages\")}" }
            }
            div { id: "reflow", class: "reflow", style: "font-size:{size}px",
                article { class: "reflow-text",
                    if no_text {
                        p { class: "hint", "{t(lang, \"reflow.no_text\")}" }
                    }
                    for (p, list) in by_page.into_iter().enumerate() {
                        div { key: "{p}", id: "rp-{p}", class: "reflow-page", "data-page": "{p}",
                            div { class: "reflow-page-no", "{tf(lang, \"reflow.page\", &[(\"page\", &(p + 1).to_string())])}" }
                            for b in list {
                                {
                                    let id = format!("rb-{}-{}", b.page, b.start);
                                    let class = if reading == Some((b.page, b.start)) { "reading" } else { "" };
                                    match b.level {
                                        1 => rsx! { h1 { key: "{id}", id: "{id}", class: "{class}", "{b.text}" } },
                                        2 => rsx! { h2 { key: "{id}", id: "{id}", class: "{class}", "{b.text}" } },
                                        3 => rsx! { h3 { key: "{id}", id: "{id}", class: "{class}", "{b.text}" } },
                                        4 => rsx! { h4 { key: "{id}", id: "{id}", class: "{class}", "{b.text}" } },
                                        _ => rsx! { p { key: "{id}", id: "{id}", class: "{class}", "{b.text}" } },
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
