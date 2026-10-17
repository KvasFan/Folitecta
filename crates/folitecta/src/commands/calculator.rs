//! Accounting calculator: tapes placed on the page as comments, edited in a
//! side pane with a keypad, and the commands of the calculator ribbon tab.

use std::collections::BTreeSet;

use dioxus::prelude::*;
use folitecta_core::calculator::{Rounding, Tape, TapeLine, TapeStyle};

use super::{handlers, no_doc, select_tool};
use crate::i18n::t;
use crate::state::{AppState, Dialog, ToastKind, Tool};

/// The tape being edited.
#[derive(Debug, Clone, PartialEq)]
pub struct CalcEdit {
    /// The document tab of the tape.
    pub tab: u64,
    pub page: usize,
    pub index: usize,
    pub tape: Tape,
    /// Selected lines.
    pub selected: BTreeSet<usize>,
    /// Earlier and later states of the tape (undo and redo of the pane).
    pub undo: Vec<Tape>,
    pub redo: Vec<Tape>,
    /// The amount being typed.
    pub entry: String,
}

pub static CALC_EDIT: GlobalSignal<Option<CalcEdit>> = Signal::global(|| None);

pub const CALC_TAB: &str = "ctx.calculator";

/// Settings of new tapes (decimals, rounding, date, look).
pub fn defaults(st: AppState) -> Tape {
    serde_json::from_str::<Tape>(&st.value("calc.defaults")).unwrap_or_default()
}

pub fn save_defaults(st: AppState, tape: &Tape) {
    let mut d = tape.clone();
    d.lines.clear();
    d.date.clear();
    st.set_value("calc.defaults", serde_json::to_string(&d).unwrap_or_default());
}

/// Open the calculator pane on the comment `(page, index)`; false when it is not a calculator.
pub fn edit_calculator(st: AppState, page: usize, index: usize) -> bool {
    let Some(tape) = st.read_active(|t| t.doc.calculator_at(page, index)).flatten() else {
        return false;
    };
    let tab = st.read_active(|t| t.id).unwrap_or(0);
    *CALC_EDIT.write() = Some(CalcEdit {
        tab,
        page,
        index,
        tape,
        selected: BTreeSet::new(),
        undo: Vec::new(),
        redo: Vec::new(),
        entry: String::new(),
    });
    st.with_active(|t| t.selected_annot = Some((page, index)));
    if st.ui.peek().ribbon {
        st.set_ribbon_tab(CALC_TAB.to_string());
    }
    true
}

pub fn close(st: AppState) {
    *CALC_EDIT.write() = None;
    if st.ribbon_tab.peek().as_str() == CALC_TAB {
        st.set_ribbon_tab("comment".to_string());
    }
}

/// Place a new tape with its top-left corner at `point` (PDF space).
pub fn place(st: AppState, page: usize, point: (f32, f32)) {
    let mut tape = defaults(st);
    tape.date = folitecta_core::util::unix_display(crate::state::now_secs());
    let author = st.ui.peek().author.clone();
    let r = st.with_active(|t| {
        t.doc.add_calculator(
            page,
            [point.0, point.1 - 20.0, point.0 + 170.0, point.1],
            &tape,
            &author,
        )
    });
    match r {
        Some(Ok(_)) => {
            crate::files::set_window_title(st);
            st.set_tool(Tool::Hand);
            let index = st
                .read_active(|t| t.doc.annotations(page).map(|a| a.len()).unwrap_or(1))
                .unwrap_or(1)
                .saturating_sub(1);
            edit_calculator(st, page, index);
        }
        Some(Err(e)) => st.error(e),
        None => {}
    }
}

/// Write the tape being edited into its comment (the pane closes when its
/// document is no longer the active one).
fn write(st: AppState, e: &CalcEdit) {
    if st.read_active(|t| t.id) != Some(e.tab) {
        close(st);
        return;
    }
    let r = st.with_active(|t| t.doc.set_calculator(e.page, e.index, &e.tape));
    super::core_result(st, r);
}

