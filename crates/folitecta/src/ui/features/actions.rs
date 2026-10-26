//! The Action Sequences dialog: the sequences (new, delete, import,
//! export), the steps of the one shown with their settings, and running it
//! on the open document or on files and folders — the plan of inputs,
//! outputs and collisions first, then a result for every file.

use dioxus::prelude::*;
use folitecta_core::{PageSet, RecompressPreset, Step};

use crate::commands::actions::{self, ACTIONS};
use crate::i18n::{t, tf};
use crate::state::AppState;
use crate::ui::dialogs::Frame;

const POSITIONS: [&str; 7] = [
    "bottom-center",
    "bottom-right",
    "bottom-left",
    "top-center",
    "top-right",
    "top-left",
    "center",
];

fn file_name(p: &std::path::Path) -> String {
    p.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default()
}

/// The settings of step `i`.
fn step_settings(st: AppState, i: usize, step: &Step) -> Element {
    let lang = st.lang();
    let set = move |s: Step| {
        actions::edit(st, move |q| {
            if let Some(x) = q.steps.get_mut(i) {
                *x = s;
            }
        })
    };
    match step.clone() {
        Step::Rotate { degrees, pages } => rsx! {
            select { class: "actions-degrees", onchange: move |e| set(Step::Rotate { degrees: e.value().parse().unwrap_or(90), pages }),
                for d in [90, 180, 270] { option { key: "{d}", value: "{d}", selected: d == degrees, "{d}°" } }
            }
            select { onchange: move |e| {
                    let p = match e.value().as_str() { "odd" => PageSet::Odd, "even" => PageSet::Even, _ => PageSet::All };
                    set(Step::Rotate { degrees, pages: p });
                },
                option { value: "all", selected: pages == PageSet::All, "{t(lang, \"sequence.pages.all\")}" }
                option { value: "odd", selected: pages == PageSet::Odd, "{t(lang, \"sequence.pages.odd\")}" }
                option { value: "even", selected: pages == PageSet::Even, "{t(lang, \"sequence.pages.even\")}" }
            }
        },
        Step::Recompress { preset } => {
            let presets: Vec<(&'static str, String)> = RecompressPreset::ALL
                .iter()
                .map(|p| (p.id(), t(lang, &format!("recompress.preset.{}", p.id()))))
                .collect();
            rsx! {
                select { onchange: move |e| set(Step::Recompress { preset: e.value() }),
                    for (id, label) in presets {
                        option { key: "{id}", value: "{id}", selected: id == preset, "{label}" }
                    }
                }
            }
        }
        Step::Watermark { text, opacity } => rsx! {
            input { class: "actions-text", value: "{text}", oninput: move |e| set(Step::Watermark { text: e.value(), opacity }) }
        },
        Step::PageNumbers { format, position } => {
            let pos = position.clone();
            let positions: Vec<(&'static str, String)> = POSITIONS
                .iter()
                .map(|p| (*p, t(lang, &format!("sequence.position.{p}"))))
                .collect();
            rsx! {
                input { class: "actions-text", value: "{format}", oninput: move |e| set(Step::PageNumbers { format: e.value(), position: pos.clone() }) }
                select { onchange: move |e| set(Step::PageNumbers { format: format.clone(), position: e.value() }),
                    for (p, label) in positions { option { key: "{p}", value: "{p}", selected: p == position, "{label}" } }
                }
            }
        }
        Step::Info {
            title,
            author,
            subject,
            keywords,
        } => {
            let (a, s2, k) = (author.clone(), subject.clone(), keywords.clone());
            let (t2, s3, k2) = (title.clone(), subject.clone(), keywords.clone());
            rsx! {
                input { class: "actions-text", placeholder: "{t(lang, \"sequence.info.title\")}", value: "{title}",
                    oninput: move |e| set(Step::Info { title: e.value(), author: a.clone(), subject: s2.clone(), keywords: k.clone() }) }
                input { class: "actions-text", placeholder: "{t(lang, \"sequence.info.author\")}", value: "{author}",
                    oninput: move |e| set(Step::Info { title: t2.clone(), author: e.value(), subject: s3.clone(), keywords: k2.clone() }) }
            }
        }
        _ => rsx! {},
    }
}

#[component]
pub fn ActionsDialog() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let state = ACTIONS.read().clone();
    let index = state.current.min(state.sequences.len().saturating_sub(1));
    let Some(seq) = state.sequences.get(index).cloned() else {
        return rsx! {};
    };
    let names: Vec<String> = state.sequences.iter().map(|s| s.name.clone()).collect();
    let mut adding = use_signal(|| "recompress".to_string());
    let has_doc = st.has_doc();
    let batch = state.batch.clone();
    let steps: Vec<(usize, Step, String)> = seq
        .steps
        .iter()
        .cloned()
        .enumerate()
        .map(|(i, s)| {
            let name = t(lang, &format!("sequence.step.{}", s.id()));
            (i, s, name)
        })
        .collect();
    let offered: Vec<(&'static str, String)> = Step::IDS
        .iter()
        .map(|id| (*id, t(lang, &format!("sequence.step.{id}"))))
        .collect();
    let n = steps.len();
    rsx! {
        Frame { title: t(lang, "sequence.title"), wide: true,
            div { class: "actions-head",
                select { class: "actions-sequence",
                    onchange: move |e| {
                        let i: usize = e.value().parse().unwrap_or(0);
                        let mut s = ACTIONS.write();
                        s.current = i;
                        s.batch = None;
                    },
                    for (i, name) in names.iter().enumerate() { option { key: "{i}", value: "{i}", selected: i == index, "{name}" } }
                }
                input { class: "actions-name", value: "{seq.name}", oninput: move |e| { let v = e.value(); actions::edit(st, move |q| q.name = v); } }
                button { class: "tbtn", onclick: move |_| actions::new_sequence(st), "{t(lang, \"sequence.new\")}" }
                button { class: "tbtn", disabled: names.len() <= 1, onclick: move |_| actions::delete_sequence(st), "{t(lang, \"sequence.delete\")}" }
                button { class: "tbtn", onclick: move |_| actions::import(st), "{t(lang, \"sequence.import\")}" }
                button { class: "tbtn", onclick: move |_| actions::export(st), "{t(lang, \"sequence.export\")}" }
            }
            div { class: "actions-steps",
                if steps.is_empty() {
                    p { class: "hint", "{t(lang, \"sequence.no_steps\")}" }
                }
                for (i, step, name) in steps.into_iter() {
                    div { key: "{i}-{step.id()}", class: "actions-step",
                        span { class: "actions-step-name", "{i + 1}. {name}" }
                        div { class: "actions-step-settings", {step_settings(st, i, &step)} }
                        button { class: "ibtn", disabled: i == 0, title: "{t(lang, \"sequence.up\")}",
                            onclick: move |_| actions::edit(st, move |q| q.steps.swap(i, i - 1)), "↑" }
                        button { class: "ibtn", disabled: i + 1 >= n, title: "{t(lang, \"sequence.down\")}",
                            onclick: move |_| actions::edit(st, move |q| q.steps.swap(i, i + 1)), "↓" }
                        button { class: "ibtn", title: "{t(lang, \"sequence.remove\")}",
                            onclick: move |_| actions::edit(st, move |q| { q.steps.remove(i); }), "×" }
                    }
                }
                div { class: "actions-add",
                    select { class: "actions-add-step", onchange: move |e| adding.set(e.value()),
                        for (id, label) in offered { option { key: "{id}", value: "{id}", selected: id == adding(), "{label}" } }
                    }
                    button { class: "tbtn actions-add-btn",
                        onclick: move |_| {
                            if let Some(s) = Step::new(&adding()) {
                                actions::edit(st, move |q| q.steps.push(s));
                            }
                        },
                        "{t(lang, \"sequence.add\")}"
                    }
                }
            }
            div { class: "actions-run",
                button { class: "tbtn primary actions-run-here", disabled: !has_doc || n == 0, onclick: move |_| actions::run_here(st), "{t(lang, \"sequence.run_here\")}" }
                button { class: "tbtn", disabled: n == 0, onclick: move |_| actions::choose(st, false), "{t(lang, \"sequence.run_files\")}" }
                button { class: "tbtn", disabled: n == 0, onclick: move |_| actions::choose(st, true), "{t(lang, \"sequence.run_folder\")}" }
            }
            if let Some(b) = batch {
                {batch_view(st, b)}
            }
        }
    }
}

fn batch_view(st: AppState, b: actions::Batch) -> Element {
    let lang = st.lang();
    let folder = b
        .rule
        .folder
        .as_ref()
        .map(|f| f.display().to_string())
        .unwrap_or_else(|| t(lang, "sequence.next_to_input"));
    let suffix = b.rule.suffix.clone();
    let overwrite = b.rule.overwrite;
    let total = b.items.len();
    let done = b.results.len();
    let count = |s: &str| b.results.iter().filter(|r| r.status == s).count().to_string();
    let summary = tf(
        lang,
        "sequence.summary",
        &[
            ("done", &count("done")),
            ("failed", &count("failed")),
            ("skipped", &count("skipped")),
        ],
    );
    let finished = !b.running && done == total && done > 0;
    let rows: Vec<(String, String, String, String, String)> = b
        .items
        .iter()
        .enumerate()
        .map(|(i, item)| {
            let (status, message, log) = match b.results.get(i) {
                Some(r) => (
                    t(lang, &format!("sequence.status.{}", r.status)),
                    r.message.clone(),
                    r.log.join("\n"),
                ),
                None => (
                    item.collision
                        .map(|c| t(lang, &format!("sequence.collision.{c}")))
                        .unwrap_or_default(),
                    String::new(),
                    String::new(),
                ),
            };
            (
                file_name(&item.input),
                file_name(&item.output),
                status,
                message,
                log,
            )
        })
        .collect();
    rsx! {
        div { class: "actions-batch",
            div { class: "field-grid",
                div { class: "field", label { "{t(lang, \"sequence.output\")}" }
                    div { class: "actions-output",
                        span { class: "ellipsis", title: "{folder}", "{folder}" }
                        button { class: "tbtn", disabled: b.running, onclick: move |_| actions::choose_output(st), "{t(lang, \"sequence.choose\")}" }
                    }
                }
                div { class: "field", label { "{t(lang, \"sequence.suffix\")}" }
                    input { class: "actions-suffix", value: "{suffix}", disabled: b.running,
                        oninput: move |e| { let v = e.value(); actions::set_rule(st, move |r| r.suffix = v); } }
                }
            }
            label { class: "check", input { r#type: "checkbox", checked: overwrite, disabled: b.running,
                    onchange: move |_| actions::set_rule(st, |r| r.overwrite = !r.overwrite) }
                span { "{t(lang, \"sequence.overwrite\")}" } }
            div { class: "audit-table actions-plan",
                for (i, (input, output, status, message, log)) in rows.into_iter().enumerate() {
                    div { key: "{i}", class: "audit-row", title: "{log}",
                        span { class: "ellipsis", "{input}" }
                        span { class: "ellipsis", "{output}" }
                        span { "{status}" }
                        span { class: "ellipsis muted", "{message}" }
                    }
                }
            }
            div { class: "actions-run",
                if b.running {
                    span { class: "hint actions-progress", "{done} / {total}" }
                    button { class: "tbtn", onclick: move |_| actions::cancel(), "{t(lang, \"sequence.cancel\")}" }
                } else {
                    button { class: "tbtn primary actions-start", disabled: total == 0, onclick: move |_| actions::run_batch(st), "{t(lang, \"sequence.start\")}" }
                }
                if finished || b.cancelled {
                    span { class: "actions-summary", "{summary}" }
                }
            }
        }
    }
}
