//! Comment dialogs: replies, review states, filters, view options, summaries, stamps, comment text and measurement calibration.

use super::*;

pub(super) fn review_name(lang: Lang, state: &str) -> String {
    t(lang, &format!("review.{}", state.to_lowercase()))
}

pub(super) fn group_name(lang: Lang, group: &str) -> String {
    t(lang, &format!("group.{group}"))
}

#[component]
pub(super) fn ReplyDialog(page: usize, index: usize) -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let text = use_signal(String::new);
    let comment = st
        .read_active(|t| {
            t.doc
                .page_comment_infos(page)
                .into_iter()
                .find(|c| c.index == index)
        })
        .flatten();
    let Some(c) = comment else {
        return rsx! {};
    };
    let replies: Vec<(String, String, String)> = st
        .read_active(|t| {
            t.doc
                .page_comment_infos(page)
                .into_iter()
                .filter(|x| x.reply_to == Some(c.id) && !x.is_state)
                .map(|x| (x.author, x.modified, x.contents))
                .collect()
        })
        .unwrap_or_default();
    rsx! {
        Frame { title: t(lang, "cmd.annot.addReply"), ok_label: Some(t(lang, "popup.reply")),
            on_ok: move |_| {
                let author = st.ui.peek().author.clone();
                let r = st.with_active(|t| t.doc.add_reply(page, index, &text(), &author));
                st.with_active(|t| t.selected_annot = Some((page, index)));
                match r {
                    Some(Err(e)) => st.error(e),
                    _ => {
                        files::set_window_title(st);
                        st.set_dialog(Dialog::None);
                    }
                }
            },
            div { class: "thread",
                div { class: "thread-item",
                    div { class: "meta", "{crate::i18n::annot_name(lang, &c.subtype)} · {c.author} · {c.modified}" }
                    div { class: "text", "{c.contents}" }
                }
                for (i, (author, date, body)) in replies.into_iter().enumerate() {
                    div { class: "thread-item reply", key: "{i}",
                        div { class: "meta", "{author} · {date}" }
                        div { class: "text", "{body}" }
                    }
                }
            }
            Area { label: t(lang, "popup.reply_placeholder"), value: text, autofocus: true }
        }
    }
}

