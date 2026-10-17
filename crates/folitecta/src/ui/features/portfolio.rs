//! Files view of a portfolio, the bar over its cover, and its dialogs.

use dioxus::html::ModifiersInteraction;
use dioxus::prelude::*;
use folitecta_core::portfolio::{FIELD_KINDS, PortfolioFolder, PortfolioItem, SchemaField};
use folitecta_core::registry;

use crate::commands::portfolio::{current, layout, open_item, preview_image, preview_mode};
use crate::commands::{dispatch, option_on};
use crate::i18n::{Lang, cmd_label, t, ui_label};
use crate::icons::Icon;
use crate::state::{AppState, Dialog};
use crate::ui::dialogs::Frame;

fn kind_label(lang: Lang, kind: &str) -> String {
    ui_label(lang, "portfolio.kind", kind, kind)
}

/// Readable size in bytes.
fn size_text(v: &str) -> String {
    v.parse::<u64>()
        .map(crate::util::human_size)
        .unwrap_or_else(|_| v.to_string())
}

fn cell(p: &folitecta_core::portfolio::Portfolio, item: &PortfolioItem, f: &SchemaField) -> String {
    let v = p.value(item, f);
    if matches!(f.kind.as_str(), "Size" | "CompressedSize") {
        size_text(&v)
    } else {
        v
    }
}

/// Bar shown over the cover of a portfolio.
#[component]
pub fn CoverBar() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    rsx! {
        div { class: "portfolio-coverbar",
            Icon { name: "attachment" }
            span { class: "ellipsis", "{t(lang, \"portfolio.cover_hint\")}" }
            button { class: "tbtn primary", onclick: move |_| dispatch(st, "view.portfolio.cover"), "{t(lang, \"portfolio.show_files\")}" }
        }
    }
}

