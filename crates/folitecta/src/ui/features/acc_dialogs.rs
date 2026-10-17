//! Accessibility dialogs: new tags and their properties, tags from a
//! selection, role map and class map, alternate text of the figures, pane
//! options, the meaning of a rule and the language of the document.

use base64::Engine;
use dioxus::prelude::*;
use folitecta_core::accessibility::{CATEGORIES, Fix, rule};
use folitecta_core::registry;
use folitecta_core::structure::{STANDARD_TYPES, StructProps};

use crate::commands::accessibility::{Id, fix_rule, run_check, select_tag};
use crate::commands::{option_on, show_pane};
use crate::i18n::{Lang, cmd_label, t};
use crate::state::{AppState, Dialog, Pane};
use crate::ui::panes::accessibility::rule_name;

fn mode_label(lang: Lang, m: &str) -> String {
    label_of(lang, &format!("orderview.showmode.{m}"))
}

fn cat_label(lang: Lang, cat: &str) -> String {
    t(lang, &format!("acc.cat.{cat}"))
}
use crate::ui::dialogs::Frame;

fn label_of(lang: Lang, id: &str) -> String {
    registry()
        .get(id)
        .map(|c| cmd_label(lang, c))
        .unwrap_or_else(|| id.to_string())
}

/// The standard types offered as tags.
#[component]
fn TypeList() -> Element {
    rsx! {
        datalist { id: "struct-types",
            for ty in STANDARD_TYPES.iter() { option { key: "{ty}", value: "{ty}" } }
        }
    }
}

fn done(st: AppState) {
    crate::files::set_window_title(st);
    st.set_dialog(Dialog::None);
}

#[component]
pub fn TagNewDialog(parent: Option<Id>, index: Option<usize>) -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let mut tag = use_signal(|| "P".to_string());
    let mut title = use_signal(String::new);
    let inside = parent
        .and_then(|p| {
            st.read_active(|t| t.doc.structure().and_then(|tr| tr.find(p).map(|e| e.tag.clone())))
                .flatten()
        })
        .unwrap_or_else(|| t(lang, "tags.root"));
    let inside_text = format!("{} <{}>", t(lang, "tags.inside"), inside);
    rsx! {
        Frame { title: t(lang, "tags.new"),
            on_ok: move |_| {
                let r = st.with_active(|t| t.doc.add_struct_elem(parent, index, &tag(), &title()));
                match r {
                    Some(Ok(id)) => { done(st); show_pane(st, Pane::Tags); select_tag(st, Some(id)); }
                    Some(Err(e)) => st.error(e),
                    None => {}
                }
            },
            TypeList {}
            p { class: "muted", "{inside_text}" }
            div { class: "field-grid",
                div { class: "field", label { "{t(lang, \"tags.type\")}" }
                    input { list: "struct-types", value: "{tag}", autofocus: true, oninput: move |e| tag.set(e.value()) } }
                div { class: "field", label { "{t(lang, \"tags.title\")}" }
                    input { value: "{title}", oninput: move |e| title.set(e.value()) } }
            }
        }
    }
}

