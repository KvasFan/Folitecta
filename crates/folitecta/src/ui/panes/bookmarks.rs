//! Bookmarks and named destinations panes.

use std::collections::HashSet;

use dioxus::prelude::*;
use folitecta_core::OutlineItem;
use folitecta_core::registry;

use crate::commands::bookmarks::go_to_item;
use crate::commands::{dispatch, option_on};
use crate::i18n::{cmd_label, t};
use crate::icons::Icon;
use crate::state::{AppState, Dialog};

/// Where a dragged bookmark lands relative to the bookmark under the pointer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Zone {
    Before,
    Inside,
    After,
}

fn key_of(path: &[usize]) -> String {
    path.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(".")
}

/// Keys of every bookmark that has children.
fn parent_keys(items: &[OutlineItem], prefix: &mut Vec<usize>, out: &mut HashSet<String>) {
    for (i, item) in items.iter().enumerate() {
        prefix.push(i);
        if !item.children.is_empty() {
            out.insert(key_of(prefix));
            parent_keys(&item.children, prefix, out);
        }
        prefix.pop();
    }
}

/// Path of the bookmark of the current page: the last one (in document order)
/// whose page is at or before it.
fn current_bookmark(items: &[OutlineItem], page: usize) -> Option<Vec<usize>> {
    fn walk(
        items: &[OutlineItem],
        prefix: &mut Vec<usize>,
        page: usize,
        best: &mut Option<(usize, Vec<usize>)>,
    ) {
        for (i, item) in items.iter().enumerate() {
            prefix.push(i);
            if let Some(p) = item.page {
                if p <= page && best.as_ref().map(|(bp, _)| p >= *bp).unwrap_or(true) {
                    *best = Some((p, prefix.clone()));
                }
            }
            walk(&item.children, prefix, page, best);
            prefix.pop();
        }
    }
    let mut best = None;
    walk(items, &mut Vec::new(), page, &mut best);
    best.map(|(_, p)| p)
}

/// Bookmark colour adjusted so it stays readable on the pane background.
fn readable(c: [f32; 3], dark: bool) -> String {
    let l = 0.299 * c[0] + 0.587 * c[1] + 0.114 * c[2];
    let adjust = |v: f32| -> f32 {
        if dark && l < 0.45 {
            v + (1.0 - v) * 0.55
        } else if !dark && l > 0.7 {
            v * 0.55
        } else {
            v
        }
    };
    crate::util::color_hex([adjust(c[0]), adjust(c[1]), adjust(c[2])])
}

#[derive(Debug, Clone, PartialEq)]
struct TreeOpts {
    icons: bool,
    colors: bool,
    invalid: bool,
    dark: bool,
}

