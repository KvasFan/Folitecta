//! The Quick Access Toolbar, next to the ribbon tabs or in a row below the
//! ribbon, and the dialog that chooses its commands.

use dioxus::prelude::*;
use folitecta_core::{Command, registry};

use crate::commands::qat::{self, SELECTED};
use crate::commands::{self, is_implemented};
use crate::i18n::{cmd_label, t};
use crate::icons::{self, Icon};
use crate::state::AppState;
use crate::ui::chrome::ribbon::is_active;
use crate::ui::dialogs::Frame;

/// The buttons of the toolbar; a right click takes a command away.
#[component]
pub fn QuickAccess(row: bool) -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let list = qat::items(st);
    let reg = registry();
    let customize = reg
        .get("customizeToolbars")
        .map(|c| cmd_label(lang, c))
        .unwrap_or_default();
    rsx! {
        div { class: if row { "qat qat-row" } else { "qat" },
            // The entries scroll when the window is too narrow for them; the
            // button that customizes them stays in view.
            div { class: "qat-items",
                for (i, item) in list.into_iter().enumerate() {
                    if item.is_separator() {
                        div { key: "s{i}", class: "qat-sep" }
                    } else if let Some(c) = reg.get(&item.id) {
                        {
                            let label = cmd_label(lang, c);
                            let tip = match c.shortcut.as_deref() {
                                Some(s) => format!("{label} ({s})"),
                                None => label.clone(),
                            };
                            let mut class = String::from(if item.text { "tbtn qat-btn" } else { "ibtn qat-btn" });
                            if is_active(st, &item.id) {
                                class.push_str(" active");
                            }
                            let (run, drop) = (item.id.clone(), item.id.clone());
                            rsx! {
                                button { key: "{i}-{item.id}", class: "{class}", title: "{tip}", "aria-label": "{label}", "data-cmd": "{item.id}",
                                    onclick: move |e| { e.stop_propagation(); commands::dispatch(st, &run); },
                                    oncontextmenu: move |e| { e.prevent_default(); e.stop_propagation(); qat::toggle(st, &drop); },
                                    if item.icon { Icon { name: icons::for_command(&item.id) } }
                                    if item.text { span { class: "txt", "{label}" } }
                                }
                            }
                        }
                    }
                }
            }
            button { class: "ibtn qat-customize", title: "{customize}", "aria-label": "{customize}",
                onclick: move |e| { e.stop_propagation(); commands::dispatch(st, "customizeToolbars"); },
                Icon { name: "more" }
            }
        }
    }
}

/// The commands of the current ribbon tab, for the list before anything is searched.
fn tab_commands(st: AppState) -> Vec<&'static Command> {
    let tab = st.ribbon_tab.peek().clone();
    let reg = registry();
    let mut out: Vec<&'static Command> = Vec::new();
    if let Some(t) = crate::ui::chrome::ribbon_layout::tab(&tab) {
        for g in t.groups {
            for id in g.items {
                if let Some(c) = reg.get(id) {
                    if is_implemented(&c.id) && !out.iter().any(|o| o.id == c.id) {
                        out.push(c);
                    }
                }
            }
        }
    }
    out
}

