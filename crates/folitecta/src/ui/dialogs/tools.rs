//! Dialogs of the drawing tools: notes, text boxes, links, callouts, text edits and field values; security and flattening.

use super::*;

/// Return to the hand tool after creating something, unless the user keeps tools selected.
pub(super) fn tool_done(st: AppState) {
    if !option_on(st, "tool.keepSelected") {
        st.set_tool(Tool::Hand);
    }
}

/// The interface icon that stands for a note icon in the lists.
pub(crate) fn note_icon_symbol(name: &str) -> &'static str {
    match name {
        "Note" => "note",
        "Key" => "key",
        "Help" => "help",
        "NewParagraph" => "new-paragraph",
        "Paragraph" => "pilcrow",
        "Insert" => "caret-up",
        "Check" => "check",
        "Circle" => "circle",
        "Cross" => "close",
        "Star" => "star",
        _ => "comment",
    }
}

#[component]
pub(super) fn NoteDialog(page: usize, x: f32, y: f32) -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let text = use_signal(String::new);
    let author = use_signal(|| st.ui.read().author.clone());
    let mut color = use_signal(|| st.ui.read().value("note.color").unwrap_or("#ffd833").to_string());
    let mut icon = use_signal(|| st.ui.read().value("note.icon").unwrap_or("Comment").to_string());
    rsx! {
        Frame { title: t(lang, "dialog.note.title"),
            on_ok: move |_| {
                let (c, a, col, ic) = (text(), author(), color(), icon());
                st.set_value("note.color", col.clone());
                st.set_value("note.icon", ic.clone());
                apply(st, |tab| tab.doc.add_note_with_icon(page, x, y, &c, &a, parse_color(&col), &ic).map(|_| ()));
                tool_done(st);
            },
            Area { label: t(lang, "dialog.note.text"), value: text, autofocus: true }
            div { class: "field-grid",
                Text { label: t(lang, "dialog.note.author"), value: author }
                div { class: "field", label { "{t(lang, \"dialog.watermark.color\")}" } input { r#type: "color", value: "{color}", oninput: move |e| color.set(e.value()) } }
                div { class: "field", label { "{t(lang, \"dialog.note.icon\")}" }
                    select { onchange: move |e| icon.set(e.value()),
                        for name in folitecta_core::NOTE_ICONS.iter() {
                            {
                                let label = t(lang, &format!("annot.icon.{name}"));
                                rsx! { option { key: "{name}", value: "{name}", selected: icon() == *name, "{label}" } }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// The icon of a note (or, with no note, of the new notes), picked from a grid.
#[component]
pub(super) fn NoteIconDialog(target: Option<(usize, usize)>) -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let current = match target {
        Some((page, index)) => st.read_active(|t| t.doc.note_icon_of(page, index)).flatten(),
        None => st.ui.read().value("note.icon").map(str::to_string),
    }
    .unwrap_or_else(|| "Comment".into());
    let pick = move |name: &'static str| match target {
        Some((page, index)) => {
            apply(st, |tab| tab.doc.set_note_icon(page, index, name));
            st.with_active(|tab| tab.selected_annot = Some((page, index)));
        }
        None => {
            st.set_value("note.icon", name);
            st.set_dialog(Dialog::None);
        }
    };
    rsx! {
        Frame { title: t(lang, "annot.icon.title"),
            if target.is_none() {
                p { class: "hint", "{t(lang, \"annot.icon.new_notes\")}" }
            }
            div { class: "choice-grid note-icons",
                for name in folitecta_core::NOTE_ICONS.iter().copied() {
                    {
                        let label = t(lang, &format!("annot.icon.{name}"));
                        rsx! {
                            button { class: if current == name { "choice on" } else { "choice" }, key: "{name}",
                                onclick: move |_| pick(name),
                                Icon { name: note_icon_symbol(name) }
                                span { "{label}" }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
pub(super) fn TextBoxDialog(page: usize, rect: Vec<f32>, border: bool) -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let text = use_signal(String::new);
    let size = use_signal(|| st.ui.read().value("text.size").unwrap_or("12").to_string());
    let mut color = use_signal(|| {
        st.ui
            .read()
            .value("text.color")
            .unwrap_or(if border { "#1d4ed8" } else { "#000000" })
            .to_string()
    });
    let mut with_border = use_signal(|| border);
    let r = [rect[0], rect[1], rect[2], rect[3]];
    let title = if border {
        t(lang, "tool.text")
    } else {
        t(lang, "tool.typewriter")
    };
    rsx! {
        Frame { title,
            on_ok: move |_| {
                let (c, fs, col, b) = (text(), num_or(&size, 12.0).clamp(4.0, 200.0), color(), with_border());
                st.set_value("text.size", fs.to_string());
                st.set_value("text.color", col.clone());
                let author = st.ui.read().author.clone();
                apply(st, |tab| tab.doc.add_free_text(page, r, &c, &author, fs, parse_color(&col), b).map(|_| ()));
                tool_done(st);
            },
            Area { label: t(lang, "dialog.note.text"), value: text, autofocus: true }
            div { class: "field-grid",
                Num { label: t(lang, "dialog.watermark.size"), value: size, min: "4", max: "200" }
                div { class: "field", label { "{t(lang, \"dialog.watermark.color\")}" } input { r#type: "color", value: "{color}", oninput: move |e| color.set(e.value()) } }
            }
            label { class: "check", input { r#type: "checkbox", checked: with_border(), onchange: move |_| with_border.set(!with_border()) } span { "{t(lang, \"dialog.textbox.border\")}" } }
        }
    }
}

/// What a new link does, in the order of its dialog.
const LINK_KINDS: [&str; 5] = ["web", "page", "dest", "file", "script"];

#[component]
pub(super) fn LinkDialog(page: usize, rect: Vec<f32>) -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let url = use_signal(|| "https://".to_string());
    let target_page = use_signal(|| "1".to_string());
    let mut file = use_signal(String::new);
    let script = use_signal(|| "app.alert('…');".to_string());
    // The kind chosen by the command that started the link tool.
    let mut kind = use_signal(|| {
        let k = st.value("link.kind");
        if LINK_KINDS.contains(&k.as_str()) {
            k
        } else {
            "web".to_string()
        }
    });
    let dests: Vec<String> = st
        .read_active(|t| t.doc.named_destinations().into_iter().map(|d| d.name).collect())
        .unwrap_or_default();
    let mut dest = use_signal(|| dests.first().cloned().unwrap_or_default());
    let count = st.page_count();
    let r = [rect[0], rect[1], rect[2], rect[3]];
    rsx! {
        Frame { title: t(lang, "dialog.link.title"),
            on_ok: move |_| {
                match kind().as_str() {
                    "page" => {
                        let p = num_or(&target_page, 1.0) as usize;
                        if p < 1 || p > count {
                            st.error(tf(st.lang(), "dialog.replace.bad_range", &[("count", &count.to_string())]));
                            return;
                        }
                        apply(st, |tab| tab.doc.add_page_link(page, r, p - 1).map(|_| ()));
                    }
                    "dest" => {
                        let d = dest();
                        apply(st, |tab| tab.doc.add_named_link(page, r, &d).map(|_| ()));
                    }
                    "file" => {
                        let f = file();
                        apply(st, |tab| tab.doc.add_launch_link(page, r, &f).map(|_| ()));
                    }
                    "script" => {
                        let s = script();
                        apply(st, |tab| tab.doc.add_script_link(page, r, &s).map(|_| ()));
                    }
                    _ => {
                        let u = url();
                        apply(st, |tab| tab.doc.add_uri_link(page, r, &u).map(|_| ()));
                    }
                }
                tool_done(st);
            },
            div { class: "field",
                div { class: "radio-row",
                    for k in LINK_KINDS {
                        {
                            let name = t(lang, &format!("dialog.link.{k}"));
                            rsx! {
                                label { key: "{k}",
                                    input { r#type: "radio", name: "link-kind", checked: kind() == k, disabled: k == "dest" && dests.is_empty(),
                                        onchange: move |_| kind.set(k.to_string()) }
                                    span { "{name}" }
                                }
                            }
                        }
                    }
                }
            }
            match kind().as_str() {
                "page" => rsx! { Num { label: format!("{} (1-{count})", t(lang, "status.page")), value: target_page, min: "1", max: count.to_string() } },
                "dest" => rsx! {
                    div { class: "field", label { "{t(lang, \"dialog.link.dest\")}" }
                        select { onchange: move |e| dest.set(e.value()),
                            for d in dests.iter() { option { key: "{d}", value: "{d}", selected: *d == dest(), "{d}" } }
                        }
                    }
                },
                "file" => rsx! {
                    div { class: "field", label { "{t(lang, \"dialog.link.file\")}" }
                        div { class: "row",
                            input { class: "grow", value: "{file}", autofocus: true, oninput: move |e| file.set(e.value()) }
                            button { class: "tbtn", onclick: move |_| {
                                    spawn(async move {
                                        if let Some(f) = rfd::AsyncFileDialog::new().pick_file().await {
                                            file.set(f.path().display().to_string());
                                        }
                                    });
                                },
                                "{t(lang, \"dialog.browse\")}"
                            }
                        }
                        p { class: "hint", "{t(lang, \"dialog.link.file_hint\")}" }
                    }
                },
                "script" => rsx! { Area { label: t(lang, "dialog.link.script"), value: script, autofocus: true } },
                _ => rsx! { Text { label: t(lang, "dialog.link.url"), value: url, autofocus: true } },
            }
        }
    }
}

#[component]
pub(super) fn FieldValue(id_num: u32, id_gen: u16, name: String, value: String) -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let v = use_signal(|| value.clone());
    rsx! {
        Frame { title: format!("{} — {}", t(lang, "dialog.field.title"), name),
            on_ok: move |_| { let val = v(); apply(st, |tab| tab.doc.set_field_value((id_num, id_gen), &val)); },
            Text { label: name.clone(), value: v, autofocus: true }
        }
    }
}

/// A permission checkbox: label key, current state and the toggle.
pub(super) type PermRow = (&'static str, bool, fn(&mut SecurityPermissions));

#[component]
pub(super) fn Security() -> Element {
    use crate::commands::policies::{self as pol, PRESET};
    use crate::ui::features::policies::profile_label;
    use folitecta_core::{EncryptionProfile, PasswordIssue};
    let st = use_context::<AppState>();
    let lang = st.lang();
    let mut user = use_signal(String::new);
    let mut owner = use_signal(String::new);
    // A policy applied from the list of policies sets the permissions.
    let preset = use_hook(|| PRESET.write().take());
    let mut policy = use_signal({
        let preset = preset.clone();
        move || preset.map(|p| p.name).unwrap_or_default()
    });
    let mut perms = use_signal({
        let preset = preset.clone();
        move || preset.map(|p| p.permissions).unwrap_or_default()
    });
    let mut profile = use_signal({
        let preset = preset.clone();
        move || preset.map(|p| p.encryption).unwrap_or_default()
    });
    let mut wants_open = use_signal(move || preset.is_some_and(|p| p.open_password));
    let mut new_name = use_signal(String::new);
    // A signed document (its signatures do not hold in the copy), and the
    // protection the document has already.
    let (signed, current) = use_hook(|| {
        st.read_active(|t| (t.doc.has_signatures(), t.doc.encryption().map(|e| e.method)))
            .unwrap_or((false, None))
    });
    let policies = pol::policies(st);
    let p = perms();
    let restricted = p != SecurityPermissions::default();
    let rows: [PermRow; 6] = [
        ("dialog.security.print", p.print, |p| p.print = !p.print),
        ("dialog.security.modify", p.modify, |p| p.modify = !p.modify),
        ("dialog.security.copy", p.copy, |p| p.copy = !p.copy),
        ("dialog.security.annotate", p.annotate, |p| {
            p.annotate = !p.annotate
        }),
        ("dialog.security.fill", p.fill_forms, |p| {
            p.fill_forms = !p.fill_forms
        }),
        ("dialog.security.assemble", p.assemble, |p| {
            p.assemble = !p.assemble
        }),
    ];
    rsx! {
        Frame { title: t(lang, "dialog.security.title"), wide: true, ok_label: Some(t(lang, "dialog.security.save")),
            on_ok: move |_| {
                let (u, o) = (user(), owner());
                if u.is_empty() && o.is_empty() {
                    st.toast(ToastKind::Warn, t(st.lang(), "dialog.security.need_password"), "");
                    return;
                }
                // Passwords the encryption cannot keep whole: the dialog stays open.
                let chosen = profile();
                if let Err(issue) = chosen.check_password(&u).and(chosen.check_password(&o)) {
                    let key = match issue {
                        PasswordIssue::Characters => "dialog.security.password_chars",
                        PasswordIssue::TooLong => "dialog.security.password_long",
                    };
                    st.toast(ToastKind::Warn, t(st.lang(), key), "");
                    return;
                }
                st.set_dialog(Dialog::None);
                files::save_encrypted(st, u, o, perms(), chosen);
            },
            div { class: "callout", "{t(lang, \"dialog.security.hint\")}" }
            if let Some(method) = current.clone() {
                p { class: "hint security-current", "{tf(lang, \"dialog.security.current\", &[(\"method\", &method)])}" }
            }
            if signed {
                p { class: "hint warn security-signed-warning", "{t(lang, \"dialog.security.signed_warning\")}" }
            }
            div { class: "field",
                label { "{t(lang, \"policy.policy\")}" }
                select { class: "security-policy",
                    onchange: {
                        let policies = policies.clone();
                        move |e: Event<FormData>| {
                            policy.set(e.value());
                            if let Some(p) = policies.iter().find(|p| p.name == e.value()) {
                                perms.set(p.permissions);
                                wants_open.set(p.open_password);
                                profile.set(p.encryption);
                            }
                        }
                    },
                    option { value: "", selected: policy().is_empty(), "{t(lang, \"policy.none\")}" }
                    for p in policies.iter() {
                        option { key: "{p.name}", value: "{p.name}", selected: policy() == p.name, "{p.name}" }
                    }
                }
            }
            div { class: "field",
                label { "{t(lang, \"dialog.security.encryption\")}" }
                select { class: "security-encryption",
                    onchange: move |e: Event<FormData>| {
                        profile.set(EncryptionProfile::from_key(&e.value()));
                        policy.set(String::new());
                    },
                    for choice in EncryptionProfile::ALL {
                        option { key: "{choice.key()}", value: "{choice.key()}", selected: profile() == choice, "{t(lang, profile_label(choice))}" }
                    }
                }
            }
            div { class: "field-grid",
                div { class: "field", label { "{t(lang, \"dialog.security.user\")}" } input { r#type: "password", class: "security-user", autofocus: true, value: "{user}", oninput: move |e| user.set(e.value()) } }
                div { class: "field", label { "{t(lang, \"dialog.security.owner\")}" } input { r#type: "password", class: "security-owner", value: "{owner}", oninput: move |e| owner.set(e.value()) } }
            }
            if wants_open() && user().is_empty() {
                p { class: "hint warn", "{t(lang, \"policy.needs_open_password\")}" }
            }
            // Without an owner password the password to open is the owner's:
            // whoever opens the file can lift its restrictions.
            if restricted && owner().is_empty() {
                p { class: "hint warn security-owner-hint", "{t(lang, \"dialog.security.owner_empty\")}" }
            }
            b { "{t(lang, \"dialog.security.permissions\")}" }
            div { class: "check-grid",
                for (key, on, toggle) in rows {
                    label { class: "check", key: "{key}",
                        input { r#type: "checkbox", checked: on, onchange: move |_| { toggle(&mut perms.write()); policy.set(String::new()); } }
                        span { "{t(lang, key)}" }
                    }
                }
            }
            div { class: "row",
                input { class: "grow security-policy-name", value: "{new_name}", placeholder: "{t(lang, \"policy.name\")}",
                    oninput: move |e| new_name.set(e.value()) }
                button { class: "tbtn security-save-policy",
                    onclick: move |_| {
                        pol::save_permissions(st, &new_name(), perms(), !user().is_empty(), profile());
                        new_name.set(String::new());
                    },
                    "{t(lang, \"policy.save_as\")}"
                }
            }
        }
    }
}

#[component]
pub(super) fn Flatten() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let mode = use_signal(|| Scope::All);
    let range = use_signal(String::new);
    let mut what = use_signal(|| "comments".to_string());
    rsx! {
        Frame { title: t(lang, "dialog.flatten.title"),
            on_ok: move |_| match scope_pages(st, mode(), &range()) {
                Ok(pages) => {
                    let scope = match what().as_str() { "fields" => FlattenScope::FormFields, "all" => FlattenScope::All, _ => FlattenScope::Comments };
                    apply_count(st, "toast.flattened", |tab| tab.doc.flatten(&pages, scope));
                }
                Err(e) => st.error(e),
            },
            div { class: "hint", "{t(lang, \"dialog.flatten.hint\")}" }
            div { class: "field",
                div { class: "radio-row",
                    label { input { r#type: "radio", checked: what() == "comments", onchange: move |_| what.set("comments".into()) } "{t(lang, \"dialog.flatten.comments\")}" }
                    label { input { r#type: "radio", checked: what() == "fields", onchange: move |_| what.set("fields".into()) } "{t(lang, \"dialog.flatten.fields\")}" }
                    label { input { r#type: "radio", checked: what() == "all", onchange: move |_| what.set("all".into()) } "{t(lang, \"dialog.flatten.all\")}" }
                }
            }
            PageScope { mode, range }
        }
    }
}

#[component]
pub(super) fn CalloutDialog(page: usize, tip_x: f32, tip_y: f32, rect: Vec<f32>) -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let text = use_signal(String::new);
    let r = [rect[0], rect[1], rect[2], rect[3]];
    rsx! {
        Frame { title: t(lang, "tool.callout"),
            on_ok: move |_| {
                let c = text();
                let author = st.ui.read().author.clone();
                let style = crate::state::comment_styles::default_style(st, "Callout");
                apply(st, |tab| tab.doc.add_callout(page, (tip_x, tip_y), r, &c, &style, &author).map(|_| ()));
                tool_done(st);
            },
            Area { label: t(lang, "dialog.note.text"), value: text, autofocus: true }
        }
    }
}

/// The text typed on the page that opened a text dialog (taken once).
fn typed_text() -> String {
    let typed = crate::commands::textedit::TYPED.peek().clone();
    if !typed.is_empty() {
        spawn(async {
            crate::commands::textedit::TYPED.write().clear();
        });
    }
    typed
}

/// Change the text of the page, or suggest the change with a comment.
#[component]
fn TextMode(note: Signal<bool>) -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let mut note = note;
    rsx! {
        div { class: "radio-col",
            label { class: "check",
                input { r#type: "radio", name: "text-mode", checked: !note(), onchange: move |_| note.set(false) }
                span { "{t(lang, \"text.mode.page\")}" }
            }
            label { class: "check",
                input { r#type: "radio", name: "text-mode", checked: note(), onchange: move |_| note.set(true) }
                span { "{t(lang, \"text.mode.note\")}" }
            }
        }
    }
}

#[component]
pub(super) fn InsertTextDialog(page: usize, index: usize, after: bool, note: bool) -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    // What was typed on the page to open the dialog comes first.
    let text = use_signal(typed_text);
    let as_note = use_signal(|| note);
    rsx! {
        Frame { title: t(lang, "tool.insert_text"),
            on_ok: move |_| {
                let c = text();
                if as_note() {
                    let author = st.ui.read().author.clone();
                    let color = crate::state::comment_styles::default_style(st, "Caret").color.unwrap_or([0.1, 0.4, 0.95]);
                    apply(st, |tab| tab.doc.insert_text_mark(page, index, after, &c, color, &author).map(|_| ()));
                } else {
                    let at = if after { index + 1 } else { index };
                    if crate::commands::textedit::edit_text(st, page, at, at, &c) {
                        st.set_dialog(Dialog::None);
                    }
                }
                tool_done(st);
            },
            Area { label: t(lang, "dialog.insert_text.text"), value: text, autofocus: true }
            TextMode { note: as_note }
        }
    }
}

#[component]
pub(super) fn ReplaceTextDialog(page: usize, start: usize, end: usize, note: bool) -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let original = st
        .read_active(|t| t.doc.text_range(page, start, end))
        .unwrap_or_default();
    // What was typed on the page comes first; otherwise changing the page
    // starts from the text there, and a suggestion from nothing.
    let start_text = original.clone();
    let text = use_signal(move || {
        let typed = typed_text();
        if !typed.is_empty() {
            typed
        } else if note {
            String::new()
        } else {
            start_text.clone()
        }
    });
    let as_note = use_signal(|| note);
    rsx! {
        Frame { title: t(lang, "tool.replace_text"),
            on_ok: move |_| {
                let c = text();
                if as_note() {
                    let author = st.ui.read().author.clone();
                    let color = crate::state::comment_styles::default_style(st, "Caret").color.unwrap_or([0.1, 0.4, 0.95]);
                    apply(st, |tab| {
                        let r = tab.doc.replace_text_range(page, start, end, &c, color, &author);
                        tab.text_sel = None;
                        r
                    });
                } else {
                    if crate::commands::textedit::edit_text(st, page, start, end, &c) {
                        st.set_dialog(Dialog::None);
                    }
                }
                tool_done(st);
            },
            div { class: "kv", span { class: "k", "{t(lang, \"dialog.replace_text.original\")}" } span { class: "v", "{original}" } }
            Area { label: t(lang, "dialog.replace_text.with"), value: text, autofocus: true }
            TextMode { note: as_note }
        }
    }
}
