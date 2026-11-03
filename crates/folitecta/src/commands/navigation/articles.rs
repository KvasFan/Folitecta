//! The Article tool (FOL-L022): the rectangles drawn with it are the parts
//! of a new article thread, in the order drawn; Enter asks its title and
//! makes it, Escape drops it.

use dioxus::prelude::*;

use crate::commands::{handlers, select_tool};
use crate::i18n::{t, tf};
use crate::state::{AppState, Dialog, ToastKind, Tool};

/// The parts of the article being drawn: page and area (PDF user space).
pub static DRAFT: GlobalSignal<Vec<(usize, [f32; 4])>> = Signal::global(Vec::new);

/// A rectangle drawn with the tool: the next part of the article.
pub fn add_part(st: AppState, page: usize, rect: [f32; 4]) {
    let n = {
        let mut d = DRAFT.write();
        d.push((page, rect));
        d.len()
    };
    st.toast(
        ToastKind::Info,
        tf(st.lang(), "toast.article_part", &[("count", &n.to_string())]),
        t(st.lang(), "toast.article_part_detail"),
    );
}

/// Enter: ask the title of the article drawn (nothing when no part is).
pub fn finish(st: AppState) -> bool {
    if DRAFT.peek().is_empty() {
        return false;
    }
    st.set_dialog(Dialog::ArticleTitle);
    true
}

/// Make the article drawn, titled `title`.
pub fn make(st: AppState, title: &str) {
    let parts = std::mem::take(&mut *DRAFT.write());
    match st.with_active(|t| t.doc.add_article(title, &parts)) {
        Some(Ok(())) => {
            crate::files::set_window_title(st);
            st.set_dialog(Dialog::None);
            st.set_tool(Tool::Hand);
            st.toast(
                ToastKind::Ok,
                tf(
                    st.lang(),
                    "toast.article_made",
                    &[("count", &parts.len().to_string())],
                ),
                "",
            );
        }
        Some(Err(e)) => st.error(e.to_string()),
        None => {}
    }
}

/// Drop the parts drawn (Escape, another tool or document).
pub fn discard() {
    if !DRAFT.peek().is_empty() {
        DRAFT.write().clear();
    }
}

fn start(st: AppState) {
    discard();
    select_tool(st, Tool::Article);
    if st.has_doc() {
        st.toast(ToastKind::Info, t(st.lang(), "toast.article_tool"), "");
    }
}

handlers! {
    "tool.article" => start,
}
