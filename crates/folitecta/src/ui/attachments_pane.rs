//! Attachments pane and its dialogs.

use dioxus::html::ModifiersInteraction;
use dioxus::prelude::*;
use folitecta_core::attachments::RELATIONSHIPS;
use folitecta_core::{Attachment, registry};

use crate::commands::attachments::{COLUMNS, open_attachment};
use crate::commands::{dispatch, option_on};
use crate::i18n::{Lang, cmd_label, t, ui_label};
use crate::icons::Icon;
use crate::state::{AppState, Dialog};
use crate::ui::dialogs::Frame;

/// Value of a column for an attachment (empty when unknown).
fn column_value(lang: Lang, col: &str, a: &Attachment) -> String {
    match col {
        "attachmentsView.hdr.description" => a.description.clone(),
        "attachmentsView.hdr.modified" => {
            if a.modified.is_empty() {
                a.created.clone()
            } else {
                a.modified.clone()
            }
        }
        "attachmentsView.hdr.size" => a.size.map(crate::util::human_size).unwrap_or_default(),
        "attachmentsView.hdr.compressed" => a.compressed.map(crate::util::human_size).unwrap_or_default(),
        "attachmentsView.hdr.location" => match a.page {
            Some(p) => format!("{} {}", t(lang, "status.page"), p + 1),
            None => t(lang, "attach.document"),
        },
        "attachmentsView.hdr.type" => {
            if a.mime.is_empty() {
                std::path::Path::new(&a.name)
                    .extension()
                    .map(|e| e.to_string_lossy().to_uppercase())
                    .unwrap_or_default()
            } else {
                a.mime.clone()
            }
        }
        "attachmentsView.hdr.relationship" => {
            if a.relationship.is_empty() {
                String::new()
            } else {
                ui_label(lang, "relationship", &a.relationship, &a.relationship)
            }
        }
        _ => String::new(),
    }
}

