//! Layers pane commands: view visibility, defaults, selection, new layers and
//! groups, merging, flattening, extraction and importing pages as a layer.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::Ordering;

use dioxus::prelude::ReadableExt;
use folitecta_core::LayerNode;
use folitecta_core::layers::node_at;

use crate::commands::{doc_dialog, doc_op, handlers, no_doc, option_on};
use crate::i18n::{t, tf};
use crate::state::{AppState, Dialog, LAYER_INHERIT, ToastKind};

type Id = (u32, u16);

/// "all" or "visible": the layers listed by the pane.
pub fn pages_mode(st: AppState) -> String {
    st.ui.peek().value("layers.pages").unwrap_or("all").to_string()
}

/// Layers of the selected entries (groups give their layers), optionally
/// with the layers below them.
pub fn selected_ids(st: AppState, with_children: bool) -> Vec<Id> {
    st.read_active(|t| {
        let tree = t.doc.layer_tree();
        let mut out = Vec::new();
        for p in &t.selected_layers {
            match node_at(&tree, p) {
                Some(n @ LayerNode::Layer { id, .. }) => {
                    if with_children {
                        n.layer_ids(&mut out);
                    } else {
                        out.push(*id);
                    }
                }
                Some(n @ LayerNode::Group { .. }) => n.layer_ids(&mut out),
                None => {}
            }
        }
        let mut seen = HashSet::new();
        out.retain(|id| seen.insert(*id));
        out
    })
    .unwrap_or_default()
}

/// Selected layers, or every layer when nothing is selected.
fn target_ids(st: AppState) -> Vec<Id> {
    let sel = selected_ids(st, true);
    if !sel.is_empty() {
        return sel;
    }
    st.read_active(|t| t.doc.layers().iter().map(|l| l.id).collect())
        .unwrap_or_default()
}

/// Current view visibility of every layer.
pub fn view_states(st: AppState) -> HashMap<Id, bool> {
    st.read_active(|t| {
        t.doc
            .layers()
            .iter()
            .map(|l| (l.id, t.layer_visible(l.id, l.visible)))
            .collect()
    })
    .unwrap_or_default()
}

fn no_layers(st: AppState) -> bool {
    if no_doc(st) {
        return true;
    }
    if st.read_active(|t| t.doc.layers().is_empty()).unwrap_or(true) {
        st.toast(ToastKind::Info, t(st.lang(), "toast.no_layers"), "");
        return true;
    }
    false
}

/// Show (`Some(true)`), hide or toggle (`None`) the targeted layers in the view.
pub fn set_visibility(st: AppState, ids: &[Id], visible: Option<bool>) {
    let mut states = view_states(st);
    for id in ids {
        if let Some(v) = states.get_mut(id) {
            *v = visible.unwrap_or(!*v);
        }
    }
    st.with_active(|t| t.set_layer_view(states));
}

fn show_targets(st: AppState, visible: Option<bool>) {
    if no_layers(st) {
        return;
    }
    let ids = target_ids(st);
    set_visibility(st, &ids, visible);
}

/// Make the view visibility the document's default (selected layers only when `selected`).
fn make_default(st: AppState, selected: bool) {
    if no_layers(st) {
        return;
    }
    let states = view_states(st);
    let ids: HashSet<Id> = if selected {
        target_ids(st).into_iter().collect()
    } else {
        states.keys().copied().collect()
    };
    let chosen: HashMap<Id, bool> = states.into_iter().filter(|(id, _)| ids.contains(id)).collect();
    let r = st.with_active(|t| {
        let r = t.doc.set_layer_defaults(&chosen);
        let rest: HashMap<Id, bool> = t
            .layer_view
            .iter()
            .filter(|(id, _)| !chosen.contains_key(id))
            .map(|(k, v)| (*k, *v))
            .collect();
        t.refresh();
        t.layer_view = rest;
        t.update_render();
        r
    });
    crate::commands::core_result(st, r);
}

fn reset_view(st: AppState) {
    if no_doc(st) {
        return;
    }
    st.with_active(|t| t.set_layer_view(HashMap::new()));
}

/// Apply a visibility to every layer of the view.
pub fn override_all(st: AppState, visible: Option<bool>) {
    let states: HashMap<Id, bool> = match visible {
        Some(v) => view_states(st).into_keys().map(|id| (id, v)).collect(),
        None => HashMap::new(),
    };
    st.with_active(|t| t.set_layer_view(states));
}

fn toggle_inherit(st: AppState) {
    let on = !option_on(st, "layersView.overrideByParent");
    st.set_option("layersView.overrideByParent", on);
    LAYER_INHERIT.store(on, Ordering::Relaxed);
    for t in st.docs_mut().iter_mut() {
        t.bump_view();
        t.update_render();
    }
}

