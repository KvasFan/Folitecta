//! A month calendar shown by a date field while it is filled: a click on a
//! day writes that date in the field's format.

use dioxus::prelude::*;
use folitecta_core::form_format::{print_date, scan_date};

use crate::i18n::{Lang, t};
use crate::state::AppState;

/// The days of `month` (1 to 12) of `year`.
fn days_in(year: i32, month: u32) -> u32 {
    match month {
        2 if (year % 4 == 0 && year % 100 != 0) || year % 400 == 0 => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// The day of the week of a date, 0 Sunday to 6 Saturday.
fn weekday(year: i32, month: u32, day: u32) -> u32 {
    const OFFSETS: [i32; 12] = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
    let y = if month < 3 { year - 1 } else { year };
    ((y + y / 4 - y / 100 + y / 400 + OFFSETS[(month - 1) as usize] + day as i32).rem_euclid(7)) as u32
}

/// Today (year, month, day).
fn today() -> (i32, u32, u32) {
    let s = folitecta_core::util::today_display();
    let mut p = s.split('-').filter_map(|n| n.parse::<i64>().ok());
    match (p.next(), p.next(), p.next()) {
        (Some(y), Some(m), Some(d)) => (y as i32, m as u32, d as u32),
        _ => (2026, 1, 1),
    }
}

/// The week as the language writes it: from Monday in Italian, from Sunday
/// in English.
fn first_day(lang: Lang) -> u32 {
    if lang == Lang::It { 1 } else { 0 }
}

#[component]
pub fn DateCalendar(
    format: String,
    value: String,
    left: f32,
    top: f32,
    on_pick: EventHandler<String>,
) -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let chosen = scan_date(&format, &value);
    let now = today();
    let mut shown = use_signal(|| chosen.map(|(y, m, _)| (y, m)).unwrap_or((now.0, now.1)));
    let (year, month) = shown();
    let first = first_day(lang);
    let lead = (weekday(year, month, 1) + 7 - first) % 7;
    let days = days_in(year, month);
    let title = format!("{} {year}", t(lang, &format!("calendar.month_{month}")));
    let names: Vec<String> = (0..7)
        .map(|i| t(lang, &format!("calendar.day_{}", (first + i) % 7)))
        .collect();
    let mut step = move |by: i32| {
        let (y, m) = shown();
        let k = y * 12 + m as i32 - 1 + by;
        shown.set((k.div_euclid(12), (k.rem_euclid(12) + 1) as u32));
    };
    rsx! {
        // A press here must not take the focus away from the field (that commits it).
        div { class: "date-calendar", style: "left:{left}px;top:{top}px",
            onmousedown: move |e| { e.prevent_default(); e.stop_propagation(); },
            div { class: "date-calendar-head",
                button { class: "ibtn date-prev", title: "{t(lang, \"calendar.previous\")}", onclick: move |_| step(-1), "‹" }
                span { class: "date-calendar-title", "{title}" }
                button { class: "ibtn date-next", title: "{t(lang, \"calendar.next\")}", onclick: move |_| step(1), "›" }
            }
            div { class: "date-calendar-grid",
                for n in names.iter() { span { key: "{n}", class: "date-calendar-name", "{n}" } }
                for i in 0..lead { span { key: "lead{i}" } }
                for d in 1..=days {
                    {
                        let on = chosen == Some((year, month, d));
                        let is_today = now == (year, month, d);
                        let class = match (on, is_today) {
                            (true, _) => "date-day on",
                            (false, true) => "date-day today",
                            _ => "date-day",
                        };
                        let fmt = format.clone();
                        rsx! {
                            button { key: "{d}", class, "data-day": "{d}",
                                onclick: move |_| on_pick.call(print_date(&fmt, year, month, d)),
                                "{d}"
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weeks_and_months() {
        assert_eq!(weekday(2026, 10, 28), 3, "a Wednesday");
        assert_eq!(weekday(2024, 2, 29), 4, "a Thursday");
        assert_eq!(weekday(2000, 1, 1), 6, "a Saturday");
        assert_eq!(days_in(2024, 2), 29);
        assert_eq!(days_in(2100, 2), 28);
        assert_eq!(days_in(2026, 4), 30);
    }
}