#[component]
pub fn TagPropsDialog(id: Id) -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let info = st
        .read_active(|t| {
            let tree = t.doc.structure()?;
            let e = tree.find(id)?.clone();
            let text = t.doc.struct_text(&e);
            Some((e, text))
        })
        .flatten();
    let Some((elem, text)) = info else {
        return rsx! { Frame { title: t(lang, "tags.props"), p { "{t(lang, \"toast.select_tag\")}" } } };
    };
    let mut props = use_signal(|| StructProps {
        tag: elem.tag.clone(),
        title: elem.title.clone(),
        alt: elem.alt.clone(),
        actual_text: elem.actual_text.clone(),
        lang: elem.lang.clone(),
    });
    let role = if elem.role != elem.tag {
        elem.role.clone()
    } else {
        String::new()
    };
    let pages: Vec<String> = elem
        .content()
        .iter()
        .map(|c| (c.0 + 1).to_string())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    let excerpt = folitecta_core::structure::excerpt(&text, 200);
    rsx! {
        Frame { title: t(lang, "tags.props"), wide: true,
            on_ok: move |_| {
                let p = props();
                let r = st.with_active(|t| t.doc.set_struct_props(id, &p));
                match r {
                    Some(Ok(())) => done(st),
                    Some(Err(e)) => st.error(e),
                    None => {}
                }
            },
            TypeList {}
            div { class: "field-grid",
                div { class: "field", label { "{t(lang, \"tags.type\")}" }
                    input { list: "struct-types", value: "{props.read().tag}", oninput: move |e| props.write().tag = e.value() } }
                div { class: "field", label { "{t(lang, \"tags.title\")}" }
                    input { value: "{props.read().title}", oninput: move |e| props.write().title = e.value() } }
                div { class: "field", label { "{t(lang, \"tags.lang\")}" }
                    input { value: "{props.read().lang}", placeholder: "en-US", oninput: move |e| props.write().lang = e.value() } }
                div { class: "field", label { "{t(lang, \"tags.actual\")}" }
                    input { value: "{props.read().actual_text}", oninput: move |e| props.write().actual_text = e.value() } }
            }
            div { class: "field", label { "{t(lang, \"tags.alt\")}" }
                textarea { rows: "3", value: "{props.read().alt}", oninput: move |e| props.write().alt = e.value() } }
            if !role.is_empty() { p { class: "muted", "{t(lang, \"tags.role\")}: {role}" } }
            if !pages.is_empty() { p { class: "muted", "{t(lang, \"tags.pages\")}: {pages.join(\", \")}" } }
            if !excerpt.is_empty() { p { class: "acc-excerpt", "{excerpt}" } }
        }
    }
}

#[component]
pub fn TagFromAreaDialog(page: usize, rect: Vec<f32>) -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let mut tag = use_signal(|| "P".to_string());
    let parent = crate::commands::accessibility::selected_tag(st);
    let r4 = [rect[0], rect[1], rect[2], rect[3]];
    rsx! {
        Frame { title: label_of(lang, "tagsview.createTagFromSelection"),
            on_ok: move |_| {
                let r = st.with_active(|t| { t.text_sel = None; t.doc.tag_area(page, r4, &tag(), parent, None) });
                match r {
                    Some(Ok(id)) => { done(st); show_pane(st, Pane::Tags); select_tag(st, Some(id)); }
                    Some(Err(e)) => st.error(e),
                    None => {}
                }
            },
            TypeList {}
            div { class: "field", label { "{t(lang, \"tags.type\")}" }
                input { list: "struct-types", value: "{tag}", autofocus: true, oninput: move |e| tag.set(e.value()) } }
            p { class: "hint", "{t(lang, \"tags.area_hint\")}" }
        }
    }
}

/// Rows of a two-column editor (role map, class map).
#[component]
fn PairRows(rows: Signal<Vec<(String, String)>>, left: String, right: String, right_list: bool) -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let n = rows.read().len();
    rsx! {
        div { class: "pair-rows",
            div { class: "pair-row head", span { "{left}" } span { "{right}" } span {} }
            for i in 0..n {
                div { class: "pair-row", key: "{i}",
                    input { value: "{rows.read()[i].0}", oninput: move |e| rows.write()[i].0 = e.value() }
                    if right_list {
                        input { list: "struct-types", value: "{rows.read()[i].1}", oninput: move |e| rows.write()[i].1 = e.value() }
                    } else {
                        input { value: "{rows.read()[i].1}", oninput: move |e| rows.write()[i].1 = e.value() }
                    }
                    button { class: "ibtn", title: "{t(lang, \"dialog.remove\")}", onclick: move |_| { rows.write().remove(i); }, "×" }
                }
            }
            button { class: "tbtn", onclick: move |_| rows.write().push((String::new(), String::new())), "{t(lang, \"dialog.add\")}" }
        }
    }
}

