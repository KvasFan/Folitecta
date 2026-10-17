//! Left pane strip and pane contents.

use dioxus::html::input_data::MouseButton;
use dioxus::html::{InteractionElementOffset, InteractionLocation, ModifiersInteraction, PointerInteraction};
use dioxus::prelude::*;
use folitecta_core::commands::PANES;
use folitecta_core::search::FoundIn;
use folitecta_core::{Command, registry};

use crate::commands::{dispatch, option_on};
use crate::files;
use crate::i18n::{cmd_label, t, tf, ui_label};
use crate::icons::Icon;
use crate::state::{AppState, Pane, ToastKind};
use crate::ui::{MENU_W, status_badge};

fn pane_icon(p: &Pane) -> &'static str {
    match p {
        Pane::Thumbnails => "pages",
        Pane::Bookmarks => "bookmark",
        Pane::Comments => "comment",
        Pane::Attachments => "attachment",
        Pane::Layers => "layers",
        Pane::Fields => "field",
        Pane::Links => "link",
        Pane::Destinations => "star",
        Pane::Search => "search",
        Pane::Signatures => "signature",
        Pane::History => "history",
        Pane::Sessions => "window",
        Pane::Other(_) => "grid",
    }
}

fn pane_label(st: AppState, p: &Pane) -> String {
    let fallback = PANES
        .iter()
        .find(|(id, _)| *id == p.id())
        .map(|(_, l)| *l)
        .unwrap_or("Pane");
    ui_label(st.lang(), "pane", &p.id(), fallback)
}

// ---- Search -------------------------------------------------------------------

/// Search the active document and show the first result.
pub fn run_search(st: AppState, query: &str) {
    let q = query.to_string();
    let (opts, proximity, scopes) = crate::commands::search::search_settings(st);
    let found = st.with_active(|t| {
        t.search.query = q.clone();
        let empty = q.trim().is_empty();
        t.search.hits = if empty || !scopes.page_text {
            Vec::new()
        } else {
            t.doc.search_words(&q, opts, proximity)
        };
        t.search.other = if empty {
            Vec::new()
        } else {
            t.doc.search_other(&q, opts, proximity, &scopes)
        };
        t.search.current = 0;
        t.search.collapsed.clear();
        (t.search.hits.len(), t.search.other.len())
    });
    match found {
        Some((0, 0)) if !q.trim().is_empty() => {
            st.toast(ToastKind::Info, t(st.lang(), "toast.not_found"), "")
        }
        Some((n, _)) if n > 0 => reveal_hit(st, 0),
        _ => {}
    }
}

/// Result group of a match outside the page text.
pub fn other_group(f: &FoundIn) -> &'static str {
    match f {
        FoundIn::Comment { .. } => "comments",
        FoundIn::Bookmark { .. } => "bookmarks",
        FoundIn::Field { .. } => "fields",
        FoundIn::DocInfo { .. } => "docinfo",
        FoundIn::Attachment { .. } | FoundIn::AttachmentPage { .. } => "attachments",
        FoundIn::Link { .. } => "links",
    }
}

fn other_icon(f: &FoundIn) -> &'static str {
    match f {
        FoundIn::Comment { .. } => "comment",
        FoundIn::Bookmark { .. } => "bookmark",
        FoundIn::Field { .. } => "field",
        FoundIn::DocInfo { .. } => "info",
        FoundIn::Attachment { .. } | FoundIn::AttachmentPage { .. } => "attachment",
        FoundIn::Link { .. } => "link",
    }
}

/// Where a match outside the page text is, for its result line.
fn other_place(lang: crate::i18n::Lang, f: &FoundIn, types: bool) -> String {
    let page = |p: usize| format!("{} {}", t(lang, "status.page"), p + 1);
    match f {
        FoundIn::Comment { page: p, subtype, .. } => {
            if types {
                format!("{} · {}", crate::i18n::annot_name(lang, subtype), page(*p))
            } else {
                page(*p)
            }
        }
        FoundIn::Bookmark { page: p, .. } => p.map(page).unwrap_or_default(),
        FoundIn::Field { name, .. } => name.clone(),
        FoundIn::DocInfo { key } => ui_label(lang, "docinfo", key, key),
        FoundIn::Attachment { name } => name.clone(),
        FoundIn::AttachmentPage { name, page: p } => format!("{name} · {}", page(*p)),
        FoundIn::Link { page: p, .. } => page(*p),
    }
}

