//! The format and the check of a text field, chosen in its properties: none,
//! a number (decimals, separators, currency), a percentage, a date, a time
//! or a postal code or phone number, and the range a number must be in.

use dioxus::prelude::*;
use folitecta_core::form_format::{DATE_FORMATS, TIME_FORMATS, print_date};
use folitecta_core::{FieldCheck, FieldFormat, FieldProps};

use crate::i18n::{Lang, t};
use crate::state::AppState;

/// The separator styles of numbers, as they show 1234.5.
const SEPARATORS: [&str; 5] = ["1,234.50", "1234.50", "1.234,50", "1234,50", "1'234.50"];

fn kind_of(f: &FieldFormat) -> &'static str {
    match f {
        FieldFormat::None => "none",
        FieldFormat::Number { .. } => "number",
        FieldFormat::Percent { .. } => "percent",
        FieldFormat::Date(_) => "date",
        FieldFormat::Time(_) => "time",
        FieldFormat::Special(_) => "special",
        FieldFormat::Custom => "custom",
    }
}

/// A new format of the kind `kind`, with what makes sense at first.
fn new_format(lang: Lang, kind: &str) -> FieldFormat {
    match kind {
        "number" => FieldFormat::Number {
            decimals: 2,
            separators: if lang == Lang::It { 2 } else { 0 },
            parentheses: false,
            currency: String::new(),
            before: false,
        },
        "percent" => FieldFormat::Percent {
            decimals: 0,
            separators: if lang == Lang::It { 2 } else { 0 },
        },
        "date" => FieldFormat::Date(t(lang, "field.date_format_default")),
        "time" => FieldFormat::Time(TIME_FORMATS[0].into()),
        "special" => FieldFormat::Special(0),
        _ => FieldFormat::None,
    }
}

/// A number typed in a box (empty: none).
fn number(text: &str) -> Option<f64> {
    text.trim().replace(',', ".").parse::<f64>().ok()
}

