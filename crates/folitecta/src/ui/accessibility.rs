//! Accessibility panes: tags, reading order, content, the accessibility
//! checker and its report.

use std::collections::HashSet;

use dioxus::prelude::*;
use folitecta_core::accessibility::{CATEGORIES, CheckResult, Fix, rule};
use folitecta_core::registry;
use folitecta_core::structure::{StructElem, StructKid, excerpt};

use crate::commands::accessibility::{
    Id, ORDER_TAGS, Shown, categories_off, indent_tag, move_order, move_tag, order_items, order_mode,
    select_order, select_tag, show_check_item, show_content_item, shown_status,
};
use crate::commands::{dispatch, option_on};
use crate::i18n::{Lang, cmd_label, t, tf};
use crate::icons::Icon;
use crate::state::{AppState, CheckState, Dialog};

fn label_of(lang: Lang, id: &str) -> String {
    registry()
        .get(id)
        .map(|c| cmd_label(lang, c))
        .unwrap_or_else(|| id.to_string())
}

/// Tool buttons that run commands: id, icon, enabled, on.
fn tool_buttons(st: AppState, lang: Lang, tools: Vec<(&'static str, &'static str, bool, bool)>) -> Element {
    rsx! {
        for (id, icon, enabled, on) in tools {
            button { class: if on { "ibtn on" } else { "ibtn" }, key: "{id}", title: "{label_of(lang, id)}", disabled: !enabled,
                onclick: move |_| dispatch(st, id), Icon { name: icon } }
        }
    }
}

fn no_tags(st: AppState, lang: Lang) -> Element {
    rsx! {
        div { class: "pane-empty acc-empty",
            p { "{t(lang, \"acc.no_tags\")}" }
            button { class: "tbtn primary", onclick: move |_| dispatch(st, "acc.autotag.document"), "{label_of(lang, \"acc.autotag.document\")}" }
            button { class: "tbtn", onclick: move |_| dispatch(st, "tagsView.createRoot"), "{label_of(lang, \"tagsView.createRoot\")}" }
        }
    }
}

// ---- Tags ----------------------------------------------------------------------

#[derive(Clone, PartialEq)]
enum TagRow {
    Root {
        selected: bool,
    },
    Elem {
        id: Id,
        depth: usize,
        tag: String,
        role: String,
        title: String,
        open: bool,
        has_kids: bool,
        selected: bool,
        alt: bool,
    },
    Content {
        depth: usize,
        page: usize,
        mcid: i64,
    },
    Annot {
        depth: usize,
        page: usize,
        index: usize,
        subtype: String,
    },
    Other {
        depth: usize,
        text: String,
    },
}

const MAX_ROWS: usize = 2500;

fn tag_rows(tree: &StructElem, sel: Option<Id>, collapsed: &HashSet<Id>) -> (Vec<TagRow>, bool) {
    let mut rows = vec![TagRow::Root {
        selected: sel.is_none(),
    }];
    let mut more = false;
    fn go(
        e: &StructElem,
        depth: usize,
        sel: Option<Id>,
        collapsed: &HashSet<Id>,
        rows: &mut Vec<TagRow>,
        more: &mut bool,
    ) {
        for k in &e.kids {
            if rows.len() >= MAX_ROWS {
                *more = true;
                return;
            }
            match k {
                StructKid::Elem(c) => {
                    let open = !collapsed.contains(&c.id);
                    rows.push(TagRow::Elem {
                        id: c.id,
                        depth,
                        tag: c.tag.clone(),
                        role: c.role.clone(),
                        title: c.title.clone(),
                        open,
                        has_kids: !c.kids.is_empty(),
                        selected: sel == Some(c.id),
                        alt: !c.alt.trim().is_empty(),
                    });
                    if open {
                        go(c, depth + 1, sel, collapsed, rows, more);
                    }
                }
                StructKid::Content { page, mcid } => rows.push(TagRow::Content {
                    depth,
                    page: *page,
                    mcid: *mcid,
                }),
                StructKid::Annot { page, index, subtype } => rows.push(TagRow::Annot {
                    depth,
                    page: *page,
                    index: *index,
                    subtype: subtype.clone(),
                }),
                StructKid::Other(s) => rows.push(TagRow::Other {
                    depth,
                    text: s.clone(),
                }),
            }
        }
    }
    go(tree, 1, sel, collapsed, &mut rows, &mut more);
    (rows, more)
}

