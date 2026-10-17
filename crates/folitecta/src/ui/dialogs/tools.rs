//! Dialogs of the drawing tools: notes, text boxes, links, callouts, text edits and field values; security and flattening.

use super::*;

/// Return to the hand tool after creating something, unless the user keeps tools selected.
pub(super) fn tool_done(st: AppState) {
    if !option_on(st, "tool.keepSelected") {
        st.set_tool(Tool::Hand);
    }
}

#[component]
pub(super) fn NoteDialog(page: usize, x: f32, y: f32) -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let text = use_signal(String::new);
    let author = use_signal(|| st.ui.read().author.clone());
    let mut color = use_signal(|| st.ui.read().value("note.color").unwrap_or("#ffd833").to_string());
    rsx! {
        Frame { title: t(lang, "dialog.note.title"),
            on_ok: move |_| {
                let (c, a, col) = (text(), author(), color());
                st.set_value("note.color", col.clone());
                apply(st, |tab| tab.doc.add_note(page, x, y, &c, &a, parse_color(&col)).map(|_| ()));
                tool_done(st);
            },
            Area { label: t(lang, "dialog.note.text"), value: text, autofocus: true }
            div { class: "field-grid",
                Text { label: t(lang, "dialog.note.author"), value: author }
                div { class: "field", label { "{t(lang, \"dialog.watermark.color\")}" } input { r#type: "color", value: "{color}", oninput: move |e| color.set(e.value()) } }
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

#[component]
pub(super) fn LinkDialog(page: usize, rect: Vec<f32>) -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let url = use_signal(|| "https://".to_string());
    let target_page = use_signal(|| "1".to_string());
    let mut kind = use_signal(|| "web".to_string());
    let count = st.page_count();
    let r = [rect[0], rect[1], rect[2], rect[3]];
    rsx! {
        Frame { title: t(lang, "dialog.link.title"),
            on_ok: move |_| {
                if kind() == "page" {
                    let p = num_or(&target_page, 1.0) as usize;
                    if p < 1 || p > count {
                        st.error(tf(st.lang(), "dialog.replace.bad_range", &[("count", &count.to_string())]));
                        return;
                    }
                    apply(st, |tab| tab.doc.add_page_link(page, r, p - 1).map(|_| ()));
                } else {
                    let u = url();
                    apply(st, |tab| tab.doc.add_uri_link(page, r, &u).map(|_| ()));
                }
                tool_done(st);
            },
            div { class: "field",
                div { class: "radio-row",
                    label { input { r#type: "radio", checked: kind() == "web", onchange: move |_| kind.set("web".into()) } "{t(lang, \"dialog.link.web\")}" }
                    label { input { r#type: "radio", checked: kind() == "page", onchange: move |_| kind.set("page".into()) } "{t(lang, \"dialog.link.page\")}" }
                }
            }
            if kind() == "page" {
                Num { label: format!("{} (1-{count})", t(lang, "status.page")), value: target_page, min: "1", max: count.to_string() }
            } else {
                Text { label: t(lang, "dialog.link.url"), value: url, autofocus: true }
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
    let st = use_context::<AppState>();
    let lang = st.lang();
    let mut user = use_signal(String::new);
    let mut owner = use_signal(String::new);
    let mut perms = use_signal(SecurityPermissions::default);
    let p = perms();
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
                st.set_dialog(Dialog::None);
                files::save_encrypted(st, u, o, perms());
            },
            div { class: "callout", "{t(lang, \"dialog.security.hint\")}" }
            div { class: "field-grid",
                div { class: "field", label { "{t(lang, \"dialog.security.user\")}" } input { r#type: "password", autofocus: true, value: "{user}", oninput: move |e| user.set(e.value()) } }
                div { class: "field", label { "{t(lang, \"dialog.security.owner\")}" } input { r#type: "password", value: "{owner}", oninput: move |e| owner.set(e.value()) } }
            }
            b { "{t(lang, \"dialog.security.permissions\")}" }
            div { class: "check-grid",
                for (key, on, toggle) in rows {
                    label { class: "check", key: "{key}",
                        input { r#type: "checkbox", checked: on, onchange: move |_| toggle(&mut perms.write()) }
                        span { "{t(lang, key)}" }
                    }
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
                let style = crate::styles::default_style(st, "Callout");
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
                    let color = crate::styles::default_style(st, "Caret").color.unwrap_or([0.1, 0.4, 0.95]);
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
                    let color = crate::styles::default_style(st, "Caret").color.unwrap_or([0.1, 0.4, 0.95]);
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