#[component]
pub(super) fn ReviewStatusDialog(page: usize, index: usize) -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let comment = st
        .read_active(|t| {
            t.doc
                .page_comment_infos(page)
                .into_iter()
                .find(|c| c.index == index)
        })
        .flatten();
    let Some(c) = comment else {
        return rsx! {};
    };
    let initial = if c.review.is_empty() {
        "None".to_string()
    } else {
        c.review.clone()
    };
    let initial_marked = c.marked;
    let mut state = use_signal(|| initial.clone());
    let mut marked = use_signal(|| initial_marked);
    rsx! {
        Frame { title: t(lang, "cmd.annot.status"),
            on_ok: move |_| {
                st.set_dialog(Dialog::None);
                if state() != initial {
                    crate::commands::review::set_state(st, page, index, "Review", &state());
                }
                if marked() != initial_marked {
                    crate::commands::review::set_state(st, page, index, "Marked", if marked() { "Marked" } else { "Unmarked" });
                }
            },
            div { class: "field",
                label { "{t(lang, \"review.state\")}" }
                div { class: "radio-col",
                    for s in folitecta_core::REVIEW_STATES.iter().copied() {
                        label { key: "{s}", input { r#type: "radio", name: "review-state", checked: state() == s, onchange: move |_| state.set(s.to_string()) } "{review_name(lang, s)}" }
                    }
                }
            }
            label { class: "check", input { r#type: "checkbox", checked: marked(), onchange: move |_| marked.set(!marked()) } span { "{t(lang, \"review.marked\")}" } }
        }
    }
}

#[component]
pub(super) fn ReviewHistoryDialog(page: usize, index: usize) -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let history = st
        .read_active(|t| t.doc.review_history(page, index))
        .and_then(|r| r.ok())
        .unwrap_or_default();
    rsx! {
        Frame { title: t(lang, "cmd.annot.reviewHistory"),
            if history.is_empty() {
                div { class: "hint", "{t(lang, \"review.no_history\")}" }
            } else {
                table { class: "grid-table",
                    thead { tr { th { "{t(lang, \"dialog.summary.by_date\")}" } th { "{t(lang, \"dialog.summary.by_author\")}" } th { "{t(lang, \"review.state\")}" } } }
                    tbody {
                        for (i, h) in history.into_iter().enumerate() {
                            tr { key: "{i}",
                                td { "{h.date}" }
                                td { "{h.author}" }
                                td { "{review_name(lang, &h.state)}" }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
pub(super) fn CommentFilterDialog() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let Some((infos, current)) = st.read_active(|t| (t.doc.comment_infos(), t.comment_filter.clone())) else {
        return rsx! {};
    };
    let top: Vec<_> = infos
        .into_iter()
        .filter(|c| c.reply_to.is_none() && !c.is_state)
        .collect();
    let mut authors: Vec<String> = top.iter().map(|c| c.author.clone()).collect();
    authors.sort();
    authors.dedup();
    let mut types: Vec<String> = top.iter().map(|c| c.subtype.clone()).collect();
    types.sort();
    types.dedup();
    let mut filter = use_signal(|| current.clone());
    let status_value = filter()
        .status
        .clone()
        .map(|s| if s.is_empty() { "-".to_string() } else { s })
        .unwrap_or_default();
    let checked_value = match filter().checked {
        Some(true) => "yes",
        Some(false) => "no",
        None => "",
    };
    rsx! {
        Frame { title: t(lang, "filter.title"), wide: true,
            on_ok: move |_| {
                let f = filter();
                st.with_active(|t| t.set_comment_filter(f));
                st.set_dialog(Dialog::None);
            },
            div { class: "field-grid",
                div { class: "field",
                    label { "{t(lang, \"cmd.comments.show.byAuthor\")}" }
                    div { class: "check-list",
                        for a in authors {
                            {
                                let shown = !filter().hidden_authors.contains(&a);
                                let a2 = a.clone();
                                let label = if a.is_empty() { "—".to_string() } else { a.clone() };
                                rsx! { label { class: "check", key: "{a}", input { r#type: "checkbox", checked: shown, onchange: move |_| filter.with_mut(|f| { if !f.hidden_authors.remove(&a2) { f.hidden_authors.insert(a2.clone()); } }) } span { "{label}" } } }
                            }
                        }
                    }
                }
                div { class: "field",
                    label { "{t(lang, \"cmd.comments.show.byType\")}" }
                    div { class: "check-list",
                        for ty in types {
                            {
                                let shown = !filter().hidden_types.contains(&ty);
                                let ty2 = ty.clone();
                                rsx! { label { class: "check", key: "{ty}", input { r#type: "checkbox", checked: shown, onchange: move |_| filter.with_mut(|f| { if !f.hidden_types.remove(&ty2) { f.hidden_types.insert(ty2.clone()); } }) } span { "{crate::i18n::annot_name(lang, &ty)}" } } }
                            }
                        }
                    }
                }
                div { class: "field",
                    label { "{t(lang, \"review.state\")}" }
                    select { onchange: move |e| { let v = e.value(); filter.with_mut(|f| f.status = match v.as_str() { "" => None, "-" => Some(String::new()), s => Some(s.to_string()) }); },
                        option { value: "", selected: status_value.is_empty(), "{t(lang, \"filter.all\")}" }
                        option { value: "-", selected: status_value == "-", "{t(lang, \"review.none\")}" }
                        for s in folitecta_core::REVIEW_STATES.iter().copied().filter(|s| *s != "None") {
                            option { key: "{s}", value: "{s}", selected: status_value == s, "{review_name(lang, s)}" }
                        }
                    }
                }
                div { class: "field",
                    label { "{t(lang, \"cmd.comments.show.checked\")}" }
                    select { onchange: move |e| { let v = e.value(); filter.with_mut(|f| f.checked = match v.as_str() { "yes" => Some(true), "no" => Some(false), _ => None }); },
                        option { value: "", selected: checked_value.is_empty(), "{t(lang, \"filter.all\")}" }
                        option { value: "yes", selected: checked_value == "yes", "{t(lang, \"review.marked\")}" }
                        option { value: "no", selected: checked_value == "no", "{t(lang, \"review.unmarked\")}" }
                    }
                }
            }
            div { class: "row",
                button { class: "tbtn", onclick: move |_| filter.set(crate::state::CommentFilter::default()), "{t(lang, \"filter.show_all\")}" }
            }
        }
    }
}

#[component]
pub(super) fn CommentsViewOptionsDialog() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let group = crate::commands::review::current_group(st);
    let toggles = [
        "commentsView.show.authors",
        "commentsView.show.dates",
        "commentsView.show.colors",
        "commentsView.show.subjects",
        "commentsView.show.states",
        "commentsView.show.text",
        "commentsView.show.noTextPlaceholder",
        "commentsView.show.compactItems",
        "commentsView.show.uniformText",
        "commentsView.show.dynamicIcons",
        "commentsView.show.wrapLongTitles",
        "commentsView.showHiddenComments",
        "commentsView.showCommentsFromHiddenLayers",
    ];
    let reg = registry();
    rsx! {
        Frame { title: t(lang, "cmd.commentsView.options"),
            div { class: "field",
                label { "{t(lang, \"cmd.commentsView.groupBy\")}" }
                select { onchange: move |e| crate::commands::review::set_group(st, &e.value()),
                    for g in crate::commands::review::GROUPS.iter().copied().filter(|g| *g != "stateAndAuthor") {
                        option { key: "{g}", value: "{g}", selected: group == g, "{group_name(lang, g)}" }
                    }
                }
            }
            div { class: "check-list",
                for id in toggles {
                    {
                        let on = option_on(st, id);
                        let label = reg.get(id).map(|c| cmd_label(lang, c)).unwrap_or_else(|| id.to_string());
                        rsx! { label { class: "check", key: "{id}", input { r#type: "checkbox", checked: on, onchange: move |_| st.set_option(id, !on) } span { "{label}" } } }
                    }
                }
            }
        }
    }
}

#[component]
pub(super) fn AnnotationText(page: usize, index: usize, contents: String) -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let text = use_signal(|| contents.clone());
    rsx! {
        Frame { title: t(lang, "dialog.note.title"),
            on_ok: move |_| { let c = text(); apply(st, |tab| tab.doc.set_annotation_contents(page, index, &c)); },
            Area { label: t(lang, "dialog.note.title"), value: text, autofocus: true }
        }
    }
}

#[component]
pub(super) fn StampPicker() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let custom = use_signal(|| st.ui.read().value("stamp.custom").unwrap_or("").to_string());
    let mut color = use_signal(|| st.ui.read().value("stamp.color").unwrap_or("#c81e1e").to_string());
    let choose = move |text: String| {
        crate::commands::stamps::clear_pending();
        st.set_value("stamp.text", text);
        st.set_value("stamp.color", color());
        st.set_value("stamp.kind", "text");
        st.set_dialog(Dialog::None);
        st.set_tool(Tool::Stamp);
        st.toast(ToastKind::Info, t(st.lang(), "toast.place_stamp"), "");
    };
    rsx! {
        Frame { title: t(lang, "dialog.stamp.title"), wide: true, ok_label: Some(t(lang, "dialog.stamp.use_custom")),
            on_ok: move |_| {
                let c = custom().trim().to_string();
                if c.is_empty() {
                    st.toast(ToastKind::Info, t(st.lang(), "dialog.stamp.custom_empty"), "");
                } else {
                    st.set_value("stamp.custom", c.clone());
                    choose(c);
                }
            },
            div { class: "stamp-grid",
                for p in STAMP_PRESETS.iter() {
                    button { class: "stamp-preview", key: "{p}", style: "color:{color};border-color:{color}", onclick: move |_| choose(p.to_string()), "{p}" }
                }
            }
            div { class: "field-grid",
                Text { label: t(lang, "dialog.stamp.custom"), value: custom, placeholder: "PAID" }
                div { class: "field", label { "{t(lang, \"dialog.watermark.color\")}" } input { r#type: "color", value: "{color}", oninput: move |e| color.set(e.value()) } }
            }
            div { class: "row", style: "gap:8px",
                button { class: "tbtn", onclick: move |_| { st.set_dialog(Dialog::None); files::pick_image_stamp(st); }, Icon { name: "image" } "{t(lang, \"dialog.stamp.image\")}" }
            }
            div { class: "hint", "{t(lang, \"dialog.stamp.hint\")}" }
        }
    }
}

/// Units offered for measurement scales.
pub(super) const SCALE_UNITS: &[&str] = &["mm", "cm", "m", "km", "in", "ft", "pt"];

/// Current measurement scale from the preferences.
pub fn measure_scale(st: AppState) -> folitecta_core::MeasureScale {
    let ui = st.ui.read();
    let get = |k: &str, d: &str| ui.value(k).unwrap_or(d).to_string();
    let paper: f32 = get("measure.paper", "1").replace(',', ".").parse().unwrap_or(1.0);
    let real: f32 = get("measure.real", "1").replace(',', ".").parse().unwrap_or(1.0);
    let decimals: u8 = get("measure.decimals", "1").parse().unwrap_or(1);
    folitecta_core::MeasureScale::from_ratio(
        paper,
        &get("measure.paper_unit", "mm"),
        real,
        &get("measure.real_unit", "mm"),
        decimals,
    )
}

#[component]
pub(super) fn SummarizeCommentsDialog() -> Element {
    use folitecta_core::{SummaryLabels, SummaryLayout, SummaryOptions, SummarySort};
    let st = use_context::<AppState>();
    let lang = st.lang();
    let get = |k: &str, d: &str| st.ui.read().value(k).unwrap_or(d).to_string();
    let mut layout = use_signal(|| get("summary.layout", "only"));
    let mut sort = use_signal(|| get("summary.sort", "page"));
    let mut size = use_signal(|| get("summary.size", "10"));
    let mut paper = use_signal(|| get("summary.paper", "a4"));
    let mut hidden = use_signal(|| get("summary.hidden", "0") == "1");
    let count = st
        .read_active(|t| t.doc.summary_entries(true, SummarySort::Page).len())
        .unwrap_or(0);
    let layouts = [
        ("only", "dialog.summary.only"),
        ("separate", "dialog.summary.separate"),
        ("connector", "dialog.summary.connector"),
    ];
    let sorts = [
        ("page", "dialog.summary.by_page"),
        ("author", "dialog.summary.by_author"),
        ("date", "dialog.summary.by_date"),
        ("type", "dialog.summary.by_type"),
    ];
    let sizes = [
        ("8", "dialog.summary.small"),
        ("10", "dialog.summary.medium"),
        ("12", "dialog.summary.large"),
    ];
    rsx! {
        Frame { title: t(lang, "cmd.comments.summarize"), ok_label: Some(t(lang, "dialog.summary.create")),
            on_ok: move |_| {
                st.set_value("summary.layout", layout());
                st.set_value("summary.sort", sort());
                st.set_value("summary.size", size());
                st.set_value("summary.paper", paper());
                st.set_value("summary.hidden", if hidden() { "1" } else { "0" });
                let (_, _, pw, ph) = PAPER.iter().copied().find(|p| p.0 == paper()).unwrap_or(PAPER[0]);
                let types = ["Text", "FreeText", "Line", "Square", "Circle", "Polygon", "PolyLine", "Highlight", "Underline", "Squiggly", "StrikeOut", "Stamp", "Caret", "Ink", "FileAttachment", "Sound", "Redact"]
                    .iter()
                    .map(|s| (s.to_string(), crate::i18n::annot_name(lang, s)))
                    .collect();
                let opts = SummaryOptions {
                    layout: match layout().as_str() { "separate" => SummaryLayout::SeparatePages, "connector" => SummaryLayout::ConnectorLines, _ => SummaryLayout::CommentsOnly },
                    sort: match sort().as_str() { "author" => SummarySort::Author, "date" => SummarySort::Date, "type" => SummarySort::Type, _ => SummarySort::Page },
                    font_size: size().parse().unwrap_or(10.0),
                    include_hidden: hidden(),
                    paper: (pw, ph),
                    labels: SummaryLabels {
                        title: t(lang, "dialog.summary.doc_title"),
                        page: t(lang, "status.page"),
                        no_comments: t(lang, "toast.no_comments"),
                        types,
                    },
                };
                match st.read_active(|t| t.doc.summarize_comments(&opts)) {
                    Some(Ok(doc)) => {
                        st.set_dialog(Dialog::None);
                        files::open_document_tab(st, doc);
                    }
                    Some(Err(e)) => st.error(e),
                    None => st.set_dialog(Dialog::None),
                }
            },
            div { class: "hint", "{tf(lang, \"dialog.summary.count\", &[(\"count\", &count.to_string())])}" }
            div { class: "field",
                label { "{t(lang, \"dialog.summary.layout\")}" }
                div { class: "radio-col",
                    for (value, key) in layouts {
                        label { key: "{value}", input { r#type: "radio", name: "summary-layout", checked: layout() == value, onchange: move |_| layout.set(value.to_string()) } "{t(lang, key)}" }
                    }
                }
            }
            div { class: "field-grid",
                div { class: "field", label { "{t(lang, \"dialog.summary.sort\")}" }
                    select { onchange: move |e| sort.set(e.value()),
                        for (value, key) in sorts { option { key: "{value}", value: "{value}", selected: sort() == value, "{t(lang, key)}" } }
                    }
                }
                div { class: "field", label { "{t(lang, \"dialog.summary.size\")}" }
                    select { onchange: move |e| size.set(e.value()),
                        for (value, key) in sizes { option { key: "{value}", value: "{value}", selected: size() == value, "{t(lang, key)}" } }
                    }
                }
                if layout() == "only" {
                    div { class: "field", label { "{t(lang, \"dialog.summary.paper\")}" }
                        select { onchange: move |e| paper.set(e.value()),
                            for (id, name, _, _) in PAPER.iter().copied() { option { key: "{id}", value: "{id}", selected: paper() == id, "{name}" } }
                        }
                    }
                }
            }
            label { class: "check", input { r#type: "checkbox", checked: hidden(), onchange: move |_| hidden.set(!hidden()) } span { "{t(lang, \"dialog.summary.hidden\")}" } }
        }
    }
}

#[component]
pub(super) fn CalibrateDialog() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let get = |k: &str, d: &str| st.ui.read().value(k).unwrap_or(d).to_string();
    let paper = use_signal(|| get("measure.paper", "1"));
    let real = use_signal(|| get("measure.real", "1"));
    let decimals = use_signal(|| get("measure.decimals", "1"));
    let mut paper_unit = use_signal(|| get("measure.paper_unit", "mm"));
    let mut real_unit = use_signal(|| get("measure.real_unit", "mm"));
    let preview = {
        let p: f32 = paper().replace(',', ".").parse().unwrap_or(1.0);
        let r: f32 = real().replace(',', ".").parse().unwrap_or(1.0);
        folitecta_core::MeasureScale::from_ratio(p, &paper_unit(), r, &real_unit(), 1).ratio
    };
    rsx! {
        Frame { title: t(lang, "dialog.calibrate.title"),
            on_ok: move |_| {
                st.set_value("measure.paper", paper());
                st.set_value("measure.paper_unit", paper_unit());
                st.set_value("measure.real", real());
                st.set_value("measure.real_unit", real_unit());
                st.set_value("measure.decimals", decimals());
                st.set_dialog(Dialog::None);
            },
            div { class: "hint", "{t(lang, \"dialog.calibrate.hint\")}" }
            div { class: "field-grid",
                Num { label: t(lang, "dialog.calibrate.paper"), value: paper, min: "0", step: "0.1" }
                div { class: "field", label { "{t(lang, \"dialog.calibrate.unit\")}" }
                    select { onchange: move |e| paper_unit.set(e.value()),
                        for u in SCALE_UNITS.iter().filter(|u| !["m", "km", "ft"].contains(u)) { option { key: "{u}", value: "{u}", selected: *u == paper_unit(), "{u}" } }
                    }
                }
                Num { label: t(lang, "dialog.calibrate.real"), value: real, min: "0", step: "0.1" }
                div { class: "field", label { "{t(lang, \"dialog.calibrate.unit\")}" }
                    select { onchange: move |e| real_unit.set(e.value()),
                        for u in SCALE_UNITS.iter() { option { key: "{u}", value: "{u}", selected: *u == real_unit(), "{u}" } }
                    }
                }
                Num { label: t(lang, "dialog.calibrate.decimals"), value: decimals, min: "0", max: "6" }
            }
            div { class: "kv", span { class: "k", "{t(lang, \"dialog.calibrate.scale\")}" } span { class: "v", "{preview}" } }
        }
    }
}
