//! The Measurements pane (FOL-L028): the measurement comments of the
//! document by kind, with the total of each kind whose values share their
//! units; a click shows the measurement on its page.

use dioxus::prelude::*;
use folitecta_core::{DestView, Measurement, measure_totals};

use crate::i18n::{t, tf};
use crate::state::AppState;

/// A total as it is shown: the number with up to two decimals, the unit.
fn total_text(totals: &[(f64, String)]) -> String {
    totals
        .iter()
        .map(|(n, u)| {
            let s = format!("{n:.2}");
            let s = s.trim_end_matches('0').trim_end_matches('.');
            format!("{s} {u}")
        })
        .collect::<Vec<_>>()
        .join(" + ")
}

#[component]
pub fn MeasurementsPane() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let all = st.read_active(|t| t.doc.measurement_list()).unwrap_or_default();
    let mut kinds: Vec<String> = all.iter().map(|m| m.kind.clone()).collect();
    kinds.dedup();
    kinds.sort();
    kinds.dedup();
    let groups: Vec<(String, Vec<Measurement>, String)> = kinds
        .into_iter()
        .map(|k| {
            let items: Vec<Measurement> = all.iter().filter(|m| m.kind == k).cloned().collect();
            // Angles do not add up.
            let total = if k == "Angle" {
                String::new()
            } else {
                total_text(&measure_totals(items.iter().map(|m| m.value.as_str())))
            };
            (k, items, total)
        })
        .collect();
    rsx! {
        div { class: "pane-toolbar",
            button { class: "tbtn measures-export", disabled: all.is_empty(),
                onclick: move |_| crate::commands::dispatch(st, "exportMeasuresToCSV"),
                "{t(lang, \"pane.measurements.export\")}" }
        }
        div { class: "measurements-list",
            if all.is_empty() {
                div { class: "pane-empty", "{t(lang, \"pane.measurements.none\")}" }
            }
            for (kind, items, total) in groups {
                div { key: "{kind}", class: "measure-group",
                    div { class: "title",
                        b { {t(lang, &format!("pane.measurements.kind.{}", kind.to_lowercase()))} }
                        span { class: "muted", " ({items.len()})" }
                        if !total.is_empty() {
                            span { class: "measure-total", " · {tf(lang, \"pane.measurements.total\", &[(\"total\", &total)])}" }
                        }
                    }
                    for m in items {
                        {
                            let (page, rect, index) = (m.page, m.rect, m.index);
                            rsx! {
                                button { key: "{page}-{index}", class: "linkish measure-item",
                                    onclick: move |_| {
                                        st.go_to_dest(page, Some(DestView::Rect(rect)));
                                        st.with_active(|t| t.selected_annot = Some((page, index)));
                                    },
                                    "{m.value} · {t(lang, \"status.page\")} {page + 1}"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
