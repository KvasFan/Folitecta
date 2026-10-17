//! Explorer pane, the new folder dialog and the folder filter dialog.

use dioxus::html::ModifiersInteraction;
use dioxus::prelude::*;
use folitecta_core::registry;

use crate::commands::explorer::{
    EXPLORER, FolderFilter, activate, builtin_places, create_folder, current_dir, custom_filters,
    custom_places, filter_index, filters, go, parse_exts, remove_place, save_custom_filters, shown_entries,
    thumb_url,
};
use crate::commands::{dispatch, option_on};
use crate::i18n::{Lang, cmd_label, t};
use crate::icons::Icon;
use crate::state::{AppState, Dialog, ToastKind};
use crate::ui::dialogs::Frame;
use crate::util::human_size;

fn label_of(lang: Lang, id: &str) -> String {
    registry()
        .get(id)
        .map(|c| cmd_label(lang, c))
        .unwrap_or_else(|| id.to_string())
}

fn place_name(lang: Lang, key: &str, path: &std::path::Path) -> String {
    if key.is_empty() {
        path.display().to_string()
    } else {
        t(lang, key)
    }
}

#[component]
pub fn ExplorerPane() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let state = EXPLORER();
    let _ = state.generation;
    let dir = current_dir(st);
    let entries = shown_entries(st);
    let thumbs = option_on(st, "explorer.thumbs");
    let custom = custom_places(st);
    let all_filters = filters(st);
    let fi = filter_index(st).min(all_filters.len() - 1);
    let tools: [(&str, &str, bool); 7] = [
        ("explorer.folder.back", "prev", !state.back.is_empty()),
        ("explorer.folder.forward", "next", !state.forward.is_empty()),
        ("explorer.folder.up", "up", dir.parent().is_some()),
        ("explorer.folder.sync", "page", st.has_doc()),
        ("explorer.refresh", "redo", true),
        ("explorer.folder.create", "folder", true),
        ("explorer.delete", "delete", !state.selected.is_empty()),
    ];
    rsx! {
        div { class: "pane-toolbar icon-toolbar",
            for (id, icon, enabled) in tools {
                button { class: "ibtn", key: "{id}", title: "{label_of(lang, id)}", disabled: !enabled, onclick: move |_| dispatch(st, id), Icon { name: icon } }
            }
            button { class: if thumbs { "ibtn on" } else { "ibtn" }, title: "{label_of(lang, \"explorer.thumb.loading\")}", onclick: move |_| dispatch(st, "explorer.thumb.loading"), Icon { name: "image" } }
            button { class: "ibtn", title: "{label_of(lang, \"addPlace\")}", onclick: move |_| dispatch(st, "addPlace"), Icon { name: "star" } }
        }
        div { class: "explorer-path", title: "{dir.display()}", "{dir.display()}" }
        details { class: "explorer-places", open: true,
            summary { "{t(lang, \"explorer.places\")}" }
            for (key, path) in builtin_places() {
                {
                    let shown = place_name(lang, &key, &path);
                    let target = path.clone();
                    rsx! {
                        div { class: if dir == path { "tree-item selected" } else { "tree-item" }, key: "b{path.display()}", title: "{path.display()}",
                            onclick: move |_| go(st, target.clone()),
                            Icon { name: "folder" }
                            span { class: "tree-label", "{shown}" }
                        }
                    }
                }
            }
            for path in custom {
                {
                    let shown = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| path.display().to_string());
                    let selected = state.place.as_ref() == Some(&path);
                    let (target, sel, rm) = (path.clone(), path.clone(), path.clone());
                    rsx! {
                        div { class: if selected { "tree-item selected" } else { "tree-item" }, key: "c{path.display()}", title: "{path.display()}",
                            onclick: move |_| { EXPLORER.write().place = Some(sel.clone()); go(st, target.clone()); },
                            Icon { name: "star" }
                            span { class: "tree-label", "{shown}" }
                            button { class: "ibtn", title: "{t(lang, \"dialog.remove\")}", onclick: move |e| { e.stop_propagation(); remove_place(st, &rm); }, "×" }
                        }
                    }
                }
            }
            div { class: "row", style: "gap:2px",
                button { class: "ibtn", title: "{label_of(lang, \"explorer.places.nav.up\")}", onclick: move |_| dispatch(st, "explorer.places.nav.up"), Icon { name: "up" } }
                button { class: "ibtn", title: "{label_of(lang, \"explorer.places.nav.down\")}", onclick: move |_| dispatch(st, "explorer.places.nav.down"), Icon { name: "down" } }
            }
        }
        div { class: "explorer-filter",
            select { title: "{label_of(lang, \"folderExts\")}",
                onchange: move |e| st.set_value("explorer.filter", e.value()),
                for (i, f) in all_filters.iter().enumerate() {
                    option { key: "{i}", value: "{i}", selected: i == fi, "{f.name}" }
                }
            }
            button { class: "ibtn", title: "{label_of(lang, \"folderExts.addCustom\")}", onclick: move |_| dispatch(st, "folderExts.addCustom"), Icon { name: "new" } }
            button { class: "ibtn", title: "{label_of(lang, \"folderExts.editCustom\")}", disabled: fi < 3, onclick: move |_| dispatch(st, "folderExts.editCustom"), Icon { name: "pencil" } }
        }
        div { class: "tree explorer-list",
            if entries.is_empty() { div { class: "pane-empty", "{t(lang, \"explorer.empty\")}" } }
            for entry in entries.into_iter() {
                {
                    let selected = state.selected.contains(&entry.path);
                    let pdf = !entry.dir && entry.path.extension().and_then(|x| x.to_str()).is_some_and(|x| x.eq_ignore_ascii_case("pdf"));
                    let thumb = (thumbs && pdf).then(|| thumb_url(&entry.path));
                    let size = if entry.dir { String::new() } else { human_size(entry.size) };
                    let (pick, open) = (entry.path.clone(), entry.clone());
                    let icon = if entry.dir { "folder" } else { "page" };
                    rsx! {
                        div { class: if selected { "tree-item explorer-row selected" } else { "tree-item explorer-row" }, key: "{entry.path.display()}", title: "{entry.path.display()}",
                            onclick: move |e| {
                                let add = e.modifiers().ctrl() || e.modifiers().meta();
                                let mut s = EXPLORER.write();
                                if add { if !s.selected.remove(&pick) { s.selected.insert(pick.clone()); } } else { s.selected = std::collections::BTreeSet::from([pick.clone()]); }
                            },
                            ondoubleclick: move |_| activate(st, &open),
                            if let Some(src) = thumb {
                                img { class: "explorer-thumb", src: "{src}", alt: "", loading: "lazy" }
                            } else {
                                Icon { name: icon }
                            }
                            span { class: "tree-label", "{entry.name}" }
                            if !size.is_empty() { span { class: "muted small", "{size}" } }
                        }
                    }
                }
            }
        }
    }
}