#[component]
pub fn TagsPane() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let Some((tree, sel, collapsed, tagged)) = st.read_active(|t| {
        (
            t.doc.structure(),
            t.acc.tag,
            t.acc.collapsed.clone(),
            t.doc.is_tagged(),
        )
    }) else {
        return rsx! {};
    };
    let has = tree.is_some();
    let some = sel.is_some();
    let tools = vec![
        ("tagsview.new", "new", true, false),
        ("pageStructure.deleteItem", "delete", some, false),
        ("tagsview.changeToArtifact", "eye-off", some, false),
        ("tagsview.findTagFromSelection", "search", has, false),
        (
            "tagsview.highlightContent",
            "highlight",
            true,
            option_on(st, "tagsview.highlightContent"),
        ),
        ("acc.autotag.document", "tags", true, false),
        ("tagsView.options", "settings", true, false),
    ];
    let Some(tree) = tree else {
        return rsx! {
            div { class: "pane-toolbar icon-toolbar", {tool_buttons(st, lang, tools)} }
            {no_tags(st, lang)}
        };
    };
    let (rows, more) = tag_rows(&tree, sel, &collapsed);
    // Text of the marked content shown in the tree.
    let pages: Vec<usize> = rows
        .iter()
        .filter_map(|r| match r {
            TagRow::Content { page, .. } => Some(*page),
            _ => None,
        })
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    let marked = st
        .read_active(|t| t.doc.marked_content_pages(&pages))
        .unwrap_or_default();
    let text_of = |page: usize, mcid: i64| {
        marked
            .get(&page)
            .and_then(|items| items.iter().find(|m| m.mcid == Some(mcid)))
            .map(|m| {
                if m.has_text {
                    excerpt(&m.text, 48)
                } else if m.has_image {
                    t(lang, "content.image")
                } else {
                    t(lang, "content.drawing")
                }
            })
            .unwrap_or_default()
    };
    let move_title = (
        t(lang, "tags.move_up"),
        t(lang, "tags.move_down"),
        t(lang, "tags.indent"),
        t(lang, "tags.outdent"),
    );
    rsx! {
        div { class: "pane-toolbar icon-toolbar",
            {tool_buttons(st, lang, tools)}
            button { class: "ibtn", title: "{move_title.0}", disabled: !some, onclick: move |_| move_tag(st, -1), Icon { name: "up" } }
            button { class: "ibtn", title: "{move_title.1}", disabled: !some, onclick: move |_| move_tag(st, 1), Icon { name: "down" } }
            button { class: "ibtn", title: "{move_title.2}", disabled: !some, onclick: move |_| indent_tag(st, true), Icon { name: "next" } }
            button { class: "ibtn", title: "{move_title.3}", disabled: !some, onclick: move |_| indent_tag(st, false), Icon { name: "prev" } }
        }
        if !tagged {
            div { class: "acc-note", "{t(lang, \"tags.not_marked\")}" }
        }
        div { class: "tree tag-tree",
            for (i, row) in rows.into_iter().enumerate() {
                match row {
                    TagRow::Root { selected } => {
                        let root_key = "root";
                        rsx! {
                        div { class: if selected { "tree-item tag-row selected" } else { "tree-item tag-row" }, key: "{root_key}",
                            onclick: move |_| { st.with_active(|t| t.acc.tag = None); },
                            Icon { name: "tags" }
                            span { class: "tree-label", "{t(lang, \"tags.root\")}" }
                        }
                        }
                    }
                    TagRow::Elem { id, depth, tag, role, title, open, has_kids, selected, alt } => {
                        let style = format!("padding-left:{}px", 4 + depth * 14);
                        let shown_role = if role != tag && !role.is_empty() { format!("→ {role}") } else { String::new() };
                        let row_key = format!("e{}-{}", id.0, id.1);
                        let tag_label = format!("<{tag}>");
                        rsx! {
                            div { class: if selected { "tree-item tag-row selected" } else { "tree-item tag-row" }, key: "{row_key}", style: "{style}",
                                onclick: move |_| select_tag(st, Some(id)),
                                ondoubleclick: move |_| st.set_dialog(Dialog::TagProps(id)),
                                span { class: "twisty",
                                    onclick: move |e| { e.stop_propagation(); st.with_active(|t| { if !t.acc.collapsed.remove(&id) { t.acc.collapsed.insert(id); } }); },
                                    if has_kids { if open { "▾" } else { "▸" } } else { " " }
                                }
                                span { class: "tag-name", "{tag_label}" }
                                if !shown_role.is_empty() { span { class: "muted", "{shown_role}" } }
                                if !title.is_empty() { span { class: "tree-label muted", "{title}" } }
                                if alt { span { class: "chip", title: "{t(lang, \"tags.has_alt\")}", "Alt" } }
                            }
                        }
                    }
                    TagRow::Content { depth, page, mcid } => {
                        let style = format!("padding-left:{}px", 18 + depth * 14);
                        let text = text_of(page, mcid);
                        rsx! {
                            div { class: "tree-item tag-content", key: "c{i}", style: "{style}",
                                title: "{t(lang, \"status.page\")} {page + 1} · MCID {mcid}",
                                onclick: move |_| { st.with_active(|t| t.acc.content = Some((page, Some(mcid)))); if st.current_page() != page { st.navigate(page, true); } show_content_item(st, page, Some(mcid)); },
                                Icon { name: "text" }
                                span { class: "tree-label", "{text}" }
                                span { class: "chip", "{page + 1}" }
                            }
                        }
                    }
                    TagRow::Annot { depth, page, index, subtype } => {
                        let style = format!("padding-left:{}px", 18 + depth * 14);
                        let name = crate::i18n::annot_name(lang, &subtype);
                        rsx! {
                            div { class: "tree-item tag-content", key: "a{i}", style: "{style}",
                                onclick: move |_| { st.with_active(|t| t.selected_annot = Some((page, index))); st.go_to_page(page); },
                                Icon { name: "comment" }
                                span { class: "tree-label", "{name}" }
                                span { class: "chip", "{page + 1}" }
                            }
                        }
                    }
                    TagRow::Other { depth, text } => {
                        let style = format!("padding-left:{}px", 18 + depth * 14);
                        rsx! { div { class: "tree-item tag-content muted", key: "o{i}", style: "{style}", "{text}" } }
                    }
                }
            }
            if more { div { class: "hint", "{t(lang, \"tags.more\")}" } }
        }
    }
}

