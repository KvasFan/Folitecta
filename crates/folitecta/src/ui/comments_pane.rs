//! Comments pane: threaded comments with review states, grouping, search and filters.

use std::collections::HashSet;

use dioxus::prelude::*;
use folitecta_core::CommentInfo;

use crate::commands::option_on;
use crate::commands::review::{current_group, set_state, update_filter};
use crate::i18n::{annot_name, t, tf};
use crate::icons::Icon;
use crate::state::{AppState, Dialog};
use crate::util::color_hex;

/// A comment and its replies.
type Thread = (CommentInfo, Vec<CommentInfo>);

/// Display options of the comment items.
#[derive(Debug, Clone, PartialEq)]
pub struct ItemOpts {
    authors: bool,
    dates: bool,
    colors: bool,
    subjects: bool,
    states: bool,
    text: bool,
    placeholder: bool,
    compact: bool,
    uniform: bool,
    icons: bool,
    wrap: bool,
    replies_open: bool,
}

/// Icon of a comment type.
fn type_icon(subtype: &str) -> &'static str {
    match subtype {
        "Text" => "note",
        "FreeText" => "text",
        "Line" => "line",
        "Square" => "rect",
        "Circle" => "ellipse",
        "Polygon" | "PolyLine" | "Ink" => "pencil",
        "Highlight" => "highlight",
        "Underline" | "StrikeOut" | "Squiggly" => "text",
        "Stamp" => "tag",
        "Caret" => "insert",
        "FileAttachment" => "attachment",
        "Sound" => "media",
        "Redact" => "redact",
        _ => "comment",
    }
}

fn state_label(lang: crate::i18n::Lang, state: &str) -> String {
    t(lang, &format!("review.{}", state.to_lowercase()))
}

/// Group heading of a comment for the chosen grouping.
fn group_key(lang: crate::i18n::Lang, group: &str, c: &CommentInfo) -> String {
    let no_state = t(lang, "review.no_state");
    let state = if c.review.is_empty() {
        no_state.clone()
    } else {
        state_label(lang, &c.review)
    };
    let author = if c.author.is_empty() {
        "—".to_string()
    } else {
        c.author.clone()
    };
    match group {
        "page" => format!("{} {}", t(lang, "status.page"), c.page + 1),
        "author" => author,
        "type" => annot_name(lang, &c.subtype),
        "color" => c.color.map(color_hex).unwrap_or_else(|| "—".into()),
        "subject" => {
            if c.subject.is_empty() {
                "—".into()
            } else {
                c.subject.clone()
            }
        }
        "state" => state,
        "crtDate" => c.created.split(' ').next().unwrap_or("—").to_string(),
        "modDate" => c.modified.split(' ').next().unwrap_or("—").to_string(),
        "stateAuthor" | "stateAndAuthor" => format!("{state} · {author}"),
        _ => String::new(),
    }
}

