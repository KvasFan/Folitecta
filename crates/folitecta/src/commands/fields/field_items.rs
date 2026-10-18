//! The items of a combo box or list box, edited as a list: added, deleted,
//! moved up and down, kept sorted, and written to the field in one undo
//! step. The list lives here, so the commands of the menus work on the
//! dialog that shows it.

use dioxus::prelude::*;
use folitecta_core::{FieldKind, FieldProps};

use crate::commands::handlers;
use crate::files;
use crate::i18n::t;
use crate::state::{AppState, Dialog, ToastKind};

/// The list being edited.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ItemsEdit {
    pub field: (u32, u16),
    /// The widget chosen (page and index), chosen again after the change.
    pub page: usize,
    pub index: usize,
    pub items: Vec<String>,
    pub selected: Option<usize>,
    pub sorted: bool,
    /// The text of the item to add.
    pub draft: String,
}

pub static ITEMS: GlobalSignal<Option<ItemsEdit>> = Signal::global(|| None);

/// The combo box or list box of the chosen widget, with its properties.
fn chosen_choice(st: AppState) -> Option<(usize, usize, (u32, u16), FieldProps)> {
    st.read_active(|t| {
        let (p, i) = t.selected_annot?;
        let a = t
            .doc
            .annotations(p)
            .ok()?
            .into_iter()
            .find(|a| a.index == i && a.subtype == "Widget")?;
        let wid = a.id?;
        let f = t
            .doc
            .form_fields()
            .into_iter()
            .find(|f| t.doc.field_widgets(f.id).contains(&wid))?;
        if !matches!(f.kind, FieldKind::ComboBox | FieldKind::ListBox) {
            return None;
        }
        Some((p, i, f.id, t.doc.field_props(f.id).ok()?))
    })
    .flatten()
}

/// Open the list of items of the chosen combo box or list box.
pub fn open(st: AppState) {
    match chosen_choice(st) {
        Some((page, index, field, props)) => {
            *ITEMS.write() = Some(ItemsEdit {
                field,
                page,
                index,
                items: props.options,
                selected: None,
                sorted: props.sorted,
                draft: String::new(),
            });
            st.set_dialog(Dialog::FieldItems);
        }
        None => st.toast(ToastKind::Info, t(st.lang(), "toast.choose_choice_field"), ""),
    }
}

/// Change the list (opening it first when it is not shown).
fn edit(st: AppState, f: impl FnOnce(&mut ItemsEdit)) {
    if ITEMS.peek().is_none() || !matches!(*st.dialog.peek(), Dialog::FieldItems) {
        open(st);
        return;
    }
    if let Some(e) = ITEMS.write().as_mut() {
        f(e);
        if e.sorted {
            let chosen = e.selected.and_then(|i| e.items.get(i).cloned());
            e.items.sort_by_key(|o| o.to_lowercase());
            e.selected = chosen.and_then(|c| e.items.iter().position(|o| *o == c));
        }
    }
}

/// Add the item typed, after the one selected (or at the end).
pub fn add(st: AppState) {
    edit(st, |e| {
        let item = e.draft.trim().to_string();
        if item.is_empty() || e.items.contains(&item) {
            return;
        }
        let at = e
            .selected
            .map(|i| i + 1)
            .unwrap_or(e.items.len())
            .min(e.items.len());
        e.items.insert(at, item);
        e.selected = Some(at);
        e.draft.clear();
    });
}

pub fn delete(st: AppState) {
    edit(st, |e| {
        if let Some(i) = e.selected.filter(|i| *i < e.items.len()) {
            e.items.remove(i);
            e.selected = if e.items.is_empty() {
                None
            } else {
                Some(i.min(e.items.len() - 1))
            };
        }
    });
}

/// Move the selected item up (`-1`) or down (`1`); a sorted list keeps its order.
pub fn move_by(st: AppState, delta: isize) {
    edit(st, |e| {
        if e.sorted {
            return;
        }
        let Some(i) = e.selected else { return };
        let j = i as isize + delta;
        if j < 0 || j as usize >= e.items.len() {
            return;
        }
        e.items.swap(i, j as usize);
        e.selected = Some(j as usize);
    });
}

pub fn toggle_sorted(st: AppState) {
    edit(st, |e| e.sorted = !e.sorted);
}

/// Write the list to the field (one undo step) and close the dialog.
pub fn apply(st: AppState) {
    let Some(e) = ITEMS.peek().clone() else {
        return;
    };
    let r = st.with_active(|t| {
        let mut props = t.doc.field_props(e.field)?;
        props.options = e.items.clone();
        props.sorted = e.sorted;
        t.doc.set_field_props(e.field, &props)
    });
    // The edit clears the selection: the widget is chosen again.
    st.with_active(|t| t.selected_annot = Some((e.page, e.index)));
    match r {
        Some(Ok(())) => {
            files::set_window_title(st);
            st.set_dialog(Dialog::None);
            *ITEMS.write() = None;
        }
        Some(Err(err)) => st.error(err),
        None => {}
    }
}

/// Show the properties of the chosen field (where its font size is).
fn font_size(st: AppState) {
    let chosen = st
        .read_active(|t| {
            let (p, i) = t.selected_annot?;
            t.doc
                .annotations(p)
                .ok()?
                .into_iter()
                .find(|a| a.index == i && a.subtype == "Widget")
        })
        .flatten();
    if chosen.is_some() {
        st.ui_mut().show_right_pane = true;
    } else {
        st.toast(ToastKind::Info, t(st.lang(), "toast.choose_field"), "");
    }
}

handlers! {
    "combobox.items" => open,
    "listbox.items" => open,
    "editFields.items" => open,
    "editFields.items.add" => add,
    "editFields.items.delete" => delete,
    "editFields.items.moveUp" => |st| move_by(st, -1),
    "editFields.items.moveDown" => |st| move_by(st, 1),
    "editFields.items.sorted" => toggle_sorted,
    "widget.fontSize" => font_size,
}
