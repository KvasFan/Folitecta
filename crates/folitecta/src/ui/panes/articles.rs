//! The Articles pane (FOL-L022): the article threads of the document, each
//! with its beads in reading order; a click shows the area of a bead.

use dioxus::prelude::*;
use folitecta_core::DestView;

use crate::i18n::{t, tf};
use crate::icons::Icon;
use crate::state::{AppState, Dialog};

/// Run an edit of the articles and say what went wrong.
fn edit(st: AppState, f: impl FnOnce(&mut folitecta_core::Document) -> folitecta_core::Result<()>) {
    match st.with_active(|t| f(&mut t.doc)) {
        Some(Ok(())) => crate::files::set_window_title(st),
        Some(Err(e)) => st.error(e.to_string()),
        None => {}
    }
}

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
                    let own = a.title.clone();
                    let count = a.beads.len();
                    rsx! {
                        div { key: "{k}", class: "list-item article",
                            div { class: "title", Icon { name: "book" } span { class: "grow", "{title}" }
                                button { class: "ibtn article-add-parts", title: "{t(lang, \"pane.articles.add_parts\")}",
                                    onclick: move |_| crate::commands::articles::add_parts(st, k),
                                    Icon { name: "insert" } }
                                button { class: "ibtn article-rename", title: "{t(lang, \"pane.articles.rename\")}",
                                    onclick: move |_| st.set_dialog(Dialog::ArticleRename { index: k, title: own.clone() }),
                                    Icon { name: "pencil" } }
                                button { class: "ibtn article-delete", title: "{t(lang, \"pane.articles.delete\")}",
                                    onclick: move |_| edit(st, |d| d.delete_article(k)),
                                    Icon { name: "delete" } }
                            }
                            div { class: "meta", "{beads}" }
                            div { class: "article-beads",
                                for (j, (page, rect)) in a.beads.iter().copied().enumerate() {
                                    span { key: "{j}", class: "article-part-row",
                                        button { class: "linkish article-bead",
                                            onclick: move |_| st.go_to_dest(page, Some(DestView::Rect(rect))),
                                            "{j + 1}. {t(lang, \"status.page\")} {page + 1}"
                                        }
                                        button { class: "ibtn small article-part-earlier", title: "{t(lang, \"pane.articles.earlier\")}", disabled: j == 0,
                                            onclick: move |_| edit(st, |d| d.move_article_part(k, j, j.saturating_sub(1))),
                                            "‹" }
                                        button { class: "ibtn small article-part-later", title: "{t(lang, \"pane.articles.later\")}", disabled: j + 1 >= count,
                                            onclick: move |_| edit(st, |d| d.move_article_part(k, j, j + 1)),
                                            "›" }
                                        button { class: "ibtn small article-part-delete", title: "{t(lang, \"pane.articles.delete_part\")}",
                                            onclick: move |_| edit(st, |d| d.delete_article_part(k, j)),
                                            "×" }
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
