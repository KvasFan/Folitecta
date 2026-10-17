//! Fields pane: fields grouped by their dotted names, sorted by name or page,
//! or listed by page in tab order to be moved by hand; and its options.

use std::collections::{BTreeMap, HashSet};

use dioxus::html::ModifiersInteraction;
use dioxus::prelude::*;
use folitecta_core::{FieldKind, FormField, registry};

use crate::commands::fields_pane::{manual_order, move_in_tab_order, select_field, sort_mode};
use crate::commands::{dispatch, option_on};
use crate::i18n::{Lang, cmd_label, t, tf};
use crate::icons::Icon;
use crate::state::{AppState, Dialog};
use crate::ui::dialogs::Frame;

fn kind_name(lang: Lang, k: &FieldKind) -> String {
    let key = match k {
        FieldKind::Text => "fieldkind.text",
        FieldKind::CheckBox => "fieldkind.check",
        FieldKind::RadioButton => "fieldkind.radio",
        FieldKind::ComboBox => "fieldkind.combo",
        FieldKind::ListBox => "fieldkind.list",
        FieldKind::PushButton => "fieldkind.button",
        FieldKind::Signature => "fieldkind.signature",
        FieldKind::Unknown(_) => "fieldkind.unknown",
    };
    t(lang, key)
}

/// A row of the pane: a name group, a page heading or a field.
#[derive(Clone, PartialEq)]
enum Row {
    Group {
        key: String,
        label: String,
        depth: usize,
        open: bool,
    },
    Page(usize),
    Field {
        field: Box<FormField>,
        depth: usize,
        label: String,
        tab: Option<(usize, usize)>,
    },
}

/// Order the fields: document order, by name, or by page and position.
fn sorted(mut fields: Vec<FormField>, mode: &str) -> Vec<FormField> {
    match mode {
        "alphabetic" => fields.sort_by_key(|f| f.name.to_lowercase()),
        "pages" => fields.sort_by(|a, b| {
            let key = |f: &FormField| {
                let r = f.rect.unwrap_or([0.0; 4]);
                (f.page.unwrap_or(usize::MAX), -(r[3] as i64), r[0] as i64)
            };
            key(a).cmp(&key(b))
        }),
        _ => {}
    }
    fields
}

/// Rows of the name tree; groups keep the order of their first field.
fn tree_rows(fields: &[FormField], collapsed: &HashSet<String>, full: bool) -> Vec<Row> {
    let mut rows = Vec::new();
    let mut opened: Vec<String> = Vec::new();
    for f in fields {
        let parts: Vec<&str> = f.name.split('.').collect();
        // Close the groups this field is not in.
        while let Some(last) = opened.last() {
            if f.name.starts_with(&format!("{last}.")) {
                break;
            }
            opened.pop();
        }
        let mut hidden = opened.iter().any(|g| collapsed.contains(g));
        for depth in opened.len()..parts.len().saturating_sub(1) {
            let key = parts[..=depth].join(".");
            if !hidden {
                rows.push(Row::Group {
                    key: key.clone(),
                    label: parts[depth].to_string(),
                    depth,
                    open: !collapsed.contains(&key),
                });
            }
            hidden |= collapsed.contains(&key);
            opened.push(key);
        }
        if !hidden {
            let depth = parts.len().saturating_sub(1);
            let label = if full {
                f.name.clone()
            } else {
                f.short_name().to_string()
            };
            rows.push(Row::Field {
                field: Box::new(f.clone()),
                depth,
                label,
                tab: None,
            });
        }
    }
    rows
}

