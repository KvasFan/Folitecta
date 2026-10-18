//! The list of security policies: each with the permissions it allows,
//! edited in place, cloned, renamed, deleted, imported and exported, and
//! applied to the Password Security dialog.

use dioxus::prelude::*;
use folitecta_core::{SecurityPermissions, registry};

use crate::commands::policies::{self as pol, VIEW};
use crate::i18n::{Lang, cmd_label, t};
use crate::state::AppState;
use crate::ui::dialogs::Frame;

/// A permission: its label key, whether it is allowed, and the switch.
type Permission = (
    &'static str,
    fn(&SecurityPermissions) -> bool,
    fn(&mut SecurityPermissions),
);

/// The permissions of password protection.
pub const PERMISSIONS: [Permission; 6] = [
    ("dialog.security.print", |p| p.print, |p| p.print = !p.print),
    ("dialog.security.modify", |p| p.modify, |p| p.modify = !p.modify),
    ("dialog.security.copy", |p| p.copy, |p| p.copy = !p.copy),
    (
        "dialog.security.annotate",
        |p| p.annotate,
        |p| p.annotate = !p.annotate,
    ),
    (
        "dialog.security.fill",
        |p| p.fill_forms,
        |p| p.fill_forms = !p.fill_forms,
    ),
    (
        "dialog.security.assemble",
        |p| p.assemble,
        |p| p.assemble = !p.assemble,
    ),
];

/// What a set of permissions allows, in a few words.
pub fn summary(lang: Lang, p: &SecurityPermissions) -> String {
    let allowed: Vec<String> = PERMISSIONS
        .iter()
        .filter(|(_, get, _)| get(p))
        .map(|(key, _, _)| t(lang, key))
        .collect();
    if allowed.is_empty() {
        t(lang, "policy.nothing_allowed")
    } else {
        allowed.join(" · ")
    }
}

#[component]
pub fn SecurityPoliciesDialog() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let title = registry()
        .get("securityPolicies")
        .map(|c| cmd_label(lang, c))
        .unwrap_or_default();
    let label = |id: &str| registry().get(id).map(|c| cmd_label(lang, c)).unwrap_or_default();
    let list = pol::policies(st);
    let view = VIEW.read().clone();
    let has_sel = view.selected.is_some_and(|i| i < list.len());
    rsx! {
        Frame { title, wide: true, ok_label: Some(t(lang, "policy.apply")), on_ok: move |_| pol::apply(st),
            p { class: "hint", "{t(lang, \"policy.hint\")}" }
            div { class: "policy-list",
                for (i, p) in list.iter().enumerate() {
                    button { key: "{i}", class: if view.selected == Some(i) { "policy-row on" } else { "policy-row" },
                        onclick: move |_| VIEW.write().selected = Some(i),
                        ondoubleclick: move |_| { VIEW.write().selected = Some(i); pol::edit(st); },
                        span { class: "policy-name", "{p.name}" }
                        span { class: "muted policy-summary", "{summary(lang, &p.permissions)}" }
                        if p.open_password {
                            span { class: "chip", "{t(lang, \"policy.open_password\")}" }
                        }
                    }
                }
            }
            div { class: "row",
                button { class: "tbtn policy-add", onclick: move |_| pol::add(st), "{label(\"secPoliciesList.add\")}" }
                button { class: "tbtn policy-edit", disabled: !has_sel, onclick: move |_| pol::edit(st), "{label(\"secPoliciesList.edit\")}" }
                button { class: "tbtn policy-clone", disabled: !has_sel, onclick: move |_| pol::clone(st), "{label(\"secPoliciesList.clone\")}" }
                button { class: "tbtn danger policy-delete", disabled: !has_sel, onclick: move |_| pol::delete(st), "{label(\"secPoliciesList.delete\")}" }
                span { class: "grow" }
                button { class: "tbtn", onclick: move |_| crate::commands::dispatch(st, "secPoliciesList.import"), "{label(\"secPoliciesList.import\")}" }
                button { class: "tbtn", onclick: move |_| crate::commands::dispatch(st, "secPoliciesList.export"), "{label(\"secPoliciesList.export\")}" }
            }
            if let Some((_, editing)) = view.editing {
                div { class: "policy-editor",
                    div { class: "field", label { "{t(lang, \"policy.name\")}" }
                        input { class: "policy-name-input", value: "{editing.name}", autofocus: true,
                            oninput: move |e| { if let Some((_, p)) = VIEW.write().editing.as_mut() { p.name = e.value(); } },
                            onkeydown: move |e| { if e.key() == Key::Enter { e.stop_propagation(); pol::save_edit(st); } },
                        }
                    }
                    div { class: "check-grid",
                        for (key, get, toggle) in PERMISSIONS {
                            label { class: "check", key: "{key}",
                                input { r#type: "checkbox", checked: get(&editing.permissions),
                                    onchange: move |_| { if let Some((_, p)) = VIEW.write().editing.as_mut() { toggle(&mut p.permissions); } } }
                                span { "{t(lang, key)}" }
                            }
                        }
                    }
                    label { class: "check",
                        input { r#type: "checkbox", checked: editing.open_password,
                            onchange: move |_| { if let Some((_, p)) = VIEW.write().editing.as_mut() { p.open_password = !p.open_password; } } }
                        span { "{t(lang, \"policy.open_password_long\")}" }
                    }
                    div { class: "row",
                        button { class: "tbtn primary policy-save", onclick: move |_| pol::save_edit(st), "{t(lang, \"policy.save\")}" }
                        button { class: "tbtn", onclick: move |_| VIEW.write().editing = None, "{t(lang, \"dialog.cancel\")}" }
                    }
                }
            }
        }
    }
}
