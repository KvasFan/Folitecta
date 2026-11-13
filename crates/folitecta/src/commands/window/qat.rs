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

/// A toolbar (classic interface) or a ribbon tab of one's own, made in
/// Customize Toolbars: its name and its entries.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct UserBar {
    pub name: String,
    /// A tab of the ribbon (else a toolbar of the classic interface).
    pub tab: bool,
    /// A toolbar is shown (tabs are always there).
    pub shown: bool,
    pub items: Vec<QatItem>,
}

pub fn user_bars(st: AppState) -> Vec<UserBar> {
    serde_json::from_str(&st.value("user.bars")).unwrap_or_default()
}

pub fn save_user_bars(st: AppState, bars: &[UserBar]) {
    st.set_value("user.bars", serde_json::to_string(bars).unwrap_or_default());
}

/// What Customize Toolbars changes: the Quick Access Toolbar (None) or a
/// toolbar or tab of one's own.
pub static TARGET: GlobalSignal<Option<usize>> = Signal::global(|| None);

/// The entries of what Customize Toolbars changes.
pub fn target_items(st: AppState) -> Vec<QatItem> {
    match *TARGET.peek() {
        Some(i) => user_bars(st).get(i).map(|b| b.items.clone()).unwrap_or_default(),
        None => items(st),
    }
}

fn target_save(st: AppState, list: &[QatItem]) {
    match *TARGET.peek() {
        Some(i) => {
            let mut bars = user_bars(st);
            if let Some(b) = bars.get_mut(i) {
                b.items = list.to_vec();
                save_user_bars(st, &bars);
            }
        }
        None => save(st, list),
    }
}

/// Toolbars and panes cannot be changed (View › Lock All…).
pub fn bars_locked(st: AppState) -> bool {
    st.value("ui.lock_bars") == "1"
}

pub fn panes_locked(st: AppState) -> bool {
    st.value("ui.lock_panes") == "1"
}

/// Say that the toolbars are locked (and change nothing).
pub fn refused_locked(st: AppState) -> bool {
    let locked = bars_locked(st);
    if locked {
        st.toast(ToastKind::Info, t(st.lang(), "toast.bars_locked"), "");
    }
    locked
}

/// Add a command at the end, or take it away when it is there already
/// (a right click on a tool). Returns whether it was added.
pub fn toggle(st: AppState, id: &str) -> bool {
    if refused_locked(st) {
        return false;
    }
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
    if refused_locked(st) {
        return;
    }
    let mut list = target_items(st);
    if list.iter().any(|i| i.id == id) {
        return;
    }
    let selected = *SELECTED.peek();
    let at = selected.map(|i| i + 1).unwrap_or(list.len()).min(list.len());
    list.insert(at, QatItem::command(id));
    target_save(st, &list);
    *SELECTED.write() = Some(at);
}