// ---- Reading order -------------------------------------------------------------

#[component]
pub fn OrderPane() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let has = st.read_active(|t| t.doc.has_structure()).unwrap_or(false);
    let mode = order_mode(st);
    let cells = st.value("order.cells") == "1";
    let only = st.value("order.tf") == "1";
    let page = st.current_page();
    let tools = vec![
        ("orderview.showmode.order", "number", true, mode == "order"),
        ("orderview.showmode.type", "tag", true, mode == "type"),
        ("orderview.showmode.none", "eye-off", true, mode == "none"),
        ("orderview.show.tableCells", "grid", has, cells),
        ("orderview.show.tablesAndFigures", "image", has, only),
        ("orderview.groupsimilar", "merge", has, false),
        ("orderView.options", "settings", true, false),
    ];
    if !has {
        return rsx! {
            div { class: "pane-toolbar icon-toolbar", {tool_buttons(st, lang, tools)} }
            {no_tags(st, lang)}
        };
    }
    let items = order_items(st);
    let sel = st.read_active(|t| t.acc.order).flatten();
    let n = items.len();
    let sel_tag = sel
        .and_then(|i| items.get(i))
        .map(|i| i.tag.clone())
        .unwrap_or_default();
    rsx! {
        div { class: "pane-toolbar icon-toolbar", {tool_buttons(st, lang, tools)} }
        div { class: "acc-head",
            span { "{t(lang, \"status.page\")} {page + 1}" }
            div { class: "spacer" }
            if sel.is_some() {
                select { title: "{t(lang, \"order.tag_as\")}",
                    onchange: move |e| { let v = e.value(); if let Some((id, _)) = ORDER_TAGS.iter().find(|(_, tg)| *tg == v) { dispatch(st, id); } },
                    option { value: "", selected: true, disabled: true, "{t(lang, \"order.tag_as\")}" }
                    for (id, tg) in ORDER_TAGS.iter() {
                        option { key: "{id}", value: "{tg}", selected: false, "{label_of(lang, id)}" }
                    }
                }
            }
        }
        if items.is_empty() { div { class: "pane-empty", "{t(lang, \"order.empty\")}" } }
        div { class: "tree order-list",
            for (i, item) in items.into_iter().enumerate() {
                {
                    let selected = sel == Some(i);
                    let order_key = format!("o{}-{}-{}", i, item.id.0, item.id.1);
                    let text = if !item.text.is_empty() {
                        excerpt(&item.text, 70)
                    } else if !item.annotation.is_empty() {
                        crate::i18n::annot_name(lang, &item.annotation)
                    } else {
                        t(lang, "content.drawing")
                    };
                    rsx! {
                        div { class: if selected { "tree-item order-row selected" } else { "tree-item order-row" }, key: "{order_key}",
                            onclick: move |_| select_order(st, Some(i)),
                            ondoubleclick: move |_| st.set_dialog(Dialog::TagProps(item.id)),
                            span { class: "order-num", "{i + 1}" }
                            span { class: "tag-name", "{item.tag}" }
                            span { class: "tree-label", title: "{item.text}", "{text}" }
                            if selected {
                                button { class: "ibtn", title: "{t(lang, \"tags.move_up\")}", disabled: i == 0,
                                    onclick: move |e| { e.stop_propagation(); move_order(st, i, i - 1); }, Icon { name: "up" } }
                                button { class: "ibtn", title: "{t(lang, \"tags.move_down\")}", disabled: i + 1 >= n,
                                    onclick: move |e| { e.stop_propagation(); move_order(st, i, i + 1); }, Icon { name: "down" } }
                            }
                        }
                    }
                }
            }
        }
        if !sel_tag.is_empty() { div { class: "hint", "{t(lang, \"order.hint\")}" } }
    }
}

