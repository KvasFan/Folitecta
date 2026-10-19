//! The e-mail dialog: the files of the message (each version of a document,
//! its form data and its comments), the options of the one selected and what
//! its prepared file holds (size, facts, first pages), the archive, subject
//! and message; Save Draft… and Open in Mail Program. Nothing is sent.

use dioxus::prelude::*;
use folitecta_core::{CommentFormat, CopyComments, CopySignatures, FormDataFormat};

use crate::commands::mail::{self as m, DataKind, ItemKind, MAIL, MailItem, Pages, Phase, Prep};
use crate::i18n::{Lang, t, tf};
use crate::icons::Icon;
use crate::state::AppState;
use crate::ui::dialogs::Frame;
use crate::util::human_size;

const FORM_FORMATS: [(FormDataFormat, &str); 4] = [
    (FormDataFormat::Xfdf, "dialog.mail.format_xfdf"),
    (FormDataFormat::Fdf, "dialog.mail.format_fdf"),
    (FormDataFormat::Csv, "dialog.mail.format_csv"),
    (FormDataFormat::Text, "dialog.mail.format_text"),
];

const COMMENT_FORMATS: [(CommentFormat, &str); 2] = [
    (CommentFormat::Xfdf, "dialog.mail.format_xfdf"),
    (CommentFormat::Fdf, "dialog.mail.format_fdf"),
];

const COMMENT_CHOICES: [(CopyComments, &str, &str); 3] = [
    (CopyComments::Keep, "keep", "dialog.mail.comments_keep"),
    (CopyComments::Flatten, "flatten", "dialog.mail.comments_flatten"),
    (CopyComments::Remove, "remove", "dialog.mail.comments_remove"),
];

/// The second line of a row: which version, or which data.
fn kind_text(lang: Lang, it: &MailItem) -> String {
    let format = |k: DataKind| match k {
        DataKind::Form(f) => f.extension().to_uppercase(),
        DataKind::Comments(CommentFormat::Fdf) => "FDF".into(),
        DataKind::Comments(_) => "XFDF".into(),
    };
    match &it.kind {
        ItemKind::Saved(_) => tf(
            lang,
            "dialog.mail.kind_saved",
            &[("when", &folitecta_core::util::unix_display(it.doc.saved_at))],
        ),
        ItemKind::Current if it.doc.saved.is_some() => t(lang, "dialog.mail.kind_changes"),
        ItemKind::Current => t(lang, "dialog.mail.kind_new"),
        ItemKind::Data(k @ DataKind::Form(_)) => {
            tf(lang, "dialog.mail.kind_data", &[("format", &format(*k))])
        }
        ItemKind::Data(k) => tf(lang, "dialog.mail.kind_comments", &[("format", &format(*k))]),
    }
}

/// What was left out with the other pages, in words.
fn left_out_text(lang: Lang, l: &folitecta_core::LeftOut) -> String {
    let mut parts = Vec::new();
    for (n, key) in [
        (l.bookmarks, "dialog.mail.left_bookmarks"),
        (l.links, "dialog.mail.left_links"),
        (l.destinations, "dialog.mail.left_dests"),
        (l.fields, "dialog.mail.left_fields"),
        (l.tags, "dialog.mail.left_tags"),
        (l.resources, "dialog.mail.left_resources"),
    ] {
        if n > 0 {
            parts.push(tf(lang, key, &[("count", &n.to_string())]));
        }
    }
    if l.open_action {
        parts.push(t(lang, "dialog.mail.left_open_view"));
    }
    if l.threads {
        parts.push(t(lang, "dialog.mail.left_threads"));
    }
    parts.join(", ")
}

