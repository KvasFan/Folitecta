//! Style editor of the properties pane: edits the selected comment or, while a
//! drawing tool is active, the default style of the comments that tool creates.

use dioxus::prelude::*;
use folitecta_core::{AnnotStyle, LINE_ENDINGS};

use crate::files;
use crate::i18n::{t, tf};
use crate::state::AppState;
use crate::styles::{builtin, default_style, kind_of, set_default_style, tool_subtype};
use crate::util::{color_hex, parse_color};

#[derive(Clone, Copy, PartialEq)]
enum Target {
    /// A comment: page, index and style kind.
    Comment(usize, usize, &'static str),
    /// Defaults of new comments of a kind.
    Defaults(&'static str),
    /// A comment style kept by name (its index) and its kind.
    Named(usize, &'static str),
}

pub(crate) fn static_kind(kind: &str) -> &'static str {
    KINDS.iter().find(|k| **k == kind).copied().unwrap_or("Square")
}

use crate::styles::{KINDS, named_styles, save_named_styles};

/// What the editor offers for a kind of comment.
struct Caps {
    color_label: &'static str,
    color_optional: bool,
    fill: bool,
    width: bool,
    dash: bool,
    ends: bool,
    start: bool,
    text: bool,
}

fn caps(kind: &str) -> Caps {
    let c = |color_label, color_optional, fill, width, dash, ends, start, text| Caps {
        color_label,
        color_optional,
        fill,
        width,
        dash,
        ends,
        start,
        text,
    };
    match kind {
        "Square" | "Circle" | "Polygon" | "Cloud" => {
            c("style.border_color", true, true, true, true, false, false, false)
        }
        "Line" | "PolyLine" | "Distance" | "Perimeter" => c(
            "style.color",
            false,
            true,
            true,
            true,
            true,
            true,
            kind == "Distance" || kind == "Perimeter",
        ),
        "Area" => c("style.color", false, true, true, true, false, false, true),
        "Ink" | "FreeHighlight" => c("style.color", false, false, true, false, false, false, false),
        "FreeText" | "Typewriter" => c("style.border_color", true, true, true, true, false, false, true),
        "Callout" => c("style.border_color", true, true, true, true, true, false, true),
        "Redact" => c("style.color", false, false, false, false, false, false, false),
        _ => c("style.color", false, false, false, false, false, false, false),
    }
}

/// Style kind of the selected comment (measures and callouts have their own panels).
fn comment_kind(st: AppState, page: usize, index: usize, subtype: &str, style: &AnnotStyle) -> String {
    let intent = st
        .read_active(|t| {
            let id = t
                .doc
                .annotations(page)
                .ok()?
                .into_iter()
                .find(|a| a.index == index)?
                .id?;
            let d = t.doc.inner().get_dictionary(id).ok()?;
            d.get(b"IT")
                .ok()
                .and_then(|n| n.as_name().ok())
                .map(|n| String::from_utf8_lossy(n).to_string())
        })
        .flatten()
        .unwrap_or_default();
    match intent.as_str() {
        "LineDimension" => "Distance".into(),
        "PolyLineDimension" => "Perimeter".into(),
        "PolygonDimension" => "Area".into(),
        "FreeTextCallout" => "Callout".into(),
        _ => kind_of(subtype, style),
    }
}

#[component]
pub fn StyleEditor() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let tool = (st.tool)();
    let selected = st
        .read_active(|t| {
            let (p, i) = t.selected_annot?;
            let a = t.doc.annotations(p).ok()?.into_iter().find(|a| a.index == i)?;
            let style = t.doc.annotation_style(p, i).ok()?;
            Some((p, i, a.subtype, style))
        })
        .flatten();
    // A style chosen in the comment styles pane, when no comment is selected.
    let editing = crate::commands::comment_styles::EDITING();
    let (target, style) = match (selected, editing.clone()) {
        (Some((p, i, subtype, style)), _)
            if folitecta_core::appearance::GENERATED_SUBTYPES.contains(&subtype.as_str()) =>
        {
            let kind = comment_kind(st, p, i, &subtype, &style);
            (Target::Comment(p, i, static_kind(&kind)), style)
        }
        (Some(_), _) => return rsx! {},
        (None, Some((kind, Some(i)))) => match named_styles(st).get(i) {
            Some(n) => (Target::Named(i, static_kind(&kind)), n.style()),
            None => return rsx! {},
        },
        (None, Some((kind, None))) => (Target::Defaults(static_kind(&kind)), default_style(st, &kind)),
        (None, None) => match tool_subtype(tool) {
            Some(kind) => (Target::Defaults(static_kind(kind)), default_style(st, kind)),
            None => return rsx! {},
        },
    };
    let kind = match target {
        Target::Comment(_, _, k) | Target::Defaults(k) | Target::Named(_, k) => k,
    };
    let caps = caps(kind);
    let apply = move |new: AnnotStyle| match target {
        Target::Comment(p, i, _) => {
            match st.with_active(|t| {
                let r = t.doc.set_annotation_style(p, i, &new);
                t.selected_annot = Some((p, i));
                r
            }) {
                Some(Err(e)) => st.error(e),
                _ => {
                    st.with_active(|t| t.selected_annot = Some((p, i)));
                    files::set_window_title(st);
                }
            }
        }
        Target::Defaults(kind) => set_default_style(st, kind, &new),
        Target::Named(i, _) => {
            let mut list = named_styles(st);
            if let Some(n) = list.get_mut(i) {
                *n = crate::styles::NamedStyle::new(n.name.clone(), &n.kind, &new);
                save_named_styles(st, &list);
            }
        }
    };
    let title = match target {
        Target::Comment(..) => t(lang, "style.title_comment"),
        Target::Defaults(k) if editing.is_some() => tf(
            lang,
            "style.title_named",
            &[("name", &crate::commands::comment_styles::default_name(lang, k))],
        ),
        Target::Defaults(_) => tf(lang, "style.title_tool", &[("tool", &t(lang, tool.label_key()))]),
        Target::Named(i, _) => tf(
            lang,
            "style.title_named",
            &[(
                "name",
                &named_styles(st)
                    .get(i)
                    .map(|n| n.name.clone())
                    .unwrap_or_default(),
            )],
        ),
    };
    let color = style.color;
    let fill = style.fill;
    let s = style.clone();
    let opacity_pct = (style.opacity * 100.0).round() as i32;
    let ends_label = |name: &str| t(lang, &format!("le.{name}"));
    rsx! {
        div { class: "style-editor",
            div { class: "prop-head", "{title}" }
            div { class: "style-row",
                label { "{t(lang, caps.color_label)}" }
                input { r#type: "color", value: "{color_hex(color.unwrap_or([0.0, 0.0, 0.0]))}",
                    onchange: { let s = s.clone(); move |e: Event<FormData>| { let mut n = s.clone(); n.color = Some(parse_color(&e.value())); apply(n); } },
                }
                if caps.color_optional {
                    label { class: "check mini",
                        input { r#type: "checkbox", checked: color.is_none(), onchange: { let s = s.clone(); move |_| { let mut n = s.clone(); n.color = if n.color.is_some() { None } else { Some([0.0, 0.0, 0.0]) }; apply(n); } } }
                        span { "{t(lang, \"style.none\")}" }
                    }
                }
            }
            if caps.fill {
                div { class: "style-row",
                    label { "{t(lang, \"style.fill\")}" }
                    input { r#type: "color", value: "{color_hex(fill.unwrap_or([1.0, 1.0, 1.0]))}",
                        onchange: { let s = s.clone(); move |e: Event<FormData>| { let mut n = s.clone(); n.fill = Some(parse_color(&e.value())); apply(n); } },
                    }
                    label { class: "check mini",
                        input { r#type: "checkbox", checked: fill.is_none(), onchange: { let s = s.clone(); move |_| { let mut n = s.clone(); n.fill = if n.fill.is_some() { None } else { Some([1.0, 1.0, 1.0]) }; apply(n); } } }
                        span { "{t(lang, \"style.none\")}" }
                    }
                }
            }
            div { class: "style-row",
                label { "{t(lang, \"style.opacity\")}" }
                input { r#type: "range", min: "5", max: "100", step: "5", value: "{opacity_pct}",
                    onchange: { let s = s.clone(); move |e: Event<FormData>| { if let Ok(v) = e.value().parse::<f32>() { let mut n = s.clone(); n.opacity = (v / 100.0).clamp(0.05, 1.0); apply(n); } } },
                }
                span { class: "muted value", "{opacity_pct}%" }
            }
            if caps.width {
                div { class: "style-row",
                    label { "{t(lang, \"style.width\")}" }
                    input { r#type: "number", min: "0", max: "40", step: "0.5", value: "{style.width}",
                        onchange: { let s = s.clone(); move |e: Event<FormData>| { if let Ok(v) = e.value().replace(',', ".").parse::<f32>() { let mut n = s.clone(); n.width = v.clamp(0.0, 40.0); apply(n); } } },
                    }
                    if caps.dash {
                        label { class: "check mini",
                            input { r#type: "checkbox", checked: style.dashed, onchange: { let s = s.clone(); move |_| { let mut n = s.clone(); n.dashed = !n.dashed; apply(n); } } }
                            span { "{t(lang, \"style.dashed\")}" }
                        }
                    }
                }
            }
            if caps.ends {
                if caps.start {
                    div { class: "style-row",
                        label { "{t(lang, \"style.start\")}" }
                        select { onchange: { let s = s.clone(); move |e: Event<FormData>| { let mut n = s.clone(); n.line_start = e.value(); apply(n); } },
                            for name in LINE_ENDINGS.iter() { option { key: "{name}", value: "{name}", selected: style.line_start == *name, "{ends_label(name)}" } }
                        }
                    }
                }
                div { class: "style-row",
                    label { if caps.start { "{t(lang, \"style.end\")}" } else { "{t(lang, \"style.pointer\")}" } }
                    select { onchange: { let s = s.clone(); move |e: Event<FormData>| { let mut n = s.clone(); n.line_end = e.value(); apply(n); } },
                        for name in LINE_ENDINGS.iter() { option { key: "{name}", value: "{name}", selected: style.line_end == *name, "{ends_label(name)}" } }
                    }
                }
            }
            if caps.text {
                div { class: "style-row",
                    label { "{t(lang, \"style.font_size\")}" }
                    input { r#type: "number", min: "4", max: "200", step: "1", value: "{style.font_size}",
                        onchange: { let s = s.clone(); move |e: Event<FormData>| { if let Ok(v) = e.value().replace(',', ".").parse::<f32>() { let mut n = s.clone(); n.font_size = v.clamp(4.0, 200.0); apply(n); } } },
                    }
                }
                if kind != "Distance" && kind != "Perimeter" && kind != "Area" {
                    div { class: "style-row",
                        label { "{t(lang, \"style.text_color\")}" }
                        input { r#type: "color", value: "{color_hex(style.text_color)}",
                            onchange: { let s = s.clone(); move |e: Event<FormData>| { let mut n = s.clone(); n.text_color = parse_color(&e.value()); apply(n); } },
                        }
                    }
                    div { class: "style-row",
                        label { "{t(lang, \"style.align\")}" }
                        div { class: "seg-buttons",
                            for (a, key) in [(0u8, "style.left"), (1, "style.center"), (2, "style.right")] {
                                button { class: if style.align == a { "tbtn on" } else { "tbtn" }, key: "{a}", title: "{t(lang, key)}",
                                    onclick: { let s = s.clone(); move |_| { let mut n = s.clone(); n.align = a; apply(n); } },
                                    "{t(lang, key)}"
                                }
                            }
                        }
                    }
                }
            }
            div { class: "row",
                match target {
                    Target::Comment(_, _, k) => {
                        let s1 = s.clone();
                        rsx! {
                            button { class: "tbtn", title: "{t(lang, \"style.make_default_hint\")}", onclick: move |_| { set_default_style(st, k, &s1); st.toast(crate::state::ToastKind::Ok, t(st.lang(), "style.default_saved"), ""); }, "{t(lang, \"style.make_default\")}" }
                            button { class: "tbtn", onclick: move |_| apply(default_style(st, k)), "{t(lang, \"style.apply_default\")}" }
                        }
                    }
                    Target::Defaults(k) => rsx! { button { class: "tbtn", onclick: move |_| set_default_style(st, k, &builtin(k)), "{t(lang, \"style.reset_default\")}" } },
                    Target::Named(_, k) => {
                        let s1 = s.clone();
                        rsx! {
                            button { class: "tbtn", title: "{t(lang, \"style.make_default_hint\")}", onclick: move |_| { set_default_style(st, k, &s1); st.toast(crate::state::ToastKind::Ok, t(st.lang(), "style.default_saved"), ""); }, "{t(lang, \"style.make_default\")}" }
                        }
                    }
                }
            }
        }
    }
}
