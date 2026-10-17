//! Accounting calculator pane (right side) and the settings of new tapes.

use dioxus::html::ModifiersInteraction;
use dioxus::prelude::*;
use folitecta_core::calculator::{OPERATIONS, Rounding, Tape};
use folitecta_core::registry;

use crate::commands::calculator::{
    CALC_EDIT, change, close, defaults, enter, parse_amount, save_defaults, view,
};
use crate::commands::dispatch;
use crate::i18n::{Lang, cmd_label, t};
use crate::icons::Icon;
use crate::state::{AppState, Dialog};
use crate::ui::dialogs::Frame;
use crate::util::{color_hex, parse_color};

fn label_of(lang: Lang, id: &str) -> String {
    registry()
        .get(id)
        .map(|c| cmd_label(lang, c))
        .unwrap_or_else(|| id.to_string())
}

fn op_symbol(op: char) -> &'static str {
    match op {
        '+' => "+",
        '-' => "−",
        '*' => "×",
        '/' => "÷",
        _ => "=",
    }
}

fn decimals_value(d: Option<u8>) -> String {
    d.map(|d| d.to_string()).unwrap_or_else(|| "f".into())
}

fn rounding_value(r: Rounding) -> &'static str {
    match r {
        Rounding::Default => "default",
        Rounding::Up => "up",
        Rounding::Down => "down",
    }
}

fn parse_rounding(v: &str) -> Rounding {
    match v {
        "up" => Rounding::Up,
        "down" => Rounding::Down,
        _ => Rounding::Default,
    }
}

fn num(v: &str, d: f32) -> f32 {
    v.trim().replace(',', ".").parse().unwrap_or(d)
}

/// Decimals, rounding, date and look of a tape (shared by the pane and the settings).
#[component]
fn TapeSettings(tape: Tape, on_change: EventHandler<Tape>, ids: bool) -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let tp = tape.clone();
    let id = |s: &str| {
        if ids {
            format!("calc-{s}")
        } else {
            format!("calcd-{s}")
        }
    };
    let s = tape.style.clone();
    let bg_on = s.background.is_some();
    let decimals = decimals_value(tape.decimals);
    let rounding = rounding_value(tape.rounding);
    rsx! {
        div { class: "field-grid calc-settings",
            div { class: "field", label { "{label_of(lang, \"ac.decimal\")}" }
                select { onchange: { let tp = tp.clone(); move |e: Event<FormData>| { let mut t = tp.clone(); t.decimals = e.value().parse::<u8>().ok(); on_change.call(t); } },
                    for d in ["0", "1", "2", "3", "4", "5", "6", "f"] {
                        option { key: "{d}", value: "{d}", selected: decimals == d,
                            if d == "f" { "{label_of(lang, \"ac.decimal.f\")}" } else { "{d}" } }
                    }
                }
            }
            div { class: "field", label { "{label_of(lang, \"ac.rounding\")}" }
                select { onchange: { let tp = tp.clone(); move |e: Event<FormData>| { let mut t = tp.clone(); t.rounding = parse_rounding(&e.value()); on_change.call(t); } },
                    for (v, cmd) in [("default", "ac.rounding.default"), ("up", "ac.rounding.up"), ("down", "ac.rounding.down")] {
                        option { key: "{v}", value: "{v}", selected: rounding == v, "{label_of(lang, cmd)}" }
                    }
                }
            }
            div { class: "field", label { "{t(lang, \"calc.background\")}" }
                div { class: "row", style: "gap:6px;align-items:center",
                    input { r#type: "checkbox", checked: bg_on, title: "{t(lang, \"calc.background\")}",
                        onchange: { let tp = tp.clone(); move |_| { let mut t = tp.clone(); t.style.background = if bg_on { None } else { Some([1.0, 1.0, 0.94]) }; on_change.call(t); } } }
                    input { id: "{id(\"bg\")}", r#type: "color", disabled: !bg_on, value: "{color_hex(s.background.unwrap_or([1.0, 1.0, 1.0]))}",
                        oninput: { let tp = tp.clone(); move |e: Event<FormData>| { let mut t = tp.clone(); t.style.background = Some(parse_color(&e.value())); on_change.call(t); } } }
                }
            }
            div { class: "field", label { "{t(lang, \"calc.border_color\")}" }
                input { id: "{id(\"bc\")}", r#type: "color", value: "{color_hex(s.border_color)}",
                    oninput: { let tp = tp.clone(); move |e: Event<FormData>| { let mut t = tp.clone(); t.style.border_color = parse_color(&e.value()); on_change.call(t); } } }
            }
            div { class: "field", label { "{t(lang, \"calc.border_width\")}" }
                input { id: "{id(\"bw\")}", r#type: "number", min: "0", max: "6", step: "0.5", value: "{s.border_width}",
                    onchange: { let tp = tp.clone(); move |e: Event<FormData>| { let mut t = tp.clone(); t.style.border_width = num(&e.value(), 1.0).clamp(0.0, 6.0); on_change.call(t); } } }
            }
            div { class: "field", label { "{t(lang, \"calc.border_style\")}" }
                select { id: "{id(\"bs\")}",
                    onchange: { let tp = tp.clone(); move |e: Event<FormData>| { let mut t = tp.clone(); t.style.border_dashed = e.value() == "dashed"; on_change.call(t); } },
                    option { value: "solid", selected: !s.border_dashed, "{t(lang, \"calc.solid\")}" }
                    option { value: "dashed", selected: s.border_dashed, "{t(lang, \"calc.dashed\")}" }
                }
            }
            div { class: "field", label { "{t(lang, \"calc.font_size\")}" }
                input { id: "{id(\"fs\")}", r#type: "number", min: "5", max: "24", value: "{s.font_size}",
                    onchange: { let tp = tp.clone(); move |e: Event<FormData>| { let mut t = tp.clone(); t.style.font_size = num(&e.value(), 10.0).clamp(5.0, 24.0); on_change.call(t); } } }
            }
            div { class: "field", label { "{t(lang, \"calc.text_color\")}" }
                input { r#type: "color", value: "{color_hex(s.text_color)}",
                    oninput: { let tp = tp.clone(); move |e: Event<FormData>| { let mut t = tp.clone(); t.style.text_color = parse_color(&e.value()); on_change.call(t); } } }
            }
        }
        label { class: "check",
            input { r#type: "checkbox", checked: tape.show_date,
                onchange: { let tp = tp.clone(); move |_| { let mut t = tp.clone(); t.show_date = !t.show_date; on_change.call(t); } } }
            span { "{t(lang, \"calc.show_date\")}" }
        }
    }
}