#[component]
pub fn BookmarksPane() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let mut drag_src: Signal<Option<Vec<usize>>> = use_signal(|| None);
    let mut drop_at: Signal<Option<(Vec<usize>, Zone)>> = use_signal(|| None);
    let Some((items, expanded, selected, page)) = st.read_active(|t| {
        (
            t.doc.outline(),
            t.expanded_bookmarks.clone(),
            t.selected_bookmark.clone(),
            t.page,
        )
    }) else {
        return rsx! {};
    };
    let opts = TreeOpts {
        icons: option_on(st, "bookmarksView.showIcons"),
        colors: option_on(st, "bookmarksView.showContrastText"),
        invalid: option_on(st, "bookmarksView.markInvalid"),
        dark: st.ui.read().theme.is_dark(),
    };
    let wrap = option_on(st, "bookmarksView.wrapTitles");
    let size = st
        .ui
        .read()
        .value("bookmarks.text_size")
        .unwrap_or("medium")
        .to_string();
    let tree_class = format!(
        "tree bookmark-tree size-{size}{}",
        if wrap { " wrap" } else { "" }
    );
    let current = current_bookmark(&items, page);
    let mut auto: HashSet<String> = HashSet::new();
    if option_on(st, "bookmarksView.autoExpandBookmarks") {
        if let Some(c) = &current {
            for depth in 1..c.len() {
                auto.insert(key_of(&c[..depth]));
            }
        }
    }
    let ensure = option_on(st, "bookmarksView.ensureVisibility");
    let current_key = current.as_ref().map(|c| key_of(c));
    use_effect(use_reactive!(|current_key, ensure| {
        if ensure && current_key.is_some() {
            spawn(async move {
                let _ = document::eval("const c = document.querySelector('.bookmark-tree .tree-item.current'); if (c) c.scrollIntoView({ block: 'nearest' });").await;
            });
        }
    }));
    let has_sel = selected
        .as_ref()
        .map(|p| folitecta_core::outline::item_at(&items, p).is_some())
        .unwrap_or(false);
    let reg = registry();
    let label = |id: &str| {
        reg.get(id)
            .map(|c| cmd_label(lang, c))
            .unwrap_or_else(|| id.to_string())
    };
    let tools: [(&str, &str, bool); 9] = [
        ("bookmarksView.new", "insert", true),
        ("bookmarksView.moveUp", "up", has_sel),
        ("bookmarksView.moveDown", "down", has_sel),
        ("bookmarksView.promote", "prev", has_sel),
        ("bookmarksView.demote", "next", has_sel),
        ("bookmarksView.expandAll", "zoom-in", !items.is_empty()),
        ("bookmarksView.collapseAll", "zoom-out", !items.is_empty()),
        ("bookmarksView.importExport", "export", true),
        ("bookmarksView.options", "settings", true),
    ];
    let all_keys = {
        let mut s = HashSet::new();
        parent_keys(&items, &mut Vec::new(), &mut s);
        s
    };
    let sel_props = selected.clone();
    let sel_del = selected.clone();
    let mut finish_drop = move || {
        let (src, target) = (drag_src.peek().clone(), drop_at.peek().clone());
        drag_src.set(None);
        drop_at.set(None);
        let (Some(src), Some((target, zone))) = (src, target) else {
            return;
        };
        if src == target {
            return;
        }
        let (parent, index) = match zone {
            Zone::Inside => {
                let n = st
                    .read_active(|t| {
                        folitecta_core::outline::item_at(&t.doc.outline(), &target).map(|i| i.children.len())
                    })
                    .flatten()
                    .unwrap_or(0);
                (target.clone(), n)
            }
            Zone::Before | Zone::After => {
                let (last, parent) = target.split_last().expect("path");
                (
                    parent.to_vec(),
                    if zone == Zone::Before { *last } else { last + 1 },
                )
            }
        };
        match st.with_active(|t| t.doc.move_bookmark(&src, &parent, index)) {
            Some(Ok(new)) => {
                crate::files::set_window_title(st);
                st.with_active(|t| t.selected_bookmark = Some(new));
            }
            Some(Err(e)) => st.toast(crate::state::ToastKind::Info, e.to_string(), ""),
            None => {}
        }
    };
    rsx! {
        div { class: "pane-toolbar icon-toolbar",
            for (id, icon, enabled) in tools {
                button { class: "ibtn", key: "{id}", title: "{label(id)}", disabled: !enabled, onclick: move |_| dispatch(st, id), Icon { name: icon } }
            }
            button { class: "ibtn", title: "{t(lang, \"dialog.bookmark.props\")}", disabled: !has_sel,
                onclick: move |_| { if let Some(p) = sel_props.clone() { st.set_dialog(Dialog::BookmarkProps(p)); } },
                Icon { name: "pencil" }
            }
            button { class: "ibtn danger", title: "{t(lang, \"label.delete\")}", disabled: !has_sel,
                onclick: move |_| {
                    if let Some(p) = sel_del.clone() {
                        let r = st.with_active(|t| t.doc.delete_bookmark(&p));
                        st.with_active(|t| t.selected_bookmark = None);
                        if let Some(Err(e)) = r { st.error(e); }
                    }
                },
                Icon { name: "delete" }
            }
        }
        if items.is_empty() {
            div { class: "pane-empty",
                "{t(lang, \"label.none\")}"
                div { class: "row", style: "justify-content:center;margin-top:8px",
                    button { class: "tbtn", onclick: move |_| { if let Some(Err(e)) = st.with_active(|t| t.doc.bookmarks_from_pages()) { st.error(e); } }, "{t(lang, \"pane.bookmarks.from_pages\")}" }
                }
            }
        }
        div { class: "{tree_class}",
            onmouseleave: move |_| { drag_src.set(None); drop_at.set(None); },
            onmouseup: move |_| finish_drop(),
            {bookmark_nodes(st, &items, Vec::new(), &expanded, &auto, &all_keys, &selected, &current, &opts, drag_src, drop_at)}
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn bookmark_nodes(
    st: AppState,
    items: &[OutlineItem],
    path: Vec<usize>,
    expanded: &HashSet<String>,
    auto: &HashSet<String>,
    all_keys: &HashSet<String>,
    selected: &Option<Vec<usize>>,
    current: &Option<Vec<usize>>,
    opts: &TreeOpts,
    drag_src: Signal<Option<Vec<usize>>>,
    drop_at: Signal<Option<(Vec<usize>, Zone)>>,
) -> Element {
    let mut drag_src = drag_src;
    let mut drop_at = drop_at;
    let lang = st.lang();
    rsx! {
        for (i, item) in items.iter().enumerate() {
            {
                let mut p = path.clone();
                p.push(i);
                let key = key_of(&p);
                let is_open = expanded.contains("*") || expanded.contains(&key) || auto.contains(&key);
                let has_children = !item.children.is_empty();
                let depth = p.len() - 1;
                let is_sel = selected.as_ref() == Some(&p);
                let is_current = current.as_ref() == Some(&p);
                let invalid = opts.invalid && !item.valid;
                let drop_zone = drop_at.read().as_ref().filter(|(dp, _)| *dp == p).map(|(_, z)| *z);
                let mut class = String::from("tree-item");
                if is_sel { class.push_str(" selected"); }
                if is_current { class.push_str(" current"); }
                if invalid { class.push_str(" invalid"); }
                match drop_zone {
                    Some(Zone::Before) => class.push_str(" drop-before"),
                    Some(Zone::After) => class.push_str(" drop-after"),
                    Some(Zone::Inside) => class.push_str(" drop-inside"),
                    None => {}
                }
                let mut style = format!("padding-left: {}px;", 6 + depth * 14);
                if let (true, Some(c)) = (opts.colors, item.color) {
                    style.push_str(&format!("color:{};", readable(c, opts.dark)));
                }
                if item.bold { style.push_str("font-weight:700;"); }
                if item.italic { style.push_str("font-style:italic;"); }
                let p_click = p.clone();
                let p_dbl = p.clone();
                let p_down = p.clone();
                let p_move = p.clone();
                let key_t = key.clone();
                let all = all_keys.clone();
                let item_c = item.clone();
                let title = item.title.clone();
                let tip = if item.action.is_empty() { title.clone() } else { format!("{title}\n{}", item.action) };
                let children = item.children.clone();
                let icon = if item.page.is_none() && !item.action.is_empty() { "link" } else { "bookmark" };
                // The mouse moves over the rows are listened to only while a
                // bookmark is dragged: a listener of mouse moves makes every move
                // in the window wait for the program. (Starting and ending a drag
                // draws the rows again.)
                let dragging = drag_src.read().is_some();
                let on_down = move |e: Event<MouseData>| {
                    if e.trigger_button() == Some(dioxus::html::input_data::MouseButton::Primary) {
                        drag_src.set(Some(p_down.clone()));
                    }
                };
                let on_move = move |e: Event<MouseData>| {
                    let src = drag_src.peek().clone();
                    let Some(src) = src else { return };
                    if src == p_move || (p_move.len() > src.len() && p_move[..src.len()] == src[..]) {
                        drop_at.set(None);
                        return;
                    }
                    let y = e.element_coordinates().y;
                    let zone = if y < 7.0 { Zone::Before } else if y > 17.0 { Zone::After } else { Zone::Inside };
                    let next = Some((p_move.clone(), zone));
                    if *drop_at.peek() != next { drop_at.set(next); }
                };
                let on_click = move |_: Event<MouseData>| {
                    st.with_active(|t| t.selected_bookmark = Some(p_click.clone()));
                    go_to_item(st, &item_c);
                };
                let on_dbl = move |_: Event<MouseData>| st.set_dialog(Dialog::BookmarkRename(p_dbl.clone()));
                let content = rsx! {
                    span { class: "twisty",
                        onmousedown: move |e| e.stop_propagation(),
                        onclick: move |e| {
                            e.stop_propagation();
                            let all = all.clone();
                            st.with_active(|t| {
                                if t.expanded_bookmarks.remove("*") {
                                    t.expanded_bookmarks.extend(all);
                                }
                                if !t.expanded_bookmarks.remove(&key_t) {
                                    t.expanded_bookmarks.insert(key_t.clone());
                                }
                            });
                        },
                        if has_children { if is_open { "▾" } else { "▸" } } else { "" }
                    }
                    if opts.icons { Icon { name: icon } }
                    span { class: "tree-label", "{title}" }
                    if invalid { span { class: "chip warn", title: "{t(lang, \"toast.bookmark_invalid\")}", "!" } }
                    if let Some(pg) = item.page { span { class: "page-ref", "{pg + 1}" } }
                };
                let row = if dragging {
                    rsx! {
                        div { class: "{class}", style: "{style}", title: "{tip}",
                            onmousedown: on_down, onmousemove: on_move, onclick: on_click, ondoubleclick: on_dbl,
                            {content}
                        }
                    }
                } else {
                    rsx! {
                        div { class: "{class}", style: "{style}", title: "{tip}",
                            onmousedown: on_down, onclick: on_click, ondoubleclick: on_dbl,
                            {content}
                        }
                    }
                };
                rsx! {
                    div { key: "{key}",
                        {row}
                        if has_children && is_open {
                            {bookmark_nodes(st, &children, p.clone(), expanded, auto, all_keys, selected, current, opts, drag_src, drop_at)}
                        }
                    }
                }
            }
        }
    }
}

#[component]
pub fn DestinationsPane() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let Some((dests, selected)) = st.read_active(|t| (t.doc.named_destinations(), t.selected_dest.clone()))
    else {
        return rsx! {};
    };
    let show_page = option_on(st, "pdfNamedDestsView.hdr.page");
    let show_type = option_on(st, "pdfNamedDestsView.hdr.type");
    let show_details = option_on(st, "pdfNamedDestsView.hdr.details");
    let wrap = option_on(st, "pdfNamedDestsView.wrapTitles");
    let has_sel = selected
        .as_ref()
        .map(|s| dests.iter().any(|d| &d.name == s))
        .unwrap_or(false);
    let any_invalid = dests.iter().any(|d| d.page.is_none());
    let reg = registry();
    let label = |id: &str| {
        reg.get(id)
            .map(|c| cmd_label(lang, c))
            .unwrap_or_else(|| id.to_string())
    };
    let tools: [(&str, &str, bool); 6] = [
        ("pdfNamedDestsView.new", "insert", true),
        ("pdfNamedDestsView.edit", "pencil", has_sel),
        ("pdfNamedDestsView.goto", "next", has_sel),
        ("pdfNamedDestsView.set", "select", has_sel),
        ("pdfNamedDestsView.removeInvalid", "close", any_invalid),
        ("pdfNamedDestsView.options", "settings", true),
    ];
    let sel_del = selected.clone();
    rsx! {
        div { class: "pane-toolbar icon-toolbar",
            for (id, icon, enabled) in tools {
                button { class: "ibtn", key: "{id}", title: "{label(id)}", disabled: !enabled, onclick: move |_| dispatch(st, id), Icon { name: icon } }
            }
            button { class: "ibtn danger", title: "{t(lang, \"label.delete\")}", disabled: !has_sel,
                onclick: move |_| {
                    if let Some(n) = sel_del.clone() {
                        if let Some(Err(e)) = st.with_active(|t| t.doc.delete_named_destinations(&[n])) { st.error(e); }
                        st.with_active(|t| t.selected_dest = None);
                    }
                },
                Icon { name: "delete" }
            }
        }
        if dests.is_empty() { div { class: "pane-empty", "{t(lang, \"label.none\")}" } }
        div { class: if wrap { "tree dest-list wrap" } else { "tree dest-list" },
            for d in dests {
                {
                    let name = d.name.clone();
                    let name_dbl = d.name.clone();
                    let is_sel = selected.as_deref() == Some(d.name.as_str());
                    let invalid = d.page.is_none();
                    let class = match (is_sel, invalid) {
                        (true, true) => "tree-item selected invalid",
                        (true, false) => "tree-item selected",
                        (false, true) => "tree-item invalid",
                        (false, false) => "tree-item",
                    };
                    let kind = d.view.as_ref().map(|v| v.describe().split(' ').next().unwrap_or_default().to_string()).unwrap_or_default();
                    let details = d.view.as_ref().map(|v| v.describe()).unwrap_or_default();
                    let page = d.page;
                    let view = d.view.clone();
                    rsx! {
                        div { class: "{class}", key: "{d.name}", title: "{details}",
                            onclick: move |_| {
                                st.with_active(|t| t.selected_dest = Some(name.clone()));
                                if let Some(p) = page { st.go_to_dest(p, view.clone()); }
                            },
                            ondoubleclick: move |_| st.set_dialog(Dialog::NamedDestEdit(name_dbl.clone())),
                            Icon { name: "star" }
                            span { class: "tree-label", "{d.name}" }
                            if show_type && !kind.is_empty() { span { class: "chip", "{kind}" } }
                            if show_details && !details.is_empty() { span { class: "muted small", "{details}" } }
                            if invalid { span { class: "chip warn", "!" } }
                            if show_page { if let Some(p) = page { span { class: "page-ref", "{p + 1}" } } }
                        }
                    }
                }
            }
        }
    }
}
