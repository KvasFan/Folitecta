//! The split pages editor.

use std::time::Duration;

use dioxus::prelude::*;
use folitecta_core::registry;

use crate::commands::split::{
    EVERY, SPLIT, UNITS, ZOOMS, apply, even_lines, every, fit_zoom, line_color, page_size, parts, units,
};
use crate::commands::{dispatch, option_on};
use crate::i18n::{Lang, cmd_label, t, tf};
use crate::icons::Icon;
use crate::state::AppState;
use crate::ui::dialogs::Frame;

fn label_of(lang: Lang, id: &str) -> String {
    registry()
        .get(id)
        .map(|c| cmd_label(lang, c))
        .unwrap_or_else(|| id.to_string())
}

/// Width of the band around a line that takes the pointer (CSS pixels).
const GRIP: f32 = 5.0;

/// Ticks of a ruler: position (CSS pixels) and label.
fn ticks(length_pt: f32, unit_pt: f32, z: f32) -> Vec<(f32, String)> {
    // A labelled tick about every 60 pixels, on round numbers of the unit.
    let per_px = 1.0 / (unit_pt * z);
    let raw = 60.0 * per_px;
    let step = [
        0.1f32, 0.2, 0.5, 1.0, 2.0, 5.0, 10.0, 20.0, 50.0, 100.0, 200.0, 500.0,
    ]
    .into_iter()
    .find(|s| *s >= raw)
    .unwrap_or(1000.0);
    let mut out = Vec::new();
    let mut v = 0.0f32;
    while v * unit_pt <= length_pt + 0.01 {
        let label = if step < 1.0 {
            format!("{v:.1}")
        } else {
            format!("{v:.0}")
        };
        out.push((v * unit_pt * z, label));
        v += step;
    }
    out
}