#[component]
pub fn RoleMapDialog() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let rows = use_signal(|| st.read_active(|t| t.doc.role_map()).unwrap_or_default());
    rsx! {
        Frame { title: label_of(lang, "tagsview.editRoleMap"), wide: true,
            on_ok: move |_| {
                let map = rows();
                let r = st.with_active(|t| t.doc.set_role_map(&map));
                match r { Some(Ok(())) => done(st), Some(Err(e)) => st.error(e), None => {} }
            },
            TypeList {}
            p { class: "hint", "{t(lang, \"tags.rolemap_hint\")}" }
            PairRows { rows, left: t(lang, "tags.rolemap_tag"), right: t(lang, "tags.rolemap_type"), right_list: true }
        }
    }
}

#[component]
pub fn ClassMapDialog() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let rows = use_signal(|| st.read_active(|t| t.doc.class_map()).unwrap_or_default());
    rsx! {
        Frame { title: label_of(lang, "tagsview.editClassMap"), wide: true,
            on_ok: move |_| {
                let map = rows();
                let r = st.with_active(|t| t.doc.set_class_map(&map));
                match r { Some(Ok(())) => done(st), Some(Err(e)) => st.error(e), None => {} }
            },
            p { class: "hint", "{t(lang, \"tags.classmap_hint\")}" }
            PairRows { rows, left: t(lang, "tags.classmap_class"), right: t(lang, "tags.classmap_attrs"), right_list: false }
        }
    }
}

/// Alternate text of the figures, one after the other.
#[component]
pub fn AltTextDialog() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let figures = st
        .read_active(|t| t.doc.struct_elems_of("Figure"))
        .unwrap_or_default();
    let mut index = use_signal(|| 0usize);
    let mut texts = use_signal(|| figures.iter().map(|f| f.alt.clone()).collect::<Vec<_>>());
    let mut decorative = use_signal(|| vec![false; figures.len()]);
    let n = figures.len();
    let i = index().min(n.saturating_sub(1));
    let current = figures.get(i).cloned();
    let preview = current.as_ref().and_then(|f| {
        let p = f.first_page()?;
        st.read_active(|t| {
            let r = t.doc.struct_rect(f, p)?;
            let pad = 4.0;
            let png = t
                .doc
                .render_region_png(p, [r[0] - pad, r[1] - pad, r[2] + pad, r[3] + pad], 1.5)
                .ok()?;
            Some(format!(
                "data:image/png;base64,{}",
                base64::engine::general_purpose::STANDARD.encode(png)
            ))
        })
        .flatten()
    });
    let page = current
        .as_ref()
        .and_then(|f| f.first_page())
        .map(|p| p + 1)
        .unwrap_or(0);
    let ids: Vec<Id> = figures.iter().map(|f| f.id).collect();
    rsx! {
        Frame { title: label_of(lang, "acc.alt"), wide: true,
            on_ok: move |_| {
                let alts = texts();
                let deco = decorative();
                let r = st.with_active(|t| -> folitecta_core::Result<()> {
                    let tree = t.doc.structure();
                    for (k, id) in ids.iter().enumerate() {
                        if deco.get(k).copied().unwrap_or(false) {
                            t.doc.struct_to_artifact(*id)?;
                            continue;
                        }
                        let Some(e) = tree.as_ref().and_then(|tr| tr.find(*id)) else { continue };
                        let alt = alts.get(k).cloned().unwrap_or_default();
                        if alt.trim() != e.alt.trim() {
                            t.doc.set_struct_props(*id, &StructProps { tag: e.tag.clone(), title: e.title.clone(), alt, actual_text: e.actual_text.clone(), lang: e.lang.clone() })?;
                        }
                    }
                    Ok(())
                });
                match r {
                    Some(Ok(())) => {
                        done(st);
                        if st.read_active(|t| t.acc.check.is_some()).unwrap_or(false) { run_check(st); }
                    }
                    Some(Err(e)) => st.error(e),
                    None => {}
                }
            },
            if n == 0 {
                p { "{t(lang, \"toast.no_figures\")}" }
            } else {
                div { class: "alt-nav",
                    button { class: "ibtn", disabled: i == 0, onclick: move |_| index.set(i.saturating_sub(1)), "‹" }
                    span { "{t(lang, \"alt.figure\")} {i + 1} / {n} · {t(lang, \"status.page\")} {page}" }
                    button { class: "ibtn", disabled: i + 1 >= n, onclick: move |_| index.set(i + 1), "›" }
                }
                div { class: "alt-preview",
                    if let Some(src) = preview { img { src: "{src}", alt: "" } } else { span { class: "muted", "{t(lang, \"alt.no_preview\")}" } }
                }
                div { class: "field", label { "{t(lang, \"tags.alt\")}" }
                    textarea { rows: "3", disabled: decorative.read().get(i).copied().unwrap_or(false),
                        value: "{texts.read().get(i).cloned().unwrap_or_default()}",
                        oninput: move |e| { if let Some(x) = texts.write().get_mut(i) { *x = e.value(); } } }
                }
                label { class: "check",
                    input { r#type: "checkbox", checked: decorative.read().get(i).copied().unwrap_or(false),
                        onchange: move |_| { if let Some(x) = decorative.write().get_mut(i) { *x = !*x; } } }
                    span { "{t(lang, \"alt.decorative\")}" }
                }
            }
        }
    }
}

