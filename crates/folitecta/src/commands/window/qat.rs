//! The Quick Access Toolbar: the commands the user keeps at hand next to
//! the ribbon tabs (or in a row below the ribbon), chosen in Customize
//! Toolbars or with a right click on a tool, shown as icons with or without
//! their names, and separators between them. Kept in the settings.

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

use crate::commands::handlers;
use crate::i18n::{t, tf};
use crate::state::{AppState, Dialog, ToastKind};

/// An entry of the toolbar: a command, or a separator (`id` "|").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct QatItem {
    pub id: String,
    /// The name is written next to the icon.
    pub text: bool,
    /// The icon is shown (an entry shows at least its name or its icon).
    pub icon: bool,
}

impl Default for QatItem {
    fn default() -> Self {
        QatItem {
            id: String::new(),
            text: false,
            icon: true,
        }
    }
}

impl QatItem {
    pub fn command(id: &str) -> QatItem {
        QatItem {
            id: id.to_string(),
            ..QatItem::default()
        }
    }

    pub fn separator() -> QatItem {
        QatItem::command(SEPARATOR)
    }

    pub fn is_separator(&self) -> bool {
        self.id == SEPARATOR
    }
}

pub const SEPARATOR: &str = "|";

/// The toolbar of a new installation: open, save, undo, redo.
pub fn defaults() -> Vec<QatItem> {
    vec![
        QatItem {
            text: true,
            ..QatItem::command("open")
        },
        QatItem {
            text: true,
            ..QatItem::command("save")
        },
        QatItem::command("undo"),
        QatItem::command("redo"),
    ]
}

/// The entries of the toolbar, as saved.
pub fn items(st: AppState) -> Vec<QatItem> {
    let saved = st.value("qat.items");
    if saved.trim().is_empty() {
        return defaults();
    }
    serde_json::from_str(&saved).unwrap_or_else(|_| defaults())
}

pub fn save(st: AppState, list: &[QatItem]) {
    if let Ok(json) = serde_json::to_string(list) {
        st.set_value("qat.items", json);
    }
}

/// The toolbar is a row below the ribbon (else next to the tabs).
pub fn below(st: AppState) -> bool {
    st.value("qat.position") == "below"
}

/// The entry selected in Customize Toolbars.
pub static SELECTED: GlobalSignal<Option<usize>> = Signal::global(|| None);

/// Add a command at the end, or take it away when it is there already
/// (a right click on a tool). Returns whether it was added.
pub fn toggle(st: AppState, id: &str) -> bool {
    let mut list = items(st);
    let added = match list.iter().position(|i| i.id == id) {
        Some(at) => {
            list.remove(at);
            false
        }
        None => {
            list.push(QatItem::command(id));
            true
        }
    };
    save(st, &list);
    let label = folitecta_core::registry()
        .get(id)
        .map(|c| crate::i18n::cmd_label(st.lang(), c))
        .unwrap_or_else(|| id.to_string());
    let key = if added {
        "toast.qat_added"
    } else {
        "toast.qat_removed"
    };
    st.toast(ToastKind::Info, tf(st.lang(), key, &[("command", &label)]), "");
    added
}

/// Add a command after the selected entry (or at the end).
pub fn add(st: AppState, id: &str) {
    let mut list = items(st);
    if list.iter().any(|i| i.id == id) {
        return;
    }
    let selected = *SELECTED.peek();
    let at = selected.map(|i| i + 1).unwrap_or(list.len()).min(list.len());
    list.insert(at, QatItem::command(id));
    save(st, &list);
    *SELECTED.write() = Some(at);
}

/// Change the entries with the selected one (when there is one).
fn with_selected(st: AppState, f: impl FnOnce(&mut Vec<QatItem>, usize) -> Option<usize>) {
    if !matches!(*st.dialog.peek(), Dialog::CustomizeQat) {
        open(st);
        return;
    }
    let mut list = items(st);
    let selected = *SELECTED.peek();
    let Some(i) = selected.filter(|i| *i < list.len()) else {
        st.toast(ToastKind::Info, t(st.lang(), "toast.qat_choose"), "");
        return;
    };
    let next = f(&mut list, i);
    save(st, &list);
    *SELECTED.write() = next;
}

pub fn remove(st: AppState) {
    with_selected(st, |list, i| {
        list.remove(i);
        (!list.is_empty()).then(|| i.min(list.len() - 1))
    });
}

pub fn move_by(st: AppState, delta: isize) {
    with_selected(st, |list, i| {
        let j = i as isize + delta;
        if j < 0 || j as usize >= list.len() {
            return Some(i);
        }
        list.swap(i, j as usize);
        Some(j as usize)
    });
}

/// Put a separator before or after the selected entry.
pub fn separator(st: AppState, after: bool) {
    with_selected(st, |list, i| {
        let at = if after { i + 1 } else { i };
        list.insert(at, QatItem::separator());
        Some(if after { i } else { i + 1 })
    });
}

/// Show or hide the name, or the icon, of the selected entry (one of the
/// two always stays).
pub fn toggle_text(st: AppState, text: bool) {
    with_selected(st, |list, i| {
        let item = &mut list[i];
        if !item.is_separator() {
            if text {
                item.text = !item.text;
                if !item.text {
                    item.icon = true;
                }
            } else {
                item.icon = !item.icon;
                if !item.icon {
                    item.text = true;
                }
            }
        }
        Some(i)
    });
}

pub fn reset(st: AppState) {
    save(st, &defaults());
    *SELECTED.write() = None;
}

fn open(st: AppState) {
    *SELECTED.write() = None;
    st.set_dialog(Dialog::CustomizeQat);
}

fn place(st: AppState, below: bool) {
    st.set_value("qat.position", if below { "below" } else { "above" });
}

/// Check states of the position of the toolbar and of the selected entry.
pub fn qat_state(st: AppState, id: &str) -> Option<bool> {
    match id {
        "QABarAbove" => Some(!below(st)),
        "QABarBelow" => Some(below(st)),
        "item.showText" | "item.showIcon" => {
            let i = (*SELECTED.read())?;
            let item = items(st).get(i).cloned()?;
            Some(if id == "item.showText" {
                item.text
            } else {
                item.icon
            })
        }
        _ => None,
    }
}

handlers! {
    "QABarAbove" => |st| place(st, false),
    "QABarBelow" => |st| place(st, true),
    "customizeToolbars" => open,
    "item.props" => open,
    "customizeToolbars.up" => |st| move_by(st, -1),
    "customizeToolbars.down" => |st| move_by(st, 1),
    "customizeToolbars.delete" => remove,
    "item.delete" => remove,
    "customizeToolbars.reset" => reset,
    "resetToolbar" => reset,
    "item.insertSepBefore" => |st| separator(st, false),
    "item.insertSepAfter" => |st| separator(st, true),
    "item.showText" => |st| toggle_text(st, true),
    "item.showIcon" => |st| toggle_text(st, false),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entries_read_back_with_defaults_for_missing_fields() {
        let list: Vec<QatItem> =
            serde_json::from_str(r#"[{"id":"save","text":true},{"id":"|"},{"id":"print"}]"#).unwrap();
        assert!(list[0].text && list[0].icon);
        assert!(list[1].is_separator());
        assert!(!list[2].text && list[2].icon);
        assert_eq!(defaults().len(), 4);
    }
}
