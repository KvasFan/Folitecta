//! Tools › Allow/Block List: the web sites, patterns of hosts, files and
//! folders whose links open without asking, or never; Add Host, Add
//! Pattern, Add File and Add Folder open the list on that kind.

use crate::commands::handlers;
use crate::commands::navigation::follow::{ListEntry, list_entries, save_list};
use crate::state::{AppState, Dialog};

/// Add an entry (an entry for the same thing is replaced).
pub fn add(st: AppState, kind: &str, value: &str, allow: bool) {
    let value = value.trim();
    if value.is_empty() {
        return;
    }
    let mut list = list_entries(st);
    list.retain(|e| !(e.kind == kind && e.value == value));
    list.push(ListEntry {
        kind: kind.to_string(),
        value: value.to_string(),
        allow,
    });
    save_list(st, &list);
}

pub fn remove(st: AppState, index: usize) {
    let mut list = list_entries(st);
    if index < list.len() {
        list.remove(index);
        save_list(st, &list);
    }
}

handlers! {
    "bwList.addHost" => |st| st.set_dialog(Dialog::AllowBlock("host".into())),
    "bwList.addPattern" => |st| st.set_dialog(Dialog::AllowBlock("pattern".into())),
    "bwList.addFile" => |st| st.set_dialog(Dialog::AllowBlock("file".into())),
    "bwList.addFolder" => |st| st.set_dialog(Dialog::AllowBlock("folder".into())),
}