#[component]
pub fn PortfolioView() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let mut thumb: Signal<Option<(Vec<u8>, Option<String>)>> = use_signal(|| None);
    let Some((p, ps)) = current(st) else {
        return rsx! {};
    };
    let lay = layout(st);
    let flat = lay == "flat";
    let compact = lay == "compact";
    let tiles = p.view == "T" && !compact;
    let folder = ps.folder;
    // Path from the root to the current folder.
    let mut crumbs: Vec<(Option<i64>, String)> = Vec::new();
    let mut cur = folder;
    let mut guard = 0;
    while let Some(f) = cur {
        guard += 1;
        let Some(fo) = p.folders.iter().find(|x| x.id == f) else {
            break;
        };
        if Some(f) == p.root || guard > 64 {
            break;
        }
        crumbs.push((Some(f), fo.name.clone()));
        cur = fo.parent.filter(|x| Some(*x) != p.root);
    }
    crumbs.push((None, t(lang, "portfolio.root")));
    crumbs.reverse();
    let folders: Vec<PortfolioFolder> = if flat {
        Vec::new()
    } else {
        p.subfolders(folder).into_iter().cloned().collect()
    };
    let files: Vec<PortfolioItem> = if flat {
        let mut all: Vec<&PortfolioItem> = p.items.iter().collect();
        p.sort_items(&mut all);
        all.into_iter().cloned().collect()
    } else {
        p.files_in(folder).into_iter().cloned().collect()
    };
    let columns: Vec<SchemaField> = if compact {
        p.schema.iter().filter(|f| f.kind == "F").cloned().collect()
    } else {
        p.schema.iter().filter(|f| f.visible).cloned().collect()
    };
    let columns = if columns.is_empty() {
        vec![SchemaField {
            key: "FileName".into(),
            name: t(lang, "attach.col.name"),
            kind: "F".into(),
            visible: true,
            editable: false,
        }]
    } else {
        columns
    };
    let grid = format!(
        "grid-template-columns: repeat({}, minmax(90px, 1fr));",
        columns.len()
    );
    let sel_key = ps.selected.first().cloned();
    let selected_item = sel_key
        .as_ref()
        .and_then(|k| p.items.iter().find(|i| &i.key == k))
        .cloned();
    // Preview of the selected file, made in the background.
    let wanted = selected_item.as_ref().map(|i| i.key.clone());
    use_effect(use_reactive!(|wanted| {
        let Some(key) = wanted else {
            thumb.set(None);
            return;
        };
        if thumb.peek().as_ref().is_some_and(|(k, _)| *k == key) {
            return;
        }
        let item = st
            .read_active(|t| {
                t.doc
                    .portfolio()
                    .and_then(|p| p.items.into_iter().find(|i| i.key == key))
            })
            .flatten();
        let data = item.as_ref().and_then(|i| {
            st.read_active(|t| t.doc.attachment_data(&i.attachment).ok())
                .flatten()
        });
        let name = item.map(|i| i.name).unwrap_or_default();
        spawn(async move {
            let url = match data {
                Some(d) => tokio::task::spawn_blocking(move || preview_image(&name, &d))
                    .await
                    .ok()
                    .flatten(),
                None => None,
            };
            thumb.set(Some((key, url)));
        });
    }));
    let preview = preview_mode(st);
    let (vw, _) = (st.viewport)();
    let position = match preview.as_str() {
        "auto" => {
            if vw >= 1100.0 {
                "right"
            } else {
                "bottom"
            }
        }
        other => other,
    }
    .to_string();
    let show_preview = position != "off";
    let tools = option_on(st, "portfolio.tools");
    let reg = registry();
    let label = |id: &str| {
        reg.get(id)
            .map(|c| cmd_label(lang, c))
            .unwrap_or_else(|| id.to_string())
    };
    let can_up = folder.is_some();
    let can_back = !ps.back.is_empty();
    let can_fwd = !ps.forward.is_empty();
    let has_sel = !ps.selected.is_empty() || !ps.selected_folders.is_empty();
    let sort = p.sort.clone();
    let body_class = if position == "bottom" {
        "portfolio-body column"
    } else {
        "portfolio-body"
    };
    let tool_btns: [(&str, &str, bool); 9] = [
        ("view.portfolio.view.nav.prev", "prev", can_back),
        ("view.portfolio.view.nav.next", "next", can_fwd),
        ("view.portfolio.view.nav.up", "up", can_up),
        ("view.portfolio.addFiles", "insert", true),
        ("view.portfolio.createFolder", "folder", true),
        ("view.portfolio.extractFiles", "extract", true),
        ("view.portfolio.itemInfo", "info", has_sel),
        ("view.portfolio.delete", "delete", has_sel),
        ("view.portfolio.properties", "settings", true),
    ];
    let preview_item = selected_item.clone();
    let thumb_url = thumb()
        .filter(|(k, _)| Some(k) == sel_key.as_ref())
        .and_then(|(_, u)| u);
    let empty = folders.is_empty() && files.is_empty();
    let layouts = [
        ("folders", "view.portfolio.view.folders"),
        ("flat", "view.portfolio.view.flat"),
        ("compact", "view.portfolio.view.compact"),
    ];
    let previews = [
        ("auto", "view.portfolio.preview.auto"),
        ("right", "view.portfolio.preview.right"),
        ("bottom", "view.portfolio.preview.bottom"),
        ("off", "view.portfolio.preview.off"),
    ];
    rsx! {
        div { class: "portfolio",
            div { class: "portfolio-bar",
                button { class: "tbtn", title: "{label(\"view.portfolio.cover\")}", onclick: move |_| dispatch(st, "view.portfolio.cover"), Icon { name: "page" } "{t(lang, \"portfolio.cover\")}" }
                if tools {
                    for (id, icon, enabled) in tool_btns {
                        button { class: "ibtn", key: "{id}", title: "{label(id)}", disabled: !enabled, onclick: move |_| dispatch(st, id), Icon { name: icon } }
                    }
                }
                div { class: "crumbs",
                    for (i, (id, name)) in crumbs.into_iter().enumerate() {
                        span { class: "crumb-item", key: "c{i}",
                            if i > 0 { span { class: "sep", "›" } }
                            button { class: "crumb", onclick: move |_| { st.with_active(|t| t.portfolio.open_folder(id)); }, "{name}" }
                        }
                    }
                }
                div { class: "spacer" }
                select { title: "{label(\"view.portfolio.view\")}", onchange: move |e| st.set_value("portfolio.layout", e.value().as_str()),
                    for (v, id) in layouts { option { key: "{v}", value: "{v}", selected: lay == v, "{label(id)}" } }
                }
                select { title: "{label(\"view.portfolio.preview\")}", onchange: move |e| st.set_value("portfolio.preview", e.value().as_str()),
                    for (v, id) in previews { option { key: "{v}", value: "{v}", selected: preview == v, "{label(id)}" } }
                }
            }
            div { class: "{body_class}",
                div { class: "portfolio-list",
                    if empty {
                        div { class: "pane-empty",
                            "{t(lang, \"portfolio.empty\")}"
                            div { class: "row", style: "justify-content:center;margin-top:8px",
                                button { class: "tbtn", onclick: move |_| dispatch(st, "view.portfolio.addFiles"), Icon { name: "insert" } "{label(\"view.portfolio.addFiles\")}" }
                            }
                        }
                    } else if tiles {
                        div { class: "portfolio-tiles",
                            for f in folders.iter().cloned() {
                                {
                                    let id = f.id;
                                    let sel = ps.selected_folders.contains(&id);
                                    rsx! {
                                        div { class: if sel { "pf-tile selected" } else { "pf-tile" }, key: "d{id}", title: "{f.description}",
                                            onclick: move |e| select_folder(st, id, e.modifiers().ctrl() || e.modifiers().meta()),
                                            ondoubleclick: move |_| { st.with_active(|t| t.portfolio.open_folder(Some(id))); },
                                            Icon { name: "folder" }
                                            span { class: "ellipsis", "{f.name}" }
                                        }
                                    }
                                }
                            }
                            for it in files.iter().cloned() {
                                {
                                    let key = it.key.clone();
                                    let sel = ps.selected.contains(&key);
                                    let size = it.attachment.size.map(crate::util::human_size).unwrap_or_default();
                                    let open = it.clone();
                                    rsx! {
                                        div { class: if sel { "pf-tile selected" } else { "pf-tile" }, key: "f{it.attachment.stream.0}", title: "{it.name}",
                                            onclick: move |e| select_file(st, key.clone(), e.modifiers().ctrl() || e.modifiers().meta()),
                                            ondoubleclick: move |_| open_item(st, open.clone()),
                                            Icon { name: "attachment" }
                                            span { class: "ellipsis", "{it.name}" }
                                            span { class: "muted", "{size}" }
                                        }
                                    }
                                }
                            }
                        }
                    } else {
                        div { class: "pf-table",
                            div { class: "pf-row head", style: "{grid}",
                                for f in columns.iter().cloned() {
                                    {
                                        let key = f.key.clone();
                                        let arrow = match &sort { Some((k, asc)) if *k == f.key => if *asc { " ▲" } else { " ▼" }, _ => "" };
                                        let asc = !matches!(&sort, Some((k, true)) if *k == f.key);
                                        rsx! {
                                            button { class: "pf-head", key: "{f.key}", title: "{t(lang, \"portfolio.sort_hint\")}",
                                                onclick: move |_| { let k = key.clone(); if let Some(Err(e)) = st.with_active(|t| t.doc.set_portfolio_sort(&k, asc)) { st.error(e); } },
                                                "{f.name}{arrow}"
                                            }
                                        }
                                    }
                                }
                            }
                            for f in folders.iter().cloned() {
                                {
                                    let id = f.id;
                                    let sel = ps.selected_folders.contains(&id);
                                    let n = columns.len();
                                    rsx! {
                                        div { class: if sel { "pf-row selected" } else { "pf-row" }, key: "d{id}", style: "{grid}", title: "{f.description}",
                                            onclick: move |e| select_folder(st, id, e.modifiers().ctrl() || e.modifiers().meta()),
                                            ondoubleclick: move |_| { st.with_active(|t| t.portfolio.open_folder(Some(id))); },
                                            span { class: "pf-name", Icon { name: "folder" } span { class: "ellipsis", "{f.name}" } }
                                            for c in 1..n { span { key: "{c}", "" } }
                                        }
                                    }
                                }
                            }
                            for it in files.iter().cloned() {
                                {
                                    let key = it.key.clone();
                                    let sel = ps.selected.contains(&key);
                                    let cells: Vec<(String, bool)> = columns.iter().map(|c| (cell(&p, &it, c), c.kind == "F")).collect();
                                    let open = it.clone();
                                    rsx! {
                                        div { class: if sel { "pf-row selected" } else { "pf-row" }, key: "f{it.attachment.stream.0}", style: "{grid}",
                                            onclick: move |e| select_file(st, key.clone(), e.modifiers().ctrl() || e.modifiers().meta()),
                                            ondoubleclick: move |_| open_item(st, open.clone()),
                                            for (c, (v, is_name)) in cells.into_iter().enumerate() {
                                                if is_name {
                                                    span { class: "pf-name", key: "{c}", title: "{v}", Icon { name: "attachment" } span { class: "ellipsis", "{v}" } }
                                                } else {
                                                    span { class: "ellipsis", key: "{c}", title: "{v}", "{v}" }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                if show_preview {
                    div { class: "portfolio-preview",
                        if let Some(it) = preview_item {
                            {
                                let open = it.clone();
                                let size = it.attachment.size.map(crate::util::human_size).unwrap_or_default();
                                rsx! {
                                    div { class: "pf-preview-title ellipsis", title: "{it.name}", "{it.name}" }
                                    if let Some(url) = thumb_url.clone() {
                                        img { class: "pf-thumb", src: "{url}", alt: "" }
                                    } else {
                                        div { class: "pf-thumb none", Icon { name: "attachment" } }
                                    }
                                    div { class: "kv",
                                        span { class: "k", "{t(lang, \"attach.col.size\")}" } span { "{size}" }
                                        span { class: "k", "{t(lang, \"attach.col.modified\")}" } span { "{it.attachment.modified}" }
                                        if !it.attachment.description.is_empty() {
                                            span { class: "k", "{t(lang, \"attach.col.description\")}" } span { "{it.attachment.description}" }
                                        }
                                    }
                                    button { class: "tbtn primary", onclick: move |_| open_item(st, open.clone()), "{label(\"view.portfolio.previewFile\")}" }
                                }
                            }
                        } else {
                            div { class: "pane-empty", "{t(lang, \"portfolio.preview_none\")}" }
                        }
                    }
                }
            }
        }
    }
}

fn select_file(st: AppState, key: Vec<u8>, add: bool) {
    st.with_active(|t| {
        let ps = &mut t.portfolio;
        if add {
            if let Some(i) = ps.selected.iter().position(|k| *k == key) {
                ps.selected.remove(i);
            } else {
                ps.selected.push(key);
            }
        } else {
            ps.selected = vec![key];
            ps.selected_folders.clear();
        }
    });
}

fn select_folder(st: AppState, id: i64, add: bool) {
    st.with_active(|t| {
        let ps = &mut t.portfolio;
        if add {
            if let Some(i) = ps.selected_folders.iter().position(|k| *k == id) {
                ps.selected_folders.remove(i);
            } else {
                ps.selected_folders.push(id);
            }
        } else {
            ps.selected_folders = vec![id];
            ps.selected.clear();
        }
    });
}

/// A new folder, or a folder's name and description.
#[component]
pub fn PortfolioFolderDialog(id: Option<i64>) -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let existing = id.and_then(|i| current(st).and_then(|(p, _)| p.folders.into_iter().find(|f| f.id == i)));
    let mut name = use_signal(|| existing.as_ref().map(|f| f.name.clone()).unwrap_or_default());
    let mut desc = use_signal(|| {
        existing
            .as_ref()
            .map(|f| f.description.clone())
            .unwrap_or_default()
    });
    let title = if id.is_some() {
        t(lang, "portfolio.folder_props")
    } else {
        t(lang, "portfolio.new_folder")
    };
    rsx! {
        Frame { title,
            on_ok: move |_| {
                let r = st.with_active(|t| match id {
                    Some(i) => t.doc.portfolio_edit_folder(i, &name(), &desc()),
                    None => t.doc.portfolio_create_folder(&name(), t.portfolio.folder).map(|_| ()),
                });
                match r {
                    Some(Err(e)) => st.error(e),
                    _ => {
                        crate::files::set_window_title(st);
                        st.set_dialog(Dialog::None);
                    }
                }
            },
            div { class: "field",
                label { "{t(lang, \"layers.name\")}" }
                input { value: "{name}", autofocus: true, oninput: move |e| name.set(e.value()) }
            }
            if id.is_some() {
                div { class: "field",
                    label { "{t(lang, \"attach.col.description\")}" }
                    textarea { value: "{desc}", oninput: move |e| desc.set(e.value()) }
                }
            }
        }
    }
}

/// Description and column values of a portfolio file.
#[component]
pub fn PortfolioItemDialog(file: Vec<u8>) -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let Some((p, _)) = current(st) else { return rsx! {} };
    let Some(item) = p.items.iter().find(|i| i.key == file).cloned() else {
        return rsx! {};
    };
    let custom: Vec<SchemaField> = p
        .schema
        .iter()
        .filter(|f| matches!(f.kind.as_str(), "S" | "D" | "N"))
        .cloned()
        .collect();
    let mut desc = use_signal(|| item.attachment.description.clone());
    let initial: Vec<String> = custom.iter().map(|f| p.value(&item, f)).collect();
    let mut values: Signal<Vec<String>> = use_signal(|| initial);
    let it = item.clone();
    let cols = custom.clone();
    rsx! {
        Frame { title: t(lang, "portfolio.item_info"),
            on_ok: move |_| {
                let rel = it.attachment.relationship.clone();
                let mut r = st.with_active(|t| t.doc.set_attachment_info(&it.attachment, &it.name, &desc(), &rel));
                for (f, v) in cols.iter().zip(values()) {
                    if matches!(r, Some(Err(_))) { break; }
                    r = st.with_active(|t| t.doc.set_portfolio_item_value(&it.key, &f.key, &v));
                }
                match r {
                    Some(Err(e)) => st.error(e),
                    _ => {
                        crate::files::set_window_title(st);
                        st.set_dialog(Dialog::None);
                    }
                }
            },
            div { class: "hint", "{item.name}" }
            div { class: "field",
                label { "{t(lang, \"attach.col.description\")}" }
                textarea { value: "{desc}", autofocus: true, oninput: move |e| desc.set(e.value()) }
            }
            for (i, f) in custom.into_iter().enumerate() {
                {
                    let hint = match f.kind.as_str() { "D" => "YYYY-MM-DD", "N" => "0", _ => "" };
                    let v = values().get(i).cloned().unwrap_or_default();
                    rsx! {
                        div { class: "field", key: "{f.key}",
                            label { "{f.name}" }
                            input { value: "{v}", placeholder: "{hint}", oninput: move |e| { values.with_mut(|vals| { if let Some(x) = vals.get_mut(i) { *x = e.value(); } }); } }
                        }
                    }
                }
            }
        }
    }
}

/// Layout, initial file, sort order and columns of a portfolio.
#[component]
pub fn PortfolioPropsDialog() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let Some((p, ps)) = current(st) else {
        return rsx! {};
    };
    let reg = registry();
    let label = |id: &str| {
        reg.get(id)
            .map(|c| cmd_label(lang, c))
            .unwrap_or_else(|| id.to_string())
    };
    let views = [
        ("D", t(lang, "portfolio.view.details")),
        ("T", t(lang, "portfolio.view.tiles")),
        ("H", t(lang, "portfolio.view.cover")),
    ];
    let initial_key = p.initial.clone();
    let sort_key = p.sort.as_ref().map(|(k, _)| k.clone()).unwrap_or_default();
    let ascending = p.sort.as_ref().map(|(_, a)| *a).unwrap_or(true);
    let items: Vec<(String, String)> = p
        .items
        .iter()
        .map(|i| (String::from_utf8_lossy(&i.key).to_string(), i.name.clone()))
        .collect();
    let schema = p.schema.clone();
    let sel = ps.column;
    let list_tools: [(&str, &str, bool); 4] = [
        ("portfolioSchemaList.add", "insert", true),
        ("portfolioSchemaList.up", "up", sel.is_some_and(|i| i > 0)),
        (
            "portfolioSchemaList.down",
            "down",
            sel.is_some_and(|i| i + 1 < schema.len()),
        ),
        ("portfolioSchemaList.delete", "delete", sel.is_some()),
    ];
    let set_schema = move |s: Vec<SchemaField>| {
        if let Some(Err(e)) = st.with_active(|t| t.doc.set_portfolio_schema(&s)) {
            st.error(e);
        }
    };
    rsx! {
        Frame { title: t(lang, "portfolio.props"), wide: true,
            div { class: "field-grid",
                div { class: "field",
                    label { "{t(lang, \"portfolio.layout\")}" }
                    select { onchange: move |e| { let v = e.value(); if let Some(Err(err)) = st.with_active(|t| t.doc.set_portfolio_view(&v)) { st.error(err); } },
                        for (v, text) in views { option { key: "{v}", value: "{v}", selected: p.view == v, "{text}" } }
                    }
                }
                div { class: "field",
                    label { "{t(lang, \"portfolio.initial\")}" }
                    select { onchange: move |e| { let v = e.value(); let key = (!v.is_empty()).then(|| v.into_bytes()); if let Some(Err(err)) = st.with_active(|t| t.doc.set_portfolio_initial(key.as_deref())) { st.error(err); } },
                        option { value: "", selected: initial_key.is_none(), "{t(lang, \"portfolio.cover\")}" }
                        for (k, name) in items {
                            option { key: "{k}", value: "{k}", selected: initial_key.as_deref() == Some(k.as_bytes()), "{name}" }
                        }
                    }
                }
            }
            div { class: "field-grid",
                div { class: "field",
                    label { "{label(\"view.portfolio.sortBy\")}" }
                    select { onchange: move |e| { let v = e.value(); if let Some(Err(err)) = st.with_active(|t| t.doc.set_portfolio_sort(&v, ascending)) { st.error(err); } },
                        for f in schema.iter() { option { key: "{f.key}", value: "{f.key}", selected: f.key == sort_key, "{f.name}" } }
                    }
                }
                div { class: "field",
                    label { "{t(lang, \"portfolio.order\")}" }
                    select { onchange: move |e| { let asc = e.value() == "asc"; let k = sort_key.clone(); if let Some(Err(err)) = st.with_active(|t| t.doc.set_portfolio_sort(&k, asc)) { st.error(err); } },
                        option { value: "asc", selected: ascending, "{t(lang, \"portfolio.ascending\")}" }
                        option { value: "desc", selected: !ascending, "{t(lang, \"portfolio.descending\")}" }
                    }
                }
            }
            div { class: "field",
                label { "{t(lang, \"portfolio.columns\")}" }
                div { class: "pane-toolbar icon-toolbar",
                    for (id, icon, enabled) in list_tools {
                        button { class: "ibtn", key: "{id}", title: "{label(id)}", disabled: !enabled, onclick: move |_| dispatch(st, id), Icon { name: icon } }
                    }
                }
                div { class: "pf-columns",
                    for (i, f) in schema.iter().cloned().enumerate() {
                        {
                            let s_vis = schema.clone();
                            let s_name = schema.clone();
                            let s_kind = schema.clone();
                            let set_a = set_schema;
                            let set_b = set_schema;
                            let set_c = set_schema;
                            rsx! {
                                div { class: if sel == Some(i) { "pf-col selected" } else { "pf-col" }, key: "{f.key}",
                                    onclick: move |_| { st.with_active(|t| t.portfolio.column = Some(i)); },
                                    input { r#type: "checkbox", checked: f.visible, title: "{t(lang, \"portfolio.visible\")}",
                                        onclick: move |e| e.stop_propagation(),
                                        onchange: move |_| { let mut s = s_vis.clone(); s[i].visible = !s[i].visible; set_a(s); }
                                    }
                                    input { value: "{f.name}", onclick: move |e| e.stop_propagation(),
                                        onchange: move |e| { let mut s = s_name.clone(); s[i].name = e.value(); set_b(s); }
                                    }
                                    select { onclick: move |e| e.stop_propagation(),
                                        onchange: move |e| { let mut s = s_kind.clone(); s[i].kind = e.value(); set_c(s); },
                                        for k in FIELD_KINDS.iter() { option { key: "{k}", value: "{k}", selected: f.kind == *k, "{kind_label(lang, k)}" } }
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
