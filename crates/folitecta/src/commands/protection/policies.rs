//! Security policies: named sets of the permissions of password protection,
//! kept in the settings, managed in a list (new, edit, clone, rename,
//! delete, import, export) and applied to the Password Security dialog.
//! Passwords are never kept: they are typed when the policy is applied.

use dioxus::prelude::*;
use folitecta_core::SecurityPermissions;
use serde::{Deserialize, Serialize};

use crate::commands::{handlers, no_doc};
use crate::i18n::{Lang, t};
use crate::state::{AppState, Dialog, ToastKind};

/// A named set of permissions.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SecurityPolicy {
    pub name: String,
    pub permissions: SecurityPermissions,
    /// The document asks for a password to be opened (else only the
    /// permissions are protected, by the owner password).
    pub open_password: bool,
}

/// What the list shows: the policy selected and the one being edited
/// (`Some(index)` for a policy of the list, `None` for a new one).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PolicyView {
    pub selected: Option<usize>,
    pub editing: Option<(Option<usize>, SecurityPolicy)>,
}

pub static VIEW: GlobalSignal<PolicyView> = Signal::global(PolicyView::default);

/// The policy the Password Security dialog starts from.
pub static PRESET: GlobalSignal<Option<SecurityPolicy>> = Signal::global(|| None);

/// The policies offered before any was saved.
fn defaults(lang: Lang) -> Vec<SecurityPolicy> {
    vec![
        SecurityPolicy {
            name: t(lang, "policy.default_read_only"),
            permissions: SecurityPermissions {
                print: true,
                modify: false,
                copy: false,
                annotate: false,
                fill_forms: false,
                assemble: false,
            },
            open_password: false,
        },
        SecurityPolicy {
            name: t(lang, "policy.default_no_print"),
            permissions: SecurityPermissions {
                print: false,
                copy: false,
                ..SecurityPermissions::default()
            },
            open_password: false,
        },
    ]
}

/// The saved policies.
pub fn policies(st: AppState) -> Vec<SecurityPolicy> {
    let saved = st.value("security.policies");
    if saved.trim().is_empty() {
        return defaults(st.lang());
    }
    serde_json::from_str(&saved).unwrap_or_else(|_| defaults(st.lang()))
}

fn save(st: AppState, list: &[SecurityPolicy]) {
    if let Ok(json) = serde_json::to_string(list) {
        st.set_value("security.policies", json);
    }
}

/// A name not used yet: `name`, else `name (2)`, `name (3)`, ...
pub fn free_name(list: &[SecurityPolicy], name: &str) -> String {
    let name = name.trim();
    let taken = |n: &str| list.iter().any(|p| p.name.eq_ignore_ascii_case(n));
    if !taken(name) {
        return name.to_string();
    }
    (2..)
        .map(|i| format!("{name} ({i})"))
        .find(|n| !taken(n))
        .unwrap_or_else(|| name.to_string())
}

/// Add policies read from a file, renaming those whose name is taken.
pub fn merged(mut list: Vec<SecurityPolicy>, more: Vec<SecurityPolicy>) -> Vec<SecurityPolicy> {
    for mut p in more {
        if p.name.trim().is_empty() {
            continue;
        }
        p.name = free_name(&list, &p.name);
        list.push(p);
    }
    list
}

/// Open the list of policies.
pub fn open(st: AppState) {
    *VIEW.write() = PolicyView::default();
    st.set_dialog(Dialog::SecurityPolicies);
}

fn selected(st: AppState) -> Option<(usize, SecurityPolicy)> {
    let i = VIEW.peek().selected?;
    policies(st).get(i).cloned().map(|p| (i, p))
}

/// Run `f` on the list when it is shown, else show it.
fn on_list(st: AppState, f: impl FnOnce(AppState)) {
    if matches!(*st.dialog.peek(), Dialog::SecurityPolicies) {
        f(st);
    } else {
        open(st);
    }
}

pub fn add(st: AppState) {
    on_list(st, |st| {
        let list = policies(st);
        let policy = SecurityPolicy {
            name: free_name(&list, &t(st.lang(), "policy.new_name")),
            ..SecurityPolicy::default()
        };
        VIEW.write().editing = Some((None, policy));
    });
}

pub fn edit(st: AppState) {
    on_list(st, |st| match selected(st) {
        Some((i, p)) => VIEW.write().editing = Some((Some(i), p)),
        None => st.toast(ToastKind::Info, t(st.lang(), "toast.choose_policy"), ""),
    });
}

pub fn clone(st: AppState) {
    on_list(st, |st| match selected(st) {
        Some((_, p)) => {
            let list = policies(st);
            let copy = SecurityPolicy {
                name: free_name(&list, &p.name),
                ..p
            };
            VIEW.write().editing = Some((None, copy));
        }
        None => st.toast(ToastKind::Info, t(st.lang(), "toast.choose_policy"), ""),
    });
}

