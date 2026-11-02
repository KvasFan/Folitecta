//! The Fonts pane (FOL-L004): the fonts the document draws with, what kind
//! they are, whether the file holds them and where they appear. A click goes
//! to the first page of a font.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use dioxus::prelude::*;
use folitecta_core::UsedFont;

use crate::i18n::{Lang, t, tf};
use crate::icons::Icon;
use crate::state::AppState;

/// `pages` (zero-based, ascending) as numbers, with ranges.
fn page_ranges(pages: &[usize]) -> String {
    let mut out: Vec<String> = Vec::new();
    let mut i = 0;
    while i < pages.len() {
        let start = pages[i];
        let mut end = start;
        while i + 1 < pages.len() && pages[i + 1] == end + 1 {
            i += 1;
            end = pages[i];
        }
        out.push(if start == end {
            (start + 1).to_string()
        } else {
            format!("{}\u{2013}{}", start + 1, end + 1)
        });
        i += 1;
    }
    out.join(", ")
}

/// The fonts of the active document, found again when it changes.
type Made = Option<((u64, u64), Arc<Vec<UsedFont>>)>;

fn state_of(lang: Lang, f: &UsedFont) -> String {
    let key = if f.subset && f.embedded {
        "pane.fonts.subset"
    } else if f.embedded {
        "pane.fonts.embedded"
    } else if f.standard {
        "pane.fonts.standard"
    } else {
        "pane.fonts.not_embedded"
    };
    t(lang, key)
}

#[component]
pub fn FontsPane() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let made: Rc<RefCell<Made>> = use_hook(|| Rc::new(RefCell::new(None)));
    let Some(key) = st.read_active(|t| (t.id, t.doc.revision())) else {
        return rsx! {};
    };
    let fonts = {
        let mut slot = made.borrow_mut();
        match slot.as_ref() {
            Some((k, f)) if *k == key => f.clone(),
            _ => {
                let f = Arc::new(st.peek_active(|t| t.doc.used_fonts()).unwrap_or_default());
                *slot = Some((key, f.clone()));
                f
            }
        }
    };
    let missing = fonts.iter().filter(|f| !f.embedded && !f.standard).count();
    rsx! {
        if fonts.is_empty() { div { class: "pane-empty", "{t(lang, \"label.none\")}" } }
        if missing > 0 {
            div { class: "callout warn fonts-missing", "{tf(lang, \"pane.fonts.missing\", &[(\"count\", &missing.to_string())])}" }
        }
        for (i, f) in fonts.iter().enumerate() {
            {
                let page = f.pages.first().copied();
                let kind = t(lang, &format!("pane.fonts.kind.{}", f.kind.id()));
                let state = state_of(lang, f);
                let pages = tf(lang, "pane.fonts.pages", &[("pages", &page_ranges(&f.pages))]);
                let class = if f.embedded || f.standard { "list-item font-row" } else { "list-item font-row missing" };
                rsx! {
                    div { class: "{class}", key: "{i}",
                        onclick: move |_| { if let Some(p) = page { st.go_to_page(p); } },
                        div { class: "title", Icon { name: "font" } "{f.name}" }
                        div { class: "body wrap", "{kind} · {state}" }
                        div { class: "body ellipsis", title: "{pages}", "{pages}" }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::page_ranges;

    #[test]
    fn pages_read_as_ranges() {
        assert_eq!(page_ranges(&[]), "");
        assert_eq!(page_ranges(&[0]), "1");
        assert_eq!(page_ranges(&[0, 1, 2, 5, 7, 8]), "1\u{2013}3, 6, 8\u{2013}9");
    }
}
