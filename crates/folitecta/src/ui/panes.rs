//! Left pane strip and pane contents.

use dioxus::html::ModifiersInteraction;
use dioxus::prelude::*;
use folitecta_core::commands::PANES;
use folitecta_core::{Command, OutlineItem, SearchOptions, registry};

use crate::commands::{dispatch, is_implemented};
use crate::files;
use crate::i18n::{cmd_label, t, ui_label};
use crate::icons::Icon;
use crate::state::{AppState, Dialog, Pane};
use crate::ui::status_badge;

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

#[component]
pub fn PaneStrip() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let (show, current) = {
        let ui = st.ui.read();
        (ui.show_left_pane, ui.left_pane.clone())
    };
    let more_open = (st.open_dropdown)().as_deref() == Some("panes:more");
    let others: Vec<(&str, &str)> = PANES
        .iter()
        .filter(|(id, _)| !Pane::MAIN.iter().any(|m| m.id() == *id) && *id != "properties")
        .copied()
        .collect();
    let show_right = st.ui.read().show_right_pane;
    rsx! {
        div { class: "pane-strip", onclick: move |e| e.stop_propagation(),
            for pane in Pane::MAIN.iter() {
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
            div { style: "position:relative",
                button { class: if more_open { "strip-btn active" } else { "strip-btn" }, title: "{t(lang, \"pane.more\")}",
                    onclick: move |_| { let cur = st.open_dropdown.peek().clone(); st.set_open_dropdown(if cur.as_deref() == Some("panes:more") { None } else { Some("panes:more".into()) }); },
                    Icon { name: "more" }
                }
                if more_open {
                    div { class: "rbtn-menu", style: "left: 40px; top: 0",
                        for (id, label) in others {
                            {
                                let id_s = id.to_string();
                                rsx! {
                                    div { class: "menu-item", key: "{id}",
                                        onclick: move |_| { let mut ui = st.ui_mut(); ui.show_left_pane = true; ui.left_pane = id_s.clone(); drop(ui); st.set_open_dropdown(None); st.save_prefs(); },
                                        span { class: "label", "{label}" }
                                        span { class: "badge planned", "planned" }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            div { style: "flex:1" }
            button { class: if show_right { "strip-btn active" } else { "strip-btn" }, title: "{t(lang, \"pane.properties\")}",
                onclick: move |_| { let v = !st.ui.read().show_right_pane; st.ui_mut().show_right_pane = v; st.save_prefs(); },
                Icon { name: "info" }
            }
        }
    }
}

/// Buttons for the implemented pane commands plus a count of planned ones.
fn pane_toolbar(st: AppState, pane_id: &str) -> Element {
    let lang = st.lang();
    let reg = registry();
    let cmds: Vec<&'static Command> = reg.pane_commands(pane_id);
    let (done, planned): (Vec<&Command>, Vec<&Command>) = cmds
        .into_iter()
        .partition(|c| is_implemented(&c.id) && !c.id.starts_with("view."));
    if done.is_empty() && planned.is_empty() {
        return rsx! {};
    }
    rsx! {
        div { class: "pane-toolbar",
            for c in done {
                {
                    let id = c.id.clone();
                    let label = cmd_label(lang, c);
                    rsx! { button { class: "tbtn", key: "{c.id}", onclick: move |_| dispatch(st, &id), "{label}" } }
                }
            }
            if !planned.is_empty() {
                span { class: "chip", title: "{planned.iter().map(|c| c.label.as_str()).collect::<Vec<_>>().join(\", \")}", "{planned.len()} {t(lang, \"label.planned\").to_lowercase()}" }
            }
        }
    }
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
    let body = if !has_doc {
        rsx! { div { class: "pane-empty", "{t(lang, \"toast.no_document\")}" } }
    } else {
        match &pane {
            Pane::Thumbnails => rsx! { ThumbnailsPane {} },
            Pane::Bookmarks => rsx! { BookmarksPane {} },
            Pane::Comments => rsx! { CommentsPane {} },
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
    rsx! {
        div { class: "pane-panel", onclick: move |e| e.stop_propagation(),
            div { class: "pane-header", Icon { name: pane_icon(&pane) } "{label}" div { class: "spacer" }
                button { class: "ibtn", title: "{t(lang, \"dialog.close\")}", onclick: move |_| { st.ui_mut().show_left_pane = false; st.save_prefs(); }, "×" }
            }
            if has_doc { {pane_toolbar(st, &current)} }
            div { class: "pane-body", {body} }
        }
    }
}

#[component]
fn ThumbnailsPane() -> Element {
    let st = use_context::<AppState>();
    let Some((tab_id, rev, page, count, sizes, selected, rotation)) = st.read_active(|t| {
        (
            t.id,
            t.revision,
            t.page,
            t.doc.page_count(),
            t.page_sizes.clone(),
            t.selected_pages.clone(),
            t.view_rotation,
        )
    }) else {
        return rsx! {};
    };
    rsx! {
        div { class: "thumbs",
            for p in 0..count {
                {
                    let s = sizes.get(p).copied();
                    let (w, h) = s.map(|s| if rotation % 180 != 0 { (s.height, s.width) } else { (s.width, s.height) }).unwrap_or((595.0, 842.0));
                    let scale = 160.0 / w.max(h);
                    let (wpx, hpx) = ((w * scale) as i32, (h * scale) as i32);
                    let is_sel = selected.contains(&p);
                    let class = format!("thumb{}{}", if p == page { " current" } else { "" }, if is_sel { " selected" } else { "" });
                    let src = format!("/page/{tab_id}/{p}/{rev}/250.png");
                    rsx! {
                        div { class: "{class}", key: "{p}",
                            onclick: move |e| {
                                if e.modifiers().ctrl() {
                                    st.with_active(|t| { if let Some(i) = t.selected_pages.iter().position(|x| *x == p) { t.selected_pages.remove(i); } else { t.selected_pages.push(p); } });
                                } else if e.modifiers().shift() {
                                    st.with_active(|t| { let (a, b) = (t.page.min(p), t.page.max(p)); t.selected_pages = (a..=b).collect(); });
                                } else {
                                    st.with_active(|t| t.selected_pages = vec![p]);
                                    st.go_to_page(p);
                                }
                            },
                            img { src: "{src}", alt: "", loading: "lazy", width: "{wpx}", height: "{hpx}", style: "width:{wpx}px;height:{hpx}px" }
                            span { class: "num", "{p + 1}" }
                        }
                    }
                }
            }
        }
    }
}

fn outline_nodes(
    st: AppState,
    items: &[OutlineItem],
    path: Vec<usize>,
    expanded: &std::collections::HashSet<String>,
    selected: &Option<Vec<usize>>,
    selected_sig: Signal<Option<Vec<usize>>>,
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
                let key_t = key.clone();
                let page = item.page;
                let title = item.title.clone();
                let children = item.children.clone();
                rsx! {
                    div { key: "{key}",
                        div { class: if is_sel { "tree-item selected" } else { "tree-item" }, style: "padding-left: {6 + depth * 14}px",
                            onclick: move |_| { selected_sig.set(Some(p_click.clone())); if let Some(pg) = page { st.go_to_page(pg); } },
                            span { class: "twisty", onclick: move |e| { e.stop_propagation(); st.with_active(|t| { if !t.expanded_bookmarks.remove(&key_t) { t.expanded_bookmarks.insert(key_t.clone()); } t.expanded_bookmarks.remove("*"); }); },
                                if has_children { if is_open { "▾" } else { "▸" } } else { "" }
                            }
                            Icon { name: "bookmark" }
                            span { "{title}" }
                            if let Some(pg) = page { span { class: "page-ref", "{pg + 1}" } }
                        }
                        if has_children && is_open {
                            {outline_nodes(st, &children, p.clone(), expanded, selected, selected_sig)}
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
    let from_pages_label = format!("{} → {}", t(lang, "dialog.pages"), t(lang, "pane.bookmarks"));
    rsx! {
        div { class: "pane-toolbar",
            button { class: "tbtn primary", onclick: move |_| st.set_dialog(Dialog::BookmarkNew), Icon { name: "insert" } "{t(lang, \"cmd.bookmarksView.new\")}" }
            button { class: "tbtn", disabled: sel.is_none(), onclick: move |_| { if let Some(p) = sel_ren.clone() { st.set_dialog(Dialog::BookmarkRename(p)); } }, "{t(lang, \"cmd.rename\")}" }
            button { class: "tbtn danger", disabled: sel.is_none(), onclick: move |_| { if let Some(p) = sel_del.clone() { if let Some(Err(e)) = st.with_active(|t| t.doc.delete_bookmark(&p)) { st.error(e); } selected.set(None); } }, Icon { name: "delete" } }
            button { class: "tbtn", title: "{from_pages_label}", onclick: move |_| { if let Some(Err(e)) = st.with_active(|t| t.doc.bookmarks_from_pages()) { st.error(e); } }, "{from_pages_label}" }
        }
        if items.is_empty() {
            div { class: "pane-empty", "{t(lang, \"label.none\")}" }
        }
        div { class: "tree", {outline_nodes(st, &items, Vec::new(), &expanded, &sel, selected)} }
    }
}

#[component]
fn CommentsPane() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let Some((comments, selected)) = st.read_active(|t| (t.doc.comments(), t.selected_annot)) else {
        return rsx! {};
    };
    rsx! {
        if comments.is_empty() {
            div { class: "pane-empty", "{t(lang, \"label.none\")}" }
        }
        for a in comments {
            {
                let (page, index) = (a.page, a.index);
                let is_sel = selected == Some((page, index));
                let contents = a.contents.clone();
                let color = a.color.map(crate::util::color_hex).unwrap_or_else(|| "#888".into());
                rsx! {
                    div { class: if is_sel { "list-item selected" } else { "list-item" }, key: "{page}-{index}",
                        onclick: move |_| { st.with_active(|t| t.selected_annot = Some((page, index))); st.go_to_page(page); },
                        div { class: "title", span { style: "width:10px;height:10px;border-radius:50%;background:{color};display:inline-block" } "{a.subtype}" span { class: "muted", "· {t(lang, \"status.page\")} {page + 1}" } }
                        div { class: "meta", span { "{a.author}" } span { "{a.modified}" } }
                        if !a.contents.is_empty() { div { class: "body", "{a.contents}" } }
                        div { class: "row", style: "gap:6px",
                            button { class: "tbtn", onclick: move |e| { e.stop_propagation(); st.set_dialog(Dialog::AnnotationText { page, index, contents: contents.clone() }); }, "Edit…" }
                            button { class: "tbtn danger", onclick: move |e| { e.stop_propagation(); if let Some(Err(err)) = st.with_active(|t| t.doc.delete_annotation(page, index)) { st.error(err); } }, Icon { name: "delete" } }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn AttachmentsPane() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let Some(atts) = st.read_active(|t| t.doc.attachments()) else {
        return rsx! {};
    };
    rsx! {
        div { class: "pane-toolbar",
            button { class: "tbtn primary", onclick: move |_| files::add_attachment(st), Icon { name: "insert" } "{t(lang, \"cmd.attachmentsView.new\")}" }
        }
        if atts.is_empty() { div { class: "pane-empty", "{t(lang, \"label.none\")}" } }
        for att in atts {
            {
                let a_save = att.clone();
                let a_rm = att.clone();
                let size = att.size.map(crate::util::human_size).unwrap_or_default();
                rsx! {
                    div { class: "list-item", key: "{att.name}-{att.stream.0}",
                        div { class: "title", Icon { name: "attachment" } "{att.name}" }
                        div { class: "meta", span { "{size}" } span { "{att.mime}" } span { "{att.modified}" } if let Some(p) = att.page { span { "{t(lang, \"status.page\")} {p + 1}" } } }
                        if !att.description.is_empty() { div { class: "body", "{att.description}" } }
                        div { class: "row", style: "gap:6px",
                            button { class: "tbtn", onclick: move |_| files::save_attachment(st, a_save.clone()), Icon { name: "save" } "{t(lang, \"cmd.attachmentsView.save\")}" }
                            if att.key.is_some() {
                                button { class: "tbtn danger", onclick: move |_| { if let Some(Err(e)) = st.with_active(|t| t.doc.remove_attachment(&a_rm)) { st.error(e); } }, Icon { name: "delete" } }
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
                    label { class: "check", key: "{id.0}",
                        input { r#type: "checkbox", checked: visible, onchange: move |_| { if let Some(Err(e)) = st.with_active(|t| t.doc.set_layer_visible(id, !visible)) { st.error(e); } } }
                        span { "{layer.name}" }
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
                        div { class: "title", Icon { name: "field" } "{f.name}" span { class: "chip", "{kind.label()}" } }
                        div { class: "meta",
                            if let Some(p) = page { span { "{t(lang, \"status.page\")} {p + 1}" } }
                            if f.required { span { "required" } }
                            if f.read_only { span { "read-only" } }
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
                        div { class: "body", "{desc}" }
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
                        Icon { name: "star" } span { "{d.name}" }
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
                        div { class: "title", Icon { name: "signature" } "{s.field}" span { class: if s.signed { "badge ok" } else { "badge planned" }, if s.signed { "signed" } else { "empty" } } }
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

#[component]
fn SearchPane() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let Some(search) = st.read_active(|t| t.search.clone()) else {
        return rsx! {};
    };
    let mut query = use_signal(|| search.query.clone());
    let run = move || {
        let q = query();
        let opts = SearchOptions {
            case_sensitive: search.case_sensitive,
            whole_words: search.whole_words,
        };
        st.with_active(|t| {
            t.search.query = q.clone();
            t.search.hits = t.doc.search(&q, opts);
            t.search.current = 0;
        });
        let n = st.read_active(|t| t.search.hits.len()).unwrap_or(0);
        if let Some(first) = st
            .read_active(|t| t.search.hits.first().map(|h| h.page))
            .flatten()
        {
            st.go_to_page(first);
        }
        if n == 0 && !q.trim().is_empty() {
            st.toast(crate::state::ToastKind::Info, t(st.lang(), "toast.not_found"), "");
        }
    };
    let hits = search.hits.clone();
    let results_label = t(lang, "search.results").replace("{count}", &hits.len().to_string());
    rsx! {
        div { class: "field", style: "padding: 4px 6px",
            input { placeholder: "{t(lang, \"search.placeholder\")}", value: "{query}", autofocus: true,
                oninput: move |e| query.set(e.value()),
                onkeydown: move |e| { e.stop_propagation(); if e.key() == Key::Enter { run(); } }
            }
        }
        div { class: "row", style: "padding: 0 6px 6px; gap: 10px",
            label { class: "check", input { r#type: "checkbox", checked: search.case_sensitive, onchange: move |_| dispatch(st, "searchView.caseSensitive") } span { "{t(lang, \"search.case\")}" } }
            label { class: "check", input { r#type: "checkbox", checked: search.whole_words, onchange: move |_| dispatch(st, "searchView.wholeWords") } span { "{t(lang, \"search.whole\")}" } }
            button { class: "tbtn primary", onclick: move |_| run(), Icon { name: "search" } }
        }
        if !hits.is_empty() { div { class: "muted", style: "padding: 0 8px 6px", "{results_label}" } }
        for (i, h) in hits.into_iter().enumerate() {
            {
                let page = h.page;
                rsx! {
                    div { class: if i == search.current { "list-item selected" } else { "list-item" }, key: "{page}-{h.index}",
                        onclick: move |_| { st.with_active(|t| t.search.current = i); st.go_to_page(page); },
                        div { class: "title", "{t(lang, \"status.page\")} {page + 1}" }
                        div { class: "body", "{h.snippet}" }
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
                        span { "{label}" }
                        {status_badge(&c.id)}
                    }
                }
            }
        }
    }
}