#[component]
pub fn NewFolderDialog() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let mut name = use_signal(|| t(lang, "explorer.new_folder"));
    rsx! {
        Frame { title: label_of(lang, "explorer.folder.create"),
            on_ok: move |_| match create_folder(st, &name()) {
                Ok(_) => st.set_dialog(Dialog::None),
                Err(e) => st.toast(ToastKind::Warn, e, ""),
            },
            div { class: "field", label { "{t(lang, \"explorer.folder_name\")}" }
                input { value: "{name}", autofocus: true, oninput: move |e| name.set(e.value()) } }
            p { class: "hint", "{current_dir(st).display()}" }
        }
    }
}

/// A custom folder filter: new (None) or the n-th one.
#[component]
pub fn FolderFilterDialog(index: Option<usize>) -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let existing = index.and_then(|i| custom_filters(st).get(i).cloned());
    let mut name = use_signal(|| existing.as_ref().map(|f| f.name.clone()).unwrap_or_default());
    let mut exts = use_signal(|| existing.as_ref().map(|f| f.exts.join(", ")).unwrap_or_default());
    rsx! {
        Frame { title: label_of(lang, if index.is_some() { "folderExts.editCustom" } else { "folderExts.addCustom" }),
            on_ok: move |_| {
                let list = parse_exts(&exts());
                if name().trim().is_empty() || list.is_empty() {
                    st.toast(ToastKind::Info, t(lang, "toast.filter_incomplete"), "");
                    return;
                }
                let f = FolderFilter { name: name().trim().to_string(), exts: list };
                let mut all = custom_filters(st);
                let pos = match index {
                    Some(i) if i < all.len() => { all[i] = f; i }
                    _ => { all.push(f); all.len() - 1 }
                };
                save_custom_filters(st, &all);
                st.set_value("explorer.filter", (pos + 3).to_string());
                st.set_dialog(Dialog::None);
            },
            div { class: "field", label { "{t(lang, \"explorer.filter_name\")}" }
                input { value: "{name}", autofocus: true, oninput: move |e| name.set(e.value()) } }
            div { class: "field", label { "{t(lang, \"explorer.filter_exts\")}" }
                input { value: "{exts}", placeholder: "pdf, png, jpg", oninput: move |e| exts.set(e.value()) } }
        }
    }
}
