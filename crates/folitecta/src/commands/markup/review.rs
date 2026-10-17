//! Comment review: replies, review states, check marks, flags, pop-up notes,
//! comment filters and the options of the comments pane.

use dioxus::prelude::ReadableExt;
use folitecta_core::CommentInfo;
use folitecta_core::comment_review::{FLAG_LOCKED, FLAG_PRINT};

use crate::commands::{core_result, handlers, no_doc, option_on};
use crate::i18n::{t, tf};
use crate::state::{AppState, CommentFilter, Dialog, ToastKind};

/// Grouping choices of the comments pane (command suffix and preference value).
pub const GROUPS: &[&str] = &[
    "none",
    "page",
    "author",
    "type",
    "color",
    "subject",
    "state",
    "crtDate",
    "modDate",
    "stateAuthor",
    "stateAndAuthor",
];

/// The selected comment (not a link, field or popup) with its review data.
pub(crate) fn selected_comment(st: AppState) -> Option<CommentInfo> {
    st.read_active(|t| {
        let (p, i) = t.selected_annot?;
        t.doc.page_comment_infos(p).into_iter().find(|c| c.index == i)
    })
    .flatten()
}

fn with_selected(st: AppState, f: impl FnOnce(AppState, CommentInfo)) {
    if no_doc(st) {
        return;
    }
    match selected_comment(st) {
        Some(c) => f(st, c),
        None => st.toast(ToastKind::Info, t(st.lang(), "toast.select_comment_first"), ""),
    }
}

/// Run a change on the selected comment and keep it selected.
fn change(
    st: AppState,
    c: &CommentInfo,
    f: impl FnOnce(&mut folitecta_core::Document) -> folitecta_core::Result<()>,
) {
    let (page, index) = (c.page, c.index);
    let r = st.with_active(|t| f(&mut t.doc));
    st.with_active(|t| t.selected_annot = Some((page, index)));
    core_result(st, r);
}

/// Record a check mark or review state on a comment.
pub(crate) fn set_state(st: AppState, page: usize, index: usize, model: &str, state: &str) {
    let author = st.ui.peek().author.clone();
    let lang = st.lang();
    let state_name = t(lang, &format!("review.{}", state.to_lowercase()));
    let contents = tf(
        lang,
        "review.set_by",
        &[("state", &state_name), ("author", &author)],
    );
    let r = st.with_active(|t| {
        t.doc
            .set_review_state(page, index, model, state, &author, &contents)
    });
    st.with_active(|t| t.selected_annot = Some((page, index)));
    core_result(st, r);
}

fn toggle_checked(st: AppState) {
    with_selected(st, |st, c| {
        let state = if c.marked { "Unmarked" } else { "Marked" };
        set_state(st, c.page, c.index, "Marked", state);
    });
}

fn toggle_flag(st: AppState, flag: i64) {
    with_selected(st, |st, c| {
        let on = if flag == FLAG_LOCKED {
            !c.locked
        } else {
            !c.printable
        };
        change(st, &c, |d| d.set_annotation_flag(c.page, c.index, flag, on));
    });
}

fn popup(st: AppState, open: bool) {
    with_selected(st, |st, c| {
        change(st, &c, |d| d.set_popup_open(c.page, c.index, open))
    });
}

fn all_popups(st: AppState, open: Option<bool>, non_empty: bool) {
    if no_doc(st) {
        return;
    }
    let open = open.unwrap_or_else(|| {
        // Toggle: close when any note is open.
        !st.read_active(|t| {
            t.doc
                .comment_infos()
                .iter()
                .any(|c| c.popup.map(|p| p.1).unwrap_or(false))
        })
        .unwrap_or(false)
    });
    match st.with_active(|t| t.doc.set_all_popups_open(open, non_empty)) {
        Some(Ok(0)) => st.toast(ToastKind::Info, t(st.lang(), "toast.no_comments"), ""),
        Some(Ok(_)) => crate::files::set_window_title(st),
        Some(Err(e)) => st.error(e),
        None => {}
    }
}

fn copy_text(st: AppState) {
    with_selected(st, |st, c| {
        crate::clipboard::set_text(&c.contents);
        st.toast(ToastKind::Ok, t(st.lang(), "toast.copied"), "");
    });
}

fn flip_line(st: AppState) {
    with_selected(st, |st, c| {
        if c.subtype != "Line" {
            st.toast(ToastKind::Info, t(st.lang(), "toast.select_line_first"), "");
            return;
        }
        change(st, &c, |d| d.flip_line(c.page, c.index));
    });
}

/// Change the comment filter of the active document.
pub(crate) fn update_filter(st: AppState, f: impl FnOnce(&mut CommentFilter)) {
    if no_doc(st) {
        return;
    }
    st.with_active(|t| {
        let mut filter = t.comment_filter.clone();
        f(&mut filter);
        t.set_comment_filter(filter);
    });
}

