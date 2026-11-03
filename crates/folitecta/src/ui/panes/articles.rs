//! The Articles pane (FOL-L022): the article threads of the document, each
//! with its beads in reading order; a click shows the area of a bead.

use dioxus::prelude::*;
use folitecta_core::DestView;

use crate::i18n::{t, tf};
use crate::icons::Icon;
use crate::state::AppState;

#[component]
pub fn ArticlesPane() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let articles = st.read_active(|t| t.doc.articles()).unwrap_or_default();
    rsx! {
        div { class: "articles-list",
            if articles.is_empty() {
                div { class: "pane-empty", "{t(lang, \"pane.articles.none\")}" }
            }
            for (k, a) in articles.into_iter().enumerate() {
                {
                    let title = if a.title.trim().is_empty() {
                        tf(lang, "pane.articles.untitled", &[("n", &(k + 1).to_string())])
                    } else {
                        a.title.clone()
                    };
                    let beads = tf(lang, "pane.articles.beads", &[("count", &a.beads.len().to_string())]);
                    rsx! {
                        div { key: "{k}", class: "list-item article",
                            div { class: "title", Icon { name: "book" } "{title}" }
                            div { class: "meta", "{beads}" }
                            div { class: "article-beads",
                                for (j, (page, rect)) in a.beads.into_iter().enumerate() {
                                    button { key: "{j}", class: "linkish article-bead",
                                        onclick: move |_| st.go_to_dest(page, Some(DestView::Rect(rect))),
                                        "{j + 1}. {t(lang, \"status.page\")} {page + 1}"
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