#[component]
pub fn MailDialog() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let Some(d) = MAIL() else {
        return rsx! {
            Frame { title: t(lang, "dialog.mail.title"),
                p { class: "hint", "{t(lang, \"dialog.mail.nothing\")}" }
            }
        };
    };
    let ready = m::attachments(&d);
    let busy = matches!(d.phase, Phase::Writing | Phase::Handing);
    let names = m::final_names(&d.items);
    let sel = d.selected.min(d.items.len().saturating_sub(1));
    let included = d.items.iter().filter(|i| i.include).count();
    let failed = d
        .items
        .iter()
        .filter(|i| i.include && matches!(i.prep, Prep::Failed(_)))
        .count();
    let status = match &ready {
        _ if included == 0 => t(lang, "dialog.mail.nothing"),
        Some(files) if d.zip => tf(
            lang,
            "dialog.mail.summary_zip",
            &[
                ("count", &included.to_string()),
                (
                    "size",
                    &human_size(files.iter().map(|f| f.bytes.len() as u64).sum()),
                ),
            ],
        ),
        Some(files) => tf(
            lang,
            "dialog.mail.summary",
            &[
                ("count", &files.len().to_string()),
                (
                    "size",
                    &human_size(files.iter().map(|f| f.bytes.len() as u64).sum()),
                ),
            ],
        ),
        None if failed > 0 => tf(
            lang,
            "dialog.mail.status_failed",
            &[("count", &failed.to_string())],
        ),
        None => t(lang, "dialog.mail.preparing"),
    };
    let can_go = ready.is_some() && !busy;
    rsx! {
        Frame { title: t(lang, "dialog.mail.title"), wide: true,
            ok_label: Some(t(lang, "dialog.mail.open_program")), ok_disabled: !can_go, enter_from_text: true,
            on_ok: move |_| m::hand_over(st),
            div { class: "mail-dialog",
                div { class: "mail-list", role: "list", "aria-label": "{t(lang, \"dialog.mail.files\")}",
                    for (i, it) in d.items.iter().enumerate() {
                        {
                            let shown = names[i].clone().unwrap_or_else(|| it.name.clone());
                            let icon = match it.kind { ItemKind::Data(DataKind::Form(_)) => "form", ItemKind::Data(_) => "comment", _ => "page" };
                            let include_label = tf(lang, "dialog.mail.include", &[("name", &shown)]);
                            let sub = kind_text(lang, it);
                            let selected = i == sel;
                            rsx! {
                                div { class: if selected { "mail-row selected" } else { "mail-row" }, role: "listitem", key: "{d.id}-{i}",
                                    input { r#type: "checkbox", class: "mail-include", checked: it.include, "aria-label": "{include_label}",
                                        onchange: move |_| m::edit_row(st, i, |it| it.include = !it.include) }
                                    button { class: "mail-pick", "aria-current": if selected { "true" } else { "false" },
                                        onclick: move |_| m::edit_text(|d| d.selected = i),
                                        Icon { name: icon }
                                        span { class: "mail-name-col",
                                            span { class: "mail-name", "{shown}" }
                                            span { class: "mail-sub", "{sub}" }
                                        }
                                    }
                                    match &it.prep {
                                        Prep::Ready(p) => rsx! { span { class: "mail-size", "{human_size(p.bytes.len() as u64)}" } },
                                        Prep::Failed(e) => rsx! { span { class: "mail-size warn", title: "{e.text(lang)}", Icon { name: "warning" } } },
                                        _ if it.include => rsx! { span { class: "mail-size", "…" } },
                                        _ => rsx! { span { class: "mail-size" } },
                                    }
                                }
                            }
                        }
                    }
                }
                if let Some(it) = d.items.get(sel).cloned() {
                    Detail { item: it, index: sel }
                }
            }
            div { class: "mail-common",
                label { class: "check",
                    input { r#type: "checkbox", class: "mail-zip", checked: d.zip,
                        onchange: move |_| m::edit_names(st, |d| d.zip = !d.zip) }
                    span { "{t(lang, \"dialog.mail.zip\")}" }
                }
                if d.zip {
                    div { class: "field",
                        label { r#for: "mail-zip-name", "{t(lang, \"dialog.mail.zip_name\")}" }
                        input { id: "mail-zip-name", class: "mail-zip-name", value: "{d.zip_name}",
                            oninput: move |e| { let v = e.value(); m::edit_names(st, move |d| d.zip_name = v) } }
                    }
                }
                div { class: "field",
                    label { r#for: "mail-subject", "{t(lang, \"dialog.mail.subject\")}" }
                    input { id: "mail-subject", class: "mail-subject", value: "{d.subject}", autofocus: true,
                        oninput: move |e| { let v = e.value(); m::edit_text(move |d| { d.subject = v; d.subject_auto = false; }) } }
                }
                div { class: "field",
                    label { r#for: "mail-body", "{t(lang, \"dialog.mail.body\")}" }
                    textarea { id: "mail-body", class: "mail-body", rows: "3", value: "{d.body}",
                        onkeydown: move |e| if e.key() == Key::Enter { e.stop_propagation() },
                        oninput: move |e| { let v = e.value(); m::edit_text(move |d| d.body = v) } }
                }
                if cfg!(target_os = "macos") {
                    p { class: "hint", "{t(lang, \"dialog.mail.mac_hint\")}" }
                }
            }
            div { class: "mail-status", role: "status", "aria-live": "polite", "{status}" }
            match &d.phase {
                Phase::Writing => rsx! { p { class: "hint", "{t(lang, \"dialog.mail.writing\")}" } },
                Phase::Handing => rsx! { p { class: "hint", "{t(lang, \"dialog.mail.waiting\")}" } },
                Phase::Opened(n) => rsx! {
                    div { class: "callout mail-opened", role: "status",
                        "{tf(lang, \"dialog.mail.opened\", &[(\"count\", &n.to_string())])}"
                    }
                    div { class: "row mail-after",
                        button { class: "tbtn mail-save-again", disabled: !can_go, onclick: move |_| m::save_draft(st), "{t(lang, \"dialog.mail.save_draft\")}" }
                        button { class: "tbtn mail-show-files", onclick: move |_| m::show_files(st), "{t(lang, \"dialog.mail.show_files\")}" }
                        button { class: "tbtn primary mail-done", onclick: move |_| m::done(st), "{t(lang, \"dialog.mail.done\")}" }
                    }
                },
                Phase::Failed(e) => rsx! {
                    div { class: "callout warn mail-failed", role: "alert",
                        "{tf(lang, \"dialog.mail.failed\", &[(\"reason\", &m::reason_text(lang, e))])}"
                    }
                    div { class: "row mail-after",
                        button { class: "tbtn mail-save-again", disabled: !can_go, onclick: move |_| m::save_draft(st), "{t(lang, \"dialog.mail.save_draft\")}" }
                        button { class: "tbtn mail-open-draft", disabled: !can_go, onclick: move |_| m::open_draft(st), "{t(lang, \"dialog.mail.open_draft\")}" }
                        button { class: "tbtn mail-show-files", onclick: move |_| m::show_files(st), "{t(lang, \"dialog.mail.show_files\")}" }
                    }
                },
                Phase::Editing => rsx! {},
            }
            div { class: "row mail-actions",
                button { class: "tbtn mail-save", disabled: !can_go, onclick: move |_| m::save_draft(st), "{t(lang, \"dialog.mail.save_draft\")}" }
                span { class: "hint", "{t(lang, \"dialog.mail.save_hint\")}" }
            }
        }
    }
}

/// The options of the selected row and what its prepared file holds.
#[component]
fn Detail(item: MailItem, index: usize) -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let i = index;
    let it = item;
    let plan_problem = it.plan().err();
    let pdf = !matches!(it.kind, ItemKind::Data(_));
    let n = it.doc.pages;
    rsx! {
        section { class: "mail-detail", "aria-label": "{it.name}",
            div { class: "field",
                label { r#for: "mail-name", "{t(lang, \"dialog.mail.name\")}" }
                input { id: "mail-name", class: "mail-name-input", value: "{it.name}",
                    oninput: move |e| { let v = e.value(); m::edit_names(st, move |d| if let Some(x) = d.items.get_mut(i) { x.name = v; }) } }
            }
            if matches!(it.kind, ItemKind::Saved(_)) && it.doc.dirty {
                p { class: "hint", "{t(lang, \"dialog.mail.saved_hint\")}" }
            }
            if it.transforms_allowed() {
                div { class: "field",
                    label { "{t(lang, \"dialog.pages\")}" }
                    div { class: "radio-row",
                        label { input { r#type: "radio", name: "mail-pages", class: "mail-pages-all", checked: it.pages == Pages::All,
                            onchange: move |_| m::edit_row(st, i, |x| x.pages = Pages::All) }
                            "{t(lang, \"dialog.pages.all\")} ({n})" }
                        label { input { r#type: "radio", name: "mail-pages", class: "mail-pages-current", checked: it.pages == Pages::Current,
                            onchange: move |_| m::edit_row(st, i, |x| x.pages = Pages::Current) }
                            "{t(lang, \"dialog.pages.current\")} ({it.doc.current_page + 1})" }
                        label { input { r#type: "radio", name: "mail-pages", class: "mail-pages-range", checked: it.pages == Pages::Range,
                            onchange: move |_| m::edit_row(st, i, |x| x.pages = Pages::Range) }
                            "{t(lang, \"dialog.mail.pages_range\")}" }
                    }
                    if it.pages == Pages::Range {
                        input { class: "mail-range", value: "{it.range}", placeholder: "1-3, 7", "aria-label": "{t(lang, \"dialog.mail.pages_range\")}",
                            onkeydown: move |e| if e.key() == Key::Enter { e.stop_propagation() },
                            oninput: move |e| { let v = e.value(); m::edit_row(st, i, move |x| x.range = v) } }
                        p { class: "hint", "{t(lang, \"dialog.pages.hint\")}" }
                    }
                }
                div { class: "field",
                    label { r#for: "mail-comments", "{t(lang, \"dialog.mail.comments\")}" }
                    select { id: "mail-comments", class: "mail-comments", disabled: it.doc.comments == 0,
                        onchange: move |e| {
                            let v = e.value();
                            let c = COMMENT_CHOICES.iter().find(|(_, id, _)| *id == v).map(|(c, _, _)| *c).unwrap_or_default();
                            m::edit_row(st, i, move |x| x.comments = c)
                        },
                        for (c, id, key) in COMMENT_CHOICES {
                            option { value: "{id}", selected: it.comments == c, "{t(lang, key)}" }
                        }
                    }
                    if it.doc.comments == 0 {
                        p { class: "hint", "{t(lang, \"dialog.mail.no_comments\")}" }
                    }
                }
            }
            if let ItemKind::Data(kind) = it.kind {
                div { class: "field",
                    label { r#for: "mail-format", "{t(lang, \"dialog.export.format\")}" }
                    select { id: "mail-format", class: "mail-format",
                        onchange: move |e| {
                            let v = e.value();
                            let next = match kind {
                                DataKind::Form(_) => FORM_FORMATS.iter().find(|(f, _)| f.extension() == v).map(|(f, _)| DataKind::Form(*f)),
                                DataKind::Comments(_) => COMMENT_FORMATS.iter().find(|(f, _)| f.extension() == v).map(|(f, _)| DataKind::Comments(*f)),
                            };
                            if let Some(next) = next {
                                m::edit_row(st, i, move |x| {
                                    let old = x.extension();
                                    x.kind = ItemKind::Data(next);
                                    let new = x.extension();
                                    if let Some(stem) = x.name.strip_suffix(&format!(".{old}")) {
                                        x.name = format!("{stem}.{new}");
                                    }
                                });
                            }
                        },
                        match kind {
                            DataKind::Form(cur) => rsx! {
                                for (f, key) in FORM_FORMATS {
                                    option { value: "{f.extension()}", selected: cur == f, "{t(lang, key)}" }
                                }
                            },
                            DataKind::Comments(cur) => rsx! {
                                for (f, key) in COMMENT_FORMATS {
                                    option { value: "{f.extension()}", selected: cur == f, "{t(lang, key)}" }
                                }
                            },
                        }
                    }
                    p { class: "hint",
                        if matches!(kind, DataKind::Form(_)) { "{t(lang, \"dialog.mail.data_hint\")}" } else { "{t(lang, \"dialog.mail.comments_hint\")}" }
                    }
                }
            }
            if let Some(problem) = &plan_problem {
                if matches!(it.prep, Prep::Pending) || !it.include {
                    div { class: "callout warn mail-problem", role: "alert", "{problem.text(lang)}" }
                }
            }
            match &it.prep {
                Prep::Failed(problem) => rsx! {
                    div { class: "callout warn mail-problem", role: "alert",
                        "{tf(lang, \"dialog.mail.failed_item\", &[(\"reason\", &problem.text(lang))])}"
                    }
                },
                Prep::Ready(p) => rsx! {
                    if let Some(f) = &p.facts {
                        div { class: "row mail-facts",
                            span { class: "chip", "{tf(lang, \"dialog.mail.facts_pages\", &[(\"count\", &f.pages.to_string())])}" }
                            span { class: "chip", "{tf(lang, \"dialog.mail.facts_comments\", &[(\"count\", &f.comments.to_string())])}" }
                            if f.fields > 0 {
                                span { class: "chip", "{tf(lang, \"dialog.mail.facts_fields\", &[(\"count\", &f.fields.to_string())])}" }
                            }
                            if f.attachments > 0 {
                                span { class: "chip", "{tf(lang, \"dialog.mail.facts_attachments\", &[(\"count\", &f.attachments.to_string())])}" }
                            }
                            if f.protected {
                                span { class: "chip mail-ok", Icon { name: "lock" } "{t(lang, \"dialog.mail.protected\")}" }
                            }
                            match f.signatures {
                                CopySignatures::Kept { count, intact } => {
                                    let text = match intact {
                                        Some(k) if k < count => tf(lang, "dialog.mail.signed_broken", &[("count", &(count - k).to_string()), ("total", &count.to_string())]),
                                        _ => tf(lang, "dialog.mail.signed_kept", &[("count", &count.to_string())]),
                                    };
                                    let class = if intact.is_some_and(|k| k < count) { "chip mail-warn" } else { "chip mail-ok" };
                                    rsx! { span { class: "{class}", "{text}" } }
                                }
                                CopySignatures::Removed(count) => rsx! {
                                    span { class: "chip mail-warn", "{tf(lang, \"dialog.mail.signed_removed\", &[(\"count\", &count.to_string())])}" }
                                },
                                CopySignatures::None => rsx! {},
                            }
                        }
                        if matches!(f.signatures, CopySignatures::Removed(_)) {
                            div { class: "callout warn", "{t(lang, \"dialog.mail.warn_signatures\")}" }
                        }
                        if matches!(f.signatures, CopySignatures::Kept { .. }) && it.kind == ItemKind::Current && it.doc.dirty {
                            div { class: "callout", "{t(lang, \"dialog.mail.signed_update\")}" }
                        }
                        if !f.left_out.is_empty() {
                            div { class: "callout mail-left-out", "{tf(lang, \"dialog.mail.left_out\", &[(\"list\", &left_out_text(lang, &f.left_out))])}" }
                        }
                    }
                    if p.facts.is_none() && pdf {
                        p { class: "hint", "{t(lang, \"dialog.mail.protected_locked\")}" }
                    }
                    if !p.thumbs.is_empty() {
                        div { class: "mail-thumbs",
                            for (k, src) in p.thumbs.iter().enumerate() {
                                img { key: "{k}", src: "{src}", alt: "{tf(lang, \"dialog.mail.page_alt\", &[(\"page\", &(k + 1).to_string())])}" }
                            }
                            if p.more_pages > 0 {
                                span { class: "muted", "{tf(lang, \"dialog.mail.more_pages\", &[(\"count\", &p.more_pages.to_string())])}" }
                            }
                        }
                    }
                    if let Some(text) = &p.text {
                        textarea { class: "mail-data-preview", readonly: true, rows: "6", "aria-label": "{it.name}", value: "{text}" }
                    }
                },
                Prep::Running(_) => rsx! { p { class: "hint", "{t(lang, \"dialog.mail.preparing\")}" } },
                Prep::Pending if it.include && plan_problem.is_none() => rsx! { p { class: "hint", "{t(lang, \"dialog.mail.preparing\")}" } },
                Prep::Pending => rsx! {},
            }
        }
    }
}
