//! Layers pane: the layer tree with view visibility, selection and locks, and
//! the layer dialogs.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::Ordering;

use dioxus::html::ModifiersInteraction;
use dioxus::prelude::*;
use folitecta_core::layers::{inherit_visibility, node_at};
use folitecta_core::{Document, Layer, LayerNode, registry};

use crate::commands::layers::{merge_into, override_all, pages_mode, selected_ids, set_visibility};
use crate::commands::{dispatch, option_on};
use crate::i18n::{cmd_label, t, tf};
use crate::icons::Icon;
use crate::state::{AppState, Dialog, LAYER_INHERIT};
use crate::ui::dialogs::Frame;

type Id = (u32, u16);

fn key_of(path: &[usize]) -> String {
    path.iter().map(|i| i.to_string()).collect::<Vec<_>>().join(".")
}

/// One visible row of the tree.
#[derive(Clone, PartialEq)]
struct Row {
    path: Vec<usize>,
    key: String,
    depth: usize,
    id: Option<Id>,
    label: String,
    has_children: bool,
    open: bool,
}

struct RowCtx<'a> {
    layers: &'a HashMap<Id, Layer>,
    collapsed: &'a HashSet<String>,
    /// Layers used on the current page, when only those are listed.
    filter: Option<&'a HashSet<Id>>,
}

fn flatten_rows(nodes: &[LayerNode], prefix: &mut Vec<usize>, ctx: &RowCtx, out: &mut Vec<Row>) {
    for (i, node) in nodes.iter().enumerate() {
        prefix.push(i);
        let listed = match ctx.filter {
            Some(f) => {
                let mut ids = Vec::new();
                node.layer_ids(&mut ids);
                ids.iter().any(|id| f.contains(id))
            }
            None => true,
        };
        if listed {
            let key = key_of(prefix);
            let open = !ctx.collapsed.contains(&key);
            let (id, label) = match node {
                LayerNode::Layer { id, .. } => (
                    Some(*id),
                    ctx.layers.get(id).map(|l| l.name.clone()).unwrap_or_default(),
                ),
                LayerNode::Group { label, .. } => (None, label.clone()),
            };
            out.push(Row {
                path: prefix.clone(),
                key,
                depth: prefix.len() - 1,
                id,
                label,
                has_children: !node.children().is_empty(),
                open,
            });
            if open {
                flatten_rows(node.children(), prefix, ctx, out);
            }
        }
        prefix.pop();
    }
}