/// Change the tape (one step of the pane's undo) and write it.
pub fn change(st: AppState, f: impl FnOnce(&mut CalcEdit)) {
    let snapshot = {
        let mut w = CALC_EDIT.write();
        let Some(e) = w.as_mut() else { return };
        let before = e.tape.clone();
        f(e);
        if e.tape == before {
            return;
        }
        e.undo.push(before);
        if e.undo.len() > 200 {
            e.undo.remove(0);
        }
        e.redo.clear();
        e.clone()
    };
    write(st, &snapshot);
}

/// Change only what the pane shows (selection, entry): no undo step.
pub fn view(f: impl FnOnce(&mut CalcEdit)) {
    if let Some(e) = CALC_EDIT.write().as_mut() {
        f(e);
    }
}

fn editing(st: AppState) -> bool {
    if CALC_EDIT.peek().is_some() {
        return true;
    }
    if let Some((p, i)) = st.read_active(|t| t.selected_annot).flatten() {
        if edit_calculator(st, p, i) {
            return true;
        }
    }
    if !no_doc(st) {
        st.toast(ToastKind::Info, t(st.lang(), "toast.select_calculator"), "");
    }
    false
}

/// Parse a typed amount (comma or point as the decimal separator, spaces and
/// thousands separators ignored).
pub fn parse_amount(s: &str) -> Option<f64> {
    let s: String = s.chars().filter(|c| !c.is_whitespace() && *c != '\'').collect();
    if s.is_empty() {
        return None;
    }
    let s = if s.contains(',') && s.contains('.') {
        // The last separator is the decimal one.
        if s.rfind(',') > s.rfind('.') {
            s.replace('.', "").replace(',', ".")
        } else {
            s.replace(',', "")
        }
    } else {
        s.replace(',', ".")
    };
    s.parse::<f64>().ok().filter(|v| v.is_finite())
}

/// Enter the typed amount with an operation (after the selected lines, or at the end).
pub fn enter(st: AppState, op: char) {
    let entry = CALC_EDIT
        .peek()
        .as_ref()
        .map(|e| e.entry.clone())
        .unwrap_or_default();
    let amount = parse_amount(&entry);
    if amount.is_none() && op != '=' {
        if !entry.trim().is_empty() {
            st.toast(ToastKind::Info, t(st.lang(), "toast.calc_bad_amount"), "");
        }
        return;
    }
    change(st, |e| {
        let at = e
            .selected
            .iter()
            .next_back()
            .map(|i| i + 1)
            .unwrap_or(e.tape.lines.len());
        let at = at.min(e.tape.lines.len());
        e.tape.lines.insert(at, TapeLine::new(op, amount.unwrap_or(0.0)));
        e.selected = if at + 1 < e.tape.lines.len() {
            BTreeSet::from([at])
        } else {
            BTreeSet::new()
        };
        e.entry.clear();
    });
}

fn add_line(st: AppState) {
    if !editing(st) {
        return;
    }
    change(st, |e| {
        let at = e
            .selected
            .iter()
            .next_back()
            .map(|i| i + 1)
            .unwrap_or(e.tape.lines.len());
        e.tape.lines.insert(at, TapeLine::new('+', 0.0));
        e.selected = BTreeSet::from([at]);
    });
    focus(st, "amount");
}

fn del_lines(st: AppState) {
    if !editing(st) {
        return;
    }
    if CALC_EDIT.peek().as_ref().is_some_and(|e| e.selected.is_empty()) {
        st.toast(ToastKind::Info, t(st.lang(), "toast.select_lines"), "");
        return;
    }
    change(st, |e| {
        let sel = std::mem::take(&mut e.selected);
        let mut i = 0;
        e.tape.lines.retain(|_| {
            let keep = !sel.contains(&i);
            i += 1;
            keep
        });
    });
}

