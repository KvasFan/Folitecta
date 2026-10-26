//! The Preflight dialog (Document › PDF/A): the rules of PDF/A-2b checked,
//! the ones broken with how many times and whether conversion fixes them,
//! the problems one by one, and Convert to PDF/A, which opens a converted
//! copy in a new tab.

use dioxus::prelude::*;
use folitecta_core::pdfa::{PROFILE, PROFILE_STANDARD, RULES};

use crate::commands::preflight::{self, PREFLIGHT};
use crate::i18n::{t, tf};
use crate::state::AppState;
use crate::ui::dialogs::Frame;

#[component]
pub fn PreflightDialog() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let state = PREFLIGHT.read().clone();
    let result = state.result.clone();
    let rows: Vec<(String, String, String, bool, &'static str)> = RULES
        .iter()
        .map(|(id, clause, _)| {
            let n = result
                .as_ref()
                .map(|r| r.findings.iter().filter(|f| f.rule == *id).count())
                .unwrap_or(0);
            let fixable = result
                .as_ref()
                .is_some_and(|r| r.findings.iter().any(|f| f.rule == *id && f.fixable));
            let status = if result.is_none() {
                String::new()
            } else if n == 0 {
                "✓".to_string()
            } else {
                n.to_string()
            };
            (
                t(lang, &format!("preflight.rule.{id}")),
                clause.to_string(),
                status,
                fixable,
                if n > 0 { "audit-row broken" } else { "audit-row" },
            )
        })
        .collect();
    let findings: Vec<String> = result
        .as_ref()
        .map(|r| {
            r.findings
                .iter()
                .take(40)
                .map(|f| {
                    let page = f
                        .page
                        .map(|p| tf(lang, "recompress.page", &[("page", &(p + 1).to_string())]))
                        .unwrap_or_default();
                    format!(
                        "{} — {} {}",
                        t(lang, &format!("preflight.rule.{}", f.rule)),
                        page,
                        f.detail
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    let summary = match result.as_ref() {
        None => String::new(),
        Some(r) if r.passed() => t(lang, "preflight.passed"),
        Some(r) => {
            let fixable = r.findings.iter().all(|f| f.fixable);
            tf(
                lang,
                if fixable {
                    "preflight.fixable"
                } else {
                    "preflight.not_fixable"
                },
                &[("count", &r.findings.len().to_string())],
            )
        }
    };
    let title = format!("{} — {PROFILE}", t(lang, "preflight.title"));
    rsx! {
        Frame { title, wide: true, ok_label: t(lang, "preflight.convert"), ok_disabled: state.busy || result.is_none(),
            on_ok: move |_| preflight::convert(st),
            p { class: "hint", "{PROFILE_STANDARD}. {t(lang, \"preflight.hint\")}" }
            if state.busy {
                p { class: "hint preflight-busy", "{t(lang, \"preflight.checking\")}" }
            }
            if !summary.is_empty() {
                div { class: "callout preflight-summary", "{summary}" }
            }
            if let Some(p) = state.problem.as_ref() {
                div { class: "callout warn", "{p}" }
            }
            div { class: "audit-table preflight-rules",
                for (i, (name, clause, status, fixable, class)) in rows.into_iter().enumerate() {
                    div { key: "{i}", class,
                        span { "{name}" }
                        span { class: "muted", "{clause}" }
                        span { "{status}" }
                        span { if fixable { "{t(lang, \"preflight.will_fix\")}" } }
                    }
                }
            }
            if !findings.is_empty() {
                div { class: "preflight-findings",
                    for (i, f) in findings.into_iter().enumerate() {
                        div { key: "{i}", class: "preflight-finding", "{f}" }
                    }
                }
            }
        }
    }
}