#[component]
pub fn LayersPane() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let visible_only = pages_mode(st) == "visible";
    let inherit = LAYER_INHERIT.load(Ordering::Relaxed);
    let Some((tree, layers, view, selected, collapsed, usage, on_page, differs, drawing)) =
        st.read_active(|t| {
            let layers = t.doc.layers();
            let view: HashMap<Id, bool> = layers
                .iter()
                .map(|l| (l.id, t.layer_visible(l.id, l.visible)))
                .collect();
            (
                t.doc.layer_tree(),
                layers,
                view,
                t.selected_layers.clone(),
                t.collapsed_layers.clone(),
                t.doc.layer_pages(),
                visible_only.then(|| t.doc.layers_on_page(t.page)),
                !t.layer_view.is_empty(),
                t.doc.drawing_layer(),
            )
        })
    else {
        return rsx! {};
    };
    let mut shown = view.clone();
    if inherit {
        inherit_visibility(&tree, &mut shown);
    }
    let by_id: HashMap<Id, Layer> = layers.iter().map(|l| (l.id, l.clone())).collect();
    let mut rows = Vec::new();
    flatten_rows(
        &tree,
        &mut Vec::new(),
        &RowCtx {
            layers: &by_id,
            collapsed: &collapsed,
            filter: on_page.as_ref(),
        },
        &mut rows,
    );
    let has_layers = !layers.is_empty();
    let has_sel = !selected.is_empty();
    let one_sel = (selected.len() == 1).then(|| selected[0].clone());
    let reg = registry();
    let label = |id: &str| {
        reg.get(id)
            .map(|c| cmd_label(lang, c))
            .unwrap_or_else(|| id.to_string())
    };
    let tools: [(&str, &str, bool); 8] = [
        ("layersView.addNewLayer", "insert", true),
        ("layersView.addNewGroup", "folder", true),
        ("layersView.showSelected", "eye", has_layers),
        ("layersView.hideSelected", "eye-off", has_layers),
        ("layersView.merge", "merge", has_sel),
        ("layersView.flattenSelected", "flatten", has_sel),
        ("layersView.importAsLayer", "import", true),
        ("layersView.options", "settings", true),
    ];
    let rename_sel = one_sel.clone();
    let rename_group = one_sel
        .as_ref()
        .and_then(|p| node_at(&tree, p))
        .map(|n| n.layer_id().is_none())
        .unwrap_or(false);
    rsx! {
        div { class: "pane-toolbar icon-toolbar",
            for (id, icon, enabled) in tools {
                button { class: "ibtn", key: "{id}", title: "{label(id)}", disabled: !enabled, onclick: move |_| dispatch(st, id), Icon { name: icon } }
            }
            button { class: "ibtn", title: "{t(lang, \"layers.rename\")}", disabled: rename_sel.is_none(),
                onclick: move |_| { if let Some(p) = rename_sel.clone() { st.set_dialog(Dialog::LayerName { group: rename_group, rename: Some(p) }); } },
                Icon { name: "pencil" }
            }
        }
        if tree.is_empty() {
            div { class: "pane-empty",
                "{t(lang, \"layers.none\")}"
                div { class: "row", style: "justify-content:center;margin-top:8px",
                    button { class: "tbtn", onclick: move |_| dispatch(st, "layersView.addNewLayer"), "{label(\"layersView.addNewLayer\")}" }
                }
            }
        } else if rows.is_empty() {
            div { class: "pane-empty", "{t(lang, \"layers.none_on_page\")}" }
        }
        div { class: "tree layer-tree",
            for row in rows {
                {
                    let is_sel = selected.contains(&row.path);
                    let layer = row.id.and_then(|id| by_id.get(&id)).cloned();
                    let own = row.id.and_then(|id| view.get(&id).copied()).unwrap_or(true);
                    let effective = row.id.and_then(|id| shown.get(&id).copied()).unwrap_or(true);
                    let differs_default = layer.as_ref().map(|l| l.visible != own).unwrap_or(false);
                    let mut class = String::from("tree-item layer-row");
                    if is_sel { class.push_str(" selected"); }
                    if !effective { class.push_str(" hidden-layer"); }
                    if row.id.is_none() { class.push_str(" layer-group"); }
                    let is_drawing = row.id.is_some() && row.id == drawing;
                    if is_drawing { class.push_str(" drawing"); }
                    let style = format!("padding-left:{}px", 6 + row.depth * 14);
                    let count = row.id.and_then(|id| usage.get(&id)).map(|p| p.len()).unwrap_or(0);
                    let p_click = row.path.clone();
                    let p_dbl = row.path.clone();
                    let key_t = row.key.clone();
                    let is_group = row.id.is_none();
                    let eye_title = if own { t(lang, "layers.hide") } else { t(lang, "layers.show") };
                    let eye_icon = if own { "eye" } else { "eye-off" };
                    let eye_class = if differs_default { "ibtn eye changed" } else { "ibtn eye" };
                    let locked = layer.as_ref().map(|l| l.locked).unwrap_or(false);
                    let lock_title = if locked { t(lang, "layers.unlock") } else { t(lang, "layers.lock") };
                    let lock_class = if locked { "ibtn lock-btn on" } else { "ibtn lock-btn" };
                    let pages_title = tf(lang, "layers.pages", &[("count", &count.to_string())]);
                    rsx! {
                        div { class: "{class}", key: "{row.key}", style: "{style}", title: "{row.label}",
                            onclick: move |e| {
                                let m = e.modifiers();
                                let add = m.ctrl() || m.meta() || m.shift();
                                let p = p_click.clone();
                                st.with_active(|t| {
                                    if add {
                                        if let Some(i) = t.selected_layers.iter().position(|x| *x == p) {
                                            t.selected_layers.remove(i);
                                        } else {
                                            t.selected_layers.push(p);
                                        }
                                    } else {
                                        t.selected_layers = vec![p.clone()];
                                        // A click on a layer makes it the drawing layer again,
                                        // even when it was already the selected entry.
                                        let id = node_at(&t.doc.layer_tree(), &p).and_then(|n| n.layer_id());
                                        if let Some(id) = id {
                                            let _ = t.doc.set_drawing_layer(Some(id));
                                        }
                                    }
                                });
                            },
                            ondoubleclick: move |_| st.set_dialog(Dialog::LayerName { group: is_group, rename: Some(p_dbl.clone()) }),
                            span { class: "twisty",
                                onclick: move |e| {
                                    e.stop_propagation();
                                    let k = key_t.clone();
                                    st.with_active(|t| { if !t.collapsed_layers.remove(&k) { t.collapsed_layers.insert(k); } });
                                },
                                if row.has_children { if row.open { "▾" } else { "▸" } } else { "" }
                            }
                            if let Some(id) = row.id {
                                button { class: "{eye_class}", title: "{eye_title}",
                                    onclick: move |e| { e.stop_propagation(); set_visibility(st, &[id], None); },
                                    Icon { name: eye_icon }
                                }
                            } else {
                                span { class: "layer-group-icon", Icon { name: "folder" } }
                            }
                            span { class: "tree-label", "{row.label}" }
                            if is_drawing {
                                button { class: "ibtn drawing-mark", title: "{t(lang, \"layers.drawing_here\")}",
                                    onclick: move |e| { e.stop_propagation(); dispatch(st, "defaultLayer"); },
                                    Icon { name: "pencil" }
                                }
                            }
                            if let Some(id) = row.id {
                                button { class: "{lock_class}", title: "{lock_title}",
                                    onclick: move |e| {
                                        e.stop_propagation();
                                        if let Some(Err(err)) = st.with_active(|t| t.doc.set_layers_locked(&[id], !locked)) { st.error(err); }
                                        crate::files::set_window_title(st);
                                    },
                                    Icon { name: "lock" }
                                }
                                span { class: "page-ref", title: "{pages_title}", "{count}" }
                            }
                        }
                    }
                }
            }
        }
        if differs {
            div { class: "layer-foot",
                span { class: "hint", "{t(lang, \"layers.view_differs\")}" }
                div { class: "row", style: "gap:6px;flex-wrap:wrap",
                    button { class: "tbtn", onclick: move |_| dispatch(st, "layersView.setCurrentStatesAsDefault"), "{t(lang, \"layers.make_default\")}" }
                    button { class: "tbtn", onclick: move |_| dispatch(st, "layersView.resetOverrides"), "{t(lang, \"layers.reset\")}" }
                }
            }
        }
    }
}