/// Make search result `i` current and scroll it into view.
pub fn reveal_hit(st: AppState, i: usize) {
    let page = st
        .with_active(|t| {
            t.search.current = i.min(t.search.hits.len().saturating_sub(1));
            t.search.hits.get(t.search.current).map(|h| h.page)
        })
        .flatten();
    let Some(page) = page else { return };
    if st.current_page() != page {
        st.navigate(page, true);
    }
    spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(80)).await;
        let _ = document::eval("const h = document.querySelector('#canvas .hit.current'); if (h) { h.scrollIntoView({ block: 'center', inline: 'nearest' }); }").await;
    });
}

/// Go to the next or previous search result (searching first when needed).
pub fn step_search(st: AppState, forward: bool) {
    let (n, cur, query) = st
        .read_active(|t| (t.search.hits.len(), t.search.current, t.search.query.clone()))
        .unwrap_or((0, 0, String::new()));
    if n == 0 {
        if query.trim().is_empty() {
            crate::commands::show_pane(st, Pane::Search);
        } else {
            run_search(st, &query);
        }
        return;
    }
    let next = if forward { (cur + 1) % n } else { (cur + n - 1) % n };
    reveal_hit(st, next);
}

// ---- Strip ----------------------------------------------------------------------

#[component]
pub fn PaneStrip() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let (show, current, ribbon) = {
        let ui = st.ui.read();
        (ui.show_left_pane, ui.left_pane.clone(), ui.ribbon)
    };
    let more_open = (st.open_dropdown)().as_deref() == Some("panes:more");
    // Main pane buttons that fit the strip height; the rest move to the "more" menu.
    let (_, vh) = (st.viewport)();
    let chrome = 31.0 + if ribbon { 132.0 } else { 40.0 } + 30.0 + 14.0;
    let slots =
        (((vh - chrome - 6.0) / 36.0).floor() as isize - 2).clamp(3, Pane::MAIN.len() as isize) as usize;
    let (shown, hidden) = Pane::MAIN.split_at(slots);
    let others: Vec<(&str, &str)> = PANES
        .iter()
        .filter(|(id, _)| !Pane::MAIN.iter().any(|m| m.id() == *id) && *id != "properties")
        .copied()
        .collect();
    let show_right = st.ui.read().show_right_pane;
    let (mx, my) = (st.dropdown_pos)();
    let max_h = ((st.viewport)().1 - my - 8.0).max(160.0);
    let hidden_active = hidden.iter().any(|p| show && p.id() == current);
    rsx! {
        div { class: "pane-strip", onclick: move |e| e.stop_propagation(),
            for pane in shown.iter() {
                {
                    let id = pane.id();
                    let active = show && current == id;
                    let label = pane_label(st, pane);
                    let id_click = id.clone();
                    rsx! {
                        button { class: if active { "strip-btn active" } else { "strip-btn" }, key: "{id}", title: "{label}",
                            onclick: move |_| {
                                let mut ui = st.ui_mut();
                                if ui.show_left_pane && ui.left_pane == id_click { ui.show_left_pane = false; } else { ui.show_left_pane = true; ui.left_pane = id_click.clone(); }
                                drop(ui);
                                st.save_prefs();
                            },
                            Icon { name: pane_icon(pane) }
                        }
                    }
                }
            }
            button { class: if more_open || hidden_active { "strip-btn active" } else { "strip-btn" }, title: "{t(lang, \"pane.more\")}",
                onclick: move |evt| {
                    let cur = st.open_dropdown.peek().clone();
                    if cur.as_deref() == Some("panes:more") {
                        st.set_open_dropdown(None);
                    } else {
                        let c = evt.client_coordinates();
                        let o = evt.element_coordinates();
                        let (left, top) = (c.x - o.x + 40.0, c.y - o.y);
                        let pos = st.clamp_menu_pos(left, top, MENU_W, 260.0);
                        st.set_dropdown_pos(pos);
                        st.set_open_dropdown(Some("panes:more".into()));
                    }
                },
                Icon { name: "more" }
            }
            if more_open {
                div { class: "rbtn-menu fixed", style: "left:{mx}px;top:{my}px;max-height:{max_h}px",
                    for pane in hidden.iter() {
                        {
                            let id_s = pane.id();
                            let label = pane_label(st, pane);
                            let active = show && current == id_s;
                            rsx! {
                                div { class: if active { "menu-item open" } else { "menu-item" }, key: "{id_s}",
                                    onclick: move |_| { let mut ui = st.ui_mut(); ui.show_left_pane = true; ui.left_pane = id_s.clone(); drop(ui); st.set_open_dropdown(None); st.save_prefs(); },
                                    Icon { name: pane_icon(pane) }
                                    span { class: "label", "{label}" }
                                }
                            }
                        }
                    }
                    if !hidden.is_empty() { div { class: "menu-sep" } }
                    for (id, label) in others {
                        {
                            let id_s = id.to_string();
                            let shown = ui_label(lang, "pane", id, label);
                            let pane = Pane::from_id(id);
                            let planned = matches!(pane, Pane::Other(_));
                            rsx! {
                                div { class: "menu-item", key: "{id}",
                                    onclick: move |_| { let mut ui = st.ui_mut(); ui.show_left_pane = true; ui.left_pane = id_s.clone(); drop(ui); st.set_open_dropdown(None); st.save_prefs(); },
                                    if !planned { Icon { name: pane_icon(&pane) } }
                                    span { class: "label", "{shown}" }
                                    if planned { span { class: "badge planned", "{t(lang, \"label.planned\").to_lowercase()}" } }
                                }
                            }
                        }
                    }
                }
            }
            div { class: "strip-spacer" }
            button { class: if show_right { "strip-btn active" } else { "strip-btn" }, title: "{t(lang, \"pane.properties\")}",
                onclick: move |_| { let v = !st.ui.read().show_right_pane; st.ui_mut().show_right_pane = v; st.save_prefs(); },
                Icon { name: "info" }
            }
        }
    }
}