#[component]
pub fn FieldFormatRows(current: FieldProps, apply: EventHandler<FieldProps>) -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let format = current.format.clone();
    let kind = kind_of(&format);
    let (year, _) = folitecta_core::templates::this_month();
    let kinds: Vec<(&str, String)> = ["none", "number", "percent", "date", "time", "special"]
        .into_iter()
        .map(|k| (k, t(lang, &format!("field.format.{k}"))))
        .collect();
    let specials: Vec<String> = (0..4).map(|i| t(lang, &format!("field.special.{i}"))).collect();
    let with = {
        let current = current.clone();
        move |f: FieldFormat| {
            let mut n = current.clone();
            // A range only for numbers.
            if !matches!(f, FieldFormat::Number { .. } | FieldFormat::Percent { .. })
                && n.check != FieldCheck::Custom
            {
                n.check = FieldCheck::None;
            }
            n.format = f;
            n
        }
    };
    let (min, max) = match &current.check {
        FieldCheck::Range { min, max } => (
            min.map(|v| v.to_string()).unwrap_or_default(),
            max.map(|v| v.to_string()).unwrap_or_default(),
        ),
        _ => (String::new(), String::new()),
    };
    let set_range = {
        let current = current.clone();
        move |min: Option<f64>, max: Option<f64>| {
            let mut n = current.clone();
            n.check = if min.is_none() && max.is_none() {
                FieldCheck::None
            } else {
                FieldCheck::Range { min, max }
            };
            n
        }
    };
    rsx! {
        div { class: "prop-head", "{t(lang, \"field.format\")}" }
        div { class: "style-row",
            label { "{t(lang, \"field.format_kind\")}" }
            select { class: "field-format-kind",
                onchange: { let with = with.clone(); move |e: Event<FormData>| apply.call(with(new_format(lang, &e.value()))) },
                for (k, label) in kinds.iter() {
                    option { key: "{k}", value: "{k}", selected: *k == kind, "{label}" }
                }
                if kind == "custom" {
                    option { value: "custom", selected: true, disabled: true, "{t(lang, \"field.format.custom\")}" }
                }
            }
        }
        match format.clone() {
            FieldFormat::Number { decimals, separators, parentheses, currency, before } => rsx! {
                div { class: "style-row",
                    label { "{t(lang, \"field.decimals\")}" }
                    input { r#type: "number", min: "0", max: "6", value: "{decimals}",
                        onchange: { let with = with.clone(); let currency = currency.clone(); move |e: Event<FormData>| if let Ok(d) = e.value().parse::<u8>() {
                            apply.call(with(FieldFormat::Number { decimals: d.min(6), separators, parentheses, currency: currency.clone(), before }))
                        } } }
                }
                div { class: "style-row",
                    label { "{t(lang, \"field.separators\")}" }
                    select { onchange: { let with = with.clone(); let currency = currency.clone(); move |e: Event<FormData>| if let Ok(s) = e.value().parse::<u8>() {
                            apply.call(with(FieldFormat::Number { decimals, separators: s, parentheses, currency: currency.clone(), before }))
                        } },
                        for (i, ex) in SEPARATORS.iter().enumerate() {
                            option { key: "{i}", value: "{i}", selected: i as u8 == separators, "{ex}" }
                        }
                    }
                }
                div { class: "style-row",
                    label { "{t(lang, \"field.currency\")}" }
                    input { class: "field-currency", value: "{currency}", maxlength: "6",
                        onchange: { let with = with.clone(); move |e: Event<FormData>| apply.call(with(FieldFormat::Number { decimals, separators, parentheses, currency: e.value(), before })) } }
                    label { class: "check mini", input { r#type: "checkbox", checked: before,
                        onchange: { let with = with.clone(); let currency = currency.clone(); move |_| apply.call(with(FieldFormat::Number { decimals, separators, parentheses, currency: currency.clone(), before: !before })) } }
                        span { "{t(lang, \"field.currency_before\")}" } }
                }
                div { class: "check-row",
                    label { class: "check mini", input { r#type: "checkbox", checked: parentheses,
                        onchange: { let with = with.clone(); let currency = currency.clone(); move |_| apply.call(with(FieldFormat::Number { decimals, separators, parentheses: !parentheses, currency: currency.clone(), before })) } }
                        span { "{t(lang, \"field.parentheses\")}" } }
                }
            },
            FieldFormat::Percent { decimals, separators } => rsx! {
                div { class: "style-row",
                    label { "{t(lang, \"field.decimals\")}" }
                    input { r#type: "number", min: "0", max: "6", value: "{decimals}",
                        onchange: { let with = with.clone(); move |e: Event<FormData>| if let Ok(d) = e.value().parse::<u8>() { apply.call(with(FieldFormat::Percent { decimals: d.min(6), separators })) } } }
                }
                div { class: "style-row",
                    label { "{t(lang, \"field.separators\")}" }
                    select { onchange: { let with = with.clone(); move |e: Event<FormData>| if let Ok(s) = e.value().parse::<u8>() { apply.call(with(FieldFormat::Percent { decimals, separators: s })) } },
                        for (i, ex) in SEPARATORS.iter().enumerate() {
                            option { key: "{i}", value: "{i}", selected: i as u8 == separators, "{ex}" }
                        }
                    }
                }
            },
            FieldFormat::Date(f) => {
                let example = print_date(&f, year, 12, 31);
                rsx! {
                    div { class: "style-row",
                        label { "{t(lang, \"field.date_format\")}" }
                        input { class: "wide field-date-format", value: "{f}", list: "field-date-formats",
                            onchange: { let with = with.clone(); move |e: Event<FormData>| { let v = e.value(); if !v.trim().is_empty() { apply.call(with(FieldFormat::Date(v.trim().to_string()))) } } } }
                        datalist { id: "field-date-formats",
                            for d in DATE_FORMATS.iter() { option { key: "{d}", value: "{d}" } }
                        }
                    }
                    div { class: "hint field-format-example", "{t(lang, \"field.example\")} {example}" }
                }
            }
            FieldFormat::Time(f) => rsx! {
                div { class: "style-row",
                    label { "{t(lang, \"field.time_format\")}" }
                    select { onchange: { let with = with.clone(); move |e: Event<FormData>| apply.call(with(FieldFormat::Time(e.value()))) },
                        for tf in TIME_FORMATS.iter() {
                            option { key: "{tf}", value: "{tf}", selected: *tf == f, "{tf}" }
                        }
                    }
                }
            },
            FieldFormat::Special(n) => rsx! {
                div { class: "style-row",
                    label { "{t(lang, \"field.special\")}" }
                    select { onchange: { let with = with.clone(); move |e: Event<FormData>| if let Ok(s) = e.value().parse::<u8>() { apply.call(with(FieldFormat::Special(s))) } },
                        for (i, label) in specials.iter().enumerate() {
                            option { key: "{i}", value: "{i}", selected: i as u8 == n, "{label}" }
                        }
                    }
                }
            },
            FieldFormat::Custom => rsx! { div { class: "hint", "{t(lang, \"field.format_custom_hint\")}" } },
            FieldFormat::None => rsx! {},
        }
        if matches!(format, FieldFormat::Number { .. } | FieldFormat::Percent { .. }) {
            if current.check == FieldCheck::Custom {
                div { class: "hint", "{t(lang, \"field.check_custom_hint\")}" }
            } else {
                div { class: "style-row",
                    label { "{t(lang, \"field.range\")}" }
                    input { class: "field-min", r#type: "text", inputmode: "decimal", placeholder: "{t(lang, \"field.min\")}", value: "{min}",
                        onchange: { let set_range = set_range.clone(); let max = max.clone(); move |e: Event<FormData>| apply.call(set_range(number(&e.value()), number(&max))) } }
                    input { class: "field-max", r#type: "text", inputmode: "decimal", placeholder: "{t(lang, \"field.max\")}", value: "{max}",
                        onchange: { let set_range = set_range.clone(); let min = min.clone(); move |e: Event<FormData>| apply.call(set_range(number(&min), number(&e.value()))) } }
                }
            }
        }
    }
}