#[component]
pub fn CommentsPane() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let mut query = use_signal(String::new);
    let mut toggled_groups: Signal<HashSet<String>> = use_signal(HashSet::new);
    let Some((infos, selected, filter_active)) = st.read_active(|t| {
        (
            t.doc.comment_infos(),
            t.selected_annot,
            !t.comment_filter.is_empty(),
        )
    }) else {
        return rsx! {};
    };
    let filter = st.read_active(|t| t.comment_filter.clone()).unwrap_or_default();
    let opts = ItemOpts {
        authors: option_on(st, "commentsView.show.authors"),
        dates: option_on(st, "commentsView.show.dates"),
        colors: option_on(st, "commentsView.show.colors"),
        subjects: option_on(st, "commentsView.show.subjects"),
        states: option_on(st, "commentsView.show.states"),
        text: option_on(st, "commentsView.show.text"),
        placeholder: option_on(st, "commentsView.show.noTextPlaceholder"),
        compact: option_on(st, "commentsView.show.compactItems"),
        uniform: option_on(st, "commentsView.show.uniformText"),
        icons: option_on(st, "commentsView.show.dynamicIcons"),
        wrap: option_on(st, "commentsView.show.wrapLongTitles"),
        replies_open: st.ui.read().value("comments.collapsed") != Some("all"),
    };
    let collapsed_all = !opts.replies_open;
    let show_hidden = option_on(st, "commentsView.showHiddenComments");
    let show_layer_hidden = option_on(st, "commentsView.showCommentsFromHiddenLayers");
    let group = current_group(st);
    let q = query().trim().to_lowercase();
    let replies_of = |c: &CommentInfo| -> Vec<CommentInfo> {
        infos
            .iter()
            .filter(|r| r.reply_to == Some(c.id) && !r.is_state)
            .cloned()
            .collect()
    };
    let total = infos
        .iter()
        .filter(|c| c.reply_to.is_none() && !c.is_state)
        .count();
    let mut shown: Vec<Thread> = infos
        .iter()
        .filter(|c| c.reply_to.is_none() && !c.is_state)
        .filter(|c| show_hidden || !c.hidden)
        .filter(|c| show_layer_hidden || !c.in_hidden_layer)
        .filter(|c| filter.shows(c))
        .map(|c| (c.clone(), replies_of(c)))
        .filter(|(c, replies)| {
            q.is_empty()
                || c.contents.to_lowercase().contains(&q)
                || c.author.to_lowercase().contains(&q)
                || c.subject.to_lowercase().contains(&q)
                || annot_name(lang, &c.subtype).to_lowercase().contains(&q)
                || replies
                    .iter()
                    .any(|r| r.contents.to_lowercase().contains(&q) || r.author.to_lowercase().contains(&q))
        })
        .collect();
    if group != "none" && group != "page" {
        // Stable sort keeps the page order inside each group.
        shown.sort_by_key(|(c, _)| group_key(lang, &group, c).to_lowercase());
    }
    let mut groups: Vec<(String, Vec<Thread>)> = Vec::new();
    for item in shown {
        let key = group_key(lang, &group, &item.0);
        match groups.last_mut() {
            Some((k, items)) if *k == key => items.push(item),
            _ => groups.push((key, vec![item])),
        }
    }
    let count: usize = groups.iter().map(|(_, items)| items.len()).sum();
    let count_label = if count == total {
        tf(lang, "comments.count", &[("count", &count.to_string())])
    } else {
        tf(
            lang,
            "comments.count_of",
            &[("count", &count.to_string()), ("total", &total.to_string())],
        )
    };
    let collapse_icon = if collapsed_all { "down" } else { "up" };
    let collapse_title = if collapsed_all {
        t(lang, "cmd.commentsView.expandAll")
    } else {
        t(lang, "cmd.commentsView.collapseAll")
    };
    rsx! {
        div { class: "pane-toolbar comments-toolbar",
            input { class: "pane-search", placeholder: "{t(lang, \"comments.search\")}", value: "{query}",
                oninput: move |e| query.set(e.value()),
                onkeydown: move |e| e.stop_propagation(),
            }
            button { class: if filter_active { "ibtn active" } else { "ibtn" }, title: "{t(lang, \"cmd.comments.show.status\")}",
                onclick: move |_| st.set_dialog(Dialog::CommentFilter), Icon { name: "select" } }
            button { class: "ibtn", title: "{collapse_title}",
                onclick: move |_| { toggled_groups.set(HashSet::new()); st.set_value("comments.collapsed", if collapsed_all { "" } else { "all" }); },
                Icon { name: collapse_icon } }
            button { class: "ibtn", title: "{t(lang, \"cmd.commentsView.options\")}", onclick: move |_| st.set_dialog(Dialog::CommentsViewOptions), Icon { name: "settings" } }
        }
        div { class: "comments-count",
            span { class: "muted", "{count_label}" }
            if filter_active {
                button { class: "chip-btn", onclick: move |_| update_filter(st, |f| *f = Default::default()), "{t(lang, \"filter.show_all\")} ×" }
            }
        }
        if total == 0 {
            div { class: "pane-empty", "{t(lang, \"label.none\")}" }
        }
        for (key, items) in groups {
            {
                let is_toggled = toggled_groups.read().contains(&key);
                let collapsed = !key.is_empty() && (collapsed_all != is_toggled);
                let key2 = key.clone();
                let n = items.len();
                rsx! {
                    div { class: "comment-group", key: "g-{key}",
                        if !key.is_empty() {
                            div { class: "group-head",
                                onclick: move |_| toggled_groups.with_mut(|s| { if !s.remove(&key2) { s.insert(key2.clone()); } }),
                                span { class: "twisty", if collapsed { "▸" } else { "▾" } }
                                span { class: "ellipsis", "{key}" }
                                span { class: "count", "{n}" }
                            }
                        }
                        if !collapsed {
                            for (c, replies) in items {
                                CommentItem { key: "{c.page}-{c.index}", selected: selected == Some((c.page, c.index)), c, replies, opts: opts.clone() }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn CommentItem(c: CommentInfo, replies: Vec<CommentInfo>, selected: bool, opts: ItemOpts) -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let mut replying = use_signal(|| false);
    let mut reply_text = use_signal(String::new);
    let mut replies_toggled = use_signal(|| false);
    let (page, index) = (c.page, c.index);
    let show_replies = opts.replies_open != replies_toggled();
    let mut class = String::from("list-item comment-item");
    if selected {
        class.push_str(" selected");
    }
    if opts.wrap {
        class.push_str(" wrap");
    }
    if opts.compact {
        class.push_str(" compact");
    }
    if opts.uniform {
        class.push_str(" uniform");
    }
    let color = c.color.map(color_hex).unwrap_or_else(|| "#888".into());
    let popup_open = c.popup.map(|p| p.1).unwrap_or(false);
    let contents = c.contents.clone();
    let marked = c.marked;
    let n_replies = replies.len();
    let replies_label = if n_replies == 1 {
        t(lang, "comments.one_reply")
    } else {
        tf(lang, "comments.replies", &[("count", &n_replies.to_string())])
    };
    let send = move || {
        let text = reply_text.peek().trim().to_string();
        if text.is_empty() {
            return;
        }
        let author = st.ui.peek().author.clone();
        let r = st.with_active(|t| t.doc.add_reply(page, index, &text, &author));
        st.with_active(|t| t.selected_annot = Some((page, index)));
        match r {
            Some(Err(e)) => st.error(e),
            _ => {
                reply_text.set(String::new());
                replying.set(false);
                replies_toggled.set(!opts.replies_open);
                crate::files::set_window_title(st);
            }
        }
    };
    let mut send_key = send;
    let mut send_click = send;
    rsx! {
        div { class: "{class}",
            onclick: move |_| { st.with_active(|t| t.selected_annot = Some((page, index))); st.go_to_page(page); },
            div { class: "title",
                if opts.colors { span { class: "swatch", style: "background:{color}" } }
                if opts.icons { Icon { name: type_icon(&c.subtype) } }
                span { class: "ellipsis", "{annot_name(lang, &c.subtype)}" }
                span { class: "muted", "· {t(lang, \"status.page\")} {page + 1}" }
                if opts.states && !c.review.is_empty() {
                    span { class: "chip state {c.review.to_lowercase()}", "{state_label(lang, &c.review)}" }
                }
                if marked { span { class: "chip ok", title: "{t(lang, \"review.marked\")}", "✓" } }
                if c.locked { Icon { name: "lock" } }
                if c.hidden { span { class: "chip", "{t(lang, \"label.hidden\")}" } }
            }
            if opts.authors || opts.dates || (opts.subjects && !c.subject.is_empty()) {
                div { class: "meta",
                    if opts.authors { span { "{c.author}" } }
                    if opts.dates { span { "{c.modified}" } }
                    if opts.subjects && !c.subject.is_empty() { span { class: "subject", "{c.subject}" } }
                }
            }
            if opts.text {
                if !c.contents.is_empty() {
                    div { class: "body", "{c.contents}" }
                } else if opts.placeholder {
                    div { class: "body placeholder", "{t(lang, \"comments.no_text\")}" }
                }
            }
            if n_replies > 0 {
                div { class: "replies-toggle", onclick: move |e| { e.stop_propagation(); replies_toggled.set(!replies_toggled()); },
                    span { class: "twisty", if show_replies { "▾" } else { "▸" } }
                    "{replies_label}"
                }
                if show_replies {
                    div { class: "replies",
                        for r in replies.iter() {
                            div { class: "reply", key: "{r.index}",
                                div { class: "meta", span { "{r.author}" } if opts.dates { span { "{r.modified}" } } }
                                div { class: "body", "{r.contents}" }
                            }
                        }
                    }
                }
            }
            if selected {
                div { class: "row comment-actions", onclick: move |e| e.stop_propagation(),
                    button { class: "tbtn", title: "{t(lang, \"cmd.annot.addReply\")}", onclick: move |_| replying.set(!replying()), Icon { name: "comment" } "{t(lang, \"popup.reply\")}" }
                    button { class: "ibtn", title: "{t(lang, \"cmd.annot.status\")}", onclick: move |_| st.set_dialog(Dialog::ReviewStatus { page, index }), Icon { name: "check" } }
                    label { class: "check mini", title: "{t(lang, \"review.marked\")}",
                        input { r#type: "checkbox", checked: marked, onchange: move |_| set_state(st, page, index, "Marked", if marked { "Unmarked" } else { "Marked" }) }
                        span { "✓" }
                    }
                    button { class: if popup_open { "ibtn active" } else { "ibtn" }, title: "{t(lang, \"comments.popup\")}",
                        onclick: move |_| {
                            let r = st.with_active(|t| t.doc.set_popup_open(page, index, !popup_open));
                            st.with_active(|t| t.selected_annot = Some((page, index)));
                            if let Some(Err(e)) = r { st.error(e); }
                        },
                        Icon { name: "note" }
                    }
                    button { class: "ibtn", title: "{t(lang, \"label.edit\")}", onclick: move |_| st.set_dialog(Dialog::AnnotationText { page, index, contents: contents.clone() }), Icon { name: "pencil" } }
                    button { class: "ibtn", title: "{t(lang, \"cmd.annot.reviewHistory\")}", onclick: move |_| st.set_dialog(Dialog::ReviewHistory { page, index }), Icon { name: "history" } }
                    button { class: "ibtn danger", title: "{t(lang, \"label.delete\")}",
                        onclick: move |_| { if let Some(Err(e)) = st.with_active(|t| t.doc.delete_annotation(page, index)) { st.error(e); } },
                        Icon { name: "delete" }
                    }
                }
                if replying() {
                    div { class: "reply-box", onclick: move |e| e.stop_propagation(),
                        textarea { value: "{reply_text}", placeholder: "{t(lang, \"popup.reply_placeholder\")}", autofocus: true,
                            oninput: move |e| reply_text.set(e.value()),
                            onkeydown: move |e| {
                                e.stop_propagation();
                                if e.key() == Key::Enter && e.modifiers().ctrl() { send_key(); }
                            },
                        }
                        div { class: "row",
                            button { class: "tbtn", onclick: move |_| { replying.set(false); reply_text.set(String::new()); }, "{t(lang, \"dialog.cancel\")}" }
                            button { class: "tbtn primary", onclick: move |_| send_click(), "{t(lang, \"popup.reply\")}" }
                        }
                    }
                }
            }
        }
    }
}