/// Registry commands of a pane, shown in the pane's options menu.
fn pane_commands(pane_id: &str) -> Vec<&'static Command> {
    registry().pane_commands(pane_id)
}

#[component]
pub fn PanePanel() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let (show, current) = {
        let ui = st.ui.read();
        (ui.show_left_pane, ui.left_pane.clone())
    };
    if !show {
        return rsx! { div { class: "pane-panel" } };
    }
    let pane = Pane::from_id(&current);
    let label = pane_label(st, &pane);
    let has_doc = st.has_doc();
    let menu_key = format!("pane-menu:{current}");
    let menu_open = (st.open_dropdown)().as_deref() == Some(menu_key.as_str());
    let cmds = if matches!(pane, Pane::Other(_)) {
        Vec::new()
    } else {
        pane_commands(&current)
    };
    let (mx, my) = (st.dropdown_pos)();
    let max_h = ((st.viewport)().1 - my - 8.0).max(160.0);
    // History and sessions work without a document.
    let body = if pane == Pane::History {
        rsx! { crate::ui::history_pane::HistoryPane {} }
    } else if pane == Pane::Sessions {
        rsx! { crate::ui::history_pane::SessionsPane {} }
    } else if !has_doc {
        rsx! { div { class: "pane-empty", "{t(lang, \"toast.no_document\")}" } }
    } else {
        match &pane {
            Pane::Thumbnails => rsx! { ThumbnailsPane {} },
            Pane::Bookmarks => rsx! { crate::ui::bookmarks_pane::BookmarksPane {} },
            Pane::Comments => rsx! { crate::ui::comments_pane::CommentsPane {} },
            Pane::Attachments => rsx! { crate::ui::attachments_pane::AttachmentsPane {} },
            Pane::Layers => rsx! { crate::ui::layers_pane::LayersPane {} },
            Pane::Fields => rsx! { crate::ui::fields_pane::FieldsPane {} },
            Pane::Links => rsx! { LinksPane {} },
            Pane::Destinations => rsx! { crate::ui::bookmarks_pane::DestinationsPane {} },
            Pane::Search => rsx! { SearchPane {} },
            Pane::Signatures => rsx! { crate::ui::signatures::SignaturesPane {} },
            Pane::History | Pane::Sessions => rsx! {},
            Pane::Other(id) => rsx! { PlannedPane { pane_id: id.clone() } },
        }
    };
    let key_click = menu_key.clone();
    rsx! {
        div { class: "pane-panel", onclick: move |e| e.stop_propagation(),
            div { class: "pane-header", Icon { name: pane_icon(&pane) } span { class: "pane-title", "{label}" } div { class: "spacer" }
                if has_doc && !cmds.is_empty() {
                    button { class: if menu_open { "ibtn active" } else { "ibtn" }, title: "{t(lang, \"pane.options\")}",
                        onclick: move |evt| {
                            if st.open_dropdown.peek().as_deref() == Some(key_click.as_str()) {
                                st.set_open_dropdown(None);
                            } else {
                                let c = evt.client_coordinates();
                                let o = evt.element_coordinates();
                                let pos = st.clamp_menu_pos(c.x - o.x, c.y - o.y + 28.0, MENU_W, 300.0);
                                st.set_dropdown_pos(pos);
                                st.submenus_mut().clear();
                                st.set_open_dropdown(Some(key_click.clone()));
                            }
                        },
                        Icon { name: "more" }
                    }
                }
                button { class: "ibtn", title: "{t(lang, \"dialog.close\")}", onclick: move |_| { st.ui_mut().show_left_pane = false; st.save_prefs(); }, "×" }
            }
            if menu_open {
                div { class: "rbtn-menu fixed", style: "left:{mx}px;top:{my}px;max-height:{max_h}px", onclick: move |e| e.stop_propagation(),
                    for c in cmds {
                        {crate::ui::menu_entry(st, lang, c, 0)}
                    }
                }
            }
            div { class: "pane-body", {body} }
        }
    }
}
// ---- Thumbnails -------------------------------------------------------------------

