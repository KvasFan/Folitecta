//! JavaScript console with snippets, the named scripts of the document and
//! the JavaScript preferences.

use dioxus::prelude::*;
use folitecta_core::registry;

use crate::commands::dispatch;
use crate::commands::javascript::{CONSOLE, changes_allowed, edit_snippet, run, snippets, timeout_secs};
use crate::i18n::{Lang, cmd_label, t};
use crate::icons::Icon;
use crate::state::{AppState, Dialog};
use crate::ui::dialogs::Frame;

fn label_of(lang: Lang, id: &str) -> String {
    registry()
        .get(id)
        .map(|c| cmd_label(lang, c))
        .unwrap_or_else(|| id.to_string())
}

#[component]
pub fn JsConsoleDialog() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let console = CONSOLE();
    let code = st.value("js.code");
    let list = snippets();
    let tools: [(&str, &str); 5] = [
        ("jsConsole.run", "next"),
        ("jsConsole.runSelect", "select"),
        ("jsConsole.clear", "close"),
        ("jssnippet.add", "save"),
        ("jsConsole.prefs", "settings"),
    ];
    let snippet_tools: [(&str, &str); 4] = [
        ("jsEditSnippets", "pencil"),
        ("jssnippet.clone", "copy"),
        ("jssnippet.delete", "delete"),
        ("jsConsole.openFolder", "folder"),
    ];
    rsx! {
        Frame { title: label_of(lang, "jsConsole"), wide: true,
            div { class: "pane-toolbar icon-toolbar",
                for (id, icon) in tools {
                    button { class: "ibtn", key: "{id}", title: "{label_of(lang, id)}", disabled: console.running && id.starts_with("jsConsole.run"),
                        onclick: move |_| dispatch(st, id), Icon { name: icon } }
                }
                if console.running { span { class: "muted", "{t(lang, \"js.running\")}" } }
            }
            div { class: "js-console",
                div { class: "js-main",
                    textarea { id: "js-code", class: "code js-code", rows: "8", spellcheck: "false", value: "{code}",
                        placeholder: "{t(lang, \"js.placeholder\")}",
                        oninput: move |e| { st.ui_mut().values.insert("js.code".into(), e.value()); },
                        onchange: move |_| st.save_prefs(),
                        onkeydown: move |e| {
                            // Ctrl+Enter runs the code.
                            if e.key() == Key::Enter && (e.modifiers().ctrl() || e.modifiers().meta()) {
                                e.prevent_default();
                                e.stop_propagation();
                                run(st, st.value("js.code"));
                            } else if e.key() != Key::Escape {
                                e.stop_propagation();
                            }
                        }
                    }
                    div { class: "js-output",
                        if console.output.is_empty() { div { class: "muted", "{t(lang, \"js.output_empty\")}" } }
                        for (i, (kind, text)) in console.output.iter().enumerate() {
                            div { class: "js-line {kind}", key: "{i}",
                                span { class: "js-mark", match kind.as_str() { "in" => "›", "result" => "←", "error" => "✗", "alert" => "!", _ => " " } }
                                pre { "{text}" }
                            }
                        }
                    }
                }
                div { class: "js-snippets",
                    div { class: "js-snippets-head",
                        span { "{t(lang, \"js.snippets\")}" }
                        div { class: "spacer" }
                        for (id, icon) in snippet_tools {
                            button { class: "ibtn", key: "{id}", title: "{label_of(lang, id)}", onclick: move |_| dispatch(st, id), Icon { name: icon } }
                        }
                    }
                    if list.is_empty() { div { class: "muted js-hint", "{t(lang, \"js.no_snippets\")}" } }
                    for (name, _) in list.into_iter() {
                        {
                            let selected = console.snippet.as_deref() == Some(name.as_str());
                            let pick = name.clone();
                            let edit = name.clone();
                            rsx! {
                                div { class: if selected { "tree-item selected" } else { "tree-item" }, key: "{name}",
                                    onclick: move |_| CONSOLE.write().snippet = Some(pick.clone()),
                                    ondoubleclick: move |_| { CONSOLE.write().snippet = Some(edit.clone()); edit_snippet(st); },
                                    Icon { name: "text" }
                                    span { class: "tree-label", "{name}" }
                                }
                            }
                        }
                    }
                }
            }
            p { class: "hint", "{t(lang, \"js.hint\")}" }
        }
    }
}