pub fn delete(st: AppState) {
    on_list(st, |st| {
        if let Some((i, _)) = selected(st) {
            let mut list = policies(st);
            list.remove(i);
            save(st, &list);
            let mut v = VIEW.write();
            v.selected = if list.is_empty() {
                None
            } else {
                Some(i.min(list.len() - 1))
            };
            v.editing = None;
        }
    });
}

/// Keep the policy being edited (refused when its name is empty or taken).
pub fn save_edit(st: AppState) {
    let Some((at, policy)) = VIEW.peek().editing.clone() else {
        return;
    };
    let mut list = policies(st);
    let name = policy.name.trim().to_string();
    let others: Vec<SecurityPolicy> = list
        .iter()
        .enumerate()
        .filter(|(i, _)| Some(*i) != at)
        .map(|(_, p)| p.clone())
        .collect();
    if name.is_empty() || others.iter().any(|p| p.name.eq_ignore_ascii_case(&name)) {
        st.toast(ToastKind::Warn, t(st.lang(), "toast.policy_name"), "");
        return;
    }
    let policy = SecurityPolicy { name, ..policy };
    let index = match at {
        Some(i) if i < list.len() => {
            list[i] = policy;
            i
        }
        _ => {
            list.push(policy);
            list.len() - 1
        }
    };
    save(st, &list);
    let mut v = VIEW.write();
    v.selected = Some(index);
    v.editing = None;
}

/// Open the Password Security dialog with the selected policy.
pub fn apply(st: AppState) {
    if no_doc(st) {
        return;
    }
    match selected(st) {
        Some((_, p)) => {
            *PRESET.write() = Some(p);
            st.set_dialog(Dialog::Security);
        }
        None => st.toast(ToastKind::Info, t(st.lang(), "toast.choose_policy"), ""),
    }
}

/// Keep the permissions of the Password Security dialog as a new policy.
pub fn save_permissions(st: AppState, name: &str, permissions: SecurityPermissions, open_password: bool) {
    let mut list = policies(st);
    let name = if name.trim().is_empty() {
        t(st.lang(), "policy.new_name")
    } else {
        name.to_string()
    };
    list.push(SecurityPolicy {
        name: free_name(&list, &name),
        permissions,
        open_password,
    });
    save(st, &list);
    st.toast(ToastKind::Ok, t(st.lang(), "toast.policy_saved"), "");
}

fn export(st: AppState) {
    let list = policies(st);
    spawn(async move {
        let Some(file) = rfd::AsyncFileDialog::new()
            .add_filter("JSON", &["json"])
            .set_file_name("security-policies.json")
            .save_file()
            .await
        else {
            return;
        };
        let json = serde_json::to_string_pretty(&list).unwrap_or_default();
        match std::fs::write(file.path(), json) {
            Ok(()) => st.toast(ToastKind::Ok, t(st.lang(), "toast.policies_exported"), ""),
            Err(e) => st.error(e),
        }
    });
}

fn import(st: AppState) {
    spawn(async move {
        let Some(file) = rfd::AsyncFileDialog::new()
            .add_filter("JSON", &["json"])
            .pick_file()
            .await
        else {
            return;
        };
        let text = String::from_utf8_lossy(&file.read().await).to_string();
        match serde_json::from_str::<Vec<SecurityPolicy>>(&text) {
            Ok(more) => {
                let n = more.len();
                save(st, &merged(policies(st), more));
                st.toast(
                    ToastKind::Ok,
                    crate::i18n::tf(st.lang(), "toast.policies_imported", &[("count", &n.to_string())]),
                    "",
                );
            }
            Err(e) => st.error(e),
        }
    });
}

handlers! {
    "securityPolicies" => open,
    "secPoliciesList" => open,
    "secPoliciesList.add" => add,
    "secPoliciesList.edit" => edit,
    "secPoliciesList.rename" => edit,
    "secPoliciesList.clone" => clone,
    "secPoliciesList.delete" => delete,
    "secPoliciesList.export" => export,
    "secPoliciesList.import" => import,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn imported_policies_get_names_of_their_own() {
        let p = |name: &str| SecurityPolicy {
            name: name.into(),
            ..SecurityPolicy::default()
        };
        let list = merged(
            vec![p("Read only")],
            vec![p("Read only"), p("read only"), p(" "), p("Other")],
        );
        let names: Vec<&str> = list.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["Read only", "Read only (2)", "read only (3)", "Other"]);
        // Policies written by an older version miss fields: they get the defaults.
        let old: Vec<SecurityPolicy> =
            serde_json::from_str(r#"[{"name":"A","permissions":{"print":false}}]"#).unwrap();
        assert!(!old[0].permissions.print && old[0].permissions.copy && !old[0].open_password);
    }
}