pub(crate) fn set_group(st: AppState, group: &str) {
    st.set_value("comments.group", group);
}

/// Current grouping of the comments pane.
pub fn current_group(st: AppState) -> String {
    st.ui.read().value("comments.group").unwrap_or("none").to_string()
}

/// Check state of the review commands for menus and toolbar buttons.
pub fn review_state(st: AppState, id: &str) -> Option<bool> {
    if let Some(g) = id.strip_prefix("commentsView.groupBy.") {
        return GROUPS.contains(&g).then(|| current_group(st) == g);
    }
    match id {
        "comments.checked" => Some(selected_comment(st).map(|c| c.marked).unwrap_or(false)),
        "comments.locked" => Some(selected_comment(st).map(|c| c.locked).unwrap_or(false)),
        "comments.printable" => Some(selected_comment(st).map(|c| c.printable).unwrap_or(false)),
        "commentsView.hide.dates" => Some(!option_on(st, "commentsView.show.dates")),
        "comments.show.byAuthor.all" => Some(
            st.read_active(|t| t.comment_filter.hidden_authors.is_empty())
                .unwrap_or(true),
        ),
        "comments.show.byType.all" => Some(
            st.read_active(|t| t.comment_filter.hidden_types.is_empty())
                .unwrap_or(true),
        ),
        "comments.show.checked.all" => Some(
            st.read_active(|t| t.comment_filter.checked.is_none())
                .unwrap_or(true),
        ),
        _ => None,
    }
}

handlers! {
    "annot.addReply" => |st| with_selected(st, |st, c| st.set_dialog(Dialog::Reply { page: c.page, index: c.index })),
    "annot.status" => |st| with_selected(st, |st, c| st.set_dialog(Dialog::ReviewStatus { page: c.page, index: c.index })),
    "comments.show.status" => |st| { if !no_doc(st) { st.set_dialog(Dialog::CommentFilter) } },
    "annot.reviewHistory" => |st| with_selected(st, |st, c| st.set_dialog(Dialog::ReviewHistory { page: c.page, index: c.index })),
    "annot.copyText" => copy_text,
    "annot.line.flip" => flip_line,
    "annot.popup.open" => |st| popup(st, true),
    "annot.popup.hide" => |st| popup(st, false),
    "annot.popup.resetLocation" => |st| with_selected(st, |st, c| change(st, &c, |d| d.reset_popup_location(c.page, c.index))),
    "commentsView.popups.open" => |st| popup(st, true),
    "commentsView.popups.close" => |st| popup(st, false),
    "comments.show.popups.openAll" => |st| all_popups(st, Some(true), false),
    "comments.show.popups.closeAll" => |st| all_popups(st, Some(false), false),
    "comments.show.popups.openNonEmpty" => |st| all_popups(st, Some(true), true),
    "comments.show.popups.opencloseAll" => |st| all_popups(st, None, false),
    "comments.checked" => toggle_checked,
    "comments.locked" => |st| toggle_flag(st, FLAG_LOCKED),
    "comments.printable" => |st| toggle_flag(st, FLAG_PRINT),
    "comments.show.byAuthor.all" => |st| update_filter(st, |f| f.hidden_authors.clear()),
    "comments.show.byType.all" => |st| update_filter(st, |f| f.hidden_types.clear()),
    "comments.show.checked.all" => |st| update_filter(st, |f| f.checked = None),
    "commentsView.groupBy.author" => |st| set_group(st, "author"),
    "commentsView.groupBy.color" => |st| set_group(st, "color"),
    "commentsView.groupBy.crtDate" => |st| set_group(st, "crtDate"),
    "commentsView.groupBy.modDate" => |st| set_group(st, "modDate"),
    "commentsView.groupBy.page" => |st| set_group(st, "page"),
    "commentsView.groupBy.state" => |st| set_group(st, "state"),
    "commentsView.groupBy.stateAndAuthor" => |st| set_group(st, "stateAndAuthor"),
    "commentsView.groupBy.stateAuthor" => |st| set_group(st, "stateAuthor"),
    "commentsView.groupBy.subject" => |st| set_group(st, "subject"),
    "commentsView.groupBy.submenu" => |st| set_group(st, "none"),
    "commentsView.groupBy.type" => |st| set_group(st, "type"),
    "commentsView.collapseAll" => |st| st.set_value("comments.collapsed", "all"),
    "commentsView.expandAll" => |st| st.set_value("comments.collapsed", ""),
    "commentsView.hide.dates" => |st| { let on = option_on(st, "commentsView.show.dates"); st.set_option("commentsView.show.dates", !on); },
    "commentsView.options" => |st| st.set_dialog(Dialog::CommentsViewOptions),
    "commentsView.showMenu" => |st| st.set_dialog(Dialog::CommentsViewOptions),
}