fn clear_tape(st: AppState) {
    if editing(st) {
        change(st, |e| {
            e.tape.lines.clear();
            e.selected.clear();
        });
    }
}

fn reset_tape(st: AppState) {
    if editing(st) {
        let d = defaults(st);
        change(st, |e| {
            let date = e.tape.date.clone();
            e.tape = d;
            e.tape.date = date;
            e.selected.clear();
        });
    }
}

fn undo(st: AppState) {
    if !editing(st) {
        return;
    }
    let snapshot = {
        let mut w = CALC_EDIT.write();
        let Some(e) = w.as_mut() else { return };
        let Some(prev) = e.undo.pop() else { return };
        let cur = std::mem::replace(&mut e.tape, prev);
        e.redo.push(cur);
        e.selected.retain(|i| *i < e.tape.lines.len());
        e.clone()
    };
    write(st, &snapshot);
}

fn redo(st: AppState) {
    if !editing(st) {
        return;
    }
    let snapshot = {
        let mut w = CALC_EDIT.write();
        let Some(e) = w.as_mut() else { return };
        let Some(next) = e.redo.pop() else { return };
        let cur = std::mem::replace(&mut e.tape, next);
        e.undo.push(cur);
        e.selected.retain(|i| *i < e.tape.lines.len());
        e.clone()
    };
    write(st, &snapshot);
}

fn set_decimals(st: AppState, d: Option<u8>) {
    if editing(st) {
        change(st, |e| e.tape.decimals = d);
    }
}

fn set_rounding(st: AppState, r: Rounding) {
    if editing(st) {
        change(st, |e| e.tape.rounding = r);
    }
}

fn set_date(st: AppState, show: bool) {
    if editing(st) {
        change(st, |e| {
            e.tape.show_date = show;
            if show && e.tape.date.is_empty() {
                e.tape.date = folitecta_core::util::unix_display(crate::state::now_secs());
            }
        });
    }
}

fn select(st: AppState, how: &str) {
    if !editing(st) {
        return;
    }
    view(|e| {
        let n = e.tape.lines.len();
        e.selected = match how {
            "all" => (0..n).collect(),
            "none" => BTreeSet::new(),
            _ => (0..n).filter(|i| !e.selected.contains(i)).collect(),
        };
    });
}

/// Focus a field of the pane: `amount`, `comment` or `op` of the first
/// selected line, or one of the look settings.
pub fn focus(st: AppState, what: &str) {
    if !editing(st) {
        return;
    }
    let line = CALC_EDIT
        .peek()
        .as_ref()
        .and_then(|e| e.selected.iter().next().copied());
    let id = match what {
        "amount" | "comment" | "op" => match line {
            Some(i) => format!("calc-{what}-{i}"),
            None => {
                st.toast(ToastKind::Info, t(st.lang(), "toast.select_lines"), "");
                return;
            }
        },
        other => format!("calc-{other}"),
    };
    let click = matches!(what, "bg" | "bc");
    let js = format!(
        "for (let i = 0; i < 20 && !document.getElementById('{id}'); i++) await new Promise(r => setTimeout(r, 30)); const el = document.getElementById('{id}'); if (el) {{ el.focus(); {} }}",
        if click {
            "el.click();"
        } else {
            "if (el.select) el.select();"
        }
    );
    spawn(async move {
        let _ = document::eval(&js).await;
    });
}

fn reset_look(st: AppState) {
    if editing(st) {
        change(st, |e| e.tape.style = TapeStyle::default());
    }
}

