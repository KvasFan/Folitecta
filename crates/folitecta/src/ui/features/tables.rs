//! Table dialogs (new table, table editor) and copies of form fields.

use dioxus::html::ModifiersInteraction;
use dioxus::prelude::*;
use folitecta_core::registry;
use folitecta_core::tables::Table;

use crate::commands::dispatch;
use crate::commands::tables::{TABLE_EDIT, apply, copies_done, edit_table, norm};
use crate::i18n::{cmd_label, t};
use crate::icons::Icon;
use crate::state::{AppState, Dialog};
use crate::ui::dialogs::Frame;
use crate::util::{color_hex, parse_color};

fn num(v: &str, d: f32) -> f32 {
    v.trim().replace(',', ".").parse().unwrap_or(d)
}

/// Rows and columns of a new table.
#[component]
pub fn TableNewDialog(page: usize, rect: Vec<f32>) -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let mut rows = use_signal(|| "3".to_string());
    let mut cols = use_signal(|| "3".to_string());
    let mut header = use_signal(|| true);
    rsx! {
        Frame { title: t(lang, "table.new"),
            on_ok: move |_| {
                let (r, c) = (rows().trim().parse().unwrap_or(3), cols().trim().parse().unwrap_or(3));
                let mut table = Table::new(r, c);
                if header() {
                    for cell in &mut table.cells[0] {
                        cell.bold = true;
                        cell.fill = Some([0.92, 0.92, 0.92]);
                    }
                }
                let author = st.ui.peek().author.clone();
                let rr = [rect[0], rect[1], rect[2], rect[3]];
                let r = st.with_active(|t| t.doc.add_table(page, rr, &table, &author));
                match r {
                    Some(Ok(_)) => {
                        crate::files::set_window_title(st);
                        st.set_tool(crate::state::Tool::Hand);
                        let index = st.read_active(|t| t.doc.annotations(page).map(|a| a.len()).unwrap_or(1)).unwrap_or(1) - 1;
                        st.with_active(|t| t.selected_annot = Some((page, index)));
                        edit_table(st, page, index);
                    }
                    Some(Err(e)) => st.error(e),
                    None => {}
                }
            },
            div { class: "field-grid",
                div { class: "field", label { "{t(lang, \"table.rows\")}" }
                    input { r#type: "number", min: "1", max: "200", value: "{rows}", autofocus: true, oninput: move |e| rows.set(e.value()) } }
                div { class: "field", label { "{t(lang, \"table.cols\")}" }
                    input { r#type: "number", min: "1", max: "50", value: "{cols}", oninput: move |e| cols.set(e.value()) } }
            }
            label { class: "check", input { r#type: "checkbox", checked: header(), onchange: move |_| header.set(!header()) } span { "{t(lang, \"table.header\")}" } }
        }
    }
}

#[component]
pub fn TableEditorDialog() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let Some(e) = TABLE_EDIT() else {
        return rsx! {};
    };
    let table = e.table.clone();
    let (r0, r1, c0, c1) = norm(&e);
    let grid = format!(
        "grid-template-columns: {};",
        table
            .cols
            .iter()
            .map(|w| format!("minmax(70px, {w}fr)"))
            .collect::<Vec<_>>()
            .join(" ")
    );
    let reg = registry();
    let label = |id: &str| {
        reg.get(id)
            .map(|c| cmd_label(lang, c))
            .unwrap_or_else(|| id.to_string())
    };
    let tools: [(&str, &str); 9] = [
        ("tool.table.insert.rows.above", "up"),
        ("tool.table.insert.rows.below", "down"),
        ("tool.table.insert.columns.left", "prev"),
        ("tool.table.insert.columns.right", "next"),
        ("tool.table.delete.rows", "delete"),
        ("tool.table.delete.columns", "close"),
        ("tool.table.cell.merge", "merge"),
        ("tool.table.cell.split", "split"),
        ("tool.table.cell.fill", "highlight"),
    ];
    let sel_cell = table.cells[r0][c0].clone();
    // The text of the cells drawn, which the table must be able to draw:
    // what it cannot is named while it is typed.
    let cells_text = table
        .layout()
        .iter()
        .map(|(r, c, ..)| table.cells[*r][*c].text.clone())
        .collect::<Vec<_>>()
        .join("\n");
    let problem = folitecta_core::fonts::check_text(&cells_text)
        .err()
        .map(|p| crate::i18n::glyph_problem(lang, &p));
    let edit_cells = move |f: Box<dyn Fn(&mut folitecta_core::tables::TableCell)>| {
        if let Some(ed) = TABLE_EDIT.write().as_mut() {
            let (a, b, c, d) = norm(ed);
            for r in a..=b {
                for col in c..=d {
                    f(&mut ed.table.cells[r][col]);
                }
            }
        }
    };
    rsx! {
        Frame { title: t(lang, "table.editor"), wide: true,
            // A table that cannot be drawn is refused and stays open with what was typed.
            on_ok: move |_| { if apply(st) { *TABLE_EDIT.write() = None; st.set_dialog(Dialog::None); } },
            div { class: "row", style: "gap:4px;flex-wrap:wrap;align-items:center",
                for (id, icon) in tools {
                    button { class: "ibtn", key: "{id}", title: "{label(id)}", onclick: move |_| dispatch(st, id), Icon { name: icon } }
                }
                input { r#type: "color", title: "{label(\"tool.table.cell.fill\")}", value: "{color_hex(e.fill)}",
                    oninput: move |ev| { if let Some(ed) = TABLE_EDIT.write().as_mut() { ed.fill = parse_color(&ev.value()); } } }
                button { class: if sel_cell.bold { "ibtn on" } else { "ibtn" }, title: "{t(lang, \"stamps.bold\")}",
                    onclick: move |_| { let b = !sel_cell.bold; edit_cells(Box::new(move |c| c.bold = b)) }, "B" }
                select { title: "{t(lang, \"stamps.align\")}",
                    onchange: move |ev| { let a = ev.value(); edit_cells(Box::new(move |c| c.align = a.clone())) },
                    for a in ["left", "center", "right"] { option { key: "{a}", value: "{a}", selected: sel_cell.align == a, "{t(lang, &align_key(a))}" } }
                }
                button { class: "ibtn danger", title: "{label(\"tool.table.delete.table\")}", onclick: move |_| dispatch(st, "tool.table.delete.table"), Icon { name: "delete" } }
            }
            div { class: "field-grid four",
                div { class: "field", label { "{t(lang, \"table.line_color\")}" }
                    input { id: "table-line-color", r#type: "color", value: "{color_hex(table.line_color)}",
                        oninput: move |ev| { if let Some(ed) = TABLE_EDIT.write().as_mut() { ed.table.line_color = parse_color(&ev.value()); } } } }
                div { class: "field", label { "{t(lang, \"stamps.line_width\")}" }
                    input { r#type: "number", min: "0", step: "0.25", value: "{table.line_width}",
                        oninput: move |ev| { let v = num(&ev.value(), 0.8).max(0.0); if let Some(ed) = TABLE_EDIT.write().as_mut() { ed.table.line_width = v; } } } }
                div { class: "field", label { "{t(lang, \"stamps.size\")}" }
                    input { r#type: "number", min: "4", max: "48", value: "{table.font_size}",
                        oninput: move |ev| { let v = num(&ev.value(), 10.0).clamp(4.0, 48.0); if let Some(ed) = TABLE_EDIT.write().as_mut() { ed.table.font_size = v; } } } }
                div { class: "field", label { "{t(lang, \"table.text_color\")}" }
                    input { r#type: "color", value: "{color_hex(table.text_color)}",
                        oninput: move |ev| { if let Some(ed) = TABLE_EDIT.write().as_mut() { ed.table.text_color = parse_color(&ev.value()); } } } }
            }
            div { class: "table-grid", style: "{grid}",
                for (r, c, _, _, _, _) in table.layout() {
                    {
                        let cell = table.cells[r][c].clone();
                        let style = format!(
                            "grid-row: {} / span {}; grid-column: {} / span {};{}",
                            r + 1, cell.rowspan, c + 1, cell.colspan,
                            cell.fill.map(|f| format!("background:{};", color_hex(f))).unwrap_or_default()
                        );
                        let selected = r >= r0 && r <= r1 && c >= c0 && c <= c1;
                        let weight = if cell.bold { "font-weight:700;" } else { "" };
                        let align = format!("text-align:{};{weight}", cell.align);
                        rsx! {
                            div { class: if selected { "table-cell selected" } else { "table-cell" }, key: "{r}-{c}", style: "{style}",
                                onmousedown: move |ev| {
                                    let extend = ev.modifiers().shift();
                                    if let Some(ed) = TABLE_EDIT.write().as_mut() {
                                        ed.sel = if extend { (ed.sel.0, r, ed.sel.2, c) } else { (r, r, c, c) };
                                    }
                                },
                                textarea { value: "{cell.text}", style: "{align}", rows: "2",
                                    oninput: move |ev| { if let Some(ed) = TABLE_EDIT.write().as_mut() { ed.table.cells[r][c].text = ev.value(); } } }
                            }
                        }
                    }
                }
            }
            if let Some(p) = problem {
                p { class: "hint warn glyph-hint", "{p}" }
            }
            div { class: "hint", "{t(lang, \"table.hint\")}" }
        }
    }
}

fn align_key(a: &str) -> String {
    format!("stamps.align.{a}")
}

/// Copies of a form field in rows and columns.
#[component]
pub fn FieldCopiesDialog(field: (u32, u16)) -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let size = st
        .read_active(|t| {
            t.doc
                .form_fields()
                .into_iter()
                .find(|f| f.id == field)
                .and_then(|f| f.rect)
        })
        .flatten()
        .map(|r| (r[2] - r[0], r[3] - r[1]))
        .unwrap_or((100.0, 20.0));
    let mut rows = use_signal(|| "3".to_string());
    let mut cols = use_signal(|| "1".to_string());
    let mut dx = use_signal(|| format!("{}", (size.0 + 10.0).round()));
    let mut dy = use_signal(|| format!("{}", (size.1 + 8.0).round()));
    rsx! {
        Frame { title: t(lang, "fieldcopies.title"),
            on_ok: move |_| {
                let (r, c) = (rows().trim().parse().unwrap_or(1), cols().trim().parse().unwrap_or(1));
                let (x, y) = (num(&dx(), 0.0), num(&dy(), 0.0));
                // Rows go down the page.
                let res = st.with_active(|t| t.doc.copy_field(field, r, c, x, -y));
                match res {
                    Some(Ok(n)) => {
                        crate::files::set_window_title(st);
                        st.set_dialog(Dialog::None);
                        copies_done(st, n);
                    }
                    Some(Err(e)) => st.error(e),
                    None => {}
                }
            },
            div { class: "field-grid",
                div { class: "field", label { "{t(lang, \"table.rows\")}" } input { r#type: "number", min: "1", value: "{rows}", oninput: move |e| rows.set(e.value()) } }
                div { class: "field", label { "{t(lang, \"table.cols\")}" } input { r#type: "number", min: "1", value: "{cols}", oninput: move |e| cols.set(e.value()) } }
                div { class: "field", label { "{t(lang, \"fieldcopies.dx\")}" } input { r#type: "number", value: "{dx}", oninput: move |e| dx.set(e.value()) } }
                div { class: "field", label { "{t(lang, \"fieldcopies.dy\")}" } input { r#type: "number", value: "{dy}", oninput: move |e| dy.set(e.value()) } }
            }
            div { class: "hint", "{t(lang, \"fieldcopies.hint\")}" }
        }
    }
}