#[component]
fn ThumbnailsPane() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let mut drag_src = use_signal(|| None::<usize>);
    let mut drag_over = use_signal(|| None::<usize>);
    let mut dragged = use_signal(|| false);
    let Some((tab_id, rev, page, count, sizes, selected, rotation)) = st.read_active(|t| {
        (
            t.id,
            t.view_rev(),
            t.page,
            t.doc.page_count(),
            t.page_sizes.clone(),
            t.selected_pages.clone(),
            t.view_rotation,
        )
    }) else {
        return rsx! {};
    };
    let box_px: f32 = st
        .ui
        .read()
        .value("thumb.size")
        .and_then(|v| v.parse().ok())
        .unwrap_or(150.0f32)
        .clamp(80.0, 320.0);
    let logical = option_on(st, "thumbnailsView.logicalVisible");
    let labels = if logical {
        st.read_active(|t| t.doc.page_labels()).unwrap_or_default()
    } else {
        Vec::new()
    };
    let render_scale = ((box_px * 1.6 / 842.0) * 1000.0).round().clamp(120.0, 900.0) as u32;
    let mut finish_drag = move || {
        let (src, over) = (*drag_src.peek(), *drag_over.peek());
        drag_src.set(None);
        drag_over.set(None);
        if let (Some(s), Some(o)) = (src, over) {
            if s != o {
                let r = st.with_active(|t| {
                    let moving: Vec<usize> = if t.selected_pages.contains(&s) {
                        t.selected_pages.clone()
                    } else {
                        vec![s]
                    };
                    let before = if o > s { o + 1 } else { o };
                    let to = (0..before).filter(|i| !moving.contains(i)).count();
                    let first_new = to;
                    let n = moving.len();
                    let res = t.doc.move_pages(&moving, to);
                    if res.is_ok() {
                        t.selected_pages = (first_new..first_new + n).collect();
                        t.page = first_new;
                    }
                    res
                });
                match r {
                    Some(Err(e)) => st.error(e),
                    _ => files::set_window_title(st),
                }
            }
        }
    };
    rsx! {
        div { class: "thumbs", style: "--thumb:{box_px}px",
            onmouseleave: move |_| { drag_src.set(None); drag_over.set(None); },
            onmouseup: move |_| finish_drag(),
            for p in 0..count {
                {
                    let s = sizes.get(p).copied();
                    let (w, h) = s.map(|s| if rotation % 180 != 0 { (s.height, s.width) } else { (s.width, s.height) }).unwrap_or((595.0, 842.0));
                    let scale = box_px / w.max(h);
                    let (wpx, hpx) = ((w * scale).round() as i32, (h * scale).round() as i32);
                    let is_sel = selected.contains(&p);
                    let is_target = drag_over() == Some(p) && drag_src() != Some(p);
                    let class = format!(
                        "thumb{}{}{}",
                        if p == page { " current" } else { "" },
                        if is_sel { " selected" } else { "" },
                        if is_target { if drag_src().map(|s| s < p).unwrap_or(false) { " drop-after" } else { " drop-before" } } else { "" }
                    );
                    let src = format!("/page/{tab_id}/{p}/{rev}/{render_scale}.png");
                    let label = if logical { labels.get(p).cloned().unwrap_or_else(|| (p + 1).to_string()) } else { (p + 1).to_string() };
                    let img_style = if rotation != 0 { format!("width:{wpx}px;height:{hpx}px;transform:rotate({rotation}deg)") } else { format!("width:{wpx}px;height:{hpx}px") };
                    let (iw, ih) = if rotation % 180 != 0 { (hpx, wpx) } else { (wpx, hpx) };
                    rsx! {
                        div { class: "{class}", key: "{p}", title: "{t(lang, \"status.page\")} {p + 1}",
                            onmousedown: move |e| {
                                if e.trigger_button() == Some(MouseButton::Primary) {
                                    drag_src.set(Some(p));
                                    dragged.set(false);
                                }
                            },
                            onmouseenter: move |_| {
                                if drag_src.peek().is_some() {
                                    drag_over.set(Some(p));
                                    dragged.set(true);
                                }
                            },
                            onclick: move |e| {
                                if *dragged.peek() {
                                    dragged.set(false);
                                    return;
                                }
                                if e.modifiers().ctrl() || e.modifiers().meta() {
                                    st.with_active(|t| { if let Some(i) = t.selected_pages.iter().position(|x| *x == p) { t.selected_pages.remove(i); } else { t.selected_pages.push(p); } });
                                } else if e.modifiers().shift() {
                                    st.with_active(|t| { let (a, b) = (t.page.min(p), t.page.max(p)); t.selected_pages = (a..=b).collect(); });
                                } else {
                                    st.with_active(|t| t.selected_pages = vec![p]);
                                    st.go_to_page(p);
                                }
                            },
                            ondoubleclick: move |_| st.go_to_page(p),
                            div { class: "thumb-img", style: "width:{wpx.max(iw)}px;height:{hpx.max(ih)}px",
                                img { src: "{src}", alt: "", loading: "lazy", draggable: "false", width: "{wpx}", height: "{hpx}", style: "{img_style}" }
                            }
                            span { class: "num", "{label}" }
                        }
                    }
                }
            }
        }
    }
}