/// Check states of the calculator commands.
pub fn calc_state(st: AppState, id: &str) -> Option<bool> {
    let e = CALC_EDIT.read();
    let tape = e.as_ref().map(|e| &e.tape);
    match id {
        "comments.accountingCalc" => Some((st.tool)() == Tool::Calculator),
        "ac.dateTime.show" => Some(tape.is_some_and(|t| t.show_date)),
        "ac.dateTime.hide" => Some(tape.is_some_and(|t| !t.show_date)),
        "ac.decimal.f" => Some(tape.is_some_and(|t| t.decimals.is_none())),
        "ac.rounding.default" => Some(tape.is_some_and(|t| t.rounding == Rounding::Default)),
        "ac.rounding.up" => Some(tape.is_some_and(|t| t.rounding == Rounding::Up)),
        "ac.rounding.down" => Some(tape.is_some_and(|t| t.rounding == Rounding::Down)),
        _ => DECIMALS
            .iter()
            .find(|(cmd, _)| *cmd == id)
            .map(|(_, d)| tape.is_some_and(|t| t.decimals == Some(*d))),
    }
}

const DECIMALS: &[(&str, u8)] = &[
    ("ac.decimal.zero", 0),
    ("ac.decimal.one", 1),
    ("ac.decimal.two", 2),
    ("ac.decimal.three", 3),
    ("ac.decimal.four", 4),
    ("ac.decimal.five", 5),
    ("ac.decimal.six", 6),
];

handlers! {
    "comments.accountingCalc" => |st| select_tool(st, Tool::Calculator),
    "comments.ac.closeAll" => close,
    "ac.addLine" => add_line,
    "ac.delLine" => del_lines,
    "ac.clearTape" => clear_tape,
    "ac.tape.reset" => reset_tape,
    "ac.undo" => undo,
    "ac.redo" => redo,
    "ac.dateTime.show" => |st| set_date(st, true),
    "ac.dateTime.hide" => |st| set_date(st, false),
    "ac.decimal.f" => |st| set_decimals(st, None),
    "ac.decimal.zero" => |st| set_decimals(st, Some(0)),
    "ac.decimal.one" => |st| set_decimals(st, Some(1)),
    "ac.decimal.two" => |st| set_decimals(st, Some(2)),
    "ac.decimal.three" => |st| set_decimals(st, Some(3)),
    "ac.decimal.four" => |st| set_decimals(st, Some(4)),
    "ac.decimal.five" => |st| set_decimals(st, Some(5)),
    "ac.decimal.six" => |st| set_decimals(st, Some(6)),
    "ac.rounding.default" => |st| set_rounding(st, Rounding::Default),
    "ac.rounding.up" => |st| set_rounding(st, Rounding::Up),
    "ac.rounding.down" => |st| set_rounding(st, Rounding::Down),
    "ac.list.selectAll" => |st| select(st, "all"),
    "ac.list.unselect" => |st| select(st, "none"),
    "ac.list.invertSel" => |st| select(st, "invert"),
    "ac.list.edit.amount" => |st| focus(st, "amount"),
    "ac.list.edit.comment" => |st| focus(st, "comment"),
    "ac.list.edit.oper" => |st| focus(st, "op"),
    "ac.preview.background" => |st| focus(st, "bg"),
    "ac.preview.borderColor" => |st| focus(st, "bc"),
    "ac.preview.borderStyle" => |st| focus(st, "bs"),
    "ac.preview.borderWidth" => |st| focus(st, "bw"),
    "ac.preview.font" => |st| focus(st, "fs"),
    "ac.preview.reset" => reset_look,
    "ac.settings" => |st| st.set_dialog(Dialog::CalcSettings),
}

#[cfg(test)]
mod tests {
    use super::parse_amount;

    #[test]
    fn amounts_with_either_separator() {
        assert_eq!(parse_amount("12,5"), Some(12.5));
        assert_eq!(parse_amount("1.234,56"), Some(1234.56));
        assert_eq!(parse_amount("1,234.56"), Some(1234.56));
        assert_eq!(parse_amount(" 7 "), Some(7.0));
        assert_eq!(parse_amount("abc"), None);
        assert_eq!(parse_amount(""), None);
    }
}
