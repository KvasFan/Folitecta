//! The tick mark tool: the mark it places (a check, a cross, a dot, or the
//! next text of a sequence, plain or in a circle), its size and colour, and
//! the place of the next mark in the sequence.

use dioxus::prelude::*;
use folitecta_core::{Sequence, TickShape, registry};

use crate::commands::ticks::{self, TickSettings};
use crate::i18n::{cmd_label, t};
use crate::state::{AppState, Dialog, Tool};
use crate::ui::dialogs::Frame;

/// How a shape looks in its button.
fn sample(shape: TickShape) -> &'static str {
    match shape {
        TickShape::Check => "\u{2713}",
        TickShape::Cross => "\u{2717}",
        TickShape::Dot => "\u{2022}",
        TickShape::Text => "1",
        TickShape::Circled => "\u{2460}",
    }
}

#[component]
pub fn TickMarksDialog() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let title = registry()
        .get("tool.annot.tickMark")
        .map(|c| cmd_label(lang, c).trim_end_matches('…').to_string())
        .unwrap_or_default();
    let start = use_hook(|| ticks::settings(st));
    let mut shape = use_signal(|| start.shape);
    let mut sequence = use_signal(|| start.sequence);
    let mut next = use_signal(|| start.next.to_string());
    let mut size = use_signal(|| format!("{}", start.size));
    let mut color = use_signal(|| crate::util::color_hex(start.color));
    let settings = move || TickSettings {
        shape: shape(),
        sequence: sequence(),
        next: next().trim().parse().unwrap_or(1).max(1),
        size: size()
            .trim()
            .replace(',', ".")
            .parse()
            .unwrap_or(18.0f32)
            .clamp(4.0, 200.0),
        color: crate::util::parse_color(&color()),
    };
    let shapes: Vec<(TickShape, String)> = TickShape::ALL
        .into_iter()
        .map(|s| (s, t(lang, &format!("dialog.tick.{}", s.id()))))
        .collect();
    let sequences: Vec<(Sequence, String)> = Sequence::ALL
        .into_iter()
        .map(|q| (q, t(lang, &format!("dialog.tick.seq_{}", q.id()))))
        .collect();
    let preview = ticks::next_mark(&settings());
    let shown = if preview.shape.has_text() {
        preview.text.clone()
    } else {
        sample(preview.shape).to_string()
    };
    rsx! {
        Frame { title, ok_label: Some(t(lang, "dialog.tick.use")),
            on_ok: move |_| {
                ticks::save(st, &settings());
                st.set_dialog(Dialog::None);
                st.set_tool(Tool::Tick);
            },
            p { class: "hint", "{t(lang, \"dialog.tick.hint\")}" }
            div { class: "field", label { "{t(lang, \"dialog.tick.shape\")}" }
                div { class: "tick-shapes",
                    for (s, name) in shapes {
                        button { key: "{s.id()}", class: if shape() == s { "tbtn toggle on tick-shape" } else { "tbtn toggle tick-shape" },
                            title: "{name}",
                            onclick: move |_| shape.set(s),
                            span { class: "tick-sample", "{sample(s)}" }
                            span { "{name}" }
                        }
                    }
                }
            }
            if shape().has_text() {
                div { class: "field-grid",
                    div { class: "field", label { "{t(lang, \"dialog.tick.sequence\")}" }
                        select { class: "tick-sequence", onchange: move |e| sequence.set(Sequence::from_id(&e.value())),
                            for (q, name) in sequences {
                                option { key: "{q.id()}", value: "{q.id()}", selected: sequence() == q, "{name}" }
                            }
                        }
                    }
                    div { class: "field", label { "{t(lang, \"dialog.tick.next\")}" }
                        input { class: "tick-next", r#type: "number", min: "1", value: "{next}", oninput: move |e| next.set(e.value()) }
                    }
                }
            }
            div { class: "field-grid",
                div { class: "field", label { "{t(lang, \"dialog.tick.size\")}" }
                    input { r#type: "number", min: "4", max: "200", value: "{size}", oninput: move |e| size.set(e.value()) }
                }
                div { class: "field", label { "{t(lang, \"dialog.watermark.color\")}" }
                    input { r#type: "color", value: "{color}", oninput: move |e| color.set(e.value()) }
                }
            }
            div { class: "tick-preview",
                span { class: "muted", "{t(lang, \"dialog.tick.next_mark\")}" }
                span { class: if preview.shape == TickShape::Circled { "tick-mark circled" } else { "tick-mark" }, style: "color:{color}", "{shown}" }
            }
        }
    }
}
