//! Tables (create, edit cells, rows and columns, merge, colours), field
//! helpers of the form editor (tab and calculation numbers, copies), and
//! opening links in the browser.

use dioxus::prelude::*;
use folitecta_core::tables::Table;

use crate::commands::{handlers, no_doc, option_on, select_tool};
use crate::i18n::{t, tf};
use crate::state::{AppState, Dialog, ToastKind, Tool};

/// The table being edited.
#[derive(Debug, Clone, PartialEq)]
pub struct TableEdit {
    pub page: usize,
    pub index: usize,
    pub table: Table,
    /// Selected cells: first and last row, first and last column.
    pub sel: (usize, usize, usize, usize),
    /// Colour given by "Fill".
    pub fill: [f32; 3],
}

pub static TABLE_EDIT: GlobalSignal<Option<TableEdit>> = Signal::global(|| None);

/// Open the table editor on the table comment `(page, index)`; false when it is not a table.
pub fn edit_table(st: AppState, page: usize, index: usize) -> bool {
    let Some(table) = st.read_active(|t| t.doc.table_at(page, index)).flatten() else {
        return false;
    };
    *TABLE_EDIT.write() = Some(TableEdit {
        page,
        index,
        table,
        sel: (0, 0, 0, 0),
        fill: [0.9, 0.93, 1.0],
    });
    st.set_dialog(Dialog::TableEditor);
    true
}

/// The table editor, opened on the selected table when needed.
fn editing(st: AppState) -> bool {
    if TABLE_EDIT.peek().is_some() && (st.dialog)() == Dialog::TableEditor {
        return true;
    }
    let sel = st.read_active(|t| t.selected_annot).flatten();
    if let Some((p, i)) = sel {
        if edit_table(st, p, i) {
            return true;
        }
    }
    if !no_doc(st) {
        st.toast(ToastKind::Info, t(st.lang(), "toast.select_table"), "");
    }
    false
}

/// Change the table being edited.
fn with_table(st: AppState, f: impl FnOnce(&mut TableEdit) -> folitecta_core::Result<()>) {
    if !editing(st) {
        return;
    }
    let r = match TABLE_EDIT.write().as_mut() {
        Some(e) => f(e),
        None => Ok(()),
    };
    if let Err(e) = r {
        st.toast(ToastKind::Info, e.to_string(), "");
    }
}

/// Selection with `r0 <= r1` and `c0 <= c1`, inside the table.
pub fn norm(e: &TableEdit) -> (usize, usize, usize, usize) {
    let (a, b, c, d) = e.sel;
    let (r0, r1) = (a.min(b), a.max(b).min(e.table.row_count() - 1));
    let (c0, c1) = (c.min(d), c.max(d).min(e.table.col_count() - 1));
    (r0, r1, c0, c1)
}

fn insert_rows(st: AppState, below: bool) {
    with_table(st, |e| {
        let (r0, r1, _, _) = norm(e);
        let at = if below { r1 + 1 } else { r0 };
        e.table.insert_row(at);
        e.sel = (at, at, e.sel.2, e.sel.3);
        Ok(())
    });
}

fn insert_cols(st: AppState, right: bool) {
    with_table(st, |e| {
        let (_, _, c0, c1) = norm(e);
        let at = if right { c1 + 1 } else { c0 };
        e.table.insert_col(at);
        e.sel = (e.sel.0, e.sel.1, at, at);
        Ok(())
    });
}

fn delete_rows(st: AppState) {
    with_table(st, |e| {
        let (r0, r1, _, _) = norm(e);
        e.table.delete_rows(r0, r1)?;
        e.sel = (
            r0.min(e.table.row_count() - 1),
            r0.min(e.table.row_count() - 1),
            e.sel.2,
            e.sel.3,
        );
        Ok(())
    });
}

fn delete_cols(st: AppState) {
    with_table(st, |e| {
        let (_, _, c0, c1) = norm(e);
        e.table.delete_cols(c0, c1)?;
        e.sel = (
            e.sel.0,
            e.sel.1,
            c0.min(e.table.col_count() - 1),
            c0.min(e.table.col_count() - 1),
        );
        Ok(())
    });
}

fn merge(st: AppState) {
    with_table(st, |e| {
        let (r0, r1, c0, c1) = norm(e);
        e.table.merge(r0, c0, r1, c1)?;
        e.sel = (r0, r0, c0, c0);
        Ok(())
    });
}

fn split(st: AppState) {
    with_table(st, |e| {
        let (r0, _, c0, _) = norm(e);
        if !e.table.split(r0, c0)? {
            return Err(folitecta_core::Error::Invalid(t(
                st.lang(),
                "toast.cell_not_merged",
            )));
        }
        Ok(())
    });
}

/// Fill the selected cells with the editor's colour (again: remove the fill).
fn fill(st: AppState) {
    with_table(st, |e| {
        let (r0, r1, c0, c1) = norm(e);
        let all_filled = (r0..=r1).all(|r| (c0..=c1).all(|c| e.table.cells[r][c].fill == Some(e.fill)));
        for r in r0..=r1 {
            for c in c0..=c1 {
                e.table.cells[r][c].fill = if all_filled { None } else { Some(e.fill) };
            }
        }
        Ok(())
    });
}