/// Options of an accessibility pane.
#[component]
pub fn AccOptionsDialog(pane: String) -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let check = |id: &'static str| {
        let on = option_on(st, id);
        rsx! {
            label { class: "check", key: "{id}",
                input { r#type: "checkbox", checked: on, onchange: move |_| st.set_option(id, !on) }
                span { "{label_of(lang, id)}" }
            }
        }
    };
    let flip = |key: &'static str, label_key: &'static str, default_on: bool| {
        let v = st.value(key);
        let on = if default_on { v != "0" } else { v == "1" };
        rsx! {
            label { class: "check", key: "{key}",
                input { r#type: "checkbox", checked: on,
                    onchange: move |_| st.set_value(key, if on { if default_on { "0" } else { "" } } else if default_on { "" } else { "1" }) }
                span { "{t(lang, label_key)}" }
            }
        }
    };
    let title = match pane.as_str() {
        "tags" => label_of(lang, "tagsView.options"),
        "order" => label_of(lang, "orderView.options"),
        "content" => label_of(lang, "contentsView.options"),
        "accessibility-report" => label_of(lang, "accReportView.options"),
        _ => label_of(lang, "chekerView.options"),
    };
    let tagged = st.read_active(|t| t.doc.is_tagged()).unwrap_or(false);
    let off = crate::commands::accessibility::categories_off(st);
    let mode = crate::commands::accessibility::order_mode(st);
    rsx! {
        Frame { title: format!("{} — {}", title, t(lang, &format!("pane.{pane}"))),
            div { class: "radio-col",
                match pane.as_str() {
                    "tags" => rsx! {
                        {check("tagsview.highlightContent")}
                        label { class: "check",
                            input { r#type: "checkbox", checked: tagged, disabled: !st.has_doc(), onchange: move |_| crate::commands::dispatch(st, "tagsview.isTagged") }
                            span { "{label_of(lang, \"tagsview.isTagged\")}" }
                        }
                    },
                    "order" => rsx! {
                        for m in ["order", "type", "none"] {
                            label { class: "check", key: "{m}",
                                input { r#type: "radio", name: "order-mode", checked: mode == m,
                                    onchange: move |_| st.set_value("order.mode", if m == "order" { "" } else { m }) }
                                span { "{mode_label(lang, m)}" }
                            }
                        }
                        {flip("order.cells", "order.cells", false)}
                        {flip("order.tf", "order.tables_figures", false)}
                    },
                    "content" => rsx! { {check("contentsView.displaySelected")} },
                    "accessibility-report" => rsx! { {flip("acc.report.passed", "acc.report.include_passed", true)} },
                    _ => rsx! {
                        p { class: "hint", "{t(lang, \"acc.options.hint\")}" }
                        for cat in CATEGORIES.iter() {
                            {
                                let on = !off.iter().any(|o| o == cat);
                                let all = off.clone();
                                rsx! {
                                    label { class: "check", key: "{cat}",
                                        input { r#type: "checkbox", checked: on,
                                            onchange: move |_| {
                                                let mut list: Vec<String> = all.iter().filter(|o| o.as_str() != *cat).cloned().collect();
                                                if on { list.push(cat.to_string()); }
                                                st.set_value("acc.off", list.join(","));
                                            } }
                                        span { "{cat_label(lang, cat)}" }
                                    }
                                }
                            }
                        }
                    },
                }
            }
        }
    }
}