#[component]
pub fn CalculatorPane() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let Some(e) = CALC_EDIT() else {
        return rsx! {};
    };
    let tape = e.tape.clone();
    let rows = tape.rows();
    let totals = tape.totals();
    let total = tape.format(tape.total());
    let entry = e.entry.clone();
    let preview = parse_amount(&entry).map(|v| tape.format(v)).unwrap_or_default();
    let tools: [(&str, &str); 5] = [
        ("ac.addLine", "insert"),
        ("ac.delLine", "delete"),
        ("ac.clearTape", "close"),
        ("ac.undo", "undo"),
        ("ac.redo", "redo"),
    ];
    let keys: [&str; 16] = [
        "7", "8", "9", "/", "4", "5", "6", "*", "1", "2", "3", "-", "0", ".", "=", "+",
    ];
    let page = e.page + 1;
    rsx! {
        div { class: "right-pane calc-pane", onclick: move |e| e.stop_propagation(),
            div { class: "pane-header",
                Icon { name: "calc" }
                span { class: "pane-title", "{label_of(lang, \"comments.accountingCalc\")}" }
                div { class: "spacer" }
                button { class: "ibtn", title: "{t(lang, \"dialog.close\")}", onclick: move |_| close(st), "×" }
            }
            div { class: "pane-body calc-body",
                div { class: "pane-toolbar icon-toolbar",
                    for (id, icon) in tools {
                        button { class: "ibtn", key: "{id}", title: "{label_of(lang, id)}", onclick: move |_| dispatch(st, id), Icon { name: icon } }
                    }
                    button { class: "ibtn", title: "{label_of(lang, \"ac.settings\")}", onclick: move |_| dispatch(st, "ac.settings"), Icon { name: "settings" } }
                }
                div { class: "calc-entry",
                    input { id: "calc-entry", value: "{entry}", placeholder: "0", inputmode: "decimal", autofocus: true,
                        oninput: move |ev| view(|c| c.entry = ev.value()),
                        onkeydown: move |ev| {
                            let k = ev.key().to_string();
                            let op = match k.as_str() { "Enter" => Some('+'), "+" | "-" | "*" | "/" | "=" => k.chars().next(), _ => None };
                            if let Some(op) = op { ev.prevent_default(); ev.stop_propagation(); enter(st, op); }
                            else if k == "Escape" { ev.stop_propagation(); view(|c| c.entry.clear()); }
                        }
                    }
                    if !preview.is_empty() { span { class: "muted", "{preview}" } }
                }
                div { class: "calc-keys",
                    for k in keys {
                        {
                            let is_op = OPERATIONS.contains(&k.chars().next().unwrap_or(' '));
                            let shown = if is_op { op_symbol(k.chars().next().unwrap_or('=')) } else { k };
                            rsx! {
                                button { class: if is_op { "calc-key op" } else { "calc-key" }, key: "{k}",
                                    onclick: move |_| {
                                        let c = k.chars().next().unwrap_or('0');
                                        if is_op { enter(st, c); } else { view(|e| e.entry.push(c)); }
                                    },
                                    "{shown}"
                                }
                            }
                        }
                    }
                    button { class: "calc-key wide", title: "{t(lang, \"calc.clear_entry\")}", onclick: move |_| view(|e| e.entry.clear()), "C" }
                    button { class: "calc-key wide", title: "{t(lang, \"calc.backspace\")}", onclick: move |_| view(|e| { e.entry.pop(); }), "⌫" }
                }
                div { class: "calc-tape",
                    if rows.is_empty() { div { class: "muted calc-empty", "{t(lang, \"calc.empty\")}" } }
                    for (i, (comment, value, op, is_total)) in rows.into_iter().enumerate() {
                        {
                            let selected = e.selected.contains(&i);
                            let amount = if is_total { value.clone() } else { tape.lines[i].amount.to_string() };
                            let run = tape.format(totals[i]);
                            let row_class = match (selected, is_total) {
                                (true, _) => "calc-line selected",
                                (false, true) => "calc-line total",
                                _ => "calc-line",
                            };
                            rsx! {
                                div { class: "{row_class}", key: "l{i}",
                                    onclick: move |ev| {
                                        let add = ev.modifiers().ctrl() || ev.modifiers().meta();
                                        view(|c| { if add { if !c.selected.remove(&i) { c.selected.insert(i); } } else { c.selected = std::collections::BTreeSet::from([i]); } });
                                    },
                                    input { r#type: "checkbox", checked: selected, title: "{t(lang, \"calc.select\")}",
                                        onclick: move |ev| ev.stop_propagation(),
                                        onchange: move |_| view(|c| { if !c.selected.remove(&i) { c.selected.insert(i); } }) }
                                    select { id: "calc-op-{i}", class: "calc-op", title: "{label_of(lang, \"ac.list.edit.oper\")}",
                                        onchange: move |ev| { let o = ev.value().chars().next().unwrap_or('+'); change(st, |c| if let Some(l) = c.tape.lines.get_mut(i) { l.op = o; }); },
                                        for o in OPERATIONS.iter() {
                                            option { key: "{o}", value: "{o}", selected: *o == op, "{op_symbol(*o)}" }
                                        }
                                    }
                                    input { id: "calc-amount-{i}", class: "calc-amount", value: "{amount}", disabled: is_total, title: "{label_of(lang, \"ac.list.edit.amount\")}",
                                        onchange: move |ev| { if let Some(v) = parse_amount(&ev.value()) { change(st, |c| if let Some(l) = c.tape.lines.get_mut(i) { l.amount = v; }); } } }
                                    input { id: "calc-comment-{i}", class: "calc-comment", value: "{comment}", placeholder: "{t(lang, \"calc.comment\")}", title: "{label_of(lang, \"ac.list.edit.comment\")}",
                                        onchange: move |ev| { let v = ev.value(); change(st, |c| if let Some(l) = c.tape.lines.get_mut(i) { l.comment = v; }); } }
                                    span { class: "calc-run", title: "{t(lang, \"calc.running\")}", "{run}" }
                                }
                            }
                        }
                    }
                    div { class: "calc-total", span { "{t(lang, \"calc.total\")}" } span { class: "calc-total-value", "{total}" } }
                }
                details { class: "calc-more",
                    summary { "{t(lang, \"calc.look\")}" }
                    TapeSettings { tape: tape.clone(), ids: true, on_change: move |t: Tape| change(st, |c| c.tape = t) }
                    button { class: "tbtn", onclick: move |_| dispatch(st, "ac.preview.reset"), "{label_of(lang, \"ac.preview.reset\")}" }
                }
                p { class: "hint", "{t(lang, \"calc.hint\")} ({t(lang, \"status.page\")} {page})" }
            }
        }
    }
}

/// Settings of new tapes.
#[component]
pub fn CalcSettingsDialog() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let mut tape = use_signal(|| defaults(st));
    rsx! {
        Frame { title: label_of(lang, "ac.settings"),
            on_ok: move |_| { save_defaults(st, &tape()); st.set_dialog(Dialog::None); },
            p { class: "hint", "{t(lang, \"calc.settings_hint\")}" }
            TapeSettings { tape: tape(), ids: false, on_change: move |t: Tape| tape.set(t) }
        }
    }
}