// ---- Content -------------------------------------------------------------------

#[component]
pub fn ContentPane() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let page = st.current_page();
    let Some((items, sel)) = st.read_active(|t| (t.doc.marked_content(page), t.acc.content)) else {
        return rsx! {};
    };
    let tools = vec![
        (
            "contentsView.showInTags",
            "tags",
            sel.is_some_and(|s| s.1.is_some()),
            false,
        ),
        (
            "contentsView.displaySelected",
            "eye",
            true,
            option_on(st, "contentsView.displaySelected"),
        ),
        ("contentsView.options", "settings", true, false),
    ];
    let untagged = items.iter().filter(|m| m.mcid.is_none() && !m.artifact).count();
    rsx! {
        div { class: "pane-toolbar icon-toolbar", {tool_buttons(st, lang, tools)} }
        div { class: "acc-head",
            span { "{t(lang, \"status.page\")} {page + 1}" }
            div { class: "spacer" }
            span { class: "muted", "{tf(lang, \"content.count\", &[(\"count\", &items.len().to_string())])}" }
        }
        if untagged > 0 { div { class: "acc-note", "{t(lang, \"content.untagged_note\")}" } }
        if items.is_empty() { div { class: "pane-empty", "{t(lang, \"content.empty\")}" } }
        div { class: "tree content-list",
            for (i, m) in items.iter().cloned().enumerate() {
                {
                    let selected = sel == Some((page, m.mcid)) && (m.mcid.is_some() || !m.artifact);
                    let icon = if m.has_image { "image" } else if m.has_text { "text" } else { "line" };
                    let head = match m.mcid {
                        Some(id) => format!("<{}> MCID {id}", m.tag),
                        None if m.artifact => t(lang, "content.artifact"),
                        None => t(lang, "content.untagged"),
                    };
                    let text = if m.has_text { excerpt(&m.text, 60) } else if m.has_image { t(lang, "content.image") } else { t(lang, "content.drawing") };
                    let class = match (selected, m.mcid.is_none() && !m.artifact) {
                        (true, _) => "tree-item content-row selected",
                        (false, true) => "tree-item content-row warn",
                        _ => "tree-item content-row",
                    };
                    let mcid = m.mcid;
                    rsx! {
                        div { class: "{class}", key: "m{i}",
                            onclick: move |_| show_content_item(st, page, mcid),
                            ondoubleclick: move |_| if mcid.is_some() { dispatch(st, "contentsView.showInTags") },
                            Icon { name: icon }
                            div { class: "content-text",
                                span { class: "tag-name", "{head}" }
                                span { class: "tree-label muted", title: "{m.text}", "{text}" }
                            }
                        }
                    }
                }
            }
        }
    }
}