#[component]
pub fn RuleExplainDialog(id: String) -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let fixable = rule(&id).map(|r| r.fix != Fix::None).unwrap_or(false);
    let rid = id.clone();
    let items = st
        .read_active(|t| {
            t.acc
                .check
                .as_ref()
                .and_then(|c| c.results.iter().find(|r| r.rule == id).map(|r| r.items.len()))
        })
        .flatten()
        .unwrap_or(0);
    let explain = t(lang, &format!("acc.explain.{id}"));
    let on_ok = fixable.then(|| {
        EventHandler::new(move |_| {
            st.set_dialog(Dialog::None);
            fix_rule(st, &rid);
        })
    });
    rsx! {
        Frame { title: rule_name(lang, &id), on_ok: on_ok, ok_label: Some(label_of(lang, "chekerView.fix")),
            p { "{explain}" }
            if items > 0 { p { class: "muted", "{t(lang, \"acc.items\")}: {items}" } }
        }
    }
}

const LANGUAGES: &[&str] = &[
    "en-US", "en-GB", "it-IT", "fr-FR", "de-DE", "es-ES", "pt-PT", "pt-BR", "nl-NL", "pl-PL", "sv-SE",
    "da-DK", "fi-FI", "nb-NO", "cs-CZ", "el-GR", "ru-RU", "uk-UA", "tr-TR", "ja-JP", "zh-CN", "zh-TW",
    "ko-KR", "ar", "he",
];

#[component]
pub fn DocLanguageDialog() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let current = st.read_active(|t| t.doc.language()).unwrap_or_default();
    let default = if current.is_empty() {
        match lang.code() {
            "it" => "it-IT".to_string(),
            _ => "en-US".to_string(),
        }
    } else {
        current
    };
    let mut value = use_signal(|| default);
    rsx! {
        Frame { title: t(lang, "acc.language.title"),
            on_ok: move |_| {
                let v = value();
                let r = st.with_active(|t| t.doc.set_language(&v));
                match r {
                    Some(Ok(())) => {
                        done(st);
                        if st.read_active(|t| t.acc.check.is_some()).unwrap_or(false) { run_check(st); }
                    }
                    Some(Err(e)) => st.error(e),
                    None => {}
                }
            },
            datalist { id: "lang-codes", for l in LANGUAGES.iter() { option { key: "{l}", value: "{l}" } } }
            div { class: "field", label { "{t(lang, \"acc.language.label\")}" }
                input { list: "lang-codes", value: "{value}", autofocus: true, oninput: move |e| value.set(e.value()) } }
            p { class: "hint", "{t(lang, \"acc.language.hint\")}" }
        }
    }
}