/// Named scripts of the document. `add` starts a new script.
#[component]
pub fn DocScriptsDialog(add: Option<String>) -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let mut list = use_signal(|| {
        let mut l = st.read_active(|t| t.doc.document_scripts()).unwrap_or_default();
        if add.is_some() {
            l.push((t(lang, "js.new_script"), String::new()));
        }
        l
    });
    let mut sel = use_signal(|| {
        let n = list.peek().len();
        if add.is_some() {
            Some(n.saturating_sub(1))
        } else if n > 0 {
            Some(0)
        } else {
            None
        }
    });
    let current = sel().and_then(|i| list.read().get(i).cloned());
    let open_action = st
        .read_active(|t| t.doc.scripts().into_iter().any(|s| s.source != "Document"))
        .unwrap_or(false);
    rsx! {
        Frame { title: label_of(lang, "javaScriptList"), wide: true,
            on_ok: move |_| {
                let l = list();
                let r = st.with_active(|t| t.doc.set_document_scripts(&l));
                match r {
                    Some(Ok(())) => { crate::files::set_window_title(st); st.set_dialog(Dialog::None); }
                    Some(Err(e)) => st.error(e),
                    None => {}
                }
            },
            div { class: "js-scripts",
                div { class: "js-script-list",
                    div { class: "pane-toolbar icon-toolbar",
                        button { class: "ibtn", title: "{label_of(lang, \"javaScriptList.add\")}",
                            onclick: move |_| { list.write().push((t(lang, "js.new_script"), String::new())); let n = list.read().len(); sel.set(Some(n - 1)); },
                            Icon { name: "new" } }
                        button { class: "ibtn", title: "{label_of(lang, \"javaScriptList.delete\")}", disabled: sel().is_none(),
                            onclick: move |_| { if let Some(i) = sel() { list.write().remove(i); let n = list.read().len(); sel.set(if n == 0 { None } else { Some(i.min(n - 1)) }); } },
                            Icon { name: "delete" } }
                    }
                    if list.read().is_empty() { div { class: "muted js-hint", "{t(lang, \"js.no_doc_scripts\")}" } }
                    for (i, (name, _)) in list.read().iter().cloned().enumerate() {
                        div { class: if sel() == Some(i) { "tree-item selected" } else { "tree-item" }, key: "{i}",
                            onclick: move |_| sel.set(Some(i)),
                            Icon { name: "text" }
                            span { class: "tree-label", "{name}" }
                        }
                    }
                }
                if let (Some(i), Some((name, code))) = (sel(), current) {
                    div { class: "js-script-edit",
                        div { class: "field", label { "{t(lang, \"js.script_name\")}" }
                            input { value: "{name}", oninput: move |e| { if let Some(s) = list.write().get_mut(i) { s.0 = e.value(); } } } }
                        div { class: "field", label { "{label_of(lang, \"javaScriptList.edit\")}" }
                            textarea { class: "code js-code", rows: "10", spellcheck: "false", value: "{code}",
                                onkeydown: move |e| { if e.key() != Key::Escape { e.stop_propagation(); } },
                                oninput: move |e| { if let Some(s) = list.write().get_mut(i) { s.1 = e.value(); } } } }
                        button { class: "tbtn", onclick: move |_| { let c = list.read().get(i).map(|s| s.1.clone()).unwrap_or_default(); run(st, c); st.set_dialog(Dialog::JsConsole); },
                            Icon { name: "next" } "{t(lang, \"js.run_in_console\")}" }
                    }
                }
            }
            if open_action { p { class: "hint", "{t(lang, \"js.open_action_note\")}" } }
            button { class: "tbtn danger", onclick: move |_| crate::commands::confirm(st, "dialog.scripts.remove_all", "dialog.scripts.remove_body", crate::state::ConfirmAction::RemoveScripts),
                Icon { name: "delete" } "{t(lang, \"dialog.scripts.remove_all\")}" }
        }
    }
}

#[component]
pub fn JsPrefsDialog() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let mut secs = use_signal(|| timeout_secs(st).to_string());
    let mut allow = use_signal(|| changes_allowed(st));
    let mut alerts = use_signal(|| st.value("js.alerts") != "0");
    rsx! {
        Frame { title: label_of(lang, "view.javaScriptOpts"),
            on_ok: move |_| {
                let s = secs().trim().parse::<u64>().unwrap_or(3).clamp(1, 60);
                st.set_value("js.timeout", s.to_string());
                st.set_value("js.changes", if allow() { "" } else { "0" });
                st.set_value("js.alerts", if alerts() { "" } else { "0" });
                st.set_dialog(Dialog::None);
            },
            div { class: "field", label { "{t(lang, \"js.timeout\")}" }
                input { r#type: "number", min: "1", max: "60", value: "{secs}", oninput: move |e| secs.set(e.value()) } }
            label { class: "check",
                input { r#type: "checkbox", checked: allow(), onchange: move |_| allow.set(!allow()) }
                span { "{t(lang, \"js.allow_changes\")}" }
            }
            label { class: "check",
                input { r#type: "checkbox", checked: alerts(), onchange: move |_| alerts.set(!alerts()) }
                span { "{t(lang, \"js.alerts\")}" }
            }
            p { class: "hint", "{t(lang, \"js.prefs_hint\")}" }
        }
    }
}