#[component]
pub fn AttachmentsPane() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let Some((atts, selected)) = st.read_active(|t| (t.doc.attachments(), t.selected_attachment)) else {
        return rsx! {};
    };
    let selected = selected.filter(|i| *i < atts.len());
    let has_sel = selected.is_some();
    let can_delete = selected
        .and_then(|i| atts.get(i))
        .is_some_and(|a| a.key.is_some());
    let show_name = option_on(st, "attachmentsView.hdr.name");
    let cols: Vec<(&str, String)> = COLUMNS
        .iter()
        .skip(1)
        .filter(|(id, _)| option_on(st, id))
        .map(|(id, key)| (*id, t(lang, key)))
        .collect();
    let reg = registry();
    let label = |id: &str| {
        reg.get(id)
            .map(|c| cmd_label(lang, c))
            .unwrap_or_else(|| id.to_string())
    };
    let tools: [(&str, &str, bool); 5] = [
        ("attachmentsView.new", "insert", true),
        ("attachmentsView.open", "open", has_sel),
        ("attachmentsView.save", "save", has_sel),
        ("attachmentsView.edit", "pencil", has_sel),
        ("attachmentsView.options", "settings", true),
    ];
    rsx! {
        div { class: "pane-toolbar icon-toolbar",
            for (id, icon, enabled) in tools {
                button { class: "ibtn", key: "{id}", title: "{label(id)}", disabled: !enabled, onclick: move |_| dispatch(st, id), Icon { name: icon } }
            }
            button { class: "ibtn danger", title: "{t(lang, \"label.delete\")}", disabled: !can_delete,
                onclick: move |_| {
                    let r = st.with_active(|t| {
                        let a = t.selected_attachment.and_then(|i| t.doc.attachments().get(i).cloned())?;
                        t.selected_attachment = None;
                        Some(t.doc.remove_attachment(&a))
                    }).flatten();
                    match r {
                        Some(Err(e)) => st.error(e),
                        _ => crate::files::set_window_title(st),
                    }
                },
                Icon { name: "delete" }
            }
        }
        if atts.is_empty() {
            div { class: "pane-empty",
                "{t(lang, \"attach.none\")}"
                div { class: "row", style: "justify-content:center;margin-top:8px",
                    button { class: "tbtn", onclick: move |_| dispatch(st, "attachmentsView.new"), "{label(\"attachmentsView.new\")}" }
                }
            }
        }
        for (i, att) in atts.iter().enumerate() {
            {
                let is_sel = selected == Some(i);
                let a_open = att.clone();
                let values: Vec<(String, String)> = cols
                    .iter()
                    .map(|(id, name)| (name.clone(), column_value(lang, id, att)))
                    .filter(|(_, v)| !v.is_empty())
                    .collect();
                let class = if is_sel { "list-item attachment selected" } else { "list-item attachment" };
                let title = if show_name { att.name.clone() } else { String::new() };
                rsx! {
                    div { class: "{class}", key: "{i}-{att.stream.0}", title: "{att.name}",
                        onclick: move |e| {
                            let toggle = e.modifiers().ctrl() && is_sel;
                            st.with_active(|t| t.selected_attachment = if toggle { None } else { Some(i) });
                        },
                        ondoubleclick: move |_| open_attachment(st, a_open.clone()),
                        if show_name {
                            div { class: "title", Icon { name: "attachment" } span { class: "ellipsis", "{title}" } }
                        }
                        if !values.is_empty() {
                            div { class: "attach-cols",
                                for (name, value) in values {
                                    span { key: "{name}", class: "k", "{name}" }
                                    span { class: "v", title: "{value}", "{value}" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Name, description, relationship and contents of an attachment.
#[component]
pub fn AttachmentEditDialog(index: usize) -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let Some(att) = st
        .read_active(|t| t.doc.attachments().get(index).cloned())
        .flatten()
    else {
        return rsx! {};
    };
    let mut name = use_signal(|| att.name.clone());
    let mut desc = use_signal(|| att.description.clone());
    let mut rel = use_signal(|| {
        if att.relationship.is_empty() {
            "Unspecified".to_string()
        } else {
            att.relationship.clone()
        }
    });
    let a_ok = att.clone();
    let a_replace = att.clone();
    let replace = move |_| {
        let a = a_replace.clone();
        spawn(async move {
            let Some(f) = rfd::AsyncFileDialog::new().pick_file().await else {
                return;
            };
            let data = f.read().await;
            match st.with_active(|t| t.doc.replace_attachment_data(&a, data)) {
                Some(Err(e)) => st.error(e),
                _ => {
                    crate::files::set_window_title(st);
                    st.toast(
                        crate::state::ToastKind::Ok,
                        t(lang, "toast.attachment_replaced"),
                        "",
                    );
                }
            }
        });
    };
    let size = att.size.map(crate::util::human_size).unwrap_or_default();
    rsx! {
        Frame { title: t(lang, "attach.edit.title"),
            on_ok: move |_| {
                let r = st.with_active(|t| t.doc.set_attachment_info(&a_ok, &name(), &desc(), &rel()));
                match r {
                    Some(Err(e)) => st.error(e),
                    _ => {
                        crate::files::set_window_title(st);
                        st.set_dialog(Dialog::None);
                    }
                }
            },
            div { class: "field",
                label { "{t(lang, \"attach.col.name\")}" }
                input { value: "{name}", autofocus: true, oninput: move |e| name.set(e.value()) }
            }
            div { class: "field",
                label { "{t(lang, \"attach.col.description\")}" }
                textarea { value: "{desc}", oninput: move |e| desc.set(e.value()) }
            }
            div { class: "field",
                label { "{t(lang, \"attach.col.relationship\")}" }
                select { onchange: move |e| rel.set(e.value()),
                    for r in RELATIONSHIPS.iter() {
                        option { key: "{r}", value: "{r}", selected: *r == rel(), "{ui_label(lang, \"relationship\", r, r)}" }
                    }
                }
            }
            div { class: "row", style: "gap:8px;align-items:center",
                button { class: "tbtn", onclick: replace, Icon { name: "import" } "{t(lang, \"attach.replace\")}" }
                span { class: "hint", "{size}" }
            }
        }
    }
}

/// Columns of the attachments pane and automatic showing.
#[component]
pub fn AttachmentOptionsDialog() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let reg = registry();
    let label = |id: &str| {
        reg.get(id)
            .map(|c| cmd_label(lang, c))
            .unwrap_or_else(|| id.to_string())
    };
    let auto = option_on(st, "attachmentsView.autoShowPane");
    rsx! {
        Frame { title: t(lang, "attach.options.title"),
            div { class: "field",
                label { "{label(\"attachmentsView.hdr\")}" }
                div { class: "radio-col",
                    for (id, key) in COLUMNS.iter() {
                        {
                            let on = option_on(st, id);
                            let id = *id;
                            rsx! {
                                label { key: "{id}", class: "check",
                                    input { r#type: "checkbox", checked: on, onchange: move |_| dispatch(st, id) }
                                    span { "{t(lang, key)}" }
                                }
                            }
                        }
                    }
                }
            }
            label { class: "check",
                input { r#type: "checkbox", checked: auto, onchange: move |_| dispatch(st, "attachmentsView.autoShowPane") }
                span { "{label(\"attachmentsView.autoShowPane\")}" }
            }
        }
    }
}