/// Change the entries with the selected one (when there is one).
fn with_selected(st: AppState, f: impl FnOnce(&mut Vec<QatItem>, usize) -> Option<usize>) {
    if !matches!(*st.dialog.peek(), Dialog::CustomizeQat) {
        open(st);
        return;
    }
    if refused_locked(st) {
        return;
    }
    let mut list = target_items(st);
    let selected = *SELECTED.peek();
    let Some(i) = selected.filter(|i| *i < list.len()) else {
        st.toast(ToastKind::Info, t(st.lang(), "toast.qat_choose"), "");
        return;
    };
    let next = f(&mut list, i);
    target_save(st, &list);
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

/// The Quick Access Toolbar as installed; a toolbar or tab of one's own
/// emptied.
pub fn reset(st: AppState) {
    if refused_locked(st) {
        return;
    }
    if TARGET.peek().is_some() {
        target_save(st, &[]);
    } else {
        save(st, &defaults());
    }
    *SELECTED.write() = None;
}

fn open(st: AppState) {
    *SELECTED.write() = None;
    st.set_dialog(Dialog::CustomizeQat);
}

/// A new toolbar (or ribbon tab) of one's own, chosen in Customize Toolbars.
pub fn new_bar(st: AppState, tab: bool) {
    if refused_locked(st) {
        return;
    }
    let mut bars = user_bars(st);
    let base = t(st.lang(), if tab { "qat.new_tab" } else { "qat.new_toolbar" });
    let name = (1..)
        .map(|n| format!("{base} {n}"))
        .find(|n| !bars.iter().any(|b| b.name == *n))
        .unwrap_or_default();
    bars.push(UserBar {
        name,
        tab,
        shown: true,
        items: Vec::new(),
    });
    save_user_bars(st, &bars);
    *TARGET.write() = Some(bars.len() - 1);
    open(st);
    if tab {
        st.set_ribbon_tab(format!("user:{}", bars.len() - 1));
    }
}

/// The toolbar or tab of one's own chosen in Customize Toolbars; says so
/// when the Quick Access Toolbar is.
fn own_target(st: AppState) -> Option<usize> {
    if !matches!(*st.dialog.peek(), Dialog::CustomizeQat) {
        open(st);
    }
    let target = (*TARGET.peek()).filter(|i| *i < user_bars(st).len());
    if target.is_none() {
        st.toast(ToastKind::Info, t(st.lang(), "toast.qat_own_first"), "");
    }
    target
}

fn rename(st: AppState) {
    if own_target(st).is_some() {
        let _ = dioxus::document::eval(
            "setTimeout(() => { const i = document.querySelector('.dialog .qat-bar-name'); if (i) { i.focus(); i.select(); } }, 50);",
        );
    }
}

/// Rename the toolbar or tab of one's own chosen.
pub fn set_name(st: AppState, name: &str) {
    let Some(i) = *TARGET.peek() else { return };
    let mut bars = user_bars(st);
    let name = name.trim();
    if name.is_empty() || refused_locked(st) {
        return;
    }
    if let Some(b) = bars.get_mut(i) {
        b.name = name.to_string();
        save_user_bars(st, &bars);
    }
}

/// Delete the toolbar or tab of one's own chosen.
pub fn delete_bar(st: AppState) {
    let Some(i) = own_target(st) else { return };
    if refused_locked(st) {
        return;
    }
    let mut bars = user_bars(st);
    bars.remove(i);
    save_user_bars(st, &bars);
    *TARGET.write() = None;
    *SELECTED.write() = None;
    if st.value("ribbon.default_tab") == format!("user:{i}") {
        st.set_value("ribbon.default_tab", "");
    }
    if st.ribbon_tab.peek().starts_with("user:") {
        st.set_ribbon_tab("home".into());
    }
}

/// Make Default: a tab of one's own opens with the window; a toolbar of
/// one's own is shown.
fn make_default(st: AppState) {
    let Some(i) = own_target(st) else { return };
    let mut bars = user_bars(st);
    let Some(b) = bars.get_mut(i) else { return };
    if b.tab {
        st.set_value("ribbon.default_tab", format!("user:{i}"));
    } else {
        b.shown = true;
        save_user_bars(st, &bars);
    }
    st.toast(ToastKind::Ok, t(st.lang(), "toast.qat_default"), "");
}

/// Lock (or unlock) the toolbars or the panes.
fn lock(st: AppState, key: &str, on: Option<bool>) {
    let now = st.value(key) == "1";
    let next = on.unwrap_or(!now);
    st.set_value(key, if next { "1" } else { "" });
}

fn place(st: AppState, below: bool) {
    st.set_value("qat.position", if below { "below" } else { "above" });
}

/// Check states of the position of the toolbar and of the selected entry.
pub fn qat_state(st: AppState, id: &str) -> Option<bool> {
    match id {
        "QABarAbove" => Some(!below(st)),
        "QABarBelow" => Some(below(st)),
        "view.lockAllCmdBars" => Some(bars_locked(st)),
        "view.unlockAllCmdBars" => Some(!bars_locked(st)),
        "view.lockAllCmdPanes" => Some(panes_locked(st)),
        "view.unlockAllCmdPanes" => Some(!panes_locked(st)),
        "item.showText" | "item.showIcon" => {
            let i = (*SELECTED.read())?;
            let item = target_items(st).get(i).cloned()?;
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
    "customizeToolbars.new" => |st| { let tab = st.ui.peek().ribbon; new_bar(st, tab) },
    "customizeToolbars.newToolbar" => |st| new_bar(st, false),
    "customizeToolbars.newTab" => |st| new_bar(st, true),
    "customizeToolbars.rename" => rename,
    "renameToolbar" => rename,
    "customizeToolbars.makeDefault" => make_default,
    // The menu bar shown again, with its menus as installed.
    "resetMenu" => |st| { st.set_value("menubar.hidden", ""); st.toast(ToastKind::Ok, t(st.lang(), "toast.menu_reset"), "") },
    "view.lockAllCmdBars" => |st| lock(st, "ui.lock_bars", None),
    "view.unlockAllCmdBars" => |st| lock(st, "ui.lock_bars", None),
    "view.lockUnlockAllCmdBars" => |st| lock(st, "ui.lock_bars", None),
    "view.lockAllCmdPanes" => |st| lock(st, "ui.lock_panes", None),
    "view.unlockAllCmdPanes" => |st| lock(st, "ui.lock_panes", None),
    "view.lockUnlockAllCmdPanes" => |st| lock(st, "ui.lock_panes", None),
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