/// Select the entries whose layers match.
fn select_where(st: AppState, pick: impl Fn(&LayerNode, bool, &HashMap<Id, Vec<usize>>) -> bool) {
    if no_layers(st) {
        return;
    }
    let states = view_states(st);
    st.with_active(|t| {
        let tree = t.doc.layer_tree();
        let usage = t.doc.layer_pages();
        let annots: HashSet<Id> = t
            .doc
            .layers()
            .iter()
            .filter(|l| !t.doc.layer_annotations(&HashSet::from([l.id])).is_empty())
            .map(|l| l.id)
            .collect();
        let mut usage = usage;
        for id in annots {
            usage.entry(id).or_default();
        }
        t.selected_layers = folitecta_core::layers::node_paths(&tree)
            .into_iter()
            .filter(|p| {
                node_at(&tree, p)
                    .map(|n| {
                        let visible = n
                            .layer_id()
                            .map(|id| states.get(&id).copied().unwrap_or(true))
                            .unwrap_or(true);
                        pick(n, visible, &usage)
                    })
                    .unwrap_or(false)
            })
            .collect();
    });
}

/// Select the layers used on the current page.
fn select_on_page(st: AppState) {
    let page_layers: HashSet<Id> = st
        .read_active(|t| t.doc.layers_on_page(t.page))
        .unwrap_or_default();
    select_where(st, |n, _, _| {
        n.layer_id().is_some_and(|id| page_layers.contains(&id))
    });
}

/// Select the pages that use the selected layers.
fn select_pages(st: AppState) {
    if no_layers(st) {
        return;
    }
    let ids: HashSet<Id> = target_ids(st).into_iter().collect();
    let pages = st
        .with_active(|t| {
            let usage = t.doc.layer_pages();
            let mut pages: Vec<usize> = ids
                .iter()
                .flat_map(|id| usage.get(id).cloned().unwrap_or_default())
                .collect();
            pages.sort_unstable();
            pages.dedup();
            t.selected_pages = pages.clone();
            pages
        })
        .unwrap_or_default();
    if let Some(first) = pages.first() {
        st.navigate(*first, true);
    }
    st.toast(
        ToastKind::Info,
        tf(
            st.lang(),
            "toast.layer_pages",
            &[("count", &pages.len().to_string())],
        ),
        "",
    );
}

#[derive(Clone, Copy, PartialEq)]
enum AnnotKind {
    Comments,
    Fields,
    Links,
}

/// Select the first comment, field or link of the selected layers.
fn select_annots(st: AppState, kind: AnnotKind) {
    if no_layers(st) {
        return;
    }
    let ids: HashSet<Id> = target_ids(st).into_iter().collect();
    let found: Vec<(usize, usize)> = st
        .read_active(|t| {
            t.doc
                .layer_annotations(&ids)
                .into_iter()
                .filter(|(_, _, s)| match kind {
                    AnnotKind::Fields => s == "Widget",
                    AnnotKind::Links => s == "Link",
                    AnnotKind::Comments => !matches!(s.as_str(), "Widget" | "Link" | "Popup"),
                })
                .map(|(p, i, _)| (p, i))
                .collect()
        })
        .unwrap_or_default();
    match found.first() {
        Some(&(page, index)) => {
            st.with_active(|t| t.selected_annot = Some((page, index)));
            st.navigate(page, true);
            st.toast(
                ToastKind::Info,
                tf(
                    st.lang(),
                    "toast.layer_annots",
                    &[("count", &found.len().to_string())],
                ),
                "",
            );
        }
        None => st.toast(ToastKind::Info, t(st.lang(), "toast.layer_annots_none"), ""),
    }
}

/// Flatten the selected layers (`selected`) or every layer, with the view visibility.
fn flatten(st: AppState, selected: bool) {
    if no_layers(st) {
        return;
    }
    let ids = if selected {
        selected_ids(st, true)
    } else {
        Vec::new()
    };
    if selected && ids.is_empty() {
        st.toast(ToastKind::Info, t(st.lang(), "toast.select_layer"), "");
        return;
    }
    let states = view_states(st);
    let r = st.with_active(|t| {
        let r = t.doc.flatten_layers(selected.then_some(ids.as_slice()), &states);
        t.selected_layers.clear();
        t.layer_view.retain(|id, _| !ids.contains(id) && selected);
        r
    });
    match r {
        Some(Ok(n)) => {
            crate::files::set_window_title(st);
            st.with_active(|t| t.refresh());
            st.toast(
                ToastKind::Ok,
                tf(st.lang(), "toast.layers_flattened", &[("count", &n.to_string())]),
                "",
            );
        }
        Some(Err(e)) => st.error(e),
        None => {}
    }
}

/// New document with the selected layers' content, or with the visible content.
fn extract(st: AppState, selected: bool) {
    if no_layers(st) {
        return;
    }
    let ids: HashSet<Id> = selected_ids(st, true).into_iter().collect();
    if selected && ids.is_empty() {
        st.toast(ToastKind::Info, t(st.lang(), "toast.select_layer"), "");
        return;
    }
    let states = view_states(st);
    let r = st.read_active(|t| t.doc.extract_layers(selected.then_some(&ids), &states));
    match r {
        Some(Ok(doc)) => crate::files::open_document_tab(st, doc),
        Some(Err(e)) => st.error(e),
        None => {}
    }
}