// ---- Checker ---------------------------------------------------------------------

fn status_icon(s: Shown) -> (&'static str, &'static str) {
    match s {
        Shown::Passed => ("✓", "acc-st passed"),
        Shown::Failed => ("✗", "acc-st failed"),
        Shown::Manual => ("?", "acc-st manual"),
        Shown::Skipped => ("–", "acc-st skipped"),
    }
}

fn status_name(lang: Lang, s: Shown) -> String {
    t(
        lang,
        match s {
            Shown::Passed => "acc.status.passed",
            Shown::Failed => "acc.status.failed",
            Shown::Manual => "acc.status.manual",
            Shown::Skipped => "acc.status.skipped",
        },
    )
}

pub fn rule_name(lang: Lang, id: &str) -> String {
    t(lang, &format!("acc.rule.{id}"))
}

fn category_name(lang: Lang, id: &str) -> String {
    t(lang, &format!("acc.cat.{id}"))
}

/// Counts of the shown statuses.
fn counts(cs: &CheckState, results: &[&CheckResult]) -> [usize; 4] {
    let mut c = [0; 4];
    for r in results {
        let i = match shown_status(cs, r) {
            Shown::Passed => 0,
            Shown::Failed => 1,
            Shown::Manual => 2,
            Shown::Skipped => 3,
        };
        c[i] += 1;
    }
    c
}