fn line_color(st: AppState) {
    if !editing(st) {
        return;
    }
    spawn(async move {
        let _ = document::eval(
            "for (let i = 0; i < 20 && !document.getElementById('table-line-color'); i++) await new Promise(r => setTimeout(r, 30)); document.getElementById('table-line-color')?.click();",
        )
        .await;
    });
}

fn delete_table(st: AppState) {
    let target = TABLE_EDIT
        .peek()
        .as_ref()
        .map(|e| (e.page, e.index))
        .or_else(|| st.read_active(|t| t.selected_annot).flatten());
    let Some((p, i)) = target else {
        st.toast(ToastKind::Info, t(st.lang(), "toast.select_table"), "");
        return;
    };
    if st.read_active(|t| t.doc.table_at(p, i).is_none()).unwrap_or(true) {
        st.toast(ToastKind::Info, t(st.lang(), "toast.select_table"), "");
        return;
    }
    let r = st.with_active(|t| {
        t.selected_annot = None;
        t.doc.delete_annotation(p, i)
    });
    *TABLE_EDIT.write() = None;
    if (st.dialog)() == Dialog::TableEditor {
        st.set_dialog(Dialog::None);
    }
    crate::commands::core_result(st, r);
}

/// Write the edited table into the document.
pub fn apply(st: AppState) {
    let Some(e) = TABLE_EDIT.peek().clone() else {
        return;
    };
    let r = st.with_active(|t| t.doc.set_table(e.page, e.index, &e.table));
    crate::commands::core_result(st, r);
}

fn toggle(st: AppState, key: &str) {
    let on = !option_on(st, key);
    st.set_option(key, on);
}

fn field_copies(st: AppState) {
    if no_doc(st) {
        return;
    }
    let field = st
        .read_active(|t| {
            let (p, i) = t.selected_annot?;
            let wid = t.doc.annotations(p).ok()?.get(i)?.id?;
            t.doc
                .form_fields()
                .into_iter()
                .find(|f| f.widgets.contains(&wid))
                .map(|f| f.id)
        })
        .flatten();
    match field {
        Some(id) => st.set_dialog(Dialog::FieldCopies(id)),
        None => st.toast(ToastKind::Info, t(st.lang(), "toast.select_field_first"), ""),
    }
}

/// Open the web address of the selected link in the browser.
fn open_link(st: AppState) {
    if no_doc(st) {
        return;
    }
    let uri = st
        .read_active(|t| {
            let (p, i) = t.selected_annot?;
            t.doc
                .links(p)
                .ok()?
                .into_iter()
                .find(|l| l.index == i)
                .and_then(|l| match l.target {
                    folitecta_core::LinkTarget::Uri(u) => Some(u),
                    _ => None,
                })
        })
        .flatten();
    match uri {
        Some(u) => {
            if let Err(e) = crate::util::open_external(&u) {
                st.error(e);
            }
        }
        None => st.toast(ToastKind::Info, t(st.lang(), "toast.select_web_link"), ""),
    }
}

/// Check states of the table and form-editor options.
pub fn tables_state(st: AppState, id: &str) -> Option<bool> {
    match id {
        "tool.editFields.showTabNumbers" | "tool.editFields.showCalcOrdIdx" => Some(option_on(st, id)),
        "tool.table.create" => Some((st.tool)() == Tool::Table),
        _ => None,
    }
}

/// Message for a finished copy of fields.
pub fn copies_done(st: AppState, n: usize) {
    st.toast(
        ToastKind::Ok,
        tf(st.lang(), "toast.fields_copied", &[("count", &n.to_string())]),
        "",
    );
}

handlers! {
    "tool.table.create" => |st| select_tool(st, Tool::Table),
    "tool.table.cell.fill" => fill,
    "tool.table.cell.merge" => merge,
    "tool.table.cell.split" => split,
    "tool.table.delete.columns" => delete_cols,
    "tool.table.delete.rows" => delete_rows,
    "tool.table.delete.table" => delete_table,
    "tool.table.insert.columns.left" => |st| insert_cols(st, false),
    "tool.table.insert.columns.right" => |st| insert_cols(st, true),
    "tool.table.insert.rows.above" => |st| insert_rows(st, false),
    "tool.table.insert.rows.below" => |st| insert_rows(st, true),
    "tool.table.line.color" => line_color,
    "tool.editFields.showTabNumbers" => |st| toggle(st, "tool.editFields.showTabNumbers"),
    "tool.editFields.showCalcOrdIdx" => |st| toggle(st, "tool.editFields.showCalcOrdIdx"),
    "tool.editFields.createMulti" => field_copies,
    "tool.link.openInBrowser.newTab" => open_link,
    "tool.link.openInBrowser.newWindow" => open_link,
}