/// Name of a new layer or group, or of an existing entry.
#[component]
pub fn LayerNameDialog(group: bool, rename: Option<Vec<usize>>) -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let initial = st
        .read_active(|t| {
            let tree = t.doc.layer_tree();
            match &rename {
                Some(p) => match node_at(&tree, p) {
                    Some(LayerNode::Layer { id, .. }) => t
                        .doc
                        .layers()
                        .into_iter()
                        .find(|l| l.id == *id)
                        .map(|l| l.name)
                        .unwrap_or_default(),
                    Some(LayerNode::Group { label, .. }) => label.clone(),
                    None => String::new(),
                },
                None if group => t_group_name(lang),
                None => tf(
                    lang,
                    "layers.default_name",
                    &[("n", &(t.doc.layers().len() + 1).to_string())],
                ),
            }
        })
        .unwrap_or_default();
    let mut name = use_signal(|| initial);
    let title = match (&rename, group) {
        (Some(_), _) => t(lang, "layers.rename"),
        (None, true) => t(lang, "layers.new_group"),
        (None, false) => t(lang, "layers.new_layer"),
    };
    let rename_ok = rename.clone();
    rsx! {
        Frame { title,
            on_ok: move |_| {
                let n = name();
                let r = st.with_active(|t| {
                    if let Some(p) = &rename_ok {
                        return t.doc.rename_layer_node(p, &n);
                    }
                    // Inside the selected group, otherwise at the top.
                    let tree = t.doc.layer_tree();
                    let parent = match t.selected_layers.as_slice() {
                        [p] if matches!(node_at(&tree, p), Some(LayerNode::Group { .. })) => p.clone(),
                        _ => Vec::new(),
                    };
                    let path = if group {
                        t.doc.add_layer_group(&n, &parent)?
                    } else {
                        t.doc.add_layer(&n, &parent)?.1
                    };
                    t.collapsed_layers.remove(&key_of(&parent));
                    t.selected_layers = vec![path];
                    Ok(())
                });
                match r {
                    Some(Err(e)) => st.core_error(&e),
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
        }
    }
}

fn t_group_name(lang: crate::i18n::Lang) -> String {
    t(lang, "layers.default_group")
}

/// Show or hide every layer in the view, or go back to the defaults.
#[component]
pub fn LayerOverridesDialog() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let mut choice = use_signal(|| "show".to_string());
    let options = [
        ("show", t(lang, "layers.override.show")),
        ("hide", t(lang, "layers.override.hide")),
        ("default", t(lang, "layers.override.default")),
    ];
    rsx! {
        Frame { title: t(lang, "layers.override.title"),
            on_ok: move |_| {
                let v = match choice().as_str() {
                    "show" => Some(true),
                    "hide" => Some(false),
                    _ => None,
                };
                override_all(st, v);
                st.set_dialog(Dialog::None);
            },
            div { class: "hint", "{t(lang, \"layers.override.hint\")}" }
            div { class: "radio-col",
                for (k, text) in options {
                    label { key: "{k}", class: "check",
                        input { r#type: "radio", name: "layer-override", checked: choice() == k, onchange: move |_| choice.set(k.to_string()) }
                        span { "{text}" }
                    }
                }
            }
        }
    }
}

/// Merge the selected layers into a chosen layer.
#[component]
pub fn LayerMergeDialog() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let sources = selected_ids(st, false);
    let layers = st.read_active(|t| t.doc.layers()).unwrap_or_default();
    let first = sources.first().copied();
    let mut target: Signal<Option<Id>> = use_signal(|| first);
    let names: HashMap<Id, String> = layers.iter().map(|l| (l.id, l.name.clone())).collect();
    let merged: Vec<String> = sources
        .iter()
        .filter(|id| Some(**id) != target())
        .filter_map(|id| names.get(id).cloned())
        .collect();
    let merged_text = merged.join(", ");
    let sources_ok = sources.clone();
    rsx! {
        Frame { title: t(lang, "layers.merge.title"),
            on_ok: move |_| {
                if let Some(tg) = target() {
                    let src: Vec<Id> = sources_ok.iter().copied().filter(|s| *s != tg).collect();
                    if src.is_empty() {
                        st.toast(crate::state::ToastKind::Info, t(lang, "toast.select_two_layers"), "");
                        return;
                    }
                    st.set_dialog(Dialog::None);
                    merge_into(st, &src, tg);
                }
            },
            div { class: "field",
                label { "{t(lang, \"layers.merge.into\")}" }
                select {
                    onchange: move |e| {
                        let v = e.value();
                        target.set(layers_lookup(&v));
                    },
                    for l in layers.iter() {
                        option { key: "{l.id.0}", value: "{l.id.0}-{l.id.1}", selected: Some(l.id) == target(), "{l.name}" }
                    }
                }
            }
            div { class: "field",
                label { "{t(lang, \"layers.merge.layers\")}" }
                div { class: "macro-preview", "{merged_text}" }
            }
            div { class: "hint", "{t(lang, \"layers.merge.hint\")}" }
        }
    }
}