// ---- Attachments, layers, fields, links, destinations, signatures ------------------

#[component]
fn LinksPane() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let Some(links) = st.read_active(|t| t.doc.all_links()) else {
        return rsx! {};
    };
    rsx! {
        if links.is_empty() { div { class: "pane-empty", "{t(lang, \"label.none\")}" } }
        for l in links {
            {
                let target = l.target.clone();
                let page = l.page;
                let desc = target.describe();
                rsx! {
                    div { class: "list-item", key: "{page}-{l.index}",
                        onclick: move |_| {
                            match &target {
                                folitecta_core::LinkTarget::Uri(u) => { if let Err(e) = crate::util::open_external(u) { st.error(e); } }
                                folitecta_core::LinkTarget::Page(p) => st.go_to_page(*p),
                                _ => st.go_to_page(page),
                            }
                        },
                        div { class: "title", Icon { name: "link" } "{t(lang, \"status.page\")} {page + 1}" }
                        div { class: "body ellipsis", title: "{desc}", "{desc}" }
                    }
                }
            }
        }
    }
}

// ---- Search pane ------------------------------------------------------------------

/// A row of the search results.
#[derive(Clone, PartialEq)]
enum ResultRow {
    Header {
        key: String,
        label: String,
        count: usize,
        open: bool,
        icon: &'static str,
    },
    Hit {
        i: usize,
        page: usize,
        snippet: String,
        context: String,
    },
    Other {
        i: usize,
        icon: &'static str,
        place: String,
        snippet: String,
    },
    File {
        i: usize,
        place: String,
        snippet: String,
    },
}

