//! Left pane strip and pane contents.

use dioxus::html::input_data::MouseButton;
use dioxus::html::{InteractionElementOffset, InteractionLocation, ModifiersInteraction, PointerInteraction};
use dioxus::prelude::*;
use folitecta_core::commands::PANES;
use folitecta_core::{Command, OutlineItem, SearchOptions, registry};

use crate::commands::{dispatch, option_on};
use crate::files;
use crate::i18n::{cmd_label, t, tf, ui_label};
use crate::icons::Icon;
use crate::state::{AppState, Dialog, Pane, ToastKind};
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
    let found = st.with_active(|t| {
        let opts = SearchOptions {
            case_sensitive: t.search.case_sensitive,
            whole_words: t.search.whole_words,
        };
        t.search.query = q.clone();
        t.search.hits = if q.trim().is_empty() {
            Vec::new()
        } else {
            t.doc.search(&q, opts)
        };
        t.search.current = 0;
        t.search.hits.len()
    });
    match found {
        Some(0) if !q.trim().is_empty() => st.toast(ToastKind::Info, t(st.lang(), "toast.not_found"), ""),
        Some(n) if n > 0 => reveal_hit(st, 0),
        _ => {}
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
                            rsx! {
                                div { class: "menu-item", key: "{id}",
                                    onclick: move |_| { let mut ui = st.ui_mut(); ui.show_left_pane = true; ui.left_pane = id_s.clone(); drop(ui); st.set_open_dropdown(None); st.save_prefs(); },
                                    span { class: "label", "{shown}" }
                                    span { class: "badge planned", "{t(lang, \"label.planned\").to_lowercase()}" }
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
    let body = if !has_doc {
        rsx! { div { class: "pane-empty", "{t(lang, \"toast.no_document\")}" } }
    } else {
        match &pane {
            Pane::Thumbnails => rsx! { ThumbnailsPane {} },
            Pane::Bookmarks => rsx! { BookmarksPane {} },
            Pane::Comments => rsx! { crate::ui::comments_pane::CommentsPane {} },
            Pane::Attachments => rsx! { AttachmentsPane {} },
            Pane::Layers => rsx! { LayersPane {} },
            Pane::Fields => rsx! { FieldsPane {} },
            Pane::Links => rsx! { LinksPane {} },
            Pane::Destinations => rsx! { DestinationsPane {} },
            Pane::Search => rsx! { SearchPane {} },
            Pane::Signatures => rsx! { SignaturesPane {} },
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

// ---- Bookmarks --------------------------------------------------------------------

fn outline_nodes(
    st: AppState,
    items: &[OutlineItem],
    path: Vec<usize>,
    expanded: &std::collections::HashSet<String>,
    selected: &Option<Vec<usize>>,
    selected_sig: Signal<Option<Vec<usize>>>,
    show_icons: bool,
) -> Element {
    let mut selected_sig = selected_sig;
    rsx! {
        for (i, item) in items.iter().enumerate() {
            {
                let mut p = path.clone();
                p.push(i);
                let key = p.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(".");
                let is_open = expanded.contains("*") || expanded.contains(&key);
                let has_children = !item.children.is_empty();
                let depth = p.len() - 1;
                let is_sel = selected.as_ref() == Some(&p);
                let p_click = p.clone();
                let p_dbl = p.clone();
                let key_t = key.clone();
                let page = item.page;
                let title = item.title.clone();
                let children = item.children.clone();
                rsx! {
                    div { key: "{key}",
                        div { class: if is_sel { "tree-item selected" } else { "tree-item" }, style: "padding-left: {6 + depth * 14}px",
                            onclick: move |_| { selected_sig.set(Some(p_click.clone())); if let Some(pg) = page { st.go_to_page(pg); } },
                            ondoubleclick: move |_| st.set_dialog(Dialog::BookmarkRename(p_dbl.clone())),
                            span { class: "twisty", onclick: move |e| { e.stop_propagation(); st.with_active(|t| { if !t.expanded_bookmarks.remove(&key_t) { t.expanded_bookmarks.insert(key_t.clone()); } t.expanded_bookmarks.remove("*"); }); },
                                if has_children { if is_open { "▾" } else { "▸" } } else { "" }
                            }
                            if show_icons { Icon { name: "bookmark" } }
                            span { class: "tree-label", "{title}" }
                            if let Some(pg) = page { span { class: "page-ref", "{pg + 1}" } }
                        }
                        if has_children && is_open {
                            {outline_nodes(st, &children, p.clone(), expanded, selected, selected_sig, show_icons)}
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn BookmarksPane() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let mut selected: Signal<Option<Vec<usize>>> = use_signal(|| None);
    let Some((items, expanded)) = st.read_active(|t| (t.doc.outline(), t.expanded_bookmarks.clone())) else {
        return rsx! {};
    };
    let sel = selected();
    let sel_del = sel.clone();
    let sel_ren = sel.clone();
    let show_icons = option_on(st, "bookmarksView.showIcons");
    let wrap = option_on(st, "bookmarksView.wrapTitles");
    rsx! {
        div { class: "pane-toolbar",
            button { class: "tbtn primary", title: "{t(lang, \"cmd.bookmarksView.new\")}", onclick: move |_| st.set_dialog(Dialog::BookmarkNew), Icon { name: "insert" } "{t(lang, \"cmd.bookmarksView.new\")}" }
            button { class: "tbtn", disabled: sel.is_none(), onclick: move |_| { if let Some(p) = sel_ren.clone() { st.set_dialog(Dialog::BookmarkRename(p)); } }, "{t(lang, \"cmd.rename\")}" }
            button { class: "tbtn danger", title: "{t(lang, \"label.delete\")}", disabled: sel.is_none(), onclick: move |_| { if let Some(p) = sel_del.clone() { if let Some(Err(e)) = st.with_active(|t| t.doc.delete_bookmark(&p)) { st.error(e); } selected.set(None); } }, Icon { name: "delete" } }
            button { class: "tbtn", title: "{t(lang, \"pane.bookmarks.from_pages\")}", onclick: move |_| { if let Some(Err(e)) = st.with_active(|t| t.doc.bookmarks_from_pages()) { st.error(e); } }, "{t(lang, \"pane.bookmarks.from_pages\")}" }
        }
        if items.is_empty() {
            div { class: "pane-empty", "{t(lang, \"label.none\")}" }
        }
        div { class: if wrap { "tree wrap" } else { "tree" }, {outline_nodes(st, &items, Vec::new(), &expanded, &sel, selected, show_icons)} }
    }
}

// ---- Attachments, layers, fields, links, destinations, signatures ------------------

#[component]
fn AttachmentsPane() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let Some(atts) = st.read_active(|t| t.doc.attachments()) else {
        return rsx! {};
    };
    rsx! {
        if atts.is_empty() { div { class: "pane-empty", "{t(lang, \"label.none\")}" } }
        for att in atts {
            {
                let a_save = att.clone();
                let a_rm = att.clone();
                let size = att.size.map(crate::util::human_size).unwrap_or_default();
                rsx! {
                    div { class: "list-item", key: "{att.name}-{att.stream.0}",
                        div { class: "title", Icon { name: "attachment" } span { class: "ellipsis", "{att.name}" } }
                        div { class: "meta", span { "{size}" } span { "{att.mime}" } span { "{att.modified}" } if let Some(p) = att.page { span { "{t(lang, \"status.page\")} {p + 1}" } } }
                        if !att.description.is_empty() { div { class: "body", "{att.description}" } }
                        div { class: "row", style: "gap:6px",
                            button { class: "tbtn", onclick: move |_| files::save_attachment(st, a_save.clone()), Icon { name: "save" } "{t(lang, \"cmd.attachmentsView.save\")}" }
                            if att.key.is_some() {
                                button { class: "tbtn danger", title: "{t(lang, \"label.delete\")}", onclick: move |_| { if let Some(Err(e)) = st.with_active(|t| t.doc.remove_attachment(&a_rm)) { st.error(e); } }, Icon { name: "delete" } }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn LayersPane() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let Some(layers) = st.read_active(|t| t.doc.layers()) else {
        return rsx! {};
    };
    rsx! {
        if layers.is_empty() { div { class: "pane-empty", "{t(lang, \"label.none\")}" } }
        for layer in layers {
            {
                let id = layer.id;
                let visible = layer.visible;
                rsx! {
                    label { class: "check layer", key: "{id.0}",
                        input { r#type: "checkbox", checked: visible, onchange: move |_| { if let Some(Err(e)) = st.with_active(|t| t.doc.set_layer_visible(id, !visible)) { st.error(e); } } }
                        span { class: "ellipsis", "{layer.name}" }
                        if layer.locked { Icon { name: "lock" } }
                    }
                }
            }
        }
    }
}

#[component]
fn FieldsPane() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let Some(fields) = st.read_active(|t| t.doc.form_fields()) else {
        return rsx! {};
    };
    rsx! {
        if fields.is_empty() { div { class: "pane-empty", "{t(lang, \"label.none\")}" } }
        for f in fields {
            {
                let id = f.id;
                let name = f.name.clone();
                let value = f.value.clone();
                let kind = f.kind.clone();
                let is_check = matches!(kind, folitecta_core::FieldKind::CheckBox | folitecta_core::FieldKind::RadioButton);
                let editable = matches!(kind, folitecta_core::FieldKind::Text | folitecta_core::FieldKind::ComboBox | folitecta_core::FieldKind::ListBox);
                let checked = !value.is_empty() && value != "Off";
                let page = f.page;
                rsx! {
                    div { class: "list-item", key: "{id.0}-{id.1}",
                        onclick: move |_| {
                            if let Some(p) = page {
                                st.go_to_page(p);
                            }
                            if editable {
                                st.set_dialog(Dialog::FieldValue { id, name: name.clone(), value: value.clone() });
                            }
                        },
                        div { class: "title", Icon { name: "field" } span { class: "ellipsis", "{f.name}" } span { class: "chip", "{kind.label()}" } }
                        div { class: "meta",
                            if let Some(p) = page { span { "{t(lang, \"status.page\")} {p + 1}" } }
                            if f.required { span { "{t(lang, \"label.required\")}" } }
                            if f.read_only { span { "{t(lang, \"label.read_only\")}" } }
                        }
                        if is_check {
                            label { class: "check", onclick: move |e| e.stop_propagation(),
                                input { r#type: "checkbox", checked: checked, onchange: move |_| { let v = if checked { "Off" } else { "Yes" }; if let Some(Err(e)) = st.with_active(|t| t.doc.set_field_value(id, v)) { st.error(e); } } }
                                span { "{f.value}" }
                            }
                        } else if !f.value.is_empty() {
                            div { class: "body", "{f.value}" }
                        }
                    }
                }
            }
        }
    }
}

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

#[component]
fn DestinationsPane() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let Some(dests) = st.read_active(|t| t.doc.named_destinations()) else {
        return rsx! {};
    };
    rsx! {
        if dests.is_empty() { div { class: "pane-empty", "{t(lang, \"label.none\")}" } }
        for d in dests {
            {
                let page = d.page;
                rsx! {
                    div { class: "tree-item", key: "{d.name}", onclick: move |_| { if let Some(p) = page { st.go_to_page(p); } },
                        Icon { name: "star" } span { class: "tree-label", "{d.name}" }
                        if let Some(p) = page { span { class: "page-ref", "{p + 1}" } }
                    }
                }
            }
        }
    }
}

#[component]
fn SignaturesPane() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let Some(sigs) = st.read_active(|t| t.doc.signatures()) else {
        return rsx! {};
    };
    rsx! {
        if sigs.is_empty() { div { class: "pane-empty", "{t(lang, \"label.none\")}" } }
        for s in sigs {
            {
                let page = s.page;
                rsx! {
                    div { class: "list-item", key: "{s.field}", onclick: move |_| { if let Some(p) = page { st.go_to_page(p); } },
                        div { class: "title", Icon { name: "signature" } span { class: "ellipsis", "{s.field}" } span { class: if s.signed { "badge ok" } else { "badge planned" }, if s.signed { "{t(lang, \"label.signed\")}" } else { "{t(lang, \"label.unsigned\")}" } } }
                        if s.signed {
                            div { class: "meta", span { "{s.signer}" } span { "{s.date}" } span { "{s.filter}" } }
                            if !s.reason.is_empty() { div { class: "body", "{s.reason}" } }
                        }
                    }
                }
            }
        }
    }
}

// ---- Search pane ------------------------------------------------------------------

#[component]
fn SearchPane() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let Some(search) = st.read_active(|t| t.search.clone()) else {
        return rsx! {};
    };
    let mut query = use_signal(|| search.query.clone());
    let hits = search.hits.clone();
    let n = hits.len();
    let results_label = tf(lang, "search.results", &[("count", &n.to_string())]);
    let show_page = option_on(st, "searchView.results.showPageNum");
    let wrap = option_on(st, "searchView.results.wrapText");
    let mark_open = (st.open_dropdown)().as_deref() == Some("search:mark");
    let (mx, my) = (st.dropdown_pos)();
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
            label { class: "check", input { r#type: "checkbox", checked: search.case_sensitive, onchange: move |_| { dispatch(st, "searchView.caseSensitive"); let q = query(); if !q.trim().is_empty() { run_search(st, &q); } } } span { "{t(lang, \"search.case\")}" } }
            label { class: "check", input { r#type: "checkbox", checked: search.whole_words, onchange: move |_| { dispatch(st, "searchView.wholeWords"); let q = query(); if !q.trim().is_empty() { run_search(st, &q); } } } span { "{t(lang, \"search.whole\")}" } }
        }
        if n > 0 {
            div { class: "search-nav",
                span { class: "muted ellipsis", "{results_label}" }
                div { class: "spacer" }
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
        div { class: "search-results",
            for (i, h) in hits.into_iter().enumerate() {
                {
                    let page = h.page;
                    let class = match (i == search.current, wrap) {
                        (true, true) => "list-item selected wrap",
                        (true, false) => "list-item selected",
                        (false, true) => "list-item wrap",
                        (false, false) => "list-item",
                    };
                    rsx! {
                        div { class: "{class}", key: "{page}-{h.index}-{h.start}",
                            onclick: move |_| reveal_hit(st, i),
                            if show_page { div { class: "title", "{t(lang, \"status.page\")} {page + 1}" } }
                            div { class: "body snippet", "{h.snippet}" }
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