#[component]
pub fn CheckerPane() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let Some(check) = st.read_active(|t| t.acc.check.clone()) else {
        return rsx! {};
    };
    let Some(cs) = check else {
        return rsx! {
            div { class: "pane-empty acc-empty",
                p { "{t(lang, \"acc.no_check\")}" }
                button { class: "tbtn primary", onclick: move |_| dispatch(st, "acc.check.document"), "{label_of(lang, \"acc.check.document\")}" }
            }
        };
    };
    let off = categories_off(st);
    let shown: Vec<&CheckResult> = cs
        .results
        .iter()
        .filter(|r| !off.iter().any(|o| o == r.category))
        .collect();
    let [passed, failed, manual, skipped] = counts(&cs, &shown);
    let sel_rule = cs.rule.clone();
    let sel_fix = sel_rule
        .as_deref()
        .and_then(rule)
        .map(|r| r.fix != Fix::None)
        .unwrap_or(false);
    let has_sel = sel_rule.is_some();
    let tools = vec![
        ("chekerView.reCheck", "redo", true, false),
        ("chekerView.fix", "check", has_sel && sel_fix, false),
        ("chekerView.skip", "close", has_sel, false),
        ("chekerView.explain", "help", has_sel, false),
        ("chekerView.showReport", "page", true, false),
        ("chekerView.options", "settings", true, false),
    ];
    rsx! {
        div { class: "pane-toolbar icon-toolbar", {tool_buttons(st, lang, tools)} }
        div { class: "acc-summary",
            span { class: "acc-st passed", "✓ {passed}" }
            span { class: "acc-st failed", "✗ {failed}" }
            span { class: "acc-st manual", "? {manual}" }
            span { class: "acc-st skipped", "– {skipped}" }
        }
        div { class: "tree acc-tree",
            for cat in CATEGORIES.iter().filter(|c| !off.iter().any(|o| o == *c)) {
                {
                    let cat_id = cat.to_string();
                    let open = !cs.collapsed.contains(&cat_id);
                    let in_cat: Vec<&CheckResult> = shown.iter().copied().filter(|r| r.category == *cat).collect();
                    let failed_here = in_cat.iter().filter(|r| shown_status(&cs, r) == Shown::Failed).count();
                    let key_c = cat_id.clone();
                    rsx! {
                        div { class: "tree-item acc-cat", key: "c{cat}",
                            onclick: move |_| { let k = key_c.clone(); st.with_active(|t| if let Some(c) = t.acc.check.as_mut() { if !c.collapsed.remove(&k) { c.collapsed.insert(k); } }); },
                            span { class: "twisty", if open { "▾" } else { "▸" } }
                            span { class: "tree-label", "{category_name(lang, cat)}" }
                            if failed_here > 0 { span { class: "chip danger", "{failed_here}" } }
                        }
                        if open {
                            for r in in_cat {
                                {
                                    let status = shown_status(&cs, r);
                                    let (mark, class) = status_icon(status);
                                    let rid = r.rule.to_string();
                                    let selected = sel_rule.as_deref() == Some(r.rule);
                                    let expanded = cs.expanded.contains(r.rule);
                                    let n = r.items.len();
                                    let items = r.items.clone();
                                    let sel_item = if selected { cs.item } else { None };
                                    let rid_click = rid.clone();
                                    rsx! {
                                        div { class: if selected { "tree-item acc-rule selected" } else { "tree-item acc-rule" }, key: "r{rid}",
                                            title: "{status_name(lang, status)}",
                                            onclick: move |_| {
                                                let k = rid_click.clone();
                                                st.with_active(|t| if let Some(c) = t.acc.check.as_mut() {
                                                    // A new rule opens; the selected one opens or closes.
                                                    let same = c.rule.as_deref() == Some(k.as_str());
                                                    if !same || !c.expanded.remove(&k) {
                                                        c.expanded.insert(k.clone());
                                                    }
                                                    c.rule = Some(k);
                                                    c.item = None;
                                                });
                                            },
                                            ondoubleclick: move |_| dispatch(st, "chekerView.explain"),
                                            span { class: "{class}", "{mark}" }
                                            span { class: "tree-label", "{rule_name(lang, &rid)}" }
                                            if n > 0 { span { class: "chip", "{n}" } }
                                        }
                                        if expanded {
                                            for (j, item) in items.into_iter().enumerate() {
                                                {
                                                    let it = item.clone();
                                                    let page = item.page.map(|p| format!("{} {}", t(lang, "status.page"), p + 1)).unwrap_or_default();
                                                    let rid2 = rid.clone();
                                                    rsx! {
                                                        div { class: if sel_item == Some(j) { "tree-item acc-item selected" } else { "tree-item acc-item" }, key: "i{rid}{j}",
                                                            onclick: move |_| {
                                                                let k = rid2.clone();
                                                                st.with_active(|t| if let Some(c) = t.acc.check.as_mut() { c.rule = Some(k); c.item = Some(j); });
                                                                show_check_item(st, &it);
                                                            },
                                                            span { class: "tree-label", title: "{item.label}", "{item.label}" }
                                                            if !page.is_empty() { span { class: "chip", "{page}" } }
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
                }
            }
        }
        if has_sel {
            div { class: "acc-actions",
                button { class: "tbtn", disabled: !sel_fix, onclick: move |_| dispatch(st, "chekerView.fix"), "{label_of(lang, \"chekerView.fix\")}" }
                button { class: "tbtn", onclick: move |_| dispatch(st, "chekerView.pass"), "{label_of(lang, \"chekerView.pass\")}" }
                button { class: "tbtn", onclick: move |_| dispatch(st, "chekerView.fail"), "{label_of(lang, \"chekerView.fail\")}" }
                button { class: "tbtn", onclick: move |_| dispatch(st, "chekerView.skip"), "{label_of(lang, \"chekerView.skip\")}" }
            }
        }
    }
}

// ---- Report ------------------------------------------------------------------------

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// The accessibility report as an HTML page.
pub fn report_html(lang: Lang, name: &str, cs: &CheckState, off: &[String], passed_too: bool) -> String {
    let shown: Vec<&CheckResult> = cs
        .results
        .iter()
        .filter(|r| !off.iter().any(|o| o == r.category))
        .collect();
    let [p, f, m, s] = counts(cs, &shown);
    let mut html = format!(
        "<!doctype html><html lang=\"{}\"><head><meta charset=\"utf-8\"><title>{}</title>\
         <style>body{{font-family:system-ui,sans-serif;margin:2em;color:#222}}table{{border-collapse:collapse;width:100%;margin-bottom:1.5em}}\
         td,th{{border:1px solid #ccc;padding:4px 8px;text-align:left}}.failed{{color:#b91c1c}}.passed{{color:#15803d}}.manual{{color:#b45309}}</style></head><body>",
        lang.code(),
        html_escape(&t(lang, "acc.report.title"))
    );
    html.push_str(&format!(
        "<h1>{}</h1><p><b>{}</b> · {}</p><p>{}</p>",
        html_escape(&t(lang, "acc.report.title")),
        html_escape(name),
        html_escape(&cs.when),
        html_escape(&tf(
            lang,
            "acc.report.summary",
            &[
                ("passed", &p.to_string()),
                ("failed", &f.to_string()),
                ("manual", &m.to_string()),
                ("skipped", &s.to_string())
            ]
        ))
    ));
    for cat in CATEGORIES.iter().filter(|c| !off.iter().any(|o| o == *c)) {
        let rows: Vec<&&CheckResult> = shown
            .iter()
            .filter(|r| r.category == *cat && (passed_too || shown_status(cs, r) != Shown::Passed))
            .collect();
        if rows.is_empty() {
            continue;
        }
        html.push_str(&format!(
            "<h2>{}</h2><table><tr><th>{}</th><th>{}</th><th>{}</th></tr>",
            html_escape(&category_name(lang, cat)),
            html_escape(&t(lang, "acc.report.rule")),
            html_escape(&t(lang, "acc.report.status")),
            html_escape(&t(lang, "acc.report.description"))
        ));
        for r in rows {
            let st = shown_status(cs, r);
            let class = match st {
                Shown::Passed => "passed",
                Shown::Failed => "failed",
                _ => "manual",
            };
            let mut desc = t(lang, &format!("acc.explain.{}", r.rule));
            if !r.items.is_empty() {
                let list: Vec<String> = r
                    .items
                    .iter()
                    .take(20)
                    .map(|i| match i.page {
                        Some(p) => format!("{} ({} {})", i.label, t(lang, "status.page"), p + 1),
                        None => i.label.clone(),
                    })
                    .collect();
                desc.push_str(&format!(" — {}", list.join("; ")));
            }
            html.push_str(&format!(
                "<tr><td>{}</td><td class=\"{class}\">{}</td><td>{}</td></tr>",
                html_escape(&rule_name(lang, r.rule)),
                html_escape(&status_name(lang, st)),
                html_escape(&desc)
            ));
        }
        html.push_str("</table>");
    }
    html.push_str("</body></html>");
    html
}

#[component]
pub fn ReportPane() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let Some((check, name)) = st.read_active(|t| (t.acc.check.clone(), t.doc.display_name())) else {
        return rsx! {};
    };
    let Some(cs) = check else {
        return rsx! {
            div { class: "pane-empty acc-empty",
                p { "{t(lang, \"acc.no_report\")}" }
                button { class: "tbtn primary", onclick: move |_| dispatch(st, "acc.check.report"), "{label_of(lang, \"acc.check.report\")}" }
            }
        };
    };
    let off = categories_off(st);
    let passed_too = st.value("acc.report.passed") != "0";
    let shown: Vec<&CheckResult> = cs
        .results
        .iter()
        .filter(|r| !off.iter().any(|o| o == r.category))
        .collect();
    let [p, f, m, s] = counts(&cs, &shown);
    let summary = tf(
        lang,
        "acc.report.summary",
        &[
            ("passed", &p.to_string()),
            ("failed", &f.to_string()),
            ("manual", &m.to_string()),
            ("skipped", &s.to_string()),
        ],
    );
    let html_name = name.clone();
    let cs_save = cs.clone();
    let off_save = off.clone();
    rsx! {
        div { class: "pane-toolbar icon-toolbar",
            button { class: "ibtn", title: "{label_of(lang, \"chekerView.reCheck\")}", onclick: move |_| dispatch(st, "chekerView.reCheck"), Icon { name: "redo" } }
            button { class: "ibtn", title: "{t(lang, \"acc.report.save\")}",
                onclick: move |_| {
                    let html = report_html(lang, &html_name, &cs_save, &off_save, passed_too);
                    let stem = std::path::Path::new(&html_name).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                    crate::files::save_text_file(st, &format!("{stem} - {}.html", t(lang, "acc.report.title")), html);
                },
                Icon { name: "save" }
            }
            button { class: "ibtn", title: "{label_of(lang, \"accReportView.options\")}", onclick: move |_| dispatch(st, "accReportView.options"), Icon { name: "settings" } }
        }
        div { class: "acc-report",
            h3 { "{t(lang, \"acc.report.title\")}" }
            p { class: "muted", "{name} · {cs.when}" }
            p { "{summary}" }
            for cat in CATEGORIES.iter().filter(|c| !off.iter().any(|o| o == *c)) {
                {
                    let rows: Vec<&CheckResult> = shown.iter().copied().filter(|r| r.category == *cat && (passed_too || shown_status(&cs, r) != Shown::Passed)).collect();
                    rsx! {
                        div { class: "acc-report-cat", key: "{cat}",
                            if !rows.is_empty() { h4 { "{category_name(lang, cat)}" } }
                            for r in rows {
                                {
                                    let status = shown_status(&cs, r);
                                    let (mark, class) = status_icon(status);
                                    let rid = r.rule;
                                    rsx! {
                                        div { class: "acc-report-row", key: "{rid}",
                                            span { class: "{class}", "{mark}" }
                                            span { class: "tree-label", "{rule_name(lang, rid)}" }
                                            span { class: "muted", "{status_name(lang, status)}" }
                                            if !r.items.is_empty() { span { class: "chip", "{r.items.len()}" } }
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
}