/// Merge the selected layers into the first of them.
fn merge_selected(st: AppState, with_children: bool) {
    if no_layers(st) {
        return;
    }
    let ids = selected_ids(st, with_children);
    if ids.len() < 2 {
        st.toast(ToastKind::Info, t(st.lang(), "toast.select_two_layers"), "");
        return;
    }
    merge_into(st, &ids[1..], ids[0]);
}

pub fn merge_into(st: AppState, sources: &[Id], target: Id) {
    let r = st.with_active(|t| {
        let r = t.doc.merge_layers(sources, target);
        t.selected_layers.clear();
        r
    });
    match r {
        Some(Ok(n)) => {
            crate::files::set_window_title(st);
            st.toast(
                ToastKind::Ok,
                tf(st.lang(), "toast.layers_merged", &[("count", &n.to_string())]),
                "",
            );
        }
        Some(Err(e)) => st.error(e),
        None => {}
    }
}

fn merge_dialog(st: AppState) {
    if no_layers(st) {
        return;
    }
    if selected_ids(st, false).is_empty() {
        st.toast(ToastKind::Info, t(st.lang(), "toast.select_layer"), "");
        return;
    }
    st.set_dialog(Dialog::LayerMerge);
}

fn import_as_layer(st: AppState) {
    if no_doc(st) {
        return;
    }
    dioxus::prelude::spawn(async move {
        if let Some(f) = rfd::AsyncFileDialog::new()
            .add_filter("PDF", &["pdf"])
            .pick_file()
            .await
        {
            st.set_dialog(Dialog::ImportLayer(f.path().to_path_buf()));
        }
    });
}

/// Check states of the layers pane options.
pub fn layers_state(st: AppState, id: &str) -> Option<bool> {
    match id {
        "layersView.overrideByParent" => Some(option_on(st, id)),
        "layersView.viewAllPages" => Some(pages_mode(st) == "all"),
        "layersView.viewVisiblePages" => Some(pages_mode(st) == "visible"),
        _ => None,
    }
}

handlers! {
    "layersView.addNewLayer" => |st| doc_dialog(st, Dialog::LayerName { group: false, rename: None }),
    "layersView.addNewGroup" => |st| doc_dialog(st, Dialog::LayerName { group: true, rename: None }),
    "layersView.showSelected" => |st| show_targets(st, Some(true)),
    "layersView.hideSelected" => |st| show_targets(st, Some(false)),
    "layersView.toggleSelected" => |st| show_targets(st, None),
    "layersView.setAsDefault" => |st| make_default(st, true),
    "layersView.setCurrentStatesAsDefault" => |st| make_default(st, false),
    "layersView.resetOverrides" => reset_view,
    "layersView.applyOverrides" => |st| { if !no_layers(st) { st.set_dialog(Dialog::LayerOverrides) } },
    "layersView.overrideByParent" => toggle_inherit,
    "layersView.viewAllPages" => |st| st.set_value("layers.pages", "all"),
    "layersView.viewVisiblePages" => |st| st.set_value("layers.pages", "visible"),
    "layersView.sort.alphabetical" => |st| { if !no_layers(st) { doc_op(st, |t| { t.selected_layers.clear(); t.doc.sort_layers() }) } },
    "layersView.select" => select_on_page,
    "layersView.selectLayers" => |st| select_where(st, |n, _, _| n.layer_id().is_some()),
    "layersView.selectVisibleLayers" => |st| select_where(st, |n, v, _| n.layer_id().is_some() && v),
    "layersView.selectHiddenLayers" => |st| select_where(st, |n, v, _| n.layer_id().is_some() && !v),
    "layersView.selectEmptyLayers" => |st| select_where(st, |n, _, used| n.layer_id().is_some_and(|id| !used.contains_key(&id))),
    "layersView.selectTextLabels" => |st| select_where(st, |n, _, _| n.layer_id().is_none()),
    "layersView.selectCorresponding.pages" => select_pages,
    "layersView.selectCorresponding.comments" => |st| select_annots(st, AnnotKind::Comments),
    "layersView.selectCorresponding.fields" => |st| select_annots(st, AnnotKind::Fields),
    "layersView.selectCorresponding.links" => |st| select_annots(st, AnnotKind::Links),
    "layersView.extract.selected" => |st| extract(st, true),
    "layersView.extract.visible" => |st| extract(st, false),
    "layersView.flattenOC" => |st| flatten(st, false),
    "layersView.flattenSelected" => |st| flatten(st, true),
    "layersView.merge" => merge_dialog,
    "layersView.mergeSelectedOnly" => |st| merge_selected(st, false),
    "layersView.mergeSelectedWithChildren" => |st| merge_selected(st, true),
    "layersView.importAsLayer" => import_as_layer,
    "layersView.options" => |st| doc_dialog(st, Dialog::LayerOptions),
}
