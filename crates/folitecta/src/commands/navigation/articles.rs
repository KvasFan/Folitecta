//! The Article tool (FOL-L022): the rectangles drawn with it are the parts
//! of a new article thread, in the order drawn, or more parts of an article
//! chosen in the Articles pane; Enter asks the title of a new one and makes
//! it (or adds the parts), Escape drops them.

use dioxus::prelude::*;

use crate::commands::{handlers, select_tool};
use crate::i18n::{t, tf};
use crate::state::{AppState, Dialog, ToastKind, Tool};

/// The parts of the article being drawn: page and area (PDF user space).
pub static DRAFT: GlobalSignal<Vec<(usize, [f32; 4])>> = Signal::global(Vec::new);

/// The article the parts drawn go to, when they do not make a new one.
pub static TARGET: GlobalSignal<Option<usize>> = Signal::global(|| None);

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

/// Enter: ask the title of the article drawn, or add the parts drawn to the
/// article chosen (nothing when no part is drawn).
pub fn finish(st: AppState) -> bool {
    if DRAFT.peek().is_empty() {
        return false;
    }
    let Some(index) = *TARGET.peek() else {
        st.set_dialog(Dialog::ArticleTitle);
        return true;
    };
    let parts = std::mem::take(&mut *DRAFT.write());
    *TARGET.write() = None;
    match st.with_active(|t| t.doc.add_article_parts(index, &parts)) {
        Some(Ok(())) => {
            crate::files::set_window_title(st);
            st.set_tool(Tool::Hand);
            st.toast(
                ToastKind::Ok,
                tf(
                    st.lang(),
                    "toast.article_parts_added",
                    &[("count", &parts.len().to_string())],
                ),
                "",
            );
        }
        Some(Err(e)) => st.error(e.to_string()),
        None => {}
    }
    true
}

/// Draw more parts of article `index` (from the Articles pane).
pub fn add_parts(st: AppState, index: usize) {
    discard();
    *TARGET.write() = Some(index);
    select_tool(st, Tool::Article);
    st.toast(ToastKind::Info, t(st.lang(), "toast.article_add_parts"), "");
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
    if TARGET.peek().is_some() {
        *TARGET.write() = None;
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