#[component]
fn SearchPane() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let Some((search, contexts)) = st.read_active(|t| {
        // Bookmark titles of the pages with hits.
        let mut pages: Vec<usize> = t.search.hits.iter().map(|h| h.page).collect();
        pages.dedup();
        let contexts: std::collections::HashMap<usize, String> =
            if option_on(st, "searchView.results.showContextBookmarks") {
                pages
                    .iter()
                    .map(|p| (*p, t.doc.bookmark_title_for_page(*p)))
                    .collect()
            } else {
                Default::default()
            };
        (t.search.clone(), contexts)
    }) else {
        return rsx! {};
    };
    let mut query = use_signal(|| search.query.clone());
    // A search started elsewhere (menus, other panes) shows its query here.
    let searched = search.query.clone();
    use_effect(use_reactive!(|searched| {
        if *query.peek() != searched {
            query.set(searched);
        }
    }));
    let mut more = use_signal(|| false);
    let n = search.hits.len();
    let total = n + search.other.len();
    let results_label = tf(lang, "search.results", &[("count", &total.to_string())]);
    let show_page = option_on(st, "searchView.results.showPageNum");
    let wrap = option_on(st, "searchView.results.wrapText");
    let icons = option_on(st, "searchView.results.showIcon");
    let types = option_on(st, "searchView.results.showCommentTypes");
    let mark_open = (st.open_dropdown)().as_deref() == Some("search:mark");
    let (mx, my) = (st.dropdown_pos)();
    let reg = registry();
    let label = |id: &str| {
        reg.get(id)
            .map(|c| cmd_label(lang, c))
            .unwrap_or_else(|| id.to_string())
    };
    let checked = |id: &str| crate::commands::toggle_state(st, id).unwrap_or(false);
    let prox = st.ui.read().value("search.proximity").unwrap_or("").to_string();

    // Result rows, grouped by page, by place and by file.
    let mut rows: Vec<ResultRow> = Vec::new();
    let mut i = 0;
    while i < n {
        let page = search.hits[i].page;
        let j = (i..n).find(|k| search.hits[*k].page != page).unwrap_or(n);
        let key = format!("p{page}");
        let open = !search.collapsed.contains(&key);
        rows.push(ResultRow::Header {
            key,
            label: format!("{} {}", t(lang, "status.page"), page + 1),
            count: j - i,
            open,
            icon: "page",
        });
        if open {
            for k in i..j {
                let h = &search.hits[k];
                rows.push(ResultRow::Hit {
                    i: k,
                    page,
                    snippet: h.snippet.clone(),
                    context: contexts.get(&page).cloned().unwrap_or_default(),
                });
            }
        }
        i = j;
    }
    for group in [
        "comments",
        "bookmarks",
        "fields",
        "docinfo",
        "attachments",
        "links",
    ] {
        let items: Vec<usize> = (0..search.other.len())
            .filter(|k| other_group(&search.other[*k].found_in) == group)
            .collect();
        if items.is_empty() {
            continue;
        }
        let open = !search.collapsed.contains(group);
        rows.push(ResultRow::Header {
            key: group.to_string(),
            label: t(lang, &format!("search.group.{group}")),
            count: items.len(),
            open,
            icon: other_icon(&search.other[items[0]].found_in),
        });
        if open {
            for k in items {
                let o = &search.other[k];
                rows.push(ResultRow::Other {
                    i: k,
                    icon: other_icon(&o.found_in),
                    place: other_place(lang, &o.found_in, types),
                    snippet: o.snippet.clone(),
                });
            }
        }
    }
    let folder = search.folder.clone();
    if let Some(f) = &folder {
        let mut k = 0;
        while k < f.hits.len() {
            let path = f.hits[k].path.clone();
            let j = (k..f.hits.len())
                .find(|x| f.hits[*x].path != path)
                .unwrap_or(f.hits.len());
            let key = path.display().to_string();
            let open = !search.collapsed.contains(&key);
            let name = path
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            rows.push(ResultRow::Header {
                key,
                label: name,
                count: j - k,
                open,
                icon: "open",
            });
            if open {
                for x in k..j {
                    rows.push(ResultRow::File {
                        i: x,
                        place: format!("{} {}", t(lang, "status.page"), f.hits[x].page + 1),
                        snippet: f.hits[x].snippet.clone(),
                    });
                }
            }
            k = j;
        }
    }
    let folder_status = folder.as_ref().map(|f| {
        let key = if f.running {
            "search.folder.running"
        } else {
            "search.folder.done"
        };
        tf(
            lang,
            key,
            &[
                ("done", &f.searched.to_string()),
                ("files", &f.files.to_string()),
                ("count", &f.hits.len().to_string()),
                ("folder", &f.root.display().to_string()),
            ],
        )
    });
    let running = folder.as_ref().is_some_and(|f| f.running);
    let scope_ids = [
        "searchView.includePageText",
        "searchView.includeAnnotations",
        "searchView.includeBookmarks",
        "searchView.includeFormFields",
        "searchView.includeDocInfo",
        "searchView.includeAttachments",
        "searchView.includeExtLinks",
    ];
    let prox_opts = [
        ("", t(lang, "search.words.phrase")),
        ("adjacent", label("searchView.proximity.adjacent")),
        ("paragraph", label("searchView.proximity.samePara")),
        ("page", label("searchView.proximity.samePage")),
        ("document", label("searchView.proximity.sameDoc")),
    ];
    let more_label = if more() {
        t(lang, "search.fewer")
    } else {
        t(lang, "search.more")
    };
    rsx! {
        div { class: "search-box",
            input { placeholder: "{t(lang, \"search.placeholder\")}", value: "{query}", autofocus: true,
                oninput: move |e| query.set(e.value()),
                onkeydown: move |e| {
                    e.stop_propagation();
                    if e.key() == Key::Enter {
                        let q = query();
                        let same = st.read_active(|t| t.search.query == q && !t.search.hits.is_empty()).unwrap_or(false);
                        if same { step_search(st, !e.modifiers().shift()); } else { run_search(st, &q); }
                    } else if e.key() == Key::Escape {
                        query.set(String::new());
                        run_search(st, "");
                    }
                }
            }
            button { class: "tbtn primary", title: "{t(lang, \"search.go\")}", onclick: move |_| run_search(st, &query()), Icon { name: "search" } }
        }
        div { class: "search-opts",
            label { class: "check", input { r#type: "checkbox", checked: search.case_sensitive, onchange: move |_| { st.with_active(|t| t.search.query = query()); dispatch(st, "searchView.caseSensitive"); } } span { "{t(lang, \"search.case\")}" } }
            label { class: "check", input { r#type: "checkbox", checked: search.whole_words, onchange: move |_| { st.with_active(|t| t.search.query = query()); dispatch(st, "searchView.wholeWords"); } } span { "{t(lang, \"search.whole\")}" } }
            label { class: "check", input { r#type: "checkbox", checked: checked("searchView.ignoreDiacritics"), onchange: move |_| { st.with_active(|t| t.search.query = query()); dispatch(st, "searchView.ignoreDiacritics"); } } span { "{t(lang, \"search.diacritics\")}" } }
            button { class: "linkish", onclick: move |_| more.set(!more()), "{more_label}" }
        }
        if more() {
            div { class: "search-more",
                div { class: "search-more-title", "{t(lang, \"search.look_in\")}" }
                div { class: "search-scopes",
                    for id in scope_ids {
                        label { class: "check", key: "{id}",
                            input { r#type: "checkbox", checked: checked(id), onchange: move |_| { st.with_active(|t| t.search.query = query()); dispatch(st, id); } }
                            span { "{label(id)}" }
                        }
                    }
                }
                div { class: "field",
                    label { "{t(lang, \"search.words\")}" }
                    select {
                        onchange: move |e| {
                            let v = e.value();
                            st.with_active(|t| t.search.query = query());
                            st.set_value("search.proximity", v.as_str());
                            crate::commands::search::rerun(st);
                        },
                        for (v, text) in prox_opts {
                            option { key: "{v}", value: "{v}", selected: prox == v, "{text}" }
                        }
                    }
                }
                div { class: "search-more-title", "{t(lang, \"search.folder\")}" }
                div { class: "row", style: "gap:6px;flex-wrap:wrap",
                    button { class: "tbtn", disabled: running,
                        onclick: move |_| { st.with_active(|t| t.search.query = query()); crate::commands::search::search_folder(st); },
                        Icon { name: "open" } "{t(lang, \"search.folder.choose\")}"
                    }
                    if running {
                        button { class: "tbtn", onclick: move |_| { st.with_active(|t| { if let Some(f) = t.search.folder.as_mut() { f.running = false; } }); }, "{t(lang, \"search.folder.stop\")}" }
                    }
                }
                label { class: "check", input { r#type: "checkbox", checked: option_on(st, "searchView.lookInSubFolders"), onchange: move |_| dispatch(st, "searchView.lookInSubFolders") } span { "{label(\"searchView.lookInSubFolders\")}" } }
                label { class: "check", input { r#type: "checkbox", checked: option_on(st, "searchView.estimateFolder"), onchange: move |_| dispatch(st, "searchView.estimateFolder") } span { "{label(\"searchView.estimateFolder\")}" } }
            }
        }
        if total > 0 {
            div { class: "search-nav",
                span { class: "muted ellipsis", "{results_label}" }
                div { class: "spacer" }
                if n > 0 {
                    button { class: "ibtn", title: "{t(lang, \"search.prev\")}", onclick: move |_| step_search(st, false), Icon { name: "prev" } }
                    span { class: "muted", "{search.current + 1}/{n}" }
                    button { class: "ibtn", title: "{t(lang, \"search.next\")}", onclick: move |_| step_search(st, true), Icon { name: "next" } }
                    button { class: if mark_open { "ibtn active" } else { "ibtn" }, title: "{t(lang, \"search.mark\")}",
                        onclick: move |evt| {
                            evt.stop_propagation();
                            if st.open_dropdown.peek().as_deref() == Some("search:mark") {
                                st.set_open_dropdown(None);
                            } else {
                                let c = evt.client_coordinates();
                                let pos = st.clamp_menu_pos(c.x - 20.0, c.y + 16.0, MENU_W, 170.0);
                                st.set_dropdown_pos(pos);
                                st.set_open_dropdown(Some("search:mark".into()));
                            }
                        },
                        Icon { name: "highlight" }
                    }
                }
            }
        }
        if let Some(s) = folder_status {
            div { class: "search-folder-status muted", "{s}" }
        }
        if mark_open {
            div { class: "rbtn-menu fixed", style: "left:{mx}px;top:{my}px", onclick: move |e| e.stop_propagation(),
                for id in ["addAnnotsBySearchResult.highlight", "addAnnotsBySearchResult.underline", "addAnnotsBySearchResult.strikeout", "addAnnotsBySearchResult.redaction"] {
                    {
                        let label = registry().get(id).map(|c| cmd_label(lang, c)).unwrap_or_else(|| id.to_string());
                        rsx! { div { class: "menu-item", key: "{id}", onclick: move |_| { st.set_open_dropdown(None); dispatch(st, id); }, span { class: "label", "{label}" } } }
                    }
                }
            }
        }
        div { class: if wrap { "search-results wrap" } else { "search-results" },
            for (r, row) in rows.into_iter().enumerate() {
                match row {
                    ResultRow::Header { key, label, count, open, icon } => {
                        let k = key.clone();
                        rsx! {
                            div { class: "result-group", key: "g{r}-{key}",
                                onclick: move |_| { let k = k.clone(); st.with_active(|t| { if !t.search.collapsed.remove(&k) { t.search.collapsed.insert(k); } }); },
                                span { class: "twisty", if open { "▾" } else { "▸" } }
                                if icons { Icon { name: icon } }
                                span { class: "ellipsis", title: "{label}", "{label}" }
                                span { class: "chip", "{count}" }
                            }
                        }
                    }
                    ResultRow::Hit { i, page, snippet, context } => {
                        let class = if i == search.current { "list-item result selected" } else { "list-item result" };
                        rsx! {
                            div { class: "{class}", key: "h{i}",
                                onclick: move |_| reveal_hit(st, i),
                                if show_page || !context.is_empty() {
                                    div { class: "title",
                                        if show_page { span { "{t(lang, \"status.page\")} {page + 1}" } }
                                        if !context.is_empty() { span { class: "muted ellipsis", title: "{context}", "{context}" } }
                                    }
                                }
                                div { class: "body snippet", "{snippet}" }
                            }
                        }
                    }
                    ResultRow::Other { i, icon, place, snippet } => {
                        let hit = search.other[i].clone();
                        rsx! {
                            div { class: "list-item result", key: "o{i}",
                                onclick: move |_| crate::commands::search::open_other(st, hit.clone()),
                                div { class: "title",
                                    if icons { Icon { name: icon } }
                                    span { class: "ellipsis", title: "{place}", "{place}" }
                                }
                                div { class: "body snippet", "{snippet}" }
                            }
                        }
                    }
                    ResultRow::File { i, place, snippet } => {
                        let hit = folder.as_ref().and_then(|f| f.hits.get(i).cloned());
                        rsx! {
                            div { class: "list-item result", key: "f{i}",
                                onclick: move |_| { if let Some(h) = hit.clone() { crate::commands::search::open_folder_hit(st, h); } },
                                if show_page { div { class: "title", "{place}" } }
                                div { class: "body snippet", "{snippet}" }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn PlannedPane(pane_id: String) -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let reg = registry();
    let cmds = reg.pane_commands(&pane_id);
    rsx! {
        div { class: "callout", style: "margin: 6px", "{t(lang, \"pane.planned\")}" }
        for c in cmds {
            {
                let id = c.id.clone();
                let label = cmd_label(lang, c);
                rsx! {
                    div { class: "tree-item", key: "{c.id}", onclick: move |_| dispatch(st, &id),
                        span { class: "tree-label", "{label}" }
                        {status_badge(&c.id)}
                    }
                }
            }
        }
    }
}
