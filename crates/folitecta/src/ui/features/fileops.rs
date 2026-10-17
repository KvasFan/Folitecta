//! Dialogs of the document file and its metadata: rename, metadata
//! templates with the XMP properties, and saving a template.

use dioxus::prelude::*;
use folitecta_core::registry;

use crate::commands::fileops::{
    apply_template, export_xmp, rename_to, save_template, save_templates, templates,
};
use crate::commands::option_on;
use crate::i18n::{Lang, cmd_label, t, tf};
use crate::state::{AppState, Dialog, ToastKind};
use crate::ui::dialogs::Frame;

fn label_of(lang: Lang, id: &str) -> String {
    registry()
        .get(id)
        .map(|c| cmd_label(lang, c))
        .unwrap_or_else(|| id.to_string())
}

#[component]
pub fn RenameDocDialog(name: String) -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let mut value = use_signal(|| name.clone());
    rsx! {
        Frame { title: label_of(lang, "renameDoc"),
            on_ok: move |_| {
                match rename_to(st, &value()) {
                    Ok(path) => {
                        st.set_dialog(Dialog::None);
                        let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                        st.toast(ToastKind::Ok, tf(lang, "toast.renamed", &[("name", &name)]), "");
                    }
                    Err(e) if e.is_empty() => st.set_dialog(Dialog::None),
                    Err(e) => st.toast(ToastKind::Warn, e, ""),
                }
            },
            div { class: "field", label { "{t(lang, \"rename.label\")}" }
                input { value: "{value}", autofocus: true, oninput: move |e| value.set(e.value()) } }
            p { class: "hint", "{t(lang, \"rename.hint\")}" }
        }
    }
}

#[component]
pub fn MetaTemplatesDialog() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let list = templates(st);
    let props = st.read_active(|t| t.doc.xmp_properties()).unwrap_or_default();
    let has_packet = st.read_active(|t| t.doc.xmp_packet().is_some()).unwrap_or(false);
    let show_name = option_on(st, "msp.hdr.tagName");
    let show_value = option_on(st, "msp.hdr.value");
    let field = |l: &str, v: &str| {
        if v.is_empty() {
            String::new()
        } else {
            format!("{l}: {v}")
        }
    };
    rsx! {
        Frame { title: t(lang, "meta.title"), wide: true,
            h4 { class: "dialog-sub", "{t(lang, \"meta.templates\")}" }
            if list.is_empty() { p { class: "muted", "{t(lang, \"meta.no_templates\")}" } }
            div { class: "meta-templates",
                for tpl in list.iter().cloned() {
                    {
                        let summary = [
                            field(&t(lang, "dialog.props.title_field"), &tpl.title),
                            field(&t(lang, "dialog.props.author"), &tpl.author),
                            field(&t(lang, "dialog.props.subject"), &tpl.subject),
                            field(&t(lang, "dialog.props.keywords"), &tpl.keywords),
                        ]
                        .into_iter()
                        .filter(|s| !s.is_empty())
                        .collect::<Vec<_>>()
                        .join(" · ");
                        let apply = tpl.clone();
                        let name = tpl.name.clone();
                        rsx! {
                            div { class: "meta-row", key: "{tpl.name}",
                                div { class: "meta-text",
                                    span { class: "meta-name", "{tpl.name}" }
                                    span { class: "muted", title: "{summary}", "{summary}" }
                                }
                                button { class: "tbtn", onclick: move |_| { apply_template(st, &apply); st.toast(ToastKind::Ok, t(lang, "toast.template_applied"), ""); }, "{t(lang, \"meta.apply\")}" }
                                button { class: "ibtn", title: "{t(lang, \"dialog.remove\")}",
                                    onclick: move |_| { let mut l = templates(st); l.retain(|x| x.name != name); save_templates(st, &l); st.set_dialog(Dialog::None); st.set_dialog(Dialog::MetaTemplates); }, "×" }
                            }
                        }
                    }
                }
            }
            div { class: "row", style: "gap:6px;flex-wrap:wrap",
                button { class: "tbtn", onclick: move |_| st.set_dialog(Dialog::MetaTemplateSave), "{t(lang, \"meta.save_template\")}" }
                button { class: "tbtn", onclick: move |_| export_xmp(st), "{t(lang, \"meta.save_xmp\")}" }
            }
            h4 { class: "dialog-sub", "{t(lang, \"meta.xmp\")}" }
            if !has_packet { p { class: "muted", "{t(lang, \"meta.no_xmp\")}" } }
            if has_packet && (show_name || show_value) {
                div { class: "meta-props",
                    div { class: "meta-prop head",
                        if show_name { span { "{label_of(lang, \"msp.hdr.tagName\")}" } }
                        if show_value { span { "{label_of(lang, \"msp.hdr.value\")}" } }
                    }
                    for (i, (k, v)) in props.into_iter().enumerate() {
                        div { class: "meta-prop", key: "{i}",
                            if show_name { span { class: "mono", title: "{k}", "{k}" } }
                            if show_value { span { title: "{v}", "{v}" } }
                        }
                    }
                }
            }
            div { class: "row", style: "gap:12px",
                for id in ["msp.hdr.tagName", "msp.hdr.value"] {
                    {
                        let on = option_on(st, id);
                        rsx! {
                            label { class: "check", key: "{id}",
                                input { r#type: "checkbox", checked: on, onchange: move |_| st.set_option(id, !on) }
                                span { "{label_of(lang, id)}" }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
pub fn MetaTemplateSaveDialog() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let default = st
        .read_active(|t| {
            let i = t.doc.info();
            if i.author.is_empty() { i.title } else { i.author }
        })
        .unwrap_or_default();
    let mut name = use_signal(|| default);
    rsx! {
        Frame { title: t(lang, "meta.save_template").trim_end_matches('…').to_string(),
            on_ok: move |_| {
                if save_template(st, &name()) {
                    st.set_dialog(Dialog::MetaTemplates);
                    st.toast(ToastKind::Ok, t(lang, "toast.template_saved"), "");
                } else {
                    st.toast(ToastKind::Info, t(lang, "toast.template_name"), "");
                }
            },
            div { class: "field", label { "{t(lang, \"meta.template_name\")}" }
                input { value: "{name}", autofocus: true, oninput: move |e| name.set(e.value()) } }
            p { class: "hint", "{t(lang, \"meta.template_hint\")}" }
        }
    }
}