#[component]
pub fn SplitPagesDialog() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let s = SPLIT();
    // The size of the visible area, once it is shown.
    use_effect(move || {
        spawn(async move {
            tokio::time::sleep(Duration::from_millis(40)).await;
            let js = "const s = document.querySelector('.split-stage'); return s ? [s.clientWidth, s.clientHeight] : [0, 0];";
            if let Ok(r) = document::eval(js).await {
                if let Some(a) = r.as_array() {
                    let w = a.first().and_then(|x| x.as_f64()).unwrap_or(0.0) as f32;
                    let h = a.get(1).and_then(|x| x.as_f64()).unwrap_or(0.0) as f32;
                    SPLIT.write().stage = (w, h);
                }
            }
        });
    });
    let mut cols = use_signal(|| "2".to_string());
    let mut rows = use_signal(|| "1".to_string());
    let count = st.page_count();
    let page = s.page.min(count.saturating_sub(1));
    let (pw, ph) = page_size(st, page);
    let z = s.zoom.unwrap_or_else(|| fit_zoom((pw, ph), s.stage));
    let (cw, ch) = (pw * z, ph * z);
    let (tab, rev) = st.read_active(|t| (t.id, t.page_rev(page))).unwrap_or((0, 0));
    let scale = (((z * 1500.0) / 50.0).round() * 50.0).clamp(150.0, 3000.0) as u32;
    let (unit, unit_pt) = units(st);
    let rulers = option_on(st, "split.rulers");
    let color = line_color(st);
    let choice = every(st);
    let pieces = parts(&s.lines, (pw, ph));
    let dragging = s.dragging;
    // Move the dragged line to a point of the page (CSS pixels).
    let move_to = move |x: f32, y: f32| {
        let mut w = SPLIT.write();
        if let Some(i) = w.dragging {
            // A line moved by hand: the lines are no longer a grid.
            w.grid = None;
            w.confirm_clear = false;
            if let Some(l) = w.lines.get_mut(i) {
                l.pos = if l.horizontal {
                    (y / z).clamp(0.0, ph)
                } else {
                    (x / z).clamp(0.0, pw)
                };
            }
        }
    };
    let every_label = |e: &str| t(lang, &format!("split.every.{e}"));
    let page_text = tf(
        lang,
        "split.page_of",
        &[("page", &(page + 1).to_string()), ("count", &count.to_string())],
    );
    let parts_text = tf(lang, "split.parts", &[("count", &pieces.to_string())]);
    rsx! {
        Frame { title: label_of(lang, "splitPages"), wide: true, ok_label: Some(t(lang, "split.apply")),
            on_ok: move |_| apply(st),
            // Enter takes a typed value or presses a button; Split is a click.
            div { class: "split-editor",
                onkeydown: move |e| {
                    if e.key() == Key::Enter {
                        e.stop_propagation();
                    }
                },
            div { class: "pane-toolbar icon-toolbar split-toolbar",
                button { class: "tbtn", onclick: move |_| dispatch(st, "splitPages.addHorizontal"), Icon { name: "insert" } span { "{label_of(lang, \"splitPages.addHorizontal\")}" } }
                button { class: "tbtn", onclick: move |_| dispatch(st, "splitPages.addVertical"), Icon { name: "insert" } span { "{label_of(lang, \"splitPages.addVertical\")}" } }
                button { class: "ibtn", title: "{label_of(lang, \"splitPages.editLine\")}", disabled: s.lines.is_empty(), onclick: move |_| dispatch(st, "splitPages.editLine"), Icon { name: "pencil" } }
                button { class: "ibtn", title: "{label_of(lang, \"splitPages.removeLine\")}", disabled: s.selected.is_none(), onclick: move |_| dispatch(st, "splitPages.removeLine.preview"), Icon { name: "delete" } }
                button { class: "tbtn", disabled: s.lines.is_empty(), onclick: move |_| dispatch(st, "splitPages.clear"), "{label_of(lang, \"splitPages.clear\")}" }
                span { class: "gap" }
                select { title: "{label_of(lang, \"splitPages.units\")}", onchange: move |e| st.set_value("split.units", e.value()),
                    for (u, _) in UNITS.iter().copied() { option { key: "{u}", value: "{u}", selected: u == unit, "{u}" } }
                }
                select { title: "{label_of(lang, \"splitPages.every\")}", onchange: move |e| st.set_value("split.every", e.value()),
                    for e in EVERY.iter().copied() { option { key: "{e}", value: "{e}", selected: e == choice, "{every_label(e)}" } }
                }
                button { class: if rulers { "ibtn on" } else { "ibtn" }, title: "{label_of(lang, \"splitPages.showRulers\")}", onclick: move |_| dispatch(st, "splitPages.showRulers"), Icon { name: "margins" } }
                input { id: "split-color", class: "split-color", r#type: "color", value: "{color}", title: "{label_of(lang, \"splitPages.colorPicker\")}",
                    oninput: move |e| st.set_value("split.color", e.value()) }
                div { class: "spacer" }
                button { class: "ibtn", title: "{label_of(lang, \"splitPages.zoomOut\")}", onclick: move |_| dispatch(st, "splitPages.zoomOut"), Icon { name: "zoom-out" } }
                select { id: "split-zoom", class: "imged-zoom", title: "{label_of(lang, \"splitPages.zoomCombo\")}",
                    onchange: move |e| {
                        let v = e.value();
                        SPLIT.write().zoom = if v == "fit" { None } else { v.parse::<f32>().ok().map(|p| p / 100.0) };
                    },
                    option { value: "fit", selected: s.zoom.is_none(), "{t(lang, \"split.fit\")}" }
                    for level in ZOOMS.iter().copied() {
                        option { key: "{level}", value: "{level * 100.0}", selected: s.zoom == Some(level), "{(level * 100.0).round()}%" }
                    }
                }
                button { class: "ibtn", title: "{label_of(lang, \"splitPages.zoomIn\")}", onclick: move |_| dispatch(st, "splitPages.zoomIn"), Icon { name: "zoom-in" } }
            }
            if s.confirm_clear {
                div { class: "split-confirm",
                    span { "{t(lang, \"split.clear_confirm\")}" }
                    button { class: "tbtn danger", onclick: move |_| dispatch(st, "splitPages.clear"), "{t(lang, \"split.clear_yes\")}" }
                    button { class: "tbtn", onclick: move |_| SPLIT.write().confirm_clear = false, "{t(lang, \"split.clear_no\")}" }
                }
            }
            div { class: "split-body",
                div { class: "split-stage",
                    div { class: if dragging.is_some() { "split-canvas dragging" } else { "split-canvas" }, style: "width:{cw}px;height:{ch}px",
                        onmousemove: move |e| {
                            if SPLIT.peek().dragging.is_some() {
                                let p = e.element_coordinates();
                                move_to(p.x as f32, p.y as f32);
                            }
                        },
                        onmouseup: move |_| SPLIT.write().dragging = None,
                        onmouseleave: move |_| SPLIT.write().dragging = None,
                        img { class: "split-page", src: "/page/{tab}/{page}/{rev}/{scale}.png", alt: "", draggable: "false" }
                        if rulers {
                            div { class: "split-ruler top",
                                for (x, label) in ticks(pw, unit_pt, z) { span { key: "t{x}", style: "left:{x}px", "{label}" } }
                            }
                            div { class: "split-ruler left",
                                for (y, label) in ticks(ph, unit_pt, z) { span { key: "l{y}", style: "top:{y}px", "{label}" } }
                            }
                        }
                        for (i, l) in s.lines.iter().copied().enumerate() {
                            {
                                let (x0, y0) = if l.horizontal { (0.0, l.pos * z - GRIP) } else { (l.pos * z - GRIP, 0.0) };
                                let style = if l.horizontal {
                                    format!("left:0;top:{y0}px;width:{cw}px;height:{}px;--line:{color}", GRIP * 2.0)
                                } else {
                                    format!("left:{x0}px;top:0;width:{}px;height:{ch}px;--line:{color}", GRIP * 2.0)
                                };
                                let class = match (l.horizontal, s.selected == Some(i)) {
                                    (true, true) => "split-line h selected",
                                    (true, false) => "split-line h",
                                    (false, true) => "split-line v selected",
                                    (false, false) => "split-line v",
                                };
                                rsx! {
                                    div { key: "line{i}", class: "{class}", style: "{style}",
                                        onmousedown: move |e| {
                                            e.stop_propagation();
                                            let mut w = SPLIT.write();
                                            w.selected = Some(i);
                                            w.dragging = Some(i);
                                        },
                                        onmousemove: move |e| {
                                            e.stop_propagation();
                                            if SPLIT.peek().dragging.is_some() {
                                                let p = e.element_coordinates();
                                                move_to(x0 + p.x as f32, y0 + p.y as f32);
                                            }
                                        },
                                    }
                                }
                            }
                        }
                    }
                }
                div { class: "split-side",
                    div { class: "split-lines",
                        if s.lines.is_empty() { div { class: "pane-empty", "{t(lang, \"split.no_lines\")}" } }
                        for (i, l) in s.lines.iter().copied().enumerate() {
                            {
                                let kind = if l.horizontal { t(lang, "split.horizontal") } else { t(lang, "split.vertical") };
                                let value = format!("{:.2}", l.pos / unit_pt);
                                rsx! {
                                    div { key: "row{i}", class: if s.selected == Some(i) { "split-row selected" } else { "split-row" },
                                        onclick: move |_| SPLIT.write().selected = Some(i),
                                        span { class: "split-kind", "{kind}" }
                                        input { id: "split-line-{i}", r#type: "number", step: "0.1", min: "0", value: "{value}",
                                            onfocus: move |_| SPLIT.write().selected = Some(i),
                                            onchange: move |e| {
                                                let typed = e.value();
                                                let Ok(v) = typed.replace(',', ".").parse::<f32>() else { return };
                                                let max = if l.horizontal { ph } else { pw };
                                                let pos = (v * unit_pt).clamp(0.0, max);
                                                crate::commands::split::edited_by_hand();
                                                if let Some(line) = SPLIT.write().lines.get_mut(i) {
                                                    line.pos = pos;
                                                }
                                                // A value out of the page shows where the line really is.
                                                if (pos - v * unit_pt).abs() > 1e-3 {
                                                    let shown = format!("{:.2}", pos / unit_pt);
                                                    spawn(async move {
                                                        let _ = document::eval(&format!("const i = document.getElementById('split-line-{i}'); if (i) i.value = '{shown}';")).await;
                                                    });
                                                }
                                            }
                                        }
                                        span { class: "muted", "{unit}" }
                                        button { class: "ibtn", title: "{label_of(lang, \"splitPages.removeLine\")}",
                                            onclick: move |e| {
                                                e.stop_propagation();
                                                SPLIT.write().selected = Some(i);
                                                dispatch(st, "splitPages.removeLine.list");
                                            },
                                            "×"
                                        }
                                    }
                                }
                            }
                        }
                    }
                    div { class: "split-grid",
                        label { "{t(lang, \"dialog.tiles.cols\")}" input { r#type: "number", min: "1", max: "20", value: "{cols}", oninput: move |e| cols.set(e.value()) } }
                        label { "{t(lang, \"dialog.tiles.rows\")}" input { r#type: "number", min: "1", max: "20", value: "{rows}", oninput: move |e| rows.set(e.value()) } }
                        button { class: "tbtn",
                            onclick: move |_| {
                                let c = cols().trim().parse::<usize>().unwrap_or(1).clamp(1, 20);
                                let r = rows().trim().parse::<usize>().unwrap_or(1).clamp(1, 20);
                                let mut w = SPLIT.write();
                                w.lines = even_lines((pw, ph), c, r);
                                w.selected = None;
                                w.confirm_clear = false;
                                w.grid = Some((c, r));
                            },
                            "{t(lang, \"split.grid\")}"
                        }
                    }
                    div { class: "split-nav",
                        button { class: "ibtn", title: "{t(lang, \"split.prev_page\")}", disabled: page == 0, onclick: move |_| SPLIT.write().page = page.saturating_sub(1), Icon { name: "prev" } }
                        span { "{page_text}" }
                        button { class: "ibtn", title: "{t(lang, \"split.next_page\")}", disabled: page + 1 >= count, onclick: move |_| SPLIT.write().page = (page + 1).min(count.saturating_sub(1)), Icon { name: "next" } }
                    }
                }
            }
            p { class: "hint", "{parts_text}" }
            }
        }
    }
}
