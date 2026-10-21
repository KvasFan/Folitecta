//! The Clean Up Scans dialog: the thresholds of blank and askew pages, the
//! pages found with their pictures (each can be left out, and shown in the
//! window), and the clean-up in one step.

use std::collections::HashSet;

use dioxus::prelude::*;
use folitecta_core::{ScanFix, ScanThresholds, registry, scan_fixes};

use crate::commands::scans::{self, SCAN};
use crate::i18n::{cmd_label, t, tf};
use crate::state::AppState;
use crate::ui::dialogs::Frame;

/// A number typed in a field, or `default`.
fn number(text: &str, default: f32) -> f32 {
    text.trim()
        .replace(',', ".")
        .parse::<f32>()
        .ok()
        .filter(|v| v.is_finite() && *v >= 0.0)
        .unwrap_or(default)
}

#[component]
pub fn CleanUpScansDialog() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let title = registry()
        .get("document.cleanUpScans")
        .map(|c| cmd_label(lang, c).trim_end_matches('…').to_string())
        .unwrap_or_default();
    let defaults = ScanThresholds::default();
    let mut blank = use_signal(|| {
        let v = st.value("scans.blank");
        if v.is_empty() {
            format!("{}", defaults.blank_ink * 100.0)
        } else {
            v
        }
    });
    let mut skew = use_signal(|| {
        let v = st.value("scans.skew");
        if v.is_empty() {
            format!("{}", defaults.min_skew)
        } else {
            v
        }
    });
    let mut delete = use_signal(|| st.value("scans.delete") != "0");
    let mut straighten = use_signal(|| st.value("scans.straighten") != "0");
    let mut left_out = use_signal(HashSet::<usize>::new);

    let run = SCAN.read().clone();
    let current = scans::current(st, &run);
    let thresholds = ScanThresholds {
        blank_ink: number(&blank(), defaults.blank_ink * 100.0) / 100.0,
        min_skew: number(&skew(), defaults.min_skew),
    };
    let found = scan_fixes(&run.looks, &thresholds);
    let wanted = |f: &ScanFix| match f {
        ScanFix::Delete { .. } => delete(),
        ScanFix::Straighten { .. } => straighten(),
    };
    let fixes: Vec<ScanFix> = found
        .iter()
        .filter(|f| wanted(f))
        .filter(|f| {
            let (ScanFix::Delete { page } | ScanFix::Straighten { page, .. }) = **f;
            !left_out.read().contains(&page)
        })
        .copied()
        .collect();
    let (blank_count, askew_count) = found.iter().fold((0, 0), |(b, a), f| match f {
        ScanFix::Delete { .. } => (b + 1, a),
        ScanFix::Straighten { .. } => (b, a + 1),
    });
    let (tab, shown) = st
        .read_active(|t| (t.id, t.page_shown.clone()))
        .unwrap_or_default();
    let percent = (run.done * 100).checked_div(run.total).unwrap_or(0);
    let progress = tf(
        lang,
        "scans.progress",
        &[
            ("page", &(run.done + 1).min(run.total).to_string()),
            ("count", &run.total.to_string()),
        ],
    );
    let summary = tf(
        lang,
        "scans.summary",
        &[
            ("scans", &run.looks.len().to_string()),
            ("pages", &run.total.to_string()),
            ("blank", &blank_count.to_string()),
            ("askew", &askew_count.to_string()),
        ],
    );
    let apply_fixes = fixes.clone();
    rsx! {
        Frame { title, side: true, ok_label: Some(t(lang, "scans.apply")),
            ok_disabled: run.running || !current || fixes.is_empty(),
            on_ok: move |_| {
                st.set_value("scans.blank", blank());
                st.set_value("scans.skew", skew());
                st.set_value("scans.delete", if delete() { "1" } else { "0" });
                st.set_value("scans.straighten", if straighten() { "1" } else { "0" });
                scans::apply(st, apply_fixes.clone());
            },
            p { class: "hint", "{t(lang, \"scans.hint\")}" }
            div { class: "field-grid",
                div { class: "field",
                    label { class: "check",
                        input { r#type: "checkbox", class: "scans-delete", checked: delete(), onchange: move |_| delete.toggle() }
                        span { "{t(lang, \"scans.delete_blank\")}" }
                    }
                    label { "{t(lang, \"scans.blank_below\")}" }
                    input { class: "scans-blank", r#type: "number", min: "0", max: "10", step: "0.05", value: "{blank}",
                        oninput: move |e| blank.set(e.value()) }
                }
                div { class: "field",
                    label { class: "check",
                        input { r#type: "checkbox", class: "scans-straighten", checked: straighten(), onchange: move |_| straighten.toggle() }
                        span { "{t(lang, \"scans.straighten\")}" }
                    }
                    label { "{t(lang, \"scans.straighten_from\")}" }
                    input { class: "scans-skew", r#type: "number", min: "0", max: "15", step: "0.1", value: "{skew}",
                        oninput: move |e| skew.set(e.value()) }
                }
            }
            if run.running {
                div { class: "ocr-progress",
                    div { class: "ocr-bar", div { class: "ocr-fill", style: "width:{percent}%" } }
                    span { class: "muted", "{progress}" }
                    button { class: "tbtn scans-stop", onclick: move |_| scans::stop(), "{t(lang, \"scans.stop\")}" }
                }
            } else if let Some(why) = run.failure.clone() {
                p { class: "hint warn", "{why}" }
            } else if !current {
                div { class: "row",
                    span { class: "hint warn", "{t(lang, \"scans.changed\")}" }
                    button { class: "tbtn scans-again", onclick: move |_| scans::look(st), "{t(lang, \"scans.again\")}" }
                }
            }
            if !run.running || !run.looks.is_empty() {
                p { class: "muted scans-summary", "{summary}" }
            }
            if !run.running && run.failure.is_none() && run.looks.is_empty() {
                p { class: "hint", "{t(lang, \"scans.none\")}" }
            } else if !run.running && found.is_empty() {
                p { class: "hint", "{t(lang, \"scans.clean\")}" }
            }
            div { class: "scans-list",
                for f in found.iter().copied() {
                    {
                        let (page, detail, kind_on) = match f {
                            ScanFix::Delete { page } => {
                                let ink = run.looks.iter().find(|l| l.page == page).map(|l| l.ink).unwrap_or(0.0);
                                (page, tf(lang, "scans.blank_row", &[("ink", &format!("{:.2}", ink * 100.0))]), delete())
                            }
                            ScanFix::Straighten { page, angle, .. } => {
                                (page, tf(lang, "scans.askew_row", &[("angle", &format!("{:.1}", angle))]), straighten())
                            }
                        };
                        let on = kind_on && !left_out.read().contains(&page);
                        let rev = shown.get(page).copied().unwrap_or_default();
                        let src = format!("/page/{tab}/{page}/{rev}/200.png?lane=thumb");
                        let label = tf(lang, "scans.page", &[("page", &(page + 1).to_string())]);
                        rsx! {
                            div { key: "{page}", class: if on { "scans-row" } else { "scans-row off" },
                                input { r#type: "checkbox", checked: on, disabled: !kind_on,
                                    onchange: move |_| {
                                        let mut l = left_out.write();
                                        if !l.remove(&page) {
                                            l.insert(page);
                                        }
                                    }
                                }
                                img { class: "scans-thumb", src: "{src}", alt: "",
                                    onclick: move |_| st.navigate(page, true) }
                                button { class: "linkish scans-go", onclick: move |_| st.navigate(page, true),
                                    span { class: "scans-page", "{label}" }
                                    span { class: "muted", "{detail}" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