#[component]
pub fn CustomizeQatDialog() -> Element {
    let st = use_context::<AppState>();
    let lang = st.lang();
    let reg = registry();
    let label = |id: &str| reg.get(id).map(|c| cmd_label(lang, c)).unwrap_or_default();
    let mut query = use_signal(String::new);
    let bars = qat::user_bars(st);
    let target = (*qat::TARGET.read()).filter(|i| *i < bars.len());
    let own = target.and_then(|i| bars.get(i).cloned());
    let list = qat::target_items(st);
    let locked = qat::bars_locked(st);
    let list_title = own
        .as_ref()
        .map(|b| b.name.clone())
        .unwrap_or_else(|| t(lang, "qat.toolbar"));
    let bar_names: Vec<String> = bars
        .iter()
        .map(|b| {
            format!(
                "{} ({})",
                b.name,
                t(lang, if b.tab { "qat.kind_tab" } else { "qat.kind_toolbar" })
            )
        })
        .collect();
    let selected = (*SELECTED.read()).filter(|i| *i < list.len());
    let chosen = selected.and_then(|i| list.get(i).cloned());
    let found: Vec<&'static Command> = if query().trim().is_empty() {
        tab_commands(st)
    } else {
        reg.search(&query(), 80)
            .into_iter()
            .filter(|c| is_implemented(&c.id))
            .collect()
    };
    let below = qat::below(st);
    rsx! {
        Frame { title: label("customizeToolbars"), wide: true,
            p { class: "hint", "{t(lang, \"qat.hint\")}" }
            if locked { p { class: "hint warn", "{t(lang, \"toast.bars_locked\")}" } }
            div { class: "row qat-targets",
                select { class: "qat-target",
                    onchange: move |e| { *qat::TARGET.write() = e.value().parse::<usize>().ok(); *SELECTED.write() = None; },
                    option { value: "qat", selected: target.is_none(), "{t(lang, \"qat.toolbar\")}" }
                    for (i, name) in bar_names.into_iter().enumerate() {
                        option { key: "{i}", value: "{i}", selected: target == Some(i), "{name}" }
                    }
                }
                button { class: "tbtn qat-new-toolbar", disabled: locked, onclick: move |_| commands::dispatch(st, "customizeToolbars.newToolbar"), "{label(\"customizeToolbars.newToolbar\")}" }
                button { class: "tbtn qat-new-tab", disabled: locked, onclick: move |_| commands::dispatch(st, "customizeToolbars.newTab"), "{label(\"customizeToolbars.newTab\")}" }
            }
            if let Some(b) = own.clone() {
                div { class: "row qat-own",
                    input { class: "qat-bar-name", value: "{b.name}", disabled: locked, onchange: move |e| qat::set_name(st, &e.value()) }
                    if !b.tab {
                        label { class: "check",
                            input { r#type: "checkbox", class: "qat-bar-shown", checked: b.shown, disabled: locked,
                                onchange: move |_| {
                                    let mut all = qat::user_bars(st);
                                    if let Some(x) = target.and_then(|i| all.get_mut(i)) { x.shown = !x.shown; }
                                    qat::save_user_bars(st, &all);
                                } }
                            span { "{t(lang, \"qat.shown\")}" }
                        }
                    }
                    button { class: "tbtn", disabled: locked, onclick: move |_| commands::dispatch(st, "customizeToolbars.makeDefault"), "{label(\"customizeToolbars.makeDefault\")}" }
                    button { class: "tbtn danger qat-delete-bar", disabled: locked, onclick: move |_| qat::delete_bar(st), "{t(lang, \"qat.delete_bar\")}" }
                }
            }
            div { class: "qat-columns",
                div { class: "qat-column",
                    b { "{t(lang, \"qat.commands\")}" }
                    input { class: "qat-search", value: "{query}", placeholder: "{t(lang, \"qat.search\")}", autofocus: true,
                        oninput: move |e| query.set(e.value()) }
                    div { class: "qat-list",
                        for c in found {
                            {
                                let id = c.id.clone();
                                let there = list.iter().any(|i| i.id == c.id);
                                rsx! {
                                    div { key: "{c.id}", class: "qat-entry",
                                        Icon { name: icons::for_command(&c.id) }
                                        span { class: "grow", title: "{reg.path_label(&c.id)}", "{cmd_label(lang, c)}" }
                                        button { class: "tbtn qat-add", disabled: there, onclick: move |_| qat::add(st, &id), "{t(lang, \"qat.add\")}" }
                                    }
                                }
                            }
                        }
                    }
                }
                div { class: "qat-column",
                    b { "{list_title}" }
                    div { class: "qat-list qat-current",
                        for (i, item) in list.iter().enumerate() {
                            button { key: "{i}-{item.id}", class: if selected == Some(i) { "qat-entry on" } else { "qat-entry" },
                                onclick: move |_| *SELECTED.write() = Some(i),
                                if item.is_separator() {
                                    span { class: "muted", "— {t(lang, \"qat.separator\")} —" }
                                } else {
                                    Icon { name: icons::for_command(&item.id) }
                                    span { class: "grow", "{label(&item.id)}" }
                                }
                            }
                        }
                    }
                    div { class: "row",
                        button { class: "tbtn qat-up", disabled: selected.is_none(), onclick: move |_| qat::move_by(st, -1), "{label(\"customizeToolbars.up\")}" }
                        button { class: "tbtn qat-down", disabled: selected.is_none(), onclick: move |_| qat::move_by(st, 1), "{label(\"customizeToolbars.down\")}" }
                        button { class: "tbtn danger qat-remove", disabled: selected.is_none(), onclick: move |_| qat::remove(st), "{label(\"customizeToolbars.delete\")}" }
                        button { class: "tbtn qat-sep-btn", disabled: selected.is_none(), onclick: move |_| qat::separator(st, true), "{label(\"item.insertSepAfter\")}" }
                    }
                    if let Some(item) = chosen.filter(|i| !i.is_separator()) {
                        div { class: "check-row",
                            label { class: "check",
                                input { r#type: "checkbox", class: "qat-show-text", checked: item.text, onchange: move |_| qat::toggle_text(st, true) }
                                span { "{label(\"item.showText\")}" }
                            }
                            label { class: "check",
                                input { r#type: "checkbox", checked: item.icon, onchange: move |_| qat::toggle_text(st, false) }
                                span { "{label(\"item.showIcon\")}" }
                            }
                        }
                    }
                    div { class: "radio-row",
                        label { input { r#type: "radio", name: "qat-place", checked: !below, onchange: move |_| commands::dispatch(st, "QABarAbove") }
                            span { "{label(\"QABarAbove\")}" } }
                        label { input { r#type: "radio", name: "qat-place", class: "qat-below", checked: below, onchange: move |_| commands::dispatch(st, "QABarBelow") }
                            span { "{label(\"QABarBelow\")}" } }
                    }
                    div { class: "row",
                        button { class: "tbtn qat-reset", onclick: move |_| qat::reset(st), "{label(\"customizeToolbars.reset\")}" }
                    }
                }
            }
        }
    }
}