#[component]
pub fn FieldsPane() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let mode = sort_mode(st);
    let manual = manual_order(st);
    let full = option_on(st, "fieldsView.showFullNames");
    let Some((fields, selected, collapsed, orders)) = st.read_active(|t| {
        let fields = t.doc.form_fields();
        let orders: BTreeMap<usize, Vec<(u32, u16)>> = if manual {
            (0..t.doc.page_count())
                .map(|p| (p, t.doc.widget_order(p)))
                .filter(|(_, w)| !w.is_empty())
                .collect()
        } else {
            BTreeMap::new()
        };
        (
            fields,
            t.selected_fields.clone(),
            t.collapsed_fields.clone(),
            orders,
        )
    }) else {
        return rsx! {};
    };
    let count = fields.len();
    let rows: Vec<Row> = if manual {
        // Pages in tab order: widgets as the page lists them.
        let mut rows = Vec::new();
        for (page, widgets) in &orders {
            rows.push(Row::Page(*page));
            let n = widgets.len();
            for (pos, w) in widgets.iter().enumerate() {
                if let Some(f) = fields.iter().find(|f| f.widgets.contains(w)) {
                    let mut f = f.clone();
                    // The widget this row moves.
                    f.widgets = vec![*w];
                    // Flat lists show full names: there is no group to give the context.
                    let label = f.name.clone();
                    rows.push(Row::Field {
                        field: Box::new(f),
                        depth: 0,
                        label,
                        tab: Some((pos, n)),
                    });
                }
            }
        }
        rows
    } else if mode == "pages" {
        let mut rows = Vec::new();
        let mut last: Option<Option<usize>> = None;
        for f in sorted(fields.clone(), "pages") {
            if last != Some(f.page) {
                if let Some(p) = f.page {
                    rows.push(Row::Page(p));
                }
                last = Some(f.page);
            }
            let label = f.name.clone();
            rows.push(Row::Field {
                field: Box::new(f),
                depth: 0,
                label,
                tab: None,
            });
        }
        rows
    } else {
        tree_rows(&sorted(fields.clone(), &mode), &collapsed, full)
    };
    let reg = registry();
    let label = |id: &str| {
        reg.get(id)
            .map(|c| cmd_label(lang, c))
            .unwrap_or_else(|| id.to_string())
    };
    let alpha_on = mode == "alphabetic";
    let pages_on = mode == "pages";
    let tools: [(&str, &str, bool, bool); 6] = [
        ("fieldsView.expandAll", "zoom-in", count > 0 && !manual, false),
        ("fieldsView.collapseAll", "zoom-out", count > 0 && !manual, false),
        (
            "fieldsView.sort.alphabetic",
            "text",
            count > 0 && !manual,
            alpha_on,
        ),
        ("fieldsView.sort.pages", "pages", count > 0 && !manual, pages_on),
        ("fieldsView.tabOrder.manually", "move", count > 0, manual),
        ("fieldsView.options", "settings", true, false),
    ];
    rsx! {
        div { class: "pane-toolbar icon-toolbar",
            for (id, icon, enabled, on) in tools {
                button { class: if on { "ibtn on" } else { "ibtn" }, key: "{id}", title: "{label(id)}", disabled: !enabled, onclick: move |_| dispatch(st, id), Icon { name: icon } }
            }
        }
        if manual {
            div { class: "fields-manual",
                span { class: "hint", "{t(lang, \"fields.manual_hint\")}" }
                button { class: "tbtn", onclick: move |_| st.set_value("fields.manual", ""), "{t(lang, \"fields.manual_done\")}" }
            }
        }
        if fields.is_empty() { div { class: "pane-empty", "{t(lang, \"label.none\")}" } }
        div { class: "tree field-tree",
            for (i, row) in rows.into_iter().enumerate() {
                match row {
                    Row::Page(p) => rsx! {
                        div { class: "field-page", key: "p{i}", "{t(lang, \"status.page\")} {p + 1}" }
                    },
                    Row::Group { key, label, depth, open } => {
                        let k = key.clone();
                        let style = format!("padding-left:{}px", 6 + depth * 14);
                        rsx! {
                            div { class: "tree-item field-group", key: "g{key}", style: "{style}",
                                onclick: move |_| { let k = k.clone(); st.with_active(|t| { if !t.collapsed_fields.remove(&k) { t.collapsed_fields.insert(k); } }); },
                                span { class: "twisty", if open { "▾" } else { "▸" } }
                                span { class: "tree-label", "{label}" }
                            }
                        }
                    }
                    Row::Field { field, depth, label, tab } => {
                        let f = *field;
                        let is_sel = selected.contains(&f.id);
                        let style = format!("padding-left:{}px", 6 + depth * 14 + 16);
                        let kind = kind_name(lang, &f.kind);
                        let kind_text = if f.barcode { t(lang, "fieldkind.barcode") } else { kind };
                        let is_check = matches!(f.kind, FieldKind::CheckBox | FieldKind::RadioButton);
                        let editable = matches!(f.kind, FieldKind::Text | FieldKind::ComboBox | FieldKind::ListBox) && !f.read_only;
                        let checked = !f.value.is_empty() && f.value != "Off";
                        let id = f.id;
                        let f_click = f.clone();
                        let (name_d, value_d) = (f.name.clone(), f.value.clone());
                        let widget = f.widgets.first().copied();
                        let tip = format!("{}\n{}", f.name, kind_text);
                        let class = if is_sel { "tree-item field-row selected" } else { "tree-item field-row" };
                        let value = f.value.clone();
                        let up_title = t(lang, "fields.move_up");
                        let down_title = t(lang, "fields.move_down");
                        let order_title = tab.map(|(pos, n)| tf(lang, "fields.tab_pos", &[("n", &(pos + 1).to_string()), ("of", &n.to_string())])).unwrap_or_default();
                        rsx! {
                            div { class: "{class}", key: "f{i}-{id.0}", style: "{style}", title: "{tip}",
                                onclick: move |e| {
                                    let m = e.modifiers();
                                    let add = m.ctrl() || m.meta() || m.shift();
                                    st.with_active(|t| select_field(t, &f_click, add));
                                    if let Some(p) = f_click.page { st.go_to_page(p); }
                                },
                                ondoubleclick: move |_| {
                                    if editable {
                                        st.set_dialog(Dialog::FieldValue { id, name: name_d.clone(), value: value_d.clone() });
                                    }
                                },
                                if let Some((pos, _)) = tab { span { class: "chip", title: "{order_title}", "{pos + 1}" } }
                                Icon { name: "field" }
                                span { class: "tree-label", "{label}" }
                                if is_check {
                                    input { r#type: "checkbox", checked: checked, title: "{value}",
                                        onclick: move |e| e.stop_propagation(),
                                        onchange: move |_| { let v = if checked { "Off" } else { "Yes" }; if let Some(Err(e)) = st.with_active(|t| t.doc.set_field_value(id, v)) { st.error(e); } }
                                    }
                                } else if !value.is_empty() {
                                    span { class: "field-value", title: "{value}", "{value}" }
                                }
                                if let (Some((pos, n)), Some(w)) = (tab, widget) {
                                    button { class: "ibtn", title: "{up_title}", disabled: pos == 0,
                                        onclick: move |e| { e.stop_propagation(); move_in_tab_order(st, w, -1); },
                                        Icon { name: "up" }
                                    }
                                    button { class: "ibtn", title: "{down_title}", disabled: pos + 1 >= n,
                                        onclick: move |e| { e.stop_propagation(); move_in_tab_order(st, w, 1); },
                                        Icon { name: "down" }
                                    }
                                } else {
                                    span { class: "chip", "{kind_text}" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Options of the fields pane.
#[component]
pub fn FieldsOptionsDialog() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let reg = registry();
    let label = |id: &str| {
        reg.get(id)
            .map(|c| cmd_label(lang, c))
            .unwrap_or_else(|| id.to_string())
    };
    let mode = sort_mode(st);
    let full = option_on(st, "fieldsView.showFullNames");
    let manual = manual_order(st);
    rsx! {
        Frame { title: t(lang, "fields.options.title"),
            div { class: "field",
                label { "{label(\"fieldsView.sort\")}" }
                div { class: "radio-col",
                    label { class: "check",
                        input { r#type: "radio", name: "fields-sort", checked: mode.is_empty(), onchange: move |_| st.set_value("fields.sort", "") }
                        span { "{t(lang, \"fields.sort.document\")}" }
                    }
                    label { class: "check",
                        input { r#type: "radio", name: "fields-sort", checked: mode == "alphabetic", onchange: move |_| st.set_value("fields.sort", "alphabetic") }
                        span { "{label(\"fieldsView.sort.alphabetic\")}" }
                    }
                    label { class: "check",
                        input { r#type: "radio", name: "fields-sort", checked: mode == "pages", onchange: move |_| st.set_value("fields.sort", "pages") }
                        span { "{label(\"fieldsView.sort.pages\")}" }
                    }
                }
            }
            label { class: "check",
                input { r#type: "checkbox", checked: full, onchange: move |_| dispatch(st, "fieldsView.showFullNames") }
                span { "{label(\"fieldsView.showFullNames\")}" }
            }
            label { class: "check",
                input { r#type: "checkbox", checked: manual, onchange: move |_| { if manual { st.set_value("fields.manual", "") } else { dispatch(st, "fieldsView.tabOrder.manually") } } }
                span { "{t(lang, \"fields.manual_option\")}" }
            }
        }
    }
}