fn layers_lookup(v: &str) -> Option<Id> {
    let (a, b) = v.split_once('-')?;
    Some((a.parse().ok()?, b.parse().ok()?))
}

/// Draw the pages of another file as a new layer.
#[component]
pub fn ImportLayerDialog(path: String) -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let stem = std::path::Path::new(&path)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "Layer".into());
    let mut name = use_signal(|| stem);
    let mut all = use_signal(|| false);
    let mut behind = use_signal(|| false);
    let file = path.clone();
    rsx! {
        Frame { title: t(lang, "layers.import.title"),
            on_ok: move |_| {
                let other = match Document::open(&file) {
                    Ok(d) => d,
                    Err(e) => {
                        st.error(e);
                        return;
                    }
                };
                let n = name();
                let r = st.with_active(|t| {
                    let pages: Vec<usize> = if all() { (0..t.doc.page_count()).collect() } else { vec![t.page] };
                    t.doc.import_as_layer(&other, &pages, behind(), &n).map(|_| ())
                });
                match r {
                    Some(Err(e)) => st.error(e),
                    _ => {
                        crate::files::set_window_title(st);
                        st.set_dialog(Dialog::None);
                    }
                }
            },
            div { class: "hint", "{path}" }
            div { class: "field",
                label { "{t(lang, \"layers.name\")}" }
                input { value: "{name}", autofocus: true, oninput: move |e| name.set(e.value()) }
            }
            div { class: "field",
                label { "{t(lang, \"dialog.pages\")}" }
                div { class: "radio-row",
                    label { input { r#type: "radio", name: "import-pages", checked: !all(), onchange: move |_| all.set(false) } "{t(lang, \"dialog.pages.current\")}" }
                    label { input { r#type: "radio", name: "import-pages", checked: all(), onchange: move |_| all.set(true) } "{t(lang, \"dialog.pages.all\")}" }
                }
            }
            label { class: "check",
                input { r#type: "checkbox", checked: behind(), onchange: move |_| behind.set(!behind()) }
                span { "{t(lang, \"layers.import.behind\")}" }
            }
        }
    }
}

/// Options of the layers pane.
#[component]
pub fn LayerOptionsDialog() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let visible_only = pages_mode(st) == "visible";
    let inherit = option_on(st, "layersView.overrideByParent");
    let reg = registry();
    let label = |id: &str| {
        reg.get(id)
            .map(|c| cmd_label(lang, c))
            .unwrap_or_else(|| id.to_string())
    };
    rsx! {
        Frame { title: t(lang, "layers.options.title"),
            div { class: "field",
                label { "{t(lang, \"layers.options.list\")}" }
                div { class: "radio-col",
                    label { class: "check",
                        input { r#type: "radio", name: "layer-pages", checked: !visible_only, onchange: move |_| dispatch(st, "layersView.viewAllPages") }
                        span { "{label(\"layersView.viewAllPages\")}" }
                    }
                    label { class: "check",
                        input { r#type: "radio", name: "layer-pages", checked: visible_only, onchange: move |_| dispatch(st, "layersView.viewVisiblePages") }
                        span { "{label(\"layersView.viewVisiblePages\")}" }
                    }
                }
            }
            label { class: "check",
                input { r#type: "checkbox", checked: inherit, onchange: move |_| dispatch(st, "layersView.overrideByParent") }
                span { "{label(\"layersView.overrideByParent\")}" }
            }
            div { class: "hint", "{t(lang, \"layers.options.hint\")}" }
        }
    }
}

/// The value of a layer in a choice list ("none": no layer).
fn layer_value(id: Option<Id>) -> String {
    match id {
        Some((a, b)) => format!("{a}-{b}"),
        None => "none".into(),
    }
}

/// Choose the layer new drawings and text go to, or (when comments are
/// selected) the layer of the selected comments.
#[component]
pub fn ChooseLayerDialog() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let (layers, drawing, chosen) = st
        .read_active(|t| {
            let chosen = t.chosen_annots().map(|(p, list)| {
                (
                    p,
                    list.clone(),
                    list.first().and_then(|i| t.doc.annotation_layer(p, *i)),
                )
            });
            (t.doc.layers(), t.doc.drawing_layer(), chosen)
        })
        .unwrap_or_default();
    let has_comments = chosen.is_some();
    let comment_layer = chosen.as_ref().and_then(|c| c.2);
    let mut for_comments = use_signal(|| has_comments);
    let mut choice = use_signal(|| layer_value(if has_comments { comment_layer } else { drawing }));
    let reg = registry();
    let title = reg
        .get("chooseLayer")
        .map(|c| cmd_label(lang, c))
        .unwrap_or_default();
    let hint = if for_comments() {
        t(lang, "layers.choose.comments_hint")
    } else {
        t(lang, "layers.choose.drawing_hint")
    };
    rsx! {
        Frame { title,
            on_ok: move |_| {
                let layer = layers_lookup(&choice());
                st.set_dialog(Dialog::None);
                if for_comments() {
                    crate::commands::layers::set_comments_layer(st, layer);
                } else {
                    crate::commands::layers::set_drawing_layer(st, layer);
                }
            },
            if has_comments {
                div { class: "field",
                    label { "{t(lang, \"layers.choose.for\")}" }
                    div { class: "radio-col",
                        label { class: "check",
                            input { r#type: "radio", name: "choose-layer-for", checked: for_comments(),
                                onchange: move |_| { for_comments.set(true); choice.set(layer_value(comment_layer)); } }
                            span { "{t(lang, \"layers.choose.comments\")}" }
                        }
                        label { class: "check",
                            input { r#type: "radio", name: "choose-layer-for", checked: !for_comments(),
                                onchange: move |_| { for_comments.set(false); choice.set(layer_value(drawing)); } }
                            span { "{t(lang, \"layers.choose.drawing\")}" }
                        }
                    }
                }
            }
            div { class: "field",
                label { "{t(lang, \"layers.choose.layer\")}" }
                select { class: "choose-layer",
                    onchange: move |e| choice.set(e.value()),
                    option { value: "none", selected: choice() == "none", "{t(lang, \"layers.no_layer\")}" }
                    for l in layers.iter() {
                        option { key: "{l.id.0}-{l.id.1}", value: "{layer_value(Some(l.id))}", selected: choice() == layer_value(Some(l.id)), "{l.name}" }
                    }
                }
            }
            div { class: "hint", "{hint}" }
        }
    }
}

/// Name of the new layer the selected comments, or the selected picture,
/// move to.
#[component]
pub fn MoveToNewLayerDialog() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let (count, picture, n) = st
        .read_active(|t| {
            (
                t.chosen_annots().map(|(_, l)| l.len()).unwrap_or(0),
                t.image_sel,
                t.doc.layers().len(),
            )
        })
        .unwrap_or_default();
    let mut name = use_signal(|| tf(lang, "layers.default_name", &[("n", &(n + 1).to_string())]));
    let reg = registry();
    let title = reg
        .get("moveToNewLayer")
        .map(|c| cmd_label(lang, c))
        .unwrap_or_default();
    let what = if count > 0 {
        tf(lang, "layers.move.comments", &[("count", &count.to_string())])
    } else {
        t(lang, "layers.move.picture")
    };
    rsx! {
        Frame { title,
            on_ok: move |_| {
                let n = name();
                let r = st.with_active(|t| match (t.chosen_annots(), t.image_sel) {
                    (Some((page, list)), _) => t.doc.move_annotations_to_new_layer(page, &list, &n),
                    (None, Some((page, index))) => t.doc.move_picture_to_new_layer(page, index, &n),
                    (None, None) => Err(folitecta_core::Error::Invalid("nothing is selected".into())),
                });
                match r {
                    Some(Ok(_)) => {
                        crate::files::set_window_title(st);
                        st.set_dialog(Dialog::None);
                        st.toast(crate::state::ToastKind::Ok, tf(lang, "toast.moved_to_layer", &[("name", n.trim())]), "");
                    }
                    Some(Err(e)) => st.core_error(&e),
                    None => {}
                }
            },
            div { class: "field",
                label { "{t(lang, \"layers.name\")}" }
                input { value: "{name}", autofocus: true, oninput: move |e| name.set(e.value()) }
            }
            div { class: "hint", "{what}" }
            if picture.is_some() && count == 0 {
                div { class: "hint", "{t(lang, \"layers.move.picture_hint\")}" }
            }
        }
    }
}
